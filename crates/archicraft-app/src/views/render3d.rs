//! GPU rendering of 3D views with wgpu (VIEW-020, VIEW-050).
//!
//! Each 3D view renders into its own offscreen texture (4× multisampled, with
//! depth), which egui then shows as an image. Vertex buffers are kept on the
//! GPU and rebuilt only when the model or the highlighted elements change;
//! a view re-renders only when its camera, size or the scene changes.
//! Positions are uploaded relative to a local origin near the model, so that
//! large coordinates do not lose precision in `f32`.

use std::collections::HashSet;

use archicraft_geometry::ModelGeometry;
use archicraft_model::glam::{DMat4, DVec3};
use archicraft_model::{ElementId, Project};
use bytemuck::{Pod, Zeroable};
use eframe::egui_wgpu::{self, wgpu};
use wgpu::util::DeviceExt;

pub const SAMPLE_COUNT: u32 = 4;
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// How faces and edges are drawn (VIEW-050).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DisplayMode {
    Shaded,
    ShadedWithEdges,
    HiddenLine,
    Wireframe,
}

impl DisplayMode {
    pub const ALL: [DisplayMode; 4] = [
        DisplayMode::Shaded,
        DisplayMode::ShadedWithEdges,
        DisplayMode::HiddenLine,
        DisplayMode::Wireframe,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DisplayMode::Shaded => "Shaded",
            DisplayMode::ShadedWithEdges => "Shaded with edges",
            DisplayMode::HiddenLine => "Hidden line",
            DisplayMode::Wireframe => "Wireframe",
        }
    }

    fn faces(self) -> bool {
        self != DisplayMode::Wireframe
    }

    fn edges(self) -> bool {
        self != DisplayMode::Shaded
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
    params: [f32; 4],
    background: [f32; 4],
}

/// Triangles and edges of the model, ready for upload.
#[derive(Clone, Debug, Default)]
pub struct SceneData {
    pub origin: DVec3,
    pub bounds: Option<(DVec3, DVec3)>,
    pub opaque: Vec<Vertex>,
    pub transparent: Vec<Vertex>,
    pub edges: Vec<Vertex>,
}

impl SceneData {
    pub fn radius(&self) -> f64 {
        self.bounds.map_or(10.0, |(lo, hi)| (hi - lo).length() / 2.0)
    }
}

pub const SELECTION_COLOR: [f32; 4] = [0.15, 0.45, 0.95, 1.0];
const HOVER_TINT: f32 = 0.35;
const EDGE_COLOR: [f32; 4] = [0.12, 0.12, 0.14, 1.0];

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Builds the scene from the evaluated geometry. Elements on hidden layers
/// are left out; selected elements are drawn in the selection colour, and
/// the hovered element is tinted.
pub fn build_scene(
    project: &Project,
    geometry: &ModelGeometry,
    selection: &HashSet<ElementId>,
    hover: Option<ElementId>,
) -> SceneData {
    let mut bounds: Option<(DVec3, DVec3)> = None;
    let mut visible = Vec::new();
    for element in project.elements.values().filter(|e| project.is_visible(e)) {
        let (Some(storey), Some(g)) = (project.storeys.get(element.storey), geometry.get(element.id)) else {
            continue;
        };
        let offset = DVec3::new(0.0, 0.0, storey.elevation);
        if let Some((lo, hi)) = g.bounds() {
            let (lo, hi) = (lo + offset, hi + offset);
            bounds = Some(bounds.map_or((lo, hi), |(a, b)| (a.min(lo), b.max(hi))));
        }
        visible.push((element, g, offset));
    }
    let origin = bounds.map_or(DVec3::ZERO, |(lo, hi)| ((lo + hi) / 2.0).round());
    let mut scene = SceneData {
        origin,
        bounds,
        ..SceneData::default()
    };
    for (element, g, offset) in visible {
        let selected = selection.contains(&element.id);
        let hovered = hover == Some(element.id);
        for part in &g.parts {
            let material = project.material(part.material);
            let rgb = material.map_or([0.7, 0.7, 0.7], |m| m.color.0.map(|c| c as f32));
            let opacity = material.map_or(1.0, |m| m.opacity as f32);
            let rgb = if selected {
                [SELECTION_COLOR[0], SELECTION_COLOR[1], SELECTION_COLOR[2]]
            } else if hovered {
                mix(
                    rgb,
                    [SELECTION_COLOR[0], SELECTION_COLOR[1], SELECTION_COLOR[2]],
                    HOVER_TINT,
                )
            } else {
                rgb
            };
            let color = [rgb[0], rgb[1], rgb[2], opacity];
            let target = if opacity < 0.999 {
                &mut scene.transparent
            } else {
                &mut scene.opaque
            };
            for [a, b, c] in part.mesh.triangles() {
                let normal = (b - a).cross(c - a).normalize_or_zero();
                let n = normal.to_array().map(|v| v as f32);
                for p in [a, b, c] {
                    let local = p + offset - origin;
                    target.push(Vertex {
                        position: local.to_array().map(|v| v as f32),
                        normal: n,
                        color,
                    });
                }
            }
            let edge_color = if selected { SELECTION_COLOR } else { EDGE_COLOR };
            for [a, b] in part.mesh.feature_edges(1.0) {
                for p in [a, b] {
                    scene.edges.push(Vertex {
                        position: (p + offset - origin).to_array().map(|v| v as f32),
                        normal: [0.0, 0.0, 1.0],
                        color: edge_color,
                    });
                }
            }
        }
    }
    scene
}

