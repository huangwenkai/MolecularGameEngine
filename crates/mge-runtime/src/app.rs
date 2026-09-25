//! 引擎装配与主循环：App trait + 窗口/无头两种驱动模式
use glam::Vec2;
use mge_platform::input::InputState;
use mge_platform::timer::Stepper;
use mge_render::batcher::SpriteBatch;
use mge_render::camera::Camera;
use mge_render::gpu;
use mge_render::renderer::{FrameParams, Renderer};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

/// 游戏实现此 trait 接入引擎
pub trait App {
    /// 引擎就绪（渲染器可配置贴图/调色板/世界纹理）
    fn init(&mut self, ctx: &mut EngineCtx);
    /// 固定逻辑步（1/60s）
    fn tick(&mut self, ctx: &mut EngineCtx);
    /// 每显示帧：向批处理器提交精灵
    fn render(&mut self, ctx: &mut EngineCtx);
}

pub struct EngineCtx<'a> {
    pub input: &'a mut InputState,
    pub renderer: &'a mut Renderer,
    pub camera: &'a mut Camera,
    pub atlas_batch: &'a mut SpriteBatch,
    pub world_batch: &'a mut SpriteBatch,
    pub screen: (f32, f32),
    pub frame: u64,
    pub headless: bool,
    pub sky_color: [f32; 3],
    pub ambient: f32,
    /// Bloom 辉光强度（0 关闭，默认 0.45）
    pub bloom: f32,
    /// 窗口模式下的 egui 上下文（App::render 中构建 UI 面板；无头模式为 None）
    pub egui: Option<&'a egui::Context>,
    screenshot_req: Option<String>,
}

impl EngineCtx<'_> {
    pub fn request_screenshot(&mut self, path: impl Into<String>) {
        self.screenshot_req = Some(path.into());
    }

    pub fn screen_to_world(&self, p: Vec2) -> Vec2 {
        self.camera.screen_to_world(p)
    }
}

pub struct Engine {
    headless: bool,
    size: (u32, u32),
    pub renderer: Option<Renderer>,
    camera: Camera,
    input: InputState,
    stepper: Stepper,
    atlas_batch: SpriteBatch,
    world_batch: SpriteBatch,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    instance: Option<wgpu::Instance>,
    offscreen: Option<wgpu::Texture>,
    egui_state: Option<egui_winit::State>,
    egui_ctx: Option<egui::Context>,
    last: Instant,
    frame: u64,
    pending_shot: Option<String>,
}

impl Engine {
    pub fn new(size: (u32, u32)) -> Self {
        Self {
            headless: false,
            size,
            renderer: None,
            camera: Camera::default(),
            input: InputState::new(),
            stepper: Stepper::new(),
            atlas_batch: SpriteBatch::default(),
            world_batch: SpriteBatch::default(),
            window: None,
            surface: None,
            surface_config: None,
            instance: None,
            offscreen: None,
            egui_state: None,
            egui_ctx: None,
            last: Instant::now(),
            frame: 0,
            pending_shot: None,
        }
    }

