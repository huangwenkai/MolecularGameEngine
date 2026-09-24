//! 光照：天空光 + 方块光 双通道 BFS 洪泛
//! 光照网格按 LIGHT_CELL 像素/格 降采样（Noita 式平滑光照），与 1px 地形解耦
use mge_sim::materials::{Kind, Materials};
use mge_sim::PixelWorld;
use std::collections::{HashMap, VecDeque};

pub const LIGHT_CELL: i32 = 4;

// 光衰减：空气中每格损耗 / 实心中每格损耗（格 = 4px）
const COST_AIR: u8 = 9;
const COST_SOLID: u8 = 24;

pub struct LightMap {
    pub cw: i32,
    pub ch: i32,
    pub sky: Vec<u8>,
    pub block: Vec<u8>,
    /// 火/岩浆等发光像素按格聚合的临时贡献（每帧由钩子累加，随时间衰减）
    pub emissive: HashMap<(i32, i32), u32>,
    /// 玩家微光位置（格坐标）
    pub player_glow: Option<(i32, i32)>,
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
        }
    }

    /// 每帧衰减发光贡献（火移动时自然闪烁）
    pub fn decay_emissive(&mut self) {
        for v in self.emissive.values_mut() {
            *v = *v * 3 / 4;
        }
        self.emissive.retain(|_, v| *v >= 24);
    }

    /// 全量重算（524k 格 BFS，限频调用）
    pub fn relight(&mut self, pixels: &PixelWorld, mats: &Materials, torches: &[(i32, i32)]) {
        let (cw, ch) = (self.cw, self.ch);
        self.sky = vec![0; (cw * ch) as usize];
        self.block = vec![0; (cw * ch) as usize];

        // 格中心是否遮挡（实心静态像素）
        let opaque = |cx: i32, cy: i32| -> bool {
            let p = pixels.get(cx * LIGHT_CELL + LIGHT_CELL / 2, cy * LIGHT_CELL + LIGHT_CELL / 2);
            let d = mats.def(p.mat);
            d.kind == Kind::Static && d.solid
        };

        let mut queue: VecDeque<(u32, u8)> = VecDeque::with_capacity(8192);

        // ---- 天空光：垂直扫描暴露天空的列 ----
        for cx in 0..cw {
            for cy in 0..ch {
                if opaque(cx, cy) {
                    break;
                }
                self.sky[(cy * cw + cx) as usize] = 255;
                queue.push_back(((cy * cw + cx) as u32, 255));
            }
        }
        bfs_fill(&mut self.sky, &mut queue, cw, ch, &opaque);

        // ---- 方块光：火把 / 发光像素 / 玩家微光 ----
        let mut queue2: VecDeque<(u32, u8)> = VecDeque::with_capacity(1024);
        for &(tx, ty) in torches {
            let (cx, cy) = (tx / LIGHT_CELL, ty / LIGHT_CELL);
            if cx < 0 || cy < 0 || cx >= cw || cy >= ch {
                continue;
            }
            let i = (cy * cw + cx) as usize;
            if self.block[i] < 224 {
                self.block[i] = 224;
                queue2.push_back(((cy * cw + cx) as u32, 224));
            }
        }
        for ((px, py), v) in &self.emissive {
            let (cx, cy) = (*px / LIGHT_CELL, *py / LIGHT_CELL);
            if cx < 0 || cy < 0 || cx >= cw || cy >= ch {
                continue;
            }
            let v = (*v as u8).min(255).max(120);
            let i = (cy * cw + cx) as usize;
            if self.block[i] < v {
                self.block[i] = v;
                queue2.push_back(((cy * cw + cx) as u32, v));
            }
        }
        if let Some((cx, cy)) = self.player_glow {
            let i = (cy * cw + cx) as usize;
            if self.block[i] < 48 {
                self.block[i] = 48;
                queue2.push_back(((cy * cw + cx) as u32, 48));
            }
        }
        bfs_fill(&mut self.block, &mut queue2, cw, ch, &opaque);
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
}

fn bfs_fill(
    map: &mut [u8],
    queue: &mut VecDeque<(u32, u8)>,
    cw: i32,
    ch: i32,
    opaque: &impl Fn(i32, i32) -> bool,
) {
    while let Some((idx, v)) = queue.pop_front() {
        let x = (idx as i32) % cw;
        let y = (idx as i32) / cw;
        if v <= COST_AIR {
            continue;
        }
        for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= cw || ny >= ch {
                continue;
            }
            let ni = (ny * cw + nx) as usize;
            let cost = if opaque(nx, ny) { COST_SOLID } else { COST_AIR };
            let nv = v.saturating_sub(cost);
            if nv > map[ni] {
                map[ni] = nv;
                queue.push_back(((ny * cw + nx) as u32, nv));
            }
        }
    }
}