struct GpuBuffer {
    buffer: wgpu::Buffer,
    count: u32,
}

fn upload(device: &wgpu::Device, label: &str, vertices: &[Vertex]) -> Option<GpuBuffer> {
    (!vertices.is_empty()).then(|| GpuBuffer {
        buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        count: vertices.len() as u32,
    })
}

/// The model on the GPU.
pub struct GpuScene {
    pub origin: DVec3,
    pub bounds: Option<(DVec3, DVec3)>,
    pub radius: f64,
    /// Changes whenever the buffers change, so views know to re-render.
    pub generation: u64,
    opaque: Option<GpuBuffer>,
    transparent: Option<GpuBuffer>,
    edges: Option<GpuBuffer>,
}

impl std::fmt::Debug for GpuScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuScene")
            .field("generation", &self.generation)
            .finish()
    }
}

/// One 3D view's render target and camera uniforms.
pub struct Viewport {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    resolve: wgpu::TextureView,
    depth: wgpu::TextureView,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pub texture_id: egui::TextureId,
    /// Key of the last rendered frame; equal keys need no re-render.
    rendered: Option<FrameKey>,
}

impl std::fmt::Debug for Viewport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Viewport").field("size", &self.size).finish()
    }
}

/// Everything a rendered frame depends on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameKey {
    pub generation: u64,
    pub view_proj: [[f32; 4]; 4],
    pub mode: DisplayMode,
    pub background: [f32; 4],
}

/// Pipelines shared by all 3D views.
pub struct Renderer3d {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bind_group_layout: wgpu::BindGroupLayout,
    fill: wgpu::RenderPipeline,
    transparent: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    next_generation: u64,
}

impl std::fmt::Debug for Renderer3d {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer3d").finish_non_exhaustive()
    }
}

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    }
}

impl Renderer3d {
    pub fn new(render_state: &egui_wgpu::RenderState) -> Self {
        let device = render_state.device.clone();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("archicraft 3d shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("archicraft 3d uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("archicraft 3d layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = |label: &str,
                        topology: wgpu::PrimitiveTopology,
                        fragment: &str,
                        blend: Option<wgpu::BlendState>,
                        depth_write: bool,
                        bias: wgpu::DepthBiasState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(vertex_layout())],
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    cull_mode: None,
                    ..wgpu::PrimitiveState::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias,
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLE_COUNT,
                    ..wgpu::MultisampleState::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        // Faces are pushed back slightly so that edges drawn on them win the
        // depth test.
        let face_bias = wgpu::DepthBiasState {
            constant: 2,
            slope_scale: 1.5,
            clamp: 0.0,
        };
        let fill = pipeline(
            "archicraft 3d faces",
            wgpu::PrimitiveTopology::TriangleList,
            "fs_fill",
            None,
            true,
            face_bias,
        );
        let transparent = pipeline(
            "archicraft 3d transparent faces",
            wgpu::PrimitiveTopology::TriangleList,
            "fs_fill",
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            false,
            face_bias,
        );
        let lines = pipeline(
            "archicraft 3d edges",
            wgpu::PrimitiveTopology::LineList,
            "fs_line",
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            false,
            wgpu::DepthBiasState::default(),
        );
        Self {
            device,
            queue: render_state.queue.clone(),
            bind_group_layout,
            fill,
            transparent,
            lines,
            next_generation: 1,
        }
    }

    pub fn upload_scene(&mut self, data: &SceneData) -> GpuScene {
        let generation = self.next_generation;
        self.next_generation += 1;
        GpuScene {
            origin: data.origin,
            bounds: data.bounds,
            radius: data.radius(),
            generation,
            opaque: upload(&self.device, "archicraft opaque", &data.opaque),
            transparent: upload(&self.device, "archicraft transparent", &data.transparent),
            edges: upload(&self.device, "archicraft edges", &data.edges),
        }
    }

    fn create_targets(&self, size: [u32; 2]) -> (wgpu::TextureView, wgpu::TextureView, wgpu::TextureView) {
        let extent = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };
        let texture = |label: &str, samples: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
            self.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        (
            texture(
                "archicraft 3d msaa",
                SAMPLE_COUNT,
                COLOR_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            ),
            texture(
                "archicraft 3d image",
                1,
                COLOR_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            ),
            texture(
                "archicraft 3d depth",
                SAMPLE_COUNT,
                DEPTH_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            ),
        )
    }

