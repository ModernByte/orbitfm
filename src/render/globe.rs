//! GPU-ресурсы глобуса: UV-сфера, текстура Земли и wgpu-пайплайн.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, epaint::PaintCallbackInfo};
use egui_wgpu::{CallbackResources, CallbackTrait, ScreenDescriptor, wgpu};
use wgpu::util::DeviceExt;

use super::GlobeCamera;
use super::camera::CameraUniform;
use super::heatmap::{HEAT_SIZE, HeatFrame};
use super::motion::GlobeMotion;

const GLOBE_SHADER: &str = include_str!("globe.wgsl");
const EARTH_TEXTURE: &[u8] = include_bytes!("../../assets/textures/earth.jpg");
/// Версия повышенного разрешения: готовится в фоне при приближении.
const EARTH_TEXTURE_HI: &[u8] = include_bytes!("../../assets/textures/earth@2x.jpg");
/// Гистерезис LOD: детальная текстура включается на 2.6 и остаётся до 2.9 —
/// без него колесо на границе переключало бы текстуры каждый кадр.
const LOD_NEAR_ENTER: f32 = 2.6;
const LOD_NEAR_EXIT: f32 = 2.9;
/// Предзагрузка детального яруса: с этой дистанции кадр заказывает фоновую
/// задачу, чтобы к моменту `LOD_NEAR_ENTER` текстура уже лежала в GPU.
const LOD_PREFETCH: f32 = 3.0;
/// Глубина мипов: дальняя сторона сферы и мелкий силуэт сэмплируются с
/// уменьшением, иначе на континентах появляется муар.
const LOD_MAX_CLAMP: f32 = 32.0;
/// Анизотропия для наклонных участков сферы; wgpu сам срежет её до 1, если
/// устройство не поддерживает (`DownlevelFlags::ANISOTROPIC_FILTERING`).
const ANISOTROPY: u16 = 8;

const SPHERE_SEGMENTS: u32 = 64;
const SPHERE_STACKS: u32 = 32;

/// Пиксели текстуры Земли, готовые к загрузке в GPU: уровень 0 и его мипы.
///
/// Готовится либо сразу (базовый ярус, нужен первому кадру), либо в отдельном
/// потоке (детальный ярус): кадр не декодирует текстуры и не строит мипы
/// (ADR-0059 § 2).
#[derive(Debug)]
struct EarthTextureData {
    width: u32,
    height: u32,
    /// `levels[0]` — уровень 0, дальше 2×2-усреднения до 1×1.
    levels: Vec<Vec<u8>>,
}

impl EarthTextureData {
    /// Декодирует JPEG и строит полную цепочку мипов усреднением 2×2.
    fn decode(bytes: &[u8]) -> Option<Self> {
        let base = image::load_from_memory(bytes).ok()?.into_rgba8();
        let (width, height) = base.dimensions();
        let mut levels = Vec::with_capacity(mip_levels(width, height) as usize);
        levels.push(base.into_raw());
        let (mut level_width, mut level_height) = (width, height);
        while level_width > 1 || level_height > 1 {
            let (next, next_width, next_height) = downsample(
                levels.last().expect("уровень 0 только что добавлен"),
                level_width,
                level_height,
            );
            levels.push(next);
            level_width = next_width;
            level_height = next_height;
        }
        Some(Self {
            width,
            height,
            levels,
        })
    }
}

