#![no_std]

use wasm_bindgen::prelude::*;
use wgpu::{self, include_wgsl};
use log::info;

#[wasm_bindgen]
pub async fn start_webgpu_app(canvas_id: &str) {
    // Get the canvas element from the DOM
    let window = web_sys::window().expect("No window found");
    let document = window.document().expect("No document found");
    let canvas = document.get_element_by_id(&canvas_id)
        .expect("No element found")
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .expect("Element is not HTMLCanvasElement");

    console_log::init().expect("Could not initiate logging");

    let width = canvas.width();
    let height = canvas.height();

    // Create a wgpu instance
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

    let surface = instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas))
        .expect("Could not create surface");

    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::None,
        force_fallback_adapter: false,
        compatible_surface: Some(&surface),
    })
    .await
    .expect("Could not get appropriate adapter");

    let (device, queue) = adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::default(),
            required_limits: wgpu::Limits::defaults(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        },
    )
    .await
    .expect("Could not create device");

    // Configure the surface
    let config = surface.get_default_config(&adapter, width, height)
        .expect("Surface not supported by adapter");
    surface.configure(&device, &config);

    let output = surface.get_current_texture().expect("Could not get current texture");

    // Render image (simplified)
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
    {
        let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }

    let command_buffer = encoder.finish();
    queue.submit([command_buffer]);
    output.present();

    info!("{}, {}", width, height);

    // Create a compute pipeline

    // Technically optional
    let compute_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            },
        ],
    });

    // Technically optional
    let compute_pipeline_layout= device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&compute_bind_group_layout],
        push_constant_ranges: &[],
    });

    // TODO: Put this into a struct, use uniform buffer for ranges
    let grid_resolution = 32;
    let n_vertices = grid_resolution*grid_resolution;

    // TODO: Put this into a vertex attribute struct
    // All floats: x, y, z, r, g, b (TODO: x, y, z, nx, ny, nz) (leave final z zeroed)
    let n_vertex_bytes = n_vertices * 4 * 6;

    let n_indices = (grid_resolution-1)*(grid_resolution-1) * 6;
    let n_index_bytes = n_indices * 4;

    let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: n_vertex_bytes,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX,
        mapped_at_creation: false,
    });

    let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: n_index_bytes,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDEX,
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

    let gen_vertex_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
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
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: vertex_buffer.as_entire_binding(),
            },
        ],
    });

    let gen_index_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &compute_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: index_buffer.as_entire_binding(),
            },
        ],
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: None,
    });

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
    rx.await.expect("Could not get channel data").expect("Could not map buffer");
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

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: None,
    });

    encoder.copy_buffer_to_buffer(&index_buffer, 0, &result_buffer, 0, n_index_bytes);
    let command_buffer = encoder.finish();
    queue.submit([command_buffer]);

    info!("Mapping again {} bytes to {} byte indices", n_result_bytes, n_index_bytes);
    let (tx, rx) = futures::channel::oneshot::channel();
    result_buffer.map_async(wgpu::MapMode::Read, 0..n_result_bytes, move |res| {
        let _ = tx.send(res);
    });
    rx.await.expect("Could not get channel data").expect("Could not map buffer");
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
