//! NPC 生活 AI：需求驱动（口渴/饥饿/疲劳）自主决策、消耗世界资源（喝水/吃浆果）、昼夜作息
//! 验收：观察 NPC 完成「饿→找食物→吃→困→回家睡觉→天亮工作」完整循环；
//!       摧毁水源后会重新扫描寻找替代水源
use glam::Vec2;
use mge_core::rng::Rng;
use mge_world::World;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpcState {
    /// 闲逛
    Idle,
    /// 找水喝
    SeekWater,
    /// 喝水（剩余帧）
    Drink(u16),
    /// 找浆果
    SeekFood,
    /// 进食（剩余帧）
    Eat(u16),
    /// 回家睡觉
    Sleep,
    /// 白天工作（采掘）
    Work,
}

pub struct Npc {
    pub pos: Vec2,
    pub vel: Vec2,
    pub hunger: f32,  // 0 饱 → 100 极饿（决策阈值 40）
    pub thirst: f32,  // 0 饱 → 100 极渴（决策阈值 35）
    pub fatigue: f32, // 0 精神 → 100 困倦（阈值 60 / 夜晚 40）
    pub state: NpcState,
    pub state_t: u16,
    pub target: Vec2,
    pub home: Vec2,
    pub face: f32,
    pub anim: f32,
}

pub struct Npcs {
    pub list: Vec<Npc>,
    wander_cd: f32,
}

impl Default for Npcs {
    fn default() -> Self {
        Self { list: Vec::new(), wander_cd: 0.0 }
    }
}

const WALK: f32 = 34.0;
const JUMP: f32 = -280.0;
const DRINK_FRAMES: u16 = 90;
const EAT_FRAMES: u16 = 90;

impl Npcs {
    /// 在出生点附近放置 NPC 与食物（浆果丛）
    pub fn new(world: &mut World, count: usize, rng: &mut Rng) -> Self {
        let mut list = Vec::new();
        let home = Vec2::new(world.spawn_x as f32 + 0.5, world.spawn_y as f32);
        // 布置浆果丛：出生点周围表面撒几簇
        let berry = world.mats.id("berry").unwrap_or(0);
        for i in 0..6 {
            let dx = rng.range_i32(-260, 260);
            let sy = surface_y(world, world.spawn_x + dx);
            if sy > 2 {
                let y = sy - 1 - (i % 2) as i32;
                world.pixels.set(world.spawn_x + dx, y, mge_sim::Pixel {
                    mat: berry,
                    shade: rng.range_i32(100, 200) as u8,
                    life: 0,
                    aux: 0,
                });
            }
        }
        for i in 0..count {
            let sy = surface_y(world, world.spawn_x + (i as i32 - 1) * 24 - 30);
            list.push(Npc {
                pos: Vec2::new(
                    world.spawn_x as f32 + (i as f32 - 1.0) * 24.0 - 29.5,
                    sy as f32,
                ),
                vel: Vec2::ZERO,
                hunger: rng.range_f32(0.0, 30.0),
                thirst: rng.range_f32(0.0, 30.0),
                fatigue: rng.range_f32(0.0, 20.0),
                state: NpcState::Idle,
                state_t: 0,
                target: home,
                home,
                face: 1.0,
                anim: rng.range_f32(0.0, 6.0),
            });
        }
        Self { list, wander_cd: 0.0 }
    }

    /// 需求随时间增长
    fn tick_needs(n: &mut Npc) {
        n.hunger = (n.hunger + 100.0 / 60.0 / 240.0).min(100.0); // ~4 分钟饿透
        n.thirst = (n.thirst + 100.0 / 60.0 / 180.0).min(100.0); // ~3 分钟渴透
    }

