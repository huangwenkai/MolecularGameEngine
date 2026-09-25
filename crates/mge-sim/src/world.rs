//! 像素世界：分块存储 + CPU 下降沙模拟（保底实现）
//!
//! 设计要点：
//! - 世界按 CHUNK_PX 分块，静止块休眠，任何写入唤醒所在块及相邻块
//! - 粉末/液体静止后打 settled 标记，邻居变化时解除
//! - 火/岩浆/酸为反应性材质，休眠块中的特殊材质仍低频反应
use crate::materials::{Kind, MaterialDef, MaterialId, Materials, Special, EMPTY, OOB};
use mge_core::rng::Rng;

pub const CHUNK_PX: usize = 128;
const NB4: [(i32, i32); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];

/// 单个模拟像素（4 字节）
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pixel {
    pub mat: u8,
    /// 颜色明度抖动（渲染用）
    pub shade: u8,
    /// 寿命（气体）；0 表示无寿命
    pub life: u8,
    /// bit0: settled 结块；bit1: 水平方向（1=右）
    pub aux: u8,
}

impl Pixel {
    pub fn is_settled(&self) -> bool {
        self.aux & 1 != 0
    }
    fn dir(&self) -> i32 {
        if self.aux & 2 != 0 {
            1
        } else {
            -1
        }
    }
    fn with_dir(mut self, d: i32) -> Self {
        if d > 0 {
            self.aux |= 2;
        } else {
            self.aux &= !2;
        }
        self
    }
    fn unsettle(mut self) -> Self {
        self.aux &= !1;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// 模拟与外部（光照等）交互的钩子
pub trait SimHooks {
    /// 累积发光贡献（火/岩浆所在像素，坐标为模拟像素）
    fn add_emissive(&mut self, x: i32, y: i32, amount: u8) {
        let _ = (x, y, amount);
    }
}

struct Chunk {
    px: Box<[Pixel]>,
    active: u32,
    asleep: bool,
}

/// 常用材质 id 缓存
#[derive(Clone, Copy, Default, Debug)]
pub struct SimIds {
    pub water: MaterialId,
    pub steam: MaterialId,
    pub smoke: MaterialId,
    pub fire: MaterialId,
    pub stone_debris: MaterialId,
}

impl SimIds {
    pub fn new(mats: &Materials) -> Self {
        Self {
            water: mats.id("water").unwrap_or(0),
            steam: mats.id("steam").unwrap_or(0),
            smoke: mats.id("smoke").unwrap_or(0),
            fire: mats.id("fire").unwrap_or(0),
            stone_debris: mats.id("stone_debris").unwrap_or(0),
        }
    }
}

pub struct PixelWorld {
    pub w: i32,
    pub h: i32,
    cw: i32,
    ch: i32,
    chunks: Vec<Chunk>,
    dirty: Option<Rect>,
    rng: Rng,
    tick: u64,
    pub ids: SimIds,
    /// 本帧活跃像素统计（活跃+特殊）
    pub active_pixels: u64,
    pub asleep_chunks: u32,
}

impl PixelWorld {
    pub fn new(w: i32, h: i32, mats: &Materials) -> Self {
        assert!(w % CHUNK_PX as i32 == 0 && h % CHUNK_PX as i32 == 0);
        let cw = w / CHUNK_PX as i32;
        let ch = h / CHUNK_PX as i32;
        let chunks = (0..(cw * ch) as usize)
            .map(|_| Chunk {
                px: vec![Pixel::default(); CHUNK_PX * CHUNK_PX].into_boxed_slice(),
                active: 0,
                asleep: true,
            })
            .collect();
        let mut s = Self {
            w,
            h,
            cw,
            ch,
            chunks,
            dirty: None,
            rng: Rng::from_entropy(),
            tick: 0,
            ids: SimIds::new(mats),
            active_pixels: 0,
            asleep_chunks: 0,
        };
        s.mark_dirty(Rect { x: 0, y: 0, w, h });
        s
    }

    pub fn rng(&mut self) -> &mut Rng {
        &mut self.rng
    }

    /// chunk 网格尺寸（列, 行）——调试可视化用
    pub fn chunk_grid(&self) -> (i32, i32) {
        (self.cw, self.ch)
    }

    /// 某chunk是否休眠（调试可视化用）
    pub fn chunk_asleep(&self, cx: i32, cy: i32) -> bool {
        self.chunks[(cy.clamp(0, self.ch - 1) * self.cw + cx.clamp(0, self.cw - 1)) as usize].asleep
    }

    #[inline]
    fn chunk_index(&self, x: i32, y: i32) -> usize {
        let cx = (x / CHUNK_PX as i32).clamp(0, self.cw - 1);
        let cy = (y / CHUNK_PX as i32).clamp(0, self.ch - 1);
        (cy * self.cw + cx) as usize
    }

    #[inline]
    fn local_index(x: i32, y: i32) -> usize {
        (y.rem_euclid(CHUNK_PX as i32) as usize) * CHUNK_PX
            + (x.rem_euclid(CHUNK_PX as i32) as usize)
    }

    fn wake_around(&mut self, x: i32, y: i32) {
        let cx = x / CHUNK_PX as i32;
        let cy = y / CHUNK_PX as i32;
        for (dx, dy) in [(0i32, 0i32), (-1, 0), (1, 0), (0, -1), (0, 1)] {
            let (nx, ny) = (cx + dx, cy + dy);
            if nx < 0 || ny < 0 || nx >= self.cw || ny >= self.ch {
                continue;
            }
            self.chunks[(ny * self.cw + nx) as usize].asleep = false;
        }
    }

    fn mark_dirty(&mut self, r: Rect) {
        self.dirty = Some(match self.dirty {
            Some(d) => {
                let x = d.x.min(r.x);
                let y = d.y.min(r.y);
                let x1 = (d.x + d.w).max(r.x + r.w);
                let y1 = (d.y + d.h).max(r.y + r.h);
                Rect { x, y, w: x1 - x, h: y1 - y }
            }
            None => r,
        });
    }

    /// 取走本帧脏矩形（用于纹理上传）
    pub fn take_dirty(&mut self) -> Option<Rect> {
        self.dirty.take()
    }

    /// 导出整幅像素（mat/shade 交错，行主序）—— 存档用
    pub fn export_mat_shade(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity((self.w * self.h * 2) as usize);
        for y in 0..self.h {
            for x in 0..self.w {
                let p = self.get(x, y);
                out.push(p.mat);
                out.push(p.shade);
            }
        }
        out
    }

    /// 从 mat/shade 交错流导入整幅像素（life/aux 清零，全块唤醒+全脏）—— 读档用
    pub fn import_mat_shade(&mut self, data: &[u8]) {
        assert_eq!(data.len(), (self.w * self.h * 2) as usize, "像素流尺寸不匹配");
        for c in self.chunks.iter_mut() {
            c.asleep = false;
            c.active = u32::MAX / 2; // 读档后全模拟（首个 tick 会重新收敛）
        }
        for y in 0..self.h {
            for x in 0..self.w {
                let i = ((y * self.w + x) * 2) as usize;
                let ci = self.chunk_index(x, y);
                self.chunks[ci].px[Self::local_index(x, y)] =
                    Pixel { mat: data[i], shade: data[i + 1], life: 0, aux: 0 };
            }
        }
        self.dirty = Some(Rect { x: 0, y: 0, w: self.w, h: self.h });
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> Pixel {
        if x < 0 || x >= self.w {
            return Pixel { mat: OOB, ..Default::default() };
        }
        if y >= self.h {
            return Pixel { mat: OOB, ..Default::default() };
        }
        if y < 0 {
            return Pixel::default(); // 世界顶之上 = 天空
        }
        let ci = self.chunk_index(x, y);
        self.chunks[ci].px[Self::local_index(x, y)]
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, p: Pixel) {
        if x < 0 || x >= self.w || y < 0 || y >= self.h {
            return;
        }
        let ci = self.chunk_index(x, y);
        self.chunks[ci].px[Self::local_index(x, y)] = p;
        self.wake_around(x, y);
        // 邻居变化 → 解除四邻的结块
        for (dx, dy) in NB4 {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || nx >= self.w || ny < 0 || ny >= self.h {
                continue;
            }
            let ci2 = self.chunk_index(nx, ny);
            let li2 = Self::local_index(nx, ny);
            if self.chunks[ci2].px[li2].is_settled() {
                self.chunks[ci2].px[li2].aux &= !1;
            }
        }
        self.mark_dirty(Rect { x, y, w: 1, h: 1 });
    }

    fn patch(&mut self, x: i32, y: i32, f: impl FnOnce(&mut Pixel)) {
        if x < 0 || x >= self.w || y < 0 || y >= self.h {
            return;
        }
        let ci = self.chunk_index(x, y);
        f(&mut self.chunks[ci].px[Self::local_index(x, y)]);
        self.mark_dirty(Rect { x, y, w: 1, h: 1 });
    }

    /// 带颜色抖动地生成一个材质像素（仅写入空格）
    pub fn spawn(&mut self, x: i32, y: i32, mat: MaterialId, mats: &Materials) {
        if mat == EMPTY || mat == OOB || self.get(x, y).mat != EMPTY {
            return;
        }
        let d = mats.def(mat);
        let j = d.jitter as i32;
        let shade = (127 + self.rng.range_i32(-j, j)).clamp(0, 255) as u8;
        self.set(x, y, Pixel { mat, shade, life: d.life, aux: 0 });
    }

    pub fn clear_px(&mut self, x: i32, y: i32) {
        let m = self.get(x, y).mat;
        if m != EMPTY && m != OOB {
            self.set(x, y, Pixel::default());
        }
    }

    fn swap(&mut self, ax: i32, ay: i32, bx: i32, by: i32) {
        let a = self.get(ax, ay);
        let b = self.get(bx, by);
        self.set(ax, ay, b);
        self.set(bx, by, a.unsettle());
    }

    fn move_to(&mut self, fx: i32, fy: i32, tx: i32, ty: i32, p: Pixel) {
        self.set(fx, fy, Pixel::default());
        self.set(tx, ty, p.unsettle());
    }

    /// 目标格是否可进入（含密度置换规则；静态地形像素天然阻挡）
    fn can_enter(&self, mats: &Materials, x: i32, y: i32, kind: Kind, density: u16, rising: bool) -> bool {
        let t = self.get(x, y);
        if t.mat == EMPTY {
            return true;
        }
        if t.mat == OOB {
            return false;
        }
        let td = mats.def(t.mat);
        match kind {
            Kind::Powder => td.kind == Kind::Liquid && density > td.density,
            Kind::Liquid => td.kind == Kind::Liquid && density > td.density && !rising,
            Kind::Gas => td.kind == Kind::Liquid && rising,
            Kind::Static => false,
        }
    }

    /// 模拟一帧
    pub fn step(&mut self, mats: &Materials, hooks: &mut dyn SimHooks) {
        let tick = self.tick;
        self.tick += 1;
        self.active_pixels = 0;
        self.asleep_chunks = 0;
        let n = self.chunks.len();
        for ci in 0..n {
            if self.chunks[ci].asleep {
                self.asleep_chunks += 1;
                continue;
            }
            let cx = (ci as i32) % self.cw;
            let cy = (ci as i32) / self.cw;
            let x0 = cx * CHUNK_PX as i32;
            let y0 = cy * CHUNK_PX as i32;
            let mut active = 0u32;
            let mut special = 0u32;
            for ly in (0..CHUNK_PX).rev() {
                let y = y0 + ly as i32;
                // 逐行交替扫描方向，消除左右偏差
                let ltr = (y as usize).wrapping_add(tick as usize) & 1 == 0;
                for li in 0..CHUNK_PX {
                    let lx = if ltr { li } else { CHUNK_PX - 1 - li };
                    let x = x0 + lx as i32;
                    let p = self.get(x, y);
                    if p.mat == EMPTY || p.mat == OOB {
                        continue;
                    }
                    let def = mats.def(p.mat);
                    // 静态地形不参与模拟（可被挖掘/燃烧/腐蚀，由外部 API 修改）
                    if def.kind == Kind::Static {
                        continue;
                    }
                    let has_special = def.special.is_some() || def.emissive > 0;
                    if p.is_settled() {
                        if has_special {
                            // 休眠的特殊材质低频反应
                            if tick.wrapping_add((x ^ y) as u64) & 7 == 0 {
                                self.react(x, y, def, mats);
                            }
                            special += 1;
                        }
                        continue;
                    }
                    let moved = match def.kind {
                        Kind::Powder => self.step_powder(x, y, def, mats),
                        Kind::Liquid => self.step_liquid(x, y, p, def, mats),
                        Kind::Gas => self.step_gas(x, y, p, def, mats),
                        Kind::Static => continue, // 已在上方排除
                    };
                    if moved {
                        active += 1;
                    } else if !has_special {
                        self.settle(x, y);
                    }
                    if has_special {
                        self.react(x, y, def, mats);
                        special += 1;
                        if def.emissive > 0 {
                            hooks.add_emissive(x, y, def.emissive);
                        }
                    }
                }
            }
            let c = &mut self.chunks[ci];
            c.active = active;
            if active == 0 && special == 0 {
                c.asleep = true;
            }
            self.active_pixels += (active + special) as u64;
        }
    }

    fn settle(&mut self, x: i32, y: i32) {
        self.patch(x, y, |p| p.aux |= 1);
    }

    fn step_powder(&mut self, x: i32, y: i32, def: &MaterialDef, mats: &Materials) -> bool {
        if self.can_enter(mats, x, y + 1, Kind::Powder, def.density, false) {
            self.swap(x, y, x, y + 1);
            return true;
        }
        let d = if self.rng.chance(0.5) { 1 } else { -1 };
        if self.can_enter(mats, x + d, y + 1, Kind::Powder, def.density, false) {
            self.swap(x, y, x + d, y + 1);
            return true;
        }
        if self.can_enter(mats, x - d, y + 1, Kind::Powder, def.density, false) {
            self.swap(x, y, x - d, y + 1);
            return true;
        }
        false
    }

    fn step_liquid(&mut self, x: i32, y: i32, p: Pixel, def: &MaterialDef, mats: &Materials) -> bool {
        if self.can_enter(mats, x, y + 1, Kind::Liquid, def.density, false) {
            self.swap(x, y, x, y + 1);
            return true;
        }
        let d = if self.rng.chance(0.5) { 1 } else { -1 };
        if self.can_enter(mats, x + d, y + 1, Kind::Liquid, def.density, false) {
            self.swap(x, y, x + d, y + 1);
            return true;
        }
        if self.can_enter(mats, x - d, y + 1, Kind::Liquid, def.density, false) {
            self.swap(x, y, x - d, y + 1);
            return true;
        }
        // 水平扩散：沿记忆方向滑行最多 dispersal 步，途中遇可下落处提前停
        // 液面偶发沉降：防止表层液体永久来回滑动导致 chunk 无法休眠
        if self.rng.chance(0.02) {
            return false; // 触发结算沉降（未动 → settle）
        }
        let mut dir = p.dir();
        if !self.can_enter(mats, x + dir, y, Kind::Liquid, def.density, false) {
            dir = -dir;
            if !self.can_enter(mats, x + dir, y, Kind::Liquid, def.density, false) {
                return false;
            }
        }
        let mut nx = x;
        for _ in 0..def.dispersal.max(1) {
            let tx = nx + dir;
            if !self.can_enter(mats, tx, y, Kind::Liquid, def.density, false) {
                break;
            }
            nx = tx;
            if self.can_enter(mats, nx, y + 1, Kind::Liquid, def.density, false) {
                break;
            }
        }
        if nx != x {
            self.move_to(x, y, nx, y, p.with_dir(dir));
            return true;
        }
        false
    }

    fn step_gas(&mut self, x: i32, y: i32, p: Pixel, def: &MaterialDef, mats: &Materials) -> bool {
        // 寿命衰减与死亡转化
        if def.life > 0 {
            if p.life == 0 {
                let next = if def.special == Some(Special::Fire) {
                    if self.rng.chance(0.3) {
                        Pixel { mat: self.ids.smoke, shade: p.shade, life: mats.def(self.ids.smoke).life, aux: 0 }
                    } else {
                        Pixel::default()
                    }
                } else if p.mat == self.ids.steam && self.rng.chance(0.06) {
                    Pixel { mat: self.ids.water, shade: p.shade, life: 0, aux: 0 } // 蒸汽凝结
                } else {
                    Pixel::default()
                };
                self.set(x, y, next);
                return true;
            }
            self.patch(x, y, |q| q.life -= 1);
        }
        // 上升 + 随机横漂
        if self.can_enter(mats, x, y - 1, Kind::Gas, 0, true) {
            self.move_to(x, y, x, y - 1, p);
            return true;
        }
        let d = if self.rng.chance(0.5) { 1 } else { -1 };
        if self.can_enter(mats, x + d, y - 1, Kind::Gas, 0, true) {
            self.move_to(x, y, x + d, y - 1, p);
            return true;
        }
        if self.can_enter(mats, x - d, y - 1, Kind::Gas, 0, true) {
            self.move_to(x, y, x - d, y - 1, p);
            return true;
        }
        if self.can_enter(mats, x + d, y, Kind::Gas, 0, true) {
            self.move_to(x, y, x + d, y, p);
            return true;
        }
        false
    }

    /// 反应规则：火 / 岩浆 / 酸
    fn react(&mut self, x: i32, y: i32, def: &MaterialDef, mats: &Materials) {
        match def.special {
            Some(Special::Fire) => {
                for (dx, dy) in NB4 {
                    let (nx, ny) = (x + dx, y + dy);
                    let n = self.get(nx, ny);
                    if n.mat == EMPTY || n.mat == OOB {
                        continue;
                    }
                    // 灭于水 → 蒸汽
                    if n.mat == self.ids.water {
                        self.set(x, y, Pixel { mat: self.ids.steam, shade: n.shade, life: mats.def(self.ids.steam).life, aux: 0 });
                        if self.rng.chance(0.2) {
                            self.set(nx, ny, Pixel::default());
                        }
                        return;
                    }
                    let nd = mats.def(n.mat);
                    // 点燃可燃像素（含木质地形）
                    if nd.flammable > 0 && self.rng.chance(nd.flammable as f32 / 255.0 * 0.35) {
                        self.set(nx, ny, Pixel { mat: self.ids.fire, shade: n.shade, life: nd.burn_life.max(20), aux: 0 });
                    }
                }
            }
            Some(Special::Lava) => {
                for (dx, dy) in NB4 {
                    let (nx, ny) = (x + dx, y + dy);
                    let n = self.get(nx, ny);
                    if n.mat == EMPTY || n.mat == OOB {
                        continue;
                    }
                    // 遇水凝结成石
                    if n.mat == self.ids.water {
                        self.set(x, y, Pixel { mat: self.ids.stone_debris, shade: n.shade, life: 0, aux: 0 });
                        self.set(nx, ny, Pixel { mat: self.ids.steam, shade: n.shade, life: mats.def(self.ids.steam).life, aux: 0 });
                        return;
                    }
                    let nd = mats.def(n.mat);
                    if nd.flammable > 0 && self.rng.chance(nd.flammable as f32 / 255.0 * 0.2) {
                        self.set(nx, ny, Pixel { mat: self.ids.fire, shade: n.shade, life: nd.burn_life.max(20), aux: 0 });
                    }
                }
            }
            Some(Special::Acid) => {
                for (dx, dy) in NB4 {
                    let (nx, ny) = (x + dx, y + dy);
                    let n = self.get(nx, ny);
                    if n.mat == EMPTY || n.mat == OOB {
                        continue;
                    }
                    let nd = mats.def(n.mat);
                    // 腐蚀可溶静态像素（酸被消耗）
                    if nd.kind == Kind::Static && !nd.acid_proof && nd.hp > 0 && self.rng.chance(0.25) {
                        self.set(nx, ny, Pixel::default());
                        self.set(x, y, Pixel { mat: self.ids.smoke, shade: 128, life: mats.def(self.ids.smoke).life, aux: 0 });
                        return;
                    }
                }
            }
            None => {}
        }
    }

    /// 挖掘一个静态像素：累积伤害，达到硬度时破坏。
    /// 返回 Some(被破坏的材质 id)。
    pub fn mine_px(&mut self, x: i32, y: i32, power: u16, mats: &Materials) -> Option<u8> {
        let p = self.get(x, y);
        if p.mat == EMPTY || p.mat == OOB {
            return None;
        }
        let def = mats.def(p.mat);
        if def.kind != Kind::Static || def.hp == 0 {
            return None;
        }
        let dmg = (p.aux >> 2) as u16 + power;
        if dmg >= def.hp {
            self.set(x, y, Pixel::default());
            Some(p.mat)
        } else {
            self.patch(x, y, |q| q.aux = ((dmg.min(63) as u8) << 2) | (q.aux & 0b11));
            None
        }
    }
}
