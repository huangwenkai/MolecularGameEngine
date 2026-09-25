//! 背包与装备：30 格背包 + 5 装备槽 + 属性聚合 + 经验/等级/属性点 + 药水使用 + 合成
use crate::items::{Item, ItemDb, Slot, Stats};
use crate::GameApp;
use egui::{Color32, DragAndDrop};
use mge_core::rng::Rng;

pub const BAG_SIZE: usize = 30;
pub const EQUIP_SLOTS: usize = 5; // Weapon Head Chest Legs Trinket

/// 属性点分配
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct Attr {
    pub str_: u8,
    pub agi: u8,
    pub vit: u8,
}

#[derive(Debug, Clone, Default)]
pub struct Inventory {
    pub bag: Vec<Option<Item>>, // 定长 BAG_SIZE
    pub equip: [Option<Item>; EQUIP_SLOTS],
    pub gold: u32,
    pub level: u32,
    pub xp: u32,
    /// 未分配属性点
    pub points: u8,
    pub attr: Attr,
    /// 背包 UI 是否打开
    pub ui_open: bool,
}

impl Inventory {
    pub fn new() -> Self {
        // 等级从 1 起（0 会让 max_hp 公式的 level-1 下溢）
        Self { bag: vec![None; BAG_SIZE], level: 1, ..Default::default() }
    }

    /// 升级所需经验
    pub fn xp_need(&self) -> u32 {
        let l = self.level as f32;
        (40.0 + 30.0 * l * 1.35) as u32
    }

    /// 获得经验，返回升级数（特效由调用方触发）
    pub fn gain_xp(&mut self, amount: u32) -> u32 {
        self.xp += amount;
        let mut ups = 0;
        while self.xp >= self.xp_need() {
            self.xp -= self.xp_need();
            self.level += 1;
            self.points += 3;
            ups += 1;
        }
        ups
    }

    /// 放入物品：先堆叠后空格；返回是否成功
    pub fn add(&mut self, item: Item, db: &ItemDb) -> bool {
        let stack = db.def(&item.def).stack;
        if stack > 1 {
            for s in self.bag.iter_mut().flatten() {
                if s.def == item.def {
                    let room = stack - s.count;
                    let move_n = room.min(item.count);
                    s.count += move_n;
                    if move_n == item.count {
                        return true;
                    }
                    // 部分入包，继续找
                    let rest = item.count - move_n;
                    return self.add(
                        Item { def: item.def.clone(), count: rest, affixes: vec![] },
                        db,
                    );
                }
            }
        }
        if let Some(slot) = self.bag.iter_mut().find(|s| s.is_none()) {
            *slot = Some(item);
            return true;
        }
        false // 背包满
    }

    /// 统计某材料数量
    pub fn count_of(&self, id: &str) -> u16 {
        self.bag
            .iter()
            .flatten()
            .filter(|i| i.def == id)
            .map(|i| i.count)
            .sum()
    }

    /// 扣除材料（合成用）；数量不足返回 false
    pub fn take(&mut self, id: &str, amount: u16) -> bool {
        if self.count_of(id) < amount {
            return false;
        }
        let mut left = amount;
        for s in self.bag.iter_mut() {
            if let Some(it) = s {
                if it.def == id {
                    let take = it.count.min(left);
                    it.count -= take;
                    left -= take;
                    if it.count == 0 {
                        *s = None;
                    }
                    if left == 0 {
                        return true;
                    }
                }
            }
        }
        left == 0
    }

    /// 装备某背包格（放回原装备到背包）
    pub fn equip_from_bag(&mut self, idx: usize, db: &ItemDb) -> bool {
        let Some(Some(item)) = self.bag.get(idx) else { return false };
        let slot = db.def(&item.def).slot;
        let ei = slot.equip_index();
        let old = self.equip[ei].take();
        self.equip[ei] = self.bag[idx].take();
        if let Some(o) = old {
            // 原装备放回背包（可能满）
            if !self.add(o.clone(), db) {
                // 放回失败则退回装备位
                let cur = self.equip[ei].take();
                self.bag[idx] = cur;
                self.equip[ei] = Some(o);
                return false;
            }
        }
        true
    }

    /// 卸下装备位到背包
    pub fn unequip(&mut self, ei: usize, db: &ItemDb) -> bool {
        let Some(item) = self.equip[ei].take() else { return false };
        if self.add(item.clone(), db) {
            true
        } else {
            self.equip[ei] = Some(item);
            false
        }
    }