    fn ctx(&mut self) -> EngineCtx<'_> {
        EngineCtx {
            input: &mut self.input,
            renderer: self.renderer.as_mut().expect("renderer not ready"),
            camera: &mut self.camera,
            atlas_batch: &mut self.atlas_batch,
            world_batch: &mut self.world_batch,
            screen: (self.size.0 as f32, self.size.1 as f32),
            frame: self.frame,
            headless: self.headless,
            sky_color: [0.5, 0.7, 0.9],
            ambient: 1.0,
            bloom: 0.45,
            egui: None,
            screenshot_req: None,
        }
    }

    fn setup_window(&mut self, win: Arc<Window>) {
        let instance = self.instance.take().expect("instance");
        let surface = instance.create_surface(win.clone()).expect("create surface");
        let (gpu, adapter) = gpu::create_device(&instance, Some(&surface));
        let caps = surface.get_capabilities(&adapter);
        let alpha = caps
            .alpha_modes
            .iter()
            .copied()
            .find(|a| matches!(a, wgpu::CompositeAlphaMode::Auto))
            .unwrap_or(caps.alpha_modes[0]);
        let sz = win.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: gpu.format,
            width: sz.width.max(1),
            height: sz.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: alpha,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&gpu.device, &config);
        self.size = (sz.width, sz.height);
        let renderer = Renderer::new(gpu);
        self.camera
            .set_viewport(self.size.0 as f32 / self.camera.zoom, self.size.1 as f32 / self.camera.zoom);
        self.renderer = Some(renderer);
        self.surface = Some(surface);
        self.surface_config = Some(config);
        self.window = Some(win.clone());
        // ---- egui（特效编辑器等 UI）----
        let ectx = egui::Context::default();
        self.egui_ctx = Some(ectx.clone());
        self.egui_state = Some(egui_winit::State::new(
            ectx.clone(),
            egui::viewport::ViewportId::ROOT,
            &win,
            Some(win.scale_factor() as f32),
            None,
            None,
        ));
        self.last = Instant::now();
    }

    fn resize(&mut self, w: u32, h: u32) {
        self.size = (w.max(1), h.max(1));
        if let (Some(surface), Some(config)) = (&self.surface, &mut self.surface_config) {
            config.width = self.size.0;
            config.height = self.size.1;
            if let Some(r) = &self.renderer {
                surface.configure(&r.device, config);
            }
        }
        self.camera
            .set_viewport(self.size.0 as f32 / self.camera.zoom, self.size.1 as f32 / self.camera.zoom);
    }

    fn render_once(&mut self, app: &mut dyn App) {
        let shot_path = self.pending_shot.take();
        self.atlas_batch.clear();
        self.world_batch.clear();

        // ---- egui：取输入并开始 pass（窗口模式；UI 在 app.render 中构建）----
        let mut ectx: Option<egui::Context> = None;
        if self.egui_state.is_some() {
            let state = self.egui_state.as_mut().unwrap();
            let win = self.window.as_ref().unwrap().clone();
            let raw = state.take_egui_input(&win);
            let c = self.egui_ctx.clone().expect("egui ctx");
            c.begin_pass(raw);
            ectx = Some(c);
        }

        let mut ctx = self.ctx();
        ctx.egui = ectx.as_ref().map(|c| c as &egui::Context);
        app.render(&mut ctx);
        let (sky, amb, ctx_bloom) = (ctx.sky_color, ctx.ambient, ctx.bloom);
        drop(ctx);

        // ---- egui：结束 pass → UI 网格 ----
        let mut egui_frame: Option<(egui::TexturesDelta, Vec<egui::epaint::ClippedPrimitive>, f32)> =
            None;
        if let Some(c) = &ectx {
            let state = self.egui_state.as_mut().unwrap();
            let win = self.window.as_ref().unwrap().clone();
            let out = c.end_pass();
            state.handle_platform_output(&win, out.platform_output);
            let jobs = c.tessellate(out.shapes, out.pixels_per_point);
            egui_frame = Some((out.textures_delta, jobs, out.pixels_per_point));
        }

        let Some(renderer) = self.renderer.as_mut() else { return };
        if let (Some(surface), Some(config)) = (&self.surface, &self.surface_config) {
            // ---- 窗口模式 ----
            let st = match surface.get_current_texture() {
                Ok(t) => t,
                Err(_) => {
                    surface.configure(&renderer.device, config);
                    return;
                }
            };
            let img = renderer.draw_frame(
                &st.texture,
                FrameParams {
                    camera: &self.camera,
                    sky_color: sky,
                    ambient: amb,
                    bloom: ctx_bloom,
                },
                &self.atlas_batch.verts,
                &self.world_batch.verts,
                false,
            );
            let _ = img;
            // UI 叠加在场景之上
            if let Some((td, jobs, ppp)) = egui_frame {
                let view = st.texture.create_view(&Default::default());
                renderer.egui_paint(
                    &view,
                    td,
                    &jobs,
                    (config.width as f32, config.height as f32),
                    ppp,
                );
            }
            st.present();
        } else {
            // ---- 无头模式：离屏渲染 ----
            let need_new = self
                .offscreen
                .as_ref()
                .map(|t| t.width() != self.size.0 || t.height() != self.size.1)
                .unwrap_or(true);
            if need_new {
                self.offscreen = Some(renderer.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("offscreen"),
                    size: wgpu::Extent3d { width: self.size.0, height: self.size.1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: renderer.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                }));
            }
            let tex = self.offscreen.as_ref().unwrap();
            let img = renderer.draw_frame(
                tex,
                FrameParams {
                    camera: &self.camera,
                    sky_color: sky,
                    ambient: amb,
                    bloom: ctx_bloom,
                },
                &self.atlas_batch.verts,
                &self.world_batch.verts,
                shot_path.is_some(),
            );
            if let (Some(img), Some(path)) = (img, shot_path) {
                if let Some(dir) = std::path::Path::new(&path).parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                match img.save(&path) {
                    Ok(_) => tracing::info!("screenshot saved: {path}"),
                    Err(e) => tracing::error!("screenshot save failed: {e}"),
                }
            }
        }
    }

    fn pump_ticks(&mut self, app: &mut dyn App) {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        let steps = self.stepper.step(dt);
        for _ in 0..steps {
            self.frame += 1;
            let mut ctx = self.ctx();
            app.tick(&mut ctx);
            let req = ctx.screenshot_req.take();
            drop(ctx);
            if req.is_some() {
                self.pending_shot = req;
            }
            self.input.end_frame();
            self.camera.update();
        }
    }

    /// 窗口模式主循环
    pub fn run_windowed<A: App>(&mut self, app: &mut A) {
        self.headless = false;
        self.instance = Some(gpu::create_instance());
        let el = EventLoop::new().expect("create event loop");
        let mut handler = Handler { engine: self, app };
        el.run_app(&mut handler).expect("run event loop");
    }

    /// 无头模式：跑固定 tick 数后退出（自测/自动化验收）
    pub fn run_headless<A: App>(&mut self, app: &mut A, frames: u64) {
        self.headless = true;
        tracing::info!("headless: creating instance");
        let instance = gpu::create_instance();
        let (gpu, _) = gpu::create_device(&instance, None);
        tracing::info!("headless: creating renderer");
        let renderer = Renderer::new(gpu);
        tracing::info!("headless: renderer ready");
        self.camera
            .set_viewport(self.size.0 as f32 / self.camera.zoom, self.size.1 as f32 / self.camera.zoom);
        self.renderer = Some(renderer);
        {
            let mut ctx = self.ctx();
            app.init(&mut ctx);
        }
        tracing::info!("headless: init done, ticking");
        for _ in 0..frames {
            self.frame += 1;
            let mut ctx = self.ctx();
            app.tick(&mut ctx);
            let req = ctx.screenshot_req.take();
            drop(ctx);
            if req.is_some() {
                self.pending_shot = req;
            }
            self.input.end_frame();
            self.camera.update();
            self.render_once(app);
        }
    }
}

