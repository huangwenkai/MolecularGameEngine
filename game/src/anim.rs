//! 序列帧动画系统：PNG 精灵表（Aseprite 可直接导出）切帧 + 帧事件标注 + animations.ron 数据驱动
//! 验收标准：编辑器里加载一张精灵表、标注帧事件并保存，全程不重启；实体按名播放
use image::RgbaImage;
use mge_render::renderer::Renderer;
use mge_render::{AtlasBuilder, Region};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// 动画资源路径由 crate::project 解析（anims_dir / path_of）
pub fn anims_dir() -> std::path::PathBuf {
    crate::project::dir_of("anims")
}

/// 怪物动画状态（由 AI/行为映射）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonAnimState {
    /// 待机（站立）
    Idle,
    /// 移动（走/跑/飞）
    Walk,
    /// 攻击
    Attack,
    /// 受击
    Hurt,
}

/// 递归收集目录（含子目录）下的 PNG
fn collect_png(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_png(&p, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("png") {
            out.push(p);
        }
    }
}

/// 文件名关键字 → 怪物动画名（保留状态后缀：base_state；其余沿用文件名）
fn monster_anim_name(stem: &str) -> String {
    let s = stem.to_ascii_lowercase().replace(' ', "");
    let base = [
        ("skeleton", "mon_skeleton"),
        ("mushroom", "mon_mushroom"),
        ("goblin", "mon_goblin"),
        ("flyingeye", "mon_eyebat"),
        ("eyebat", "mon_eyebat"),
        ("bat", "mon_eyebat"),
        ("slime", "mon_slime"),
    ]
    .iter()
    .find(|(k, _)| s.contains(k))
    .map(|(_, v)| *v);
    let Some(base) = base else {
        return s.replace([' ', '-'], "_");
    };
    // 状态后缀（LuizMelo 命名：Idle/Run/Walk/Attack/Take Hit/Flight/Death/Shield）
    let state = [
        ("idle", "idle"),
        ("walk", "walk"),
        ("run", "run"),
        ("attack", "attack"),
        ("takehit", "hurt"),
        ("take_hit", "hurt"),
        ("hurt", "hurt"),
        ("flight", "fly"),
        ("fly", "fly"),
        ("death", "death"),
        ("shield", "shield"),
    ]
    .iter()
    .find(|(k, _)| s.contains(k))
    .map(|(_, v)| *v);
    match state {
        Some(st) => format!("{base}_{st}"),
        None => base.to_string(),
    }
}


fn default_loop() -> bool {
    true
}

/// 动画定义（数据驱动，可热重载）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimDef {
    pub name: String,
    /// 精灵表 PNG 文件名（相对 assets/anims/）
    pub sheet: String,
    pub frame_w: u32,
    pub frame_h: u32,
    /// 每帧时长（秒）；数量少于帧数时末尾帧沿用最后一个值
    #[serde(default)]
    pub frame_times: Vec<f32>,
    /// 帧事件：(帧号, 事件名)，如 (2, "hit")
    #[serde(default)]
    pub events: Vec<(u32, String)>,
    #[serde(default = "default_loop")]
    pub r#loop: bool,
}

#[derive(Default)]
pub struct AnimBank {
    pub defs: HashMap<String, AnimDef>,
    /// 各动画帧数
    counts: HashMap<String, u32>,
    /// 帧区域，key: "anim:{name}:{i}"
    pub frames: HashMap<String, Region>,
}

impl AnimBank {
    /// 读取 animations.ron（缺失/损坏时为空库）
    pub fn load() -> Self {
        ensure_demo_assets();
        let mut bank = Self::default();
        if let Ok(s) = std::fs::read_to_string(crate::project::path_of("data/animations.ron")) {
            match ron::from_str::<Vec<AnimDef>>(&s) {
                Ok(defs) => {
                    for d in defs {
                        bank.counts.insert(d.name.clone(), 0);
                        bank.defs.insert(d.name.clone(), d);
                    }
                }
                Err(e) => tracing::warn!("animations.ron 解析失败: {e}"),
            }
        }
        bank
    }

