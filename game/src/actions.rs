//! 数据驱动动作系统：连招、前摇/后摇/命中帧、取消窗口、输入缓冲
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct ActionDef {
    pub name: String,
    pub windup: u8,
    pub active: u8,
    pub recovery: u8,
    pub hit_offset: (f32, f32),
    pub hit_size: (f32, f32),
    pub damage: f32,
    pub knockback: (f32, f32),
    pub next: Option<String>,
    pub cancel_from: u8,
    pub swing_from: f32,
    pub swing_to: f32,
}

#[derive(Debug, Clone, Deserialize)]
struct ActionsFile {
    actions: Vec<ActionDef>,
}

#[derive(Debug, Clone)]
pub struct ActionTable {
    map: HashMap<String, ActionDef>,
    chain: Vec<String>,
}

impl ActionTable {
    pub fn from_ron(text: &str) -> Result<Self, String> {
        let f: ActionsFile = ron::from_str(text).map_err(|e| format!("actions.ron error: {e}"))?;
        let mut map = HashMap::new();
        let mut chain = Vec::new();
        for a in f.actions {
            if chain.is_empty() || a.name != *chain.last().unwrap() {
                if !chain.is_empty() {
                    // 多套连招场景暂只支持一套链的记录
                }
                chain.push(a.name.clone());
            }
            map.insert(a.name.clone(), a);
        }
        Ok(Self { map, chain })
    }

    pub fn embedded() -> Self {
        Self::from_ron(include_str!("../../assets/data/actions.ron")).expect("embedded actions.ron invalid")
    }

    pub fn get(&self, name: &str) -> Option<&ActionDef> {
        self.map.get(name)
    }

    pub fn chain_start(&self) -> Option<String> {
        self.chain.first().cloned()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Windup,
    Active,
    Recovery,
}

#[derive(Debug, Clone)]
pub struct ActionState {
    pub current: Option<ActionDef>,
    /// 累计帧（攻速缩放后，f32）
    pub frame: f32,
    pub phase: Phase,
    /// 连招缓冲：active/recovery 期间按下攻击
    pub buffered: bool,
    /// 本段攻击已命中的目标
    pub hit_done: bool,
    pub aim_angle: f32,
}

impl Default for ActionState {
    fn default() -> Self {
        Self { current: None, frame: 0.0, phase: Phase::Windup, buffered: false, hit_done: false, aim_angle: 0.0 }
    }
}

impl ActionState {
    pub fn busy(&self) -> bool {
        self.current.is_some()
    }

    /// 每逻辑帧推进。attack_pressed：本帧攻击键刚按下；scale：攻速倍率。
    /// 返回 (进入Active第一帧, 结束)
    pub fn update(&mut self, table: &ActionTable, attack_pressed: bool, scale: f32) -> (bool, bool) {
        let mut started = false;
        let mut finished = false;
        if attack_pressed {
            match &self.current {
                None => {
                    if let Some(name) = table.chain_start() {
                        self.start(table, &name);
                        started = true;
                    }
                }
                Some(_) => self.buffered = true,
            }
        }
        if let Some(def) = self.current.clone() {
            self.frame += scale;
            match self.phase {
                Phase::Windup => {
                    if self.frame >= def.windup as f32 {
                        self.phase = Phase::Active;
                        self.frame = 0.0;
                        self.hit_done = false;
                        started = true; // Active 第一帧
                    }
                }
                Phase::Active => {
                    if self.frame >= def.active as f32 {
                        self.phase = Phase::Recovery;
                        self.frame = 0.0;
                    }
                }
                Phase::Recovery => {
                    let can_cancel = self.frame >= def.cancel_from as f32 && self.buffered;
                    if can_cancel {
                        if let Some(next) = &def.next {
                            let next = next.clone();
                            self.start(table, &next);
                            self.buffered = false;
                            started = true;
                            return (started, false);
                        }
                    }
                    if self.frame >= def.recovery as f32 {
                        self.current = None;
                        self.frame = 0.0;
                        self.buffered = false;
                        finished = true;
                    }
                }
            }
        }
        (started, finished)
    }

    fn start(&mut self, table: &ActionTable, name: &str) {
        if let Some(def) = table.get(name).cloned() {
            self.current = Some(def);
            self.frame = 0.0;
            self.phase = Phase::Windup;
            self.hit_done = false;
        }
    }

    /// 当前手臂角度（弧度，世界系 y 向下）。aim：瞄准角
    pub fn arm_angle(&self, aim: f32) -> f32 {
        let Some(def) = &self.current else { return aim };
        let from = def.swing_from.to_radians();
        let to = def.swing_to.to_radians();
        match self.phase {
            Phase::Windup => {
                let t = (self.frame as f32 / def.windup.max(1) as f32).min(1.0);
                aim + (from - aim) * t
            }
            Phase::Active => {
                let t = self.frame as f32 / def.active.max(1) as f32;
                from + (to - from) * t
            }
            Phase::Recovery => {
                let t = (self.frame as f32 / def.recovery.max(1) as f32).min(1.0);
                to + (aim - to) * t * 0.6
            }
        }
    }

    /// 当前命中框（世界系 aabb 中心/半尺寸）
    pub fn hitbox(&self, pos: glam::Vec2, facing: f32) -> Option<(glam::Vec2, glam::Vec2)> {
        let def = self.current.as_ref()?;
        if self.phase != Phase::Active {
            return None;
        }
        use glam::Vec2;
        let center = pos
            + Vec2::new(def.hit_offset.0 * facing, def.hit_offset.1)
            + Vec2::new(0.0, -10.0);
        Some((center, Vec2::new(def.hit_size.0 * 0.5, def.hit_size.1 * 0.5)))
    }
}