struct Handler<'a, A: App> {
    engine: &'a mut Engine,
    app: &'a mut A,
}

impl<A: App> ApplicationHandler for Handler<'_, A> {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.engine.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("MolecularGameEngine")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0f32, 720.0f32))
            .with_resizable(true);
        let win = el.create_window(attrs).expect("create window");
        let win = Arc::new(win);
        self.engine.setup_window(win);
        let mut ctx = self.engine.ctx();
        self.app.init(&mut ctx);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // ---- egui 优先消费输入；UI 悬停/文本输入时不转发给游戏 ----
        let mut wants_ptr = false;
        let mut wants_key = false;
        if let (Some(state), Some(win)) = (&mut self.engine.egui_state, &self.engine.window) {
            let _ = state.on_window_event(win, &event);
            wants_ptr = state.egui_ctx().wants_pointer_input();
            wants_key = state.egui_ctx().wants_keyboard_input();
        }
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(sz) => self.engine.resize(sz.width, sz.height),
            WindowEvent::KeyboardInput { event, .. } => {
                if !wants_key {
                    self.engine.input.key_event(event.physical_key, event.state);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if !wants_ptr {
                    self.engine.input.mouse_event(button, state);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.engine.input.mouse_pos = Vec2::new(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if !wants_ptr {
                    let d = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                        winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32,
                    };
                    self.engine.input.mouse_wheel += d;
                }
            }
            WindowEvent::RedrawRequested => {
                self.engine.pump_ticks(self.app);
                self.engine.render_once(self.app);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(w) = &self.engine.window {
            w.request_redraw();
        }
    }
}
