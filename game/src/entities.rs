//! 实体：训练假人（hecs ECS 管理）——受击闪白、击退、死亡重生
use glam::Vec2;
use mge_core::rng::Rng;
use mge_world::World;

#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub pos: Vec2, // 脚底中心
}

#[derive(Debug, Clone, Copy)]
pub struct Vel {
    pub v: Vec2,
}

#[derive(Debug, Clone, Copy)]
pub struct Phys {
    pub half: Vec2,
}

#[derive(Debug, Clone)]
pub struct Dummy {
    pub home: Vec2,
    pub hp: f32,
    pub max_hp: f32,
    pub flash: f32,
    pub respawn: u32,
    pub bob: f32,
}

pub fn spawn_dummy(ecs: &mut hecs::World, pos: Vec2) -> hecs::Entity {
    ecs.spawn((
        Transform { pos },
        Vel { v: Vec2::ZERO },
        Phys { half: Vec2::new(6.0, 10.0) },
        Dummy { home: pos, hp: 60.0, max_hp: 60.0, flash: 0.0, respawn: 0, bob: 0.0 },
    ))
}

pub fn update(ecs: &mut hecs::World, world: &mut World, rng: &mut Rng) -> Vec<Vec2> {
    let mut deaths = Vec::new();
    for (_e, (tr, vel, ph, dm)) in ecs
        .query::<(&mut Transform, &mut Vel, &Phys, &mut Dummy)>()
        .iter()
    {
        if dm.respawn > 0 {
            dm.respawn -= 1;
            if dm.respawn == 0 {
                dm.hp = dm.max_hp;
                tr.pos = dm.home;
                vel.v = Vec2::ZERO;
            }
            continue;
        }
        if dm.flash > 0.0 {
            dm.flash -= 1.0 / 60.0;
        }
        dm.bob += 0.05;
        // 物理
        vel.v.y += 900.0 / 60.0;
        if vel.v.y > 420.0 {
            vel.v.y = 420.0;
        }
        vel.v.x *= 0.9;
        // X 轴
        tr.pos.x += vel.v.x / 60.0;
        if solid_overlapping(world, tr.pos, ph.half) {
            tr.pos.x -= vel.v.x / 60.0;
            vel.v.x = -vel.v.x * 0.4;
        }
        // Y 轴
        let prev = tr.pos;
        tr.pos.y += vel.v.y / 60.0;
        if solid_overlapping(world, tr.pos, ph.half) {
            tr.pos = prev;
            vel.v.y = 0.0;
        }
        if tr.pos.y > (world.pixels.h + 32) as f32 {
            dm.hp = 0.0;
        }
        // 死亡 → 烟雾重生
        if dm.hp <= 0.0 {
            for _ in 0..16 {
                let fx = (tr.pos.x + rng.range_f32(-6.0, 6.0)) as i32;
                let fy = (tr.pos.y - rng.range_f32(0.0, 16.0)) as i32;
                world.pixels.spawn(fx, fy, world.pixels.ids.smoke, &world.mats);
            }
            dm.respawn = 240;
            deaths.push(tr.pos);
        }
    }
    deaths
}

fn solid_overlapping(world: &World, pos: Vec2, half: Vec2) -> bool {
    let a = mge_core::math::Aabb::new(pos - Vec2::new(0.0, half.y), half);
    let x0 = a.min.x as i32;
    let x1 = a.max.x as i32;
    let y0 = a.min.y as i32;
    let y1 = a.max.y as i32;
    for ty in y0..=y1 {
        for tx in x0..=x1 {
            if world.solid_px(tx, ty) {
                return true;
            }
        }
    }
    false
}

pub fn render(
    ecs: &hecs::World,
    batch: &mut mge_render::SpriteBatch,
    regions: &std::collections::HashMap<String, mge_render::Region>,
) {
    let mut query = ecs.query::<(&Transform, &Dummy)>();
    for (_e, (tr, dm)) in query.iter() {
        if dm.respawn > 0 {
            continue;
        }
        let region = regions.get("dummy").unwrap();
        let tint: [f32; 4] = if dm.flash > 0.0 {
            [3.0, 1.2, 1.2, 1.0]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };
        let bob = (dm.bob * 2.0).sin() * 0.4;
        batch.push_at(
            tr.pos + Vec2::new(0.0, -10.0 + bob),
            Vec2::new(16.0, 20.0),
            region,
            tint,
        );
    }
}
