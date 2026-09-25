//! 交互工具：镐挖掘（像素刷）/ 方块 / 火把 / 水 / 沙（快捷栏 1-6）
use glam::Vec2;
use mge_platform::input::{Action, InputState};
use mge_world::World;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Sword,
    Pickaxe,
    Block,
    Torch,
    Water,
    Sand,
    Bow,
    Fireball,
}

pub const REACH: f32 = 56.0;

impl Tool {
    pub fn from_slot(n: u8) -> Tool {
        match n {
            1 => Tool::Sword,
            2 => Tool::Pickaxe,
            3 => Tool::Block,
            4 => Tool::Torch,
            5 => Tool::Water,
            6 => Tool::Sand,
            7 => Tool::Bow,
            _ => Tool::Fireball,
        }
    }
}

pub struct ToolCtx {
    pub tool: Tool,
    pub place_cooldown: u8,
    pub scoop_cooldown: u8,
}

impl Default for ToolCtx {
    fn default() -> Self {
        Self { tool: Tool::Sword, place_cooldown: 0, scoop_cooldown: 0 }
    }
}

/// 返回 (本帧是否应挥剑, 震屏)
pub fn update(
    t: &mut ToolCtx,
    input: &InputState,
    world: &mut World,
    player_pos: Vec2,
    player_half: Vec2,
    mouse_world: Vec2,
    busy_with_action: bool,
) -> (bool, f32) {
    let mut shake = 0.0;
    let mut sword_swing = false;
    if t.place_cooldown > 0 {
        t.place_cooldown -= 1;
    }
    if t.scoop_cooldown > 0 {
        t.scoop_cooldown -= 1;
    }

    let dist = (mouse_world - player_pos).length();
    let in_reach = dist <= REACH;
    // 2px 吸附：放置类工具对齐偶数坐标，保证直线
    let snap = |v: f32| ((v as i32) >> 1) << 1;
    let cx = snap(mouse_world.x);
    let cy = snap(mouse_world.y);
    let free = mouse_world.x as i32;
    let fy = mouse_world.y as i32;

    // 放置保护：目标像素与玩家身体（外扩 1px）重叠时禁止放置（防止把自己砌进地形）
    let overlaps_player = |x: i32, y: i32| -> bool {
        let px = x as f32 + 0.5;
        let py = y as f32 + 0.5;
        px >= player_pos.x - player_half.x - 1.0
            && px <= player_pos.x + player_half.x + 1.0
            && py >= player_pos.y - player_half.y * 2.0 - 1.0
            && py <= player_pos.y + 1.0
    };

    match t.tool {
        Tool::Sword => {
            sword_swing = input.just_pressed(Action::Attack);
        }
        // 以下工具均为左键单击触发单次（悬停不生效）
        Tool::Pickaxe if in_reach && !busy_with_action && input.just_pressed(Action::Attack) => {
            // 像素刷挖掘：圆形刷内累积伤害，单击最多破坏 26 像素（挖掘不吸附，保留自由手感）
            let r = 6;
            let mut breaks = 0;
            'outer: for dy in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dy * dy > r * r {
                        continue;
                    }
                    if world.mine_px(free + dx, fy + dy, 3) {
                        breaks += 1;
                        if breaks >= 26 {
                            break 'outer;
                        }
                    }
                }
            }
            if breaks > 0 {
                shake = 0.5;
            }
            // 舀取松散像素（水/沙等）
            if t.scoop_cooldown == 0 {
                let r = 3;
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx * dx + dy * dy <= r * r {
                            world.pixels.clear_px(free + dx, fy + dy);
                        }
                    }
                }
                t.scoop_cooldown = 5;
            }
        }
        Tool::Block if in_reach && input.just_pressed(Action::Attack) => {
            if t.place_cooldown == 0 {
                // 放置 4x4 石块（2px 吸附 + 玩家保护）
                let mat = world.mats.id("stone").unwrap_or(0);
                let mut placed = false;
                for dy in -2..2 {
                    for dx in -2..2 {
                        let (tx, ty) = (cx + dx, cy + dy);
                        if world.pixels.get(tx, ty).mat == 0
                            && !world.solid_px(tx, ty)
                            && !overlaps_player(tx, ty)
                        {
                            world.pixels.spawn(tx, ty, mat, &world.mats);
                            placed = true;
                        }
                    }
                }
                if placed {
                    world.mark_terrain_dirty();
                    t.place_cooldown = 6;
                }
            }
        }
        Tool::Torch if in_reach && input.just_pressed(Action::Attack) => {
            if t.place_cooldown == 0 && world.place_torch(cx, cy) {
                t.place_cooldown = 8;
            }
        }
        Tool::Water | Tool::Sand if in_reach && input.just_pressed(Action::Attack) => {
            // 单击倒一勺（约 16 像素）
            let mat = match t.tool {
                Tool::Water => world.mats.id("water").unwrap_or(0),
                _ => world.mats.id("sand").unwrap_or(0),
            };
            for _ in 0..16 {
                let dx = (world_rng(world) % 7) - 3;
                let dy = (world_rng(world) % 7) - 3;
                if dx * dx + dy * dy <= 9 && !world.solid_px(free + dx, fy + dy) {
                    world.pixels.spawn(free + dx, fy + dy, mat, &world.mats);
                }
            }
        }
        _ => {}
    }
    (sword_swing, shake)
}

// 工具层无独立随机流：借用世界像素随机源
fn world_rng(world: &mut World) -> i32 {
    let r = world.pixels.rng().next_u32();
    (r % 7) as i32
}
