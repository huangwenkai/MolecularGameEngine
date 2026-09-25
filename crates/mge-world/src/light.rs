//! 光照：天空光 + 方块光 双通道 BFS 洪泛
//! 光照网格按 LIGHT_CELL 像素/格 降采样（Noita 式平滑光照），与 1px 地形解耦
//!
//! 性能设计：
//! - 脏区按 chunk 粒度收集，间距大于 2×margin 的区域独立重算（避免包围盒膨胀成全图）
//! - 每帧最多重算配额个区域，积压过多时丢弃最旧的过时区域（2 秒安全网会修正）
//! - 全量重算用不透明位图缓存（BFS 每格多次访问，查表划算）；区域重算直接采样（访问少，缓存反而亏）
use mge_sim::materials::{Kind, Materials};
use mge_sim::{PixelWorld, Rect};
use std::collections::{HashMap, VecDeque};

pub const LIGHT_CELL: i32 = 4;

// 光衰减：空气中每格损耗 / 实心中每格损耗（格 = 4px）
const COST_AIR: u8 = 9;
const COST_SOLID: u8 = 24;
/// 区域重算的外扩 margin（≥ 255/COST_AIR = 28.4 格，保证光不会越过 margin 泄漏）
const REGION_MARGIN: i32 = 34;
/// 每帧最多重算的独立区域数（其余留到后续帧）
const LIGHT_QUOTA: usize = 3;
/// 脏区积压上限：超过则丢弃最旧的过时区域（安全网会修正）
const MAX_PENDING: usize = 9;

/// 格中心是否为遮挡（实心静态像素）
#[inline]
fn is_opaque(pixels: &PixelWorld, mats: &Materials, cx: i32, cy: i32) -> bool {
    let p = pixels.get(cx * LIGHT_CELL + LIGHT_CELL / 2, cy * LIGHT_CELL + LIGHT_CELL / 2);
    let d = mats.def(p.mat);
    d.kind == Kind::Static && d.solid
}

pub struct LightMap {
    pub cw: i32,
    pub ch: i32,
    pub sky: Vec<u8>,
    pub block: Vec<u8>,
    /// 火/岩浆等发光像素按格聚合的临时贡献（每帧由钩子累加，随时间衰减）
    pub emissive: HashMap<(i32, i32), u32>,
    /// 玩家微光位置（格坐标）
    pub player_glow: Option<(i32, i32)>,
    /// 独立脏区列表（格坐标 x0/y0/x1/y1 闭区间）
    dirty: Vec<(i32, i32, i32, i32)>,
    /// 微光移动（仅 block 通道需要重算，天空不受影响）
    glow_dirty: bool,
    /// 上次微光位置（变化时标记脏区）
    last_glow: Option<(i32, i32)>,
}

impl LightMap {
    pub fn new(cw: i32, ch: i32) -> Self {
        Self {
            cw,
            ch,
            sky: vec![0; (cw * ch) as usize],
            block: vec![0; (cw * ch) as usize],
            emissive: HashMap::new(),
            player_glow: None,
            dirty: Vec::new(),
            glow_dirty: false,
            last_glow: None,
        }
    }

    /// 标记脏区域（像素矩形，如 chunk 矩形）。
    /// 与现有脏区接近（间距 < 2×margin）时合并——光会互窜必须一起算；否则独立成区。
    pub fn mark_px_rect(&mut self, r: &Rect) {
        let x0 = r.x / LIGHT_CELL;
        let y0 = r.y / LIGHT_CELL;
        let x1 = (r.x + r.w - 1) / LIGHT_CELL;
        let y1 = (r.y + r.h - 1) / LIGHT_CELL;
        let nr = (x0, y0, x1, y1);
        let gap = REGION_MARGIN * 2;
        for e in self.dirty.iter_mut() {
            if nr.0 <= e.2 + gap && e.0 <= nr.2 + gap && nr.1 <= e.3 + gap && e.1 <= nr.3 + gap {
                *e = (e.0.min(nr.0), e.1.min(nr.1), e.2.max(nr.2), e.3.max(nr.3));
                return;
            }
        }
        self.dirty.push(nr);
    }

