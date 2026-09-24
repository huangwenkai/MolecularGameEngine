//! 精灵批处理：CPU 组装四边形顶点（每精灵 6 顶点），单次 draw 提交
use glam::Vec2;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpriteVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Default)]
pub struct SpriteBatch {
    pub verts: Vec<SpriteVertex>,
}

impl SpriteBatch {
    pub fn clear(&mut self) {
        self.verts.clear();
    }

    /// 推入一个旋转矩形精灵
    pub fn push(&mut self, center: Vec2, size: Vec2, rot: f32, region: &crate::Region, color: [f32; 4]) {
        let (sx, sy) = (size.x * 0.5, size.y * 0.5);
        let (c, s) = (rot.cos(), rot.sin());
        let corners = [(-sx, -sy), (sx, -sy), (sx, sy), (-sx, sy)];
        let uvs = [
            [region.uv0[0], region.uv0[1]],
            [region.uv1[0], region.uv0[1]],
            [region.uv1[0], region.uv1[1]],
            [region.uv0[0], region.uv1[1]],
        ];
        let mut px = [0f32; 4];
        let mut py = [0f32; 4];
        for i in 0..4 {
            let (cx, cy) = corners[i];
            px[i] = center.x + cx * c - cy * s;
            py[i] = center.y + cx * s + cy * c;
        }
        let order = [0usize, 1, 2, 0, 2, 3];
        for i in order {
            self.verts.push(SpriteVertex {
                pos: [px[i], py[i]],
                uv: uvs[i],
                color,
            });
        }
    }

    /// 推入无旋转精灵
    pub fn push_at(&mut self, center: Vec2, size: Vec2, region: &crate::Region, color: [f32; 4]) {
        self.push(center, size, 0.0, region, color);
    }

    pub fn is_empty(&self) -> bool {
        self.verts.is_empty()
    }
}
