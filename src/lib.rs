#![no_std]

use glam::{Mat4, Vec3};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;
use wgpu::{
    Surface,
    util::{BufferInitDescriptor, DeviceExt},
};

mod meshgrid;

struct Camera {
    eye: Vec3,
    target: Vec3,
    aspect: f32,
    fovy: f32,
    znear: f32,
    zfar: f32,
}

impl Camera {
    const UP: Vec3 = Vec3::Z;

    fn view_proj(&self) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye, self.target, Self::UP);
        let proj = Mat4::perspective_rh(self.fovy, self.aspect, self.znear, self.zfar);
        proj * view
    }

    /// Rotate about the z axis in radians
    fn rotate_z(&mut self, angle: f32) {
        self.eye = self.eye.rotate_z(angle); // TODO: Normalize to constant magnitude
    }
}

#[wasm_bindgen]
pub struct State {
    surface: Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    meshgrid_buffers: meshgrid::GridBuffers,
    depth_texture: wgpu::Texture,
    depth_texture_view: wgpu::TextureView,
    render_pipeline: wgpu::RenderPipeline,
    camera: Camera,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
}

impl State {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    // This is an associated function because if it took &self then it would not be callable from the constructor
    #[must_use]
    pub fn configure_depth_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let depth_texture_view = depth_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Depth texture view"),
            ..Default::default()
        });

        // TODO: Group these depth objects somehow
        /*let depth_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            ..Default::default()
        });*/
        (depth_texture, depth_texture_view)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);

            // More efficient to explicitly destory here than rely on Drop
            self.depth_texture.destroy();
            let (tex, view) = Self::configure_depth_texture(&self.device, width, height);
            self.depth_texture = tex;
            self.depth_texture_view = view;

            self.camera.aspect = width as f32 / height as f32;
            let camera_uniform = self.camera.view_proj();
            self.queue
                .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera_uniform));
        }
    }

    pub fn render(&mut self) {
        let output = self
            .surface
            .get_current_texture()
            .expect("Could not get current texture");

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Command encoder"),
            });
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.meshgrid_buffers.vertex_buffer.slice(..));
            render_pass.set_index_buffer(
                self.meshgrid_buffers.index_buffer.slice(..),
                self.meshgrid_buffers.index_format,
            );
            render_pass.draw_indexed(0..self.meshgrid_buffers.index_count, 0, 0..1);
        }

        let command_buffer = encoder.finish();
        self.queue.submit([command_buffer]);
        output.present();
    }

    pub fn rotate_z(&mut self, angle: f32) {
        self.camera.rotate_z(angle);
        let camera_uniform = self.camera.view_proj();
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera_uniform));
    }
}

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

// Wasm bindgen doesn't like standalones in impls so we declare bare functions
// Also could instead move them out of the impl but they make more sense there

#[wasm_bindgen]
pub fn resize(s: &mut State, width: u32, height: u32) {
    s.resize(width, height);
}

#[wasm_bindgen]
pub fn render(s: &mut State) {
    s.render();
}

#[wasm_bindgen]
pub fn rotate_z(s: &mut State, angle: f32) {
    s.rotate_z(angle * core::f32::consts::PI);
}

#[wasm_bindgen]
pub async fn start_webgpu_app(canvas: HtmlCanvasElement) -> State {
    console_log::init().expect("Could not initiate logging");
    console_error_panic_hook::set_once();

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
            label: Some("Device"),
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
    let meshgrid_buffers = meshgrid_generator.generate_buffers((31, 31), -5.0..=5.0, -5.0..=5.0);

    let evaluator_module = device.create_shader_module(wgpu::include_wgsl!("evaluator.wgsl"));
    let evaluator = meshgrid_generator.create_evaluator(&evaluator_module, Some("evaluate"));

    evaluator.evaluate_buffers(&[&meshgrid_buffers]);

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

    // TODO: Move render setup somewhere else

    let camera_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

    let camera = Camera {
        eye: Vec3::new(4.0, -8.0, 8.0),
        target: Vec3::ZERO,
        aspect: width as f32 / height as f32,
        fovy: f32::to_radians(90.0),
        znear: 0.1,
        zfar: 100.0,
    };

    let camera_uniform: Mat4 = camera.view_proj();

    let camera_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("Camera buffer"),
        contents: bytemuck::bytes_of(&camera_uniform),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Camera bind group"),
        layout: &camera_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(camera_buffer.as_entire_buffer_binding()),
        }],
    });

    let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render pipeline layout"),
        bind_group_layouts: &[&camera_bind_group_layout],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render pipeline"),
        layout: Some(&render_pipeline_layout),
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
        depth_stencil: Some(wgpu::DepthStencilState {
            format: State::DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    });

    let (depth_texture, depth_texture_view) =
        State::configure_depth_texture(&device, width, height);

    State {
        surface,
        device,
        queue,
        config,
        meshgrid_buffers,
        depth_texture,
        depth_texture_view,
        render_pipeline,
        camera,
        camera_buffer,
        camera_bind_group,
    }
}
