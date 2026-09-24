//! 物品与暗黑层：稀有度 / 随机词缀 / 掉落表 / 物品生成（数据驱动 RON）
use mge_core::rng::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const ITEMS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/data/items.ron");

// ---- 稀有度（白/蓝/金/绿/暗金）----
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rarity {
    Common,
    Magic,
    Rare,
    Masterwork,
    Legendary,
}

impl Rarity {
    /// UI/光柱颜色
    pub fn color(self) -> [f32; 4] {
        match self {
            Rarity::Common => [0.85, 0.85, 0.85, 1.0],
            Rarity::Magic => [0.35, 0.55, 1.0, 1.0],
            Rarity::Rare => [1.0, 0.82, 0.25, 1.0],
            Rarity::Masterwork => [0.3, 1.0, 0.45, 1.0],
            Rarity::Legendary => [0.75, 0.35, 1.0, 1.0],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Rarity::Common => "普通",
            Rarity::Magic => "魔法",
            Rarity::Rare => "稀有",
            Rarity::Masterwork => "杰作",
            Rarity::Legendary => "传奇",
        }
    }
}

// ---- 装备位 / 属性 ----
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Slot {
    Weapon,
    Head,
    Chest,
    Legs,
    Trinket,
}

impl Slot {
    pub fn name(self) -> &'static str {
        match self {
            Slot::Weapon => "武器",
            Slot::Head => "头盔",
            Slot::Chest => "胸甲",
            Slot::Legs => "护腿",
            Slot::Trinket => "饰品",
        }
    }

    pub fn equip_index(self) -> usize {
        match self {
            Slot::Weapon => 0,
            Slot::Head => 1,
            Slot::Chest => 2,
            Slot::Legs => 3,
            Slot::Trinket => 4,
        }
    }
}

/// 词缀/聚合属性种类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stat {
    DmgFlat,
    DmgPct,
    Armor,
    MaxHp,
    MovePct,
    AtkPct,
    Crit,
    CritDmg,
}

impl Stat {
    pub fn name(self) -> &'static str {
        match self {
            Stat::DmgFlat => "伤害",
            Stat::DmgPct => "伤害%",
            Stat::Armor => "护甲",
            Stat::MaxHp => "生命",
            Stat::MovePct => "移速%",
            Stat::AtkPct => "攻速%",
            Stat::Crit => "暴击率%",
            Stat::CritDmg => "暴击伤害%",
        }
    }
}

// ---- 数据定义（RON）----
#[derive(Debug, Clone, Deserialize)]
pub struct ItemDef {
    pub id: String,
    pub name: String,
    pub slot: Slot,
    /// 堆叠上限（>1 即材料/消耗品）
    #[serde(default = "def_stack")]
    pub stack: u16,
    #[serde(default)]
    pub dmg: f32,
    #[serde(default)]
    pub armor: f32,
    /// 攻速/使用速度倍率
    #[serde(default = "def_one")]
    pub speed: f32,
    #[serde(default)]
    pub hp: f32,
    /// 物品等级（词缀数值缩放）
    #[serde(default = "def_one8")]
    pub lvl: u8,
    #[serde(default)]
    pub value: u32,
}

