use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Tab,
    Pane,
    Split,
    Stack,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum SplitDirection {
    Row,
    Column,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Focus {
    #[serde(default)]
    pub tab: Option<String>,
    #[serde(default)]
    pub pane: Option<String>,
    #[serde(default)]
    pub stack: Option<String>,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Stack {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub is_loading: bool,
    #[serde(default)]
    pub icon: crate::PageIcon,
    #[serde(default)]
    pub is_self: bool,
    /// For terminal stacks: the terminal's `ProcessId` (its handle for `run` /
    /// `read_terminal`). `None` for browser stacks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<String>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[rkyv(serialize_bounds(__S: rkyv::ser::Writer + rkyv::ser::Allocator))]
#[rkyv(deserialize_bounds(__D::Error: rkyv::rancor::Source))]
#[rkyv(bytecheck(bounds(__C: rkyv::validation::ArchiveContext)))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayoutNode {
    Split {
        #[serde(default)]
        id: Option<String>,
        direction: SplitDirection,
        #[serde(default)]
        flex_weights: Vec<f32>,
        #[rkyv(omit_bounds)]
        children: Vec<LayoutNode>,
    },
    Pane {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        is_zoomed: bool,
        #[serde(default)]
        stacks: Vec<Stack>,
    },
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Tab {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub is_active: bool,
    pub root: LayoutNode,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct LayoutSnapshot {
    pub tabs: Vec<Tab>,
    pub focused: Focus,
}

pub fn format_id(kind: NodeKind, value: u64) -> String {
    match kind {
        NodeKind::Tab => format!("tab:{value}"),
        NodeKind::Pane => format!("pane:{value}"),
        NodeKind::Split => format!("split:{value}"),
        NodeKind::Stack => format!("stack:{value}"),
    }
}

pub fn parse_id(s: &str) -> Result<(NodeKind, u64), String> {
    let (prefix, rest) = s
        .split_once(':')
        .ok_or_else(|| format!("id missing ':' separator: {s:?}"))?;
    let kind = match prefix {
        "tab" => NodeKind::Tab,
        "pane" => NodeKind::Pane,
        "split" => NodeKind::Split,
        "stack" => NodeKind::Stack,
        other => return Err(format!("unknown id prefix {other:?} in {s:?}")),
    };
    let value: u64 = rest
        .parse()
        .map_err(|err| format!("id value not u64 in {s:?}: {err}"))?;
    Ok((kind, value))
}

#[cfg(test)]
#[path = "layout.test.rs"]
mod tests;
