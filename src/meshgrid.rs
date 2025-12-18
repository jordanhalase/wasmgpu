#[cfg(feature = "readback")]
use log::info;

use bytemuck::bytes_of;
use core::ops::RangeInclusive;

use wgpu::{self, include_wgsl};

pub struct Generator {
    device: wgpu::Device,
    queue: wgpu::Queue,
    compute_bind_group_layout: wgpu::BindGroupLayout,
    gen_vertex_pipeline: wgpu::ComputePipeline,
    gen_index_pipeline: wgpu::ComputePipeline,
    uniform_buffer: wgpu::Buffer,
    evaluator_pipeline_layout: wgpu::PipelineLayout,
    evaluator_bind_group_layout: wgpu::BindGroupLayout,
}

pub struct GridBuffers {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub index_format: wgpu::IndexFormat,
    evaluator_dispatch_count: u32,
    evaluator_bind_group: wgpu::BindGroup,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct GeneratorUniform {
    resolution: [u32; 2],
    x_range: [f32; 2],
    y_range: [f32; 2],
}

impl Generator {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let compute_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&compute_bind_group_layout],
                push_constant_ranges: &[],
            });

        let gen_vertex_module = device.create_shader_module(include_wgsl!("gen_vertex.wgsl"));
        let gen_index_module = device.create_shader_module(include_wgsl!("gen_index.wgsl"));

        let gen_vertex_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None,
                layout: Some(&compute_pipeline_layout),
                module: &gen_vertex_module,
                entry_point: Some("generate_vertex_buffer"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });

        let gen_index_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: Some(&compute_pipeline_layout),
            module: &gen_index_module,
            entry_point: Some("generate_index_buffer"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: core::mem::size_of::<GeneratorUniform>() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });

        let evaluator_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let evaluator_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&evaluator_bind_group_layout],
                push_constant_ranges: &[],
            });

        Self {
            device: device.clone(),
            queue: queue.clone(),
            compute_bind_group_layout,
            gen_vertex_pipeline,
            gen_index_pipeline,
            uniform_buffer,
            evaluator_pipeline_layout,
            evaluator_bind_group_layout,
        }
    }

    pub fn generate_buffers(
        &self,
        grid_resolution: (u32, u32),
        x_range: RangeInclusive<f32>,
        y_range: RangeInclusive<f32>,
    ) -> GridBuffers {
        let grid_leftover = (grid_resolution.0 & 0xf, grid_resolution.1 & 0xf);
        let grid_chunks = {
            let width = if grid_leftover.0 > 0 {
                (grid_resolution.0 >> 4) + 1
            } else {
                grid_resolution.0 >> 4
            };
            let height = if grid_leftover.1 > 0 {
                (grid_resolution.1 >> 4) + 1
            } else {
                grid_resolution.1 >> 4
            };
            (width, height)
        };

        let vertex_count = grid_resolution.0 * grid_resolution.1;
        let vertex_byte_count = vertex_count * 4 * 6;

        let index_count = (grid_resolution.0 - 1) * (grid_resolution.1 - 1) * 6;
        let index_byte_count = index_count * 4;

        let uniform_data = GeneratorUniform {
            resolution: [grid_resolution.0, grid_resolution.1],
            x_range: [*x_range.start(), *x_range.end()],
            y_range: [*y_range.start(), *y_range.end()],
        };

        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytes_of(&uniform_data));

        let vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: vertex_byte_count as u64,
            usage: wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        let index_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: index_byte_count as u64,
            usage: wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::INDEX,
            mapped_at_creation: false,
        });

        let gen_vertex_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.compute_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: vertex_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let gen_index_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.compute_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: index_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });

            // Generate vertex buffer
            pass.set_pipeline(&self.gen_vertex_pipeline);
            pass.set_bind_group(0, &gen_vertex_bind_group, &[]);
            pass.dispatch_workgroups(grid_chunks.0, grid_chunks.1, 1);

            // Generate index buffer in same compute pass (optimal)
            pass.set_pipeline(&self.gen_index_pipeline);
            pass.set_bind_group(0, &gen_index_bind_group, &[]);
            pass.dispatch_workgroups(grid_chunks.0, grid_chunks.1, 1);
        }
        self.queue.submit([encoder.finish()]);

        let evaluator_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.evaluator_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: vertex_buffer.as_entire_binding(),
            }],
        });

        let evaluator_dispatch_count = if vertex_count & 0xff > 0 {
            (vertex_count >> 8) + 1
        } else {
            vertex_count >> 8
        };

        GridBuffers {
            evaluator_bind_group,
            vertex_buffer,
            index_buffer,
            index_count,
            evaluator_dispatch_count,
            index_format: wgpu::IndexFormat::Uint32,
        }
    }

    pub fn create_evaluator(
        &self,
        module: &wgpu::ShaderModule,
        entry_point: Option<&str>,
    ) -> Evaluator {
        let evaluator_pipeline =
            self.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: None,
                    layout: Some(&self.evaluator_pipeline_layout),
                    module,
                    entry_point,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    cache: None,
                });
        Evaluator {
            device: self.device.clone(),
            queue: self.queue.clone(),
            evaluator_pipeline,
        }
    }

    #[cfg(feature = "readback")]
    pub async fn print_vertices(&self, buffers: &GridBuffers) {
        let n_staging_bytes = buffers.vertex_buffer.size();

        let staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: n_staging_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_buffer_to_buffer(
            &buffers.vertex_buffer,
            0,
            &staging_buffer,
            0,
            n_staging_bytes,
        );
        self.queue.submit([encoder.finish()]);

        info!("Mapping vertex buffer");

        let (tx, rx) = futures::channel::oneshot::channel();
        staging_buffer.map_async(wgpu::MapMode::Read, 0..n_staging_bytes, move |res| {
            let _ = tx.send(res);
        });
        rx.await
            .expect("Could not get channel data")
            .expect("Could not map buffer");
        {
            let mapped = staging_buffer.get_mapped_range(0..n_staging_bytes);
            let uints: &[f32] = bytemuck::cast_slice(&mapped);
            for (i, vtx) in uints.chunks(6).enumerate() {
                info!("{i}: {:.2?}", vtx);
            }
        }
        staging_buffer.unmap();
    }

    #[cfg(feature = "readback")]
    pub async fn print_indices(&self, buffers: &GridBuffers) {
        let n_staging_bytes = buffers.index_buffer.size();

        let staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: n_staging_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_buffer_to_buffer(
            &buffers.index_buffer,
            0,
            &staging_buffer,
            0,
            n_staging_bytes,
        );
        self.queue.submit([encoder.finish()]);

        info!("Mapping index buffer");

        let (tx, rx) = futures::channel::oneshot::channel();
        staging_buffer.map_async(wgpu::MapMode::Read, 0..n_staging_bytes, move |res| {
            let _ = tx.send(res);
        });
        rx.await
            .expect("Could not get channel data")
            .expect("Could not map buffer");
        {
            let mapped = staging_buffer.get_mapped_range(0..n_staging_bytes);
            let uints: &[u32] = bytemuck::cast_slice(&mapped);
            for (i, idx) in uints.chunks(6).enumerate() {
                info!("{i}: {:.2?}", idx);
            }
        }
        staging_buffer.unmap();
    }
}

pub struct Evaluator {
    device: wgpu::Device,
    queue: wgpu::Queue,
    evaluator_pipeline: wgpu::ComputePipeline,
}

impl Evaluator {
    pub fn evaluate_buffers(&self, grid_buffers: &[&GridBuffers]) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });

            // Evaluate vertex buffers
            for &grid_buffer in grid_buffers {
                pass.set_pipeline(&self.evaluator_pipeline);
                pass.set_bind_group(0, &grid_buffer.evaluator_bind_group, &[]);
                pass.dispatch_workgroups(grid_buffer.evaluator_dispatch_count, 1, 1);
            }
        }
        self.queue.submit([encoder.finish()]);
    }
}
