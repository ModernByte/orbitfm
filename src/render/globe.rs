//! GPU-ресурсы глобуса: UV-сфера, текстура Земли и wgpu-пайплайн.

use eframe::egui::{self, epaint::PaintCallbackInfo};
use egui_wgpu::{CallbackResources, CallbackTrait, ScreenDescriptor, wgpu};
use wgpu::util::DeviceExt;

use super::GlobeCamera;
use super::camera::CameraUniform;
use super::motion::GlobeMotion;

const GLOBE_SHADER: &str = include_str!("globe.wgsl");
const EARTH_TEXTURE: &[u8] = include_bytes!("../../assets/textures/earth.jpg");

const SPHERE_SEGMENTS: u32 = 64;
const SPHERE_STACKS: u32 = 32;

/// GPU-ресурсы рендерера глобуса.
///
/// Живёт в `CallbackResources` egui-wgpu: создаётся один раз при старте
/// и переиспользуется всеми кадрами.
pub struct GlobeRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl GlobeRenderer {
    /// Создаёт пайплайн, буферы и текстуру Земли.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("globe shader"),
            source: wgpu::ShaderSource::Wgsl(GLOBE_SHADER.into()),
        });

        let (vertices, indices) = sphere_mesh();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("globe vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("globe indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globe camera uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let texture = earth_texture(device, queue);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("earth sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globe bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globe bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
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
            label: Some("globe pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("globe pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(GlobeVertex::layout())],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            uniform_buffer,
            bind_group,
        }
    }

    /// Загружает униформ-данные камеры в GPU-буфер.
    pub fn prepare(&self, queue: &wgpu::Queue, uniform: &CameraUniform) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniform));
    }

    /// Рисует сферу в текущий render pass.
    pub fn paint(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

/// Кадровый callback для egui-wgpu.
pub struct GlobeCallback {
    uniform: CameraUniform,
}

impl GlobeCallback {
    /// Снимает текущее состояние камеры и движения для отрисовки кадра.
    pub fn new(camera: &GlobeCamera, aspect: f32, motion: &GlobeMotion) -> Self {
        Self {
            uniform: camera.uniform_with_motion(aspect, motion),
        }
    }
}

impl CallbackTrait for GlobeCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(renderer) = callback_resources.get::<GlobeRenderer>() {
            renderer.prepare(queue, &self.uniform);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        if let Some(renderer) = callback_resources.get::<GlobeRenderer>() {
            renderer.paint(render_pass);
        }
    }
}

/// Добавляет callback с глобусом в painter.
pub fn paint_globe(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &GlobeCamera,
    motion: &GlobeMotion,
) {
    let callback = GlobeCallback::new(camera, rect.aspect_ratio(), motion);
    painter.add(egui_wgpu::Callback::new_paint_callback(rect, callback));
}

fn earth_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
    let image = image::load_from_memory(EARTH_TEXTURE)
        .expect("встроенная текстура Земли должна декодироваться")
        .to_rgba8();
    let (width, height) = image.dimensions();
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("earth texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
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
        image.as_raw(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        size,
    );

    texture
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlobeVertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

impl GlobeVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

fn sphere_mesh() -> (Vec<GlobeVertex>, Vec<u32>) {
    let row = SPHERE_SEGMENTS + 1;
    let mut vertices = Vec::with_capacity((row * (SPHERE_STACKS + 1)) as usize);

    for stack in 0..=SPHERE_STACKS {
        let v = stack as f32 / SPHERE_STACKS as f32;
        let lat = (0.5 - v) * std::f32::consts::PI;
        let (sin_lat, cos_lat) = lat.sin_cos();

        for segment in 0..=SPHERE_SEGMENTS {
            let u = segment as f32 / SPHERE_SEGMENTS as f32;
            let lon = (u * 2.0 - 1.0) * std::f32::consts::PI;
            let (sin_lon, cos_lon) = lon.sin_cos();
            let position = [cos_lat * sin_lon, sin_lat, cos_lat * cos_lon];

            vertices.push(GlobeVertex {
                position,
                normal: position,
                uv: [u, v],
            });
        }
    }

    let mut indices = Vec::with_capacity((SPHERE_SEGMENTS * SPHERE_STACKS * 6) as usize);
    for stack in 0..SPHERE_STACKS {
        for segment in 0..SPHERE_SEGMENTS {
            let a = stack * row + segment;
            let b = (stack + 1) * row + segment;
            let c = a + 1;
            let d = b + 1;
            indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;

    #[test]
    fn sphere_mesh_faces_outward() {
        let (vertices, indices) = sphere_mesh();

        assert_eq!(
            vertices.len(),
            ((SPHERE_SEGMENTS + 1) * (SPHERE_STACKS + 1)) as usize
        );
        assert_eq!(
            indices.len(),
            (SPHERE_SEGMENTS * SPHERE_STACKS * 6) as usize
        );

        for triangle in indices.as_chunks::<3>().0 {
            let a = Vec3::from(vertices[triangle[0] as usize].position);
            let b = Vec3::from(vertices[triangle[1] as usize].position);
            let c = Vec3::from(vertices[triangle[2] as usize].position);
            let normal = (b - a).cross(c - a);
            let centroid = (a + b + c) / 3.0;

            if normal.length_squared() > 1e-8 {
                assert!(normal.dot(centroid) > 0.0);
            }
        }
    }

    #[test]
    fn vertices_are_on_unit_sphere() {
        let (vertices, _) = sphere_mesh();

        for vertex in &vertices {
            assert!((Vec3::from(vertex.position).length() - 1.0).abs() < 1e-5);
            assert!((0.0..=1.0).contains(&vertex.uv[0]));
            assert!((0.0..=1.0).contains(&vertex.uv[1]));
        }
    }
}
