//! 人物形象：部件贴图数据层（F1 人物页逐像素编辑）
//! 每个部位是一张小贴图（默认纯色/带五官），编辑器逐像素修改后上传图集实时生效；
//! 保存到 assets/character/*.png，下次启动自动加载。
use image::{Rgba, RgbaImage};

pub const CHAR_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/character/");

/// 部件定义（尺寸与默认色）
pub struct PartDef {
    /// 图集 key（渲染查 regions 用）
    pub key: &'static str,
    pub label: &'static str,
    pub w: u32,
    pub h: u32,
    /// 默认填充色
    pub color: [u8; 3],
}

pub const PARTS: &[PartDef] = &[
    PartDef { key: "char_head",  label: "头", w: 8, h: 8, color: [224, 173, 140] },
    PartDef { key: "char_hair",  label: "发", w: 8, h: 4, color: [89, 61, 31] },
    PartDef { key: "char_torso", label: "身", w: 8, h: 9, color: [61, 120, 199] },
    PartDef { key: "char_arm",   label: "臂", w: 4, h: 8, color: [224, 173, 140] },
    PartDef { key: "char_leg",   label: "腿", w: 4, h: 8, color: [66, 66, 87] },
    PartDef { key: "char_shoe",  label: "鞋", w: 4, h: 2, color: [51, 51, 56] },
];

/// 单个部件的贴图 + 图集坐标（art::build 打包后填写）
pub struct PartTex {
    pub key: &'static str,
    pub img: RgbaImage,
    /// 图集像素坐标
    pub ax: u32,
    pub ay: u32,
}

/// 人物皮肤（全部部件）
pub struct Skin {
    pub parts: Vec<PartTex>,
}

impl Skin {
    pub fn get(&self, key: &str) -> Option<&PartTex> {
        self.parts.iter().find(|p| p.key == key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut PartTex> {
        self.parts.iter_mut().find(|p| p.key == key)
    }
}

fn file_name(key: &str) -> String {
    format!("{}.png", key.trim_start_matches("char_"))
}

/// 默认部件贴图：纯色填充；头部带眼睛（默认朝右，朝左渲染时镜像）
pub fn default_img(def: &PartDef) -> RgbaImage {
    let mut img = RgbaImage::new(def.w, def.h);
    let c = def.color;
    for p in img.pixels_mut() {
        *p = Rgba([c[0], c[1], c[2], 255]);
    }
    if def.key == "char_head" {
        let (w, h) = (def.w, def.h);
        let eye = Rgba([56, 42, 38, 255]);
        img.put_pixel(w * 5 / 8, h * 4 / 8, eye);
        img.put_pixel((w * 6 / 8).min(w - 1), h * 4 / 8, eye);
    }
    img
}

/// 加载皮肤：assets/character/*.png 优先（尺寸须匹配），缺失/不符用默认
pub fn load() -> Skin {
    let parts = PARTS
        .iter()
        .map(|def| {
            let path = format!("{CHAR_DIR}{}", file_name(def.key));
            let img = image::open(&path)
                .ok()
                .map(|i| i.to_rgba8())
                .filter(|i| i.dimensions() == (def.w, def.h))
                .unwrap_or_else(|| default_img(def));
            PartTex { key: def.key, img, ax: 0, ay: 0 }
        })
        .collect();
    Skin { parts }
}

/// 保存全部部件贴图
pub fn save(skin: &Skin) -> Result<(), String> {
    std::fs::create_dir_all(CHAR_DIR).map_err(|e| e.to_string())?;
    for p in &skin.parts {
        let path = format!("{CHAR_DIR}{}", file_name(p.key));
        p.img.save(&path).map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(())
}

/// 重置部件为默认形象
pub fn reset_part(pt: &mut PartTex) {
    if let Some(def) = PARTS.iter().find(|d| d.key == pt.key) {
        pt.img = default_img(def);
    }
}
