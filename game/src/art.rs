//! 程序化美术：角色/武器/火把等部件贴图运行时生成
//! （地形已并入像素世界纹理，不再需要 Tile 贴图）
use image::{Rgba, RgbaImage};
use mge_render::AtlasBuilder;

pub struct Art {
    pub regions: std::collections::HashMap<String, mge_render::Region>,
}

/// 生成全部贴图并构建图集
pub fn build(ctx_renderer: &mut mge_render::renderer::Renderer) -> Art {
    let mut b = AtlasBuilder::new(1024);

    // 基础白色块（角色/部件/放置预览染色用）
    let mut white = RgbaImage::new(16, 16);
    for p in white.pixels_mut() {
        *p = Rgba([255, 255, 255, 255]);
    }
    b.add("white", &white);

    // 剑（指向右，握把在左）
    let mut sword = RgbaImage::new(22, 7);
    for y in 2..5 {
        for x in 7..20 {
            let t = (x - 7) as f32 / 13.0;
            let v = (200.0 + 55.0 * t) as u8;
            sword.put_pixel(x, y, Rgba([v, v, (v as f32 * 1.02) as u8, 255]));
        }
    }
    sword.put_pixel(19, 3, Rgba([240, 240, 245, 255]));
    sword.put_pixel(20, 3, Rgba([240, 240, 245, 255]));
    for y in 0..7 {
        sword.put_pixel(6, y, Rgba([190, 150, 60, 255])); // 护手
    }
    for x in 0..6 {
        sword.put_pixel(x, 3, Rgba([110, 80, 50, 255])); // 握把
    }
    b.add("sword", &sword);

    // 火把
    let mut torch = RgbaImage::new(8, 12);
    for y in 4..12 {
        for x in 3..5 {
            torch.put_pixel(x, y, Rgba([130, 95, 55, 255]));
        }
    }
    for (x, y, c) in [
        (3, 1, [255, 200, 60, 255]), (4, 1, [255, 200, 60, 255]),
        (2, 2, [255, 140, 30, 255]), (3, 2, [255, 220, 90, 255]),
        (4, 2, [255, 220, 90, 255]), (5, 2, [255, 140, 30, 255]),
        (3, 3, [255, 160, 40, 255]), (4, 3, [255, 160, 40, 255]),
    ] {
        torch.put_pixel(x, y, Rgba(c));
    }
    b.add("torch", &torch);

    // 木桩训练假人
    let mut dummy = RgbaImage::new(16, 20);
    for y in 0..20 {
        for x in 6..10 {
            dummy.put_pixel(x, y, Rgba([150, 108, 58, 255]));
        }
    }
    for x in 1..15 {
        for y in 7..9 {
            dummy.put_pixel(x, y, Rgba([170, 126, 70, 255]));
        }
    }
    for y in 1..6 {
        for x in 5..11 {
            dummy.put_pixel(x, y, Rgba([220, 210, 190, 255]));
        }
    }
    dummy.put_pixel(6, 2, Rgba([80, 60, 40, 255]));
    dummy.put_pixel(9, 2, Rgba([80, 60, 40, 255]));
    for x in 3..13 {
        dummy.put_pixel(x, 19, Rgba([120, 88, 48, 255]));
    }
    b.add("dummy", &dummy);

    let regions = ctx_renderer.set_atlas(b);
    Art { regions }
}

/// 调色板：材质 id → 颜色（像素世界渲染用，含静态地形）
pub fn palette(mats: &mge_sim::Materials) -> Vec<[u8; 4]> {
    let mut out = vec![[0u8, 0, 0, 0]; 256];
    for i in 1..mats.len() {
        let d = mats.def(i as u8);
        out[i] = [d.color.0, d.color.1, d.color.2, 255];
    }
    out
}
