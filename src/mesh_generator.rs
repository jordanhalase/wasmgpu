use log::info;
use wgpu::{self, include_wgsl};

pub struct MeshGenerator {
    // TODO
}

impl MeshGenerator {
    // TODO: Is async needed here?
    pub async fn new(device: &wgpu::Device, queue: &wgpu::Queue) {
        // Technically optional
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

        // Technically optional
        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&compute_bind_group_layout],
                push_constant_ranges: &[],
            });

        // TODO: Put this into a struct, use uniform buffer for ranges
        let grid_resolution = 32;
        let n_vertices = grid_resolution * grid_resolution;

        // TODO: Put this into a vertex attribute struct
        // All floats: x, y, z, r, g, b (TODO: x, y, z, nx, ny, nz) (leave final z zeroed)
        let n_vertex_bytes = n_vertices * 4 * 6;

        let n_indices = (grid_resolution - 1) * (grid_resolution - 1) * 6;
        let n_index_bytes = n_indices * 4;

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: n_vertex_bytes,
            usage: wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: n_index_bytes,
            usage: wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::INDEX,
            mapped_at_creation: false,
        });

        let n_result_bytes = core::cmp::max(n_vertex_bytes, n_index_bytes);

        let result_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: n_result_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
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

        let gen_vertex_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &compute_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: vertex_buffer.as_entire_binding(),
            }],
        });

        let gen_index_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &compute_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: index_buffer.as_entire_binding(),
            }],
        });

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&gen_vertex_pipeline);
            pass.set_bind_group(0, &gen_vertex_bind_group, &[]);
            pass.dispatch_workgroups(2, 2, 1); // 16x16 workgroup size

            // TODO: Generate index buffer in same compute pass (optimal)
            pass.set_pipeline(&gen_index_pipeline);
            pass.set_bind_group(0, &gen_index_bind_group, &[]);
            pass.dispatch_workgroups(2, 2, 1); // 16x16 workgroup size
        }
        encoder.copy_buffer_to_buffer(&vertex_buffer, 0, &result_buffer, 0, n_vertex_bytes);
        let command_buffer = encoder.finish();
        queue.submit([command_buffer]);

        let (tx, rx) = futures::channel::oneshot::channel();
        result_buffer.map_async(wgpu::MapMode::Read, 0..n_result_bytes, move |res| {
            let _ = tx.send(res);
        });
        rx.await
            .expect("Could not get channel data")
            .expect("Could not map buffer");
        {
            let mapped = result_buffer.get_mapped_range(0..n_vertex_bytes);
            let uints: &[f32] = bytemuck::cast_slice(&mapped);
            //info!("{:?}", &uints[..]);
            for (i, vtx) in uints.chunks(6).enumerate() {
                info!("{i}: {:.2?}", vtx);
            }
        }
        result_buffer.unmap();
        info!("Unmapped!");

        // Does not work on WebGPU!
        //device.poll(wgpu::PollType::wait_indefinitely()).expect("Oops");

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        encoder.copy_buffer_to_buffer(&index_buffer, 0, &result_buffer, 0, n_index_bytes);
        let command_buffer = encoder.finish();
        queue.submit([command_buffer]);

        info!(
            "Mapping again {} bytes to {} byte indices",
            n_result_bytes, n_index_bytes
        );
        let (tx, rx) = futures::channel::oneshot::channel();
        result_buffer.map_async(wgpu::MapMode::Read, 0..n_result_bytes, move |res| {
            let _ = tx.send(res);
        });
        rx.await
            .expect("Could not get channel data")
            .expect("Could not map buffer");
        {
            let mapped = result_buffer.get_mapped_range(0..n_index_bytes);
            let uints: &[u32] = bytemuck::cast_slice(&mapped);
            for idx in uints.chunks(6) {
                info!("{:?}", idx);
            }
            //info!("{:?}", &uints[..]);
        }
        result_buffer.unmap();
        info!("Unmapped!");
    }
}
