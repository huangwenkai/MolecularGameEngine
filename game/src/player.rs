//! 玩家：角色控制器（走跑跳/二段跳/攀爬/爬墙滑落/游泳/下落伤害）+ 程序化动画
//! 地形为 1px 像素粒度（Noita 式）：实心静态像素碰撞；结块粉末可站立不陷落
use glam::Vec2;
use mge_core::rng::Rng;
use mge_platform::input::{Action, InputState};
use mge_world::World;

pub const GRAVITY: f32 = 900.0;
pub const RUN: f32 = 130.0;
pub const ACCEL: f32 = 1200.0;
pub const FRICTION: f32 = 1000.0;
pub const JUMP_V: f32 = 300.0;
pub const MAX_FALL: f32 = 420.0;

#[derive(Debug, Clone)]
pub struct Player {
    pub pos: Vec2,   // 脚底中心
    pub vel: Vec2,
    pub half: Vec2,  // 碰撞半尺寸
    pub facing: f32,
    pub on_ground: bool,
    pub coyote: u8,
    pub jump_buf: u8,
    pub jumps_left: u8,
    pub climbing: bool,
    pub in_water: bool,
    pub wading: bool,
    pub wall_slide: bool,
    pub fall_peak: f32,
    pub anim_t: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub hurt_flash: f32,
    pub on_fire: u16,
    pub dead: u32,
    /// 移速倍率（装备/词缀聚合，默认 1.0）
    pub move_mult: f32,
}

impl Player {
    pub fn new(pos: Vec2) -> Self {
        Self {
            pos,
            vel: Vec2::ZERO,
            half: Vec2::new(4.5, 11.0),
            facing: 1.0,
            on_ground: false,
            coyote: 0,
            jump_buf: 0,
            jumps_left: 1,
            climbing: false,
            in_water: false,
            wading: false,
            wall_slide: false,
            fall_peak: 0.0,
            anim_t: 0.0,
            hp: 100.0,
            max_hp: 100.0,
            hurt_flash: 0.0,
            on_fire: 0,
            dead: 0,
            move_mult: 1.0,
        }
    }

    pub fn aabb(&self) -> mge_core::math::Aabb {
        mge_core::math::Aabb::new(self.pos - Vec2::new(0.0, self.half.y), self.half)
    }
}

/// 该像素是否实心
#[inline]
fn is_solid(world: &World, x: i32, y: i32) -> bool {
    world.solid_px(x, y)
}

/// AABB 是否与实心像素重叠
fn aabb_solid(world: &World, pos: Vec2, half: Vec2) -> bool {
    let a = mge_core::math::Aabb::new(pos - Vec2::new(0.0, half.y), half);
    let x0 = a.min.x as i32;
    let x1 = a.max.x as i32;
    let y0 = a.min.y as i32;
    let y1 = a.max.y as i32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            if is_solid(world, x, y) {
                return true;
            }
        }
    }
    false
}

/// 水平位移与碰撞，支持 ≤8px 自动上台阶（像素级踏步）
fn move_axis_x(world: &World, pos: &mut Vec2, half: Vec2, dx: f32, allow_step: bool) -> bool {
    let desired_x = pos.x + dx;
    pos.x = desired_x;
    let a = mge_core::math::Aabb::new(*pos - Vec2::new(0.0, half.y), half);
    let y0 = a.min.y as i32;
    let y1 = a.max.y as i32;
    let x_scan = if dx > 0.0 {
        (a.max.x as i32)..=(a.max.x as i32)
    } else {
        (a.min.x as i32)..=(a.min.x as i32)
    };
    for y in y0..=y1 {
        for x in x_scan.clone() {
            if is_solid(world, x, y) {
                // 尝试自动上台阶：障碍顶面在脚上方 ≤8px 时抬升
                let rise = pos.y - y as f32;
                if allow_step && rise > 0.0 && rise <= 8.0 {
                    let saved_y = pos.y;
                    pos.y = y as f32;
                    pos.x = desired_x;
                    if !aabb_solid(world, *pos, half) {
                        return false; // 成功踏上
                    }
                    pos.y = saved_y;
                }
                if dx > 0.0 {
                    pos.x = x as f32 - half.x - 0.01;
                } else if dx < 0.0 {
                    pos.x = (x + 1) as f32 + half.x + 0.01;
                }
                return true;
            }
        }
    }
    false
}