    /// 使用药水（第一瓶治疗药水，回复量 = def.hp）
    pub fn use_potion(&mut self, hp: &mut f32, max_hp: f32, db: &ItemDb) -> bool {
        if *hp >= max_hp {
            return false;
        }
        for i in 0..self.bag.len() {
            let Some(slot) = self.bag.get_mut(i) else { continue };
            let Some(it) = slot else { continue };
            if it.def == "potion_hp" {
                let heal = db.def("potion_hp").hp;
                *hp = (*hp + heal).min(max_hp);
                if it.count > 1 {
                    it.count -= 1;
                } else {
                    self.bag[i] = None;
                }
                return true;
            }
        }
        false
    }

    /// 合成
    pub fn craft(&mut self, ri: usize, db: &ItemDb) -> bool {
        let Some(r) = db.recipes.get(ri) else { return false };
        if r.cost.iter().any(|(id, n)| self.count_of(id) < *n) {
            return false;
        }
        for (id, n) in &r.cost {
            self.take(id, *n);
        }
        let def = db.def(&r.out);
        let item = if def.stack > 1 {
            Item { def: r.out.clone(), count: r.count, affixes: vec![] }
        } else {
            db.roll(&mut Rng::from_entropy(), &r.out, 0)
        };
        self.add(item, db)
    }

    /// 属性聚合：装备基础 + 词缀 + 属性点
    pub fn aggregate(&self, db: &ItemDb) -> Stats {
        let mut st = Stats::new();
        for e in self.equip.iter().flatten() {
            db.item_stats(e, &mut st);
        }
        // 属性点：力量 +2%伤害/点，敏捷 +1%攻速 +0.5%暴击/点，体力 +6 生命/点
        st.dmg_pct += self.attr.str_ as f32 * 2.0;
        st.atk_pct += self.attr.agi as f32 * 1.0;
        st.crit += self.attr.agi as f32 * 0.5;
        st.hp += self.attr.vit as f32 * 6.0;
        st
    }

    /// 玩家最大生命 = 100 + 聚合生命 + 等级成长
    pub fn max_hp(&self, db: &ItemDb) -> f32 {
        100.0 + self.aggregate(db).hp + (self.level - 1) as f32 * 8.0
    }
}

// ============ egui 背包界面 ============

/// 拖拽 payload：>=100 为装备位（100+ei），<100 为背包格
const DRAG_EQUIP: usize = 100;

fn rarity_color32(db: &ItemDb, item: &Item) -> Color32 {
    let c = db.rarity(item).color();
    Color32::from_rgb(
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
    )
}

