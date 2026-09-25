//! 主渲染器：场景离屏渲染 → 光照合成 → 目标纹理
use crate::atlas::AtlasBuilder;
use crate::batcher::SpriteVertex;
use crate::camera::Camera;
use image::RgbaImage;
use wgpu;

#[allow(dead_code)]
const ATLAS_SIZE: u32 = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderKind {
    Sprite,
    Pixels,
    Composite,
    Bloom,
}

pub struct FrameParams<'a> {
    pub camera: &'a Camera,
    pub sky_color: [f32; 3],
    pub ambient: f32,
    /// Bloom 强度（0 = 关闭）
    pub bloom: f32,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,

    sprite_pipe: wgpu::RenderPipeline,
    pixels_pipe: wgpu::RenderPipeline,
    composite_pipe: wgpu::RenderPipeline,
    bloom_bright_pipe: wgpu::RenderPipeline,
    bloom_blur_pipe: wgpu::RenderPipeline,
    egui: Option<crate::egui::EguiRenderer>,

    camera_buf: wgpu::Buffer,
    camera_bg: wgpu::BindGroup,
    #[allow(dead_code)]
    camera_bg_layout: wgpu::BindGroupLayout,
    tex_bg_layout: wgpu::BindGroupLayout,
    pixels_bg_layout: wgpu::BindGroupLayout,
    composite_bg_layout: wgpu::BindGroupLayout,

    comp_buf: wgpu::Buffer,
    vbuf: wgpu::Buffer,
    vbuf_verts: usize,

    atlas: Option<wgpu::TextureView>,
    atlas_tex: Option<wgpu::Texture>,
    atlas_size: u32,
    /// 图集货架分配游标（x, y, row_h）——运行时新增贴图（动画帧）用
    atlas_cursor: (u32, u32, u32),
    /// Bloom 中间纹理（1/4 分辨率，双缓冲 ping-pong）
    bloom_a: Option<wgpu::TextureView>,
    bloom_b: Option<wgpu::TextureView>,
    bloom_size: (u32, u32),
    bloom_ubo: Vec<wgpu::Buffer>, // [bright, blur_h, blur_v]
    bloom_bg_layout: wgpu::BindGroupLayout,
    world: Option<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
    palette: Option<wgpu::TextureView>,
    light: Option<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
    world_size: (u32, u32),
    scene: Option<(wgpu::Texture, wgpu::TextureView, u32, u32)>,

    samp_nearest: wgpu::Sampler,
    samp_linear: wgpu::Sampler,
}

