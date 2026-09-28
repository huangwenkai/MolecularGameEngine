//! 游戏内容层入口：组装世界、玩家、动作、工具、实体（地形即像素，Noita 式）
mod actions;
mod anim;
mod art;
mod astar;
mod audio;
mod character;
mod debug;
mod drops;
mod editor;
mod entities;
mod ide;
mod inventory;
mod items;
mod monsters;
mod npc;
mod player;
mod projectiles;
mod project;
mod save;
mod selftest;
mod settings;
mod icons;
mod skills;
mod tools;
mod vfx;

use actions::{ActionState, ActionTable};
use egui::{Align2, Area, Frame as EguiFrame, Id};
use glam::Vec2;
use mge_core::math::Aabb;
use mge_core::rng::Rng;
use mge_platform::input::Action;
use mge_render::{Region, SpriteBatch};
use mge_runtime::{App, Engine, EngineCtx};
use mge_world::{LightUpload, World, LIGHT_CELL};
use player::Player;
use std::collections::HashMap;
use tools::{Tool, ToolCtx};

/// 世界尺寸（模拟像素）
pub const WORLD_W_PX: i32 = 4096;
pub const WORLD_H_PX: i32 = 2048;

pub struct GameApp {
    pub world: World,
    pub player: Player,
    pub action: ActionState,
    pub actions: ActionTable,
    pub tool: ToolCtx,
    pub ecs: hecs::World,
    pub regions: HashMap<String, Region>,
    pub mouse_world: Vec2,
    pub rng: Rng,
    pub dbg: debug::DebugUi,
    pub selftest: bool,
    pub tick_ms_sum: f32,
    pub tick_count: u64,
    pub vfx: vfx::Vfx,
    pub projectiles: projectiles::Projectiles,
    pub hitstop: u8,
    pub weapons: editor::WeaponFx,
    pub editor: editor::VfxEditor,
    /// 物品图标库（图集 Region + egui 纹理缓存）
    pub icons: icons::IconBank,
    pub db: items::ItemDb,
    pub inv: inventory::Inventory,
    pub drops: drops::Drops,
    pub monsters: monsters::Monsters,
    pub npcs: npc::Npcs,
    pub anims: anim::AnimBank,
    /// 动画预览播放器（编辑器 ▶ 触发，玩家头顶播放）
    pub anim_preview: Option<anim::AnimPlayer>,
    /// 最近一次动画帧事件（自测/调试观察）
    pub anim_last_event: Option<String>,
    /// 新手引导剩余显示时间（秒）
    pub guide_t: f32,
    pub audio: audio::Audio,
    /// 系统设置（音量/震动/键位，持久化于 saves/settings.ron）
    pub settings: settings::Settings,
    /// 植被定义（vegetation.ron，编辑器可编辑 + 重新生长）
    pub veg: mge_world::veg::VegFile,
    /// 人物形象（部件贴图，F1 人物页逐像素编辑）
    pub skin: character::Skin,
    /// 系统设置面板（ESC）
    pub settings_ui: settings::SettingsUi,
    /// 屏幕提示（文字, 剩余秒数）——背包满等一次性提醒
    pub hint: (String, f32),
    /// 主动技能（学习/冷却/施放，M17）
    pub skills: skills::Skills,
    /// 游戏工程（素材根，M18）
    pub project: project::ProjectManager,
    /// 引擎 IDE（左中右布局，F4）
    pub ide: ide::Ide,
    /// notify 文件监听（保活；drop 即停止监听）
    #[allow(dead_code)] // 仅保活，事件经 fs_events 通道消费
    fs_watcher: Option<notify::RecommendedWatcher>,
    /// 文件变更事件接收端（tick 消费 → 立即热重载检查）
    fs_events: Option<std::sync::mpsc::Receiver<()>>,
    /// egui 中文字体是否已注入
    fonts_done: bool,
}

impl GameApp {
    pub fn new(seed: u64, selftest: bool) -> Self {
        let veg = editor::load_veg();
        let world = World::new_with_veg(seed, WORLD_W_PX, WORLD_H_PX, &veg.plants);
        tracing::info!("初始化：世界创建完成");
        let spawn =
            Vec2::new(world.spawn_x as f32 + 0.5, world.spawn_y as f32);
        let player = Player::new(spawn);
        let weapons = editor::load_weapons();
        let mut projectiles = projectiles::Projectiles::default();
        projectiles.fx_fizz = weapons.fizz.clone();
        projectiles.fx_explosion = weapons.explosion.clone();
        projectiles.fx_arrow_hit = weapons.arrow_hit.clone();
        projectiles.fx_hit_spark = weapons.hit_spark.clone();
        let vfx = vfx::Vfx::embedded();
        tracing::info!("初始化：特效库就绪");
        let db = items::ItemDb::load();
        tracing::info!("初始化：物品库就绪（{} 个物品）", db.defs.len());
        let anims = anim::AnimBank::load();
        tracing::info!("初始化：动画库就绪");
        let mut audio = audio::Audio::new();
        tracing::info!("初始化：音频就绪");
        let settings = settings::Settings::load();
        audio.set_volume(settings.volume);
        // 工程：恢复上次打开的项目（决定素材路径）
        let mut project = project::ProjectManager::default();
        project.load_state();
        // IDE 配置（界面字号等）
        let mut ide = ide::Ide::default();
        ide.font_size = ide::IdeConfig::load().font_size;
        ide.cfg_loaded = true;
        if let Some(p) = &project.current {
            tracing::info!("已恢复工程「{}」({})", p.name, p.root.display());
        }
        let (fs_watcher, fs_events) = editor::spawn_watcher();
        Self {
            world,
            player,
            action: ActionState::default(),
            actions: ActionTable::embedded(),
            tool: ToolCtx::default(),
            ecs: hecs::World::new(),
            regions: HashMap::new(),
            mouse_world: spawn,
            rng: Rng::new(seed ^ 0xABCD),
            dbg: debug::DebugUi::default(),
            selftest,
            tick_ms_sum: 0.0,
            tick_count: 0,
            vfx,
            projectiles,
            hitstop: 0,
            weapons,
            editor: editor::VfxEditor::default(),
            icons: icons::IconBank::default(),
            db,
            inv: inventory::Inventory::new(),
            drops: drops::Drops::default(),
            monsters: monsters::Monsters::default(),
            npcs: npc::Npcs::default(),
            anims,
            anim_preview: None,
            anim_last_event: None,
            guide_t: 8.0,
            audio,
            settings,
            veg,
            skin: character::load(),
            skills: skills::Skills::default(),
            project,
            ide,
            fs_watcher,
            fs_events,
            settings_ui: settings::SettingsUi::default(),
            hint: (String::new(), 0.0),
            fonts_done: false,
        }
    }