    /// 自主决策（优先级：睡 > 喝 > 吃 > 工作/闲逛）
    fn decide(n: &mut Npc, world: &World, night: bool) {
        // 进食/饮水进行中不打断（需求跌破阈值也不会中途放弃，直到计数归零自然结束）
        if matches!(n.state, NpcState::Eat(_) | NpcState::Drink(_)) {
            return;
        }
        let prev = n.state;
        if n.fatigue > 60.0 || (night && n.fatigue > 35.0) {
            if n.state != NpcState::Sleep {
                n.state = NpcState::Sleep;
                n.target = n.home;
            }
        } else if n.thirst > 35.0 {
            // 进食/饮水中途不被重置（matches! 防止 Eat(90) != Eat(0) 误判）
            if n.state != NpcState::SeekWater && !matches!(n.state, NpcState::Drink(_)) {
                if let Some(t) = find_nearby(world, n.pos, world.pixels.ids.water, 220.0) {
                    n.state = NpcState::SeekWater;
                    n.target = t;
                } else {
                    // 附近无水 → 继续保留状态等待重扫（替代水源逻辑）
                    n.state = NpcState::SeekWater;
                    n.target = n.home;
                }
            }
        } else if n.hunger > 40.0 {
            if n.state != NpcState::SeekFood && !matches!(n.state, NpcState::Eat(_)) {
                if let Some(t) = find_nearby(world, n.pos, world.mats.id("berry").unwrap_or(0), 260.0) {
                    n.state = NpcState::SeekFood;
                    n.target = t;
                } else {
                    n.state = NpcState::SeekFood;
                    n.target = n.home;
                }
            }
        } else if !night {
            n.state = NpcState::Work;
        } else {
            n.state = NpcState::Idle;
        }
        if n.state != prev {
            n.state_t = 0;
        }
    }

    /// 主更新。返回 (状态图标粒子生成位, 是否在睡)
    pub fn update(&mut self, world: &mut World, night: bool, rng: &mut Rng) {
        self.wander_cd = (self.wander_cd - 1.0 / 60.0).max(0.0);
        let water = world.pixels.ids.water;
        for n in self.list.iter_mut() {
            Self::tick_needs(n);
            n.anim += 1.0 / 60.0;
            Self::decide(n, world, night);
            n.state_t += 1;

            // 坠入地下（地块被打碎/天然洞）→ 送回地表，防止在洞穴里永久卡死
            let surf = surface_y(world, n.pos.x as i32);
            if n.pos.y > (surf + 48) as f32 {
                n.pos.y = surf as f32 - 2.0;
                n.vel = Vec2::ZERO;
                n.state = NpcState::Idle;
                n.state_t = 0;
                n.target = n.pos;
            }

            let at_target = (n.pos - n.target).length() < 14.0;
            match n.state {
                NpcState::Sleep => {
                    // 回家 → 蜷睡，疲劳快速恢复
                    n.thirst = (n.thirst + 0.01).min(100.0);
                    if at_target {
                        n.fatigue = (n.fatigue - 0.25).max(0.0);
                        if rng.chance(0.02) {
                            // zZ 粒子
                            world.pixels.spawn(
                                n.pos.x as i32 + rng.range_i32(-2, 4),
                                n.pos.y as i32 - 14,
                                world.pixels.ids.smoke,
                                &world.mats,
                            );
                        }
                    } else {
                        walk_towards(n, spd(n, 0.6), world, rng);
                    }
                }
                NpcState::SeekWater | NpcState::SeekFood => {
                    let want = if n.state == NpcState::SeekWater { water } else { world.mats.id("berry").unwrap_or(0) };
                    // 周期性重扫（水源被毁 → 找替代；到达后低频复核目标有效性）
                    let rescan = n.state_t % 120 == 119 || (at_target && n.state_t % 20 == 0);
                    if rescan {
                        if let Some(t) = find_nearby(world, n.pos, want, if n.state_t > 600 { 480.0 } else { 220.0 }) {
                            n.target = t;
                        }
                    }
                    if at_target {
                        // 到达：喝水/进食（消耗世界像素）
                        if n.state == NpcState::SeekWater {
                            // 必须真的有水才开喝（找不到水源时目标可能只是家）
                            if world.pixels.get(n.target.x as i32, n.target.y as i32).mat == water {
                                n.state = NpcState::Drink(DRINK_FRAMES);
                                n.state_t = 0;
                            }
                        } else {
                            n.state = NpcState::Eat(EAT_FRAMES);
                            n.state_t = 0;
                        }
                    } else {
                        walk_towards(n, spd(n, 1.0), world, rng);
                    }
                }
                NpcState::Drink(frames) => {
                    n.thirst = (n.thirst - 100.0 / DRINK_FRAMES as f32).max(0.0);
                    // 消耗水像素（真实喝水）
                    if frames == DRINK_FRAMES / 2 {
                        let (px, py) = (n.target.x as i32, n.target.y as i32);
                        if world.pixels.get(px, py).mat == water {
                            world.pixels.set(px, py, mge_sim::Pixel::default());
                        }
                    }
                    let next = frames.saturating_sub(1);
                    n.state = if next == 0 {
                        n.thirst = 0.0;
                        NpcState::Idle
                    } else {
                        NpcState::Drink(next)
                    };
                }
                NpcState::Eat(frames) => {
                    n.hunger = (n.hunger - 100.0 / EAT_FRAMES as f32).max(0.0);
                    let next = frames.saturating_sub(1);
                    n.state = if next == 0 {
                        n.hunger = 0.0;
                        NpcState::Idle
                    } else {
                        NpcState::Eat(next)
                    };
                }
                NpcState::Work => {
                    // 挖掘"侧前方"矿物/土模拟劳动（不再挖自己脚下——那会挖穿地块让自己坠落）
                    n.thirst = (n.thirst + 0.03).min(100.0);
                    n.hunger = (n.hunger + 0.03).min(100.0);
                    if n.state_t % 45 == 0 {
                        let side = if rng.chance(0.5) { 1 } else { -1 };
                        let px = n.pos.x as i32 + side * rng.range_i32(8, 22);
                        let py = n.pos.y as i32 + rng.range_i32(-4, 8);
                        if world.solid_px(px, py) {
                            world.mine_px(px, py, 4);
                        }
                    }
                    if n.state_t % 240 == 0 {
                        let dx = if rng.chance(0.5) { 1 } else { -1 };
                        n.target = n.pos + Vec2::new(dx as f32 * rng.range_f32(30.0, 90.0), 0.0);
                    }
                    if !at_target {
                        walk_towards(n, spd(n, 0.5), world, rng);
                    }
                }
                NpcState::Idle => {
                    if n.state_t % 300 == 0 {
                        let dx = if rng.chance(0.5) { 1 } else { -1 };
                        n.target = n.pos + Vec2::new(dx as f32 * rng.range_f32(20.0, 60.0), 0.0);
                    }
                    if !at_target && n.state_t % 300 > 60 {
                        walk_towards(n, spd(n, 0.4), world, rng);
                    }
                    n.fatigue = (n.fatigue + 0.01).min(100.0);
                }
            }
        }
    }
}