/// Усреднение блоков 2×2 — корректный мип-фильтр. `Lanczos3` (было до ADR-0059)
/// стоит 95 мс на 2048×1024 и 452 мс на 5400×2700 против 10 и 60 мс здесь.
fn downsample(source: &[u8], width: u32, height: u32) -> (Vec<u8>, u32, u32) {
    let next_width = (width / 2).max(1);
    let next_height = (height / 2).max(1);
    let mut target = vec![0u8; (next_width * next_height * 4) as usize];

    for y in 0..next_height {
        let top = (y * 2).min(height - 1);
        let bottom = (y * 2 + 1).min(height - 1);
        for x in 0..next_width {
            let left = (x * 2).min(width - 1);
            let right = (x * 2 + 1).min(width - 1);
            let mut sum = [0u32; 4];
            for row in [top, bottom] {
                for column in [left, right] {
                    let index = ((row * width + column) * 4) as usize;
                    for channel in 0..4 {
                        sum[channel] += u32::from(source[index + channel]);
                    }
                }
            }
            let index = ((y * next_width + x) * 4) as usize;
            for channel in 0..4 {
                // Крайние блоки вырожденного размера добираются повтором
                // пикселя (clamp-to-edge), поэтому делитель всегда 4.
                target[index + channel] = (sum[channel] / 4) as u8;
            }
        }
    }
    (target, next_width, next_height)
}

/// Итог фоновой подготовки детального яруса.
#[derive(Debug)]
enum LodOutcome {
    Ready(wgpu::Texture),
    /// Декодирование не удалось: повторять его каждый кадр бессмысленно.
    Failed,
}

/// Детальный ярус участвует в кадре, только когда он готов и камера внутри
/// порога; `paint` выбирает bind group по тому же правилу.
fn near_texture_in_use(lod: &LodState, near_active: bool) -> bool {
    near_active && matches!(lod, LodState::Ready(_))
}