/// 垂直位移与碰撞（实心 + 单向平台），返回 (撞实心, 落在平台)
fn move_axis_y(world: &World, pos: &mut Vec2, half: Vec2, dy: f32, drop_through: bool) -> (bool, bool) {
    let prev_bottom = pos.y + half.y;
    pos.y += dy;
    let a = mge_core::math::Aabb::new(*pos - Vec2::new(0.0, half.y), half);
    let x0 = a.min.x as i32;
    let x1 = a.max.x as i32;
    let y_scan = if dy > 0.0 {
        (a.max.y as i32)..=(a.max.y as i32)
    } else if dy < 0.0 {
        (a.min.y as i32)..=(a.min.y as i32)
    } else {
        return (false, false);
    };
    for y in y_scan {
        for x in x0..=x1 {
            let p = world.pixels.get(x, y);
            let def = world.mats.def(p.mat);
            if def.solid {
                if dy > 0.0 {
                    pos.y = y as f32 - 0.01; // 落在像素顶（pos.y 为脚底）
                } else {
                    pos.y = (y + 1) as f32 + half.y * 2.0 + 0.01; // 头顶撞底
                }
                return (true, false);
            } else if def.platform && dy > 0.0 && !drop_through {
                // 单向平台：仅从上方穿过顶面时落上
                let top = y as f32;
                if prev_bottom <= top + 0.5 && pos.y >= top - 0.5 {
                    pos.y = top - 0.01;
                    return (false, true);
                }
            }
        }
    }
    (false, false)
}

