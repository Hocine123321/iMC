//! The Rust side of the iOS app (milestone M2, "hello Metal").
//!
//! The Xcode project's `main` calls [`horizon_ios_main`], which runs a winit event loop (winit starts
//! `UIApplicationMain` itself) and draws a spinning, textured, depth-tested cube through `wgpu` on
//! Metal. It is deliberately **not** Horizon's renderer: the point is to prove, on a real iPhone, that
//! Rust, winit, wgpu and Metal work together before the game's own code is involved (see
//! `docs/IOS_IMPLEMENTATION_ROADMAP.md`, M2 and M4).
//!
//! What it logs to stderr (Xcode's console): the adapter, its features and limits, and the frame
//! rate every five seconds. It drops the GPU state on `suspended` and rebuilds it on `resumed`, the
//! simplest lifecycle that does not touch Metal in the background.

mod math;
mod overlay;
mod telemetry;

#[cfg(feature = "game")]
mod game;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

const SHADER: &str = r#"
struct Uniforms { mvp: mat4x4<f32> };
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) shade: f32,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) shade: f32,
) -> VsOut {
    var out: VsOut;
    out.position = u.mvp * vec4<f32>(position, 1.0);
    out.uv = uv;
    out.shade = shade;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let colour = textureSample(tex, samp, in.uv);
    return vec4<f32>(colour.rgb * in.shade, 1.0);
}
"#;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const TEXTURE_SIZE: u32 = 64;
const FLOATS_PER_VERTEX: usize = 6;

/// 36 vertices: position (3), texture coordinate (2), face shade (1).
fn cube_vertices() -> Vec<f32> {
    // Four corners per face, and how bright the face is.
    let faces: [([[f32; 3]; 4], f32); 6] = [
        ([[-1., -1., 1.], [1., -1., 1.], [1., 1., 1.], [-1., 1., 1.]], 1.0),
        ([[1., -1., -1.], [-1., -1., -1.], [-1., 1., -1.], [1., 1., -1.]], 0.55),
        ([[1., -1., 1.], [1., -1., -1.], [1., 1., -1.], [1., 1., 1.]], 0.8),
        ([[-1., -1., -1.], [-1., -1., 1.], [-1., 1., 1.], [-1., 1., -1.]], 0.65),
        ([[-1., 1., 1.], [1., 1., 1.], [1., 1., -1.], [-1., 1., -1.]], 0.95),
        ([[-1., -1., -1.], [1., -1., -1.], [1., -1., 1.], [-1., -1., 1.]], 0.45),
    ];
    let uvs = [[0., 1.], [1., 1.], [1., 0.], [0., 0.]];
    let mut out = Vec::with_capacity(36 * FLOATS_PER_VERTEX);
    for (corners, shade) in faces {
        for i in [0, 1, 2, 0, 2, 3] {
            out.extend_from_slice(&corners[i]);
            out.extend_from_slice(&uvs[i]);
            out.push(shade);
        }
    }
    out
}

fn floats_to_bytes(floats: &[f32]) -> Vec<u8> {
    floats.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// An 8-by-8 checkerboard of 8-pixel squares, RGBA.
fn checkerboard() -> Vec<u8> {
    let mut data = Vec::with_capacity((TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize);
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let light = (x / 8 + y / 8) % 2 == 0;
            data.extend_from_slice(if light {
                &[240, 160, 60, 255]
            } else {
                &[40, 60, 110, 255]
            });
        }
    }
    data
}

/// Everything that lives on the GPU and the surface.
struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    uniforms: wgpu::Buffer,
    depth: wgpu::TextureView,
    overlay: overlay::Overlay,
    overlay_updated: Instant,
    last_frame: Instant,
    started: Instant,
    frames: u32,
    last_report: Instant,
}

impl State {
    fn new(window: Arc<Window>) -> Result<Self, String> {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        if cfg!(any(target_os = "ios", target_os = "macos")) {
            descriptor.backends = wgpu::Backends::METAL;
        }
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("create_surface: {e}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .map_err(|e| format!("request_adapter: {e}"))?;

        let info = adapter.get_info();
        eprintln!(
            "horizon-ios: adapter {:?} ({:?}, {:?}), driver {:?}",
            info.name, info.backend, info.device_type, info.driver
        );
        eprintln!("horizon-ios: features {:?}", adapter.features());
        eprintln!("horizon-ios: limits {:?}", adapter.limits());
        // The same, as a file in Documents/telemetry (the Files app shows it): ledger L-012, L-025.
        telemetry::write_text(
            "adapter.txt",
            &format!(
                "{}\nadapter: {:?}\n\ndownlevel: {:?}\n\nfeatures: {:?}\n\nlimits: {:#?}\n",
                telemetry::device_report(),
                info,
                adapter.get_downlevel_capabilities(),
                adapter.features(),
                adapter.limits()
            ),
        );

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("horizon-ios device"),
            required_features: wgpu::Features::empty(),
            // The adapter's own limits, as Horizon does: wgpu's defaults ask for 16 inter-stage
            // variables and iOS's Metal offers 15 (found by the simulator smoke test in CI).
            required_limits: adapter.limits(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        }))
        .map_err(|e| format!("request_device: {e}"))?;

