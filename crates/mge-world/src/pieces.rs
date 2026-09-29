//! 建筑件：木墙/门/工作台/床（实体级放置，区别于 1px 地形）
//! 木墙与关闭的门参与碰撞（solid_px）；门随玩家靠近自动开、离开自动关。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PieceKind {
    Wall,
    Door,
    Workbench,
    Bed,
}

impl PieceKind {
    /// 像素尺寸（宽, 高）
    pub fn size(&self) -> (i32, i32) {
        match self {
            PieceKind::Wall => (8, 8),
            PieceKind::Door => (6, 12),
            PieceKind::Workbench => (12, 6),
            PieceKind::Bed => (8, 14),
        }
    }

    /// 中文名（UI/提示）
    pub fn name(&self) -> &'static str {
        match self {
            PieceKind::Wall => "木墙",
            PieceKind::Door => "木门",
            PieceKind::Workbench => "工作台",
            PieceKind::Bed => "木床",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Piece {
    pub kind: PieceKind,
    /// 左上角（像素坐标，2px 吸附）
    pub x: i32,
    pub y: i32,
    /// 门：玩家靠近时自动开启
    pub open: bool,
}

impl Piece {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        let (w, h) = self.kind.size();
        x >= self.x && x < self.x + w && y >= self.y && y < self.y + h
    }

    /// 与矩形（左上 x,y 尺寸 w,h）是否重叠
    pub fn overlaps(&self, x: i32, y: i32, w: i32, h: i32) -> bool {
        let (pw, ph) = self.kind.size();
        x < self.x + pw && self.x < x + w && y < self.y + ph && self.y < y + h
    }
}