    /// init 阶段：把全部动画的帧切图打包进图集
    pub fn pack(&mut self, b: &mut AtlasBuilder) {
        let names: Vec<String> = self.defs.keys().cloned().collect();
        for name in names {
            let Some(def) = self.defs.get(&name).cloned() else { continue };
            if def.sheet.is_empty() {
                continue; // 未指定精灵表的动画跳过
            }
            match load_sheet_frames(&def) {
                Ok(frames) => {
                    for (i, img) in frames.iter().enumerate() {
                        let key = format!("anim:{name}:{i}");
                        let region = b.add(&key, img);
                        self.frames.insert(key, region);
                    }
                    self.counts.insert(name, frames.len() as u32);
                }
                Err(e) => tracing::warn!("动画 {name} 精灵表加载失败: {e}"),
            }
        }
    }

    pub fn def(&self, name: &str) -> Option<&AnimDef> {
        self.defs.get(name)
    }

    pub fn frame_count(&self, name: &str) -> u32 {
        self.counts.get(name).copied().unwrap_or(0)
    }

    pub fn frame_region(&self, name: &str, i: u32) -> Option<Region> {
        self.frames.get(&format!("anim:{name}:{i}")).copied()
    }

    /// 删除动画定义（帧区域保留：图集内容仍可复用）
    pub fn remove(&mut self, name: &str) {
        self.defs.remove(name);
        self.counts.remove(name);
    }

    /// 运行时注册动画（编辑器新增/自测）：帧上传图集空闲区，立即生效
    pub fn register_runtime(
        &mut self,
        def: AnimDef,
        frames: &[RgbaImage],
        renderer: &mut Renderer,
    ) -> Result<(), String> {
        if frames.is_empty() {
            return Err("没有帧".into());
        }
        let s = renderer.atlas_size() as f32;
        for (i, img) in frames.iter().enumerate() {
            let (w, h) = img.dimensions();
            let Some((x, y)) = renderer.atlas_alloc(w, h) else {
                return Err("图集空间不足".into());
            };
            renderer.upload_atlas(x, y, w, h, img.as_raw());
            let region = Region {
                uv0: [x as f32 / s, y as f32 / s],
                uv1: [(x + w) as f32 / s, (y + h) as f32 / s],
                size: [w as f32, h as f32],
            };
            self.frames.insert(format!("anim:{}:{i}", def.name), region);
        }
        self.counts.insert(def.name.clone(), frames.len() as u32);
        self.defs.insert(def.name.clone(), def);
        Ok(())
    }

    /// 取怪物当前状态动画帧（回退链：请求状态 → idle/fly → None）
    pub fn monster_frame(
        &self,
        kind: &crate::monsters::Kind,
        state: MonAnimState,
        t: f32,
    ) -> Option<Region> {
        let base = kind.anim_base()?;
        let sname = match state {
            MonAnimState::Idle => "idle",
            MonAnimState::Walk => "walk",
            MonAnimState::Attack => "attack",
            MonAnimState::Hurt => "hurt",
        };
        let mut key = format!("{base}_{sname}");
        if !self.counts.contains_key(&key) {
            key = if base == "mon_eyebat" {
                format!("{base}_fly")
            } else {
                format!("{base}_idle")
            };
            if !self.counts.contains_key(&key) {
                return None;
            }
        }
        let n = self.counts.get(&key).copied().unwrap_or(0);
        if n == 0 {
            return None;
        }
        let i = ((t * 10.0).floor() as u32) % n;
        self.frames.get(&format!("anim:{key}:{i}")).copied()
    }

