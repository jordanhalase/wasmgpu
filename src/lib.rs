use wasm_bindgen::prelude::*;
use wgpu;
use log::info;

#[wasm_bindgen]
pub async fn start_webgpu_app(canvas_id: String) {
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

    let n_elements = 32*32;
    let n_bytes = n_elements * 4;

    let work_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: n_bytes,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    let result_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: n_bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let compute_shader_text = r#"
    @group(0) @binding(0) var<storage, read_write> data: array<u32>;

    @compute @workgroup_size(32) fn compute_main(@builtin(global_invocation_id) gid: vec3u)
    {
        let i = gid.y*32 + gid.x;
        data[i] = i;
    }
    "#;

    let compute_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(compute_shader_text.into()),
    });

    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&compute_pipeline_layout),
        module: &compute_shader_module,
        entry_point: Some("compute_main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &compute_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: work_buffer.as_entire_binding(),
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
        pass.set_pipeline(&compute_pipeline);
        pass.set_bind_group(0, &compute_bind_group, &[]);
        pass.dispatch_workgroups(1, 32, 1);
    }
    encoder.copy_buffer_to_buffer(&work_buffer, 0, &result_buffer, 0, n_bytes);
    let command_buffer = encoder.finish();
    queue.submit([command_buffer]);

    let buf = result_buffer.clone();
    result_buffer.map_async(wgpu::MapMode::Read, 0..n_bytes, move |res| {
        res.expect("Could not map buffer");
        info!("Wew!");

        let mapped = buf.get_mapped_range(..);
        let uints: &[u32] = bytemuck::cast_slice(&mapped);
        info!("{:?}", &uints[..]);

        drop(mapped);
        buf.unmap();
    });
}