        let capabilities = surface.get_capabilities(&adapter);
        let format = *capabilities
            .formats
            .first()
            .ok_or("the surface has no formats")?;
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::default(),
        };
        surface.configure(&device, &config);
        eprintln!(
            "horizon-ios: surface {}x{} {:?}",
            config.width, config.height, config.format
        );

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("horizon-ios cube"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let texture_extent = wgpu::Extent3d {
            width: TEXTURE_SIZE,
            height: TEXTURE_SIZE,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("checkerboard"),
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // sRGB like the surface (Bgra8UnormSrgb on Metal), so the colours come out as written.
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &checkerboard(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TEXTURE_SIZE * 4),
                rows_per_image: Some(TEXTURE_SIZE),
            },
            texture_extent,
        );
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let vertex_bytes = floats_to_bytes(&cube_vertices());
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cube vertices"),
            size: vertex_bytes.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&vertices, 0, &vertex_bytes);
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cube uniforms"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let layout_entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty,
            count: None,
        };
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cube bindings"),
            entries: &[
                layout_entry(
                    0,
                    wgpu::ShaderStages::VERTEX,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                layout_entry(
                    1,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                layout_entry(
                    2,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cube bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cube layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            ..Default::default()
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cube pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: (FLOATS_PER_VERTEX * 4) as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let depth = depth_view(&device, config.width, config.height);
        let overlay = overlay::Overlay::new(&device, &queue, config.format, DEPTH_FORMAT);
        overlay.set_layout(&queue, config.width, config.height);
        let now = Instant::now();
        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
            bind_group,
            vertices,
            uniforms,
            depth,
            overlay,
            overlay_updated: now,
            last_frame: now,
            started: now,
            frames: 0,
            last_report: now,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth = depth_view(&self.device, width, height);
        self.overlay.set_layout(&self.queue, width, height);
    }

    fn render(&mut self) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                // Draw this one, and reconfigure for the next.
                let (w, h) = (self.config.width, self.config.height);
                self.render_to(frame);
                self.resize(w, h);
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost
            | wgpu::CurrentSurfaceTexture::Validation => {
                let size = self.window.inner_size();
                self.resize(size.width, size.height);
                return;
            }
        };
        self.render_to(frame);
    }

    fn render_to(&mut self, frame: wgpu::SurfaceTexture) {
        let seconds = self.started.elapsed().as_secs_f32();
        let aspect = self.config.width as f32 / self.config.height as f32;
        let model = math::mul(&math::rotation_y(seconds), &math::rotation_x(seconds * 0.6));
        let view = math::translation(0.0, 0.0, -4.5);
        let projection = math::perspective(std::f32::consts::FRAC_PI_3, aspect, 0.1, 50.0);
        let mvp = math::mul(&projection, &math::mul(&view, &model));
        self.queue
            .write_buffer(&self.uniforms, 0, &math::to_bytes(&mvp));

        let target = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("horizon-ios frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cube"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.07,
                            b: 0.12,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.draw(0..36, 0..1);
            self.overlay.draw(&mut pass);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        self.queue.present(frame);

        let frame_ms = self.last_frame.elapsed().as_secs_f32() * 1000.0;
        self.last_frame = Instant::now();
        telemetry::frame(frame_ms);
        if self.overlay_updated.elapsed() >= Duration::from_millis(500) {
            self.overlay.set_text(&self.queue, &telemetry::overlay_lines());
            self.overlay_updated = Instant::now();
        }

        self.frames += 1;
        let since = self.last_report.elapsed();
        if since >= Duration::from_secs(5) {
            eprintln!(
                "horizon-ios: {:.1} fps ({} frames in {:.1} s)",
                self.frames as f32 / since.as_secs_f32(),
                self.frames,
                since.as_secs_f32()
            );
            self.frames = 0;
            self.last_report = Instant::now();
        }
    }
}

fn depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

#[derive(Default)]
struct App {
    state: Option<State>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        eprintln!("horizon-ios: resumed");
        let window = match event_loop.create_window(Window::default_attributes().with_title("iMC")) {
            Ok(window) => Arc::new(window),
            Err(e) => {
                eprintln!("horizon-ios: create_window failed: {e}");
                event_loop.exit();
                return;
            }
        };
        match State::new(window) {
            Ok(state) => {
                state.window.request_redraw();
                self.state = Some(state);
            }
            Err(e) => {
                eprintln!("horizon-ios: GPU setup failed: {e}");
                event_loop.exit();
            }
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // No Metal work in the background: drop the surface and the device, rebuild on resume.
        eprintln!("horizon-ios: suspended");
        self.state = None;
    }

    fn memory_warning(&mut self, _event_loop: &ActiveEventLoop) {
        eprintln!("horizon-ios: memory warning");
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(state) = &mut self.state {
                    state.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(state) = &mut self.state {
                    state.render();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}

/// Runs the app until it is closed. On iOS winit never returns from this: it owns `UIApplicationMain`.
pub fn run() {
    telemetry::start("cube");
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            eprintln!("horizon-ios: no event loop: {e}");
            return;
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);
    if let Err(e) = event_loop.run_app(&mut App::default()) {
        eprintln!("horizon-ios: event loop ended with an error: {e}");
    }
}

/// The function the Xcode app's `main` calls (`platform/ios/App/horizon_ios.h`).
#[unsafe(no_mangle)]
pub extern "C" fn horizon_ios_main() {
    run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cube_has_twelve_triangles() {
        assert_eq!(cube_vertices().len(), 36 * FLOATS_PER_VERTEX);
    }

    #[test]
    fn the_texture_is_rgba_for_every_pixel() {
        assert_eq!(checkerboard().len(), (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize);
    }
}
