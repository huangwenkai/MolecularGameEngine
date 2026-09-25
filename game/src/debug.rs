//! F3 调试面板 + 场景叠加可视化（chunk 休眠态 / 实体碰撞框）
use crate::GameApp;
use glam::Vec2;
use mge_render::{Region, SpriteBatch};
use std::time::Instant;

#[derive(Default)]
pub struct DebugUi {
    pub open: bool,
    pub show_chunks: bool,
    pub show_hitboxes: bool,
    pub fps: f32,
    last_frame: Option<Instant>,
}

impl DebugUi {
    /// 每显示帧调用：统计 FPS（指数平滑）
    pub fn frame(&mut self) {
        let now = Instant::now();
        if let Some(t) = self.last_frame {
            let dt = now.duration_since(t).as_secs_f32();
            if dt > 0.0 {
                let f = 1.0 / dt;
                self.fps = if self.fps <= 0.0 { f } else { self.fps + (f - self.fps) * 0.08 };
            }
        }
        self.last_frame = Some(now);
    }

    /// 场景叠加：视口内 chunk 休眠态着色 + 实体碰撞框
    pub fn draw_overlays(
        game: &mut GameApp,
        batch: &mut SpriteBatch,
        white: Region,
        tl: Vec2,
        br: Vec2,
        show_chunks: bool,
        show_hitboxes: bool,
    ) {
        if show_chunks {
            let cs = 128.0f32;
            let (cols, rows) = game.world.pixels.chunk_grid();
            let c0 = (tl.x / cs).floor().max(0.0) as i32;
            let r0 = (tl.y / cs).floor().max(0.0) as i32;
            let c1 = ((br.x / cs).ceil() as i32).min(cols);
            let r1 = ((br.y / cs).ceil() as i32).min(rows);
            for cy in r0..r1 {
                for cx in c0..c1 {
                    let asleep = game.world.pixels.chunk_asleep(cx, cy);
                    let col = if asleep {
                        [0.2, 1.0, 0.4, 0.07]
                    } else {
                        [1.0, 0.3, 0.15, 0.14]
                    };
                    batch.push_at(
                        Vec2::new(cx as f32 * cs, cy as f32 * cs),
                        Vec2::splat(cs),
                        &white,
                        col,
                    );
                }
            }
        }
        if show_hitboxes {
            // 玩家（蓝）
            let p = &game.player;
            box_fill(batch, &white, p.pos + Vec2::new(0.0, -p.half.y), p.half, [0.3, 0.6, 1.0, 0.25]);
            // 怪物（红）
            for m in &game.monsters.list {
                box_fill(batch, &white, m.pos + Vec2::new(0.0, -m.half.y), m.half, [1.0, 0.25, 0.2, 0.28]);
            }
            // 假人（黄）
            let mut q = game.ecs.query::<(&crate::entities::Transform, &crate::entities::Phys)>();
            for (_e, (tr, ph)) in q.iter() {
                box_fill(
                    batch,
                    &white,
                    tr.pos + Vec2::new(0.0, -ph.half.y),
                    ph.half,
                    [1.0, 0.9, 0.2, 0.22],
                );
            }
        }
    }

    /// egui 调试面板（快照传参避免借用冲突）
    pub fn draw(&mut self, s: &DbgSnapshot, egui: &egui::Context) {
        egui::Window::new("调试 (F3)")
            .open(&mut self.open)
            .default_pos([16.0, 300.0])
            .default_width(260.0)
            .show(egui, |ui| {
                ui.checkbox(&mut self.show_chunks, "显示 chunk 休眠态（绿=休眠 红=活动）");
                ui.checkbox(&mut self.show_hitboxes, "显示碰撞框");
                ui.separator();
                egui::Grid::new("dbg_stats")
                    .num_columns(2)
                    .spacing([12.0, 3.0])
                    .show(ui, |ui| {
                        let fps_col = if self.fps >= 55.0 {
                            egui::Color32::GREEN
                        } else if self.fps >= 30.0 {
                            egui::Color32::YELLOW
                        } else {
                            egui::Color32::RED
                        };
                        ui.label("FPS");
                        ui.colored_label(fps_col, format!("{:.0}", self.fps));
                        end_row(ui);
                        ui.label("tick 耗时");
                        ui.label(format!("{:.2} ms", s.tick_ms));
                        end_row(ui);
                        ui.label("活动像素");
                        ui.label(format!("{}", s.active_px));
                        end_row(ui);
                        ui.label("休眠 chunk");
                        ui.label(format!("{}/{}", s.asleep, s.chunks.0 * s.chunks.1));
                        end_row(ui);
                        ui.label("怪物");
                        ui.label(format!("{}", s.monsters));
                        end_row(ui);
                        ui.label("掉落物");
                        ui.label(format!("{}", s.drops));
                        end_row(ui);
                        ui.label("投射物");
                        ui.label(format!("{}", s.projectiles));
                        end_row(ui);
                        ui.label("粒子/飘字");
                        ui.label(format!("{} / {}", s.particles, s.texts));
                        end_row(ui);
                        ui.label("NPC");
                        ui.label(format!("{}", s.npcs));
                        end_row(ui);
                    });
                ui.separator();
                egui::Grid::new("dbg_player")
                    .num_columns(2)
                    .spacing([12.0, 3.0])
                    .show(ui, |ui| {
                        ui.label("玩家位置");
                        ui.label(format!("({:.0}, {:.0})", s.pos.x, s.pos.y));
                        end_row(ui);
                        ui.label("生命");
                        ui.label(format!("{:.0}/{:.0}", s.hp, s.max_hp));
                        end_row(ui);
                        ui.label("等级/经验");
                        ui.label(format!("{} / {}", s.level, s.xp));
                        end_row(ui);
                        ui.label("金币");
                        ui.label(format!("{}", s.gold));
                        end_row(ui);
                        ui.label("世界时间");
                        ui.label(format!("{:.2}", s.time));
                        end_row(ui);
                    });
            });
    }
}

/// 面板数据快照（每帧复制，无借用）
pub struct DbgSnapshot {
    pub tick_ms: f32,
    pub active_px: u64,
    pub asleep: u32,
    pub chunks: (i32, i32),
    pub monsters: usize,
    pub drops: usize,
    pub projectiles: usize,
    pub particles: usize,
    pub texts: usize,
    pub npcs: usize,
    pub pos: Vec2,
    pub hp: f32,
    pub max_hp: f32,
    pub level: u32,
    pub xp: u32,
    pub gold: u32,
    pub time: f32,
}

impl DbgSnapshot {
    pub fn of(game: &GameApp) -> Self {
        Self {
            tick_ms: game.tick_ms_sum / game.tick_count.max(1) as f32,
            active_px: game.world.pixels.active_pixels,
            asleep: game.world.pixels.asleep_chunks,
            chunks: game.world.pixels.chunk_grid(),
            monsters: game.monsters.list.len(),
            drops: game.drops.list.len(),
            projectiles: game.projectiles.list.len(),
            particles: game.vfx.particles.len(),
            texts: game.vfx.texts.len(),
            npcs: game.npcs.list.len(),
            pos: game.player.pos,
            hp: game.player.hp,
            max_hp: game.player.max_hp,
            level: game.inv.level,
            xp: game.inv.xp,
            gold: game.inv.gold,
            time: game.world.time,
        }
    }
}

fn end_row(ui: &mut egui::Ui) {
    ui.end_row();
}

fn box_fill(
    batch: &mut SpriteBatch,
    white: &Region,
    center: Vec2,
    half: Vec2,
    col: [f32; 4],
) {
    batch.push_at(center - half, half * 2.0, white, col);
}
