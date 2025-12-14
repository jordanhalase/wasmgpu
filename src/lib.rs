#![no_std]

use log::info;
use wasm_bindgen::prelude::*;

mod meshgrid;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use core::mem::size_of;

        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[wasm_bindgen]
pub async fn start_webgpu_app(canvas_id: &str) {
    // Get the canvas element from the DOM
    let window = web_sys::window().expect("No window found");
    let document = window.document().expect("No document found");
    let canvas = document
        .get_element_by_id(canvas_id)
        .expect("No element found")
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .expect("Element is not HTMLCanvasElement");

    console_log::init().expect("Could not initiate logging");

    let width = canvas.width();
    let height = canvas.height();

    // Create a wgpu instance
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

    let surface = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
        .expect("Could not create surface");

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::None,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        })
        .await
        .expect("Could not get appropriate adapter");

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::default(),
            required_limits: wgpu::Limits::defaults(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        })
        .await
        .expect("Could not create device");

    // Create a compute pipeline

    let meshgrid_generator = meshgrid::Generator::new(&device, &queue);
    let meshgrid_buffers = meshgrid_generator.generate_buffers((2, 2), -1.0..1.0, -1.0..1.0);

    // Inspect the meshgrid buffers
    #[cfg(feature = "readback")]
    {
        meshgrid_generator.print_vertices(&meshgrid_buffers).await;
        meshgrid_generator.print_indices(&meshgrid_buffers).await;
    }

    // Configure the surface
    let config = surface
        .get_default_config(&adapter, width, height)
        .expect("Surface not supported by adapter");
    surface.configure(&device, &config);

    let output = surface
        .get_current_texture()
        .expect("Could not get current texture");

    let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: None, // TODO
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[Vertex::desc()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: config.format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None, // Some(wgpu::Face::Back),
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    });

    // Render image (simplified)
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let view = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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

        render_pass.set_pipeline(&render_pipeline);
        render_pass.set_vertex_buffer(0, meshgrid_buffers.vertex_buffer.slice(..));
        render_pass.set_index_buffer(
            meshgrid_buffers.index_buffer.slice(..),
            meshgrid_buffers.index_format,
        );
        render_pass.draw_indexed(0..meshgrid_buffers.index_count, 0, 0..1);
    }

    let command_buffer = encoder.finish();
    queue.submit([command_buffer]);
    output.present();

    info!("Screen resolution: {width}, {height}");
}
