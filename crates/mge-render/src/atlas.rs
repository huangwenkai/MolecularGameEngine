//! 纹理图集：运行时货架式打包（程序化生成的贴图都塞进一张大图）
use image::RgbaImage;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub struct Region {
    /// 左上 uv
    pub uv0: [f32; 2],
    /// 右下 uv
    pub uv1: [f32; 2],
    /// 像素尺寸
    pub size: [f32; 2],
}

pub struct AtlasBuilder {
    size: u32,
    cursor: (u32, u32),
    row_h: u32,
    entries: HashMap<String, Region>,
    pixels: Vec<u8>,
}

impl AtlasBuilder {
    pub fn new(size: u32) -> Self {
        Self {
            size,
            cursor: (1, 1),
            row_h: 0,
            entries: HashMap::new(),
            pixels: vec![0; (size * size * 4) as usize],
        }
    }

    /// 添加一张贴图，返回其在图集中的区域
    pub fn add(&mut self, name: &str, img: &RgbaImage) -> Region {
        let (iw, ih) = img.dimensions();
        assert!(iw <= self.size && ih <= self.size, "atlas image too large");
        if self.cursor.0 + iw + 1 > self.size {
            self.cursor.0 = 1;
            self.cursor.1 += self.row_h;
            self.row_h = 0;
        }
        let (cx, cy) = self.cursor;
        for y in 0..ih {
            let src = (y * iw * 4) as usize;
            let dst = (((cy + y) * self.size + cx) * 4) as usize;
            self.pixels[dst..dst + (iw * 4) as usize]
                .copy_from_slice(&img.as_raw()[src..src + (iw * 4) as usize]);
        }
        let s = self.size as f32;
        let region = Region {
            uv0: [cx as f32 / s, cy as f32 / s],
            uv1: [(cx + iw) as f32 / s, (cy + ih) as f32 / s],
            size: [iw as f32, ih as f32],
        };
        self.entries.insert(name.to_string(), region);
        self.cursor.0 += iw + 1;
        self.row_h = self.row_h.max(ih + 1);
        region
    }

    pub fn get(&self, name: &str) -> Option<Region> {
        self.entries.get(name).copied()
    }

    /// 当前分配游标（供 Renderer 延续运行时分配）
    pub fn cursor(&self) -> ((u32, u32), u32) {
        (self.cursor, self.row_h)
    }

    /// 打包完成：输出扁平化 RGBA 数据
    pub fn flatten(self) -> (Vec<u8>, u32, HashMap<String, Region>) {
        (self.pixels, self.size, self.entries)
    }
}
