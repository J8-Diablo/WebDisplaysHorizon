//! The mod's own renderer: browser pictures drawn on quads in the world.
//!
//! Horizon's built-in drawing only takes sprites of the shared atlas, and a browser picture
//! changes every frame, so this draws with wgpu from the render hook `world_drawn`, over the
//! frame's picture (the approach of Theatrical's `fx.rs`). The world's depth is only bound
//! to read there, and is smaller than the picture under Resolution Scale, so what is hidden
//! behind a block is thrown away by hand in the fragment shader.
//!
//! Game hooks say what to draw (`set_quads`) and where each browser's frames are (`attach`);
//! the render thread reads the frames from hcef-host's shared memory and draws.
use super::ambilight::{Ambilight, Frame};
use super::geometry::{CARD, QUAD_VERTICES, Quad, VERTEX, test_card, vertices};
use super::glow::{self, CELLS, Glow};
use crate::ipc::shm::Mapping;
use crate::proto::frame::{self, Reader};
use horizon_client::mod_api::{RenderHooks, RenderNeedsEvent, WorldDrawnEvent, wgpu};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

/// Bytes of the shader's uniforms: the view-projection, then two vectors of parameters.
const UNIFORMS: u64 = 96;
/// How bright a pixel of an OLED screen must be to show at all, and over how much more
/// brightness it becomes solid: black is see-through, and dark greys fade in.
const OLED: [f32; 2] = [0.03, 0.10];
/// The page shown where a browser has painted nothing yet: the mod's test card.
pub const NO_PAGE: u32 = u32::MAX;
/// A plain dark page: the back and the edges of a traced screen.
pub const PANEL_PAGE: u32 = u32::MAX - 1;
/// A plain bright page: the outline a player is tracing.
pub const MARK_PAGE: u32 = u32::MAX - 2;

const SHADER: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    // xy: picture pixels to depth pixels; z: 1 when the frame is linear light; w: 1 for a
    // screen whose black is see-through.
    params: vec4<f32>,
    // x: the brightness under which such a screen shows nothing; y: over how much more it
    // becomes solid.
    oled: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var page: texture_2d<f32>;
@group(0) @binding(2) var page_sampler: sampler;
@group(0) @binding(3) var scene: texture_depth_2d;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex fn vs(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>) -> VertexOut {
    var out: VertexOut;
    out.clip = u.view_proj * vec4<f32>(pos, 1.0);
    out.uv = uv;
    return out;
}

// A browser paints sRGB. Horizon's 8-bit and 16-bit frames hold gamma-encoded values too,
// so the bytes go through as they are; only its float frame is linear light.
fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

@fragment fn fs(in: VertexOut) -> @location(0) vec4<f32> {
    // The world's depth is reversed (0 is the sky): what is behind it is not drawn.
    let at = vec2<i32>(in.clip.xy * u.params.xy);
    let behind = in.clip.z < textureLoad(scene, at, 0);
    var rgb = textureSampleLevel(page, page_sampler, in.uv, 0.0).rgb;
    // Past the picture's edge: the bars beside a picture fitted whole in a screen of
    // another shape. A hair of slack, so that an edge that is exactly the edge is not one.
    if (in.uv.x < -0.0005 || in.uv.y < -0.0005 || in.uv.x > 1.0005 || in.uv.y > 1.0005) {
        rgb = vec3<f32>(0.0);
    }
    // As bright as its brightest channel: a pure red is not dark.
    let bright = max(rgb.r, max(rgb.g, rgb.b));
    if (u.params.z > 0.5) {
        rgb = to_linear(rgb);
    }
    if (behind) {
        discard;
    }
    var alpha = 1.0;
    if (u.params.w > 0.5) {
        alpha = smoothstep(u.oled.x, u.oled.x + u.oled.y, bright);
    }
    return vec4<f32>(rgb, alpha);
}
"#;

/// A browser's picture on the GPU.
struct Page {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: [u32; 2],
}

/// What lives on the GPU. Made again when the game makes a new device or the frame changes
/// format (HDR on or off).
struct Gpu {
    device: wgpu::Device,
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    /// The same uniforms for the screens whose black is see-through.
    uniforms_oled: wgpu::Buffer,
    /// The uniforms of what is laid flat over the picture: no camera, no world in front.
    uniforms_flat: wgpu::Buffer,
    ambilight: Ambilight,
    /// The colours each page's lights show, following its picture.
    colors: HashMap<u32, ([[f32; 3]; CELLS], [[f32; 3]; CELLS])>,
    vertices: wgpu::Buffer,
    capacity: u64,
    sampler: wgpu::Sampler,
    pages: HashMap<u32, Page>,
}