/// Состояние детального яруса текстуры: `Idle` — ещё не понадобился,
/// `Preparing` — текстуру готовит фоновый поток, `Ready` — лежит в GPU.
///
/// Декодирование, мипы и загрузка в GPU идут в рабочем потоке: `wgpu::Device`
/// и `wgpu::Queue` — многопоточные handle'ы, поэтому кадру остаётся только
/// создать view и bind group (ADR-0059 § 2).
#[derive(Debug)]
enum LodState {
    Idle,
    /// Слот результата и handle потока: handle нужен только чтобы отличить
    /// «ещё готовит» от «поток умер, ничего не записав» (паника в декодере
    /// или загрузке). Без него состояние зависло бы в `Preparing` навсегда.
    Preparing {
        slot: Arc<Mutex<Option<LodOutcome>>>,
        worker: std::thread::JoinHandle<()>,
    },
    Ready(wgpu::BindGroup),
    Failed,
}

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
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
    /// Детальная текстура создаётся один раз, когда её пиксели готовы.
    near_active: bool,
    /// Тепловая карта стран: текстура фиксированного размера, содержимое
    /// обновляется только при смене выборки или режима.
    heat_texture: wgpu::Texture,
    heat_view: wgpu::TextureView,
    uploaded_heat: Option<Arc<HeatFrame>>,
    lod: LodState,
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

        let data = EarthTextureData::decode(EARTH_TEXTURE)
            .expect("встроенная текстура Земли должна декодироваться");
        let texture = earth_texture(device, queue, &data, "earth texture")
            .expect("базовая текстура Земли должна загружаться");
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("earth sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: LOD_MAX_CLAMP,
            compare: None,
            anisotropy_clamp: ANISOTROPY,
            border_color: None,
        });

        let heat_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("country heat"),
            size: wgpu::Extent3d {
                width: HEAT_SIZE[0],
                height: HEAT_SIZE[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let heat_view = heat_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = earth_bind_group(
            device,
            &texture_layout,
            &uniform_buffer,
            &texture_view,
            &sampler,
            &heat_view,
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("globe pipeline layout"),
            bind_group_layouts: &[Some(&texture_layout)],
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
            texture_layout,
            sampler,
            bind_group,
            near_active: false,
            heat_texture,
            heat_view,
            uploaded_heat: None,
            lod: LodState::Idle,
        }
    }

    /// Загружает униформ-данные камеры, заказывает и подхватывает детальный
    /// ярус текстуры. Ни одна операция здесь не ждёт фоновую задачу: пока её
    /// результат не готов, рисуется базовая текстура (ADR-0059 § 2).
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uniform: &CameraUniform,
        heat: Option<&Arc<HeatFrame>>,
    ) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniform));

        if let Some(frame) = heat
            && !self
                .uploaded_heat
                .as_ref()
                .is_some_and(|uploaded| Arc::ptr_eq(uploaded, frame))
        {
            debug_assert_eq!(
                frame.size, HEAT_SIZE,
                "текстура карты фиксированного размера"
            );
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.heat_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &frame.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * frame.size[0]),
                    rows_per_image: Some(frame.size[1]),
                },
                wgpu::Extent3d {
                    width: frame.size[0],
                    height: frame.size[1],
                    depth_or_array_layers: 1,
                },
            );
            self.uploaded_heat = Some(Arc::clone(frame));
        }

        let eye = glam::Vec3::from_slice(&uniform.camera_position[..3]);
        let distance = eye.length();
        if distance <= LOD_PREFETCH {
            self.start_lod_job(device, queue);
        }
        self.take_lod_result(device);
        // Гистерезис: войти в детальный LOD на 2.6, выйти — только после 2.9.
        let near = if self.near_active {
            distance <= LOD_NEAR_EXIT
        } else {
            distance <= LOD_NEAR_ENTER
        };
        self.near_active = near_texture_in_use(&self.lod, near);
    }

    /// Ставит фоновую задачу подготовки детального яруса, если она ещё не
    /// поставлена и не провалилась.
    fn start_lod_job(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if matches!(self.lod, LodState::Idle) {
            let slot: Arc<Mutex<Option<LodOutcome>>> = Arc::new(Mutex::new(None));
            let worker = Arc::clone(&slot);
            let device = device.clone();
            let queue = queue.clone();
            // Отдельный поток: декодирование 5400×2700, мипы (~150 мс) и
            // загрузка 78 МБ в GPU — не работа кадра (ADR-0059 § 2).
            let spawned = std::thread::Builder::new()
                .name("orbitfm-lod".to_owned())
                .spawn(move || {
                    let outcome = match EarthTextureData::decode(EARTH_TEXTURE_HI) {
                        Some(data) => {
                            let limit = device.limits().max_texture_dimension_2d;
                            if data.width > limit || data.height > limit {
                                tracing::warn!(
                                    width = data.width,
                                    height = data.height,
                                    limit,
                                    "orbitfm render: детальный ярус больше предела устройства"
                                );
                                LodOutcome::Failed
                            } else {
                                earth_texture(&device, &queue, &data, "earth texture hi")
                                    .map_or(LodOutcome::Failed, LodOutcome::Ready)
                            }
                        }
                        None => LodOutcome::Failed,
                    };
                    // Блокировка короткая: кадр берёт её через `try_lock`.
                    if let Ok(mut slot) = worker.lock() {
                        *slot = Some(outcome);
                    }
                });
            match spawned {
                Ok(worker) => self.lod = LodState::Preparing { slot, worker },
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        "orbitfm render: поток подготовки детальной текстуры не запустился"
                    );
                    self.lod = LodState::Failed;
                }
            }
        }
    }

    /// Забирает готовую текстуру у фоновой задачи и подключает её к пайплайну.
    fn take_lod_result(&mut self, device: &wgpu::Device) {
        let LodState::Preparing { slot, worker } = &self.lod else {
            return;
        };
        // `try_lock`: если поток ещё пишет результат, кадр этого не ждёт.
        let outcome = match slot.try_lock() {
            Ok(mut slot) => slot.take(),
            Err(std::sync::TryLockError::WouldBlock) => return,
            // Поток паниковал с захваченной блокировкой: яруса не будет.
            Err(std::sync::TryLockError::Poisoned(_)) => None,
        };
        // Поток уже завершился, но ничего не записал — паника до записи
        // (декодер, мипы или загрузка в GPU). Ждать больше нечего.
        let worker_died = outcome.is_none() && worker.is_finished();
        let texture = match outcome {
            Some(LodOutcome::Ready(texture)) => texture,
            Some(LodOutcome::Failed) => {
                tracing::warn!("orbitfm render: детальная текстура Земли не декодировалась");
                self.lod = LodState::Failed;
                return;
            }
            None if worker_died => {
                tracing::warn!(
                    "orbitfm render: поток подготовки детальной текстуры завершился с паникой"
                );
                self.lod = LodState::Failed;
                return;
            }
            None => return,
        };
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.lod = LodState::Ready(earth_bind_group(
            device,
            &self.texture_layout,
            &self.uniform_buffer,
            &view,
            &self.sampler,
            &self.heat_view,
        ));
    }

    /// Рисует сферу в текущий render pass.
    pub fn paint(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_pipeline(&self.pipeline);
        let bind_group = match &self.lod {
            LodState::Ready(near) if self.near_active => near,
            _ => &self.bind_group,
        };
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

/// Кадровый callback для egui-wgpu.
pub struct GlobeCallback {
    uniform: CameraUniform,
    heat: Option<Arc<HeatFrame>>,
}

impl GlobeCallback {
    /// Снимает текущее состояние камеры и движения для отрисовки кадра.
    pub fn new(
        camera: &GlobeCamera,
        aspect: f32,
        motion: &GlobeMotion,
        heat: Option<&Arc<HeatFrame>>,
    ) -> Self {
        let mut uniform = camera.uniform_with_motion(aspect, motion);
        uniform.heat[0] = if heat.is_some() { 1.0 } else { 0.0 };
        Self {
            uniform,
            heat: heat.cloned(),
        }
    }
}

impl CallbackTrait for GlobeCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(renderer) = callback_resources.get_mut::<GlobeRenderer>() {
            renderer.prepare(device, queue, &self.uniform, self.heat.as_ref());
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
    heat: Option<&Arc<HeatFrame>>,
) {
    let callback = GlobeCallback::new(camera, rect.aspect_ratio(), motion, heat);
    painter.add(egui_wgpu::Callback::new_paint_callback(rect, callback));
}

fn earth_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    data: &EarthTextureData,
    label: &str,
) -> Option<wgpu::Texture> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: data.width,
            height: data.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: data.levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let (mut level_width, mut level_height) = (data.width, data.height);
    for (mip, level) in data.levels.iter().enumerate() {
        if mip > 0 {
            level_width = (level_width / 2).max(1);
            level_height = (level_height / 2).max(1);
        }
        if !upload_level(
            device,
            queue,
            &texture,
            mip as u32,
            level,
            level_width,
            level_height,
        ) {
            return None;
        }
    }

    Some(texture)
}