fn spd(n: &Npc, k: f32) -> f32 {
    WALK * k * if n.fatigue > 80.0 { 0.5 } else { 1.0 }
}

/// 地面行走（水平 + 跳障）
fn walk_towards(n: &mut Npc, speed: f32, world: &mut World, rng: &mut Rng) {
    let dir = (n.target.x - n.pos.x).signum();
    n.face = dir;
    n.vel.y += 900.0 / 60.0;
    let grounded = world.solid_px(n.pos.x as i32, (n.pos.y + 1.0) as i32) && n.vel.y >= 0.0;
    if grounded {
        n.vel.y = 0.0;
        // 悬崖检测：前方脚下 12px 内无地面 → 停步（防走进被打碎的地块/坑洞）
        let ahead = n.pos.x + dir * 6.0;
        let ground_ahead = (1..=3).any(|k| {
            world.solid_px(ahead as i32, (n.pos.y + k as f32 * 4.0) as i32)
        });
        if ground_ahead {
            n.vel.x = dir * speed;
            // 撞墙/高台阶 → 跳（仅矮障碍 ≤10px；高墙不跳，防止沿峭壁反复跳爬）
            if world.solid_px(ahead as i32, n.pos.y as i32) {
                let wall = (0..24)
                    .take_while(|&k| world.solid_px(ahead as i32, (n.pos.y - k as f32) as i32))
                    .count() as i32;
                if wall <= 10 {
                    n.vel.y = JUMP;
                }
            }
        } else {
            n.vel.x = 0.0;
        }
    } else {
        n.vel.x = n.vel.x * 0.9;
    }
    let _ = rng;
    // X 移动 + 碰撞（被挡且是矮障碍 → 跳；高墙停步）
    let nx = n.pos.x + n.vel.x / 60.0;
    if !world.solid_px(nx as i32, n.pos.y as i32) && !world.solid_px(nx as i32, (n.pos.y - 10.0) as i32) {
        n.pos.x = nx;
    } else if grounded {
        let wall = (0..24)
            .take_while(|&k| world.solid_px(nx as i32, (n.pos.y - k as f32) as i32))
            .count() as i32;
        if wall <= 10 {
            n.vel.y = JUMP;
        } else {
            n.vel.x = 0.0;
        }
    }
    // Y 移动 + 碰撞
    let ny = n.pos.y + n.vel.y / 60.0;
    if !world.solid_px(n.pos.x as i32, ny as i32) {
        n.pos.y = ny;
    } else {
        n.vel.y = 0.0;
    }
}