/// 一帧玩家更新，返回震屏强度
pub fn update(p: &mut Player, input: &InputState, world: &mut World, rng: &mut Rng) -> f32 {
    let mut shake = 0.0;
    if p.dead > 0 {
        p.dead -= 1;
        if p.dead == 0 {
            p.hp = p.max_hp;
            p.pos = Vec2::new(world.spawn_x as f32 + 0.5, world.spawn_y as f32);
            p.vel = Vec2::ZERO;
        }
        return shake;
    }
    let dt = 1.0 / 60.0;
    let center = p.pos - Vec2::new(0.0, p.half.y);
    let feet = p.pos - Vec2::new(0.0, 1.0);

    // ---- 环境感知 ----
    let cmat = |pos: Vec2| world.pixels.get(pos.x as i32, pos.y as i32).mat;
    let water_id = world.mats.id("water").unwrap_or(0);
    let acid_id = world.mats.id("acid").unwrap_or(0);
    let lava_id = world.mats.id("lava").unwrap_or(0);
    p.in_water = cmat(center) == water_id || cmat(center) == acid_id;
    let feet_mat = cmat(feet);
    // 粉末层（沙/雪/碎屑）：结块的可站立（不陷落），流动的会陷落
    let (_, feet_in_powder) = powder_ground(world, p);
    p.wading = feet_in_powder || (feet_mat != 0 && powder_kind(world, feet_mat));
    let in_lava = cmat(center) == lava_id || cmat(feet) == lava_id;
    if in_lava {
        p.on_fire = 240;
        p.hurt_flash = 0.2;
        p.hp -= 0.5;
    }
    if p.in_water {
        p.on_fire = 0;
    } else if p.on_fire > 0 {
        p.on_fire -= 1;
        p.hp -= 0.03;
        // 身上冒火
        if rng.chance(0.4) {
            let fx = (p.pos.x + rng.range_f32(-p.half.x, p.half.x)) as i32;
            let fy = (p.pos.y - rng.range_f32(0.0, p.half.y * 2.0)) as i32;
            if world.pixels.get(fx, fy).mat == 0 {
                world.pixels.spawn(fx, fy, world.pixels.ids.fire, &world.mats);
            }
        }
    }

    // ---- 意图 ----
    let mut intent = 0.0f32;
    if input.pressed(Action::Left) {
        intent -= 1.0;
    }
    if input.pressed(Action::Right) {
        intent += 1.0;
    }
    let drop = input.pressed(Action::Down);

    // ---- 攀爬 ----
    let can_climb = world.climbable_px(center.x as i32, center.y as i32)
        || world.climbable_px(p.pos.x as i32, p.pos.y as i32);
    if can_climb && (input.pressed(Action::Up) || p.climbing) && !p.on_ground {
        p.climbing = true;
    }
    if p.on_ground && !input.pressed(Action::Up) {
        p.climbing = false;
    }

    if p.climbing {
        p.vel.y = if input.pressed(Action::Up) {
            -80.0
        } else if input.pressed(Action::Down) && !drop {
            80.0
        } else {
            0.0
        };
        if drop {
            p.climbing = false;
        }
        p.vel.x = intent * 60.0;
        if input.just_pressed(Action::Jump) {
            p.climbing = false;
            p.vel.y = -JUMP_V * 0.85;
        }
    } else {
        // ---- 水平 ----
        let top_speed = if p.in_water { 70.0 } else { RUN * p.move_mult };
        let target = intent * top_speed;
        let rate = if intent != 0.0 { ACCEL } else { FRICTION };
        p.vel.x = mge_core::math::Aabb::approach(p.vel.x, target, rate * dt);

        // 前方粉末：低矮可直接踏上，高墙阻挡（踩沙踩雪不下沉）
        if intent != 0.0 {
            let (step_y, blocked) = powder_ahead(world, p, intent);
            if blocked {
                p.vel.x = 0.0;
            } else if let Some(ny) = step_y {
                if p.on_ground {
                    p.pos.y = ny;
                }
            }
        }

        // ---- 重力 ----
        let g = if p.in_water { 240.0 } else { GRAVITY };
        p.vel.y += g * dt;
        let max_fall = if p.in_water {
            90.0
        } else if p.wall_slide {
            60.0
        } else {
            MAX_FALL
        };
        if p.vel.y > max_fall {
            p.vel.y = max_fall;
        }

        // ---- 跳跃（缓冲 + 土狼 + 二段跳 + 蹬墙跳）----
        if input.just_pressed(Action::Jump) {
            p.jump_buf = 6;
        }
        let mut jumped = false;
        if p.jump_buf > 0 {
            if p.on_ground || p.coyote > 0 {
                p.vel.y = -JUMP_V;
                p.jump_buf = 0;
                p.coyote = 0;
                p.jumps_left = 1;
                jumped = true;
            } else if p.wall_slide {
                // 蹬墙跳
                let wall_dir = wall_dir_at(world, p);
                p.vel.x = -wall_dir * 190.0;
                p.vel.y = -JUMP_V * 0.95;
                p.jump_buf = 0;
                jumped = true;
            } else if p.jumps_left > 0 && !p.in_water {
                p.vel.y = -JUMP_V * 0.92;
                p.jumps_left -= 1;
                p.jump_buf = 0;
                jumped = true;
            } else if p.in_water {
                p.vel.y = -110.0; // 游泳划水
                p.jump_buf = 0;
                jumped = true;
            }
        }
        if p.jump_buf > 0 {
            p.jump_buf -= 1;
        }
        if jumped && p.wading {
            // 从沙/雪中跳出踢起碎屑
            for _ in 0..4 {
                let fx = (p.pos.x + rng.range_f32(-5.0, 5.0)) as i32;
                let fy = (p.pos.y - 1.0) as i32;
                world.pixels.spawn(fx, fy - 2, feet_mat_for(world, feet_mat), &world.mats);
            }
        }

        // ---- 爬墙滑落判定 ----
        p.wall_slide = false;
        if !p.on_ground && p.vel.y > 0.0 && !p.in_water {
            if intent < 0.0 && wall_dir_at(world, p) < 0.0 {
                p.wall_slide = true;
            }
            if intent > 0.0 && wall_dir_at(world, p) > 0.0 {
                p.wall_slide = true;
            }
        }
    }

    // ---- 位移与碰撞 ----
    if p.vel.x != 0.0 {
        move_axis_x(world, &mut p.pos, p.half, p.vel.x * dt, p.on_ground);
    }
    let dy = p.vel.y * dt;
    let (hit_solid, hit_platform) = move_axis_y(world, &mut p.pos, p.half, dy, drop && !p.on_ground);
    if hit_solid || hit_platform {
        if p.vel.y > 0.0 {
            // 落地
            if !p.on_ground {
                let impact = p.fall_peak;
                if impact > 340.0 {
                    let dmg = (impact - 340.0) * 0.06;
                    p.hp -= dmg;
                    p.hurt_flash = 0.25;
                    shake = 4.0 + dmg * 0.3;
                    // 尘土
                    for _ in 0..8 {
                        let fx = (p.pos.x + rng.range_f32(-6.0, 6.0)) as i32;
                        let fy = (p.pos.y - 2.0) as i32;
                        world.pixels.spawn(fx, fy, world.pixels.ids.smoke, &world.mats);
                    }
                }
                if p.wading {
                    // 踩进松散物
                    for _ in 0..3 {
                        let fx = (p.pos.x + rng.range_f32(-5.0, 5.0)) as i32;
                        world
                            .pixels
                            .spawn(fx, p.pos.y as i32 - 2, feet_mat_for(world, feet_mat), &world.mats);
                    }
                }
            }
            p.on_ground = true;
            p.fall_peak = 0.0;
            p.jumps_left = 1;
            p.coyote = 6;
        }
        p.vel.y = 0.0;
    } else {
        p.on_ground = false;
        if p.coyote > 0 {
            p.coyote -= 1;
        }
        if p.vel.y > p.fall_peak {
            p.fall_peak = p.vel.y;
        }
    }

    // ---- 粉末支撑：结块沙/雪/碎屑可站立（陷入 1px，不下沉），高速落入缓冲免摔伤 ----
    if p.vel.y >= 0.0 {
        let (support, _) = powder_ground(world, p);
        if let Some(sup) = support {
            // 到达/穿过支撑面（8px 窗口）→ 落定；或被新沉积的粉末缓缓托起
            if p.pos.y >= sup && p.pos.y - sup <= 8.0 {
                let new_pos = Vec2::new(p.pos.x, sup);
                if !aabb_solid(world, new_pos, p.half) {
                    if !p.on_ground && p.fall_peak > 340.0 {
                        // 粉末缓冲：无摔伤，扬尘
                        for _ in 0..6 {
                            let fx = (p.pos.x + rng.range_f32(-6.0, 6.0)) as i32;
                            world
                                .pixels
                                .spawn(fx, p.pos.y as i32 - 2, world.pixels.ids.smoke, &world.mats);
                        }
                        shake = 2.0;
                    }
                    p.pos.y = sup;
                    p.on_ground = true;
                    p.jumps_left = 1;
                    p.coyote = 6;
                    p.vel.y = 0.0;
                    p.fall_peak = 0.0;
                }
                // 上方被堵（悬岩/嵌入）→ 不托起，交给脱困器处理
            }
        }
    }

    // ---- 脱困：身体与实心像素重叠（被沙埋/生成异常）时向上挤出，防止穿地 ----
    if aabb_solid(world, p.pos, p.half) {
        for lift in 1..=30 {
            if !aabb_solid(world, Vec2::new(p.pos.x, p.pos.y - lift as f32), p.half) {
                p.pos.y -= lift as f32;
                p.vel.y = p.vel.y.min(0.0);
                break;
            }
        }
    }

    // 世界边界
    p.pos.x = p.pos.x.clamp(16.0, (world.pixels.w - 16) as f32);
    if p.pos.y > (world.pixels.h + 32) as f32 {
        p.hp = 0.0;
    }

    // ---- 朝向 ----
    if intent != 0.0 {
        p.facing = intent.signum();
    }

    // ---- 生命 ----
    if p.hurt_flash > 0.0 {
        p.hurt_flash -= dt;
    }
    if p.hp <= 0.0 {
        // 死亡：爆烟 + 重生
        for _ in 0..20 {
            let fx = (p.pos.x + rng.range_f32(-8.0, 8.0)) as i32;
            let fy = (p.pos.y - rng.range_f32(0.0, 20.0)) as i32;
            world.pixels.spawn(fx, fy, world.pixels.ids.smoke, &world.mats);
        }
        p.dead = 180;
        shake = 6.0;
    } else if p.hp < p.max_hp {
        p.hp = (p.hp + 0.02).min(p.max_hp);
    }

    // 动画计时
    p.anim_t += p.vel.x.abs() * dt * 0.09;
    shake
}

