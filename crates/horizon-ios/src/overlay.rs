//! A small on-screen text overlay for the telemetry (milestone M3): a 5x7 pixel font rasterised on
//! the CPU into a texture twice a second, drawn as one alpha-blended quad in the corner. No font
//! file and no dependency, so the overlay works before any of Horizon's own text code is running.

const COLS: usize = 46;
const LINES: usize = 6;
const CELL_W: usize = 6;
const CELL_H: usize = 8;
const PAD: usize = 3;
pub const WIDTH: u32 = (COLS * CELL_W + 2 * PAD) as u32;
pub const HEIGHT: u32 = (LINES * CELL_H + 2 * PAD) as u32;

/// The rows of a character, five bits each (bit 4 is the left column). Upper case, digits and a
/// little punctuation; anything else is blank.
pub fn glyph(c: char) -> [u8; 7] {
    match c {
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        '.' => [0, 0, 0, 0, 0, 0b01100, 0b01100],
        ':' => [0, 0b01100, 0b01100, 0, 0b01100, 0b01100, 0],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        '(' => [0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010],
        ')' => [0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000],
        '?' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0, 0b00100],
        '/' => [0b00001, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b10000],
        '%' => [0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011],
        _ => [0; 7],
    }
}

/// RGBA pixels (`WIDTH * HEIGHT * 4` bytes) of the lines: white text on a translucent black panel.
pub fn rasterize(lines: &[String]) -> Vec<u8> {
    let (w, h) = (WIDTH as usize, HEIGHT as usize);
    let mut pixels = vec![0u8; w * h * 4];
    for px in pixels.chunks_exact_mut(4) {
        px.copy_from_slice(&[0, 0, 0, 150]);
    }
    for (line_index, line) in lines.iter().take(LINES).enumerate() {
        for (col, c) in line.chars().take(COLS).enumerate() {
            for (row, bits) in glyph(c.to_ascii_uppercase()).into_iter().enumerate() {
                for bit in 0..5 {
                    if bits & (0b10000 >> bit) != 0 {
                        let x = PAD + col * CELL_W + bit;
                        let y = PAD + line_index * CELL_H + row;
                        let at = (y * w + x) * 4;
                        pixels[at..at + 4].copy_from_slice(&[255, 255, 255, 255]);
                    }
                }
            }
        }
    }
    pixels
}

const SHADER: &str = r#"
struct Rect { v: vec4<f32> };
@group(0) @binding(0) var<uniform> r: Rect;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> Out {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let c = corners[i];
    var out: Out;
    out.position = vec4<f32>(mix(r.v.x, r.v.z, c.x), mix(r.v.w, r.v.y, c.y), 0.0, 1.0);
    out.uv = c;
    return out;
}

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, in.uv);
}
"#;

/// The overlay's GPU side.
pub struct Overlay {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    rect: wgpu::Buffer,
    texture: wgpu::Texture,
}

impl Overlay {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let extent = wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("overlay text"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("overlay nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let rect = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("overlay rect"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("overlay bindings"),
            entries: &[
                entry(
                    0,
                    wgpu::ShaderStages::VERTEX,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(
                    1,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    2,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("overlay bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: rect.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("overlay layout"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let overlay = Self {
            pipeline,
            bind_group,
            rect,
            texture,
        };
        overlay.set_text(queue, &["STARTING".to_string()]);
        overlay
    }

    /// Re-rasterises the text into the texture.
    pub fn set_text(&self, queue: &wgpu::Queue, lines: &[String]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rasterize(lines),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(WIDTH * 4),
                rows_per_image: Some(HEIGHT),
            },
            wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Places the panel in the top left corner of a `width` x `height` pixel target.
    pub fn set_layout(&self, queue: &wgpu::Queue, width: u32, height: u32) {
        let rect = panel_rect(width as f32, height as f32);
        let bytes: Vec<u8> = rect.iter().flat_map(|v| v.to_le_bytes()).collect();
        queue.write_buffer(&self.rect, 0, &bytes);
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}

/// The panel's corners in clip space (x0, y0 bottom, x1, y1 top) on a target of the given size: a
/// whole-number pixel scale that grows with the screen, with a margin clear of a notch or the
/// rounded corners.
pub fn panel_rect(width: f32, height: f32) -> [f32; 4] {
    let scale = (height.min(width) / 380.0).round().max(1.0);
    let (panel_w, panel_h) = (WIDTH as f32 * scale, HEIGHT as f32 * scale);
    let (margin_x, margin_y) = (28.0 * scale, 14.0 * scale);
    let x0 = -1.0 + 2.0 * margin_x / width;
    let y1 = 1.0 - 2.0 * margin_y / height;
    [x0, y1 - 2.0 * panel_h / height, x0 + 2.0 * panel_w / width, y1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_overlay_character_has_a_glyph() {
        // The characters the telemetry lines use.
        for c in "FPS P50 P99 MS FOOTPRINT MB PEAK AVAILABLE MIN THERMAL NOMINAL FAIR SERIOUS CRITICAL LOW POWER ON OFF UPTIME MODE CUBE GAME TELEMETRY STARTING IPHONE CPU 0123456789.:-()/%?".chars() {
            if c != ' ' {
                assert_ne!(glyph(c), [0; 7], "no glyph for {c:?}");
            }
        }
    }

    #[test]
    fn rasterising_fills_the_texture_and_draws_text() {
        let pixels = rasterize(&["A".to_string()]);
        assert_eq!(pixels.len(), (WIDTH * HEIGHT * 4) as usize);
        assert!(pixels.chunks_exact(4).any(|p| p == [255, 255, 255, 255]));
        assert!(pixels.chunks_exact(4).any(|p| p == [0, 0, 0, 150]));
    }

    #[test]
    fn long_text_is_cut_not_overrun() {
        let long = "W".repeat(COLS * 3);
        assert_eq!(rasterize(&[long]).len(), (WIDTH * HEIGHT * 4) as usize);
    }

    #[test]
    fn the_panel_sits_in_the_top_left_inside_the_screen() {
        let [x0, y0, x1, y1] = panel_rect(2250.0, 1146.0);
        assert!(x0 > -1.0 && x1 < 1.0 && y0 > -1.0 && y1 < 1.0);
        assert!(x1 > x0 && y1 > y0);
        assert!(x0 < -0.9 && y1 > 0.9);
    }
}
