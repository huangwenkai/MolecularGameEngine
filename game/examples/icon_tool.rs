//! 图标素材小工具（开发用，不进游戏）：
//!   crop <sheet> <frame_index> <out.png> [cell=64]   — 导出单帧
//!   band <sheet> <row0> <rows> <out.png> [cell=64]   — 导出横向浏览条
//!   montage <out.png> <img...>                       — 横向拼接
use image::GenericImageView;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a[1].as_str() {
        "crop" => {
            let sheet = image::open(&a[2]).expect("open sheet").to_rgba8();
            let (w, _) = sheet.dimensions();
            let cell: u32 = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(64);
            let cols = (w / cell).max(1);
            let i: u32 = a[3].parse().unwrap();
            let (sx, sy) = ((i % cols) * cell, (i / cols) * cell);
            image::imageops::crop_imm(&sheet, sx, sy, cell, cell)
                .to_image()
                .save(&a[4])
                .unwrap();
        }
        "band" => {
            let sheet = image::open(&a[2]).expect("open sheet").to_rgba8();
            let (w, h) = sheet.dimensions();
            let cell: u32 = a.get(6).and_then(|s| s.parse().ok()).unwrap_or(64);
            let row0: u32 = a[3].parse().unwrap();
            let rows: u32 = a[4].parse().unwrap();
            let mut out = image::RgbaImage::new(w, rows * cell);
            for r in 0..rows {
                let sy = (row0 + r) * cell;
                if sy + cell > h {
                    break;
                }
                let strip = image::imageops::crop_imm(&sheet, 0, sy, w, cell).to_image();
                image::imageops::replace(&mut out, &strip, 0, (r * cell) as i64);
            }
            out.save(&a[5]).unwrap();
        }
        "tint" => {
            // tint <in.png> <out.png> <r> <g> <b> — 通道乘法调色
            let im = image::open(&a[2]).unwrap().to_rgba8();
            let m: [f32; 3] = [
                a[4].parse().unwrap(),
                a[5].parse().unwrap(),
                a[6].parse().unwrap(),
            ];
            let mut out = im.clone();
            for p in out.pixels_mut() {
                if p.0[3] > 0 {
                    p.0[0] = ((p.0[0] as f32 * m[0]).min(255.0)) as u8;
                    p.0[1] = ((p.0[1] as f32 * m[1]).min(255.0)) as u8;
                    p.0[2] = ((p.0[2] as f32 * m[2]).min(255.0)) as u8;
                }
            }
            out.save(&a[3]).unwrap();
        }
        _ => eprintln!("usage: crop|band|tint|montage ..."),
    }
}
