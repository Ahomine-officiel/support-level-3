//! Renderer wgpu : pipeline monde (instances + matériaux), post-process, UI.
//! Ray tracing optionnel : pré-pass profondeur + passe de rayons (rt.wgsl)
//! contre la scène AABB (rtscene.rs), appliquée dans world.wgsl.

pub mod model;
pub mod rtscene;
pub mod texture;
pub mod ui;
pub use ui::UiOp;

use model::{Model, Vertex};
use sl3_shared::map::MapData;
use std::collections::HashMap;
use std::path::Path;
use winit::window::Window;
use wgpu::util::DeviceExt;

pub const MAX_LIGHTS: usize = 24;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WorldUniform {
    pub view_proj: [[f32; 4]; 4],
    pub cam_pos: [f32; 4],
    pub light_pos: [[f32; 4]; MAX_LIGHTS],
    pub light_col: [[f32; 4]; MAX_LIGHTS],
    pub flash_pos: [f32; 4],
    pub flash_dir: [f32; 4],
    pub misc: [f32; 4],
    pub flash_col: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    pub m: [[f32; 4]; 4],
    pub emissive: [f32; 4],
    pub tint: [f32; 4],
}

pub const INSTANCE_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: std::mem::size_of::<InstanceRaw>() as u64,
    step_mode: wgpu::VertexStepMode::Instance,
    attributes: &[
        wgpu::VertexAttribute { offset: 0, shader_location: 3, format: wgpu::VertexFormat::Float32x4 },
        wgpu::VertexAttribute { offset: 16, shader_location: 4, format: wgpu::VertexFormat::Float32x4 },
        wgpu::VertexAttribute { offset: 32, shader_location: 5, format: wgpu::VertexFormat::Float32x4 },
        wgpu::VertexAttribute { offset: 48, shader_location: 6, format: wgpu::VertexFormat::Float32x4 },
        wgpu::VertexAttribute { offset: 64, shader_location: 7, format: wgpu::VertexFormat::Float32x4 },
        wgpu::VertexAttribute { offset: 80, shader_location: 8, format: wgpu::VertexFormat::Float32x4 },
    ],
};

#[derive(Clone)]
pub struct InstanceData {
    pub model: glam::Mat4,
    pub emissive: [f32; 4],
    pub tint: [f32; 4],
}

impl InstanceData {
    pub fn new(model: glam::Mat4) -> Self {
        InstanceData { model, emissive: [1.0, 1.0, 1.0, 1.0], tint: [1.0, 1.0, 1.0, 1.0] }
    }
    pub fn with_emissive(mut self, e: [f32; 4]) -> Self {
        self.emissive = e;
        self
    }
    pub fn with_tint(mut self, t: [f32; 4]) -> Self {
        self.tint = t;
        self
    }
    fn raw(&self) -> InstanceRaw {
        InstanceRaw {
            m: self.model.to_cols_array_2d(),
            emissive: self.emissive,
            tint: self.tint,
        }
    }
}

/// (fichier texture sans extension, force émissive du matériau).
pub fn material_def(name: &str) -> (String, f32) {
    let strength = match name {
        "led_strip" => 2.0,
        "screen_on" => 0.8,
        "terminal_screen" => 0.9,
        "exit_sign" => 1.4,
        "eyes" => 2.6,
        "light_panel" => 1.25,
        "battery" => 0.35,
        "receipt_paper" => 0.05,
        "poster_a" | "poster_b" | "poster_c" | "poster_d" => 0.04,
        "entity_mask" => 0.03,
        _ => 0.0,
    };
    (name.to_string(), strength)
}

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct PartKey {
    pub model: String,
    pub part: usize,
}

pub struct StaticBatch {
    pub key: PartKey,
    pub buffer: wgpu::Buffer,
    pub count: u32,
}

/// Uniform de la passe de ray tracing (miroir de shaders/rt.wgsl).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RtParams {
    pub inv_vp: [[f32; 4]; 4],
    pub cam_pos: [f32; 4],
    /// x,y : taille tampon RT · z,w : taille texture profondeur.
    pub dims: [f32; 4],
    /// x : boîtes statiques · y : dynamiques · z : lumières · w : rayon AO (m).
    pub counts: [f32; 4],
    /// x : force GI (0 = désactivée) · y : AO appliquée à la lumière directe.
    pub misc: [f32; 4],
}