fn feet_mat_for(world: &World, feet_mat: u8) -> u8 {
    if feet_mat != 0 {
        feet_mat
    } else {
        world.pixels.ids.stone_debris
    }
}

/// 材质是否为粉末类（沙/雪/碎屑等可陷落材质）
fn powder_kind(world: &World, mat: u8) -> bool {
    mat != 0 && matches!(world.mats.def(mat).kind, mge_sim::materials::Kind::Powder)
}

/// 该像素格是否为已结块粉末（可支撑角色）
fn settled_powder(world: &World, x: i32, y: i32) -> bool {
    let px = world.pixels.get(x, y);
    powder_kind(world, px.mat) && px.is_settled()
}

/// 脚部粉末探测：跨 3 列采样，寻找最高结块粉末面
/// 返回 (支撑面 y —— 脚底应停留处（陷 1px）, 是否处于粉末中)
fn powder_ground(world: &World, p: &Player) -> (Option<f32>, bool) {
    let mut support: Option<f32> = None;
    let mut in_powder = false;
    let feet = p.pos.y as i32;
    for sx in [p.pos.x - 2.5, p.pos.x, p.pos.x + 2.5] {
        let x = sx as i32;
        for y in (feet - 9)..=(feet + 3) {
            if settled_powder(world, x, y) {
                in_powder = true;
                let sup = y as f32 + 1.0; // 站在粉末表面，仅陷 1px
                support = Some(match support {
                    Some(s) => s.min(sup),
                    None => sup,
                });
                break;
            }
        }
    }
    (support, in_powder)
}

