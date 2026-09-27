//! 物品图标库：64×64 图标 PNG（game/assets/icons/*.png）→ 图集 Region
//! + egui 纹理缓存（背包/编辑器内显示）。
//! 图标来源：编辑器从外部精灵表（如 Raven Fantasy Icons 64×64）按帧导入，
//! 每个选中的图标保存为独立 PNG，键 = 文件名（不含扩展名）。
use mge_render::renderer::Renderer;
use mge_render::Region;
use std::collections::HashMap;

pub struct IconBank {
    /// 键（文件名去扩展名）→ 图集区域
    pub regions: HashMap<String, Region>,
    /// 原始像素（egui 显示用；64×64 每张，量小无压力）
    pub pixels: HashMap<String, image::RgbaImage>,
    /// egui 纹理缓存
    tex: HashMap<String, egui::TextureHandle>,
    /// 是否已完成启动扫描
    pub loaded: bool,
}

impl Default for IconBank {
    fn default() -> Self {
        Self {
            regions: HashMap::new(),
            pixels: HashMap::new(),
            tex: HashMap::new(),
            loaded: false,
        }
    }
}

impl IconBank {
    /// 启动时扫描 game/assets/icons/*.png，全部上传图集
    pub fn load_dir(&mut self, renderer: &mut Renderer) {
        let dir = crate::project::dir_of("icons");
        let _ = std::fs::create_dir_all(&dir);
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map(|x| x == "png").unwrap_or(false) {
                    files.push(p);
                }
            }
        }
        files.sort();
        let mut n = 0;
        for p in files {
            let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else { continue };
            let Ok(img) = image::open(&p) else { continue };
            let img = img.to_rgba8();
            self.register(renderer, stem, img);
            n += 1;
        }
        self.loaded = true;
        tracing::info!("图标库就绪（{} 个，assets/icons）", n);
    }

    /// 注册单个图标（上传图集 + 缓存像素；同键覆盖）
    pub fn register(&mut self, renderer: &mut Renderer, key: &str, img: image::RgbaImage) {
        let (w, h) = img.dimensions();
        let Some((x, y)) = renderer.atlas_alloc(w, h) else {
            tracing::warn!("图集空间不足，图标 {key} 未上传");
            return;
        };
        renderer.upload_atlas(x, y, w, h, img.as_raw());
        let s = renderer.atlas_size() as f32;
        self.tex.remove(key);
        let region = Region {
            uv0: [x as f32 / s, y as f32 / s],
            uv1: [(x + w) as f32 / s, (y + h) as f32 / s],
            size: [w as f32, h as f32],
        };
        self.regions.insert(key.to_string(), region);
        self.pixels.insert(key.to_string(), img);
    }

    /// 全部图标键（排序稳定，供选择器展示）
    pub fn keys(&self) -> Vec<String> {
        let mut k: Vec<String> = self.regions.keys().cloned().collect();
        k.sort();
        k
    }

    /// 精灵图第 i 帧在选择器中的像素缓存键（不入图集）
    pub fn sheet_key(i: usize) -> String {
        format!("@sheet:{i}")
    }

    /// 仅缓存像素（选择器预览用；不入图集、不产生 Region）
    pub fn store_pixels(&mut self, key: &str, img: image::RgbaImage) {
        self.pixels.insert(key.to_string(), img);
    }

    /// egui 图像（懒加载纹理；背包格/编辑器选择器共用）
    pub fn egui_image(
        &mut self,
        ctx: &egui::Context,
        key: &str,
        size: f32,
    ) -> Option<egui::Image<'static>> {
        let tex = match self.tex.get(key) {
            Some(t) => t.clone(),
            None => {
                let px = self.pixels.get(key)?;
                let img = egui::ColorImage::from_rgba_unmultiplied(
                    [px.width() as usize, px.height() as usize],
                    px.as_raw(),
                );
                let t = ctx.load_texture(format!("icon:{key}"), img, egui::TextureOptions::NEAREST);
                self.tex.insert(key.to_string(), t.clone());
                t
            }
        };
        Some(
            egui::Image::new(egui::load::SizedTexture::new(tex.id(), egui::vec2(size, size))),
        )
    }
}
