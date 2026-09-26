//! 局部网格 A* 寻路（怪物导航）：以起点为中心开窗、4px 格、8 方向（防穿角）
//! 像素世界全图寻路不现实，这里只在目标周围窗口内搜索，失败时调用方回退启发式
use glam::Vec2;
use mge_world::World;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

const CELL: i32 = 4;
const MAX_ITER: usize = 6000;

type Cell = (i32, i32);

fn solid_cell(world: &World, c: Cell) -> bool {
    world.solid_px(c.0 * CELL, c.1 * CELL)
}

/// 重力可走：格子非实心且脚下 3 格（12px）内有地面（怪物靠重力/跳跃站立行走）。
/// 向下移动视为坠落无需此检查——防止路径横穿悬空缺口或给出爬不上去的竖井。
fn grounded_cell(world: &World, c: Cell) -> bool {
    (1..=3).any(|dy| world.solid_px(c.0 * CELL, (c.1 + dy) * CELL))
}

#[inline]
fn key(c: Cell) -> i64 {
    ((c.1 as i64) << 32) | (c.0 as i64 & 0xFFFF_FFFF)
}

/// 求路径：返回路径点（世界坐标，含终点，不含起点）；失败返回 None
pub fn find_path(world: &World, start: Vec2, goal: Vec2, window_px: i32) -> Option<Vec<Vec2>> {
    let s = ((start.x as i32) / CELL, (start.y as i32) / CELL);
    let mut g = ((goal.x as i32) / CELL, (goal.y as i32) / CELL);
    if s == g {
        return Some(vec![goal]);
    }
    // 目标格不可走或悬空（玩家贴墙/跳跃中）→ 向下找最近的"可站立"格
    // （优先脚下有地面的；深坑上方找不到时兜底取最近空格）
    if solid_cell(world, g) || !grounded_cell(world, g) {
        let mut fallback = None;
        let mut found = None;
        for dy in 1..=12i32 {
            let c = (g.0, g.1 + dy);
            if !solid_cell(world, c) {
                if fallback.is_none() {
                    fallback = Some(c);
                }
                if grounded_cell(world, c) {
                    found = Some(c);
                    break;
                }
            }
        }
        g = found.or(fallback)?;
    }
    let win = window_px / CELL; // 格子半径
    let win2 = (win * win) as f32;

    let h = |c: Cell| -> f32 {
        let dx = (c.0 - g.0).abs();
        let dy = (c.1 - g.1).abs();
        (dx.max(dy) as f32) + 0.414 * (dx.min(dy) as f32)
    };

    // 优先级用 i64（分数 ×1024 取整，避开 f32 无 Ord）
    let enc = |v: f32| -> i64 { (v * 1024.0) as i64 };

    let mut open: BinaryHeap<Reverse<(i64, i64)>> = BinaryHeap::new();
    let mut came: HashMap<i64, i64> = HashMap::new();
    let mut gs: HashMap<i64, f32> = HashMap::new();
    open.push(Reverse((enc(h(s)), key(s))));
    gs.insert(key(s), 0.0);

    let dirs: [(i32, i32, f32); 8] = [
        (1, 0, 1.0),
        (-1, 0, 1.0),
        (0, 1, 1.0),
        (0, -1, 1.0),
        (1, 1, 1.414),
        (1, -1, 1.414),
        (-1, 1, 1.414),
        (-1, -1, 1.414),
    ];

    let mut found = false;
    for _ in 0..MAX_ITER {
        let Reverse((_, k)) = open.pop()?;
        if k == key(g) {
            found = true;
            break;
        }
        let cur = (k as i32, (k >> 32) as i32);
        let cg = *gs.get(&k).unwrap_or(&f32::INFINITY);
        for (dx, dy, cost) in dirs {
            let n = (cur.0 + dx, cur.1 + dy);
            // 窗口限制
            let ddx = (n.0 - s.0) as f32;
            let ddy = (n.1 - s.1) as f32;
            if ddx * ddx + ddy * ddy > win2 {
                continue;
            }
            if solid_cell(world, n) {
                continue;
            }
            // 重力可走：水平/向上移动要求落点脚下有地面（dy>0 向下 = 坠落，允许）
            if dy <= 0 && !grounded_cell(world, n) {
                continue;
            }
            // 防穿角：对角需要两个正交格都可走
            if dx != 0 && dy != 0
                && (solid_cell(world, (cur.0 + dx, cur.1)) || solid_cell(world, (cur.0, cur.1 + dy)))
            {
                continue;
            }
            let ng = cg + cost;
            let nk = key(n);
            if ng < *gs.get(&nk).unwrap_or(&f32::INFINITY) {
                gs.insert(nk, ng);
                came.insert(nk, k);
                open.push(Reverse((enc(ng + h(n)), nk)));
            }
        }
    }
    if !found {
        return None;
    }
    // 回溯
    let mut cells = Vec::new();
    let mut k = key(g);
    while k != key(s) {
        cells.push(((k & 0xFFFF_FFFF) as i32, (k >> 32) as i32));
        k = *came.get(&k)?;
    }
    cells.reverse();
    // 简化：每 4 格取一点 + 终点
    let step = CELL as f32;
    let mut pts = Vec::new();
    for (i, c) in cells.iter().enumerate() {
        if i % 4 == 3 || i == cells.len() - 1 {
            pts.push(Vec2::new(c.0 as f32 * step + step / 2.0, c.1 as f32 * step + step / 2.0));
        }
    }
    if pts.is_empty() {
        pts.push(goal);
    }
    Some(pts)
}