/// Données par frame nécessaires au ray tracing (None = RT désactivé).
pub struct RtFrame<'a> {
    pub inv_vp: [[f32; 4]; 4],
    pub dyn_boxes: &'a [rtscene::GpuAabb],
}

pub struct StaticBatches {
    pub batches: Vec<StaticBatch>,
    pub total_instances: usize,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    pub window: std::sync::Arc<Window>,
    pub models: HashMap<String, Model>,

    world_pipeline: wgpu::RenderPipeline,
    world_bind0: wgpu::BindGroup,
    world_uniform_buf: wgpu::Buffer,
    material_bind_groups: HashMap<String, wgpu::BindGroup>,

    post_pipeline: wgpu::RenderPipeline,
    post_bind: wgpu::BindGroup,
    post_uniform_buf: wgpu::Buffer,
    offscreen_view: wgpu::TextureView,
    offscreen_tex: wgpu::Texture,
    depth_view: wgpu::TextureView,
    depth_tex: wgpu::Texture,
    /// Échelle de rendu dynamique (1.0 = pleine résolution, 0.45 min) — iGPU friendly.
    render_scale: f32,

    ui_pipeline: wgpu::RenderPipeline,
    ui_bind0: wgpu::BindGroup,
    ui_uniform_buf: wgpu::Buffer,
    pub font_bg: wgpu::BindGroup,
    pub white_bg: wgpu::BindGroup,
    pub font: ui::FontData,
    ui_buf: wgpu::Buffer,

    dyn_scratch: HashMap<PartKey, (wgpu::Buffer, u64)>,

    // ----- Ray tracing optionnel -----
    /// 0 = désactivé, 1 = qualité (ombres + AO), 2 = ultra (+ rebond GI).
    pub rt_mode: u8,
    /// Échelle du tampon RT relativement au tampon monde.
    rt_scale: f32,
    world_bind_layout: wgpu::BindGroupLayout,
    world_pipeline_rt: wgpu::RenderPipeline,
    prepass_pipeline: wgpu::RenderPipeline,
    rt_pipeline: wgpu::RenderPipeline,
    rt_bind_layout: wgpu::BindGroupLayout,
    rt_bind: wgpu::BindGroup,
    rt_params_buf: wgpu::Buffer,
    rt_static_buf: wgpu::Buffer,
    rt_dyn_buf: wgpu::Buffer,
    rt_static_count: u32,
    rt0_tex: wgpu::Texture,
    rt1_tex: wgpu::Texture,
    rt0_view: wgpu::TextureView,
    rt1_view: wgpu::TextureView,
    rt_neutral0: wgpu::TextureView,
    rt_neutral1: wgpu::TextureView,
    /// Nom de l'adaptateur GPU détecté (pour l'auto-configuration RT).
    pub adapter_name: String,
}