    /// 扫描某像素列的地表高度
    fn surface_y(&self, x: i32) -> i32 {
        for y in 0..self.world.pixels.h {
            if self.world.solid_px(x, y) {
                return y;
            }
        }
        self.world.pixels.h / 2
    }

    /// 打开设置面板时清掉残留的物理键捕获（避免误触发重绑定）
    fn settings_ui_listen_reset(&self, ctx: &mut EngineCtx) {
        let _ = ctx.input.take_raw_key();
    }

    /// 注入系统中文字体（仅一次；egui 默认字体不含 CJK，中文会显示为方框）
    fn ensure_cjk_fonts(&mut self, egui: &egui::Context) {
        if self.fonts_done {
            return;
        }
        self.fonts_done = true;
        const CANDIDATES: &[&str] = &[
            "C:/Windows/Fonts/msyh.ttc",
            "C:/Windows/Fonts/msyh.ttf",
            "C:/Windows/Fonts/simhei.ttf",
            "C:/Windows/Fonts/simsun.ttc",
            "C:/Windows/Fonts/Deng.ttf",
        ];
        for path in CANDIDATES {
            let Ok(bytes) = std::fs::read(path) else { continue };
            let mut fonts = egui::FontDefinitions::default();
            fonts
                .font_data
                .insert("cjk".into(), egui::FontData::from_owned(bytes).into());
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().push("cjk".into());
            }
            egui.set_fonts(fonts);
            tracing::info!("CJK 字体已加载: {path}");
            return;
        }
        tracing::warn!("未找到系统中文字体，界面中文可能显示为方框");
    }
}

impl GameApp {
    /// IDE 视口帧：世界模拟 + AI + 纹理上传 + 素材热重载（不读玩家输入，避免面板点击误触发）
    fn ide_tick(&mut self, ctx: &mut EngineCtx) {
        self.world.update();
        let st = self.inv.aggregate(&self.db);
        let (_deaths, _hurt, _knock) = self.monsters.update(
            &mut self.world,
            (&self.player.pos, &self.player.half, &mut self.player.hp, st.mitigation()),
            &st,
            &mut self.vfx,
            &mut self.rng,
        );
        let night = self.world.time > 0.58 && self.world.time < 0.95;
        self.npcs.update(&mut self.world, night, &mut self.rng);
        self.vfx.update(1.0 / 60.0);
        self.audio.tick(night);
        self.audio.tick_ambient(night);

        // 相机跟随玩家（无鼠标偏移）
        let target = self.player.pos + Vec2::new(0.0, -14.0);
        ctx.camera.center += (target - ctx.camera.center) * 0.12;
        self.world.light.player_glow = Some((
            (self.player.pos.x as i32) / LIGHT_CELL,
            (self.player.pos.y as i32 - 10) / LIGHT_CELL,
        ));

        // 世界/光照纹理上传（脏 chunk 粒度）
        let mut bg_mats: Vec<u8> = ["wood", "leaf", "berry", "rope"]
            .iter()
            .filter_map(|n| self.world.mats.id(n))
            .collect();
        for id in &self.world.veg_mats {
            if !bg_mats.contains(id) {
                bg_mats.push(*id);
            }
        }
        {
            let mut data = Vec::with_capacity(128 * 128 * 2);
            for (_ci, rect) in self.world.pending_uploads.drain(..) {
                self.world.pixels.export_chunk_data(_ci, &mut data);
                for i in (0..data.len()).step_by(2) {
                    if bg_mats.contains(&data[i]) {
                        data[i] += 128;
                    }
                }
                ctx.renderer.upload_world(
                    rect.x.max(0) as u32,
                    rect.y.max(0) as u32,
                    rect.w as u32,
                    rect.h as u32,
                    &data,
                );
            }
        }
        for up in self.world.take_light_uploads() {
            match up {
                LightUpload::Full(data) => ctx.renderer.upload_light(&data),
                LightUpload::Region { x, y, w, h, data } => {
                    ctx.renderer.upload_light_region(x, y, w, h, &data)
                }
            }
        }

        // 素材热重载：在 IDE 里改文件即时在视口生效
        if editor::reload_if_changed(self) {
            let pal = art::palette(&self.world.mats);
            ctx.renderer.set_palette(&pal);
        }
        for (kind, src) in self.editor.shader_req.drain(..) {
            let _ = ctx.renderer.reload_shader(kind, &src);
        }
        if !self.editor.char_dirty.is_empty() {
            for key in self.editor.char_dirty.drain(..) {
                if let Some(pt) = self.skin.get(key) {
                    let (w, h) = pt.img.dimensions();
                    ctx.renderer.upload_atlas(pt.ax, pt.ay, w, h, pt.img.as_raw());
                }
            }
        }
        // 动画/物品编辑器数据服务（IDE 暂停模式下也要响应编辑器请求）
        self.editor_data_services(ctx);
    }