/// 前方粉末：可踏上（面比脚高 ≤8px）返回新脚底 y；过高则阻挡
/// 返回 (踏上面 y, 是否被阻挡)
fn powder_ahead(world: &World, p: &Player, dir: f32) -> (Option<f32>, bool) {
    let x = (p.pos.x + dir * (p.half.x + 1.0)) as i32;
    let feet = p.pos.y as i32;
    for y in (feet - 9)..=(feet + 2) {
        if settled_powder(world, x, y) {
            let rise = p.pos.y - y as f32;
            if rise > 0.0 && rise <= 8.0 {
                // 上去后身体是否有空间
                if !aabb_solid(world, Vec2::new(p.pos.x, y as f32 - 1.0), p.half) {
                    return (Some(y as f32 - 1.0), false);
                }
            }
            // 粉末比脚低（下坡方向）→ 直接走过去
            if rise <= 0.0 {
                return (None, false);
            }
            return (None, true); // 高墙阻挡
        }
    }
    (None, false)
}

/// 检测身侧是否紧贴墙（返回 -1/0/1）
fn wall_dir_at(world: &World, p: &Player) -> f32 {
    let mid_y = p.pos.y - p.half.y;
    let probe = 1.5;
    let left = is_solid(world, (p.pos.x - p.half.x - probe) as i32, mid_y as i32)
        || is_solid(world, (p.pos.x - p.half.x - probe) as i32, (mid_y - 6.0) as i32);
    let right = is_solid(world, (p.pos.x + p.half.x + probe) as i32, mid_y as i32)
        || is_solid(world, (p.pos.x + p.half.x + probe) as i32, (mid_y - 6.0) as i32);
    if left {
        -1.0
    } else if right {
        1.0
    } else {
        0.0
    }
}

/// 推入一个部件：有人物形象贴图用贴图（受伤时染色），否则回退纯色块
#[allow(clippy::too_many_arguments)]
fn push_part(
    batch: &mut mge_render::SpriteBatch,
    regions: &std::collections::HashMap<String, mge_render::Region>,
    white: &mge_render::Region,
    key: &str,
    center: Vec2,
    size: Vec2,
    color: [f32; 4],
    tex_tint: [f32; 4],
) {
    match regions.get(key) {
        Some(r) => batch.push_at(center, size, r, tex_tint),
        None => batch.push_at(center, size, white, color),
    }
}