pub fn draw(app: &mut GameApp, ctx: &egui::Context) {
    if !app.inv.ui_open {
        return;
    }
    egui::Window::new("背包 [I]").show(ctx, |ui| {
        let st = app.inv.aggregate(&app.db);
        let max_hp = app.inv.max_hp(&app.db);

        // ---- 属性面板 ----
        ui.horizontal(|ui| {
            ui.heading(format!("等级 {}", app.inv.level));
            ui.separator();
            ui.monospace(format!("生命 {:.0}/{:.0}", app.player.hp, max_hp));
            ui.separator();
            ui.monospace(format!("金币 {}", app.inv.gold));
        });
        ui.add(
            egui::ProgressBar::new(app.inv.xp as f32 / app.inv.xp_need() as f32).show_percentage(),
        );
        if app.inv.points > 0 {
            ui.colored_label(Color32::GOLD, format!("可用属性点 {}（力量=伤害 敏捷=攻速/暴击 体力=生命）", app.inv.points));
        }
        ui.horizontal(|ui| {
            let mut add = |ui: &mut egui::Ui, name: &str, f: &mut u8| {
                ui.add_enabled_ui(app.inv.points > 0, |ui| {
                    if ui.button(format!("+ {name}")).clicked() {
                        *f += 1;
                        app.inv.points -= 1;
                    }
                });
                ui.monospace(format!("{name} {}", *f));
            };
            add(ui, "力量", &mut app.inv.attr.str_);
            add(ui, "敏捷", &mut app.inv.attr.agi);
            add(ui, "体力", &mut app.inv.attr.vit);
        });
        ui.separator();
        ui.monospace(format!(
            "伤害 {}+{:.0}%  护甲 {:.0}({:.0}%减伤)  攻速 ×{:.2}  移速 +{:.0}%  暴击 {:.1}%  暴伤 +{:.0}%",
            st.dmg_flat, st.dmg_pct, st.armor, st.mitigation() * 100.0, st.atk_speed(),
            st.move_pct, st.crit, st.crit_dmg
        ));
        ui.separator();

        // ---- 装备槽 ----
        ui.label("装备（点击卸下）");
        ui.horizontal(|ui| {
            for (ei, slot) in [
                Slot::Weapon,
                Slot::Head,
                Slot::Chest,
                Slot::Legs,
                Slot::Trinket,
            ]
            .into_iter()
            .enumerate()
            {
                let inner = egui::Frame::NONE
                    .stroke(egui::Stroke::new(1.0_f32, Color32::from_gray(120)))
                    .inner_margin(4.0);
                let resp = inner
                    .show(ui, |ui| {
                        match &app.inv.equip[ei] {
                            Some(it) => {
                                let d = app.db.def(&it.def);
                                ui.set_min_size(egui::vec2(64.0, 52.0));
                                ui.colored_label(rarity_color32(&app.db, it), &d.name);
                                ui.small(format!("{} {}", slot_label(d.slot), slot_stat_text(&app.db, it)));
                            }
                            None => {
                                ui.set_min_size(egui::vec2(64.0, 52.0));
                                ui.weak(slot.name());
                            }
                        }
                    })
                    .response;
                // 卸下 / 拖放目标
                if resp.clicked() && app.inv.equip[ei].is_some() {
                    app.inv.unequip(ei, &app.db);
                }
                if let Some(payload) = DragAndDrop::payload::<usize>(ui.ctx()) {
                    if resp.hovered() && ui.input(|i| i.pointer.any_released()) {
                        let src = *payload;
                        if src < DRAG_EQUIP {
                            // 背包 → 装备
                            if let Some(Some(it)) = app.inv.bag.get(src) {
                                let want = app.db.def(&it.def).slot.equip_index();
                                if want == ei {
                                    app.inv.equip_from_bag(src, &app.db);
                                }
                            }
                        }
                    }
                }
                item_tooltip(resp.clone(), app, app.inv.equip[ei].as_ref(), None);
            }
        });
        ui.separator();

        // ---- 背包格子 ----
        ui.label("背包（点击装备/使用，可拖拽）");
        let mut used_click: Option<usize> = None;
        egui::Grid::new("bag_grid").spacing([4.0, 4.0]).show(ui, |ui| {
            for idx in 0..app.inv.bag.len() {
                let cell = egui::Frame::NONE
                    .fill(Color32::from_gray(28))
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        app.inv.bag[idx]
                            .as_ref()
                            .map(|it| rarity_color32(&app.db, it))
                            .unwrap_or(Color32::from_gray(60)),
                    ))
                    .inner_margin(3.0);
                let resp = cell.show(ui, |ui| {
                    ui.set_min_size(egui::vec2(58.0, 44.0));
                    if let Some(it) = &app.inv.bag[idx] {
                        let d = app.db.def(&it.def);
                        ui.vertical(|ui| {
                            ui.colored_label(rarity_color32(&app.db, it), &d.name);
                            if it.count > 1 {
                                ui.small(format!("×{}", it.count));
                            }
                        });
                    }
                })
                .response;
                // 拖拽源
                if resp.drag_started() && app.inv.bag[idx].is_some() {
                    DragAndDrop::set_payload(ui.ctx(), idx);
                }
                // 放置目标
                if let Some(payload) = DragAndDrop::payload::<usize>(ui.ctx()) {
                    if resp.hovered() && ui.input(|i| i.pointer.any_released()) {
                        let src = *payload;
                        let src2 = if src >= DRAG_EQUIP {
                            let ei = src - DRAG_EQUIP;
                            // 装备位 → 背包格：卸下到该格（简单 swap）
                            if app.inv.bag[idx].is_none() {
                                if let Some(it) = app.inv.equip[ei].take() {
                                    app.inv.bag[idx] = Some(it);
                                }
                            }
                            None
                        } else {
                            Some(src)
                        };
                        if let Some(src) = src2 {
                            app.inv.bag.swap(src, idx);
                        }
                    }
                }
                // 点击：装备/使用
                if resp.clicked() && app.inv.bag[idx].is_some() {
                    used_click = Some(idx);
                }
                item_tooltip(resp.clone(), app, app.inv.bag[idx].as_ref(), Some(idx));
            }
            ui.end_row();
        });

        // 点击处理（放在 grid 外避免借用冲突）
        if let Some(idx) = used_click {
            let def_id = app.inv.bag[idx].as_ref().map(|it| it.def.clone());
            if let Some(def_id) = def_id {
                let d = app.db.def(&def_id);
                if d.stack > 1 && d.hp > 0.0 {
                    let hp = &mut app.player.hp;
                    let max_hp = app.player.max_hp;
                    app.inv.use_potion(hp, max_hp, &app.db);
                } else if d.stack == 1 {
                    app.inv.equip_from_bag(idx, &app.db);
                }
            }
        }
        ui.separator();

        // ---- 合成 ----
        ui.label("合成");
        for (ri, r) in app.db.recipes.iter().enumerate() {
            let d = app.db.def(&r.out);
            let can = r
                .cost
                .iter()
                .all(|(id, n)| app.inv.count_of(id) >= *n);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(can, |ui| {
                    if ui.button(format!("合成 {}", d.name)).clicked() {
                        app.inv.craft(ri, &app.db);
                    }
                });
                let cost: Vec<String> = r
                    .cost
                    .iter()
                    .map(|(id, n)| {
                        format!(
                            "{}×{}({})",
                            app.db.def(id).name,
                            n,
                            app.inv.count_of(id)
                        )
                    })
                    .collect();
                ui.weak(cost.join(" + "));
            });
        }
    });
}

