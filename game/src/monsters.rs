//! 怪物与战斗 AI：状态机（巡逻/追击/攻击/撤退）+ 夜间刷怪 + 精英词缀 + BOSS 框架
use crate::items::Stats;
use glam::Vec2;
use mge_core::rng::Rng;
use mge_world::World;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 史莱姆：地面跳跃、接触伤害
    Slime,
    /// 蝙蝠：飞行、直接追击
    Bat,
    /// 骷髅弓手：保持距离射箭
    Archer,
    /// 僵尸：慢速高血近战（A* 寻路追击）
    Zombie,
    /// 地狱犬：快速低血近战（A* 寻路追击）
    Hound,
    /// BOSS：多阶段
    Boss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiState {
    Patrol,
    Chase,
    Flee,
}

/// 精英词缀
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elite {
    /// 迅捷：移速+60%
    Swift,
    /// 坚韧：血量×2.2
    Tank,
    /// 狂暴：低血量时加速+加伤
    Berserk,
}

impl Elite {
    pub fn name(self) -> &'static str {
        match self {
            Elite::Swift => "迅捷",
            Elite::Tank => "坚韧",
            Elite::Berserk => "狂暴",
        }
    }

    pub fn color(self) -> [f32; 4] {
        match self {
            Elite::Swift => [0.5, 0.9, 1.0, 1.0],
            Elite::Tank => [1.0, 0.6, 0.3, 1.0],
            Elite::Berserk => [1.0, 0.3, 0.3, 1.0],
        }
    }
}

#[derive(Clone)]
pub struct Monster {
    pub kind: Kind,
    pub pos: Vec2,
    pub vel: Vec2,
    pub half: Vec2,
    pub hp: f32,
    pub max_hp: f32,
    pub state: AiState,
    pub state_t: f32,
    pub atk_cd: f32,
    pub home: Vec2,
    pub elite: Option<Elite>,
    pub flash: f32,
    pub face: f32,
    pub dmg: f32,
    pub speed: f32,
    pub xp: u32,
    pub boss: bool,
    /// BOSS 阶段（0=P1 撞击 1=P2 弹幕 2=P3 狂暴）
    pub phase: u8,
    pub phase_t: f32,
    pub anim: f32,
    /// A* 路径（世界坐标 waypoints）
    pub path: Vec<Vec2>,
    pub path_i: usize,
    /// 路径重算冷却
    pub path_cd: f32,
}

impl Monster {
    /// 导航方向：沿 A* 路径走向当前 waypoint（到达后推进）；无路径返回 None
    fn nav_dir(&mut self) -> Option<f32> {
        while self.path_i < self.path.len() {
            let wp = self.path[self.path_i];
            let dx = wp.x - self.pos.x;
            if dx.abs() < 8.0 && (wp.y - self.pos.y).abs() < 22.0 {
                self.path_i += 1;
                continue;
            }
            return Some(dx.signum());
        }
        None
    }
}

/// 敌方弹丸（弓手箭 / BOSS 弹幕）
pub struct Bullet {
    pub pos: Vec2,
    pub vel: Vec2,
    pub dmg: f32,
    pub life: f32,
    pub color: [f32; 3],
}

#[derive(Default)]
pub struct Monsters {
    pub list: Vec<Monster>,
    pub bullets: Vec<Bullet>,
    /// 刷怪冷却（秒）
    pub spawn_cd: f32,
    pub boss_alive: bool,
    /// 本夜 BOSS 是否已出（每夜一只）
    boss_spawned_night: bool,
    last_night_t: f32,
}

