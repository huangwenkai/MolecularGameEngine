//! 游戏工程：工程根即素材根目录（IDE 素材管理的基础层）
//!
//! 设计：
//! - 每个工程是一个目录 `projects/<name>/`，内含 assets/{data,shaders,anims,character}
//! - 资源读取统一走 `path_of(rel)`：工程内存在该文件则用它，否则回落内置资源
//!   （内置 = 仓库 assets/ 与 game/assets/，编译期路径常量）
//! - 打开工程后，所有编辑器（特效/材质/植被/动画/着色器/人物形象）读写都作用于工程文件，
//!   配合已有热重载即时生效
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub const PROJECTS_DIR: &str = "projects";
const STATE_PATH: &str = "saves/projects.ron";

/// 内置资源根：引擎侧 assets（materials/vegetation/shaders）
const ENGINE_ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/");
/// 内置资源根：游戏侧 assets（vfx/weapons/animations/anims/character）
const GAME_ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/");

/// 当前工程根（全局；编辑器各模块通过 path_of 读取）
static CURRENT: RwLock<Option<PathBuf>> = RwLock::new(None);

// ---------------------------------------------------------------------------
// 工程数据
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct State {
    /// 上次打开工程名
    last: Option<String>,
    recents: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectManager {
    pub current: Option<Project>,
    pub recents: Vec<String>,
    /// 新建工程输入框
    pub new_name: String,
    /// 最近一次操作提示
    pub msg: Option<String>,
}

impl ProjectManager {
    /// 读取存档状态（不自动打开工程，由调用方决定）
    pub fn load_state(&mut self) {
        if let Ok(s) = std::fs::read_to_string(STATE_PATH) {
            if let Ok(st) = ron::from_str::<State>(&s) {
                self.recents = st.recents;
                if let Some(name) = st.last {
                    self.open(&name);
                }
            }
        }
    }

    fn persist(&self) {
        let st = State {
            last: self.current.as_ref().map(|p| p.name.clone()),
            recents: self.recents.clone(),
        };
        if let Ok(s) = ron::ser::to_string_pretty(&st, Default::default()) {
            let _ = std::fs::create_dir_all("saves");
            let _ = std::fs::write(STATE_PATH, s);
        }
    }

    /// 打开工程（不存在则创建骨架）
    pub fn open(&mut self, name: &str) {
        let root = PathBuf::from(PROJECTS_DIR).join(name);
        let existed = root.exists();
        if !existed && std::fs::create_dir_all(&root).is_err() {
            self.msg = Some(format!("工程目录创建失败: {name}"));
            return;
        }
        self.current = Some(Project { name: name.to_string(), root: root.clone() });
        *CURRENT.write().unwrap() = Some(root);
        if !self.recents.contains(&name.to_string()) {
            self.recents.push(name.to_string());
        }
        self.msg = Some(format!(
            "已{}工程「{}」",
            if existed { "打开" } else { "创建" },
            name
        ));
        self.persist();
    }

    /// 关闭工程 → 回到内置资源
    pub fn close(&mut self) {
        self.current = None;
        *CURRENT.write().unwrap() = None;
        self.msg = Some("已关闭工程，使用内置资源".to_string());
        self.persist();
    }

    /// 新建工程：创建目录骨架 + 从内置复制默认素材
    pub fn create(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
            self.msg = Some("工程名不合法（不能为空或含 / \\ : * ? \" < > |）".to_string());
            return;
        }
        let root = PathBuf::from(PROJECTS_DIR).join(name);
        for sub in ["assets/data", "assets/shaders", "assets/anims", "assets/character"] {
            let _ = std::fs::create_dir_all(root.join(sub));
        }
        // 复制内置默认素材（缺失则跳过）
        let mut copied = 0;
        for rel in [
            "data/materials.ron",
            "data/vegetation.ron",
            "data/vfx.ron",
            "data/weapons.ron",
            "data/animations.ron",
            "data/skills.ron",
            "shaders/sprite.wgsl",
            "shaders/pixels.wgsl",
            "shaders/composite.wgsl",
            "shaders/bloom.wgsl",
        ] {
            let src = builtin_path(rel);
            if let Ok(s) = std::fs::read_to_string(&src) {
                let dst = root.join("assets").join(rel);
                if !dst.exists() && std::fs::write(&dst, s).is_ok() {
                    copied += 1;
                }
            }
        }
        self.msg = Some(format!("工程「{name}」已创建（复制 {copied} 个默认素材）"));
        self.open(name);
    }
}

