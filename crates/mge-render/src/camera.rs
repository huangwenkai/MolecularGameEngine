//! 2D 相机：跟随、缩放、震屏
use glam::{Mat4, Vec2};
use mge_core::rng::Rng;

pub struct Camera {
    pub center: Vec2,
    pub zoom: f32,
    pub viewport: (f32, f32),
    /// 震屏强度倍率（系统设置，0 = 关闭震动）
    pub shake_scale: f32,
    shake: f32,
    #[allow(dead_code)]
    shake_rng: Rng,
    shake_off: Vec2,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            center: Vec2::ZERO,
            zoom: 3.0,
            viewport: (640.0, 360.0),
            shake_scale: 1.0,
            shake: 0.0,
            shake_rng: Rng::from_entropy(),
            shake_off: Vec2::ZERO,
        }
    }
}

impl Camera {
    pub fn set_viewport(&mut self, w: f32, h: f32) {
        self.viewport = (w, h);
    }

    pub fn add_shake(&mut self, amount: f32) {
        let s = amount * self.shake_scale.max(0.0);
        self.shake = (self.shake + s).min(12.0);
    }

    /// 每帧衰减震屏并更新偏移
    pub fn update(&mut self) {
        self.shake *= 0.88;
        if self.shake < 0.05 {
            self.shake = 0.0;
            self.shake_off = Vec2::ZERO;
        } else {
            let s = self.shake;
            self.shake_off = Vec2::new(
                self.shake_rng.range_f32(-s, s),
                self.shake_rng.range_f32(-s, s),
            );
        }
    }

    pub fn top_left(&self) -> Vec2 {
        self.center - Vec2::new(self.viewport.0, self.viewport.1) * 0.5 + self.shake_off
    }

    pub fn view_proj(&self) -> Mat4 {
        let tl = self.top_left();
        // 世界 y 向下：把上边界映射到 NDC +1
        Mat4::orthographic_rh(
            tl.x,
            tl.x + self.viewport.0,
            tl.y + self.viewport.1,
            tl.y,
            -1.0,
            1.0,
        )
    }

    pub fn world_to_screen(&self, p: Vec2) -> Vec2 {
        (p - self.top_left()) * self.zoom
    }

    pub fn screen_to_world(&self, p: Vec2) -> Vec2 {
        self.top_left() + p / self.zoom
    }
}