impl Monsters {
    /// 生成一只怪（应用精英词缀）
    fn spawn_one(&mut self, kind: Kind, pos: Vec2, rng: &mut Rng, elite: Option<Elite>) {
        let (half, hp, dmg, speed, xp) = match kind {
            Kind::Slime => (Vec2::new(5.0, 4.0), 34.0, 8.0, 46.0, 6),
            Kind::Bat => (Vec2::new(4.0, 3.0), 18.0, 6.0, 82.0, 5),
            Kind::Archer => (Vec2::new(4.5, 8.0), 30.0, 0.0, 42.0, 8),
            Kind::Zombie => (Vec2::new(4.5, 9.0), 72.0, 12.0, 30.0, 10),
            Kind::Hound => (Vec2::new(6.0, 5.0), 40.0, 9.0, 95.0, 9),
            Kind::Boss => (Vec2::new(16.0, 14.0), 900.0, 18.0, 60.0, 150),
        };
        let (hp, dmg, speed) = match elite {
            Some(Elite::Tank) => (hp * 2.2, dmg, speed),
            Some(Elite::Swift) => (hp, dmg, speed * 1.6),
            Some(Elite::Berserk) => (hp * 1.2, dmg * 1.3, speed),
            None => (hp, dmg, speed),
        };
        self.list.push(Monster {
            kind,
            pos,
            vel: Vec2::ZERO,
            half,
            hp,
            max_hp: hp,
            state: AiState::Patrol,
            state_t: 0.0,
            atk_cd: 0.0,
            home: pos,
            elite,
            flash: 0.0,
            face: 1.0,
            dmg,
            speed,
            xp,
            boss: kind == Kind::Boss,
            phase: 0,
            phase_t: 0.0,
            anim: rng.range_f32(0.0, 6.0),
            path: Vec::new(),
            path_i: 0,
            path_cd: 0.0,
        });
        if kind == Kind::Boss {
            self.boss_alive = true;
        }
    }

    /// 测试/工具用：直接生成一只怪
    pub fn test_spawn(&mut self, kind: Kind, pos: Vec2, rng: &mut Rng) {
        self.spawn_one(kind, pos, rng, None);
    }

    /// 刷怪系统：夜晚加强、精英概率、每夜 BOSS（玩家 2 级后）
    pub fn spawn_tick(
        &mut self,
        world: &World,
        player_pos: Vec2,
        player_level: u32,
        rng: &mut Rng,
    ) {
        let t = world.time;
        let night = t > 0.58 && t < 0.95;
        // 跨夜重置 BOSS 标记
        if t < self.last_night_t {
            self.boss_spawned_night = false;
        }
        self.last_night_t = t;
        if !night || self.spawn_cd > 0.0 {
            self.spawn_cd -= 1.0 / 60.0;
            return;
        }
        let cap = if night { 10 } else { 3 };
        if self.list.len() >= cap {
            self.spawn_cd = 1.5;
            return;
        }
        self.spawn_cd = rng.range_f32(1.2, 3.0);
        // 位置：玩家周围环带（不直接砸脸）
        let dx: i32 = if rng.chance(0.5) { 1 } else { -1 };
        let px = player_pos.x as i32 + dx * rng.range_i32(240, 460);
        let surface = surface_y(world, px);
        let elite = if night && rng.chance(0.12) {
            match rng.range_i32(0, 2) {
                0 => Some(Elite::Swift),
                1 => Some(Elite::Tank),
                _ => Some(Elite::Berserk),
            }
        } else {
            None
        };
        match rng.range_i32(0, 4) {
            0 => self.spawn_one(Kind::Slime, Vec2::new(px as f32 + 0.5, surface as f32), rng, elite),
            1 => {
                let fly = surface as f32 - rng.range_f32(40.0, 110.0);
                self.spawn_one(Kind::Bat, Vec2::new(px as f32 + 0.5, fly), rng, elite);
            }
            2 => self.spawn_one(Kind::Zombie, Vec2::new(px as f32 + 0.5, surface as f32), rng, elite),
            3 => self.spawn_one(Kind::Hound, Vec2::new(px as f32 + 0.5, surface as f32), rng, elite),
            _ => self.spawn_one(Kind::Archer, Vec2::new(px as f32 + 0.5, surface as f32), rng, elite),
        }
        // 深夜 BOSS：一次性
        if night && !self.boss_spawned_night && !self.boss_alive && player_level >= 2 && t > 0.78 {
            let px = player_pos.x as i32 + dx * rng.range_i32(320, 460);
            let surface = surface_y(world, px);
            self.spawn_one(
                Kind::Boss,
                Vec2::new(px as f32 + 0.5, surface as f32 - 10.0),
                rng,
                None,
            );
            self.boss_spawned_night = true;
        }
    }