    /// 编辑器数据服务：动画精灵表导入、图标精灵图导入/选用（需要 renderer；普通 tick 与 ide_tick 共用）
    fn editor_data_services(&mut self, ctx: &mut EngineCtx) {
        // ---- 动画编辑器：加载/重切精灵表 ----
        if self.editor.anim_load_req {
            self.editor.anim_load_req = false;
            self.editor.anim_err = None;
            if let Some(def) = self.anims.defs.get(&self.editor.anim_sel).cloned() {
                match anim::load_sheet_frames(&def) {
                    Ok(frames) => {
                        let n = frames.len();
                        if let Some(d) = self.anims.defs.get_mut(&self.editor.anim_sel) {
                            d.frame_times = vec![0.12; n];
                        }
                        match self
                            .anims
                            .register_runtime(def, &frames, ctx.renderer)
                        {
                            Ok(_) => {
                                tracing::info!("动画 {} 加载完成（{n} 帧）", self.editor.anim_sel)
                            }
                            Err(e) => self.editor.anim_err = Some(e),
                        }
                    }
                    Err(e) => self.editor.anim_err = Some(e),
                }
            }
        }
        // ---- 物品编辑器：导入图标精灵图（切 64×64 帧，缓存像素供选择器预览）----
        if self.editor.icon_import_req {
            self.editor.icon_import_req = false;
            self.editor.item_err = None;
            let path = self.editor.icon_path.trim().to_string();
            match image::open(&path).map(|i| i.to_rgba8()) {
                Ok(img) => {
                    let (w, h) = img.dimensions();
                    let (cx, cy) = (w / 64, h / 64);
                    if cx == 0 || cy == 0 {
                        self.editor.item_err = Some("图太小（需 ≥64×64）".into());
                    } else {
                        let sheet: Vec<image::RgbaImage> = (0..cx * cy)
                            .map(|i| {
                                image::imageops::crop_imm(
                                    &img,
                                    (i % cx) * 64,
                                    (i / cx) * 64,
                                    64,
                                    64,
                                )
                                .to_image()
                            })
                            .collect();
                        let n = sheet.len();
                        for (i, f) in sheet.iter().enumerate() {
                            self.icons
                                .store_pixels(&icons::IconBank::sheet_key(i), f.clone());
                        }
                        self.editor.icon_sheet = sheet;
                        tracing::info!("图标精灵图导入：{n} 帧（{path}）");
                    }
                }
                Err(e) => self.editor.item_err = Some(format!("读取失败: {e}")),
            }
        }
        // ---- 物品编辑器：选用第 N 帧为当前物品图标（存 PNG + 上传图集）----
        if let Some(i) = self.editor.icon_pick.take() {
            let sel = self.editor.item_sel.clone();
            if let Some(f) = self.editor.icon_sheet.get(i).cloned() {
                let stem = std::path::Path::new(self.editor.icon_path.trim())
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("icons")
                    .to_string();
                let key = format!("{stem}_{i:04}");
                let dir = project::dir_of("icons");
                let _ = std::fs::create_dir_all(&dir);
                if f.save(dir.join(format!("{key}.png"))).is_ok() {
                    self.icons.register(ctx.renderer, &key, f);
                    if let Some(d) = self.db.defs.get_mut(&sel) {
                        d.icon = Some(key.clone());
                    }
                    tracing::info!("物品 {sel} 图标 → {key}");
                } else {
                    self.editor.item_err = Some("图标 PNG 保存失败".into());
                }
            }
        }
    }

}

impl App for GameApp {
    fn init(&mut self, ctx: &mut EngineCtx) {
        // 程序化美术 + 图集上传（含动画帧/人物部件打包）
        let art = art::build(ctx.renderer, &mut self.anims, &mut self.skin);
        self.regions = art.regions;
        // 物品图标库（assets/icons/*.png → 图集）
        self.icons.load_dir(ctx.renderer);
        // 调色板 + 世界/光照纹理
        let pal = art::palette(&self.world.mats);
        ctx.renderer.set_palette(&pal);
        ctx.renderer
            .set_world_texture(self.world.pixels.w as u32, self.world.pixels.h as u32);
        ctx.renderer
            .set_light_texture(self.world.light.cw as u32, self.world.light.ch as u32);
        // 背景墙纹理（生成后不变，上传一次）
        ctx.renderer
            .set_wall_texture(self.world.wall_w as u32, self.world.wall_h as u32);
        ctx.renderer.upload_walls(&self.world.wall_texture_data());

        // 出生点空地：清理树木（背景像素）—— 覆盖树冠最高点与斜坡低处，避免残留光杆树干
        let wood = self.world.mats.id("wood").unwrap_or(0);
        let leaf = self.world.mats.id("leaf").unwrap_or(0);
        let stx = self.world.spawn_x;
        let sty = self.world.spawn_y;
        for ty in (sty - 170).max(0)..(sty + 140).min(self.world.pixels.h) {
            for tx in (stx - 320).max(0)..(stx + 360).min(self.world.pixels.w) {
                let m = self.world.pixels.get(tx, ty).mat;
                if m == wood || m == leaf {
                    self.world.pixels.set(tx, ty, mge_sim::Pixel::default());
                }
            }
        }
        self.world.mark_terrain_dirty();

        // 训练假人
        let sx = self.world.spawn_x;
        for dx in [-140i32, -80, 200] {
            let px = sx + dx;
            let py = self.surface_y(px);
            entities::spawn_dummy(&mut self.ecs, Vec2::new(px as f32 + 0.5, py as f32));
        }

        // NPC 生活 AI（地表清理后生成：NPC + 浆果丛）
        self.npcs = npc::Npcs::new(&mut self.world, 2, &mut self.rng);

        // 系统设置：键位覆盖 + 震屏倍率
        self.settings.apply(ctx.input.map_mut());
        ctx.camera.shake_scale = self.settings.shake;

        ctx.camera.center = self.player.pos - Vec2::new(0.0, 16.0);
        self.mouse_world = ctx.camera.screen_to_world(ctx.input.mouse_pos);
    }