#[derive(Default)]
pub struct Renderer {
    gpu: Mutex<Option<Gpu>>,
    quads: Mutex<Vec<Quad>>,
    /// Where each browser's frames are read from.
    sources: Mutex<HashMap<u32, Reader<Mapping>>>,
    /// Pages whose browser is gone, for the render thread to free.
    gone: Mutex<Vec<u32>>,
    /// The last frame copied out of a mapping, kept to be refilled without allocating.
    scratch: Mutex<Vec<u8>>,
    /// What is laid flat over the picture (the minePad): places in units of half the
    /// picture's height from its middle, x to the right and y up.
    overlay: Mutex<Vec<Quad>>,
    /// The lights the screens throw.
    glows: Mutex<Vec<Glow>>,
    /// The pages of the screens whose black is see-through.
    oled: Mutex<HashSet<u32>>,
    /// How strong the screens' light is, as `f32` bits; 0 for none.
    glow_strength: AtomicU32,
}

impl Renderer {
    /// What to draw from now on, from a game hook.
    pub fn set_quads(&self, quads: Vec<Quad>) {
        if let Ok(mut slot) = self.quads.lock() {
            *slot = quads;
        }
    }

    /// What to lay flat over the picture from now on (`client/pad.rs` says in what units).
    pub fn set_overlay(&self, quads: Vec<Quad>) {
        if let Ok(mut slot) = self.overlay.lock() {
            *slot = quads;
        }
    }

    /// The lights the screens throw from now on, `strength` strong (0 for none), and the
    /// pages whose black is see-through.
    pub fn set_look(&self, glows: Vec<Glow>, strength: f32, oled: HashSet<u32>) {
        if let Ok(mut slot) = self.glows.lock() {
            *slot = glows;
        }
        if let Ok(mut slot) = self.oled.lock() {
            *slot = oled;
        }
        self.glow_strength.store(strength.to_bits(), Ordering::Relaxed);
    }

    /// The frames of browser `page` are in the mapping `name`, sized for `capacity`. Called
    /// again when hcef-host moves them to a larger mapping.
    pub fn attach(&self, page: u32, name: &str, capacity: [u32; 2]) -> Result<(), String> {
        let reader = Reader::open(Mapping::open(name, frame::mapping_size(capacity[0], capacity[1]))?)?;
        self.sources.lock().map_err(|_| "the renderer's sources are poisoned")?.insert(page, reader);
        Ok(())
    }

    /// Browser `page` is gone: its frames are let go of and its texture freed.
    pub fn detach(&self, page: u32) {
        if let Ok(mut sources) = self.sources.lock() {
            sources.remove(&page);
        }
        if let Ok(mut gone) = self.gone.lock() {
            gone.push(page);
        }
    }

    /// Lets go of every browser's frames: hcef-host stopped.
    pub fn detach_all(&self) {
        let pages: Vec<u32> = self.sources.lock().map(|sources| sources.keys().copied().collect()).unwrap_or_default();
        for page in pages {
            self.detach(page);
        }
    }

