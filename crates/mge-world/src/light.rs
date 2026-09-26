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
// 传播半径 = 亮度 / COST_AIR：火把 255 → 36 格 ≈ 145px 圆形光斑
const COST_AIR: u8 = 7;
const COST_SOLID: u8 = 24;
/// 对角衰减（×√2：7→10 / 24→34），8 邻接传播使光斑呈圆形而非菱形
const COST_DIAG: u8 = 10;
const COST_SOLID_DIAG: u8 = 34;
/// 区域重算的外扩 margin（≥ 255/COST_AIR ≈ 36.4 格，保证光不会越过 margin 泄漏）
const REGION_MARGIN: i32 = 40;
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

/// 区间包含判断
#[inline]
fn cx_ok(v: i32, lo: i32, hi: i32) -> bool {
    v >= lo && v <= hi
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
    /// 移动光源（格坐标 + 强度，如火球），外部每帧写入
    pub moving_lights: Vec<(i32, i32, u8)>,
    /// 独立脏区列表（格坐标 x0/y0/x1/y1 闭区间）
    dirty: Vec<(i32, i32, i32, i32)>,
    /// 微光/移动光源变化（仅 block 通道需要重算，天空不受影响）
    glow_dirty: bool,
    /// 上次微光位置（变化时标记脏区）
    last_glow: Option<(i32, i32)>,
    /// 上次移动光源（变化时标记脏区）
    last_moving: Vec<(i32, i32, u8)>,
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
            moving_lights: Vec::new(),
            dirty: Vec::new(),
            glow_dirty: false,
            last_glow: None,
            last_moving: Vec::new(),
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

    /// 玩家微光/移动光源变化 → 标记 block 通道脏（静止时不产生脏区）
    pub fn mark_glow(&mut self) {
        if self.player_glow != self.last_glow || self.moving_lights != self.last_moving {
            self.glow_dirty = true;
            self.last_glow = self.player_glow;
            self.last_moving = self.moving_lights.clone();
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
            if let Some(b) = self.relight_glow_region(pixels, mats, torches) {
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

    /// 重算移动光源区域：只清零内圈 block → 种子 → 边界环 → 仅方块通道 BFS（跳过天空列扫描）。
    /// 内圈 = 玩家微光 + 全部移动光源的联合包围盒 ± R。
    fn relight_glow_region(
        &mut self,
        pixels: &PixelWorld,
        mats: &Materials,
        torches: &[(i32, i32)],
    ) -> Option<(i32, i32, i32, i32)> {
        // 所有移动光源的联合包围盒
        const R: i32 = 26; // 微光 72≈10 格、火球 170≈24 格可达（COST_AIR=7），取 26 留余量
        let mut b = (self.cw, self.ch, 0, 0); // 反向初始值，任意光源都会收窄
        if let Some((gx, gy)) = self.player_glow {
            b.0 = b.0.min(gx);
            b.1 = b.1.min(gy);
            b.2 = b.2.max(gx);
            b.3 = b.3.max(gy);
        }
        for &(lx, ly, _) in &self.moving_lights {
            b.0 = b.0.min(lx);
            b.1 = b.1.min(ly);
            b.2 = b.2.max(lx);
            b.3 = b.3.max(ly);
        }
        if b.0 > b.2 {
            // 无任何移动光源
            return None;
        }
        let ix0 = (b.0 - R).max(0).min(self.cw - 1);
        let iy0 = (b.1 - R).max(0).min(self.ch - 1);
        let ix1 = (b.2 + R).min(self.cw - 1);
        let iy1 = (b.3 + R).min(self.ch - 1);
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
        Some((ex0, ey0, ex1, ey1))
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
            if me.block[i] < 255 {
                me.block[i] = 255;
                queue2.push_back(((cy * me.cw + cx) as u32, 255));
            }
        }
        for ((px, py), v) in &me.emissive {
            let (cx, cy) = (*px / LIGHT_CELL, *py / LIGHT_CELL);
            if cx < ix0 || cx > ix1 || cy < iy0 || cy > iy1 {
                continue;
            }
            let v = (*v as u8).min(255).max(160);
            let i = (cy * me.cw + cx) as usize;
            if me.block[i] < v {
                me.block[i] = v;
                queue2.push_back(((cy * me.cw + cx) as u32, v));
            }
        }
        if let Some((gx, gy)) = me.player_glow {
            if cx_ok(gx, ix0, ix1) && cx_ok(gy, iy0, iy1) {
                let i = (gy * me.cw + gx) as usize;
                if me.block[i] < 72 {
                    me.block[i] = 72;
                    queue2.push_back(((gy * me.cw + gx) as u32, 72));
                }
            }
        }
        // 移动光源（火球等）
        for &(lx, ly, lv) in &me.moving_lights {
            if cx_ok(lx, ix0, ix1) && cx_ok(ly, iy0, iy1) {
                let i = (ly * me.cw + lx) as usize;
                if me.block[i] < lv {
                    me.block[i] = lv;
                    queue2.push_back(((ly * me.cw + lx) as u32, lv));
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

/// 8 邻接方向与对应衰减（正交 / 对角）——等距面为圆形
const NEIGH: [(i32, i32, u8, u8); 8] = [
    (-1, 0, COST_AIR, COST_SOLID),
    (1, 0, COST_AIR, COST_SOLID),
    (0, -1, COST_AIR, COST_SOLID),
    (0, 1, COST_AIR, COST_SOLID),
    (-1, -1, COST_DIAG, COST_SOLID_DIAG),
    (1, -1, COST_DIAG, COST_SOLID_DIAG),
    (-1, 1, COST_DIAG, COST_SOLID_DIAG),
    (1, 1, COST_DIAG, COST_SOLID_DIAG),
];

/// 光传播：8 邻接 + 两档边权 → 按亮度降序的桶式 Dijkstra（等距面为圆形）。
/// 入队 seeds（值, 格索引）；同层处理期间只可能写入更低的桶，降序遍历即正确。
fn bfs_fill(
    map: &mut [u8],
    queue: &mut VecDeque<(u32, u8)>,
    cw: i32,
    opaque: &impl Fn(i32, i32) -> bool,
    bounds: (i32, i32, i32, i32),
) {
    let (bx0, by0, bx1, by1) = bounds;
    let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); 256];
    while let Some((idx, v)) = queue.pop_front() {
        buckets[v as usize].push(idx);
    }
    for v in (1..=255u8).rev() {
        let level = v as usize;
        if buckets[level].is_empty() {
            continue;
        }
        let cur = std::mem::take(&mut buckets[level]);
        for idx in cur {
            let x = idx as i32 % cw;
            let y = idx as i32 / cw;
            for &(dx, dy, ca, cs) in &NEIGH {
                let nx = x + dx;
                let ny = y + dy;
                if nx < bx0 || ny < by0 || nx > bx1 || ny > by1 {
                    continue;
                }
                let ni = (ny * cw + nx) as usize;
                let nv = v.saturating_sub(if opaque(nx, ny) { cs } else { ca });
                if nv > map[ni] {
                    map[ni] = nv;
                    if nv > 0 {
                        buckets[nv as usize].push(ni as u32);
                    }
                }
            }
        }
    }
}