    /// 批量导入精灵表：递归扫描目录下 PNG，按帧尺寸切帧，最近邻缩放到 64×64
    /// 并重排为紧凑网格（避免大序列条塞爆图集），注册动画。
    /// 文件名按关键字映射到怪物动画名（skeleton/mushroom/goblin/eyebat|bat）。
    pub fn import_sheets(&mut self, dir: &str, fw: u32, fh: u32) -> (usize, Vec<String>) {
        const OUT: u32 = 64; // 导出帧尺寸（怪物在屏上约 18px，64px 足够清晰）
        let mut ok = Vec::new();
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        collect_png(std::path::Path::new(dir), &mut files);
        files.sort();
        for src in files {
            let stem = src
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("sheet")
                .to_ascii_lowercase();
            // 怪物名在父目录（如 "Skeleton/Attack.png"）→ 拼接后映射
            let parent = src
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let name = monster_anim_name(&format!("{parent} {stem}"));
            if !name.starts_with("mon_") {
                continue; // 非怪物素材（如 All Characters 合集图）跳过
            }
            let Ok(img) = image::open(&src) else { continue };
            let (w, h) = (img.width(), img.height());
            if w < fw || h < fh {
                continue;
            }
            let (fx_n, fy_n) = (w / fw, h / fh);
            let total = fx_n * fy_n;
            // 组合网格：ceil(sqrt(total)) 列
            let cols = ((total as f32).sqrt().ceil()) as u32;
            let rows = (total + cols - 1) / cols;
            let mut sheet = image::RgbaImage::new(cols * OUT, rows * OUT);
            for i in 0..total {
                let (sx, sy) = ((i % fx_n) * fw, (i / fx_n) * fh);
                let frame = image::imageops::resize(
                    &image::imageops::crop_imm(&img, sx, sy, fw, fh).to_image(),
                    OUT,
                    OUT,
                    image::imageops::FilterType::Nearest,
                );
                let (ox, oy) = ((i % cols) * OUT, (i / cols) * OUT);
                image::imageops::replace(&mut sheet, &frame, ox as i64, oy as i64);
            }
            let target_dir = crate::project::dir_of("anims");
            let _ = std::fs::create_dir_all(&target_dir);
            let file = format!("{name}.png");
            if sheet.save(target_dir.join(&file)).is_err() {
                continue;
            }
            self.defs.insert(
                name.clone(),
                AnimDef {
                    name: name.clone(),
                    sheet: file,
                    frame_w: OUT,
                    frame_h: OUT,
                    frame_times: vec![0.12; total as usize],
                    events: Vec::new(),
                    r#loop: true,
                },
            );
            self.counts.insert(name.clone(), 0);
            ok.push(name);
        }
        (ok.len(), ok)
    }

    /// 保存全部定义到 animations.ron
    pub fn save(&self) -> Result<(), String> {
        let defs: Vec<&AnimDef> = self.defs.values().collect();
        let s = ron::ser::to_string_pretty(&defs, Default::default())
            .map_err(|e| e.to_string())?;
        std::fs::write(crate::project::path_of("data/animations.ron"), &s)
            .map_err(|e| e.to_string())?;
        tracing::info!("animations.ron 已保存（{} 个动画）", defs.len());
        Ok(())
    }
}

/// 按定义加载并切割精灵表（行优先：第 0 行从左到右）
pub fn load_sheet_frames(def: &AnimDef) -> Result<Vec<RgbaImage>, String> {
    let path = crate::project::dir_of("anims").join(&def.sheet);
    let img = image::open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .to_rgba8();
    cut_sheet(&img, def.frame_w, def.frame_h)
}

pub fn cut_sheet(img: &RgbaImage, fw: u32, fh: u32) -> Result<Vec<RgbaImage>, String> {
    let (w, h) = img.dimensions();
    if fw == 0 || fh == 0 || w < fw || h < fh {
        return Err(format!("帧尺寸 {fw}x{fh} 与精灵表 {w}x{h} 不符"));
    }
    let cols = w / fw;
    let rows = h / fh;
    let mut frames = Vec::with_capacity((cols * rows) as usize);
    for r in 0..rows {
        for c in 0..cols {
            let mut sub = RgbaImage::new(fw, fh);
            for y in 0..fh {
                for x in 0..fw {
                    let p = img.get_pixel(c * fw + x, r * fh + y);
                    sub.put_pixel(x, y, *p);
                }
            }
            frames.push(sub);
        }
    }
    Ok(frames)
}