    fn tick(&mut self, ctx: &mut EngineCtx) {
        let t0 = std::time::Instant::now();

        // ---- 系统设置（ESC）：打开时暂停游戏，仅面板响应 ----
        if ctx.input.just_pressed(Action::ToggleMenu) {
            self.settings_ui.open = !self.settings_ui.open;
            if self.settings_ui.open {
                self.settings_ui_listen_reset(ctx);
            }
        }
        ctx.camera.shake_scale = self.settings.shake;
        if self.settings_ui.open {
            self.tick_ms_sum += t0.elapsed().as_secs_f32() * 1000.0;
            self.tick_count += 1;
            return;
        }

        // 新手引导倒计时
        self.guide_t = (self.guide_t - 1.0 / 60.0).max(0.0);
        // 屏幕提示倒计时
        if self.hint.1 > 0.0 {
            self.hint.1 = (self.hint.1 - 1.0 / 60.0).max(0.0);
        }

        // ---- 自测脚本（必须在输入消费之前注入）----
        if self.selftest {
            selftest::drive(self, ctx);
        }

        // ---- 顿帧（打击感）：冻结模拟，保持渲染 ----
        if self.hitstop > 0 {
            self.hitstop -= 1;
            self.tick_ms_sum += t0.elapsed().as_secs_f32() * 1000.0;
            self.tick_count += 1;
            return;
        }

        // ---- 快捷栏 ----
        for (slot, tool) in [
            (Action::Slot1, Tool::Sword),
            (Action::Slot2, Tool::Pickaxe),
            (Action::Slot3, Tool::Block),
            (Action::Slot4, Tool::Torch),
            (Action::Slot5, Tool::Water),
            (Action::Slot6, Tool::Sand),
            (Action::Slot7, Tool::Bow),
            (Action::Slot8, Tool::Fireball),
        ] {
            if ctx.input.just_pressed(slot) {
                self.tool.tool = tool;
                self.action.current = None; // 切工具打断动作
            }
        }

        self.mouse_world = ctx.camera.screen_to_world(ctx.input.mouse_pos);

        // ---- 特效编辑器：F1 开关 / 预览触发 / 热重载 ----
        if ctx.input.just_pressed(Action::ToggleEditor) {
            self.editor.open = !self.editor.open;
        }
        // ---- F1：打开 IDE 并跳到编辑器功能区（编辑器已并入 IDE）----
        if ctx.input.just_pressed(Action::ToggleEditor) {
            self.ide.open = true;
            self.ide.func = (self.editor.tab + 1).clamp(1, 5);
        }
        // ---- 引擎 IDE（F4）：打开时暂停游戏逻辑，仅 IDE 面板响应 ----
        if ctx.input.just_pressed(Action::ToggleIde) {
            self.ide.open = !self.ide.open;
        }
        if !self.ide.open {
            self.editor.open = false; // IDE 关闭后放开编辑器热重载保护
        }
        if self.ide.open {
            // IDE 模式：视口实时预览——run=true 时推进世界/AI（不读玩家输入），否则冻结画面
            if self.ide.run {
                self.ide_tick(ctx);
            }
            self.tick_ms_sum += t0.elapsed().as_secs_f32() * 1000.0;
            self.tick_count += 1;
            return;
        }
        if self.editor.trigger {
            self.editor.trigger = false;
            if let Some(bp) = self.vfx.bps.get(&self.editor.sel).cloned() {
                let (shake, hs) = self.vfx.spawn_bp(
                    &bp,
                    self.player.pos + Vec2::new(0.0, -10.0),
                    self.player.facing,
                    &mut self.rng,
                );
                self.hitstop = self.hitstop.max(hs);
                ctx.camera.add_shake(shake);
            }
        }
        // 文件监听热重载：notify 事件 → 立即检查；无事件时 0.5s mtime 轮询兜底
        let fs_dirty = self
            .fs_events
            .as_ref()
            .map(|rx| {
                let mut dirty = false;
                while rx.try_recv().is_ok() {
                    dirty = true;
                }
                dirty
            })
            .unwrap_or(false);
        if (fs_dirty || self.tick_count % 30 == 0) && editor::reload_if_changed(self) {
            // 材质表重载 → 重建调色板纹理
            let pal = art::palette(&self.world.mats);
            ctx.renderer.set_palette(&pal);
        }
        // 植被编辑器：重新生长（清除现有植被 → 按当前定义重铺）
        if self.editor.veg_regrow_req {
            self.editor.veg_regrow_req = false;
            let defs = self.veg.plants.clone();
            self.world.regrow_vegetation(&defs);
            tracing::info!("植被已重新生长（{} 种）", defs.len());
        }
        // 人物形象编辑器：像素变更 → 上传图集（实时生效）
        if !self.editor.char_dirty.is_empty() {
            let keys: Vec<&str> = self.editor.char_dirty.drain(..).collect();
            for key in keys {
                if let Some(pt) = self.skin.get(key) {
                    let (w, h) = pt.img.dimensions();
                    ctx.renderer.upload_atlas(pt.ax, pt.ay, w, h, pt.img.as_raw());
                }
            }
        }
        let fx = self.weapons.clone();

        // ---- WGSL 着色器热重载 ----
        if !self.editor.shader_req.is_empty() {
            for (kind, src) in self.editor.shader_req.drain(..) {
                match ctx.renderer.reload_shader(kind, &src) {
                    Ok(_) => tracing::info!("着色器热重载成功: {kind:?}"),
                    Err(e) => tracing::error!("着色器热重载失败: {kind:?} {e}"),
                }
            }
        }

        // ---- 动画/物品编辑器数据服务（动画精灵表导入、图标导入/选用）----
        self.editor_data_services(ctx);

        // ---- 动画预览（玩家头顶循环播放，帧事件输出日志）----
        if let Some(pl) = &mut self.anim_preview {
            for ev in pl.update(&self.anims, 1.0 / 60.0) {
                tracing::info!("动画事件: {ev} @ 帧 {}", pl.frame);
                self.anim_last_event = Some(ev);
            }
        }

        // ---- 暗黑层：聚合属性 / 药水 / 背包 / max_hp / 移速 ----
        let st = self.inv.aggregate(&self.db);
        self.player.max_hp = 100.0 + st.hp + self.inv.level.saturating_sub(1) as f32 * 8.0;
        self.player.move_mult = 1.0 + st.move_pct / 100.0;
        if ctx.input.just_pressed(Action::Inventory) {
            self.inv.ui_open = !self.inv.ui_open;
        }
        if ctx.input.just_pressed(Action::SkillPanel) {
            self.skills.ui_open = !self.skills.ui_open;
        }
        if ctx.input.just_pressed(Action::Potion) {
            if self.inv.use_potion(&mut self.player.hp, self.player.max_hp, &self.db) {
                for _ in 0..12 {
                    self.vfx.dot(
                        self.player.pos
                            + Vec2::new(self.rng.range_f32(-6.0, 6.0), self.rng.range_f32(0.0, 14.0)),
                        Vec2::new(0.0, -30.0),
                        0.4,
                        1.6,
                        [0.4, 1.0, 0.5],
                        -40.0,
                        true,
                    );
                }
            }
        }

        // ---- 存档 F5 / 读档 F9 ----
        if ctx.input.just_pressed(Action::QuickSave) {
            if let Err(e) = save::save_game(self) {
                tracing::error!("存档失败: {e}");
            }
        }
        if ctx.input.just_pressed(Action::QuickLoad) {
            if let Err(e) = save::load_game(self) {
                tracing::error!("读档失败: {e}");
            }
        }

        // ---- 主动技能（Z/X/C，M17）----
        skills::tick(self, ctx);

        // ---- 玩家 ----
        let shake_player =
            player::update(&mut self.player, ctx.input, &mut self.world, &mut self.rng);

        // ---- 工具与动作 ----
        let busy = self.action.busy();
        // 攻击时面向鼠标
        if self.tool.tool == Tool::Sword || busy {
            let d = self.mouse_world.x - self.player.pos.x;
            if d.abs() > 2.0 {
                self.player.facing = d.signum();
            }
        }
        let (swing, shake_tool) = tools::update(
            &mut self.tool,
            ctx.input,
            &mut self.world,
            self.player.pos,
            self.player.half,
            self.mouse_world,
            busy,
        );
        let (active_started, _finished) =
            self.action.update(&self.actions, swing, st.atk_speed().clamp(0.3, 3.0));
        if active_started && self.action.phase == actions::Phase::Active {
            // 出招前冲 + 挥砍弧线特效
            self.player.vel.x += self.player.facing * 45.0;
            self.audio.play(audio::Sfx::Swing);
            let _ = self.vfx.spawn(
                &fx.slash,
                self.player.pos + Vec2::new(0.0, -10.0),
                self.player.facing,
                &mut self.rng,
            );
        }

        // ---- 远程武器（弓 / 火球法杖）按住循环射击 ----
        if !busy && self.tool.place_cooldown == 0 && ctx.input.pressed(Action::Attack) {
            let hand = self.player.pos + Vec2::new(0.0, -10.0);
            let dir = (self.mouse_world - hand).normalize_or_zero();
            match self.tool.tool {
                Tool::Bow => {
                    self.projectiles
                        .spawn(projectiles::ProjKind::Arrow, hand + dir * 6.0, dir * 340.0);
                    self.tool.place_cooldown = (14.0 / st.atk_speed()) as u8;
                    self.audio.play(audio::Sfx::Shoot);
                }
                Tool::Fireball => {
                    self.projectiles.spawn(
                        projectiles::ProjKind::Fireball,
                        hand + dir * 6.0,
                        dir * 240.0,
                    );
                    self.tool.place_cooldown = (22.0 / st.atk_speed()) as u8;
                    self.audio.play(audio::Sfx::Shoot);
                }
                _ => {}
            }
        }

        // ---- 命中判定（伤害管线：暴击 → 打击火花 → 飘字 → 顿帧）----
        if let (Some((center, half)), false, Some(def)) = (
            self.action.hitbox(self.player.pos, self.player.facing),
            self.action.hit_done,
            self.action.current.clone(),
        ) {
            let hb = Aabb::new(center, half);
            self.action.hit_done = true;
            let crit = self.rng.chance(st.crit / 100.0);
            let crit_mult = 1.8 + st.crit_dmg / 100.0;
            let dmg = st.damage(def.damage) * if crit { crit_mult } else { 1.0 };
            // 怪物受击
            let hit_pos = self.monsters.melee_hit(&hb, dmg, self.player.facing, crit);
            for hp_pos in &hit_pos {
                self.audio.play(if crit { audio::Sfx::Crit } else { audio::Sfx::Hit });
                let bp = if crit { fx.crit.as_str() } else { fx.hit.as_str() };
                let (_, hs) = self.vfx.spawn(bp, *hp_pos, self.player.facing, &mut self.rng);
                self.hitstop = self.hitstop.max(hs);
                self.vfx.text(*hp_pos + Vec2::new(0.0, -18.0), dmg as u32, crit);
                ctx.camera.add_shake(if crit { 4.0 } else { 2.0 });
            }
            let mut query = self
                .ecs
                .query::<(&entities::Transform, &mut entities::Dummy, &mut entities::Vel)>();
            for (_e, (tr, dm, vel)) in query.iter() {
                if dm.respawn > 0 {
                    continue;
                }
                let da = Aabb::new(tr.pos - Vec2::new(0.0, 10.0), Vec2::new(6.0, 10.0));
                if hb.intersects(&da) {
                    dm.hp -= dmg;
                    dm.flash = 0.18;
                    vel.v += Vec2::new(def.knockback.0 * self.player.facing, def.knockback.1) / 60.0;
                    let bp = if crit { fx.crit.as_str() } else { fx.hit.as_str() };
                    let (_, hs) = self.vfx.spawn(
                        bp,
                        tr.pos + Vec2::new(0.0, -10.0),
                        self.player.facing,
                        &mut self.rng,
                    );
                    self.hitstop = self.hitstop.max(hs);
                    self.vfx.text(tr.pos + Vec2::new(0.0, -18.0), dmg as u32, crit);
                    ctx.camera.add_shake(if crit { 4.0 } else { 2.0 });
                }
            }
        }

        // ---- 投射物 ----
        let mut targets = Vec::new();
        {
            let mut q = self.ecs.query::<(&entities::Transform, &entities::Phys)>();
            for (e, (tr, ph)) in q.iter() {
                if self.ecs.get::<&entities::Dummy>(e).map(|d| d.respawn == 0) == Ok(true) {
                    targets.push((e, tr.pos, ph.half));
                }
            }
        }
        let hs = self.projectiles.update(
            &mut self.world,
            &mut self.vfx,
            &mut self.rng,
            1.0 / 60.0,
        );
        self.hitstop = self.hitstop.max(hs);
        let dhits = self.projectiles.check_dummies(
            &targets,
            &mut self.vfx,
            &mut self.rng,
            &st,
        );
        for (e, dmg, knock, crit, kind, pos) in dhits {
            let q = self
                .ecs
                .query_one::<(&entities::Transform, &mut entities::Dummy, &mut entities::Vel)>(e);
            if let Ok(mut q) = q {
                if let Some((tr, dm, vel)) = q.get() {
                    dm.hp -= dmg;
                    dm.flash = 0.18;
                    vel.v += knock / 60.0;
                    self.vfx.text(tr.pos + Vec2::new(0.0, -18.0), dmg as u32, crit);
                }
            }
            if kind == projectiles::ProjKind::Fireball {
                self.world.explode(pos.x as i32, pos.y as i32, 7);
                let (_, hs) = self.vfx.spawn(&fx.explosion, pos, 1.0, &mut self.rng);
                self.hitstop = self.hitstop.max(hs);
                ctx.camera.add_shake(8.0);
            }
        }

        // ---- 投射物 vs 怪物 ----
        let mut proj_hits = Vec::new();
        for (i, p) in self.projectiles.list.iter().enumerate() {
            if p.stuck {
                continue;
            }
            let crit = self.rng.chance(st.crit / 100.0);
            let crit_mult = 1.8 + st.crit_dmg / 100.0;
            let base = match p.kind {
                projectiles::ProjKind::Arrow => projectiles::ARROW_DMG,
                projectiles::ProjKind::Fireball => projectiles::FIREBALL_DMG,
            };
            let dmg = st.damage(base) * if crit { crit_mult } else { 1.0 };
            if let Some(mpos) = self.monsters.proj_hit(p.pos, dmg) {
                proj_hits.push((i, dmg, crit, p.kind, mpos));
            }
        }
        for (i, dmg, crit, kind, pos) in proj_hits.into_iter().rev() {
            self.projectiles.list.remove(i);
            self.vfx.text(pos + Vec2::new(0.0, -14.0), dmg as u32, crit);
            if kind == projectiles::ProjKind::Fireball {
                self.world.explode(pos.x as i32, pos.y as i32, 7);
                let (_, hs) = self.vfx.spawn(&fx.explosion, pos, 1.0, &mut self.rng);
                self.hitstop = self.hitstop.max(hs);
                ctx.camera.add_shake(8.0);
            } else {
                let _ = self.vfx.spawn(&fx.arrow_hit, pos, 1.0, &mut self.rng);
            }
        }

        // ---- 世界与实体 ----
        self.world.time_frozen = self.settings.time_lock;
        self.world.update();
        let deaths = entities::update(&mut self.ecs, &mut self.world, &mut self.rng);
        for d in deaths {
            let _ = self.vfx.spawn(&fx.death, d, 1.0, &mut self.rng);
            // 掉落 + 经验（暗黑循环）
            let loot = self.db.roll_loot(&mut self.rng, "dummy_loot");
            self.drops.spawn_loot(loot, d, &mut self.rng);
            let ups = self.inv.gain_xp(10);
            if ups > 0 {
                for _ in 0..24 {
                    let a = self.rng.range_f32(0.0, 6.28);
                    self.vfx.dot(
                        d + Vec2::new(a.cos() * 10.0, a.sin() * 10.0 - 10.0),
                        Vec2::new(a.cos() * -40.0, a.sin() * -40.0 - 20.0),
                        0.7,
                        2.0,
                        [1.0, 0.85, 0.3],
                        -60.0,
                        true,
                    );
                }
            }
        }

        // ---- 掉落物 ----
        let (picked, bag_full) = self.drops.update(
            &mut self.world,
            self.player.pos,
            &mut self.inv,
            &self.db,
            self.settings.magnet,
        );
        if !picked.is_empty() {
            self.audio.play(audio::Sfx::Pickup);
        }
        if bag_full {
            self.hint = ("背包已满！丢掉一些物品才能继续拾取".to_string(), 2.0);
            self.audio.play(audio::Sfx::Hurt);
        }
        // ---- 丢弃物手动拾取（F）：附近有丢弃物时提示，按键拾取 ----
        if let Some(name) = self.drops.manual_candidate(self.player.pos, &self.db) {
            self.hint = (format!("[F] 拾取 {name}"), 0.15);
        }
        if ctx.input.just_pressed(Action::Pickup) {
            if let Some(name) =
                self.drops
                    .manual_pickup(self.player.pos, &mut self.inv, &self.db)
            {
                self.audio.play(audio::Sfx::Pickup);
                self.hint = (format!("拾取 {name}"), 1.2);
            }
        }

        // ---- 火球移动光源（黑夜发光）----
        self.world.light.moving_lights = self
            .projectiles
            .list
            .iter()
            .filter(|p| p.kind == projectiles::ProjKind::Fireball && !p.stuck)
            .map(|p| {
                (
                    (p.pos.x as i32) / LIGHT_CELL,
                    (p.pos.y as i32 - 4) / LIGHT_CELL,
                    170u8,
                )
            })
            .collect();

        // ---- 刷怪与战斗 AI（刷怪开关：关闭后不自然生成，已有怪物保留）----
        if self.settings.spawn_on {
            self.monsters
                .spawn_tick(&self.world, self.player.pos, self.inv.level, &mut self.rng);
        }
        let (mdeaths, hurt, mknock) = self.monsters.update(
            &mut self.world,
            (
                &self.player.pos,
                &self.player.half,
                &mut self.player.hp,
                // 闪避翻滚期间无敌（完全减伤）
                if self.player.invuln > 0.0 { 1.0 } else { st.mitigation() },
            ),
            &st,
            &mut self.vfx,
            &mut self.rng,
        );
        if hurt > 0.0 {
            self.player.hurt_flash = 0.3;
            self.player.vel += mknock / 60.0;
            ctx.camera.add_shake(4.0);
            self.audio.play(audio::Sfx::Hurt);
        }
        for (mpos, mxp, table) in mdeaths {
            let _ = self.vfx.spawn(&fx.death, mpos, 1.0, &mut self.rng);
            let loot = self.db.roll_loot(&mut self.rng, table);
            self.drops.spawn_loot(loot, mpos, &mut self.rng);
            let ups = self.inv.gain_xp(mxp);
            if ups > 0 {
                self.skills.pts += ups as u8; // 每级 1 技能点（M17）
                self.audio.play(audio::Sfx::LevelUp);
                for _ in 0..24 {
                    let a = self.rng.range_f32(0.0, 6.28);
                    self.vfx.dot(
                        mpos + Vec2::new(a.cos() * 10.0, a.sin() * 10.0 - 10.0),
                        Vec2::new(a.cos() * -40.0, a.sin() * -40.0 - 20.0),
                        0.7,
                        2.0,
                        [1.0, 0.85, 0.3],
                        -60.0,
                        true,
                    );
                }
            }
        }

        // ---- NPC 生活 AI ----
        let night = self.world.time > 0.58 && self.world.time < 0.95;
        self.npcs.update(&mut self.world, night, &mut self.rng);

        // ---- BGM 昼夜调度 + 环境音（M17）----
        self.audio.tick(night);
        self.audio.tick_ambient(night);

        // ---- VFX 步进 ----
        self.vfx.update(1.0 / 60.0);

        // 世界事件 → 反馈
        for ev in self.world.events.drain() {
            match ev {
                mge_world::WorldEvent::Explosion { .. } => {
                    ctx.camera.add_shake(8.0);
                    self.audio.play(audio::Sfx::Explode);
                }
            }
        }

        // ---- 挖掘/放置音效（工具点击时）----
        if ctx.input.just_pressed(Action::Attack) {
            match self.tool.tool {
                Tool::Pickaxe => self.audio.play(audio::Sfx::Mine),
                Tool::Block | Tool::Torch => self.audio.play(audio::Sfx::Place),
                _ => {}
            }
        }

        // ---- 相机跟随 ----
        let look = (self.mouse_world - self.player.pos).clamp_length_max(48.0) * 0.25;
        let target = self.player.pos + Vec2::new(0.0, -14.0) + look;
        ctx.camera.center += (target - ctx.camera.center) * 0.12;
        ctx.camera.add_shake(shake_player + shake_tool);

        // ---- 玩家微光 ----
        self.world.light.player_glow = Some((
            (self.player.pos.x as i32) / LIGHT_CELL,
            (self.player.pos.y as i32 - 10) / LIGHT_CELL,
        ));

        // ---- 世界纹理上传（地形+动态像素同源，按脏 chunk 粒度）----
        // 背景材质（树/叶/浆果丛/绳索 + 植被）在材质字节打最高位标记 → 背景通道绘制（不遮挡角色）
        let mut bg_mats: Vec<u8> = ["wood", "leaf", "berry", "rope"]
            .iter()
            .filter_map(|n| self.world.mats.id(n))
            .collect();
        for id in &self.world.veg_mats {
            if !bg_mats.contains(id) {
                bg_mats.push(*id);
            }
        }
        {
            let mut data = Vec::with_capacity(128 * 128 * 2);
            for (ci, rect) in self.world.pending_uploads.drain(..) {
                self.world.pixels.export_chunk_data(ci, &mut data);
                for i in (0..data.len()).step_by(2) {
                    if bg_mats.contains(&data[i]) {
                        data[i] += 128;
                    }
                }
                ctx.renderer.upload_world(rect.x.max(0) as u32, rect.y.max(0) as u32, rect.w as u32, rect.h as u32, &data);
            }
        }
        // 光照纹理：仅上传重算产生的任务（全量 / 多个区域）
        for up in self.world.take_light_uploads() {
            match up {
                LightUpload::Full(data) => ctx.renderer.upload_light(&data),
                LightUpload::Region { x, y, w, h, data } => {
                    ctx.renderer.upload_light_region(x, y, w, h, &data)
                }
            }
        }

        // ---- 调试 ----
        if ctx.input.just_pressed(Action::ToggleDebug) {
            self.dbg.open = !self.dbg.open;
        }
        if self.dbg.open && self.tick_count % 120 == 0 {
            let total = (self.world.pixels.w / 128) * (self.world.pixels.h / 128);
            tracing::info!(
                "tick {} | {:.2}ms/tick | sim {:.2} light {:.2} | active_px {} | asleep chunks {}/{} | sprites {}",
                self.tick_count,
                self.tick_ms_sum / 120.0,
                self.world.perf_sim_ms / 120.0,
                self.world.perf_light_ms / 120.0,
                self.world.pixels.active_pixels,
                self.world.pixels.asleep_chunks,
                total,
                ctx.atlas_batch.verts.len() / 6,
            );
            self.tick_ms_sum = 0.0;
            self.world.perf_sim_ms = 0.0;
            self.world.perf_light_ms = 0.0;
        }

        self.tick_ms_sum += t0.elapsed().as_secs_f32() * 1000.0;
        self.tick_count += 1;
    }