impl Renderer {
    pub fn new(gpu: crate::gpu::Gpu) -> Self {
        let crate::gpu::Gpu { device, queue, format } = gpu;
        let sprite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprite.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!(
                "../../../assets/shaders/sprite.wgsl"
            ).into()),
        });
        let pixels_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pixels.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!(
                "../../../assets/shaders/pixels.wgsl"
            ).into()),
        });
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("composite.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!(
                "../../../assets/shaders/composite.wgsl"
            ).into()),
        });
        let bloom_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bloom.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!(
                "../../../assets/shaders/bloom.wgsl"
            ).into()),
        });

        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera-ubo"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(64),
                },
                count: None,
            }],
        });
        let camera_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera-bg"),
            layout: &camera_bg_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        let tex_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tex-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
            ],
        });

        let pixels_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pixels-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let composite_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(48),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // bloom 纹理 + 采样
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // ---- Bloom 管线资源 ----
        let bloom_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
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
        let bloom_ubo = (0..3)
            .map(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("bloom-ubo"),
                    size: 16,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect::<Vec<_>>();

        let comp_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("composite-ubo"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite-vbuf"),
            size: (64 * 1024) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let blend = wgpu::BlendState::ALPHA_BLENDING;
        let targets = [Some(wgpu::ColorTargetState {
            format,
            blend: Some(blend),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let sprite_pipe = Self::make_pipe(
            &device,
            &sprite_shader,
            &[&camera_bg_layout, &tex_bg_layout],
            &targets,
            &[Self::sprite_vlayout()],
        );
        let pixels_pipe = Self::make_pipe(
            &device,
            &pixels_shader,
            &[&camera_bg_layout, &pixels_bg_layout],
            &targets,
            &[Self::sprite_vlayout()],
        );
        let composite_pipe = Self::make_pipe(&device, &composite_shader, &[&composite_bg_layout], &targets, &[]);
        // Bloom：亮部提取与模糊（h/v 共用模糊管线，方向由各自 ubo 提供）
        let bloom_bright_pipe = Self::make_pipe_entry(
            &device,
            &bloom_shader,
            &[&bloom_bg_layout],
            &targets,
            &[],
            Some("fs_bright"),
        );
        let bloom_blur_pipe = Self::make_pipe_entry(
            &device,
            &bloom_shader,
            &[&bloom_bg_layout],
            &targets,
            &[],
            Some("fs_blur"),
        );

        let samp_nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let samp_linear = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let egui = crate::egui::EguiRenderer::new(&device, format);
        Self {
            device,
            queue,
            format,
            sprite_pipe,
            pixels_pipe,
            composite_pipe,
            bloom_bright_pipe,
            bloom_blur_pipe,
            egui: Some(egui),
            camera_buf,
            camera_bg,
            camera_bg_layout,
            tex_bg_layout,
            pixels_bg_layout,
            composite_bg_layout,
            comp_buf,
            vbuf,
            vbuf_verts: 0,
            atlas: None,
            atlas_tex: None,
            atlas_size: 0,
            atlas_cursor: (1, 1, 0),
            bloom_a: None,
            bloom_b: None,
            bloom_size: (0, 0),
            bloom_ubo,
            bloom_bg_layout,
            world: None,
            palette: None,
            light: None,
            world_size: (4096, 2048),
            scene: None,
            samp_nearest,
            samp_linear,
        }
    }

    fn sprite_vlayout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SpriteVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 8, shader_location: 1 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 2 },
            ],
        }
    }

    fn make_pipe(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        bglayouts: &[&wgpu::BindGroupLayout],
        targets: &[Option<wgpu::ColorTargetState>],
        buffers: &[wgpu::VertexBufferLayout],
    ) -> wgpu::RenderPipeline {
        Self::make_pipe_entry(device, shader, bglayouts, targets, buffers, None)
    }

    fn make_pipe_entry(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        bglayouts: &[&wgpu::BindGroupLayout],
        targets: &[Option<wgpu::ColorTargetState>],
        buffers: &[wgpu::VertexBufferLayout],
        fs_entry: Option<&str>,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: bglayouts,
                push_constant_ranges: &[],
            })),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(fs_entry.unwrap_or("fs_main")),
                compilation_options: Default::default(),
                targets,
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    /// Bloom 中间纹理（1/4 分辨率，随场景尺寸重建）
    fn ensure_bloom(&mut self, w: u32, h: u32) {
        let matches =
            self.bloom_size == (w.max(1), h.max(1)) && self.bloom_a.is_some();
        if matches {
            return;
        }
        let make = |label: &str| {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            tex.create_view(&Default::default())
        };
        self.bloom_a = Some(make("bloom-a"));
        self.bloom_b = Some(make("bloom-b"));
        self.bloom_size = (w.max(1), h.max(1));
    }

    /// 着色器热重载：naga 预校验通过后重建对应管线
    pub fn reload_shader(&mut self, kind: ShaderKind, source: &str) -> Result<(), String> {
        // 预校验（语法 + 语义），失败时保留旧管线
        let module = naga::front::wgsl::parse_str(source).map_err(|e| format!("解析失败: {e}"))?;
        let mut validator =
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
        validator.validate(&module).map_err(|e| format!("校验失败: {e}"))?;

        let sm = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hot-reload"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let targets = [Some(wgpu::ColorTargetState {
            format: self.format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];
        match kind {
            ShaderKind::Sprite => {
                self.sprite_pipe = Self::make_pipe(
                    &self.device,
                    &sm,
                    &[&self.camera_bg_layout, &self.tex_bg_layout],
                    &targets,
                    &[Self::sprite_vlayout()],
                );
            }
            ShaderKind::Pixels => {
                self.pixels_pipe = Self::make_pipe(
                    &self.device,
                    &sm,
                    &[&self.camera_bg_layout, &self.pixels_bg_layout],
                    &targets,
                    &[Self::sprite_vlayout()],
                );
            }
            ShaderKind::Composite => {
                self.composite_pipe =
                    Self::make_pipe(&self.device, &sm, &[&self.composite_bg_layout], &targets, &[]);
            }
            ShaderKind::Bloom => {
                self.bloom_bright_pipe = Self::make_pipe_entry(
                    &self.device,
                    &sm,
                    &[&self.bloom_bg_layout],
                    &targets,
                    &[],
                    Some("fs_bright"),
                );
                self.bloom_blur_pipe = Self::make_pipe_entry(
                    &self.device,
                    &sm,
                    &[&self.bloom_bg_layout],
                    &targets,
                    &[],
                    Some("fs_blur"),
                );
            }
        }
        Ok(())
    }

    /// 由图集构建器创建图集纹理
    pub fn set_atlas(&mut self, builder: AtlasBuilder) -> std::collections::HashMap<String, crate::atlas::Region> {
        // 记录分配游标，供运行时追加贴图（动画帧等）
        let ((cx, cy), row_h) = builder.cursor();
        self.atlas_cursor = (cx, cy, row_h);
        let (data, size, entries) = builder.flatten();
        self.atlas_size = size;
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        );
        let view = tex.create_view(&Default::default());
        self.atlas = Some(view);
        self.atlas_tex = Some(tex);
        entries
    }

    /// 图集尺寸（动画帧区域 uv 计算用）
    pub fn atlas_size(&self) -> u32 {
        self.atlas_size
    }

    /// 图集货架式分配一块空闲区域（与 AtlasBuilder 同款游标逻辑）
    pub fn atlas_alloc(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if self.atlas_size == 0 {
            return None;
        }
        let size = self.atlas_size;
        let (cx, cy, row_h) = self.atlas_cursor;
        let (nx, ny, nrh) = if cx + w + 1 > size {
            (1, cy + row_h, h + 1)
        } else {
            (cx, cy, row_h.max(h + 1))
        };
        if nx + w > size || ny + h > size {
            return None; // 图集已满
        }
        self.atlas_cursor = (nx + w + 1, ny, nrh);
        Some((nx, ny))
    }

    /// 向图集写入一块 RGBA 数据（运行时新增动画帧等）
    pub fn upload_atlas(&mut self, x: u32, y: u32, w: u32, h: u32, data: &[u8]) {
        let Some(tex) = self.atlas_tex.clone() else { return };
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    }

    /// 像素世界纹理（RG8：材质 + 明度）
    pub fn set_world_texture(&mut self, w: u32, h: u32) {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("world-pixels"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        self.world = Some((tex, view, w, h));
        self.world_size = (w, h);
    }

    /// 调色板（256 色）
    pub fn set_palette(&mut self, colors: &[[u8; 4]]) {
        let mut data = vec![0u8; 256 * 4];
        for (i, c) in colors.iter().enumerate().take(256) {
            data[i * 4..i * 4 + 4].copy_from_slice(c);
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("palette"),
            size: wgpu::Extent3d { width: 256, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256 * 4), rows_per_image: None },
            wgpu::Extent3d { width: 256, height: 1, depth_or_array_layers: 1 },
        );
        self.palette = Some(tex.create_view(&Default::default()));
    }

    /// 光照纹理（RG8：天空光 / 方块光，每 Tile 一 texel）
    pub fn set_light_texture(&mut self, w: u32, h: u32) {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("light"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        self.light = Some((tex, view, w, h));
    }

    /// 上传像素世界的脏矩形（RG 交错字节）
    pub fn upload_world(&mut self, x: u32, y: u32, w: u32, h: u32, data: &[u8]) {
        let Some((tex, _, tw, th)) = &self.world else { return };
        let tex = tex.clone();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 2),
                rows_per_image: None,
            },
            wgpu::Extent3d { width: w.min(*tw - x), height: h.min(*th - y), depth_or_array_layers: 1 },
        );
    }

    /// 上传整张光照纹理
    pub fn upload_light(&mut self, data: &[u8]) {
        let Some((tex, _, w, h)) = &self.light else { return };
        let tex = tex.clone();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 2),
                rows_per_image: None,
            },
            wgpu::Extent3d { width: *w, height: *h, depth_or_array_layers: 1 },
        );
    }

    /// 上传光照纹理区域（RG 交错字节，局部重算用）
    pub fn upload_light_region(&mut self, x: u32, y: u32, w: u32, h: u32, data: &[u8]) {
        let Some((tex, _, tw, th)) = &self.light else { return };
        let tex = tex.clone();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 2),
                rows_per_image: None,
            },
            wgpu::Extent3d { width: w.min(*tw - x), height: h.min(*th - y), depth_or_array_layers: 1 },
        );
    }

    fn ensure_scene(&mut self, w: u32, h: u32) {
        let matches = self
            .scene
            .as_ref()
            .map(|(_, _, sw, sh)| *sw == w && *sh == h)
            .unwrap_or(false);
        if !matches {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("scene"),
                size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            self.scene = Some((tex, view, w, h));
        }
    }

    /// 渲染一帧；screenshot=true 时回读目标纹理（仅无头模式）
    pub fn draw_frame(
        &mut self,
        target: &wgpu::Texture,
        p: FrameParams,
        atlas_verts: &[SpriteVertex],
        world_verts: &[SpriteVertex],
        screenshot: bool,
    ) -> Option<RgbaImage> {
        let (tw, th) = (target.width(), target.height());
        self.ensure_scene(tw, th);

        // ---- uniforms ----
        let vp = p.camera.view_proj();
        self.queue
            .write_buffer(&self.camera_buf, 0, bytemuck::cast_slice(&vp.to_cols_array_2d()));
        let tl = p.camera.top_left();
        let (ww, wh) = self.world_size;
        // 光照纹理 1 texel = 1 Tile，覆盖整个世界 → uv = 世界像素 / 世界像素尺寸
        let comp: [f32; 12] = [
            tl.x,
            tl.y,
            p.camera.viewport.0,
            p.camera.viewport.1,
            1.0 / ww.max(1) as f32,
            1.0 / wh.max(1) as f32,
            p.ambient,
            0.0,
            0.0,
            0.0,
            0.0,
            p.bloom,
        ];
        self.queue.write_buffer(&self.comp_buf, 0, bytemuck::cast_slice(&comp));

        // ---- Bloom 中间资源与 uniform ----
        self.ensure_bloom(tw / 4, th / 4);
        let (bw, bh) = self.bloom_size;
        let ubos: [[f32; 4]; 3] = [
            [1.0 / tw.max(1) as f32, 1.0 / th.max(1) as f32, 0.0, 0.0], // bright（dir 无效）
            [1.0 / bw as f32, 1.0 / bh as f32, 1.0, 0.0],               // blur 水平
            [1.0 / bw as f32, 1.0 / bh as f32, 0.0, 1.0],               // blur 垂直
        ];
        for (i, u) in ubos.iter().enumerate() {
            self.queue.write_buffer(&self.bloom_ubo[i], 0, bytemuck::cast_slice(u));
        }

        // ---- 顶点缓冲 ----
        let total = atlas_verts.len() + world_verts.len();
        if total > 0 {
            let need = total * std::mem::size_of::<SpriteVertex>();
            let cap = (self.vbuf.size() as usize).max(need.next_power_of_two().max(64 * 1024));
            if cap != self.vbuf.size() as usize {
                self.vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("sprite-vbuf"),
                    size: cap as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }
            let mut all = Vec::with_capacity(total);
            all.extend_from_slice(atlas_verts);
            all.extend_from_slice(world_verts);
            self.queue.write_buffer(&self.vbuf, 0, bytemuck::cast_slice(&all));
            self.vbuf_verts = total;
        } else {
            self.vbuf_verts = 0;
        }

        // ---- bind groups ----
        let atlas_bg = self.atlas.as_ref().map(|v| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("atlas-bg"),
                layout: &self.tex_bg_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Sampler(&self.samp_nearest) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(v) },
                ],
            })
        });
        let world_bg = self.world.as_ref().zip(self.palette.as_ref()).map(|((_, wv, _, _), pv)| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("world-bg"),
                layout: &self.pixels_bg_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Sampler(&self.samp_nearest) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(wv) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.samp_nearest) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(pv) },
                ],
            })
        });
        let scene_view = self.scene.as_ref().unwrap().1.clone();
        let bloom_a_view = self.bloom_a.clone();
        let bloom_b_view = self.bloom_b.clone();
        let light_bg = self.light.as_ref().map(|(_, lv, _, _)| {
            let mut entries = vec![
                wgpu::BindGroupEntry { binding: 0, resource: self.comp_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&scene_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.samp_linear) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(lv) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&self.samp_linear) },
            ];
            if let Some(ba) = &bloom_a_view {
                entries.push(wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(ba) });
                entries.push(wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::Sampler(&self.samp_linear) });
            }
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("composite-bg"),
                layout: &self.composite_bg_layout,
                entries: &entries,
            })
        });
        // Bloom 各 pass 的 bind group：bright ← scene，blurH ← A，blurV ← B
        let mk_bloom_bg = |tex: &wgpu::TextureView, ubo: &wgpu::Buffer| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom-bg"),
                layout: &self.bloom_bg_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: ubo.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(tex) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.samp_linear) },
                ],
            })
        };
        let bloom_bg_bright = mk_bloom_bg(&scene_view, &self.bloom_ubo[0]);
        let bloom_bg_h = bloom_a_view
            .as_ref()
            .map(|a| mk_bloom_bg(a, &self.bloom_ubo[1]));
        let bloom_bg_v = bloom_b_view
            .as_ref()
            .map(|b| mk_bloom_bg(b, &self.bloom_ubo[2]));

        // ---- 编码 ----
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        {
            let (_, scene_view, _, _) = self.scene.as_ref().unwrap();
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: scene_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: p.sky_color[0] as f64,
                            g: p.sky_color[1] as f64,
                            b: p.sky_color[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.sprite_pipe);
            pass.set_bind_group(0, &self.camera_bg, &[]);
            pass.set_vertex_buffer(0, self.vbuf.slice(..));
            if let (Some(bg), false) = (atlas_bg.as_ref(), atlas_verts.is_empty()) {
                pass.set_bind_group(1, bg, &[]);
                pass.draw(0..atlas_verts.len() as u32, 0..1);
            }
            if let (Some(bg), false) = (world_bg.as_ref(), world_verts.is_empty()) {
                pass.set_pipeline(&self.pixels_pipe);
                pass.set_bind_group(0, &self.camera_bg, &[]);
                pass.set_bind_group(1, bg, &[]);
                pass.set_vertex_buffer(0, self.vbuf.slice(..));
                pass.draw(
                    atlas_verts.len() as u32..(atlas_verts.len() + world_verts.len()) as u32,
                    0..1,
                );
            }
        }
        // ---- Bloom：亮部提取 → 高斯模糊（水平/垂直）→ 结果写入 bloom_a ----
        if p.bloom > 0.0 {
            if let (Some(ba), Some(bb), Some(bgh), Some(bgv)) =
                (&bloom_a_view, &bloom_b_view, &bloom_bg_h, &bloom_bg_v)
            {
                {
                    let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("bloom-bright"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: ba,
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
                    pass.set_pipeline(&self.bloom_bright_pipe);
                    pass.set_bind_group(0, &bloom_bg_bright, &[]);
                    pass.draw(0..3, 0..1);
                }
                {
                    let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("bloom-blur-h"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: bb,
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
                    pass.set_pipeline(&self.bloom_blur_pipe);
                    pass.set_bind_group(0, bgh, &[]);
                    pass.draw(0..3, 0..1);
                }
                {
                    let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("bloom-blur-v"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: ba,
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
                    pass.set_pipeline(&self.bloom_blur_pipe);
                    pass.set_bind_group(0, bgv, &[]);
                    pass.draw(0..3, 0..1);
                }
            }
        }
        {
            let target_view = target.create_view(&Default::default());
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("composite-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
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
            if let Some(bg) = light_bg.as_ref() {
                pass.set_pipeline(&self.composite_pipe);
                pass.set_bind_group(0, bg, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        self.queue.submit([enc.finish()]);

        if screenshot {
            Some(self.read_back(target, tw, th))
        } else {
            None
        }
    }

    /// 叠加渲染 egui UI（窗口模式每帧调用；headless 不调用）
    pub fn egui_paint(
        &mut self,
        target_view: &wgpu::TextureView,
        td: egui::TexturesDelta,
        jobs: &[egui::epaint::ClippedPrimitive],
        screen: (f32, f32),
        pixels_per_point: f32,
    ) {
        if let Some(eg) = &mut self.egui {
            let (device, queue) = (&self.device, &self.queue);
            eg.handle_textures(device, queue, td);
            eg.paint(device, queue, target_view, jobs, screen, pixels_per_point);
        }
    }

    fn read_back(&self, tex: &wgpu::Texture, w: u32, h: u32) -> RgbaImage {
        let bpr = ((w * 4 + 255) / 256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (bpr * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("readback") });
        enc.copy_texture_to_buffer(
            tex.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bpr),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);
        let data = slice.get_mapped_range();
        let mut img = RgbaImage::new(w, h);
        let swap = matches!(
            self.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );
        for y in 0..h {
            let row = &data[(y * bpr) as usize..((y * bpr) + w * 4) as usize];
            for x in 0..w as usize {
                let (r, g, b, a) = (row[x * 4], row[x * 4 + 1], row[x * 4 + 2], row[x * 4 + 3]);
                img.put_pixel(
                    x as u32,
                    y,
                    if swap {
                        image::Rgba([b, g, r, a])
                    } else {
                        image::Rgba([r, g, b, a])
                    },
                );
            }
        }
        drop(data);
        buf.unmap();
        img
    }
}