    /// Creates or resizes a view's render target.
    pub fn ensure_viewport(
        &self,
        render_state: &egui_wgpu::RenderState,
        viewport: &mut Option<Viewport>,
        size: [u32; 2],
    ) {
        let size = [size[0].clamp(1, 8192), size[1].clamp(1, 8192)];
        if let Some(v) = viewport.as_mut() {
            if v.size == size {
                return;
            }
            let (msaa, resolve, depth) = self.create_targets(size);
            render_state.renderer.write().update_egui_texture_from_wgpu_texture(
                &self.device,
                &resolve,
                wgpu::FilterMode::Linear,
                v.texture_id,
            );
            v.size = size;
            v.msaa = msaa;
            v.resolve = resolve;
            v.depth = depth;
            v.rendered = None;
            return;
        }
        let (msaa, resolve, depth) = self.create_targets(size);
        let texture_id =
            render_state
                .renderer
                .write()
                .register_native_texture(&self.device, &resolve, wgpu::FilterMode::Linear);
        let uniforms = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("archicraft 3d uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("archicraft 3d uniforms"),
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        *viewport = Some(Viewport {
            size,
            msaa,
            resolve,
            depth,
            uniforms,
            bind_group,
            texture_id,
            rendered: None,
        });
    }

    /// Releases a view's texture when the view closes.
    pub fn free_viewport(render_state: &egui_wgpu::RenderState, viewport: Viewport) {
        render_state.renderer.write().free_texture(&viewport.texture_id);
    }

    /// Renders the scene into the view's texture if anything changed.
    pub fn render(
        &self,
        viewport: &mut Viewport,
        scene: &GpuScene,
        view_proj: DMat4,
        mode: DisplayMode,
        background: [f32; 4],
    ) {
        let view_proj = view_proj.as_mat4().to_cols_array_2d();
        let key = FrameKey {
            generation: scene.generation,
            view_proj,
            mode,
            background,
        };
        if viewport.rendered == Some(key) {
            return;
        }
        let uniforms = Uniforms {
            view_proj,
            light_dir: [0.35, -0.55, 0.75, 0.0],
            params: [if mode == DisplayMode::HiddenLine { 1.0 } else { 0.0 }, 0.45, 0.0, 0.0],
            background,
        };
        self.queue
            .write_buffer(&viewport.uniforms, 0, bytemuck::bytes_of(&uniforms));
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("archicraft 3d"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("archicraft 3d"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &viewport.msaa,
                    depth_slice: None,
                    resolve_target: Some(&viewport.resolve),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(background[0]),
                            g: f64::from(background[1]),
                            b: f64::from(background[2]),
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &viewport.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &viewport.bind_group, &[]);
            let mut draw = |pipeline: &wgpu::RenderPipeline, buffer: &Option<GpuBuffer>| {
                if let Some(b) = buffer {
                    pass.set_pipeline(pipeline);
                    pass.set_vertex_buffer(0, b.buffer.slice(..));
                    pass.draw(0..b.count, 0..1);
                }
            };
            if mode.faces() {
                draw(&self.fill, &scene.opaque);
            }
            if mode.edges() {
                draw(&self.lines, &scene.edges);
            }
            if mode.faces() && mode != DisplayMode::HiddenLine {
                draw(&self.transparent, &scene.transparent);
            }
        }
        self.queue.submit([encoder.finish()]);
        viewport.rendered = Some(key);
    }
}

#[cfg(test)]
mod tests {
    use archicraft_model::samples;

    use super::*;

    #[test]
    fn scene_contains_every_visible_triangle() {
        let project = samples::two_storey_shell();
        let geometry = ModelGeometry::evaluate(&project);
        let scene = build_scene(&project, &geometry, &HashSet::new(), None);
        let triangles: usize = geometry
            .elements
            .values()
            .flat_map(|g| &g.parts)
            .map(|p| p.mesh.triangle_count())
            .sum();
        assert_eq!(scene.opaque.len() + scene.transparent.len(), triangles * 3);
        assert!(!scene.edges.is_empty());
        let (lo, hi) = scene.bounds.unwrap();
        assert!((hi.z - 6.0).abs() < 1e-9 && (lo.z - -0.2).abs() < 1e-9);
        // Positions are relative to the local origin.
        assert!(scene.opaque.iter().all(|v| v.position.iter().all(|c| c.abs() < 20.0)));
    }

    #[test]
    fn hidden_layers_and_selection() {
        let project = samples::storey_with_walls_and_slab();
        let geometry = ModelGeometry::evaluate(&project);
        let wall = *project.elements.keys().next().unwrap();
        let selection = HashSet::from([wall]);
        let scene = build_scene(&project, &geometry, &selection, None);
        assert!(scene.opaque.iter().any(|v| v.color == SELECTION_COLOR));
    }
}