    /// 玩家微光移动 → 标记 block 通道脏（静止时不产生脏区）
    pub fn mark_glow(&mut self) {
        if self.player_glow != self.last_glow {
            self.glow_dirty = true;
            self.last_glow = self.player_glow;
        }
    }

    /// 每帧衰减发光贡献（火移动时自然闪烁）
    pub fn decay_emissive(&mut self) {
        for v in self.emissive.values_mut() {
            *v = *v * 3 / 4;
        }
        self.emissive.retain(|_, v| *v >= 24);
    }

    /// 全量重算（安全网：地形大改 / 2 秒一次）
    pub fn relight(&mut self, pixels: &PixelWorld, mats: &Materials, torches: &[(i32, i32)]) {
        let (cw, ch) = (self.cw, self.ch);
        self.sky = vec![0; (cw * ch) as usize];
        self.block = vec![0; (cw * ch) as usize];
        self.dirty.clear();
        self.glow_dirty = false;

        // 全图不透明缓存：一次采样（BFS 每格多次访问，查表划算）
        let mut opq = vec![0u8; (cw * ch) as usize];
        for cy in 0..ch {
            let row = (cy * cw) as usize;
            for cx in 0..cw {
                let p = pixels.get(cx * LIGHT_CELL + LIGHT_CELL / 2, cy * LIGHT_CELL + LIGHT_CELL / 2);
                let d = mats.def(p.mat);
                if d.kind == Kind::Static && d.solid {
                    opq[row + cx as usize] = 1;
                }
            }
        }
        let opaque = |cx: i32, cy: i32| opq[(cy * cw + cx) as usize] != 0;

        let mut queue: VecDeque<(u32, u8)> = VecDeque::with_capacity(8192);

        // ---- 天空光：垂直扫描暴露天空的列 ----
        for cx in 0..cw {
            for cy in 0..ch {
                let i = (cy * cw + cx) as usize;
                if opq[i] != 0 {
                    break;
                }
                self.sky[i] = 255;
                queue.push_back((i as u32, 255));
            }
        }
        bfs_fill(&mut self.sky, &mut queue, cw, &opaque, (0, 0, cw - 1, ch - 1));

        // ---- 方块光：火把 / 发光像素 / 玩家微光 ----
        let mut queue2: VecDeque<(u32, u8)> = VecDeque::with_capacity(1024);
        Self::seed_block(torches, 0, 0, cw - 1, ch - 1, self, &mut queue2);
        bfs_fill(&mut self.block, &mut queue2, cw, &opaque, (0, 0, cw - 1, ch - 1));
    }

    /// 区域重算（每帧配额个，返回实际重算的区域列表供纹理区域上传）。
    /// 像素脏区优先（天空 + 方块双通道）；无像素脏区且微光移动时只重算方块通道。
    pub fn relight_regions(
        &mut self,
        pixels: &PixelWorld,
        mats: &Materials,
        torches: &[(i32, i32)],
    ) -> Vec<(i32, i32, i32, i32)> {
        // 积压保护：模拟变化太快时丢弃最旧的过时区域（安全网会修正）
        if self.dirty.len() > MAX_PENDING {
            let drop = self.dirty.len() - MAX_PENDING;
            self.dirty.drain(..drop);
        }
        let mut done = Vec::new();
        while done.len() < LIGHT_QUOTA {
            let Some(r) = self.dirty.pop() else { break };
            if let Some(b) = self.relight_px_region(pixels, mats, torches, r) {
                done.push(b);
            }
        }
        if done.len() < LIGHT_QUOTA && self.glow_dirty {
            self.glow_dirty = false;
            if let Some((gx, gy)) = self.player_glow {
                const R: i32 = 8; // 微光 48 ≈ 6 格可达，取 8 留余量
                let b = self.relight_glow_region(
                    pixels,
                    mats,
                    torches,
                    (gx - R).max(0).min(self.cw - 1),
                    (gy - R).max(0).min(self.ch - 1),
                    (gx + R).min(self.cw - 1),
                    (gy + R).min(self.ch - 1),
                );
                done.push(b);
            }
        }
        done
    }

