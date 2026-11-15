//! Lays the screens' light over the frame (`render/glow.rs` says which lights).
//!
//! One triangle covers the picture. For each pixel the shader finds where in the world it
//! is, from the world's depth, and adds what each light sends there: nearer is brighter,
//! what faces the light is brighter, and a screen lights only what is in front of it.
//! Nothing is known of the surface but where it is, so the light is laid on twice: once
//! multiplied by what is already there (a red wall stays red under white light), once
//! faintly added (so that a wall in the dark shows at all).
//!
//! The cost is the frame's pixels times the lights, whatever the number of screens: there
//! is no per-light geometry, and no limit to raise.
use super::glow::{LIGHT, MAX_LIGHTS};
use horizon_client::mod_api::wgpu;

/// Bytes of the uniforms: the inverse view-projection, two parameter vectors, the lights.
const UNIFORMS: usize = 64 + 16 + 16 + MAX_LIGHTS * LIGHT;
/// How much of the light is added whatever is underneath.
const ADDED: f64 = 0.18;

const SHADER: &str = r#"
struct Light {
    // xyz: where, relative to the camera; w: how far it shines.
    at: vec4<f32>,
    facing: vec4<f32>,
    color: vec4<f32>,
};
struct Uniforms {
    inverse: mat4x4<f32>,
    // xy: one over the picture's size; z: how many lights; w: strength.
    params: vec4<f32>,
    // xy: picture pixels to depth pixels.
    scale: vec4<f32>,
    lights: array<Light, 32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var scene: texture_depth_2d;

@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle over the whole picture.
    let x = f32(i32(index & 1u) * 4 - 1);
    let y = f32(i32(index >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment fn fs(@builtin(position) clip: vec4<f32>) -> @location(0) vec4<f32> {
    let depth = textureLoad(scene, vec2<i32>(clip.xy * u.scale.xy), 0);
    // The world's depth is reversed: 0 is the sky, which nothing lights.
    if (depth <= 0.0) {
        discard;
    }
    let ndc = vec2<f32>(clip.x * u.params.x * 2.0 - 1.0, 1.0 - clip.y * u.params.y * 2.0);
    let far = u.inverse * vec4<f32>(ndc, depth, 1.0);
    let here = far.xyz / far.w;
    // Which way the surface faces, from how its position changes across the picture; it
    // faces the camera, or it would not be seen.
    var normal = normalize(cross(dpdx(here), dpdy(here)));
    if (dot(normal, here) > 0.0) {
        normal = -normal;
    }
    var sum = vec3<f32>(0.0);
    let count = i32(u.params.z);
    for (var i = 0; i < count; i++) {
        let light = u.lights[i];
        let to_light = light.at.xyz - here;
        let away = length(to_light);
        if (away >= light.at.w) {
            continue;
        }
        let toward = to_light / max(away, 0.0001);
        let fall = 1.0 - away / light.at.w;
        // Only what is in front of the screen, more so straight ahead of it.
        let ahead = max(dot(light.facing.xyz, -toward), 0.0);
        let lit = max(dot(normal, toward), 0.0) * 0.75 + 0.25;
        sum += light.color.rgb * (fall * fall * sqrt(ahead) * lit);
    }
    return vec4<f32>(sum * u.params.w, 1.0);
}
"#;

pub struct Ambilight {
    /// Multiplies what is there by the light, and adds it back: `frame * (1 + light)`.
    tint: wgpu::RenderPipeline,
    /// Adds a fraction of the light.
    add: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
}

/// What a frame's lights are drawn with.
pub struct Frame<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub color: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
    /// The picture's size, and the depth's, in pixels.
    pub size: [u32; 2],
    pub depth_size: [u32; 2],
    /// The inverse of the view-projection, column-major.
    pub inverse: [f32; 16],
}

impl Ambilight {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("webdisplays ambilight"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("webdisplays ambilight"),
            entries: &[
                entry(0, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }),
                entry(
                    1,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("webdisplays ambilight"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |label, src_factor| {
            let blend = wgpu::BlendComponent { src_factor, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState { color: blend, alpha: blend }),
                        // The frame's alpha is the game's.
                        write_mask: wgpu::ColorWrites::COLOR,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("webdisplays ambilight"),
            size: UNIFORMS as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            tint: pipeline("webdisplays ambilight tint", wgpu::BlendFactor::Dst),
            add: pipeline("webdisplays ambilight add", wgpu::BlendFactor::Constant),
            layout,
            uniforms,
        }
    }

    /// Lays `count` lights (`glow::pack`'s bytes) over the frame, `strength` strong.
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, frame: &Frame<'_>, lights: &[u8], count: u32, strength: f32) {
        if count == 0 || strength <= 0.0 || frame.size[0] == 0 || frame.size[1] == 0 || lights.len() != MAX_LIGHTS * LIGHT {
            return;
        }
        let mut bytes = Vec::with_capacity(UNIFORMS);
        let scale = [frame.depth_size[0] as f32 / frame.size[0] as f32, frame.depth_size[1] as f32 / frame.size[1] as f32];
        let params = [1.0 / frame.size[0] as f32, 1.0 / frame.size[1] as f32, count as f32, strength];
        for value in frame.inverse.into_iter().chain(params).chain([scale[0], scale[1], 0.0, 0.0]) {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(lights);
        frame.queue.write_buffer(&self.uniforms, 0, &bytes);
        let group = frame.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("webdisplays ambilight"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.uniforms.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(frame.depth) },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("webdisplays ambilight"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: frame.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_bind_group(0, &group, &[]);
        pass.set_pipeline(&self.tint);
        pass.draw(0..3, 0..1);
        pass.set_blend_constant(wgpu::Color { r: ADDED, g: ADDED, b: ADDED, a: 0.0 });
        pass.set_pipeline(&self.add);
        pass.draw(0..3, 0..1);
    }
}