/// 程序化动画渲染：分层部件 + 手臂持械跟随（部件外观可逐像素编辑）
#[allow(clippy::too_many_arguments)]
pub fn render(
    p: &Player,
    batch: &mut mge_render::SpriteBatch,
    regions: &std::collections::HashMap<String, mge_render::Region>,
    arm_angle: f32,
    holding_sword: bool,
) {
    let white = regions.get("white").unwrap();
    let sword = regions.get("sword").unwrap();
    let flash = p.hurt_flash > 0.0;
    let tint = |c: [f32; 3]| -> [f32; 4] {
        if flash {
            [2.5, c[1] * 0.3, c[2] * 0.3, 1.0]
        } else {
            [c[0], c[1], c[2], 1.0]
        }
    };
    let skin = tint([0.88, 0.68, 0.55]);
    let shirt = tint([0.24, 0.47, 0.78]);
    let pants = tint([0.26, 0.26, 0.34]);
    let hair = tint([0.35, 0.24, 0.12]);
    let shoe = tint([0.20, 0.20, 0.22]);
    // 自定义形象贴图的着色（正常显示原色；受伤闪红）
    let tex_tint: [f32; 4] = if flash { [2.5, 0.3, 0.3, 1.0] } else { [1.0; 4] };

    let f = p.facing;
    let base = p.pos; // 脚底中心
    let walking = p.vel.x.abs() > 8.0 && p.on_ground;
    let swing = if walking { (p.anim_t * std::f32::consts::TAU).sin() } else { 0.0 };
    let bob = if walking { (p.anim_t * std::f32::consts::TAU * 2.0).sin().abs() * 0.8 } else { 0.0 };
    let airborne = !p.on_ground && !p.climbing && !p.in_water;
    let leg_lift = if airborne { 2.0 } else { 0.0 };

    // 腿（两条，交错摆动）
    let leg1 = base + Vec2::new(-1.8 + swing * 1.6 * f, -4.0 - leg_lift + bob * 0.3);
    let leg2 = base + Vec2::new(1.8 - swing * 1.6 * f, -4.0 + leg_lift * 0.3 + bob * 0.3);
    push_part(batch, regions, white, "char_leg", leg1, Vec2::new(4.0, 8.0), pants, tex_tint);
    push_part(batch, regions, white, "char_leg", leg2, Vec2::new(4.0, 8.0), pants, tex_tint);
    // 鞋
    push_part(batch, regions, white, "char_shoe", leg1 + Vec2::new(0.0, 3.5), Vec2::new(4.0, 2.0), shoe, tex_tint);
    push_part(batch, regions, white, "char_shoe", leg2 + Vec2::new(0.0, 3.5), Vec2::new(4.0, 2.0), shoe, tex_tint);

    // 躯干
    let torso = base + Vec2::new(0.0, -11.5 - bob * 0.5);
    push_part(batch, regions, white, "char_torso", torso, Vec2::new(8.0, 9.0), shirt, tex_tint);

    // 后臂（反向摆）
    let shoulder_b = torso + Vec2::new(-1.0 * f, -3.0);
    let back_ang = if p.climbing {
        -2.4
    } else {
        std::f32::consts::PI + swing * 0.7 * f
    };
    let arm_len = 7.5;
    let hand_b = shoulder_b + Vec2::new(back_ang.cos() * arm_len * 0.5, back_ang.sin() * arm_len * 0.5);
    if regions.contains_key("char_arm") {
        batch.push(hand_b, Vec2::new(4.0, 8.0), back_ang, regions.get("char_arm").unwrap(), tex_tint);
    } else {
        batch.push(hand_b, Vec2::new(3.0, 8.0), back_ang, white, shirt);
    }

    // 头 + 头发（朝左时水平镜像，让五官朝向正确）
    let head = torso + Vec2::new(0.5 * f, -6.0);
    push_part(batch, regions, white, "char_head", head, Vec2::new(8.0 * f, 8.0), skin, tex_tint);
    push_part(
        batch,
        regions,
        white,
        "char_hair",
        head + Vec2::new(-0.5 * f, -2.6),
        Vec2::new(8.0 * f, 4.0),
        hair,
        tex_tint,
    );

    // 前臂（持械，指向 arm_angle）
    let shoulder_f = torso + Vec2::new(1.2 * f, -3.0);
    let hand_f = shoulder_f + Vec2::new(arm_angle.cos() * arm_len * 0.6, arm_angle.sin() * arm_len * 0.6);
    if regions.contains_key("char_arm") {
        batch.push(hand_f, Vec2::new(4.0, 8.0), arm_angle, regions.get("char_arm").unwrap(), tex_tint);
    } else {
        batch.push(hand_f, Vec2::new(3.0, 8.0), arm_angle, white, skin);
    }

    // 剑：挂在手上，沿手臂方向
    if holding_sword {
        let grip = hand_f + Vec2::new(arm_angle.cos() * 2.0, arm_angle.sin() * 2.0);
        batch.push(grip, Vec2::new(22.0, 7.0), arm_angle, sword, [1.0; 4]);
    }
}