    /// 重算单个像素脏区：清零内圈 → 天空列扫描 → 边界环种子 → 双通道 BFS
    fn relight_px_region(
        &mut self,
        pixels: &PixelWorld,
        mats: &Materials,
        torches: &[(i32, i32)],
        r: (i32, i32, i32, i32),
    ) -> Option<(i32, i32, i32, i32)> {
        let ix0 = r.0.max(0).min(self.cw - 1);
        let iy0 = r.1.max(0).min(self.ch - 1);
        let ix1 = r.2.max(0).min(self.cw - 1);
        let iy1 = r.3.max(0).min(self.ch - 1);
        if ix0 > ix1 || iy0 > iy1 {
            return None;
        }
        let ex0 = (ix0 - REGION_MARGIN).max(0);
        let ey0 = (iy0 - REGION_MARGIN).max(0);
        let ex1 = (ix1 + REGION_MARGIN).min(self.cw - 1);
        let ey1 = (iy1 + REGION_MARGIN).min(self.ch - 1);
        let opaque = |cx: i32, cy: i32| is_opaque(pixels, mats, cx, cy);

        // 1) 清零内圈（margin 环保留旧值，作为边界种子）
        for cy in iy0..=iy1 {
            for cx in ix0..=ix1 {
                let i = (cy * self.cw + cx) as usize;
                self.sky[i] = 0;
                self.block[i] = 0;
            }
        }

        // 2) 内圈天空：逐列自世界顶向下找首个遮挡（暴露格 = 255 种子）
        let mut queue: VecDeque<(u32, u8)> = VecDeque::with_capacity(4096);
        for cx in ix0..=ix1 {
            for cy in 0..=iy1 {
                if opaque(cx, cy) {
                    break;
                }
                if cy >= iy0 {
                    let i = (cy * self.cw + cx) as usize;
                    self.sky[i] = 255;
                    queue.push_back((i as u32, 255));
                }
            }
        }
        // 3) 边界环天空种子（光从区域外正确流入）
        self.seed_ring(&self.sky, ex0, ey0, ex1, ey1, &mut queue);
        bfs_fill(&mut self.sky, &mut queue, self.cw, &opaque, (ex0, ey0, ex1, ey1));

        // 4) 方块光：内圈光源种子 + 边界环种子
        let mut queue2: VecDeque<(u32, u8)> = VecDeque::with_capacity(1024);
        Self::seed_block(torches, ix0, iy0, ix1, iy1, self, &mut queue2);
        self.seed_ring(&self.block, ex0, ey0, ex1, ey1, &mut queue2);
        bfs_fill(&mut self.block, &mut queue2, self.cw, &opaque, (ex0, ey0, ex1, ey1));
        Some((ex0, ey0, ex1, ey1))
    }

    /// 重算微光区域：只清零内圈 block → 种子 → 边界环 → 仅方块通道 BFS（跳过天空列扫描）
    fn relight_glow_region(
        &mut self,
        pixels: &PixelWorld,
        mats: &Materials,
        torches: &[(i32, i32)],
        ix0: i32,
        iy0: i32,
        ix1: i32,
        iy1: i32,
    ) -> (i32, i32, i32, i32) {
        let ex0 = (ix0 - REGION_MARGIN).max(0);
        let ey0 = (iy0 - REGION_MARGIN).max(0);
        let ex1 = (ix1 + REGION_MARGIN).min(self.cw - 1);
        let ey1 = (iy1 + REGION_MARGIN).min(self.ch - 1);
        let opaque = |cx: i32, cy: i32| is_opaque(pixels, mats, cx, cy);

        for cy in iy0..=iy1 {
            for cx in ix0..=ix1 {
                self.block[(cy * self.cw + cx) as usize] = 0;
            }
        }
        let mut queue2: VecDeque<(u32, u8)> = VecDeque::with_capacity(256);
        Self::seed_block(torches, ix0, iy0, ix1, iy1, self, &mut queue2);
        self.seed_ring(&self.block, ex0, ey0, ex1, ey1, &mut queue2);
        bfs_fill(&mut self.block, &mut queue2, self.cw, &opaque, (ex0, ey0, ex1, ey1));
        (ex0, ey0, ex1, ey1)
    }