    /// AI + 物理 + 攻击。player: (pos, half, hp, mitigation)。
    /// 返回 (死亡 (位置, 经验, 掉落表), 玩家受伤合计, 击退向量)
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        world: &mut World,
        player: (&Vec2, &Vec2, &mut f32, f32), // (pos, half, hp, mitigation)
        st: &Stats,
        vfx: &mut crate::vfx::Vfx,
        rng: &mut Rng,
    ) -> (Vec<(Vec2, u32, &'static str)>, f32, Vec2) {
        let (ppos, phalf, php, mit) = player;
        let pcenter = *ppos - Vec2::new(0.0, phalf.y);
        let mut deaths = Vec::new();
        let mut hurt = 0.0;
        let mut knock = Vec2::ZERO;

        // ---- 弹丸 ----
        self.bullets.retain_mut(|b| {
            b.life -= 1.0 / 60.0;
            if b.life <= 0.0 {
                return false;
            }
            b.pos += b.vel / 60.0;
            let (bx, by) = (b.pos.x as i32, b.pos.y as i32);
            if world.solid_px(bx, by) {
                return false;
            }
            let da = Aabb::new(*ppos - Vec2::new(0.0, phalf.y * 2.0), *phalf * 2.0);
            if da.min.x <= b.pos.x && b.pos.x <= da.max.x && da.min.y <= b.pos.y && b.pos.y <= da.max.y {
                let d = b.dmg * (1.0 - mit);
                *php -= d;
                hurt += d;
                knock += b.vel.normalize_or_zero() * 60.0;
                return false;
            }
            true
        });

        // ---- 怪物 ----
        self.list.retain_mut(|m| {
            m.flash = (m.flash - 1.0 / 60.0).max(0.0);
            m.atk_cd = (m.atk_cd - 1.0 / 60.0).max(0.0);
            m.anim += 1.0 / 60.0;
            let to_p = pcenter - (m.pos - Vec2::new(0.0, m.half.y));
            let dist = to_p.length();
            let sight = if m.kind == Kind::Slime { 240.0 } else { 320.0 };

            // ---- 状态机 ----
            m.state = if m.hp < m.max_hp * 0.18 && m.kind == Kind::Archer {
                AiState::Flee
            } else if dist < sight {
                AiState::Chase
            } else {
                AiState::Patrol
            };

            // ---- 精英狂暴：低血加成 ----
            let rage = m.elite == Some(Elite::Berserk) && m.hp < m.max_hp * 0.4;
            let spd = m.speed * if rage { 1.5 } else { 1.0 };

            // ---- A* 导航：地面怪追击/撤退时周期性重算路径 ----
            m.path_cd -= 1.0 / 60.0;
            if matches!(m.state, AiState::Chase | AiState::Flee)
                && m.kind != Kind::Bat
                && !m.boss
                && m.path_cd <= 0.0
            {
                m.path_cd = 0.5;
                let goal = if m.state == AiState::Flee { m.home } else { pcenter };
                m.path = crate::astar::find_path(
                    world,
                    m.pos - Vec2::new(0.0, m.half.y),
                    goal,
                    320,
                )
                .unwrap_or_default();
                m.path_i = 0;
            }

            match m.kind {
                Kind::Slime => {
                    // 地面跳跃移动（追击沿 A* 路径）
                    m.vel.y += 900.0 / 60.0;
                    let grounded = m.vel.y == 0.0 && world.solid_px(m.pos.x as i32, (m.pos.y + 1.0) as i32);
                    if grounded {
                        let dir = match m.state {
                            AiState::Chase => m.nav_dir().unwrap_or_else(|| fallback_dir(to_p)),
                            AiState::Flee => -to_p.x.signum(),
                            AiState::Patrol => {
                                if m.state_t <= 0.0 {
                                    m.state_t = rng.range_f32(0.5, 1.6);
                                    m.face = if rng.chance(0.5) { 1.0 } else { -1.0 };
                                }
                                m.state_t -= 1.0 / 60.0;
                                m.face
                            }
                        };
                        m.face = dir;
                        if dir != 0.0 {
                            m.vel = Vec2::new(dir * spd, -260.0);
                        }
                    }
                }
                Kind::Bat => {
                    // 飞行：直接扑向玩家（巡逻时绕 home 晃）
                    let target = match m.state {
                        AiState::Chase => pcenter + Vec2::new(0.0, -14.0 + (m.anim * 3.0).sin() * 8.0),
                        _ => m.home + Vec2::new((m.anim * 0.8).cos() * 40.0, (m.anim * 1.3).sin() * 20.0),
                    };
                    let want = (target - m.pos).normalize_or_zero() * spd * (if m.state == AiState::Chase { 1.0 } else { 0.4 });
                    m.vel += (want - m.vel) * 0.08;
                }
                Kind::Archer => {
                    // 保持 120~240 距离，冷却好就射（走位沿 A* 路径）
                    m.vel.y += 900.0 / 60.0;
                    let dir = match m.state {
                        AiState::Chase => {
                            if dist > 240.0 {
                                m.nav_dir().unwrap_or_else(|| fallback_dir(to_p))
                            } else if dist < 120.0 {
                                -to_p.x.signum()
                            } else {
                                0.0
                            }
                        }
                        AiState::Flee => -to_p.x.signum(),
                        AiState::Patrol => 0.0,
                    };
                    m.vel.x = m.vel.x * 0.8 + dir * spd * 0.4;
                    m.face = to_p.x.signum();
                    if m.state == AiState::Chase && m.atk_cd <= 0.0 && dist < 300.0 {
                        m.atk_cd = 1.8;
                        let v = (pcenter - (m.pos - Vec2::new(0.0, m.half.y))).normalize_or_zero() * 210.0;
                        self.bullets.push(Bullet {
                            pos: m.pos - Vec2::new(0.0, m.half.y),
                            vel: v,
                            dmg: 10.0,
                            life: 3.0,
                            color: [0.85, 0.8, 0.6],
                        });
                    }
                }
                Kind::Zombie | Kind::Hound => {
                    // 步行追击（A* 导航），撞墙由通用跳障处理；僵尸慢速高血 / 地狱犬快速低血
                    m.vel.y += 900.0 / 60.0;
                    let grounded =
                        world.solid_px(m.pos.x as i32, (m.pos.y + 1.0) as i32) && m.vel.y >= 0.0;
                    if grounded {
                        m.vel.y = 0.0;
                    }
                    let dir = match m.state {
                        AiState::Chase => m.nav_dir().unwrap_or_else(|| fallback_dir(to_p)),
                        AiState::Flee => -to_p.x.signum(),
                        AiState::Patrol => {
                            if m.state_t <= 0.0 {
                                m.state_t = rng.range_f32(1.0, 2.5);
                                m.face = if rng.chance(0.5) { 1.0 } else { -1.0 };
                            }
                            m.state_t -= 1.0 / 60.0;
                            m.face
                        }
                    };
                    m.face = dir;
                    m.vel.x = dir * spd * 0.9;
                }
                Kind::Boss => {
                    // ---- 多阶段：P1 撞击 / P2 弹幕+撞击 / P3 狂暴 ----
                    let hp_frac = m.hp / m.max_hp;
                    m.phase = if hp_frac > 0.66 {
                        0
                    } else if hp_frac > 0.33 {
                        1
                    } else {
                        2
                    };
                    m.vel.y += 900.0 / 60.0;
                    m.face = to_p.x.signum();
                    m.phase_t += 1.0 / 60.0;
                    let rush_cd = match m.phase {
                        0 => 2.6,
                        1 => 2.0,
                        _ => 1.3,
                    };
                    let grounded = world.solid_px(m.pos.x as i32, (m.pos.y + 1.0) as i32);
                    if grounded && m.atk_cd <= 0.0 {
                        // 阶段切换吼叫粒子
                        let spd_r = spd * match m.phase {
                            0 => 2.2,
                            1 => 2.6,
                            _ => 3.4,
                        };
                        m.vel = Vec2::new(to_p.x.signum() * spd_r, -240.0);
                        m.atk_cd = rush_cd;
                        for _ in 0..10 {
                            vfx.dot(
                                m.pos - Vec2::new(0.0, m.half.y),
                                Vec2::new(rng.range_f32(-60.0, 60.0), rng.range_f32(-90.0, -30.0)),
                                0.4,
                                2.0,
                                [0.9, 0.25, 0.2],
                                0.0,
                                true,
                            );
                        }
                    }
                    // P2/P3 弹幕：环形爆发
                    if m.phase >= 1 && m.phase_t > 3.0 {
                        m.phase_t = 0.0;
                        let n = if m.phase == 1 { 10 } else { 16 };
                        for i in 0..n {
                            let a = i as f32 / n as f32 * std::f32::consts::TAU;
                            self.bullets.push(Bullet {
                                pos: m.pos - Vec2::new(0.0, m.half.y),
                                vel: Vec2::new(a.cos(), a.sin()) * 130.0,
                                dmg: 12.0,
                                life: 3.5,
                                color: [1.0, 0.35, 0.25],
                            });
                        }
                    }
                }
            }

            // ---- 物理（像素碰撞；蝙蝠无重力已单独处理）----
            if m.kind != Kind::Bat {
                // 撞墙跳（地面怪自动越过障碍）
                let steps = 2;
                for _ in 0..steps {
                    let nx = m.pos.x + m.vel.x / 60.0 / steps as f32;
                    if world.solid_px(nx as i32, m.pos.y as i32)
                        && world.solid_px(nx as i32, (m.pos.y - m.half.y) as i32)
                    {
                        if m.vel.y >= 0.0 && world.solid_px(m.pos.x as i32, (m.pos.y + 1.0) as i32) {
                            m.vel.y = -300.0; // 跳过障碍
                        } else {
                            m.vel.x = -m.vel.x * 0.5;
                            break;
                        }
                    } else {
                        m.pos.x = nx;
                    }
                }
                let ny = m.pos.y + m.vel.y / 60.0;
                if world.solid_px(m.pos.x as i32, ny as i32) {
                    m.vel.y = 0.0;
                    // 台阶 ≤4px 自动上（检查前方与头顶都有空间）
                    for lift in 1..=4 {
                        let fx = (m.pos.x + m.vel.x.signum()) as i32;
                        let fy = (ny - lift as f32) as i32;
                        if !world.solid_px(fx, fy)
                            && !world.solid_px(m.pos.x as i32, (m.pos.y - lift as f32) as i32)
                        {
                            m.pos.y -= lift as f32;
                            break;
                        }
                    }
                } else {
                    m.pos.y = ny;
                }
            } else {
                m.pos += m.vel / 60.0;
                if world.solid_px(m.pos.x as i32, m.pos.y as i32) {
                    m.pos -= m.vel / 60.0;
                    m.vel = -m.vel * 0.6;
                }
            }

            // ---- 接触伤害（slime/bat/boss 冲撞）----
            if m.kind != Kind::Archer && m.atk_cd <= 0.6 {
                let ma = Aabb::new(
                    m.pos - Vec2::new(0.0, m.half.y) - m.half,
                    m.half * 2.0,
                );
                let pa = Aabb::new(*ppos - Vec2::new(0.0, phalf.y * 2.0), *phalf * 2.0);
                if ma.intersects(&pa) {
                    let d = m.dmg * (1.0 + if rage { 0.5 } else { 0.0 }) * (1.0 - mit);
                    *php -= d;
                    hurt += d;
                    knock += to_p.normalize_or_zero() * 140.0;
                    m.atk_cd = 0.9;
                }
            }

            // ---- 坠入深层洞穴且远离玩家 → 烟雾清理（掉进被打碎地块的怪不再滞留地下）----
            if m.kind != Kind::Bat
                && m.pos.y > (surface_y(world, m.pos.x as i32) + 96) as f32
                && dist > 480.0
            {
                if m.boss {
                    self.boss_alive = false;
                }
                for _ in 0..14 {
                    let fx = (m.pos.x + rng.range_f32(-6.0, 6.0)) as i32;
                    let fy = (m.pos.y - rng.range_f32(0.0, m.half.y * 2.0)) as i32;
                    world.pixels.spawn(fx, fy, world.pixels.ids.smoke, &world.mats);
                }
                return false; // 无掉落无经验
            }

            // ---- 死亡 ----
            if m.hp <= 0.0 || m.pos.y > (world.pixels.h + 48) as f32 {
                for _ in 0..14 {
                    let fx = (m.pos.x + rng.range_f32(-6.0, 6.0)) as i32;
                    let fy = (m.pos.y - rng.range_f32(0.0, m.half.y * 2.0)) as i32;
                    world.pixels.spawn(fx, fy, world.pixels.ids.smoke, &world.mats);
                }
                deaths.push((m.pos, m.xp, Self::loot_table(m)));
                if m.boss {
                    self.boss_alive = false;
                }
                return false;
            }
            true
        });
        let _ = st;
        (deaths, hurt, knock)
    }

    /// 玩家近战命中检测（命中框内所有怪受击），返回受击位置列表
    pub fn melee_hit(&mut self, hb: &Aabb, dmg: f32, facing: f32, crit: bool) -> Vec<Vec2> {
        let mut hit_pos = Vec::new();
        for m in self.list.iter_mut() {
            let ma = Aabb::new(m.pos - Vec2::new(0.0, m.half.y) - m.half, m.half * 2.0);
            if hb.intersects(&ma) {
                m.hp -= dmg;
                m.flash = 0.18;
                m.vel += Vec2::new(facing * (if crit { 160.0 } else { 100.0 }), -40.0);
                hit_pos.push(m.pos - Vec2::new(0.0, m.half.y));
            }
        }
        hit_pos
    }

    /// 投射物命中检测：命中则扣血返回 Some(位置)
    pub fn proj_hit(&mut self, pos: Vec2, dmg: f32) -> Option<Vec2> {
        for m in self.list.iter_mut() {
            let ma = Aabb::new(m.pos - Vec2::new(0.0, m.half.y) - m.half, m.half * 2.0);
            if ma.min.x <= pos.x && pos.x <= ma.max.x && ma.min.y <= pos.y && pos.y <= ma.max.y {
                m.hp -= dmg;
                m.flash = 0.18;
                return Some(m.pos);
            }
        }
        None
    }

    /// 掉落表名（精英/BOSS 更丰厚）
    pub fn loot_table(m: &Monster) -> &'static str {
        if m.boss {
            "boss_loot"
        } else if m.elite.is_some() {
            "elite_loot"
        } else {
            "monster_loot"
        }
    }

    /// BOSS 血条（egui）
    pub fn draw_boss_bar(&self, ctx: &egui::Context) {
        let Some(boss) = self.list.iter().find(|m| m.boss) else { return };
        egui::TopBottomPanel::top("boss_bar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::from_rgb(255, 80, 70), "魔王");
                ui.label(format!("阶段 {}", boss.phase + 1));
            });
            ui.add(
                egui::ProgressBar::new(boss.hp / boss.max_hp)
                    .fill(egui::Color32::from_rgb(180, 40, 40)),
            );
            ui.add_space(2.0);
        });
    }

    /// 渲染
    pub fn render(
        &self,
        batch: &mut mge_render::SpriteBatch,
        white: &mge_render::Region,
        tl: Vec2,
        br: Vec2,
    ) {
        for m in &self.list {
            if m.pos.x < tl.x - 24.0 || m.pos.x > br.x + 24.0 || m.pos.y < tl.y - 24.0
                || m.pos.y > br.y + 24.0
            {
                continue;
            }
            let mut col = match m.kind {
                Kind::Slime => [0.45, 0.85, 0.4, 1.0],
                Kind::Bat => [0.35, 0.3, 0.38, 1.0],
                Kind::Archer => [0.85, 0.85, 0.9, 1.0],
                Kind::Zombie => [0.35, 0.6, 0.3, 1.0],
                Kind::Hound => [0.62, 0.32, 0.18, 1.0],
                Kind::Boss => [0.75, 0.2, 0.2, 1.0],
            };
            if let Some(e) = m.elite {
                col = e.color();
            }
            if m.flash > 0.0 {
                col = [3.0, 1.5, 1.5, 1.0];
            }
            let bob = if m.kind == Kind::Bat {
                (m.anim * 6.0).sin() * 2.0
            } else {
                0.0
            };
            let c = m.pos + Vec2::new(0.0, -m.half.y + bob);
            batch.push_at(c, m.half * 2.0, white, col);
            // 眼睛（面向）
            batch.push_at(
                c + Vec2::new(m.face * m.half.x * 0.4, -m.half.y * 0.3),
                Vec2::splat(1.5),
                white,
                [0.05, 0.05, 0.08, 1.0],
            );
            // 小血条
            if m.hp < m.max_hp {
                let w = m.half.x * 2.0;
                batch.push_at(c + Vec2::new(0.0, -m.half.y - 3.0), Vec2::new(w, 1.2), white, [
                    0.1, 0.1, 0.1, 0.8,
                ]);
                let frac = (m.hp / m.max_hp).clamp(0.0, 1.0);
                batch.push_at(
                    c + Vec2::new(-(w - w * frac) * 0.5, -m.half.y - 3.0),
                    Vec2::new(w * frac, 1.2),
                    white,
                    [0.9, 0.25, 0.2, 1.0],
                );
            }
        }
        // 弹丸
        for b in &self.bullets {
            batch.push_at(b.pos, Vec2::splat(2.0), white, [
                b.color[0], b.color[1], b.color[2], 1.0,
            ]);
        }
    }
}

use mge_core::math::Aabb;

/// 扫描某像素列的地表
fn surface_y(world: &World, x: i32) -> i32 {
    for y in 0..world.pixels.h {
        if world.solid_px(x, y) {
            return y;
        }
    }
    world.pixels.h / 2
}

/// A* 无路径时的直线回退方向；玩家显著高于自己（自己掉进了坑/洞）→ 停止水平移动，
/// 避免朝洞壁无限撞墙跳跃（等玩家靠近后再战，深层由清理机制回收）
fn fallback_dir(to_p: Vec2) -> f32 {
    if to_p.y < -48.0 {
        0.0
    } else {
        to_p.x.signum()
    }
}