/// 首次运行时生成示例精灵表 + 初始 animations.ron
fn ensure_demo_assets() {
    let dir = anims_dir();
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
    }
    let demo_path = dir.join("demo_slime.png");
    if !demo_path.exists() {
        // 4 帧果冻弹跳：16x16，压扁→正常→拉高→正常
        let mut sheet = RgbaImage::new(16 * 4, 16);
        let body = [208u8, 62, 150, 220];
        let dark = [150u8, 30, 105, 220];
        for f in 0..4u32 {
            let squash = [0.7f32, 1.0, 1.25, 1.0][f as usize];
            let bh = (10.0 * squash) as i32; // 体高
            let bw = ((10.0 / squash) as i32).max(4); // 体宽（反比）
            let base_y = 15i32;
            let x0 = (f * 16 + 8) as i32;
            for yy in 0..bh {
                let t = yy as f32 / bh as f32;
                let half_w = (bw as f32 * (0.35 + 0.65 * (1.0 - t).sqrt())) as i32 / 2 + 1;
                for xx in (-half_w)..=half_w {
                    let px = (x0 + xx).clamp(f as i32 * 16, f as i32 * 16 + 15) as u32;
                    let py = (base_y - yy).clamp(0, 15) as u32;
                    let col = if yy > bh - 2 { dark } else { body };
                    sheet.put_pixel(px, py, image::Rgba(col));
                }
            }
            // 眼睛
            let ey = (base_y - bh + 2) as u32;
            sheet.put_pixel((x0 - 2) as u32, ey, image::Rgba([255, 255, 255, 255]));
            sheet.put_pixel((x0 + 2) as u32, ey, image::Rgba([255, 255, 255, 255]));
        }
        let _ = sheet.save(&demo_path);
        tracing::info!("已生成示例精灵表 assets/anims/demo_slime.png");
    }
    if !crate::project::path_of("data/animations.ron").exists() {
        let demo = vec![AnimDef {
            name: "demo_slime".into(),
            sheet: "demo_slime.png".into(),
            frame_w: 16,
            frame_h: 16,
            frame_times: vec![0.14, 0.1, 0.14, 0.1],
            events: vec![(0, "squish".into())],
            r#loop: true,
        }];
        if let Ok(s) = ron::ser::to_string_pretty(&demo, Default::default()) {
            let _ = std::fs::write(crate::project::path_of("data/animations.ron"), &s);
        }
    }
}

/// 动画播放器：推进帧、产出帧事件
#[derive(Debug, Clone)]
pub struct AnimPlayer {
    pub anim: String,
    pub t: f32,
    pub frame: u32,
    pub finished: bool,
}

impl AnimPlayer {
    pub fn new(anim: impl Into<String>) -> Self {
        Self { anim: anim.into(), t: 0.0, frame: 0, finished: false }
    }

    fn frame_dur(def: &AnimDef, i: u32) -> f32 {
        def.frame_times
            .get(i as usize)
            .or_else(|| def.frame_times.last())
            .copied()
            .unwrap_or(0.12)
            .max(0.016)
    }

    /// 步进；返回本步触发的事件名列表
    pub fn update(&mut self, bank: &AnimBank, dt: f32) -> Vec<String> {
        let Some(def) = bank.def(&self.anim) else { return vec![] };
        let n = bank.frame_count(&self.anim);
        if n == 0 {
            return vec![];
        }
        self.t += dt;
        let mut evs = Vec::new();
        let mut guard = 0;
        while guard < 64 {
            guard += 1;
            let d = Self::frame_dur(def, self.frame);
            if self.t < d {
                break;
            }
            self.t -= d;
            self.frame += 1;
            if self.frame >= n {
                if def.r#loop {
                    self.frame = 0;
                } else {
                    self.frame = n - 1;
                    self.finished = true;
                    break;
                }
            }
            for (f, e) in &def.events {
                if *f == self.frame {
                    evs.push(e.clone());
                }
            }
        }
        evs
    }

    pub fn region(&self, bank: &AnimBank) -> Option<Region> {
        bank.frame_region(&self.anim, self.frame)
    }
}
