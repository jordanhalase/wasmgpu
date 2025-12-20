#![no_std]

use glam::{Mat4, Vec3};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;
use wgpu::{
    Surface,
    util::{BufferInitDescriptor, DeviceExt},
};

use core::f32::consts::{PI, TAU};

mod meshgrid;

#[inline(always)]
fn float_modulo(a: f32, b: f32) -> f32 {
    let r = a % b;
    if r < 0.0 { r + b.abs() } else { r }
}

struct Camera {
    target: Vec3,
    distance: f32,
    zenith: f32,
    azimuth: f32,
    aspect: f32,
    fovy: f32,
    znear: f32,
    zfar: f32,
}

impl Camera {
    const UP: Vec3 = Vec3::Z;
    const CLOSEST: f32 = 0.1;
    const FARTHEST: f32 = 15.0;
    const ZENITH_CLAMP: f32 = 0.01;

    fn view_proj(&self) -> Mat4 {
        let eye = self.spherical_to_rect() - self.target;
        let view = Mat4::look_at_rh(eye, self.target, Self::UP);
        let proj = Mat4::perspective_rh(self.fovy, self.aspect, self.znear, self.zfar);
        proj * view
    }

    /// Rotate from the Z axis in radians
    fn rotate_zenith(&mut self, angle: f32) {
        self.zenith = (self.zenith + angle).clamp(Self::ZENITH_CLAMP, PI - Self::ZENITH_CLAMP);
    }

    /// Rotate about the XY plane in radians
    fn rotate_azimuth(&mut self, angle: f32) {
        self.azimuth = float_modulo(self.azimuth + angle, TAU);
    }

    fn move_distance(&mut self, distance: f32) {
        self.distance *= 1.0 - distance;
        self.distance = self.distance.clamp(Self::CLOSEST, Self::FARTHEST);
    }

    /// Construct an X, Y, Z coordinate from the `distance`, `zenith`, and `azimuth` coordinate
    fn spherical_to_rect(&self) -> Vec3 {
        let sc_zenith = libm::sincosf(self.zenith);
        let sc_azimuth = libm::sincosf(self.azimuth);

        Vec3 {
            x: self.distance * sc_zenith.0 * sc_azimuth.1,
            y: self.distance * sc_zenith.0 * sc_azimuth.0,
            z: self.distance * sc_zenith.1,
        }
    }
}

pub struct State {
    surface: Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    meshgrid_generator: meshgrid::Generator,
    meshgrid_buffers: meshgrid::GridBuffers,
    evaluator: meshgrid::Evaluator,
    depth_texture: wgpu::Texture,
    depth_texture_view: wgpu::TextureView,
    render_pipeline_layout: wgpu::PipelineLayout,
    render_shader_module: wgpu::ShaderModule,
    render_pipeline: wgpu::RenderPipeline,
    camera: Camera,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    multisample_texture: Option<wgpu::Texture>,
    multisample_texture_view: Option<wgpu::TextureView>,
    multisampling_enabled: bool,
}

impl State {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
    pub const MSAA_SAMPLE_COUNT: u32 = 4;