/// 列出 projects/ 下已有工程
pub fn list_projects() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(PROJECTS_DIR) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                if let Some(n) = e.file_name().to_str() {
                    out.push(n.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// 路径解析
// ---------------------------------------------------------------------------

/// 内置资源路径（rel 如 "data/materials.ron" / "shaders/composite.wgsl"）
pub fn builtin_path(rel: &str) -> PathBuf {
    match rel {
        // 引擎侧（仓库根 assets/）
        r if r.starts_with("data/materials.ron")
            || r.starts_with("data/vegetation.ron")
            || r.starts_with("data/actions.ron")
            || r.starts_with("shaders/") =>
        {
            Path::new(ENGINE_ASSETS).join(rel)
        }
        // 游戏侧（game/assets/）
        _ => Path::new(GAME_ASSETS).join(rel),
    }
}

pub fn current_root() -> Option<PathBuf> {
    CURRENT.read().unwrap().clone()
}

/// 工程内路径（无论文件是否存在）——新建/保存用；无工程时返回 None
pub fn project_path(rel: &str) -> Option<PathBuf> {
    current_root().map(|r| r.join("assets").join(rel))
}

/// 资源解析：工程内存在该文件 → 用它；否则内置
pub fn path_of(rel: &str) -> PathBuf {
    if let Some(p) = project_path(rel) {
        if p.exists() {
            return p;
        }
    }
    builtin_path(rel)
}

/// 资源目录解析（着色器/精灵表/人物形象等按目录扫描）
pub fn dir_of(rel: &str) -> PathBuf {
    if let Some(r) = current_root() {
        let d = r.join("assets").join(rel);
        if d.exists() {
            return d;
        }
    }
    builtin_path(rel)
}

/// 需要监听热重载的目录（工程打开时 = 工程目录；否则 = 内置两个 data 目录 + 着色器目录）
pub fn watch_dirs() -> Vec<PathBuf> {
    if let Some(r) = current_root() {
        return vec![r.join("assets").join("data"), r.join("assets").join("shaders")];
    }
    vec![
        Path::new(ENGINE_ASSETS).join("data"),
        Path::new(GAME_ASSETS).join("data"),
        Path::new(ENGINE_ASSETS).join("shaders"),
    ]
}

/// 素材分组（IDE 文件树用）：显示名 → 相对目录/后缀
pub const GROUPS: [(&str, &str, &str); 5] = [
    ("数据表 RON", "data", "ron"),
    ("着色器 WGSL", "shaders", "wgsl"),
    ("精灵表 PNG", "anims", "png"),
    ("人物形象 PNG", "character", "png"),
    ("存档", "saves", "ron"),
];

/// 某分组的文件列表（工程优先，否则内置；内置数据表会合并两个目录）
pub fn list_group(kind: usize) -> Vec<PathBuf> {
    let (_, dir, ext) = GROUPS[kind];
    let mut out: Vec<PathBuf> = Vec::new();
    if dir == "saves" {
        if let Ok(rd) = std::fs::read_dir(Path::new("saves")) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) == Some(ext) {
                    out.push(p);
                }
            }
        }
        out.sort();
        return out;
    }
    let d = dir_of(dir);
    if let Ok(rd) = std::fs::read_dir(&d) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some(ext) {
                out.push(p);
            }
        }
    }
    // 内置数据表分布在两个目录（引擎侧 + 游戏侧）
    if kind == 0 && current_root().is_none() {
        if let Ok(rd) = std::fs::read_dir(Path::new(GAME_ASSETS).join("data")) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) == Some(ext) {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}
