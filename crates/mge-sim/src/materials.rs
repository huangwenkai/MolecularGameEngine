//! 材质定义表：由 assets/data/materials.ron 驱动（内嵌默认值兜底 + 运行时热加载覆盖）
use serde::Deserialize;
use std::collections::HashMap;

pub type MaterialId = u8;

/// id 0 恒为空
pub const EMPTY: MaterialId = 0;
/// 越界/静态占位（永远不动）
pub const OOB: MaterialId = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Kind {
    /// 粉末：受重力，可堆积（沙、灰、碎屑）
    Powder,
    /// 液体：受重力，水平扩散（水、油、岩浆、酸）
    Liquid,
    /// 气体：上升扩散，有寿命（烟、蒸汽）
    Gas,
    /// 静态地形：不参与模拟（土/石/木/矿物…），可被挖掘/腐蚀/燃烧
    Static,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Special {
    /// 火：点燃邻居、遇水变蒸汽
    Fire,
    /// 岩浆：点燃邻居、遇水凝结成石屑
    Lava,
    /// 酸：腐蚀 Tile 与可溶像素
    Acid,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MaterialDef {
    pub name: String,
    pub kind: Kind,
    #[serde(default)]
    pub color: (u8, u8, u8),
    /// 颜色抖动幅度（0..=64）
    #[serde(default)]
    pub jitter: u8,
    /// 密度：大的下沉，小的上浮（液体/粉末互换规则用）
    #[serde(default = "dflt_density")]
    pub density: u16,
    /// 液体水平扩散步数
    #[serde(default)]
    pub dispersal: u8,
    /// 0..=255，被火点燃的概率权重（0 = 不可燃）
    #[serde(default)]
    pub flammable: u8,
    /// 燃烧持续 tick 数（转为火后的寿命）
    #[serde(default = "dflt_burn")]
    pub burn_life: u8,
    /// 出生寿命（气体用：火/烟/蒸汽），0 = 永久
    #[serde(default)]
    pub life: u8,
    /// 发光贡献（0..=255）
    #[serde(default)]
    pub emissive: u8,
    #[serde(default)]
    pub special: Option<Special>,
    // ---- 静态地形属性（Kind::Static）----
    /// 实心碰撞
    #[serde(default)]
    pub solid: bool,
    /// 单向平台（仅上方碰撞）
    #[serde(default)]
    pub platform: bool,
    /// 可攀爬（绳索）
    #[serde(default)]
    pub climbable: bool,
    /// 挖掘硬度：每像素伤害需求（0 = 不可挖）
    #[serde(default)]
    pub hp: u16,
    /// 自发光（火把）
    #[serde(default)]
    pub light: u8,
    /// 免疫酸腐蚀
    #[serde(default)]
    pub acid_proof: bool,
    /// 破坏后掉落的碎屑材质
    #[serde(default)]
    pub drop: Option<String>,
}

fn dflt_density() -> u16 {
    1000
}
fn dflt_burn() -> u8 {
    60
}

#[derive(Debug, Clone, Deserialize)]
struct MaterialsFile {
    materials: Vec<MaterialDef>,
}

#[derive(Debug, Clone)]
pub struct Materials {
    defs: Vec<MaterialDef>,
    by_name: HashMap<String, MaterialId>,
}

impl Materials {
    /// 从 RON 文本解析
    pub fn from_ron(text: &str) -> Result<Self, String> {
        let file: MaterialsFile =
            ron::from_str(text).map_err(|e| format!("materials.ron parse error: {e}"))?;
        let mut defs: Vec<MaterialDef> = vec![MaterialDef {
            name: "empty".into(),
            kind: Kind::Static,
            color: (0, 0, 0),
            jitter: 0,
            density: 0,
            dispersal: 0,
            flammable: 0,
            burn_life: 0,
            life: 0,
            emissive: 0,
            special: None,
            solid: false,
            platform: false,
            climbable: false,
            hp: 0,
            light: 0,
            acid_proof: false,
            drop: None,
        }];
        let mut by_name = HashMap::new();
        by_name.insert("empty".to_string(), EMPTY);
        for d in file.materials {
            if by_name.contains_key(&d.name) {
                return Err(format!("duplicate material name: {}", d.name));
            }
            let id = defs.len() as MaterialId;
            by_name.insert(d.name.clone(), id);
            defs.push(d);
        }
        Ok(Self { defs, by_name })
    }

    /// 内嵌默认表（编译期包含 assets/data/materials.ron）
    pub fn embedded() -> Self {
        Self::from_ron(include_str!("../../../assets/data/materials.ron"))
            .expect("embedded materials.ron invalid")
    }

    pub fn def(&self, id: MaterialId) -> &MaterialDef {
        self.defs.get(id as usize).unwrap_or_else(|| &self.defs[0])
    }

    pub fn id(&self, name: &str) -> Option<MaterialId> {
        self.by_name.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}