/// Заливает уровень мипа командой копирования из staging-буфера.
///
/// `Queue::write_texture` для этого не годится: он держит общую блокировку
/// очереди всё время копирования в staging-буфер, а кадр берёт её же в
/// `write_buffer` юниформа — один уровень 58 МБ задержал бы кадр на
/// миллисекунды (измерено 4,1 мс). Здесь под блокировкой только короткий
/// `submit`, а копирование идёт в отображённый буфер без неё (ADR-0059 § 2/§ 4).
fn upload_level(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    mip_level: u32,
    level: &[u8],
    width: u32,
    height: u32,
) -> bool {
    // Копирование буфер → текстура требует выравнивания строки на 256 байт.
    let row_bytes = 4 * width as usize;
    let stride = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize);
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("earth texture staging"),
        size: (stride * height as usize) as u64,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::MAP_WRITE,
        mapped_at_creation: true,
    });
    let Ok(mut mapped) = staging.slice(..).get_mapped_range_mut() else {
        tracing::warn!("orbitfm render: staging-буфер текстуры не отобразился");
        return false;
    };
    if stride == row_bytes {
        mapped.slice(..).copy_from_slice(level);
    } else {
        for row in 0..height as usize {
            let source = &level[row * row_bytes..(row + 1) * row_bytes];
            mapped
                .slice(row * stride..row * stride + row_bytes)
                .copy_from_slice(source);
        }
    }
    drop(mapped);
    staging.unmap();

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("earth texture upload"),
    });
    encoder.copy_buffer_to_texture(
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride as u32),
                rows_per_image: Some(height),
            },
        },
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    true
}

