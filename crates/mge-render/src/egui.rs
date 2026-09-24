//! egui 集成渲染器：即时模式 UI 网格绘制（wgpu 原生实现，不依赖 egui-wgpu，
//! 避免其 wgpu 版本绑定与项目 wgpu 24 冲突）
use egui::epaint;
use std::collections::HashMap;

/// UI 顶点（与 epaint::Vertex 对应，色值预乘 alpha）
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct UiVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

pub struct EguiRenderer {
    pipe: wgpu::RenderPipeline,
    #[allow(dead_code)]
    screen_bg_layout: wgpu::BindGroupLayout,
    tex_bg_layout: wgpu::BindGroupLayout,
    screen_buf: wgpu::Buffer,
    screen_bg: wgpu::BindGroup,
    textures: HashMap<egui::TextureId, (wgpu::Texture, wgpu::TextureView)>,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    vcap: usize,
    icap: usize,
}

impl EguiRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("egui.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!(
                "../../../assets/shaders/egui.wgsl"
            )
            .into()),
        });
        let screen_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("egui-screen-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            }],
        });
        let tex_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("egui-tex-bgl"),
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
        let screen_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("egui-screen"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("egui-screen-bg"),
            layout: &screen_bg_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_buf.as_entire_binding(),
            }],
        });
        let blend = wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING;
        let pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("egui-pipe"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("egui-pl"),
                bind_group_layouts: &[&screen_bg_layout, &tex_bg_layout],
                push_constant_ranges: &[],
            })),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<UiVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 16,
                            shader_location: 2,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let empty = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("egui-empty"),
            size: 16,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipe,
            screen_bg_layout,
            tex_bg_layout,
            screen_buf,
            screen_bg,
            textures: HashMap::new(),
            vbuf: empty.clone(),
            ibuf: empty,
            vcap: 0,
            icap: 0,
        }
    }

    /// 处理 egui 每帧的纹理增删（字体图集等）
    pub fn handle_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        td: egui::TexturesDelta,
    ) {
        for (id, delta) in td.set {
            let img = match delta.image {
                egui::ImageData::Color(img) => img,
            };
            let w = img.width() as u32;
            let h = img.height() as u32;
            let mut data = Vec::with_capacity((w * h * 4) as usize);
            // ColorImage 已是预乘 RGBA（egui 0.32 字体图集同为此格式）
            for c in img.pixels.iter() {
                data.extend_from_slice(&[c.r(), c.g(), c.b(), c.a()]);
            }
            if w == 0 || h == 0 {
                continue;
            }
            let usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST;
            // 已存在则更新（尺寸相同原地写，否则重建）
            if let Some((old_tex, _)) = self.textures.get(&id) {
                let old_tex = old_tex.clone();
                if old_tex.width() == w && old_tex.height() == h {
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &old_tex,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        &data,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(w * 4),
                            rows_per_image: None,
                        },
                        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    );
                    continue;
                }
            }
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("egui-tex"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            let view = tex.create_view(&Default::default());
            self.textures.insert(id, (tex, view));
        }
        for id in td.free {
            self.textures.remove(&id);
        }
    }

    /// 渲染 UI 网格（在已绘制画面上叠加，LoadOp::Load）
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        jobs: &[epaint::ClippedPrimitive],
        screen_px: (f32, f32),
        pixels_per_point: f32,
    ) {
        if jobs.is_empty() {
            return;
        }
        queue.write_buffer(
            &self.screen_buf,
            0,
            bytemuck::cast_slice(&[screen_px.0, screen_px.1, 0.0, 0.0]),
        );

        // ---- 收集并上传网格 ----
        let mut verts: Vec<UiVertex> = Vec::new();
        let mut idxs: Vec<u32> = Vec::new();
        let mut draws = Vec::new();
        for prim in jobs {
            let epaint::Primitive::Mesh(mesh) = &prim.primitive else { continue };
            let Some((_, view)) = self.textures.get(&mesh.texture_id) else { continue };
            let view = view.clone();
            let vbase = verts.len() as u32;
            for v in &mesh.vertices {
                verts.push(UiVertex {
                    pos: [v.pos.x, v.pos.y],
                    uv: [v.uv.x, v.uv.y],
                    color: [
                        v.color.r() as f32 / 255.0,
                        v.color.g() as f32 / 255.0,
                        v.color.b() as f32 / 255.0,
                        v.color.a() as f32 / 255.0,
                    ],
                });
            }
            let icount = mesh.indices.len() as u32;
            idxs.extend(mesh.indices.iter().map(|i| i + vbase));
            // 裁剪矩形（点 → 物理像素）
            let c = &prim.clip_rect;
            let x = (c.min.x * pixels_per_point).floor().max(0.0) as u32;
            let y = (c.min.y * pixels_per_point).floor().max(0.0) as u32;
            let x1 = (c.max.x * pixels_per_point).ceil().min(screen_px.0) as u32;
            let y1 = (c.max.y * pixels_per_point).ceil().min(screen_px.1) as u32;
            if x1 > x && y1 > y && icount > 0 {
                draws.push((view, [x, y, x1 - x, y1 - y], icount));
            }
        }
        if draws.is_empty() {
            return;
        }
        let (vneed, ineed) = (verts.len(), idxs.len());
        if vneed > self.vcap {
            self.vcap = (vneed * 2).max(4096);
            self.vbuf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("egui-vbuf"),
                size: (self.vcap * std::mem::size_of::<UiVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if ineed > self.icap {
            self.icap = (ineed * 2).max(8192);
            self.ibuf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("egui-ibuf"),
                size: (self.icap * 4) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.vbuf, 0, bytemuck::cast_slice(&verts));
        queue.write_buffer(&self.ibuf, 0, bytemuck::cast_slice(&idxs));

        let mut enc = device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("egui") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
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
            pass.set_pipeline(&self.pipe);
            pass.set_bind_group(0, &self.screen_bg, &[]);
            pass.set_vertex_buffer(0, self.vbuf.slice(..));
            pass.set_index_buffer(self.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            let mut voff = 0u32;
            for (view, scissor, icount) in draws {
                pass.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
                pass.set_bind_group(
                    1,
                    &device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("egui-tex-bg"),
                        layout: &self.tex_bg_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::Sampler(
                                    &device.create_sampler(&wgpu::SamplerDescriptor {
                                        address_mode_u: wgpu::AddressMode::ClampToEdge,
                                        address_mode_v: wgpu::AddressMode::ClampToEdge,
                                        mag_filter: wgpu::FilterMode::Linear,
                                        min_filter: wgpu::FilterMode::Linear,
                                        ..Default::default()
                                    }),
                                ),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&view),
                            },
                        ],
                    }),
                    &[],
                );
                pass.draw_indexed(voff..voff + icount, 0, 0..1);
                voff += icount;
            }
        }
        queue.submit([enc.finish()]);
    }
}