    /// 收集边界环上已有光照值作为 BFS 种子（光从区域外流入）
    fn seed_ring(
        &self,
        map: &[u8],
        ex0: i32,
        ey0: i32,
        ex1: i32,
        ey1: i32,
        queue: &mut VecDeque<(u32, u8)>,
    ) {
        for cy in ey0..=ey1 {
            for cx in ex0..=ex1 {
                if cx > ex0 && cx < ex1 && cy > ey0 && cy < ey1 {
                    continue;
                }
                let i = (cy * self.cw + cx) as usize;
                if map[i] > COST_AIR {
                    queue.push_back((i as u32, map[i]));
                }
            }
        }
    }

    /// 收集区域内的方块光源（火把/发光像素/玩家微光）入队
    fn seed_block(
        torches: &[(i32, i32)],
        ix0: i32,
        iy0: i32,
        ix1: i32,
        iy1: i32,
        me: &mut LightMap,
        queue2: &mut VecDeque<(u32, u8)>,
    ) {
        for &(tx, ty) in torches {
            let (cx, cy) = (tx / LIGHT_CELL, ty / LIGHT_CELL);
            if cx < ix0 || cx > ix1 || cy < iy0 || cy > iy1 {
                continue;
            }
            let i = (cy * me.cw + cx) as usize;
            if me.block[i] < 224 {
                me.block[i] = 224;
                queue2.push_back(((cy * me.cw + cx) as u32, 224));
            }
        }
        for ((px, py), v) in &me.emissive {
            let (cx, cy) = (*px / LIGHT_CELL, *py / LIGHT_CELL);
            if cx < ix0 || cx > ix1 || cy < iy0 || cy > iy1 {
                continue;
            }
            let v = (*v as u8).min(255).max(120);
            let i = (cy * me.cw + cx) as usize;
            if me.block[i] < v {
                me.block[i] = v;
                queue2.push_back(((cy * me.cw + cx) as u32, v));
            }
        }
        if let Some((cx, cy)) = me.player_glow {
            if cx >= ix0 && cx <= ix1 && cy >= iy0 && cy <= iy1 {
                let i = (cy * me.cw + cx) as usize;
                if me.block[i] < 48 {
                    me.block[i] = 48;
                    queue2.push_back(((cy * me.cw + cx) as u32, 48));
                }
            }
        }
    }

    /// 打包为 RG 交错格式供纹理上传
    pub fn build_rg(&self) -> Vec<u8> {
        let n = (self.cw * self.ch) as usize;
        let mut out = Vec::with_capacity(n * 2);
        for i in 0..n {
            out.push(self.sky[i]);
            out.push(self.block[i]);
        }
        out
    }

    /// 打包区域 RG 数据（返回 texel 原点/尺寸 + 数据）
    pub fn build_rg_region(&self, bounds: (i32, i32, i32, i32)) -> (u32, u32, u32, u32, Vec<u8>) {
        let (ex0, ey0, ex1, ey1) = bounds;
        let w = (ex1 - ex0 + 1) as usize;
        let h = (ey1 - ey0 + 1) as usize;
        let mut out = Vec::with_capacity(w * h * 2);
        for cy in ey0..=ey1 {
            let base = (cy * self.cw) as usize;
            for cx in ex0..=ex1 {
                let i = base + cx as usize;
                out.push(self.sky[i]);
                out.push(self.block[i]);
            }
        }
        (ex0 as u32, ey0 as u32, w as u32, h as u32, out)
    }
}

fn bfs_fill(
    map: &mut [u8],
    queue: &mut VecDeque<(u32, u8)>,
    cw: i32,
    opaque: &impl Fn(i32, i32) -> bool,
    bounds: (i32, i32, i32, i32),
) {
    let (bx0, by0, bx1, by1) = bounds;
    while let Some((idx, v)) = queue.pop_front() {
        let x = idx as i32 % cw;
        let y = idx as i32 / cw;
        if v <= COST_AIR {
            continue;
        }
        for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < bx0 || ny < by0 || nx > bx1 || ny > by1 {
                continue;
            }
            let ni = (ny * cw + nx) as usize;
            let nv = v.saturating_sub(if opaque(nx, ny) { COST_SOLID } else { COST_AIR });
            if nv > map[ni] {
                map[ni] = nv;
                queue.push_back(((ny * cw + nx) as u32, nv));
            }
        }
    }
}
