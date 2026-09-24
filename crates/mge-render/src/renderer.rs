//! 主渲染器：场景离屏渲染 → 光照合成 → 目标纹理
use crate::atlas::AtlasBuilder;
use crate::batcher::SpriteVertex;
use crate::camera::Camera;
use image::RgbaImage;
use wgpu;

#[allow(dead_code)]
const ATLAS_SIZE: u32 = 2048;

pub struct FrameParams<'a> {
    pub camera: &'a Camera,
    pub sky_color: [f32; 3],
    pub ambient: f32,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,

    sprite_pipe: wgpu::RenderPipeline,
    pixels_pipe: wgpu::RenderPipeline,
    composite_pipe: wgpu::RenderPipeline,
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
            ],
        });

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

        let vlayout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SpriteVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 8, shader_location: 1 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 2 },
            ],
        };

        let blend = wgpu::BlendState::ALPHA_BLENDING;
        let targets = [Some(wgpu::ColorTargetState {
            format,
            blend: Some(blend),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let make_pipe = |shader: &wgpu::ShaderModule,
                         bglayouts: &[&wgpu::BindGroupLayout],
                         targets: &[Option<wgpu::ColorTargetState>],
                         buffers: &[wgpu::VertexBufferLayout]| {
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
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets,
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };

        let sprite_pipe = make_pipe(
            &sprite_shader,
            &[&camera_bg_layout, &tex_bg_layout],
            &targets,
            &[vlayout.clone()],
        );
        let pixels_pipe = make_pipe(
            &pixels_shader,
            &[&camera_bg_layout, &pixels_bg_layout],
            &targets,
            &[vlayout],
        );
        let composite_pipe = make_pipe(
            &composite_shader,
            &[&composite_bg_layout],
            &targets,
            &[],
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
            world: None,
            palette: None,
            light: None,
            world_size: (4096, 2048),
            scene: None,
            samp_nearest,
            samp_linear,
        }
    }

    /// 由图集构建器创建图集纹理
    pub fn set_atlas(&mut self, builder: AtlasBuilder) -> std::collections::HashMap<String, crate::atlas::Region> {
        let (data, size, entries) = builder.flatten();
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
        entries
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
            0.0,
        ];
        self.queue.write_buffer(&self.comp_buf, 0, bytemuck::cast_slice(&comp));

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
        let light_bg = self.light.as_ref().map(|(_, lv, _, _)| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("composite-bg"),
                layout: &self.composite_bg_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.comp_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&scene_view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.samp_linear) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(lv) },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&self.samp_linear) },
                ],
            })
        });

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