fn def_stack() -> u16 {
    1
}
fn def_one() -> f32 {
    1.0
}
fn def_one8() -> u8 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct AffixDef {
    pub id: String,
    pub name: String,
    pub stat: Stat,
    pub min: f32,
    pub max: f32,
    /// 数值随物品等级线性增长
    #[serde(default)]
    pub per_lvl: f32,
    pub prefix: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LootEntry {
    /// 物品 id；"GEAR_RANDOM" 表示随机装备（roll 词缀）
    pub item: String,
    #[serde(default = "def_weight")]
    pub weight: u32,
    #[serde(default)]
    pub min: u16,
    #[serde(default = "def_one16")]
    pub max: u16,
    /// 触发概率 0~1（默认必掉）
    #[serde(default = "def_onef")]
    pub chance: f32,
}
fn def_weight() -> u32 {
    10
}
fn def_one16() -> u16 {
    1
}
fn def_onef() -> f32 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemsRoot {
    pub items: Vec<ItemDef>,
    pub affixes: Vec<AffixDef>,
    /// 掉落表：表名 → 条目
    pub tables: HashMap<String, Vec<LootEntry>>,
    // ---- 合成配方 ----
    #[serde(default)]
    pub recipes: Vec<Recipe>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Recipe {
    pub out: String,
    #[serde(default = "def_one16")]
    pub count: u16,
    /// 消耗（材料 id, 数量）
    pub cost: Vec<(String, u16)>,
}

// ---- 物品实例 ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Affix {
    pub id: String,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub def: String,
    #[serde(default = "def_one16")]
    pub count: u16,
    #[serde(default)]
    pub affixes: Vec<Affix>,
}

pub struct ItemDb {
    pub defs: HashMap<String, ItemDef>,
    pub affixes: Vec<AffixDef>,
    pub tables: HashMap<String, Vec<LootEntry>>,
    pub recipes: Vec<Recipe>,
}

impl ItemDb {
    pub fn embedded() -> Self {
        let root: ItemsRoot = ron::from_str(include_str!("../assets/data/items.ron"))
            .expect("items.ron 解析失败");
        Self::from_root(root)
    }

    fn from_root(root: ItemsRoot) -> Self {
        let defs = root.items.into_iter().map(|d| (d.id.clone(), d)).collect();
        Self { defs, affixes: root.affixes, tables: root.tables, recipes: root.recipes }
    }

    /// 运行时热重载（编辑器/调平衡用）
    pub fn reload_from_disk(&mut self) -> Result<(), String> {
        let s = std::fs::read_to_string(ITEMS_PATH).map_err(|e| e.to_string())?;
        let root: ItemsRoot = ron::from_str(&s).map_err(|e| e.to_string())?;
        *self = Self::from_root(root);
        Ok(())
    }

    pub fn def(&self, id: &str) -> &ItemDef {
        self.defs.get(id).expect("未知物品 id")
    }

    /// 物品稀有度：无词缀 = Common，否则按词缀数分档
    pub fn rarity(&self, item: &Item) -> Rarity {
        match item.affixes.len() {
            0 => Rarity::Common,
            1 => Rarity::Magic,
            2 => Rarity::Rare,
            3 => Rarity::Masterwork,
            _ => Rarity::Legendary,
        }
    }

    /// 词缀显示名 + 数值文本
    pub fn affix_text(&self, a: &Affix) -> String {
        let Some(d) = self.affixes.iter().find(|x| x.id == a.id) else {
            return format!("?{}", a.id);
        };
        let v = a.value;
        let num = |stat: Stat| {
            if matches!(stat, Stat::DmgFlat | Stat::Armor | Stat::MaxHp) {
                format!("{v:+.0}")
            } else {
                format!("{v:+.1}")
            }
        };
        format!("{} {}", num(d.stat), d.stat.name())
    }

    /// 从基础装备生成带词缀的物品（暗黑式 roll）
    pub fn roll(&self, rng: &mut Rng, base: &str, min_rarity: usize) -> Item {
        let d = self.def(base);
        // 稀有度 roll：Common 45 / Magic 30 / Rare 16 / Masterwork 6 / Legendary 3
        let weights = [45, 30, 16, 6, 3];
        let mut r = rng.range_i32(0, weights.iter().sum::<u32>() as i32 - 1) as u32;
        let mut tier = 0usize;
        for (i, w) in weights.iter().enumerate() {
            if r < *w {
                tier = i;
                break;
            }
            r -= *w;
        }
        tier = tier.max(min_rarity);
        let n = tier.saturating_sub(1); // Magic=1, Rare=2, Masterwork=3, Legendary=4
        let ilvl = d.lvl as f32;
        let mut affixes = Vec::new();
        let (mut pre_pool, mut suf_pool): (Vec<&AffixDef>, Vec<&AffixDef>) =
            self.affixes.iter().partition(|a| a.prefix);
        for i in 0..n {
            let prefix = i % 2 == 0; // 交替前缀/后缀
            let pool = if prefix {
                &mut pre_pool
            } else {
                &mut suf_pool
            };
            if pool.is_empty() {
                continue;
            }
            let k = rng.range_i32(0, pool.len() as i32 - 1) as usize;
            let ad = pool.swap_remove(k);
            let base_v = ad.min + rng.range_f32(0.0, 1.0) * (ad.max - ad.min);
            let value = (base_v + ad.per_lvl * (ilvl - 1.0)).max(0.0);
            affixes.push(Affix { id: ad.id.clone(), value });
        }
        Item { def: base.to_string(), count: 1, affixes }
    }

    /// 掉落表 roll：每条目先过 chance，命中的按 weight 抽 1 条
    pub fn roll_loot(&self, rng: &mut Rng, table: &str) -> Vec<Item> {
        let Some(entries) = self.tables.get(table) else { return vec![] };
        let pool: Vec<&LootEntry> =
            entries.iter().filter(|e| rng.chance(e.chance)).collect();
        if pool.is_empty() {
            return vec![];
        }
        let total: u32 = pool.iter().map(|e| e.weight).sum();
        let mut r = rng.range_i32(0, total as i32 - 1) as u32;
        let mut picked = pool[0];
        for e in &pool {
            if r < e.weight {
                picked = e;
                break;
            }
            r -= e.weight;
        }
        let count = rng.range_i32(picked.min as i32, picked.max as i32) as u16;
        if picked.item == "GEAR_RANDOM" {
            // 随机一件装备（roll 词缀）
            let gear: Vec<&ItemDef> =
                self.defs.values().filter(|d| d.stack == 1).collect();
            if let Some(d) = gear.get(rng.range_i32(0, gear.len() as i32 - 1) as usize) {
                vec![self.roll(rng, &d.id, 0)]
            } else {
                vec![]
            }
        } else if self.defs.get(&picked.item).map(|d| d.stack) == Some(1) {
            vec![Item { def: picked.item.clone(), count: 1, affixes: vec![] }]
        } else {
            vec![Item { def: picked.item.clone(), count, affixes: vec![] }]
        }
    }

    /// 物品聚合属性（装备 def 基础 + 词缀）
    pub fn item_stats(&self, item: &Item, st: &mut Stats) {
        let d = self.def(&item.def);
        st.dmg_flat += d.dmg;
        st.armor += d.armor;
        st.hp += d.hp;
        st.atk_mult *= d.speed;
        for a in &item.affixes {
            let Some(ad) = self.affixes.iter().find(|x| x.id == a.id) else { continue };
            match ad.stat {
                Stat::DmgFlat => st.dmg_flat += a.value,
                Stat::DmgPct => st.dmg_pct += a.value,
                Stat::Armor => st.armor += a.value,
                Stat::MaxHp => st.hp += a.value,
                Stat::MovePct => st.move_pct += a.value,
                Stat::AtkPct => st.atk_pct += a.value,
                Stat::Crit => st.crit += a.value,
                Stat::CritDmg => st.crit_dmg += a.value,
            }
        }
    }
}

/// 聚合属性（装备 + 属性点）
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub dmg_flat: f32,
    pub dmg_pct: f32,
    pub armor: f32,
    pub hp: f32,
    pub move_pct: f32,
    /// 攻速乘积（武器 speed 连乘）与攻速百分比相加
    pub atk_mult: f32,
    pub atk_pct: f32,
    pub crit: f32,
    pub crit_dmg: f32,
}

impl Stats {
    pub fn new() -> Self {
        Self { atk_mult: 1.0, ..Default::default() }
    }

    /// 近战/投射最终伤害 = 基础 * (1+DmgPct/100) + DmgFlat
    pub fn damage(&self, base: f32) -> f32 {
        base * (1.0 + self.dmg_pct / 100.0) + self.dmg_flat
    }

    /// 护甲减伤比例 0~0.8
    pub fn mitigation(&self) -> f32 {
        (self.armor / (self.armor + 80.0)).min(0.8)
    }

    /// 攻速总倍率
    pub fn atk_speed(&self) -> f32 {
        self.atk_mult * (1.0 + self.atk_pct / 100.0)
    }
}