/// Число мипов до 1×1 включительно: 2048 → 12, 5400 → 13.
fn mip_levels(width: u32, height: u32) -> u32 {
    32 - width.max(height).leading_zeros()
}

fn earth_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    texture_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    heat_view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("globe bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(heat_view),
            },
        ],
    })
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
    use std::time::{Duration, Instant};

    use super::super::GlobeInput;

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

    #[test]
    fn downsample_halves_and_averages() {
        // Шахматка 2×2: каждый канал усредняется до середины диапазона.
        let source = vec![
            0, 0, 0, 10, 200, 200, 200, 20, 200, 200, 200, 30, 0, 0, 0, 40,
        ];
        let (level, width, height) = downsample(&source, 2, 2);

        assert_eq!((width, height), (1, 1));
        assert_eq!(level, vec![100, 100, 100, 25]);
    }

    #[test]
    fn downsample_uses_the_spec_mip_size_and_repeats_degenerate_pixels() {
        // Нечётная ширина: размер уровня — floor(3/2) = 1 (как у мип-цепочки),
        // третий пиксель в усреднение не попадает.
        let source = vec![0, 0, 0, 0, 40, 40, 40, 40, 200, 200, 200, 200];
        let (level, width, height) = downsample(&source, 3, 1);

        assert_eq!((width, height), (1, 1));
        assert_eq!(level, vec![20, 20, 20, 20]);

        // Вырожденное ребро 1×1: блок добирается повтором того же пикселя.
        let (single, width, height) = downsample(&[7, 8, 9, 10], 1, 1);
        assert_eq!((width, height), (1, 1));
        assert_eq!(single, vec![7, 8, 9, 10]);
    }

    #[test]
    fn earth_texture_data_builds_a_full_mip_chain() {
        let data = EarthTextureData::decode(EARTH_TEXTURE).expect("встроенная текстура");

        assert_eq!(
            data.levels.len() as u32,
            mip_levels(data.width, data.height),
            "цепочка мипов строится до 1×1"
        );
        assert_eq!(
            data.levels[0].len(),
            (data.width * data.height * 4) as usize
        );
        assert_eq!(
            data.levels.last().map(Vec::len),
            Some(4),
            "последний уровень — 1×1 RGBA"
        );

        let mut level_width = data.width;
        let mut level_height = data.height;
        for (mip, level) in data.levels.iter().enumerate().skip(1) {
            level_width = (level_width / 2).max(1);
            level_height = (level_height / 2).max(1);
            assert_eq!(
                level.len(),
                (level_width * level_height * 4) as usize,
                "уровень {mip} должен быть {level_width}×{level_height}"
            );
        }
    }

    #[test]
    fn near_tier_is_used_only_when_ready_and_within_the_threshold() {
        // Правило выбора bind group: готовность яруса и порог камеры — оба
        // условия, поэтому неготовый и проваленный ярус кадр не ломают.
        assert!(!near_texture_in_use(&LodState::Idle, true));
        assert!(!near_texture_in_use(&LodState::Failed, true));
        assert!(!near_texture_in_use(&LodState::Idle, false));
        assert!(!near_texture_in_use(&LodState::Failed, false));
    }

    /// Ручной замер ADR-0059 § 2 и § 4: переход через порог LOD не должен
    /// стоить кадру ничего. Нужен GPU, поэтому из обычного прогона исключён —
    /// запускается вручную: `cargo test --release lod_crossing -- --ignored --nocapture`.
    #[test]
    #[ignore = "ручной замер: нужен GPU"]
    fn lod_crossing_never_stalls_the_frame() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio");
        let Some((device, queue)) = runtime.block_on(async {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;
            let adapter = adapters.into_iter().max_by_key(|adapter| {
                matches!(
                    adapter.get_info().device_type,
                    wgpu::DeviceType::DiscreteGpu
                )
            })?;
            adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .ok()
        }) else {
            println!("LOD: GPU не найден, замер пропущен");
            return;
        };

        let decode = Instant::now();
        let base = EarthTextureData::decode(EARTH_TEXTURE).expect("базовая текстура");
        println!(
            "LOD: декодирование и мипы базового яруса — {:.0} мс",
            decode.elapsed().as_secs_f64() * 1e3
        );
        drop(base);

        let start = Instant::now();
        let mut renderer = GlobeRenderer::new(&device, &queue, wgpu::TextureFormat::Bgra8Unorm);
        println!(
            "LOD: базовый ярус и пайплайн — {:.0} мс",
            start.elapsed().as_secs_f64() * 1e3
        );
        let mut camera = GlobeCamera::default();
        let motion = GlobeMotion::default();
        let mut frame_times = Vec::new();
        let mut worst_at = Duration::ZERO;
        let mut frames = 0u32;
        let mut during_prepare = 0u32;
        let start = Instant::now();
        let deadline = start + Duration::from_secs(10);
        let mut ready = None;
        // Первые 120 кадров приближают камеру через оба порога (3.0 и 2.6),
        // дальше кадры идут в темпе 60 Гц, пока ярус готовится: загрузка 78 МБ
        // в GPU идёт параллельно и не должна отнимать у кадра больше бюджета
        // ADR-0059 § 4. Замер охватывает и окно загрузки яруса в GPU, а не
        // только первые быстрые кадры.
        while Instant::now() < deadline {
            if frames < 120 {
                camera.advance(
                    0.016,
                    0.016,
                    &GlobeInput {
                        scroll_delta: 3.0,
                        viewport_height: 800.0,
                        ..GlobeInput::default()
                    },
                    &motion,
                    false,
                    false,
                );
            }
            let uniform = camera.uniform_with_motion(1.0, &motion);
            let preparing = matches!(renderer.lod, LodState::Preparing { .. });
            let at = start.elapsed();
            let frame = Instant::now();
            renderer.prepare(&device, &queue, &uniform, None);
            let elapsed = frame.elapsed();
            if frame_times
                .iter()
                .max()
                .is_none_or(|worst| elapsed > *worst)
            {
                worst_at = at;
            }
            frame_times.push(elapsed);
            frames += 1;
            if preparing {
                during_prepare += 1;
            }
            if matches!(renderer.lod, LodState::Ready(_)) {
                ready = Some(Instant::now());
                break;
            }
            // Темп 60 Гц: загрузка идёт в фоне, кадр не должен её ждать.
            std::thread::sleep(Duration::from_millis(4));
        }
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .expect("poll");
        let ready_at = ready.expect("детальный ярус должен быть готов");
        // Стена времени кадра включает вытеснение планировщиком ОС (на машине
        // идёт обычная работа), поэтому бюджет проверяется по медиане и p95:
        // систематическое ожидание текстуры подняло бы именно их, а не один
        // выброс. Худший кадр печатается для истории замеров.
        frame_times.sort_unstable();
        let median = frame_times[frame_times.len() / 2];
        let p95 = frame_times[frame_times.len() * 95 / 100];
        let worst = *frame_times.last().expect("кадры были");
        println!(
            "LOD: {frames} кадров, {during_prepare} из них во время подготовки; \
             медиана {:.2} мс, p95 {:.2} мс, худший {:.2} мс на {:.0} мс; \
             ярус готов через {:.0} мс",
            median.as_secs_f64() * 1e3,
            p95.as_secs_f64() * 1e3,
            worst.as_secs_f64() * 1e3,
            worst_at.as_secs_f64() * 1e3,
            ready_at.duration_since(start).as_secs_f64() * 1e3
        );
        assert!(during_prepare > 0, "замер не попал в окно подготовки яруса");
        assert!(
            median < Duration::from_micros(500),
            "типичный кадр не должен ждать подготовку текстуры: {median:?}"
        );
        assert!(
            p95 < Duration::from_millis(2),
            "бюджет кадра ADR-0059 § 4: {p95:?}"
        );
    }
}