    fn render(&mut self, ctx: &mut EngineCtx) {
        self.dbg.frame();
        // ---- 特效编辑器面板（egui，窗口模式）----
        if let Some(egui) = ctx.egui {
            self.ensure_cjk_fonts(egui);
            if self.ide.open {
                // IDE 独占界面：只绘制 IDE（技能 HUD/Boss 血条等是 Area 前景层，会压在面板之上）
                ide::draw(self, egui);
            } else {
                self.monsters.draw_boss_bar(egui);
                inventory::draw(self, egui);
                skills::draw_window(self, egui);
                skills::draw_hud(self, egui);
                let snap = debug::DbgSnapshot::of(self);
                self.dbg.draw(&snap, egui);
                self.settings_ui
                    .draw(&mut self.settings, &mut self.audio, ctx.input, egui);
            }
        }
        // ---- 新手引导 ----
        if self.guide_t > 0.0
            && !self.editor.open
            && !self.inv.ui_open
            && !self.settings_ui.open
            && !self.ide.open
        {
            if let Some(egui) = ctx.egui {
                Area::new(Id::new("guide"))
                    .anchor(Align2::CENTER_TOP, [0.0, 36.0])
                    .show(egui, |ui| {
                      EguiFrame::group(ui.style()).show(ui, |ui| {
                        ui.set_min_width(340.0);
                        ui.vertical_centered(|ui| {
                            ui.heading("欢迎来到 Molecular!");
                        });
                        ui.separator();
                        ui.label("A/D 移动 · 空格 跳跃(可二段跳) · Shift 疾跑");
                        ui.label("左键单击 攻击/挖掘/放置 · 1~8 切换工具");
                        ui.label("I 背包 · Q 喝药 · F1 编辑器 · F3 调试");
                        ui.label("ESC 系统设置 · F5 存档 · F9 读档 · 晚上记得点火把!");
                        ui.small(format!("({:.0}s 后收起)", self.guide_t));
                    });
                });
            }
        }
        // ---- 屏幕提示（背包满等一次性提醒）----
        if self.hint.1 > 0.0 {
            if let Some(egui) = ctx.egui {
                Area::new(Id::new("hint"))
                    .anchor(Align2::CENTER_BOTTOM, [0.0, -72.0])
                    .show(egui, |ui| {
                        EguiFrame::group(ui.style()).show(ui, |ui| {
                            ui.colored_label(egui::Color32::YELLOW, self.hint.0.as_str());
                        });
                    });
            }
        }
        ctx.sky_color = self.world.sky_color();
        ctx.ambient = self.world.ambient();
        let batch: &mut SpriteBatch = ctx.atlas_batch;
        let cam = &*ctx.camera;
        let tl = cam.top_left();
        let vw = cam.viewport.0;
        let vh = cam.viewport.1;

        // ---- 火把精灵（1px 光源像素 + 火焰贴图）----
        let torch_region = self.regions.get("torch").unwrap();
        for &(tx, ty) in &self.world.torches {
            let (fx, fy) = (tx as f32 + 0.5, ty as f32);
            if fx < tl.x - 8.0 || fx > tl.x + vw + 8.0 || fy < tl.y - 12.0 || fy > tl.y + vh + 12.0 {
                continue;
            }
            batch.push_at(Vec2::new(fx, fy - 2.5), Vec2::new(3.0, 7.0), torch_region, [1.0; 4]);
        }

        // ---- 放置预览 ----
        if matches!(self.tool.tool, Tool::Block | Tool::Torch) {
            let m = self.mouse_world;
            if (m - self.player.pos).length() <= tools::REACH {
                let white = self.regions.get("white").unwrap();
                let size = if self.tool.tool == Tool::Block { 4.0 } else { 2.0 };
                batch.push_at(m, Vec2::splat(size), white, [1.0, 1.0, 1.0, 0.35]);
            }
        }

        // ---- 实体 ----
        entities::render(&self.ecs, batch, &self.regions);

        // ---- 玩家（动作姿态 + 装备外观）----
        let shoulder = self.player.pos + Vec2::new(0.0, -14.5);
        let d = self.mouse_world - shoulder;
        let aim = d.y.atan2(d.x);
        let arm_angle = self.action.arm_angle(aim);
        // 装备外观：头盔/胸甲/护腿取稀有度颜色，饰品显示徽章
        let equip_rgb = |i: usize| -> Option<[f32; 3]> {
            self.inv.equip[i]
                .as_ref()
                .map(|it| {
                    let c = self.db.rarity(it).color();
                    [c[0], c[1], c[2]]
                })
        };
        let view = player::PlayerView {
            arm_angle,
            holding_sword: self.tool.tool == Tool::Sword,
            attacking: self.action.current.is_some(),
            casting: self.player.casting,
            helmet: equip_rgb(items::Slot::Head.equip_index()),
            chest: equip_rgb(items::Slot::Chest.equip_index()),
            legs: equip_rgb(items::Slot::Legs.equip_index()),
            trinket: self.inv.equip[items::Slot::Trinket.equip_index()].is_some(),
        };
        player::render(&self.player, batch, &self.regions, &view);

        // ---- 投射物与 VFX（最上层）----
        let white = self.regions.get("white").unwrap();
        projectiles::render(&self.projectiles.list, batch, white, white);
        self.vfx
            .render(batch, white, tl, Vec2::new(tl.x + vw, tl.y + vh));
        self.drops.render(batch, &self.db, white, tl, Vec2::new(tl.x + vw, tl.y + vh));
        self.monsters
            .render(batch, white, tl, Vec2::new(tl.x + vw, tl.y + vh), &self.anims);
        npc::render(&self.npcs.list, batch, white, tl, Vec2::new(tl.x + vw, tl.y + vh));

        // ---- F3 调试叠加（chunk 休眠态 / 碰撞框）----
        let (sc, sh) = (self.dbg.show_chunks, self.dbg.show_hitboxes);
        debug::DebugUi::draw_overlays(self, batch, *white, tl, Vec2::new(tl.x + vw, tl.y + vh), sc, sh);

        // ---- 动画预览（玩家头顶）----
        if let Some(pl) = &self.anim_preview {
            if let Some(r) = pl.region(&self.anims) {
                batch.push_at(
                    self.player.pos + Vec2::new(0.0, -42.0),
                    Vec2::new(r.size[0], r.size[1]),
                    &r,
                    [1.0; 4],
                );
            }
        }

        // ---- 像素世界四边形（地形 + 动态像素同源渲染）----
        let world_w = self.world.pixels.w as f32;
        let world_h = self.world.pixels.h as f32;
        let region = Region {
            uv0: [tl.x / world_w, tl.y / world_h],
            uv1: [(tl.x + vw) / world_w, (tl.y + vh) / world_h],
            size: [vw, vh],
        };
        ctx.world_batch.push(
            Vec2::new(tl.x + vw / 2.0, tl.y + vh / 2.0),
            Vec2::new(vw, vh),
            0.0,
            &region,
            [1.0; 4],
        );
    }
}

