#[cfg(feature = "readback")]
use log::info;

use wgpu::{self, include_wgsl};

pub struct Generator {
    device: wgpu::Device,
    queue: wgpu::Queue,
    compute_bind_group_layout: wgpu::BindGroupLayout,
    compute_pipeline_layout: wgpu::PipelineLayout,
    gen_vertex_pipeline: wgpu::ComputePipeline,
    gen_index_pipeline: wgpu::ComputePipeline,
}

pub struct GridBuffers {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
}

impl Generator {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let compute_bind_group_layout =
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

        Self {
            device: device.clone(),
            queue: queue.clone(),
            compute_bind_group_layout,
            compute_pipeline_layout,
            gen_vertex_pipeline,
            gen_index_pipeline,
        }
    }

    // TODO: Remove hardcoded values, but grid_chunks will always be 16x16
    pub fn generate_buffers(&self, grid_chunks: u64) -> GridBuffers {
        let grid_resolution = grid_chunks << 4;
        let n_vertices = grid_resolution * grid_resolution;

        // TODO: Put this into a vertex attribute struct
        // All floats: x, y, z, r, g, b (TODO: x, y, z, nx, ny, nz) (leave final z zeroed)
        let n_vertex_bytes = n_vertices * 4 * 6;

        let n_indices = (grid_resolution - 1) * (grid_resolution - 1) * 6;
        let n_index_bytes = n_indices * 4;

        let vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: n_vertex_bytes,
            usage: wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        let index_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: n_index_bytes,
            usage: wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::INDEX,
            mapped_at_creation: false,
        });

        let gen_vertex_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.compute_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: vertex_buffer.as_entire_binding(),
            }],
        });

        let gen_index_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.compute_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: index_buffer.as_entire_binding(),
            }],
        });

        let mut encoder =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });

            // TODO: Do not hardcode dispatch numbers

            // Generate vertex buffer
            pass.set_pipeline(&self.gen_vertex_pipeline);
            pass.set_bind_group(0, &gen_vertex_bind_group, &[]);
            pass.dispatch_workgroups(2, 2, 1); // 16x16 workgroup size

            // Generate index buffer in same compute pass (optimal)
            pass.set_pipeline(&self.gen_index_pipeline);
            pass.set_bind_group(0, &gen_index_bind_group, &[]);
            pass.dispatch_workgroups(2, 2, 1); // 16x16 workgroup size
        }
        self.queue.submit([encoder.finish()]);

        GridBuffers {
            vertex_buffer,
            index_buffer,
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

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_buffer_to_buffer(&buffers.vertex_buffer, 0, &staging_buffer, 0, n_staging_bytes);
        self.queue.submit([encoder.finish()]);

        info!("Mapping vertex bufffer");

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

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_buffer_to_buffer(&buffers.index_buffer, 0, &staging_buffer, 0, n_staging_bytes);
        self.queue.submit([encoder.finish()]);

        info!("Mapping index bufffer");

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