impl Renderer {
    pub fn new(window: std::sync::Arc<Window>) -> Renderer {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window.clone())
            .expect("création surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("pas de GPU compatible (Vulkan/Metal/DX12 requis)");
        let adapter_info = adapter.get_info();
        let adapter_name = if adapter_info.name.is_empty() {
            format!("{:?}", adapter_info.backend)
        } else {
            format!("{} ({:?})", adapter_info.name, adapter_info.backend)
        };

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("sl3-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("création device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        // Modèles + matériaux.
        let models = model::load_all_models(&device);
        let mut material_bind_groups = HashMap::new();
        let mat_layout = Self::material_layout(&device);
        for name in [
            "concrete", "concrete_dark", "floor_hall", "floor_corridor", "floor_office",
            "floor_server", "floor_arch", "floor_elec", "ceiling", "metal_dark",
            "rack_front", "server_front", "led_strip", "screen_off", "screen_on",
            "terminal_screen", "desk_wood", "terminal_body", "chair_fabric", "door_metal",
            "door_frame", "shelf_metal", "cardboard", "entity_cloth", "entity_mask",
            "eyes", "tech_vest", "tech_pants", "tech_head", "receipt_paper",
            "breaker_panel", "exit_sign", "battery", "extinguisher", "duct",
            "light_panel", "hazard", "poster_a", "poster_b", "poster_c", "poster_d",
        ] {
            let (file, strength) = material_def(name);
            let path = Path::new("assets/textures").join(format!("{file}.png"));
            let tex = texture::load_png(&device, &queue, &path);
            let mat_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&[1.0f32, 1.0, 1.0, strength]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(name),
                layout: &mat_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Sampler(&Self::world_sampler(&device)) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&tex.view) },
                    wgpu::BindGroupEntry { binding: 2, resource: mat_buf.as_entire_binding() },
                ],
            });
            material_bind_groups.insert(name.to_string(), bg);
        }

        // Uniforms monde.
        let world_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("world-uniform"),
            contents: bytemuck::bytes_of(&WorldUniform {
                view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
                cam_pos: [0.0; 4],
                light_pos: [[0.0; 4]; MAX_LIGHTS],
                light_col: [[0.0; 4]; MAX_LIGHTS],
                flash_pos: [0.0; 4],
                flash_dir: [0.0; 4],
                misc: [0.0; 4],
                flash_col: [0.0; 4],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let world_bind_layout = Self::world_bind_layout(&device);
        // Textures RT neutres (RT désactivé) : AO=1, ombres=1, GI=0 -> rendu identique.
        let n0 = texture::neutral_rt_texture(&device, &queue, "rt-neutral0", [0x3C00, 0x3C00, 0x0000, 0x3C00]);
        let n1 = texture::neutral_rt_texture(&device, &queue, "rt-neutral1", [0x0000, 0x0000, 0x0000, 0x3C00]);
        let rt_neutral0 = n0.view;
        let rt_neutral1 = n1.view;
        let world_bind0 = Self::make_world_bind0(&device, &world_bind_layout, &world_uniform_buf, &rt_neutral0, &rt_neutral1);

        let world_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/world.wgsl").into()),
        });
        let world_pipeline = Self::world_pipeline_with(
            &device, &world_bind_layout, &mat_layout, &world_shader, format,
            true, wgpu::CompareFunction::Less,
        );
        // Variante RT : profondeur déjà écrite par la pré-pass -> test large, écriture off.
        let world_pipeline_rt = Self::world_pipeline_with(
            &device, &world_bind_layout, &mat_layout, &world_shader, format,
            false, wgpu::CompareFunction::LessEqual,
        );
        let prepass_pipeline = Self::prepass_pipeline(&device, &world_bind_layout, &world_shader);

        // Post-process.
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/post.wgsl").into()),
        });
        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, multisampled: false, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
            ],
        });
        let post_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("post-uniform"),
            contents: bytemuck::cast_slice(&[0.0f32; 4]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let (offscreen_tex, offscreen_view) = Self::make_offscreen(&device, size.width.max(1), size.height.max(1), format);
        let (depth_tex, depth_view) = Self::make_depth(&device, size.width.max(1), size.height.max(1));
        let post_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post-bind"),
            layout: &post_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: post_uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&texture::linear_sampler(&device, false)) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&offscreen_view) },
            ],
        });
        let post_pipeline = Self::post_pipeline(&device, &post_layout, &post_shader, format);

        // ----- Ray tracing : shader, pipeline, buffers, bind group -----
        let rt_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rt.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/rt.wgsl").into()),
        });
        let rt_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rt-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 4, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, multisampled: false, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
            ],
        });
        let rt_pipeline = Self::rt_pipeline(&device, &rt_bind_layout, &rt_shader);
        let rt_params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rt-params"),
            size: std::mem::size_of::<RtParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let rt_static_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rt-static-boxes"),
            size: (rtscene::MAX_RT_STATIC_BOXES * std::mem::size_of::<rtscene::GpuAabb>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let rt_dyn_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rt-dyn-boxes"),
            size: (rtscene::MAX_RT_DYN_BOXES * std::mem::size_of::<rtscene::GpuAabb>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (rt0_tex, rt0_view) = Self::make_rt_target(&device, 1, 1, "rt0");
        let (rt1_tex, rt1_view) = Self::make_rt_target(&device, 1, 1, "rt1");
        let rt_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rt-bind"),
            layout: &rt_bind_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: rt_params_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: world_uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: rt_static_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: rt_dyn_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&depth_view) },
            ],
        });

        // UI.
        let ui_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/ui.wgsl").into()),
        });
        let ui_layout0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-layout0"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let ui_layout1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-layout1"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, multisampled: false, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
            ],
        });
        let ui_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ui-uniform"),
            contents: bytemuck::cast_slice(&[size.width as f32, size.height as f32, 0.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let ui_bind0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-bind0"),
            layout: &ui_layout0,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: ui_uniform_buf.as_entire_binding() }],
        });
        let white_tex = texture::white_texture(&device, &queue);
        let white_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-white"),
            layout: &ui_layout1,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Sampler(&texture::linear_sampler(&device, false)) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&white_tex.view) },
            ],
        });
        let (font_tex, font) = ui::load_font(&device, &queue);
        let font_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-font"),
            layout: &ui_layout1,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Sampler(&texture::linear_sampler(&device, false)) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&font_tex.view) },
            ],
        });
        let ui_pipeline = Self::ui_pipeline(&device, &ui_layout0, &ui_layout1, &ui_shader, format);
        let ui_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui-verts"),
            size: (4 * 4096 * std::mem::size_of::<ui::UiVertex>() as u64).max(64),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Renderer {
            device,
            queue,
            surface,
            surface_config,
            window,
            models,
            world_pipeline,
            world_bind0,
            world_uniform_buf,
            material_bind_groups,
            post_pipeline,
            post_bind,
            post_uniform_buf,
            offscreen_view,
            offscreen_tex,
            depth_view,
            depth_tex,
            render_scale: 1.0,
            ui_pipeline,
            ui_bind0,
            ui_uniform_buf,
            font_bg,
            white_bg,
            font,
            ui_buf,
            dyn_scratch: HashMap::new(),
            rt_mode: 0,
            rt_scale: 0.4,
            world_bind_layout,
            world_pipeline_rt,
            prepass_pipeline,
            rt_pipeline,
            rt_bind_layout,
            rt_bind,
            rt_params_buf,
            rt_static_buf,
            rt_dyn_buf,
            rt_static_count: 0,
            rt0_tex,
            rt1_tex,
            rt0_view,
            rt1_view,
            rt_neutral0,
            rt_neutral1,
            adapter_name,
        }
    }

    // ---------- helpers de création ----------
    fn world_sampler(device: &wgpu::Device) -> wgpu::Sampler {
        texture::linear_sampler(device, true)
    }

    fn world_bind_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world-layout0"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, multisampled: false, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, multisampled: false, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
            ],
        })
    }

    /// Bind group 0 du monde : uniform + sampler + textures RT (réelles ou neutres).
    fn make_world_bind0(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform_buf: &wgpu::Buffer,
        rt0: &wgpu::TextureView,
        rt1: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        let sampler = texture::linear_sampler(device, false);
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world-bind0"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(rt0) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(rt1) },
            ],
        })
    }

    fn material_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world-layout1"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, multisampled: false, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
            ],
        })
    }

    fn world_pipeline_with(
        device: &wgpu::Device,
        bind0: &wgpu::BindGroupLayout,
        bind1: &wgpu::BindGroupLayout,
        shader: &wgpu::ShaderModule,
        format: wgpu::TextureFormat,
        depth_write: bool,
        depth_compare: wgpu::CompareFunction,
    ) -> wgpu::RenderPipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world-pll"),
            bind_group_layouts: &[bind0, bind1],
            push_constant_ranges: &[],
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("world-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: "vs",
                buffers: &[Vertex::LAYOUT, INSTANCE_LAYOUT],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: depth_write,
                depth_compare,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        })
    }

    /// Pré-pass profondeur (vertex seul) pour la passe de ray tracing.
    fn prepass_pipeline(device: &wgpu::Device, bind0: &wgpu::BindGroupLayout, shader: &wgpu::ShaderModule) -> wgpu::RenderPipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("prepass-pll"),
            bind_group_layouts: &[bind0],
            push_constant_ranges: &[],
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("prepass-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: "vs",
                buffers: &[Vertex::LAYOUT, INSTANCE_LAYOUT],
                compilation_options: Default::default(),
            },
            fragment: None,
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        })
    }

    /// Passe RT : fullscreen triangle, deux cibles rgba16float (MRT).
    fn rt_pipeline(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, shader: &wgpu::ShaderModule) -> wgpu::RenderPipeline {
        let pll = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rt-pll"),
            bind_group_layouts: &[layout],
            push_constant_ranges: &[],
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rt-pipeline"),
            layout: Some(&pll),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: "vs",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: "fs",
                targets: &[
                    Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba16Float, blend: None, write_mask: wgpu::ColorWrites::ALL }),
                    Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba16Float, blend: None, write_mask: wgpu::ColorWrites::ALL }),
                ],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        })
    }

    fn post_pipeline(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, shader: &wgpu::ShaderModule, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        let pll = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post-pll"),
            bind_group_layouts: &[layout],
            push_constant_ranges: &[],
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("post-pipeline"),
            layout: Some(&pll),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: "vs",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        })
    }

    fn ui_pipeline(device: &wgpu::Device, l0: &wgpu::BindGroupLayout, l1: &wgpu::BindGroupLayout, shader: &wgpu::ShaderModule, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        use ui::UiVertex;
        let pll = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-pll"),
            bind_group_layouts: &[l0, l1],
            push_constant_ranges: &[],
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-pipeline"),
            layout: Some(&pll),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: "vs",
                buffers: &[UiVertex::LAYOUT],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        })
    }

    fn make_offscreen(device: &wgpu::Device, w: u32, h: u32, format: wgpu::TextureFormat) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        (tex, view)
    }

    fn make_depth(device: &wgpu::Device, w: u32, h: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        (tex, view)
    }

    /// Cible rgba16float de la passe RT (RENDER_ATTACHMENT + TEXTURE_BINDING).
    fn make_rt_target(device: &wgpu::Device, w: u32, h: u32, label: &str) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        (tex, view)
    }

    pub fn size(&self) -> (u32, u32) {
        (self.surface_config.width, self.surface_config.height)
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        let (w, h) = (w.max(1), h.max(1));
        self.surface_config.width = w;
        self.surface_config.height = h;
        self.surface.configure(&self.device, &self.surface_config);
        let (sw, sh) = self.scaled_size();
        let (t, v) = Self::make_offscreen(&self.device, sw, sh, self.surface_config.format);
        self.offscreen_tex = t;
        self.offscreen_view = v;
        self.rebuild_post_bind();
        let (dt, dv) = Self::make_depth(&self.device, sw, sh);
        self.depth_tex = dt;
        self.depth_view = dv;
        self.rebuild_rt_bind();
        self.recreate_rt_targets();
        let ui_data = [w as f32, h as f32, 0.0, 0.0];
        self.queue.write_buffer(&self.ui_uniform_buf, 0, bytemuck::cast_slice(&ui_data));
    }

    /// Taille du buffer monde = surface × échelle de rendu.
    fn scaled_size(&self) -> (u32, u32) {
        (
            ((self.surface_config.width as f32 * self.render_scale) as u32).max(1),
            ((self.surface_config.height as f32 * self.render_scale) as u32).max(1),
        )
    }

    fn rebuild_post_bind(&mut self) {
        let post_layout = self.post_pipeline.get_bind_group_layout(0);
        self.post_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post-bind"),
            layout: &post_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.post_uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&texture::linear_sampler(&self.device, false)) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&self.offscreen_view) },
            ],
        });
    }

    /// Change l'échelle de rendu (recrée offscreen + depth, post/UI restent en pleine résolution).
    pub fn set_render_scale(&mut self, scale: f32) {
        let s = scale.clamp(0.45, 1.0);
        if (s - self.render_scale).abs() < 0.001 {
            return;
        }
        self.render_scale = s;
        let (sw, sh) = self.scaled_size();
        let (t, v) = Self::make_offscreen(&self.device, sw, sh, self.surface_config.format);
        self.offscreen_tex = t;
        self.offscreen_view = v;
        self.rebuild_post_bind();
        let (dt, dv) = Self::make_depth(&self.device, sw, sh);
        self.depth_tex = dt;
        self.depth_view = dv;
        self.rebuild_rt_bind();
        self.recreate_rt_targets();
    }

    // ---------- ray tracing (optionnel) ----------

    /// Échelle du tampon RT selon le mode : qualité 0.4x, ultra 0.5x du tampon monde.
    fn rt_target_scale(mode: u8) -> f32 {
        match mode {
            2 => 0.5,
            _ => 0.4,
        }
    }

    /// Change de mode RT (0 désactivé / 1 qualité / 2 ultra) et recrée ce qu'il faut.
    pub fn set_rt_mode(&mut self, mode: u8) {
        let mode = mode.min(2);
        if mode == self.rt_mode {
            return;
        }
        self.rt_mode = mode;
        self.rt_scale = Self::rt_target_scale(mode);
        self.recreate_rt_targets();
        self.rebuild_world_bind();
    }

    /// Recrée les cibles RT (dépendent du mode + de la taille du tampon monde).
    fn recreate_rt_targets(&mut self) {
        if self.rt_mode == 0 {
            return;
        }
        let (ow, oh) = self.scaled_size();
        let w = ((ow as f32 * self.rt_scale) as u32).max(1);
        let h = ((oh as f32 * self.rt_scale) as u32).max(1);
        let (t0, v0) = Self::make_rt_target(&self.device, w, h, "rt0");
        let (t1, v1) = Self::make_rt_target(&self.device, w, h, "rt1");
        self.rt0_tex = t0;
        self.rt1_tex = t1;
        self.rt0_view = v0;
        self.rt1_view = v1;
    }

    /// Rebranche les textures RT réelles ou neutres sur le bind group du monde.
    fn rebuild_world_bind(&mut self) {
        let (v0, v1) = if self.rt_mode > 0 {
            (&self.rt0_view, &self.rt1_view)
        } else {
            (&self.rt_neutral0, &self.rt_neutral1)
        };
        let bg = Self::make_world_bind0(&self.device, &self.world_bind_layout, &self.world_uniform_buf, v0, v1);
        self.world_bind0 = bg;
    }

    /// Rebranche la texture profondeur (change à chaque resize / changement d'échelle).
    fn rebuild_rt_bind(&mut self) {
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rt-bind"),
            layout: &self.rt_bind_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.rt_params_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.world_uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: self.rt_static_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: self.rt_dyn_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&self.depth_view) },
            ],
        });
        self.rt_bind = bg;
    }

    /// Reconstruit les boîtes statiques de la scène RT (au démarrage d'une partie).
    pub fn update_rt_statics(&mut self, map: &MapData, insts: &[(String, InstanceData)]) {
        let boxes = {
            let models = &self.models;
            rtscene::build_static_boxes(map, insts, |name| models.get(name).map(|m| m.bounds))
        };
        self.rt_static_count = boxes.len() as u32;
        let mut padded = boxes;
        padded.resize(rtscene::MAX_RT_STATIC_BOXES, rtscene::GpuAabb::ZERO);
        self.queue.write_buffer(&self.rt_static_buf, 0, bytemuck::cast_slice(&padded));
    }

    pub fn render_scale(&self) -> f32 {
        self.render_scale
    }

    // ---------- construction des batchs statiques ----------
    pub fn build_static(&self, insts: &[(String, InstanceData)]) -> StaticBatches {
        let mut groups: HashMap<PartKey, Vec<InstanceRaw>> = HashMap::new();
        for (model_name, inst) in insts {
            if let Some(model) = self.models.get(model_name) {
                for (pi, part) in model.parts.iter().enumerate() {
                    groups
                        .entry(PartKey { model: model_name.clone(), part: pi })
                        .or_default()
                        .push(inst.raw());
                    let _ = &part.mat;
                }
            }
        }
        let mut batches = Vec::new();
        let mut total = 0usize;
        for (key, raws) in groups {
            let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("static-instances"),
                contents: bytemuck::cast_slice(&raws),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
            total += raws.len();
            batches.push(StaticBatch { key, buffer, count: raws.len() as u32 });
        }
        StaticBatches { batches, total_instances: total }
    }

    // ---------- dessin ----------
    /// Upload les instances dynamiques dans le scratch GPU et renvoie le plan
    /// de dessin (clé de part + nombre d'instances).
    fn upload_dynamics(&mut self, dynamics: &[(String, Vec<InstanceData>)]) -> Vec<(PartKey, u32)> {
        let mut plan = Vec::new();
        for (model_name, insts) in dynamics {
            let Some(model) = self.models.get(model_name) else { continue };
            for (pi, _part) in model.parts.iter().enumerate() {
                if insts.is_empty() {
                    continue;
                }
                let key = PartKey { model: model_name.clone(), part: pi };
                let raws: Vec<InstanceRaw> = insts.iter().map(|i| i.raw()).collect();
                let bytes_needed = (raws.len() * std::mem::size_of::<InstanceRaw>()) as u64;
                let needs_grow = self
                    .dyn_scratch
                    .get(&key)
                    .map(|(_, cap)| *cap < bytes_needed)
                    .unwrap_or(true);
                if needs_grow {
                    let cap = bytes_needed.max(1024);
                    let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("dyn-instances"),
                        size: cap,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    self.dyn_scratch.insert(key.clone(), (buf, cap));
                }
                let (buf, _cap) = &self.dyn_scratch[&key];
                self.queue.write_buffer(buf, 0, bytemuck::cast_slice(&raws));
                plan.push((key, raws.len() as u32));
            }
        }
        plan
    }

    /// Dessine statiques + dynamiques (plan préparé) avec le pipeline donné.
    fn draw_instances_pass(
        &self,
        pass: &mut wgpu::RenderPass<'static>,
        statics: &StaticBatches,
        plan: &[(PartKey, u32)],
        pipeline: &wgpu::RenderPipeline,
    ) {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.world_bind0, &[]);

        // Statiques.
        for batch in &statics.batches {
            self.draw_part(pass, &batch.key, &batch.buffer, batch.count);
        }

        // Dynamiques (déjà uploadées).
        for (key, count) in plan {
            let (buf, _cap) = &self.dyn_scratch[key];
            self.draw_part(pass, key, buf, *count);
        }
    }

    fn draw_part(
        &self,
        pass: &mut wgpu::RenderPass<'static>,
        key: &PartKey,
        instance_buf: &wgpu::Buffer,
        count: u32,
    ) {
        let Some(model) = self.models.get(&key.model) else { return };
        let Some(part) = model.parts.get(key.part) else { return };
        let Some(bg) = self.material_bind_groups.get(&part.mat) else { return };
        pass.set_bind_group(1, bg, &[]);
        pass.set_vertex_buffer(0, part.vertex_buf.slice(..));
        pass.set_vertex_buffer(1, instance_buf.slice(..));
        pass.set_index_buffer(part.index_buf.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..part.count, 0, 0..count);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        world_uniform: &WorldUniform,
        rt: Option<RtFrame>,
        statics: &StaticBatches,
        dynamics: &[(String, Vec<InstanceData>)],
        ui_ops: &[ui::UiOp],
        post_params: [f32; 4],
    ) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let frame_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.queue
            .write_buffer(&self.world_uniform_buf, 0, bytemuck::bytes_of(world_uniform));
        self.queue
            .write_buffer(&self.post_uniform_buf, 0, bytemuck::cast_slice(&post_params));

        // Ray tracing actif ? Prépare uniform + boîtes dynamiques.
        let rt_on = self.rt_mode > 0;
        if let (true, Some(f)) = (rt_on, &rt) {
            let (rt_w, rt_h) = (self.rt0_tex.size().width, self.rt0_tex.size().height);
            let (dw, dh) = (self.depth_tex.size().width, self.depth_tex.size().height);
            let params = RtParams {
                inv_vp: f.inv_vp,
                cam_pos: world_uniform.cam_pos,
                dims: [rt_w as f32, rt_h as f32, dw as f32, dh as f32],
                counts: [
                    self.rt_static_count as f32,
                    (f.dyn_boxes.len() as f32).min(rtscene::MAX_RT_DYN_BOXES as f32),
                    world_uniform.misc[0],
                    1.6,
                ],
                misc: [if self.rt_mode >= 2 { 0.55 } else { 0.0 }, 0.35, 0.0, 0.0],
            };
            self.queue.write_buffer(&self.rt_params_buf, 0, bytemuck::bytes_of(&params));
            let mut boxes: Vec<rtscene::GpuAabb> = f
                .dyn_boxes
                .iter()
                .take(rtscene::MAX_RT_DYN_BOXES)
                .copied()
                .collect();
            boxes.resize(rtscene::MAX_RT_DYN_BOXES, rtscene::GpuAabb::ZERO);
            self.queue.write_buffer(&self.rt_dyn_buf, 0, bytemuck::cast_slice(&boxes));
        }

        // Quads UI.
        let (verts, draws) = ui::build_ui_verts(ui_ops, &self.font);
        if !verts.is_empty() {
            let bytes_needed = (verts.len() * std::mem::size_of::<ui::UiVertex>()) as u64;
            if self.ui_buf.size() < bytes_needed {
                self.ui_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("ui-verts"),
                    size: bytes_needed * 2,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }
            self.queue.write_buffer(&self.ui_buf, 0, bytemuck::cast_slice(&verts));
        }

        // Upload des dynamiques (une seule fois, partagé par toutes les passes).
        let dyn_plan = self.upload_dynamics(dynamics);

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame"),
        });

        // 1) Pré-pass profondeur + passe RT + monde (RT activé), ou monde seul.
        if rt_on {
            {
                let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("prepass-depth"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                let mut pass = pass.forget_lifetime();
                self.draw_instances_pass(&mut pass, statics, &dyn_plan, &self.prepass_pipeline);
            }
            {
                let rt0_att = wgpu::RenderPassColorAttachment {
                    view: &self.rt0_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                };
                let rt1_att = wgpu::RenderPassColorAttachment {
                    view: &self.rt1_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                };
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("rt-pass"),
                    color_attachments: &[Some(rt0_att), Some(rt1_att)],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rt_pipeline);
                pass.set_bind_group(0, &self.rt_bind, &[]);
                pass.draw(0..3, 0..1);
            }
            {
                let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("world-pass-rt"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.offscreen_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.004,
                                g: 0.005,
                                b: 0.008,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                let mut pass = pass.forget_lifetime();
                self.draw_instances_pass(&mut pass, statics, &dyn_plan, &self.world_pipeline_rt);
            }
        } else {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.offscreen_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.004,
                            g: 0.005,
                            b: 0.008,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            // 'pass borrow self...' : étendre la durée de vie via transmute sûr ici
            let mut pass = pass.forget_lifetime();
            self.draw_instances_pass(&mut pass, statics, &dyn_plan, &self.world_pipeline);
        }

        // 2) Post-process -> surface.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.post_pipeline);
            pass.set_bind_group(0, &self.post_bind, &[]);
            pass.draw(0..3, 0..1);
        }

        // 3) UI -> surface.
        if !verts.is_empty() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.ui_pipeline);
            pass.set_bind_group(0, &self.ui_bind0, &[]);
            let stride = std::mem::size_of::<ui::UiVertex>() as u64;
            let mut cursor = 0u64;
            for (_op, (is_text, quads)) in ui_ops.iter().zip(draws.iter()) {
                if *quads == 0 {
                    continue;
                }
                match is_text {
                    true => pass.set_bind_group(1, &self.font_bg, &[]),
                    false => pass.set_bind_group(1, &self.white_bg, &[]),
                }
                let bytes = *quads as u64 * 4 * stride;
                pass.set_vertex_buffer(0, self.ui_buf.slice(cursor..cursor + bytes));
                pass.draw(0..4, 0..*quads);
                cursor += bytes;
            }
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}