fn slot_label(s: crate::items::Slot) -> &'static str {
    s.name()
}

fn slot_stat_text(db: &ItemDb, it: &Item) -> String {
    let d = db.def(&it.def);
    let mut parts = Vec::new();
    if d.dmg > 0.0 {
        parts.push(format!("伤害{:.0}", d.dmg));
    }
    if d.armor > 0.0 {
        parts.push(format!("护甲{:.0}", d.armor));
    }
    if d.hp > 0.0 {
        parts.push(format!("生命+{:.0}", d.hp));
    }
    if d.speed != 1.0 {
        parts.push(format!("攻速×{:.2}", d.speed));
    }
    for a in &it.affixes {
        parts.push(db.affix_text(a));
    }
    parts.join(" ")
}

/// 悬停物品 Tooltip（装备类显示与当前装备的对比）
fn item_tooltip(resp: egui::Response, app: &GameApp, item: Option<&Item>, bag_idx: Option<usize>) {
    let Some(it) = item else { return };
    let d = app.db.def(&it.def);
    let r = app.db.rarity(it);
    resp.on_hover_ui(|ui| {
        ui.colored_label(rarity_color32(&app.db, it), format!("{}（{}）", d.name, r.name()));
        ui.weak(format!("{} · 等级{} · 价值{}", slot_label(d.slot), d.lvl, d.value));
        ui.separator();
        if d.dmg > 0.0 {
            // 装备对比：显示与当前佩戴的差值
            let ei = d.slot.equip_index();
            let cur = app.inv.equip[ei]
                .as_ref()
                .map(|c| app.db.def(&c.def).dmg)
                .unwrap_or(0.0);
            let delta = d.dmg - cur;
            let (txt, col) = if app.inv.equip[ei].is_some() {
                (
                    format!("伤害 {:.0} ({:+.0})", d.dmg, delta),
                    if delta >= 0.0 { Color32::LIGHT_GREEN } else { Color32::LIGHT_RED },
                )
            } else {
                (format!("伤害 {:.0}", d.dmg), Color32::WHITE)
            };
            ui.colored_label(col, txt);
        }
        for a in &it.affixes {
            ui.colored_label(Color32::from_rgb(90, 170, 255), app.db.affix_text(a));
        }
        if d.armor > 0.0 {
            ui.label(format!("护甲 {:.0}", d.armor));
        }
        if d.hp > 0.0 && d.stack > 1 {
            ui.label(format!("使用：恢复 {:.0} 生命 [Q]", d.hp));
        }
        if let Some(idx) = bag_idx {
            ui.weak(format!("[右键丢弃 格{}] 拖拽整理", idx));
        }
    });
}
