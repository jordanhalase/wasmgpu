use wasm_bindgen::prelude::*;
use wgpu::*;
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
    let instance = Instance::new(&InstanceDescriptor::default());

    let surface = instance.create_surface(SurfaceTarget::Canvas(canvas))
        .expect("Could not create surface");

    let adapter = instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::None,
        force_fallback_adapter: false,
        compatible_surface: Some(&surface),
    })
    .await
    .expect("Could not get appropriate adapter");

    let (device, queue) = adapter.request_device(
        &DeviceDescriptor {
            label: None,
            required_features: Features::default(),
            required_limits: Limits::defaults(),
            experimental_features: ExperimentalFeatures::disabled(),
            memory_hints: MemoryHints::default(),
            trace: Trace::Off,
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
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor { label: None });
    let view = output.texture.create_view(&TextureViewDescriptor::default());
    {
        let _render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
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
}