/// 找附近某材质像素（优先水平距离近的；水面取其上表面）
fn find_nearby(world: &World, from: Vec2, mat: u8, radius: f32) -> Option<Vec2> {
    if mat == 0 {
        return None;
    }
    let (fx, fy) = (from.x as i32, from.y as i32);
    let r = radius as i32;
    let mut best: Option<(i32, Vec2)> = None;
    for dy in (-r..=r).step_by(2) {
        for dx in (-r..=r).step_by(2) {
            let (x, y) = (fx + dx, fy + dy);
            if world.pixels.get(x, y).mat == mat {
                let d = dx * dx + dy * dy;
                if best.map(|(bd, _)| d < bd).unwrap_or(true) {
                    // 水面：向上找水顶
                    let mut top = y;
                    while top > 0 && world.pixels.get(x, top - 1).mat == mat {
                        top -= 1;
                    }
                    best = Some((d, Vec2::new(x as f32 + 0.5, top as f32 + 1.0)));
                }
            }
        }
    }
    best.map(|(_, v)| v)
}

/// 渲染（小人 + 需求提示）
pub fn render(
    list: &[Npc],
    batch: &mut mge_render::SpriteBatch,
    white: &mge_render::Region,
    tl: Vec2,
    br: Vec2,
) {
    for n in list {
        if n.pos.x < tl.x - 16.0 || n.pos.x > br.x + 16.0 || n.pos.y < tl.y - 24.0
            || n.pos.y > br.y + 24.0
        {
            continue;
        }
        let c = n.pos + Vec2::new(0.0, -6.0);
        let skin = [0.9, 0.75, 0.6, 1.0];
        let cloth = match n.state {
            NpcState::Sleep => [0.45, 0.45, 0.6, 1.0],
            NpcState::Work => [0.5, 0.6, 0.45, 1.0],
            NpcState::SeekWater | NpcState::Drink(_) => [0.4, 0.55, 0.85, 1.0],
            NpcState::SeekFood | NpcState::Eat(_) => [0.85, 0.6, 0.35, 1.0],
            NpcState::Idle => [0.7, 0.7, 0.7, 1.0],
        };
        // 身体 + 头
        batch.push_at(c, Vec2::new(5.0, 9.0), white, cloth);
        batch.push_at(c + Vec2::new(0.0, -6.5), Vec2::new(4.0, 4.0), white, skin);
        // 需求指示（头顶色点：蓝=渴 橙=饿 灰=困）
        let mut icon = None;
        if n.thirst > 35.0 {
            icon = Some([0.35, 0.6, 1.0]);
        } else if n.hunger > 40.0 {
            icon = Some([1.0, 0.6, 0.2]);
        } else if n.fatigue > 50.0 {
            icon = Some([0.6, 0.6, 0.65]);
        }
        if let Some(ic) = icon {
            let bob = (n.anim * 4.0).sin() * 0.8;
            batch.push_at(c + Vec2::new(0.0, -11.0 + bob), Vec2::splat(1.6), white, [
                ic[0], ic[1], ic[2], 1.0,
            ]);
        }
    }
}

fn surface_y(world: &World, x: i32) -> i32 {
    for y in 0..world.pixels.h {
        if world.solid_px(x, y) {
            return y;
        }
    }
    world.pixels.h / 2
}