fn main() {
    mge_core::logging::init();
    let args: Vec<String> = std::env::args().collect();

    // ---- 批量导入怪物精灵表：--import-sheets <目录> <帧宽> <帧高> ----
    if let Some(p) = args.iter().position(|a| a == "--import-sheets") {
        let dir = args.get(p + 1).map(|s| s.as_str()).unwrap_or("assets_raw");
        let fw = args.get(p + 2).and_then(|s| s.parse().ok()).unwrap_or(150u32);
        let fh = args.get(p + 3).and_then(|s| s.parse().ok()).unwrap_or(150u32);
        let mut bank = anim::AnimBank::load();
        let (n, names) = bank.import_sheets(dir, fw, fh);
        match bank.save() {
            Ok(_) => tracing::info!("导入完成：{n} 张精灵表 [{names:?}]（帧 {fw}×{fh}）→ animations.ron"),
            Err(e) => tracing::error!("保存失败: {e}"),
        }
        return;
    }
    let mut engine = Engine::new((1280, 720));
    if args.iter().any(|a| a == "--selftest") {
        tracing::info!("自测模式启动");
        let mut app = GameApp::new(2026_0924, true);
        engine.run_headless(&mut app, 1350);
        tracing::info!("自测完成");
    } else {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(42);
        let mut app = GameApp::new(seed, false);
        engine.run_windowed(&mut app);
    }
}