    pub async fn new(
        width: u32,
        height: u32,
        adapter: wgpu::Adapter,
        surface: wgpu::Surface<'static>,
    ) -> Self {
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
        let meshgrid_buffers =
            meshgrid_generator.generate_buffers((255, 255), -5.0..=5.0, -5.0..=5.0);

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
            //eye: Vec3::new(4.0, -8.0, 8.0), // TODO: Remove
            target: Vec3::ZERO,
            distance: 12.0,
            zenith: 0.841_068_7,
            azimuth: 1.107_148_8,
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

        let render_shader_module = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render pipeline layout"),
                bind_group_layouts: &[&camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = Self::create_render_pipeline(
            &device,
            &config,
            Some(&render_pipeline_layout),
            &render_shader_module,
            Some("vs_main"),
            Some("fs_main"),
            1,
        );

        let (depth_texture, depth_texture_view) = Self::create_depth_texture(&device, &config, 1);

        Self {
            surface,
            device,
            queue,
            config,
            meshgrid_generator,
            meshgrid_buffers,
            evaluator,
            depth_texture,
            depth_texture_view,
            render_pipeline_layout,
            render_shader_module,
            render_pipeline,
            camera,
            camera_buffer,
            camera_bind_group,
            multisample_texture: None,
            multisample_texture_view: None,
            multisampling_enabled: false,
        }
    }

    // This is an associated function because if it took &self then it would not be callable from the constructor
    #[must_use]
    fn create_depth_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        sample_count: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth texture"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let depth_texture_view = depth_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Depth texture view"),
            ..Default::default()
        });

        (depth_texture, depth_texture_view)
    }

    #[must_use]
    fn create_multisample_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let mutlisample_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Multisample texture"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: Self::MSAA_SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let multisample_texture_view =
            mutlisample_texture.create_view(&wgpu::TextureViewDescriptor::default());

        (mutlisample_texture, multisample_texture_view)
    }

    #[must_use]
    fn create_render_pipeline(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        layout: Option<&wgpu::PipelineLayout>,
        module: &wgpu::ShaderModule,
        vs_main: Option<&str>,
        fs_main: Option<&str>,
        sample_count: u32,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render pipeline"),
            layout,
            vertex: wgpu::VertexState {
                module,
                entry_point: vs_main,
                buffers: &[Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: fs_main,
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
                format: Self::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);

            // More efficient to explicitly destory here than rely on Drop
            self.depth_texture.destroy();

            let sample_count;
            if self.multisampling_enabled {
                sample_count = Self::MSAA_SAMPLE_COUNT;

                // May have just been enabled so this may be None
                if let Some(multisample_texture) = self.multisample_texture.take() {
                    multisample_texture.destroy();
                }

                let (tex, view) = Self::create_multisample_texture(&self.device, &self.config);
                self.multisample_texture.replace(tex);
                self.multisample_texture_view.replace(view);
            } else {
                sample_count = 1;
            }

            let (tex, view) = Self::create_depth_texture(&self.device, &self.config, sample_count);
            self.depth_texture = tex;
            self.depth_texture_view = view;

            self.camera.aspect = width as f32 / height as f32;
            let camera_uniform = self.camera.view_proj();
            self.queue
                .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera_uniform));
        }
    }

    pub fn set_grid_resolution(&mut self, width: u32, height: u32) {
        self.meshgrid_buffers.destroy();

        let meshgrid_buffers =
            self.meshgrid_generator
                .generate_buffers((width, height), -5.0..=5.0, -5.0..=5.0);

        self.evaluator.evaluate_buffers(&[&meshgrid_buffers]);
        self.meshgrid_buffers = meshgrid_buffers;
    }

    pub fn render(&mut self) {
        let output = self
            .surface
            .get_current_texture()
            .expect("Could not get current texture");

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let (view, resolve_target) = if self.multisampling_enabled {
            (
                self.multisample_texture_view
                    .as_ref()
                    .expect("Multisampling enabled with no texture view"),
                Some(view),
            )
        } else {
            (&view, None)
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Command encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: resolve_target.as_ref(),
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

    pub fn move_camera(&mut self, distance: f32, zenith: f32, azimuth: f32) {
        self.camera.move_distance(distance);
        self.camera.rotate_zenith(zenith);
        self.camera.rotate_azimuth(azimuth);
        let camera_uniform = self.camera.view_proj();
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera_uniform));
    }

    pub fn set_multisampling_enabled(&mut self, enabled: bool) {
        let sample_count;
        if enabled {
            if self.multisampling_enabled {
                // Nothing to do
                return;
            }
            self.multisampling_enabled = true;
            sample_count = Self::MSAA_SAMPLE_COUNT;
        } else {
            if !self.multisampling_enabled {
                // Nothing to do
                return;
            }
            self.multisampling_enabled = false;

            self.multisample_texture
                .as_mut()
                .expect("Multisampling enabled with no texture")
                .destroy();
            self.multisample_texture = None;
            self.multisample_texture_view = None;
            sample_count = 1;
        }

        // Technically a resize for the render targets
        self.resize(self.config.width, self.config.height);

        let render_pipeline = Self::create_render_pipeline(
            &self.device,
            &self.config,
            Some(&self.render_pipeline_layout),
            &self.render_shader_module,
            Some("vs_main"),
            Some("fs_main"),
            sample_count,
        );

        self.render_pipeline = render_pipeline;
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

#[wasm_bindgen]
pub struct JsApp {
    inner: State,
}

#[wasm_bindgen]
impl JsApp {
    pub fn resize(&mut self, width: u32, height: u32) {
        self.inner.resize(width, height);
    }

    pub fn move_camera(&mut self, distance: f32, zenith: f32, azimuth: f32) {
        self.inner.move_camera(distance, zenith * PI, azimuth * PI);
    }

    pub fn render(&mut self) {
        self.inner.render();
    }

    pub fn set_multisampling_enabled(&mut self, enabled: bool) {
        self.inner.set_multisampling_enabled(enabled);
        self.inner.render();
    }

    pub fn set_grid_resolution(&mut self, width: u32, height: u32) {
        self.inner.set_grid_resolution(width, height);
        self.inner.render();
    }
}

// Wasm bindgen currently does not support async constructors
#[wasm_bindgen]
pub async fn start_app(canvas: HtmlCanvasElement) -> JsApp {
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

    JsApp {
        inner: State::new(width, height, adapter, surface).await,
    }
}