    fn build(device: &wgpu::Device, format: wgpu::TextureFormat) -> Gpu {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("webdisplays"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry { binding, visibility, ty, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("webdisplays"),
            entries: &[
                entry(
                    0,
                    wgpu::ShaderStages::VERTEX_FRAGMENT,
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
                entry(2, wgpu::ShaderStages::FRAGMENT, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                entry(
                    3,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("webdisplays"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let attributes = [
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 12, shader_location: 1 },
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("webdisplays pages"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: VERTEX as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            // A screen is seen from its front only: its back is the block it is on.
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Cw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                // A page covers what is under it, but for the dark of an OLED screen, and
                // leaves the frame's alpha.
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniform_buffer = || {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("webdisplays uniforms"),
                size: UNIFORMS,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let (uniforms, uniforms_oled, uniforms_flat) = (uniform_buffer(), uniform_buffer(), uniform_buffer());
        let capacity = 1 << 14;
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("webdisplays"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Gpu {
            device: device.clone(),
            format,
            pipeline,
            layout,
            uniforms,
            uniforms_oled,
            uniforms_flat,
            ambilight: Ambilight::new(device, format),
            colors: HashMap::new(),
            vertices: Self::vertex_buffer(device, capacity),
            capacity,
            sampler,
            pages: HashMap::new(),
        }
    }

    fn vertex_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("webdisplays vertices"),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Puts a BGRA picture in `page`'s texture, made again when its size changed.
    fn show(gpu: &mut Gpu, queue: &wgpu::Queue, page: u32, bgra: &[u8], size: [u32; 2]) {
        let bytes = size[0] as usize * size[1] as usize * frame::PIXEL;
        if size[0] == 0 || size[1] == 0 || bgra.len() < bytes {
            return;
        }
        if gpu.pages.get(&page).is_none_or(|page| page.size != size) {
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("webdisplays page"),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // The bytes as Chromium paints them, not decoded: see `to_linear` in the shader.
                format: wgpu::TextureFormat::Bgra8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            gpu.pages.insert(page, Page { texture, view, size });
        }
        let Some(page) = gpu.pages.get(&page) else { return };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &page.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bgra[..bytes],
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size[0] * 4), rows_per_image: Some(size[1]) },
            wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        );
    }
}

impl RenderHooks for Renderer {
    fn render_needs(&self, event: &mut RenderNeedsEvent<'_>) {
        event.needs.world_depth = true;
    }

    fn world_drawn(&self, event: &mut WorldDrawnEvent<'_, '_>) {
        let world = &mut *event.world;
        let quads = match self.quads.lock() {
            Ok(quads) => quads.clone(),
            Err(_) => return,
        };
        let Ok(mut slot) = self.gpu.lock() else { return };
        if slot.as_ref().is_none_or(|gpu| gpu.device != *world.device || gpu.format != world.color_format) {
            *slot = Some(Self::build(world.device, world.color_format));
        }
        let Some(gpu) = slot.as_mut() else { return };
        if let Ok(mut gone) = self.gone.lock() {
            for page in gone.drain(..) {
                gpu.pages.remove(&page);
                gpu.colors.remove(&page);
            }
        }
        if !gpu.pages.contains_key(&NO_PAGE) {
            Self::show(gpu, world.queue, NO_PAGE, &test_card(), CARD);
            // The plain pages are one pixel each, in BGRA.
            Self::show(gpu, world.queue, PANEL_PAGE, &[24, 21, 20, 255], [1, 1]);
            Self::show(gpu, world.queue, MARK_PAGE, &[90, 255, 120, 255], [1, 1]);
        }
        // Each browser's newest frame, when it painted since the last one shown.
        if let (Ok(mut sources), Ok(mut scratch)) = (self.sources.lock(), self.scratch.lock()) {
            for (page, reader) in sources.iter_mut() {
                let Some(frame) = reader.latest() else { continue };
                scratch.resize(frame.len(), 0);
                if reader.memory().read(frame.pixels, &mut scratch) {
                    Self::show(gpu, world.queue, *page, &scratch, [frame.width, frame.height]);
                    // What the picture's lights are heading for.
                    let colors = gpu.colors.entry(*page).or_insert(([[0.0; 3]; CELLS], [[0.0; 3]; CELLS]));
                    colors.1 = glow::cell_colors(&scratch, [frame.width, frame.height]);
                }
            }
        }
        for (shown, picture) in gpu.colors.values_mut() {
            glow::follow(shown, picture);
        }
        let overlay = self.overlay.lock().map(|overlay| overlay.clone()).unwrap_or_default();
        if (quads.is_empty() && overlay.is_empty()) || world.size[0] == 0 || world.size[1] == 0 {
            return;
        }

        // The world's quads, then what is laid over the picture, in one buffer.
        let (mut vertices, runs) = vertices(&quads, world.camera.to_array());
        let (flat_vertices, flat_runs) = super::geometry::vertices(&overlay, [0.0; 3]);
        let world_vertices = runs.iter().map(|run| run.quads * QUAD_VERTICES).sum::<u32>();
        vertices.extend(flat_vertices);
        let bytes = vertices.len() as u64;
        if bytes > gpu.capacity {
            gpu.capacity = bytes.next_power_of_two();
            gpu.vertices = Self::vertex_buffer(world.device, gpu.capacity);
        }
        world.queue.write_buffer(&gpu.vertices, 0, &vertices);

        let depth_scale =
            [world.depth_size[0] as f32 / world.size[0] as f32, world.depth_size[1] as f32 / world.size[1] as f32];
        let is_linear = world.color_format == wgpu::TextureFormat::Rgba16Float;
        let linear = if is_linear { 1.0 } else { 0.0 };
        for (buffer, oled) in [(&gpu.uniforms, 0.0), (&gpu.uniforms_oled, 1.0)] {
            let mut uniforms = Vec::with_capacity(UNIFORMS as usize);
            for value in world.view_proj.to_cols_array() {
                uniforms.extend(value.to_le_bytes());
            }
            for value in [depth_scale[0], depth_scale[1], linear, oled, OLED[0], OLED[1], 0.0, 0.0] {
                uniforms.extend(value.to_le_bytes());
            }
            world.queue.write_buffer(buffer, 0, &uniforms);
        }
        // Flat over the picture: x squeezed by the picture's shape, and a depth nothing of
        // the world is in front of.
        let mut flat = Vec::with_capacity(UNIFORMS as usize);
        let squeeze = world.size[1] as f32 / world.size[0] as f32;
        let matrix = [squeeze, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0];
        for value in matrix.into_iter().chain([depth_scale[0], depth_scale[1], linear, 0.0, OLED[0], OLED[1], 0.0, 0.0]) {
            flat.extend(value.to_le_bytes());
        }
        world.queue.write_buffer(&gpu.uniforms_flat, 0, &flat);
        let see_through = self.oled.lock().map(|oled| oled.clone()).unwrap_or_default();

        // The screens' light first: the pages are then drawn over it, unlit by one another.
        let strength = f32::from_bits(self.glow_strength.load(Ordering::Relaxed));
        if strength > 0.0
            && let Ok(glows) = self.glows.lock()
        {
            let color_of = |light: &Glow| {
                let (shown, _) = gpu.colors.get(&light.page)?;
                let color = *shown.get(light.cell)?;
                Some(if is_linear { glow::to_linear(color) } else { color })
            };
            let (lights, count) = glow::pack(&glows, color_of, world.camera.to_array());
            let frame = Frame {
                device: world.device,
                queue: world.queue,
                color: world.color_view,
                depth: world.depth,
                size: world.size,
                depth_size: world.depth_size,
                inverse: world.view_proj.inverse().to_cols_array(),
            };
            gpu.ambilight.draw(world.encoder, &frame, &lights, count, strength);
        }

        // One bind group a page drawn this frame; a page with no picture yet shows the card.
        let groups: Vec<wgpu::BindGroup> = runs
            .iter()
            .filter_map(|run| {
                let page = gpu.pages.get(&run.page).or_else(|| gpu.pages.get(&NO_PAGE))?;
                Some(world.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("webdisplays"),
                    layout: &gpu.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: if see_through.contains(&run.page) { gpu.uniforms_oled.as_entire_binding() } else { gpu.uniforms.as_entire_binding() },
                        },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&page.view) },
                        wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&gpu.sampler) },
                        wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(world.depth) },
                    ],
                }))
            })
            .collect();
        if groups.len() != runs.len() {
            return;
        }
        let flat_groups: Vec<wgpu::BindGroup> = flat_runs
            .iter()
            .filter_map(|run| {
                let page = gpu.pages.get(&run.page).or_else(|| gpu.pages.get(&NO_PAGE))?;
                Some(world.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("webdisplays overlay"),
                    layout: &gpu.layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: gpu.uniforms_flat.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&page.view) },
                        wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&gpu.sampler) },
                        wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(world.depth) },
                    ],
                }))
            })
            .collect();
        if flat_groups.len() != flat_runs.len() {
            return;
        }
        let mut pass = world.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("webdisplays pages"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: world.color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&gpu.pipeline);
        pass.set_vertex_buffer(0, gpu.vertices.slice(..bytes));
        let mut first = 0;
        for (run, group) in runs.iter().zip(&groups) {
            let count = run.quads * QUAD_VERTICES;
            pass.set_bind_group(0, group, &[]);
            pass.draw(first..first + count, 0..1);
            first += count;
        }
        let mut first = world_vertices;
        for (run, group) in flat_runs.iter().zip(&flat_groups) {
            let count = run.quads * QUAD_VERTICES;
            pass.set_bind_group(0, group, &[]);
            pass.draw(first..first + count, 0..1);
            first += count;
        }
    }
}
