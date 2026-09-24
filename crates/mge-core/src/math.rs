//! 2D 数学工具：AABB、插值等
use glam::Vec2;

/// 轴对齐包围盒（世界坐标，y 向下）
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb {
    pub fn new(center: Vec2, half: Vec2) -> Self {
        Self { min: center - half, max: center + half }
    }

    pub fn from_points(a: Vec2, b: Vec2) -> Self {
        Self { min: a.min(b), max: a.max(b) }
    }

    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    pub fn half(&self) -> Vec2 {
        (self.max - self.min) * 0.5
    }

    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    pub fn intersects(&self, o: &Aabb) -> bool {
        self.min.x <= o.max.x && self.max.x >= o.min.x && self.min.y <= o.max.y && self.max.y >= o.min.y
    }

    pub fn translate(mut self, v: Vec2) -> Self {
        self.min += v;
        self.max += v;
        self
    }

    /// 向 target 逼近，每步最多 delta
    pub fn approach(cur: f32, target: f32, delta: f32) -> f32 {
        if cur < target {
            (cur + delta).min(target)
        } else {
            (cur - delta).max(target)
        }
    }
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}
