# General Layout MCP API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace ~40 auto-generated layout MCP tools and 5 read-only query tools with two declarative tools: `read_layout` and `update_layout`. Agent reads the layout tree, mutates whatever it wants, submits it back; vmux reconciles by id (React-style).

**Architecture:** New protocol types (`LayoutSnapshot`, `LayoutNodeDto`, `TabDto`, `SpaceDto`, `FocusDto`) in `vmux_service::protocol`. `read_layout` walks the Bevy ECS tree and emits the snapshot. `update_layout` runs a 3-phase reconciler (validate → plan diff → apply atomically) over the ECS world. Hand-built JSON schemas in `vmux_mcp/src/tools.rs` (the `McpTool` derive macro can't emit recursive object schemas).

**Tech Stack:** Bevy 0.18 ECS, `serde` + `rkyv` (protocol round-trip), `serde_json` (MCP wire), existing layout/pane/stack/tab systems in `vmux_layout` and `vmux_desktop`.

**Spec:** `docs/specs/2026-05-16-general-layout-mcp-api-design.md` — read this first.

**Pre-commit:** AGENTS.md requires `cargo fmt -p <pkg> -- --check`, `env -u CEF_PATH cargo clippy -p <pkg> --all-targets -- -D warnings`, and `env -u CEF_PATH cargo test -p <pkg>` on each changed crate before every commit and push. The helper `BASE=origin/main ./scripts/changed-crates.sh` computes the changed set. Commit messages must NOT include `Co-Authored-By` trailers.

---

## Terminology Map (Spec ↔ ECS)

Internal vmux naming diverges from the user-facing terminology this spec uses. Always translate at the protocol boundary:

| Spec API | ECS Component | Notes |
|---|---|---|
| `space` | `Tab { name }` | Top-level container per space. `FocusedStack.tab` holds the focused space entity. |
| `pane` (leaf) | Entity `With<Pane>` `Without<PaneSplit>` | Contains `Stack` children. |
| `split` (internal node) | Entity `With<Pane>` `With<PaneSplit>` | Has `direction: PaneSplitDirection` and `Pane` children. |
| `tab` (user-facing) | `Stack` | `FocusedStack.stack` holds focused tab entity. Children are the webview/terminal content. |
| flex weight | `Node { flex_grow }` on each split child | Bevy UI Node component, not a separate weights array. |

Use string id prefixes at the protocol boundary: `space:<entity_bits>`, `pane:<entity_bits>`, `split:<entity_bits>`, `tab:<entity_bits>`. Entity bits are `Entity::to_bits()`, the same convention already used by `agent_query.rs`.

## File Structure

**New files:**
- `crates/vmux_service/src/protocol/layout.rs` — `LayoutSnapshot`, `LayoutNodeDto`, `TabDto`, `SpaceDto`, `FocusDto`, prefixed-id parse/format helpers
- `crates/vmux_desktop/src/agent_layout.rs` — `read_layout` walker + `update_layout` reconciler (validate, plan, apply)
- `crates/vmux_desktop/src/agent_layout/reconcile.rs` — pure functions: id parsing, diff planning, validation (no Bevy world access; testable in isolation)

**Modified files:**
- `crates/vmux_service/src/protocol.rs` — add `AgentQuery::ReadLayout`, `AgentCommand::UpdateLayout`, `AgentQueryResult::Layout`, `AgentCommandResult::Layout`. Remove `AgentQuery::{GetState, ListTabs, ListSpaces, ListTerminals, GetFocused}`, `AgentQueryResult::{State, Tabs, Spaces, Terminals, Focused}`, and the deleted DTOs (`StateSnapshot`, `FocusedInfo`, `PaneInfo`, `SpaceInfo`, `TabInfo`, `TerminalInfo`). Keep `AgentQuery::GetSettings` and `AgentQueryResult::Settings` (unrelated).
- `crates/vmux_mcp/src/tools.rs` — delete `McpQueryTool` enum + impl. Add hand-built `ToolDefinition` entries for `read_layout` and `update_layout` in `tool_definitions()`. Route both in `dispatch_from_tool_call()`.
- `crates/vmux_command/src/command.rs` — strip `McpTool` derive from `LayoutCommand`, `PaneCommand`, `TabCommand`, `StackCommand`, `WindowCommand`, `ZenCommand`, `SpaceCommand`. Update `mcp_lookup_resolves_every_command_id` test.
- `crates/vmux_desktop/src/lib.rs` — register the new module.
- `crates/vmux_desktop/src/agent.rs` — add `UpdateLayout` arm in `handle_agent_commands`.
- `crates/vmux_desktop/src/agent_query.rs` — remove the five deleted query arms. Either delete the file (if `GetSettings` is the only survivor and can be inlined elsewhere) or trim the body.
- `crates/vmux_cli/tests/mcp_smoke.rs` — drop assertions for removed tool names; add coverage for `read_layout` and `update_layout`.

**Crates touched:** `vmux_service`, `vmux_command`, `vmux_mcp`, `vmux_desktop`, `vmux_cli`. `vmux_macro` is NOT touched — we bypass the macro for the two new tools.

---

## Task 1: New protocol types + IDs

**Files:**
- Create: `crates/vmux_service/src/protocol/layout.rs`
- Modify: `crates/vmux_service/src/protocol.rs:1-2` (declare submodule)
- Modify: `crates/vmux_service/Cargo.toml` (no changes expected; verify `rkyv`, `serde`, `bevy_ecs` already present)

- [ ] **Step 1: Create the protocol submodule scaffold**

Add to top of `crates/vmux_service/src/protocol.rs`:

```rust
pub mod layout;
pub use layout::{FocusDto, LayoutNodeDto, LayoutSnapshot, SpaceDto, SplitDirectionDto, TabDto};
```

Create `crates/vmux_service/src/protocol/layout.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq,
    Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct LayoutSnapshot {
    pub spaces: Vec<SpaceDto>,
    pub focused: FocusDto,
}

#[derive(
    Debug, Clone, PartialEq, Eq,
    Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct SpaceDto {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub is_active: bool,
    pub root: LayoutNodeDto,
}

#[derive(
    Debug, Clone, PartialEq, Eq,
    Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayoutNodeDto {
    Split {
        #[serde(default)]
        id: Option<String>,
        direction: SplitDirectionDto,
        #[serde(default)]
        flex_weights: Vec<f32>,
        children: Vec<LayoutNodeDto>,
    },
    Pane {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        is_zoomed: bool,
        #[serde(default)]
        tabs: Vec<TabDto>,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq,
    Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum SplitDirectionDto {
    Row,
    Column,
}

#[derive(
    Debug, Clone, PartialEq, Eq,
    Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct TabDto {
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub favicon_url: String,
}

#[derive(
    Debug, Clone, Default, PartialEq, Eq,
    Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct FocusDto {
    #[serde(default)]
    pub space: Option<String>,
    #[serde(default)]
    pub pane: Option<String>,
    #[serde(default)]
    pub tab: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Space,
    Pane,
    Split,
    Tab,
}

pub fn format_id(kind: NodeKind, value: u64) -> String {
    match kind {
        NodeKind::Space => format!("space:{value}"),
        NodeKind::Pane => format!("pane:{value}"),
        NodeKind::Split => format!("split:{value}"),
        NodeKind::Tab => format!("tab:{value}"),
    }
}

pub fn parse_id(s: &str) -> Result<(NodeKind, u64), String> {
    let (prefix, rest) = s
        .split_once(':')
        .ok_or_else(|| format!("id missing ':' separator: {s:?}"))?;
    let kind = match prefix {
        "space" => NodeKind::Space,
        "pane" => NodeKind::Pane,
        "split" => NodeKind::Split,
        "tab" => NodeKind::Tab,
        other => return Err(format!("unknown id prefix {other:?} in {s:?}")),
    };
    let value: u64 = rest
        .parse()
        .map_err(|err| format!("id value not u64 in {s:?}: {err}"))?;
    Ok((kind, value))
}
```

- [ ] **Step 2: Write round-trip tests**

Append to `crates/vmux_service/src/protocol/layout.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_id_round_trips_each_kind() {
        for (kind, value) in [
            (NodeKind::Space, 1_u64),
            (NodeKind::Pane, 42),
            (NodeKind::Split, 17),
            (NodeKind::Tab, 9999),
        ] {
            let formatted = format_id(kind, value);
            let (parsed_kind, parsed_value) = parse_id(&formatted).unwrap();
            assert_eq!(parsed_kind, kind);
            assert_eq!(parsed_value, value);
        }
    }

    #[test]
    fn parse_id_rejects_missing_separator() {
        assert!(parse_id("pane42").is_err());
    }

    #[test]
    fn parse_id_rejects_unknown_prefix() {
        assert!(parse_id("window:1").is_err());
    }

    #[test]
    fn parse_id_rejects_non_numeric_value() {
        assert!(parse_id("pane:abc").is_err());
    }

    #[test]
    fn layout_snapshot_json_round_trip_minimal() {
        let snapshot = LayoutSnapshot {
            spaces: vec![SpaceDto {
                id: Some("space:1".into()),
                name: "Work".into(),
                is_active: true,
                root: LayoutNodeDto::Pane {
                    id: Some("pane:2".into()),
                    is_zoomed: false,
                    tabs: vec![],
                },
            }],
            focused: FocusDto {
                space: Some("space:1".into()),
                pane: Some("pane:2".into()),
                tab: None,
            },
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let parsed: LayoutSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, snapshot);
    }

    #[test]
    fn layout_node_json_discriminator_uses_kind_field() {
        let pane = LayoutNodeDto::Pane {
            id: Some("pane:7".into()),
            is_zoomed: true,
            tabs: vec![],
        };
        let json = serde_json::to_value(&pane).unwrap();
        assert_eq!(json["kind"], "pane");
        assert_eq!(json["id"], "pane:7");
        assert_eq!(json["is_zoomed"], true);
    }

    #[test]
    fn split_serializes_with_snake_case_direction() {
        let split = LayoutNodeDto::Split {
            id: None,
            direction: SplitDirectionDto::Column,
            flex_weights: vec![1.0, 2.0],
            children: vec![],
        };
        let json = serde_json::to_value(&split).unwrap();
        assert_eq!(json["kind"], "split");
        assert_eq!(json["direction"], "column");
    }

    #[test]
    fn rkyv_round_trip_preserves_tree() {
        let snapshot = LayoutSnapshot {
            spaces: vec![SpaceDto {
                id: Some("space:1".into()),
                name: "S".into(),
                is_active: true,
                root: LayoutNodeDto::Split {
                    id: Some("split:5".into()),
                    direction: SplitDirectionDto::Row,
                    flex_weights: vec![1.0, 1.0],
                    children: vec![
                        LayoutNodeDto::Pane {
                            id: Some("pane:10".into()),
                            is_zoomed: false,
                            tabs: vec![TabDto {
                                id: Some("tab:abc".into()),
                                title: "T".into(),
                                url: "https://x".into(),
                                kind: "browser".into(),
                                is_loading: false,
                                favicon_url: String::new(),
                            }],
                        },
                        LayoutNodeDto::Pane {
                            id: Some("pane:11".into()),
                            is_zoomed: false,
                            tabs: vec![],
                        },
                    ],
                },
            }],
            focused: FocusDto::default(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&snapshot).unwrap();
        let recovered: LayoutSnapshot =
            rkyv::from_bytes::<LayoutSnapshot, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(recovered, snapshot);
    }
}
```

- [ ] **Step 3: Run tests, verify they pass**

Run: `env -u CEF_PATH cargo test -p vmux_service protocol::layout`
Expected: 6 tests pass.

- [ ] **Step 4: Pre-commit checks**

```bash
cargo fmt -p vmux_service -- --check
env -u CEF_PATH cargo clippy -p vmux_service --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_service
```

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_service/src/protocol.rs crates/vmux_service/src/protocol/layout.rs
git commit -m "feat(protocol): add layout snapshot DTOs"
```

---

## Task 2: Wire ReadLayout/UpdateLayout into AgentQuery/AgentCommand

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs` (around the existing `AgentQuery`, `AgentCommand`, `AgentQueryResult`, `AgentCommandResult` enums)

- [ ] **Step 1: Write a round-trip test for the new variants**

Append to the existing test module in `crates/vmux_service/src/protocol.rs`:

```rust
#[test]
fn agent_query_read_layout_rkyv_round_trip() {
    let q = AgentQuery::ReadLayout;
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
    let recovered: AgentQuery = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(recovered, AgentQuery::ReadLayout);
}

#[test]
fn agent_command_update_layout_rkyv_round_trip() {
    use crate::protocol::layout::{FocusDto, LayoutNodeDto, LayoutSnapshot, SpaceDto};
    let cmd = AgentCommand::UpdateLayout {
        layout: LayoutSnapshot {
            spaces: vec![SpaceDto {
                id: Some("space:1".into()),
                name: "X".into(),
                is_active: true,
                root: LayoutNodeDto::Pane { id: Some("pane:2".into()), is_zoomed: false, tabs: vec![] },
            }],
            focused: FocusDto { space: Some("space:1".into()), pane: Some("pane:2".into()), tab: None },
        },
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
    let recovered: AgentCommand = rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(recovered, cmd);
}
```

- [ ] **Step 2: Run the new tests; expect compile failure**

Run: `env -u CEF_PATH cargo test -p vmux_service agent_query_read_layout`
Expected: FAIL — `AgentQuery::ReadLayout` and `AgentCommand::UpdateLayout` don't exist yet.

- [ ] **Step 3: Add the new variants**

In `crates/vmux_service/src/protocol.rs`, edit `AgentQuery`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentQuery {
    ReadLayout,
    GetSettings,
}
```

(Removes `GetState`, `ListTabs`, `ListSpaces`, `ListTerminals`, `GetFocused`. Keep `GetSettings`.)

Edit `AgentCommand` — append a new variant before the closing `}`:

```rust
    UpdateLayout {
        layout: crate::protocol::layout::LayoutSnapshot,
    },
```

Edit `AgentQueryResult`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentQueryResult {
    Layout(crate::protocol::layout::LayoutSnapshot),
    Settings(String),
    Error(String),
}
```

(Removes `State`, `Tabs`, `Spaces`, `Terminals`, `Focused`. Keep `Settings` and `Error`.)

Edit `AgentCommandResult`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum AgentCommandResult {
    Ok,
    Layout(crate::protocol::layout::LayoutSnapshot),
    Error(String),
}
```

- [ ] **Step 4: Delete the orphaned DTOs**

In the same file, delete: `StateSnapshot`, `FocusedInfo`, `PaneInfo`, `SpaceInfo`, `TabInfo`, `TerminalInfo`. Also delete any imports of them in this file that are now unused.

- [ ] **Step 5: Fix compile errors crate-wide**

```bash
env -u CEF_PATH cargo build -p vmux_service 2>&1 | head -50
```

Expected: protocol crate compiles. Some downstream test in the same crate may reference removed types — delete those tests and any other code that references the removed enum variants/DTOs (e.g., `validate_agent_command` arms that no longer apply — `UpdateLayout` doesn't need string-field validation).

- [ ] **Step 6: Run vmux_service tests**

Run: `env -u CEF_PATH cargo test -p vmux_service`
Expected: all pass, including the two new round-trip tests.

- [ ] **Step 7: Pre-commit checks**

```bash
cargo fmt -p vmux_service -- --check
env -u CEF_PATH cargo clippy -p vmux_service --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_service
```

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_service/src/protocol.rs
git commit -m "feat(protocol): add ReadLayout/UpdateLayout, drop legacy queries"
```

After this commit, downstream crates (`vmux_mcp`, `vmux_desktop`, `vmux_cli`) will not compile — they reference the removed types. The next tasks fix them.

---

## Task 3: Build `read_layout` walker

**Files:**
- Create: `crates/vmux_desktop/src/agent_layout.rs`
- Modify: `crates/vmux_desktop/src/lib.rs` (register module)
- Modify: `crates/vmux_desktop/src/agent_query.rs` (replace `GetState`/`ListTabs`/etc. arms with `ReadLayout`)

- [ ] **Step 1: Add the module skeleton**

Create `crates/vmux_desktop/src/agent_layout.rs`:

```rust
use crate::layout::{
    pane::{Pane, PaneSplit, PaneSplitDirection},
    stack::{FocusedStack, Stack},
    tab::Tab,
};
use bevy::prelude::*;
use vmux_core::PageMetadata;
use vmux_service::protocol::layout::{
    FocusDto, LayoutNodeDto, LayoutSnapshot, NodeKind, SpaceDto, SplitDirectionDto, TabDto,
    format_id,
};

type SpaceQuery<'w, 's> = Query<'w, 's, (Entity, &'static Tab, Option<&'static Children>)>;
type SplitNodeQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static PaneSplit, Option<&'static Children>), With<Pane>>;
type LeafPaneQuery<'w, 's> =
    Query<'w, 's, (Entity, Option<&'static Children>), (With<Pane>, Without<PaneSplit>)>;
type StackNodeQuery<'w, 's> =
    Query<'w, 's, (Entity, Option<&'static Children>, Option<&'static PageMetadata>), With<Stack>>;
type TerminalMark<'w, 's> =
    Query<'w, 's, Entity, With<crate::layout::stack::Stack>>; // refined below
type ChildrenQuery<'w, 's> = Query<'w, 's, &'static Children>;
type NodeGrowQuery<'w, 's> = Query<'w, 's, &'static Node>;

pub fn build_layout_snapshot(
    spaces_q: &SpaceQuery,
    splits_q: &SplitNodeQuery,
    leaves_q: &LeafPaneQuery,
    stacks_q: &StackNodeQuery,
    children_q: &ChildrenQuery,
    nodes_q: &NodeGrowQuery,
    is_terminal: &dyn Fn(Entity) -> bool,
    focused: &FocusedStack,
) -> LayoutSnapshot {
    let active_space = focused.tab;
    let spaces = spaces_q
        .iter()
        .map(|(space_entity, tab, children)| {
            let root = children
                .and_then(|c| c.iter().next())
                .map(|root_entity| {
                    build_node(
                        root_entity,
                        splits_q,
                        leaves_q,
                        stacks_q,
                        children_q,
                        nodes_q,
                        is_terminal,
                    )
                })
                .unwrap_or(LayoutNodeDto::Pane {
                    id: None,
                    is_zoomed: false,
                    tabs: Vec::new(),
                });
            SpaceDto {
                id: Some(format_id(NodeKind::Space, space_entity.to_bits())),
                name: tab.name.clone(),
                is_active: Some(space_entity) == active_space,
                root,
            }
        })
        .collect();

    LayoutSnapshot {
        spaces,
        focused: FocusDto {
            space: focused.tab.map(|e| format_id(NodeKind::Space, e.to_bits())),
            pane: focused.pane.map(|e| format_id(NodeKind::Pane, e.to_bits())),
            tab: focused.stack.map(|e| format_id(NodeKind::Tab, e.to_bits())),
        },
    }
}

fn build_node(
    entity: Entity,
    splits_q: &SplitNodeQuery,
    leaves_q: &LeafPaneQuery,
    stacks_q: &StackNodeQuery,
    children_q: &ChildrenQuery,
    nodes_q: &NodeGrowQuery,
    is_terminal: &dyn Fn(Entity) -> bool,
) -> LayoutNodeDto {
    if let Ok((split_entity, split, children)) = splits_q.get(entity) {
        let child_entities: Vec<Entity> = children
            .map(|c| c.iter().collect())
            .unwrap_or_default();
        let flex_weights = child_entities
            .iter()
            .map(|child| nodes_q.get(*child).map(|n| n.flex_grow).unwrap_or(1.0))
            .collect();
        let children_dto = child_entities
            .into_iter()
            .map(|child| {
                build_node(child, splits_q, leaves_q, stacks_q, children_q, nodes_q, is_terminal)
            })
            .collect();
        return LayoutNodeDto::Split {
            id: Some(format_id(NodeKind::Split, split_entity.to_bits())),
            direction: match split.direction {
                PaneSplitDirection::Row => SplitDirectionDto::Row,
                PaneSplitDirection::Column => SplitDirectionDto::Column,
            },
            flex_weights,
            children: children_dto,
        };
    }
    if let Ok((leaf_entity, leaf_children)) = leaves_q.get(entity) {
        let tabs = leaf_children
            .map(|c| {
                c.iter()
                    .filter_map(|child| stacks_q.get(child).ok())
                    .map(|(stack_entity, stack_children, page)| {
                        build_tab(stack_entity, stack_children, page, is_terminal)
                    })
                    .collect()
            })
            .unwrap_or_default();
        return LayoutNodeDto::Pane {
            id: Some(format_id(NodeKind::Pane, leaf_entity.to_bits())),
            is_zoomed: false, // TODO Task 4 wires zoom state
            tabs,
        };
    }
    LayoutNodeDto::Pane { id: None, is_zoomed: false, tabs: Vec::new() }
}

fn build_tab(
    stack_entity: Entity,
    children: Option<&Children>,
    page: Option<&PageMetadata>,
    is_terminal: &dyn Fn(Entity) -> bool,
) -> TabDto {
    let kind = if children
        .map(|c| c.iter().any(is_terminal))
        .unwrap_or(false)
    {
        "terminal"
    } else {
        "browser"
    };
    TabDto {
        id: Some(format_id(NodeKind::Tab, stack_entity.to_bits())),
        title: page.map(|p| p.title.clone()).unwrap_or_default(),
        url: page.map(|p| p.url.clone()).unwrap_or_default(),
        kind: kind.to_string(),
        is_loading: false,
        favicon_url: String::new(),
    }
}
```

(The `TerminalMark` type alias is wrong — fix it inline by using the actual Terminal component type and removing the placeholder. See Step 2.)

- [ ] **Step 2: Fix the terminal detection helper**

Replace the placeholder `TerminalMark` alias and `is_terminal` parameter with a real one. Inspect `crates/vmux_desktop/src/terminal.rs` for the `Terminal` component (already imported in `agent_query.rs` line 9). Pass the existing query type used there:

```rust
type TerminalQuery<'w, 's> =
    Query<'w, 's, Entity, With<crate::terminal::Terminal>>;
```

Replace the `is_terminal: &dyn Fn(Entity) -> bool` parameter in `build_layout_snapshot` and `build_node` and `build_tab` with `terminals: &TerminalQuery`. Update `build_tab`:

```rust
let kind = if children
    .map(|c| c.iter().any(|child| terminals.contains(child)))
    .unwrap_or(false) { "terminal" } else { "browser" };
```

Remove the `TerminalMark` placeholder alias.

- [ ] **Step 3: Register the module**

In `crates/vmux_desktop/src/lib.rs`, add:

```rust
pub mod agent_layout;
```

(Match the visibility used by neighboring modules — `agent_query` is `pub mod` in the existing source.)

- [ ] **Step 4: Wire `ReadLayout` into the query handler**

Edit `crates/vmux_desktop/src/agent_query.rs`:

Replace the entire `handle_agent_queries` function. The new version handles only `ReadLayout` and `GetSettings`:

```rust
use crate::{
    agent::AgentQueryRequest,
    agent_layout::build_layout_snapshot,
    layout::{
        pane::{Pane, PaneSplit},
        stack::{FocusedStack, Stack},
        tab::Tab,
    },
    terminal::{ServiceClient, Terminal},
};
use bevy::prelude::*;
use vmux_core::PageMetadata;
use vmux_service::protocol::{AgentQuery, AgentQueryResult, ClientMessage};

pub(crate) fn handle_agent_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    spaces: Query<(Entity, &Tab, Option<&Children>)>,
    splits: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
    leaves: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
    stacks: Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
    terminals: Query<Entity, With<Terminal>>,
    children_q: Query<&Children>,
    nodes_q: Query<&Node>,
    settings: Res<crate::settings::AppSettings>,
    focused: Option<Res<FocusedStack>>,
) {
    let Some(service) = service else { return };
    let Some(focused) = focused else { return };

    for request in reader.read() {
        let result = match request.query {
            AgentQuery::ReadLayout => AgentQueryResult::Layout(build_layout_snapshot(
                &spaces, &splits, &leaves, &stacks, &children_q, &nodes_q, &terminals, &focused,
            )),
            AgentQuery::GetSettings => {
                AgentQueryResult::Settings(crate::settings::serialize_settings_to_json(&settings))
            }
        };
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: request.request_id,
            result,
        });
    }
}
```

Update `build_layout_snapshot`'s signature in `agent_layout.rs` to accept a `&Query<Entity, With<Terminal>>` instead of the `&dyn Fn` callback.

Delete the old helper functions in `agent_query.rs` (`focused_info`, `collect_terminals`, `stack_kind`, `stack_info`, `collect_stacks`, `collect_tabs`, `gather_leaf_panes`, `build_state_snapshot`) — all subsumed.

Delete the existing two tests in `agent_query.rs`'s `tests` mod (they test removed helpers); we'll add `agent_layout` tests in step 5.

- [ ] **Step 5: Write `agent_layout` unit tests**

Append to `crates/vmux_desktop/src/agent_layout.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::pane::PaneSplitDirection;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(FocusedStack::default());
        app
    }

    #[test]
    fn empty_world_produces_empty_snapshot() {
        let mut app = make_app();
        let snapshot = app
            .world_mut()
            .run_system_once(
                |spaces: Query<(Entity, &Tab, Option<&Children>)>,
                 splits: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
                 leaves: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
                 stacks: Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
                 terminals: Query<Entity, With<crate::terminal::Terminal>>,
                 children_q: Query<&Children>,
                 nodes_q: Query<&Node>,
                 focused: Res<FocusedStack>| {
                    build_layout_snapshot(
                        &spaces, &splits, &leaves, &stacks, &children_q, &nodes_q, &terminals,
                        &focused,
                    )
                },
            )
            .unwrap();
        assert!(snapshot.spaces.is_empty());
        assert_eq!(snapshot.focused, FocusDto::default());
    }

    #[test]
    fn single_space_one_pane_one_tab_round_trips() {
        let mut app = make_app();
        let stack_entity = app.world_mut().spawn(Stack::default()).id();
        let pane_entity = app
            .world_mut()
            .spawn((Pane, Node::default(), ChildOf(stack_entity)))
            .id();
        let _ = app.world_mut().entity_mut(stack_entity).insert(ChildOf(pane_entity)); // adjust if Bevy 0.18 reparenting differs
        let space_entity = app.world_mut().spawn(Tab { name: "W".into() }).id();
        app.world_mut().entity_mut(pane_entity).insert(ChildOf(space_entity));

        {
            let mut f = app.world_mut().resource_mut::<FocusedStack>();
            f.tab = Some(space_entity);
            f.pane = Some(pane_entity);
            f.stack = Some(stack_entity);
        }

        let snapshot = app
            .world_mut()
            .run_system_once(
                |spaces: Query<(Entity, &Tab, Option<&Children>)>,
                 splits: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
                 leaves: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
                 stacks: Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
                 terminals: Query<Entity, With<crate::terminal::Terminal>>,
                 children_q: Query<&Children>,
                 nodes_q: Query<&Node>,
                 focused: Res<FocusedStack>| {
                    build_layout_snapshot(
                        &spaces, &splits, &leaves, &stacks, &children_q, &nodes_q, &terminals,
                        &focused,
                    )
                },
            )
            .unwrap();

        assert_eq!(snapshot.spaces.len(), 1);
        let space = &snapshot.spaces[0];
        assert_eq!(space.name, "W");
        assert!(space.is_active);
        match &space.root {
            LayoutNodeDto::Pane { tabs, .. } => assert_eq!(tabs.len(), 1),
            other => panic!("expected pane root, got {other:?}"),
        }
        assert_eq!(snapshot.focused.space, Some(format_id(NodeKind::Space, space_entity.to_bits())));
    }

    #[test]
    fn split_with_two_panes_produces_recursive_node() {
        let mut app = make_app();
        let space = app.world_mut().spawn(Tab { name: "S".into() }).id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit { direction: PaneSplitDirection::Row },
                Node::default(),
                ChildOf(space),
            ))
            .id();
        let pane_a = app
            .world_mut()
            .spawn((Pane, Node { flex_grow: 1.0, ..Default::default() }, ChildOf(split)))
            .id();
        let pane_b = app
            .world_mut()
            .spawn((Pane, Node { flex_grow: 2.0, ..Default::default() }, ChildOf(split)))
            .id();

        {
            let mut f = app.world_mut().resource_mut::<FocusedStack>();
            f.tab = Some(space);
        }

        let snapshot = app
            .world_mut()
            .run_system_once(
                |spaces: Query<(Entity, &Tab, Option<&Children>)>,
                 splits: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
                 leaves: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
                 stacks: Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
                 terminals: Query<Entity, With<crate::terminal::Terminal>>,
                 children_q: Query<&Children>,
                 nodes_q: Query<&Node>,
                 focused: Res<FocusedStack>| {
                    build_layout_snapshot(
                        &spaces, &splits, &leaves, &stacks, &children_q, &nodes_q, &terminals,
                        &focused,
                    )
                },
            )
            .unwrap();

        let root = &snapshot.spaces[0].root;
        match root {
            LayoutNodeDto::Split { direction, flex_weights, children, .. } => {
                assert_eq!(*direction, SplitDirectionDto::Row);
                assert_eq!(flex_weights, &vec![1.0, 2.0]);
                assert_eq!(children.len(), 2);
            }
            other => panic!("expected split, got {other:?}"),
        }
        let _ = (pane_a, pane_b);
    }
}
```

`run_system_once` requires the `bevy::ecs::system::RunSystemOnce` extension trait — import it where needed.

- [ ] **Step 6: Compute changed crates and run pre-commit checks**

```bash
BASE=origin/main ./scripts/changed-crates.sh
# Expected: vmux_service vmux_desktop

for pkg in vmux_service vmux_desktop; do cargo fmt -p "$pkg" -- --check; done
for pkg in vmux_service vmux_desktop; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in vmux_service vmux_desktop; do env -u CEF_PATH cargo test -p "$pkg"; done
```

If clippy complains about the rkyv attribute placement on `LayoutSnapshot` inside `AgentCommand::UpdateLayout`, the fix is usually a missing `rkyv::Archive` bound on the variant — refer to existing variants like `BrowserNavigate` for the exact attribute pattern.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_desktop/src/agent_layout.rs crates/vmux_desktop/src/agent_query.rs crates/vmux_desktop/src/lib.rs
git commit -m "feat(agent): add read_layout walker"
```

---

## Task 4: Plumb pane zoom state into snapshot

**Files:**
- Modify: `crates/vmux_desktop/src/agent_layout.rs` (replace the `is_zoomed: false` placeholder)

The spec requires `is_zoomed` on the pane node. Existing zoom state lives on a Bevy component; locate it during this task.

- [ ] **Step 1: Locate the zoom marker component**

Run: `bash -c "grep -rn 'Zoom\|zoomed\|is_zoom' /Users/junichi.sugiura/Projects/github.com/vmux-ai/vmux/.worktrees/vmx-121/crates/vmux_layout/src/ --include='*.rs'"`

Identify the component or resource that signals a pane is zoomed (likely a marker component inserted on the focused pane when `PaneCommand::Zoom` fires). Note the exact path.

- [ ] **Step 2: Write a failing test**

In `crates/vmux_desktop/src/agent_layout.rs` tests, add:

```rust
#[test]
fn zoomed_pane_reports_is_zoomed_true() {
    // arrange: spawn space + leaf pane with the zoom marker component
    // act: build_layout_snapshot
    // assert: snapshot.spaces[0].root (as Pane).is_zoomed == true
    todo!("replace with real marker component from Task 4 Step 1")
}
```

Replace the `todo!` with concrete spawn + assert using the component identified in Step 1.

Run: `env -u CEF_PATH cargo test -p vmux_desktop zoomed_pane_reports`
Expected: FAIL (snapshot returns `is_zoomed: false` because the placeholder is hardcoded).

- [ ] **Step 3: Thread the zoom marker through `build_layout_snapshot`**

Add a new query parameter `zoomed: &Query<Entity, With<ZoomMarker>>` (replacing `ZoomMarker` with the actual type) to `build_layout_snapshot` and `build_node`. In `build_node`'s pane branch:

```rust
let is_zoomed = zoomed.contains(leaf_entity);
return LayoutNodeDto::Pane { id: ..., is_zoomed, tabs };
```

Update the caller in `handle_agent_queries` to pass the query.

- [ ] **Step 4: Run all tests**

```bash
env -u CEF_PATH cargo test -p vmux_desktop agent_layout
```

Expected: all pass including the new zoom test.

- [ ] **Step 5: Pre-commit + commit**

```bash
cargo fmt -p vmux_desktop -- --check
env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_desktop
git add crates/vmux_desktop/src/agent_layout.rs crates/vmux_desktop/src/agent_query.rs
git commit -m "feat(agent): plumb pane zoom state into read_layout"
```

---

## Task 5: Reconciler — id parsing + validation (pure functions)

**Files:**
- Create: `crates/vmux_desktop/src/agent_layout/reconcile.rs`
- Modify: `crates/vmux_desktop/src/agent_layout.rs` (declare submodule)

This task builds the validation pass — pure functions over `LayoutSnapshot`, no Bevy world. Easy to TDD.

- [ ] **Step 1: Create the submodule scaffold**

In `crates/vmux_desktop/src/agent_layout.rs`, add at top:

```rust
pub mod reconcile;
```

Create `crates/vmux_desktop/src/agent_layout/reconcile.rs`:

```rust
use std::collections::HashSet;
use vmux_service::protocol::layout::{
    FocusDto, LayoutNodeDto, LayoutSnapshot, NodeKind, SpaceDto, TabDto, parse_id,
};

#[derive(Debug, PartialEq, Eq)]
pub enum ValidationError {
    DuplicateId(String),
    InvalidIdFormat(String),
    WrongKindForPosition { id: String, expected: NodeKind, got: NodeKind },
    NewTabMissingUrl,
    NewTabMissingKind,
    NewPaneMissingTabs,
    NewSpaceMissingName,
    FlexWeightsLengthMismatch { children: usize, weights: usize },
    FocusReferencesUnknownId(String),
}

pub fn validate(snapshot: &LayoutSnapshot) -> Result<(), ValidationError> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut all_ids: HashSet<String> = HashSet::new();

    for space in &snapshot.spaces {
        if let Some(ref id) = space.id {
            let (kind, _) = parse_id(id).map_err(|_| ValidationError::InvalidIdFormat(id.clone()))?;
            if kind != NodeKind::Space {
                return Err(ValidationError::WrongKindForPosition {
                    id: id.clone(),
                    expected: NodeKind::Space,
                    got: kind,
                });
            }
            if !seen.insert(id.clone()) {
                return Err(ValidationError::DuplicateId(id.clone()));
            }
            all_ids.insert(id.clone());
        } else if space.name.is_empty() {
            return Err(ValidationError::NewSpaceMissingName);
        }
        validate_node(&space.root, &mut seen, &mut all_ids)?;
    }

    validate_focus(&snapshot.focused, &all_ids)?;
    Ok(())
}

fn validate_node(
    node: &LayoutNodeDto,
    seen: &mut HashSet<String>,
    all_ids: &mut HashSet<String>,
) -> Result<(), ValidationError> {
    match node {
        LayoutNodeDto::Split { id, flex_weights, children, .. } => {
            if let Some(ref id) = id {
                let (kind, _) = parse_id(id).map_err(|_| ValidationError::InvalidIdFormat(id.clone()))?;
                if kind != NodeKind::Split {
                    return Err(ValidationError::WrongKindForPosition {
                        id: id.clone(),
                        expected: NodeKind::Split,
                        got: kind,
                    });
                }
                if !seen.insert(id.clone()) {
                    return Err(ValidationError::DuplicateId(id.clone()));
                }
                all_ids.insert(id.clone());
            }
            if !flex_weights.is_empty() && flex_weights.len() != children.len() {
                return Err(ValidationError::FlexWeightsLengthMismatch {
                    children: children.len(),
                    weights: flex_weights.len(),
                });
            }
            for child in children {
                validate_node(child, seen, all_ids)?;
            }
            Ok(())
        }
        LayoutNodeDto::Pane { id, tabs, .. } => {
            if let Some(ref id) = id {
                let (kind, _) = parse_id(id).map_err(|_| ValidationError::InvalidIdFormat(id.clone()))?;
                if kind != NodeKind::Pane {
                    return Err(ValidationError::WrongKindForPosition {
                        id: id.clone(),
                        expected: NodeKind::Pane,
                        got: kind,
                    });
                }
                if !seen.insert(id.clone()) {
                    return Err(ValidationError::DuplicateId(id.clone()));
                }
                all_ids.insert(id.clone());
            } else if tabs.is_empty() {
                return Err(ValidationError::NewPaneMissingTabs);
            }
            for tab in tabs {
                validate_tab(tab, seen, all_ids)?;
            }
            Ok(())
        }
    }
}

fn validate_tab(
    tab: &TabDto,
    seen: &mut HashSet<String>,
    all_ids: &mut HashSet<String>,
) -> Result<(), ValidationError> {
    if let Some(ref id) = tab.id {
        let (kind, _) = parse_id(id).map_err(|_| ValidationError::InvalidIdFormat(id.clone()))?;
        if kind != NodeKind::Tab {
            return Err(ValidationError::WrongKindForPosition {
                id: id.clone(),
                expected: NodeKind::Tab,
                got: kind,
            });
        }
        if !seen.insert(id.clone()) {
            return Err(ValidationError::DuplicateId(id.clone()));
        }
        all_ids.insert(id.clone());
    } else {
        if tab.url.is_empty() { return Err(ValidationError::NewTabMissingUrl); }
        if tab.kind.is_empty() { return Err(ValidationError::NewTabMissingKind); }
    }
    Ok(())
}

fn validate_focus(focus: &FocusDto, all_ids: &HashSet<String>) -> Result<(), ValidationError> {
    for id in [&focus.space, &focus.pane, &focus.tab].into_iter().flatten() {
        if !all_ids.contains(id) {
            return Err(ValidationError::FocusReferencesUnknownId(id.clone()));
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Write validation tests**

Append to `reconcile.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use vmux_service::protocol::layout::SplitDirectionDto;

    fn pane(id: Option<&str>, tabs: Vec<TabDto>) -> LayoutNodeDto {
        LayoutNodeDto::Pane { id: id.map(str::to_string), is_zoomed: false, tabs }
    }

    fn split(id: Option<&str>, children: Vec<LayoutNodeDto>, weights: Vec<f32>) -> LayoutNodeDto {
        LayoutNodeDto::Split {
            id: id.map(str::to_string),
            direction: SplitDirectionDto::Row,
            flex_weights: weights,
            children,
        }
    }

    fn snapshot(root: LayoutNodeDto, focus: FocusDto) -> LayoutSnapshot {
        LayoutSnapshot {
            spaces: vec![SpaceDto {
                id: Some("space:1".into()),
                name: "S".into(),
                is_active: true,
                root,
            }],
            focused: focus,
        }
    }

    #[test]
    fn validate_accepts_minimal_existing_layout() {
        let snap = snapshot(
            pane(Some("pane:2"), vec![TabDto { id: Some("tab:3".into()), ..Default::default() }]),
            FocusDto { space: Some("space:1".into()), pane: Some("pane:2".into()), tab: Some("tab:3".into()) },
        );
        assert!(validate(&snap).is_ok());
    }

    #[test]
    fn validate_rejects_duplicate_pane_id() {
        let snap = snapshot(
            split(
                Some("split:1"),
                vec![pane(Some("pane:2"), vec![]), pane(Some("pane:2"), vec![])],
                vec![1.0, 1.0],
            ),
            FocusDto::default(),
        );
        assert!(matches!(validate(&snap), Err(ValidationError::DuplicateId(_))));
    }

    #[test]
    fn validate_rejects_new_pane_without_tabs() {
        let snap = snapshot(pane(None, vec![]), FocusDto::default());
        assert!(matches!(validate(&snap), Err(ValidationError::NewPaneMissingTabs)));
    }

    #[test]
    fn validate_rejects_new_tab_without_url() {
        let snap = snapshot(
            pane(None, vec![TabDto { id: None, url: String::new(), kind: "browser".into(), ..Default::default() }]),
            FocusDto::default(),
        );
        assert!(matches!(validate(&snap), Err(ValidationError::NewTabMissingUrl)));
    }

    #[test]
    fn validate_rejects_new_tab_without_kind() {
        let snap = snapshot(
            pane(None, vec![TabDto { id: None, url: "https://x".into(), kind: String::new(), ..Default::default() }]),
            FocusDto::default(),
        );
        assert!(matches!(validate(&snap), Err(ValidationError::NewTabMissingKind)));
    }

    #[test]
    fn validate_rejects_focus_to_unknown_id() {
        let snap = snapshot(
            pane(Some("pane:2"), vec![TabDto { id: Some("tab:3".into()), ..Default::default() }]),
            FocusDto { space: Some("space:1".into()), pane: Some("pane:99".into()), tab: None },
        );
        assert!(matches!(validate(&snap), Err(ValidationError::FocusReferencesUnknownId(_))));
    }

    #[test]
    fn validate_rejects_wrong_kind_in_position() {
        // tab id sitting where a pane id should be
        let snap = snapshot(pane(Some("tab:2"), vec![]), FocusDto::default());
        assert!(matches!(validate(&snap), Err(ValidationError::WrongKindForPosition { .. })));
    }

    #[test]
    fn validate_rejects_flex_weights_length_mismatch() {
        let snap = snapshot(
            split(
                Some("split:1"),
                vec![pane(Some("pane:2"), vec![TabDto { id: Some("tab:3".into()), ..Default::default() }])],
                vec![1.0, 2.0],
            ),
            FocusDto::default(),
        );
        assert!(matches!(validate(&snap), Err(ValidationError::FlexWeightsLengthMismatch { .. })));
    }
}
```

`TabDto::default()` requires `#[derive(Default)]` on `TabDto`. Add it in Task 1 if missing; otherwise replace `..Default::default()` with explicit fields.

- [ ] **Step 3: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_desktop agent_layout::reconcile
```

Expected: 8 tests pass.

- [ ] **Step 4: Pre-commit + commit**

```bash
cargo fmt -p vmux_desktop -- --check
env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_desktop
git add crates/vmux_desktop/src/agent_layout.rs crates/vmux_desktop/src/agent_layout/reconcile.rs
git commit -m "feat(agent): reconciler validation pass"
```

---

## Task 6: Reconciler — diff plan (pure functions)

**Files:**
- Modify: `crates/vmux_desktop/src/agent_layout/reconcile.rs`

The diff phase classifies each node in the submitted tree as Match / Create, and computes the set of currently-existing ids absent from the new tree as Close.

- [ ] **Step 1: Add the plan types and writer**

Append to `reconcile.rs`:

```rust
use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq)]
pub enum NodeAction {
    Match { existing: u64, desired_kind: NodeKind },
    Create,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DiffPlan {
    pub actions_by_id: HashMap<String, NodeAction>,
    pub closes: Vec<String>,
    pub focus: FocusDto,
}

pub fn plan_diff(
    snapshot: &LayoutSnapshot,
    existing_ids: &HashSet<String>,
) -> Result<DiffPlan, ValidationError> {
    validate(snapshot)?;
    let mut actions_by_id: HashMap<String, NodeAction> = HashMap::new();
    let mut referenced: HashSet<String> = HashSet::new();

    for space in &snapshot.spaces {
        if let Some(ref id) = space.id {
            referenced.insert(id.clone());
            let (_, value) = parse_id(id).expect("validated above");
            actions_by_id.insert(id.clone(), NodeAction::Match { existing: value, desired_kind: NodeKind::Space });
        }
        plan_node(&space.root, &mut actions_by_id, &mut referenced);
    }

    let closes: Vec<String> = existing_ids
        .difference(&referenced)
        .cloned()
        .collect();

    Ok(DiffPlan { actions_by_id, closes, focus: snapshot.focused.clone() })
}

fn plan_node(
    node: &LayoutNodeDto,
    actions_by_id: &mut HashMap<String, NodeAction>,
    referenced: &mut HashSet<String>,
) {
    match node {
        LayoutNodeDto::Split { id, children, .. } => {
            if let Some(ref id) = id {
                referenced.insert(id.clone());
                let (_, value) = parse_id(id).expect("validated");
                actions_by_id.insert(id.clone(), NodeAction::Match { existing: value, desired_kind: NodeKind::Split });
            }
            for c in children { plan_node(c, actions_by_id, referenced); }
        }
        LayoutNodeDto::Pane { id, tabs, .. } => {
            if let Some(ref id) = id {
                referenced.insert(id.clone());
                let (_, value) = parse_id(id).expect("validated");
                actions_by_id.insert(id.clone(), NodeAction::Match { existing: value, desired_kind: NodeKind::Pane });
            }
            for t in tabs {
                if let Some(ref tid) = t.id {
                    referenced.insert(tid.clone());
                    let (_, value) = parse_id(tid).expect("validated");
                    actions_by_id.insert(tid.clone(), NodeAction::Match { existing: value, desired_kind: NodeKind::Tab });
                }
            }
        }
    }
}
```

- [ ] **Step 2: Tests**

Append:

```rust
#[test]
fn plan_marks_existing_ids_as_matches() {
    let snap = snapshot(
        pane(Some("pane:2"), vec![TabDto { id: Some("tab:3".into()), ..Default::default() }]),
        FocusDto { space: Some("space:1".into()), pane: Some("pane:2".into()), tab: Some("tab:3".into()) },
    );
    let existing: HashSet<String> = ["space:1", "pane:2", "tab:3"].into_iter().map(String::from).collect();
    let plan = plan_diff(&snap, &existing).unwrap();
    assert!(plan.actions_by_id.contains_key("pane:2"));
    assert!(plan.actions_by_id.contains_key("tab:3"));
    assert!(plan.closes.is_empty());
}

#[test]
fn plan_lists_unreferenced_ids_for_close() {
    let snap = snapshot(
        pane(Some("pane:2"), vec![TabDto { id: Some("tab:3".into()), ..Default::default() }]),
        FocusDto { space: Some("space:1".into()), pane: Some("pane:2".into()), tab: Some("tab:3".into()) },
    );
    let existing: HashSet<String> = ["space:1", "pane:2", "tab:3", "tab:4"].into_iter().map(String::from).collect();
    let plan = plan_diff(&snap, &existing).unwrap();
    assert_eq!(plan.closes, vec!["tab:4".to_string()]);
}

#[test]
fn plan_treats_id_omission_as_create() {
    let snap = snapshot(
        pane(None, vec![TabDto { id: None, url: "https://x".into(), kind: "browser".into(), ..Default::default() }]),
        FocusDto { space: Some("space:1".into()), pane: None, tab: None },
    );
    let existing: HashSet<String> = ["space:1"].into_iter().map(String::from).collect();
    let plan = plan_diff(&snap, &existing).unwrap();
    // No id keys for the new pane or new tab. Closes is empty since space:1 is referenced.
    assert!(plan.closes.is_empty());
    assert_eq!(plan.actions_by_id.len(), 1); // only space:1
}
```

- [ ] **Step 3: Run + commit**

```bash
env -u CEF_PATH cargo test -p vmux_desktop agent_layout::reconcile
cargo fmt -p vmux_desktop -- --check
env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings
git add crates/vmux_desktop/src/agent_layout/reconcile.rs
git commit -m "feat(agent): reconciler diff planning"
```

---

## Task 7: Reconciler — apply Matches (prop updates only, no structural changes yet)

**Files:**
- Modify: `crates/vmux_desktop/src/agent_layout.rs`
- New private module `crates/vmux_desktop/src/agent_layout/apply.rs`

This task lands the simplest apply path: walk the submitted tree, for every Match node update mutable props (split direction, child flex weights, pane is_zoomed, tab title, space name). No moves, no creates, no closes, no focus changes yet. Validates that the wiring works end-to-end before adding structural changes.

- [ ] **Step 1: Add `apply.rs` skeleton**

Create `crates/vmux_desktop/src/agent_layout/apply.rs`:

```rust
use crate::layout::{
    pane::{Pane, PaneSplit, PaneSplitDirection},
    stack::Stack,
    tab::Tab,
};
use bevy::prelude::*;
use vmux_core::PageMetadata;
use vmux_service::protocol::layout::{LayoutNodeDto, LayoutSnapshot, SpaceDto, SplitDirectionDto};

use super::reconcile::ValidationError;

pub fn apply(world: &mut World, snapshot: &LayoutSnapshot) -> Result<(), ValidationError> {
    super::reconcile::validate(snapshot)?;
    for space in &snapshot.spaces {
        apply_space(world, space);
    }
    Ok(())
}

fn apply_space(world: &mut World, space: &SpaceDto) {
    if let Some(ref id) = space.id {
        if let Ok((_, value)) = vmux_service::protocol::layout::parse_id(id) {
            let entity = Entity::from_bits(value);
            if let Some(mut tab) = world.get_mut::<Tab>(entity) {
                tab.name = space.name.clone();
            }
        }
    }
    apply_node(world, &space.root);
}

fn apply_node(world: &mut World, node: &LayoutNodeDto) {
    match node {
        LayoutNodeDto::Split { id, direction, flex_weights, children } => {
            if let Some(ref id) = id {
                if let Ok((_, value)) = vmux_service::protocol::layout::parse_id(id) {
                    let entity = Entity::from_bits(value);
                    if let Some(mut split) = world.get_mut::<PaneSplit>(entity) {
                        split.direction = match direction {
                            SplitDirectionDto::Row => PaneSplitDirection::Row,
                            SplitDirectionDto::Column => PaneSplitDirection::Column,
                        };
                    }
                }
            }
            // Apply flex weights to children if both provided.
            if !flex_weights.is_empty() && flex_weights.len() == children.len() {
                for (child_dto, weight) in children.iter().zip(flex_weights.iter()) {
                    if let Some(child_entity) = node_entity(child_dto) {
                        if let Some(mut node_cmp) = world.get_mut::<Node>(child_entity) {
                            node_cmp.flex_grow = *weight;
                        }
                    }
                }
            }
            for c in children { apply_node(world, c); }
        }
        LayoutNodeDto::Pane { id, tabs, .. } => {
            // is_zoomed reconciliation deferred to Task 10.
            let _ = id;
            for t in tabs {
                if let Some(ref tid) = t.id {
                    if let Ok((_, value)) = vmux_service::protocol::layout::parse_id(tid) {
                        let entity = Entity::from_bits(value);
                        if !t.title.is_empty() {
                            if let Some(mut page) = world.get_mut::<PageMetadata>(entity) {
                                page.title = t.title.clone();
                            }
                            // Note: title is sourced from page metadata; user-set rename will be persisted in Task 11.
                            let _ = world.get::<Stack>(entity);
                        }
                    }
                }
            }
        }
    }
}

fn node_entity(node: &LayoutNodeDto) -> Option<Entity> {
    match node {
        LayoutNodeDto::Split { id, .. } | LayoutNodeDto::Pane { id, .. } => {
            id.as_ref().and_then(|id| {
                vmux_service::protocol::layout::parse_id(id)
                    .ok()
                    .map(|(_, value)| Entity::from_bits(value))
            })
        }
    }
}
```

In `crates/vmux_desktop/src/agent_layout.rs`, add `pub mod apply;`.

- [ ] **Step 2: Tests for prop-only application**

Append to `agent_layout/apply.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::pane::PaneSplitDirection;
    use vmux_service::protocol::layout::{
        FocusDto, LayoutNodeDto, LayoutSnapshot, NodeKind, SpaceDto, SplitDirectionDto, format_id,
    };

    #[test]
    fn updating_split_direction_changes_component() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let space = app.world_mut().spawn(Tab { name: "S".into() }).id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit { direction: PaneSplitDirection::Row },
                Node::default(),
                ChildOf(space),
            ))
            .id();
        let pane_a = app
            .world_mut()
            .spawn((Pane, Node::default(), ChildOf(split)))
            .id();
        let _pane_b = app
            .world_mut()
            .spawn((Pane, Node::default(), ChildOf(split)))
            .id();
        let _ = pane_a;

        let snap = LayoutSnapshot {
            spaces: vec![SpaceDto {
                id: Some(format_id(NodeKind::Space, space.to_bits())),
                name: "S".into(),
                is_active: true,
                root: LayoutNodeDto::Split {
                    id: Some(format_id(NodeKind::Split, split.to_bits())),
                    direction: SplitDirectionDto::Column,
                    flex_weights: vec![],
                    children: vec![],
                },
            }],
            focused: FocusDto::default(),
        };

        apply(app.world_mut(), &snap).unwrap();
        let updated = app.world().get::<PaneSplit>(split).unwrap();
        assert_eq!(updated.direction, PaneSplitDirection::Column);
    }

    #[test]
    fn updating_flex_weights_writes_node_grow() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let space = app.world_mut().spawn(Tab { name: "S".into() }).id();
        let split = app
            .world_mut()
            .spawn((
                Pane,
                PaneSplit { direction: PaneSplitDirection::Row },
                Node::default(),
                ChildOf(space),
            ))
            .id();
        let pane_a = app
            .world_mut()
            .spawn((Pane, Node { flex_grow: 1.0, ..Default::default() }, ChildOf(split)))
            .id();
        let pane_b = app
            .world_mut()
            .spawn((Pane, Node { flex_grow: 1.0, ..Default::default() }, ChildOf(split)))
            .id();

        let snap = LayoutSnapshot {
            spaces: vec![SpaceDto {
                id: Some(format_id(NodeKind::Space, space.to_bits())),
                name: "S".into(),
                is_active: true,
                root: LayoutNodeDto::Split {
                    id: Some(format_id(NodeKind::Split, split.to_bits())),
                    direction: SplitDirectionDto::Row,
                    flex_weights: vec![3.0, 1.0],
                    children: vec![
                        LayoutNodeDto::Pane {
                            id: Some(format_id(NodeKind::Pane, pane_a.to_bits())),
                            is_zoomed: false,
                            tabs: vec![],
                        },
                        LayoutNodeDto::Pane {
                            id: Some(format_id(NodeKind::Pane, pane_b.to_bits())),
                            is_zoomed: false,
                            tabs: vec![],
                        },
                    ],
                },
            }],
            focused: FocusDto::default(),
        };

        apply(app.world_mut(), &snap).unwrap();
        assert_eq!(app.world().get::<Node>(pane_a).unwrap().flex_grow, 3.0);
        assert_eq!(app.world().get::<Node>(pane_b).unwrap().flex_grow, 1.0);
    }
}
```

- [ ] **Step 3: Run + commit**

```bash
env -u CEF_PATH cargo test -p vmux_desktop agent_layout
cargo fmt -p vmux_desktop -- --check
env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings
git add crates/vmux_desktop/src/agent_layout.rs crates/vmux_desktop/src/agent_layout/apply.rs
git commit -m "feat(agent): reconciler applies prop updates"
```

---

## Task 8: Reconciler — apply structural moves (reparent existing panes/tabs)

**Files:**
- Modify: `crates/vmux_desktop/src/agent_layout/apply.rs`

When the submitted tree places an existing pane/tab id under a different parent or at a different sibling index, reparent the entity. No creates/closes yet.

- [ ] **Step 1: Investigate existing reparent paths**

Run: `bash -c "grep -rn 'set_parent\|insert(ChildOf\|reparent' /Users/junichi.sugiura/Projects/github.com/vmux-ai/vmux/.worktrees/vmx-121/crates/vmux_layout/src --include='*.rs' | head -30"`

Identify the helper (or raw API) used to move a child from one parent to another while preserving order. Note the chosen mechanism in this task's commit message.

- [ ] **Step 2: Failing test — move a pane between splits**

Append a test to `apply.rs` that builds a layout with two splits and a pane under split A, then submits a snapshot placing that pane under split B. Assert the pane is now a child of split B and no longer a child of split A.

```rust
#[test]
fn moves_pane_to_new_parent() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let space = app.world_mut().spawn(Tab { name: "S".into() }).id();
    let split_a = app.world_mut().spawn((Pane, PaneSplit { direction: PaneSplitDirection::Row }, Node::default(), ChildOf(space))).id();
    let split_b = app.world_mut().spawn((Pane, PaneSplit { direction: PaneSplitDirection::Row }, Node::default(), ChildOf(space))).id();
    let moved = app.world_mut().spawn((Pane, Node::default(), ChildOf(split_a))).id();
    let _filler_b = app.world_mut().spawn((Pane, Node::default(), ChildOf(split_b))).id();

    // Snapshot places `moved` under split_b.
    let snap = LayoutSnapshot {
        spaces: vec![SpaceDto {
            id: Some(format_id(NodeKind::Space, space.to_bits())),
            name: "S".into(),
            is_active: true,
            root: LayoutNodeDto::Split {
                id: Some(format_id(NodeKind::Split, split_a.to_bits())),
                direction: SplitDirectionDto::Row,
                flex_weights: vec![],
                children: vec![
                    LayoutNodeDto::Split {
                        id: Some(format_id(NodeKind::Split, split_b.to_bits())),
                        direction: SplitDirectionDto::Row,
                        flex_weights: vec![],
                        children: vec![
                            LayoutNodeDto::Pane {
                                id: Some(format_id(NodeKind::Pane, moved.to_bits())),
                                is_zoomed: false,
                                tabs: vec![],
                            },
                        ],
                    },
                ],
            },
        }],
        focused: FocusDto::default(),
    };

    apply(app.world_mut(), &snap).unwrap();
    let parent = app.world().get::<ChildOf>(moved).map(|p| p.0);
    assert_eq!(parent, Some(split_b));
}
```

Run: expect failure (we haven't implemented reparenting in `apply_node` yet).

- [ ] **Step 3: Extend `apply_node` to reparent**

Add a phase before prop application that walks the submitted tree carrying the "desired parent" context. For each Match node, if its current `ChildOf` parent differs from the desired parent, reparent it using Bevy's `EntityCommands::insert(ChildOf(new_parent))` (or whichever helper Step 1 surfaced). Also reorder siblings: if Bevy 0.18 preserves child order from insertion sequence, re-inserting each child in submitted order achieves the new order.

Suggested API for the new helper:

```rust
fn apply_structure(world: &mut World, parent: Option<Entity>, node: &LayoutNodeDto) {
    if let Some(entity) = node_entity(node) {
        if let Some(parent) = parent {
            world.entity_mut(entity).insert(ChildOf(parent));
        }
    }
    let parent_for_children = node_entity(node).or(parent);
    if let LayoutNodeDto::Split { children, .. } = node {
        for c in children { apply_structure(world, parent_for_children, c); }
    }
}
```

Call `apply_structure` before the existing prop walk in `apply_space`. Pass the space entity as the root parent.

- [ ] **Step 4: Run test, verify it passes**

```bash
env -u CEF_PATH cargo test -p vmux_desktop agent_layout::apply::tests::moves_pane_to_new_parent
```

- [ ] **Step 5: Run all tests, fix regressions**

```bash
env -u CEF_PATH cargo test -p vmux_desktop agent_layout
```

If existing tests now fail (e.g. `updating_flex_weights_writes_node_grow` because the structural pass also touches relationships), reconcile and re-test.

- [ ] **Step 6: Pre-commit + commit**

```bash
cargo fmt -p vmux_desktop -- --check
env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_desktop
git add crates/vmux_desktop/src/agent_layout/apply.rs
git commit -m "feat(agent): reconciler reparents existing nodes"
```

---

## Task 9: Reconciler — apply Closes

**Files:**
- Modify: `crates/vmux_desktop/src/agent_layout/apply.rs`

For each id in `existing - referenced`, dispatch a close. Reuse existing close paths so process shutdown, side-sheet sync, etc. happen correctly.

- [ ] **Step 1: Locate existing close handlers**

Run: `bash -c "grep -rn 'PaneCommand::Close\|TabCommand::Close\|fn close_pane\|fn close_stack' /Users/junichi.sugiura/Projects/github.com/vmux-ai/vmux/.worktrees/vmx-121/crates/vmux_layout/src --include='*.rs' | head -30"`

Decide whether to:
- (a) emit `AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Close))` with focus pre-positioned on the doomed pane, OR
- (b) call the close primitive directly with an explicit entity arg.

(a) is less code but mutates focus; (b) is cleaner but may need a new entry point. Pick (b) and add a public `close_pane_entity(world: &mut World, pane: Entity)` helper next to the existing close handler in `vmux_layout`'s pane module. Similarly `close_stack_entity` and `close_space_entity` if they don't already exist.

- [ ] **Step 2: Failing test — closing a pane removes it from the world**

Append:

```rust
#[test]
fn omitting_pane_from_snapshot_closes_it() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let space = app.world_mut().spawn(Tab { name: "S".into() }).id();
    let split = app.world_mut().spawn((Pane, PaneSplit { direction: PaneSplitDirection::Row }, Node::default(), ChildOf(space))).id();
    let keep = app.world_mut().spawn((Pane, Node::default(), ChildOf(split))).id();
    let drop_me = app.world_mut().spawn((Pane, Node::default(), ChildOf(split))).id();

    let snap = LayoutSnapshot {
        spaces: vec![SpaceDto {
            id: Some(format_id(NodeKind::Space, space.to_bits())),
            name: "S".into(),
            is_active: true,
            root: LayoutNodeDto::Split {
                id: Some(format_id(NodeKind::Split, split.to_bits())),
                direction: SplitDirectionDto::Row,
                flex_weights: vec![],
                children: vec![
                    LayoutNodeDto::Pane {
                        id: Some(format_id(NodeKind::Pane, keep.to_bits())),
                        is_zoomed: false,
                        tabs: vec![],
                    },
                ],
            },
        }],
        focused: FocusDto::default(),
    };

    // Pretend we know the existing-ids universe (in real apply this comes from a world walk before plan_diff).
    let existing: std::collections::HashSet<String> = [
        format_id(NodeKind::Space, space.to_bits()),
        format_id(NodeKind::Split, split.to_bits()),
        format_id(NodeKind::Pane, keep.to_bits()),
        format_id(NodeKind::Pane, drop_me.to_bits()),
    ].into_iter().collect();

    apply_with_existing(app.world_mut(), &snap, &existing).unwrap();
    assert!(app.world().get_entity(drop_me).is_err(), "drop_me should be despawned");
    assert!(app.world().get_entity(keep).is_ok(), "keep should survive");
}
```

This test requires a new entry point `apply_with_existing` that takes the existing-ids set. In production `apply` will compute the existing set by walking the world; the helper signature exposes the seam for tests.

- [ ] **Step 3: Implement `apply_with_existing`**

Add to `apply.rs`:

```rust
pub fn apply_with_existing(
    world: &mut World,
    snapshot: &LayoutSnapshot,
    existing: &std::collections::HashSet<String>,
) -> Result<(), ValidationError> {
    let plan = super::reconcile::plan_diff(snapshot, existing)?;
    // structural reparents:
    for space in &snapshot.spaces {
        apply_structure(world, None, &LayoutNodeDto::Pane { id: space.id.clone(), is_zoomed: false, tabs: vec![] });
        apply_structure(world, node_entity(&LayoutNodeDto::Pane { id: space.id.clone(), is_zoomed: false, tabs: vec![] }), &space.root);
    }
    // prop updates:
    for space in &snapshot.spaces { apply_space(world, space); }
    // closes:
    for id in &plan.closes { apply_close(world, id); }
    Ok(())
}

fn apply_close(world: &mut World, id: &str) {
    use vmux_service::protocol::layout::{NodeKind, parse_id};
    let Ok((kind, value)) = parse_id(id) else { return };
    let entity = Entity::from_bits(value);
    match kind {
        NodeKind::Pane | NodeKind::Split | NodeKind::Tab | NodeKind::Space => {
            // Recursive despawn handles descendants. Production code should instead call
            // crate-local helpers in vmux_layout to trigger process shutdown side effects.
            if let Ok(entity_ref) = world.get_entity_mut(entity) {
                entity_ref.despawn();
            }
        }
    }
}

pub fn apply(world: &mut World, snapshot: &LayoutSnapshot) -> Result<(), ValidationError> {
    let existing = collect_existing_ids(world);
    apply_with_existing(world, snapshot, &existing)
}

fn collect_existing_ids(world: &mut World) -> std::collections::HashSet<String> {
    use vmux_service::protocol::layout::{NodeKind, format_id};
    let mut out = std::collections::HashSet::new();
    let mut q_space = world.query_filtered::<Entity, With<Tab>>();
    for e in q_space.iter(world) { out.insert(format_id(NodeKind::Space, e.to_bits())); }
    let mut q_split = world.query_filtered::<Entity, (With<Pane>, With<PaneSplit>)>();
    for e in q_split.iter(world) { out.insert(format_id(NodeKind::Split, e.to_bits())); }
    let mut q_pane = world.query_filtered::<Entity, (With<Pane>, Without<PaneSplit>)>();
    for e in q_pane.iter(world) { out.insert(format_id(NodeKind::Pane, e.to_bits())); }
    let mut q_tab = world.query_filtered::<Entity, With<Stack>>();
    for e in q_tab.iter(world) { out.insert(format_id(NodeKind::Tab, e.to_bits())); }
    out
}
```

For now `apply_close` is a brute-force despawn. **Before shipping**, replace each match arm with the helper located in Step 1 (e.g., `vmux_layout::pane::close_pane_entity(world, entity)`) so process shutdown and side-sheet sync happen. If you discover that no helper exists, leave a TODO in code AND add a follow-up task to this plan; do not silently drop the integration.

- [ ] **Step 4: Run test**

```bash
env -u CEF_PATH cargo test -p vmux_desktop agent_layout::apply::tests::omitting_pane_from_snapshot_closes_it
```

- [ ] **Step 5: Pre-commit + commit**

```bash
cargo fmt -p vmux_desktop -- --check
env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_desktop
git add crates/vmux_desktop/src/agent_layout/apply.rs
git commit -m "feat(agent): reconciler applies closes"
```

---

## Task 10: Reconciler — apply Creates (new tab via LayoutSpawnRequest; new pane; new split; new space)

**Files:**
- Modify: `crates/vmux_desktop/src/agent_layout/apply.rs`

For each node with `id: None`, materialize a new ECS entity and remember the mapping so the apply pass can wire children correctly.

- [ ] **Step 1: Locate creation entry points**

- New tab — `LayoutSpawnRequest` (used today by `split_and_navigate` and `new_terminal_tab`). Run: `bash -c "grep -rn 'LayoutSpawnRequest' /Users/junichi.sugiura/Projects/github.com/vmux-ai/vmux/.worktrees/vmx-121/crates --include='*.rs' | head -20"`
- New pane / new split / new space — locate the spawn paths used by `PaneCommand::SplitH/SplitV` (probably in `vmux_layout/src/pane.rs`) and by `StackCommand::New` (`vmux_layout/src/stack.rs`). Identify functions or message types you can call programmatically.

- [ ] **Step 2: Failing test — submitting a tree with a new tab creates it**

```rust
#[test]
fn submitting_new_tab_id_none_creates_stack_entity() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    // ... vmux_layout::LayoutPlugin probably needs to run for spawn paths to work; document if unsuitable for MinimalPlugins
    let space = app.world_mut().spawn(Tab { name: "S".into() }).id();
    let pane = app.world_mut().spawn((Pane, Node::default(), ChildOf(space))).id();

    let snap = LayoutSnapshot {
        spaces: vec![SpaceDto {
            id: Some(format_id(NodeKind::Space, space.to_bits())),
            name: "S".into(),
            is_active: true,
            root: LayoutNodeDto::Pane {
                id: Some(format_id(NodeKind::Pane, pane.to_bits())),
                is_zoomed: false,
                tabs: vec![TabDto {
                    id: None,
                    url: "https://example.com".into(),
                    kind: "browser".into(),
                    ..Default::default()
                }],
            },
        }],
        focused: FocusDto::default(),
    };

    apply(app.world_mut(), &snap).unwrap();
    let stack_count = app.world_mut().query_filtered::<Entity, With<Stack>>().iter(app.world()).count();
    assert_eq!(stack_count, 1, "one new Stack should exist");
}
```

The Bevy `MinimalPlugins` test rig may not be sufficient for full spawn flows that touch CEF / webviews. In that case, gate the test behind `#[cfg_attr(not(feature = "integration-spawn"), ignore)]` and add an issue note in the commit, OR refactor the spawn helper used by `LayoutSpawnRequest` to a pure ECS function that doesn't require a webview backend (preferred — keeps the path testable).

- [ ] **Step 3: Implement Creates in `apply_with_existing`**

Add a pre-pass that walks the snapshot top-down. For each id-less node:
- Tab: emit a `LayoutSpawnRequest` (or call the underlying spawn function directly) with the desired pane entity (resolved from the parent in the submitted tree), `url`, and `kind`.
- Pane: spawn a `(Pane, Node::default(), ChildOf(parent))` entity. Its children (tabs) are handled by the next recursion.
- Split: spawn `(Pane, PaneSplit { direction }, Node::default(), ChildOf(parent))`.
- Space: spawn `Tab { name }`. Subsequent children attach via the `apply_structure` pass.

Maintain a `HashMap<*const LayoutNodeDto, Entity>` keyed by the snapshot pointer so subsequent passes know which entity each new node owns.

- [ ] **Step 4: Run tests**

```bash
env -u CEF_PATH cargo test -p vmux_desktop agent_layout
```

- [ ] **Step 5: Pre-commit + commit**

```bash
cargo fmt -p vmux_desktop -- --check
env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_desktop
git add crates/vmux_desktop/src/agent_layout/apply.rs
git commit -m "feat(agent): reconciler creates new nodes"
```

---

## Task 11: Reconciler — apply tab title (rename) and focus

**Files:**
- Modify: `crates/vmux_desktop/src/agent_layout/apply.rs`

The Match pass already touches `PageMetadata.title`, but that's overridden by browser navigation. Real rename needs whatever component vmux uses to override page-derived titles.

- [ ] **Step 1: Locate the rename path**

Run: `bash -c "grep -rn 'TabCommand::Rename\|user_title\|custom_title\|rename' /Users/junichi.sugiura/Projects/github.com/vmux-ai/vmux/.worktrees/vmx-121/crates --include='*.rs' | head -30"`

If a user-title-override component exists, write to it. If not, document the gap (file a follow-up issue) and skip writing `title` for matched tabs — declare title read-only via update_layout in the spec for v1.

- [ ] **Step 2: Apply focus**

Add focus handling at the very end of `apply_with_existing`. Resolve each non-None id in `snapshot.focused` to its entity (use the existing `parse_id` + `Entity::from_bits`), then write to `FocusedStack`:

```rust
fn apply_focus(world: &mut World, focus: &FocusDto) {
    let Some(mut focused) = world.get_resource_mut::<FocusedStack>() else { return };
    focused.tab = focus.space.as_deref().and_then(|id| parse_id(id).ok()).map(|(_, v)| Entity::from_bits(v));
    focused.pane = focus.pane.as_deref().and_then(|id| parse_id(id).ok()).map(|(_, v)| Entity::from_bits(v));
    focused.stack = focus.tab.as_deref().and_then(|id| parse_id(id).ok()).map(|(_, v)| Entity::from_bits(v));
}
```

(Note the legacy naming: `FocusedStack.tab` = space, `.pane` = pane, `.stack` = tab.)

Real focus changes also need the per-pane "last-activated stack" updated for `next/prev`-style follow-up behavior. For v1, write `FocusedStack` directly and accept that subsequent input may snap focus back via existing systems on the next frame. If smoke tests catch this, add a follow-up; do not block landing.

- [ ] **Step 3: Tests**

Add a test where the submitted snapshot changes `focused`, and assert `FocusedStack` matches afterward.

- [ ] **Step 4: Run + commit**

```bash
env -u CEF_PATH cargo test -p vmux_desktop agent_layout
cargo fmt -p vmux_desktop -- --check
env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings
git add crates/vmux_desktop/src/agent_layout/apply.rs
git commit -m "feat(agent): reconciler applies rename and focus"
```

---

## Task 12: Wire UpdateLayout into AgentCommand handler

**Files:**
- Modify: `crates/vmux_desktop/src/agent.rs` (`handle_agent_commands`)

- [ ] **Step 1: Add the `UpdateLayout` arm**

Inside `handle_agent_commands`, after the existing `ServiceAgentCommand::*` arms, add:

```rust
ServiceAgentCommand::UpdateLayout { layout } => {
    // We need exclusive world access here; the surrounding function probably uses query params.
    // If so, route this command through an event that is handled by an exclusive system instead.
    commands.queue(move |world: &mut World| {
        let result = crate::agent_layout::apply::apply(world, &layout);
        // Send AgentCommandResponse. The exact send path depends on whether ServiceClient
        // is available inside commands.queue; if not, store result on a Resource and have a
        // follow-up system flush it. See the existing `BrowserNavigate` handler for the pattern.
        if let Err(err) = result {
            warn!("update_layout validation failed: {err:?}");
        }
    });
}
```

Adapt the response-send path to match the existing pattern in this file (the existing handlers send `ClientMessage::AgentCommandResponse` via `service.0.send`). If the closure captures `service` correctly, send `AgentCommandResult::Layout(<new snapshot>)` on success or `AgentCommandResult::Error(<msg>)` on validation failure. To compute the new snapshot, call `build_layout_snapshot` from inside the closure.

- [ ] **Step 2: Compile + run all tests**

```bash
env -u CEF_PATH cargo test -p vmux_desktop
```

Expected: no regressions. The full reconciler exercise lands in Task 14 (smoke tests); this step just confirms wiring.

- [ ] **Step 3: Pre-commit + commit**

```bash
cargo fmt -p vmux_desktop -- --check
env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings
git add crates/vmux_desktop/src/agent.rs
git commit -m "feat(agent): dispatch UpdateLayout to reconciler"
```

---

## Task 13: MCP — hand-built `read_layout` and `update_layout` tools

**Files:**
- Modify: `crates/vmux_mcp/src/tools.rs`

- [ ] **Step 1: Delete `McpQueryTool`**

Remove the entire `McpQueryTool` enum and `impl` block from `tools.rs`. Also remove its entries from `tool_definitions()` and the routing branch in `dispatch_from_tool_call()`.

- [ ] **Step 2: Add hand-built ToolDefinition entries**

In `tool_definitions()`, after the macro-derived chain, push:

```rust
fn read_layout_definition() -> ToolDefinition {
    ToolDefinition {
        name: "read_layout".into(),
        description: "Return the full vmux layout: spaces, recursive pane tree, focused triple. \
                      Terminal tabs appear as tabs with kind=\"terminal\"; browser tabs use kind=\"browser\".".into(),
        input_schema: serde_json::json!({"type": "object", "properties": {}, "additionalProperties": false}),
    }
}

fn update_layout_definition() -> ToolDefinition {
    ToolDefinition {
        name: "update_layout".into(),
        description: "Submit the desired layout tree. Vmux diffs against current state and reconciles \
                      atomically by id: omit `id` to create; omit a node entirely to close it; reorder \
                      `children` to swap/move; mutate `direction`/`flex_weights`/`is_zoomed`/`title` in place.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["spaces", "focused"],
            "$defs": {
                "Space": {
                    "type": "object",
                    "required": ["name", "root"],
                    "properties": {
                        "id": {"type": "string", "description": "space:<id>; omit to create"},
                        "name": {"type": "string"},
                        "is_active": {"type": "boolean"},
                        "root": {"$ref": "#/$defs/LayoutNode"}
                    }
                },
                "LayoutNode": {
                    "oneOf": [
                        {
                            "type": "object",
                            "required": ["kind", "direction", "children"],
                            "properties": {
                                "kind": {"const": "split"},
                                "id": {"type": "string", "description": "split:<id>; omit to create"},
                                "direction": {"enum": ["row", "column"]},
                                "flex_weights": {"type": "array", "items": {"type": "number"}},
                                "children": {"type": "array", "items": {"$ref": "#/$defs/LayoutNode"}}
                            }
                        },
                        {
                            "type": "object",
                            "required": ["kind"],
                            "properties": {
                                "kind": {"const": "pane"},
                                "id": {"type": "string", "description": "pane:<id>; omit to create"},
                                "is_zoomed": {"type": "boolean"},
                                "tabs": {"type": "array", "items": {"$ref": "#/$defs/Tab"}}
                            }
                        }
                    ]
                },
                "Tab": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string", "description": "tab:<id>; omit to create"},
                        "title": {"type": "string"},
                        "url": {"type": "string", "description": "Required when id is omitted"},
                        "kind": {"type": "string", "enum": ["browser", "terminal"], "description": "Required when id is omitted"},
                        "is_loading": {"type": "boolean"},
                        "favicon_url": {"type": "string"}
                    }
                }
            },
            "properties": {
                "spaces": {"type": "array", "items": {"$ref": "#/$defs/Space"}},
                "focused": {
                    "type": "object",
                    "properties": {
                        "space": {"type": "string"},
                        "pane": {"type": "string"},
                        "tab": {"type": "string"}
                    }
                }
            }
        }),
    }
}
```

In `tool_definitions()`:

```rust
let mut defs: Vec<ToolDefinition> = AppCommand::mcp_tool_entries()
    .into_iter()
    .chain(McpParamTool::mcp_tool_entries())
    .map(|(name, description, schema)| ToolDefinition {
        name: name.to_string(),
        description: description.to_string(),
        input_schema: schema,
    })
    .collect();
defs.push(read_layout_definition());
defs.push(update_layout_definition());
defs
```

- [ ] **Step 3: Route both in `dispatch_from_tool_call`**

```rust
pub fn dispatch_from_tool_call(name: &str, arguments: Value) -> Result<DispatchTarget, String> {
    if name == "read_layout" {
        return Ok(DispatchTarget::Query(AgentQuery::ReadLayout));
    }
    if name == "update_layout" {
        let layout: vmux_service::protocol::layout::LayoutSnapshot = serde_json::from_value(arguments)
            .map_err(|e| format!("update_layout: invalid layout payload: {e}"))?;
        return Ok(DispatchTarget::Command(AgentCommand::UpdateLayout { layout }));
    }
    if let Some(parsed) = McpParamTool::from_mcp_call(name, arguments) {
        return parsed
            .and_then(McpParamTool::to_agent_command)
            .map(DispatchTarget::Command);
    }
    if AppCommand::from_mcp_id(name).is_some() {
        return Ok(DispatchTarget::Command(AgentCommand::AppCommand { id: name.to_string() }));
    }
    Err(format!("unknown tool: {name}"))
}
```

(Removes the McpQueryTool branch entirely.)

- [ ] **Step 4: Update unit tests**

In `tools.rs`'s test module:
- Remove `tool_list_includes_query_tools` and `mcp_query_tool_entries_includes_all_query_tools`.
- Add `tool_list_includes_read_and_update_layout` asserting both names present.
- Add `dispatch_read_layout_routes_to_query` and `dispatch_update_layout_parses_payload`.

```rust
#[test]
fn tool_list_includes_read_and_update_layout() {
    let names = tool_names();
    assert!(names.contains(&"read_layout".to_string()));
    assert!(names.contains(&"update_layout".to_string()));
}

#[test]
fn dispatch_read_layout_routes_to_query() {
    let target = dispatch_from_tool_call("read_layout", serde_json::json!({})).unwrap();
    assert!(matches!(target, DispatchTarget::Query(AgentQuery::ReadLayout)));
}

#[test]
fn dispatch_update_layout_parses_payload() {
    let payload = serde_json::json!({
        "spaces": [{
            "id": "space:1",
            "name": "Work",
            "is_active": true,
            "root": { "kind": "pane", "id": "pane:2", "tabs": [{ "id": "tab:3" }] }
        }],
        "focused": { "space": "space:1", "pane": "pane:2", "tab": "tab:3" }
    });
    let target = dispatch_from_tool_call("update_layout", payload).unwrap();
    assert!(matches!(target, DispatchTarget::Command(AgentCommand::UpdateLayout { .. })));
}

#[test]
fn dispatch_update_layout_rejects_malformed_payload() {
    let payload = serde_json::json!({ "not_a_layout": true });
    assert!(dispatch_from_tool_call("update_layout", payload).is_err());
}
```

- [ ] **Step 5: Run + commit**

```bash
env -u CEF_PATH cargo test -p vmux_mcp
cargo fmt -p vmux_mcp -- --check
env -u CEF_PATH cargo clippy -p vmux_mcp --all-targets -- -D warnings
git add crates/vmux_mcp/src/tools.rs
git commit -m "feat(mcp): add read_layout and update_layout tools"
```

---

## Task 14: Strip McpTool derive from layout commands

**Files:**
- Modify: `crates/vmux_command/src/command.rs`

- [ ] **Step 1: Edit the derive lists**

For each of these enums in `command.rs`, remove `McpTool` from the `#[derive(...)]` list (keep all other derives intact): `LayoutCommand`, `PaneCommand`, `TabCommand`, `StackCommand`, `WindowCommand`, `ZenCommand`, `SpaceCommand`.

`AppCommand` itself keeps `McpTool` so the remaining non-layout variants (`Scene`, `Terminal`, `Browser`, `Service`) still expose MCP tools. The `OsSubMenuGroup` derive on `LayoutCommand` is what previously routed `AppCommand::Layout(...)` into `mcp_tool_entries()`; removing `McpTool` from `LayoutCommand` means the macro's recursion skips that subtree.

- [ ] **Step 2: Update existing tests**

In `command.rs`'s `tests` module:

- `mcp_lookup_resolves_every_command_id` — should still pass; the macro emits fewer entries. Adjust the body if any assertion explicitly references a layout id (e.g., `split_v`, `close_tab`). Replace those with non-layout ids (e.g., `terminal_clear`, `browser_reload`) that are still exposed.
- `layout_menu_id_resolves_through_nested_chain` — depends on whether the test is asserting `from_menu_id` (still works, menus untouched) or `from_mcp_id` (layout ids return `None` now). Adjust accordingly.

Add a new test:

```rust
#[test]
fn layout_command_ids_no_longer_exposed_via_mcp() {
    for id in ["split_v", "split_h", "close_pane", "select_pane_left", "new_tab", "tab_select_1", "stack_new"] {
        assert!(
            AppCommand::from_mcp_id(id).is_none(),
            "{id} should not be exposed via MCP after the derive strip"
        );
    }
}

#[test]
fn non_layout_command_ids_still_exposed_via_mcp() {
    for id in ["terminal_clear", "browser_reload"] {
        assert!(
            AppCommand::from_mcp_id(id).is_some(),
            "{id} should still be exposed via MCP"
        );
    }
}
```

- [ ] **Step 3: Run + commit**

```bash
env -u CEF_PATH cargo test -p vmux_command
cargo fmt -p vmux_command -- --check
env -u CEF_PATH cargo clippy -p vmux_command --all-targets -- -D warnings
git add crates/vmux_command/src/command.rs
git commit -m "refactor(command): strip McpTool derive from layout commands"
```

If `vmux_mcp` or other downstream crates fail to compile because `from_mcp_id` is called on a removed layout id, that's the next task.

---

## Task 15: Update `mcp_smoke.rs` integration tests

**Files:**
- Modify: `crates/vmux_cli/tests/mcp_smoke.rs`

- [ ] **Step 1: Add smoke coverage for new tools**

Append two tests:

```rust
#[test]
fn mcp_tools_list_includes_layout_tools() {
    let stdin = "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n";
    let mut cmd = Command::cargo_bin("vmux").unwrap();
    cmd.arg("mcp")
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(contains("\"read_layout\""))
        .stdout(contains("\"update_layout\""));
}

#[test]
fn mcp_tools_list_no_longer_includes_legacy_layout_tools() {
    let stdin = "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n";
    let mut cmd = Command::cargo_bin("vmux").unwrap();
    cmd.arg("mcp")
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(contains("\"read_layout\""));
    // Re-run separately to assert absences (predicates::str::contains has no `not` here; spawn a fresh cmd).
    let stdin2 = "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/list\"}\n";
    let mut cmd2 = Command::cargo_bin("vmux").unwrap();
    let out = cmd2.arg("mcp").write_stdin(stdin2).assert().success().get_output().stdout.clone();
    let s = String::from_utf8_lossy(&out);
    for legacy in ["\"split_v\"", "\"split_h\"", "\"get_state\"", "\"list_tabs\"", "\"list_terminals\""] {
        assert!(!s.contains(legacy), "tool list still contains {legacy}: {s}");
    }
}
```

- [ ] **Step 2: Run + commit**

```bash
env -u CEF_PATH cargo test -p vmux_cli --test mcp_smoke
cargo fmt -p vmux_cli -- --check
env -u CEF_PATH cargo clippy -p vmux_cli --all-targets -- -D warnings
git add crates/vmux_cli/tests/mcp_smoke.rs
git commit -m "test(cli): mcp smoke covers new layout tools"
```

---

## Task 16: End-to-end smoke — round-trip update_layout via the agent path

**Files:**
- Either a new test file in `crates/vmux_desktop/tests/` if integration testing is wired up there, OR a section in an existing integration test if one exists.

This task validates that a real `read_layout → mutate → update_layout → read_layout` cycle produces the expected tree, end-to-end through the message bus.

- [ ] **Step 1: Investigate existing integration test scaffolding**

Run: `bash -c "find /Users/junichi.sugiura/Projects/github.com/vmux-ai/vmux/.worktrees/vmx-121 -name 'integration*' -type d -not -path '*/target/*'; find /Users/junichi.sugiura/Projects/github.com/vmux-ai/vmux/.worktrees/vmx-121/crates/vmux_desktop -name 'tests' -type d"`

If integration scaffolding exists, add a test driving the full cycle. If not, skip this task and rely on the unit tests from Tasks 5–11 — note this gap in the commit message for Task 15.

- [ ] **Step 2: If scaffolding present, write the test**

Pseudo-structure:

1. Boot a minimal desktop app with `LayoutPlugin`, `CommandPlugin`, and the agent plumbing registered.
2. Issue `AgentQuery::ReadLayout` via the in-process message bus, capture the snapshot.
3. Mutate the snapshot (e.g., reorder root.children).
4. Issue `AgentCommand::UpdateLayout { layout: mutated }`.
5. Re-issue `AgentQuery::ReadLayout`, assert the new snapshot matches the mutated one (modulo newly-assigned ids).

- [ ] **Step 3: Run + commit**

```bash
env -u CEF_PATH cargo test -p vmux_desktop
cargo fmt -p vmux_desktop -- --check
env -u CEF_PATH cargo clippy -p vmux_desktop --all-targets -- -D warnings
git add <test file>
git commit -m "test(agent): end-to-end update_layout round-trip"
```

---

## Task 17: Final cleanup + push

- [ ] **Step 1: Run the full changed-crate sweep one last time**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
echo "Changed crates: $PKGS"
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

If any check fails, fix it and commit before proceeding.

- [ ] **Step 2: Delete this plan file**

Per AGENTS.md: "Delete the plan file once the plan is fully implemented."

```bash
git rm docs/plans/2026-05-16-general-layout-mcp-api.md
git commit -m "chore: remove implemented plan"
```

- [ ] **Step 3: Push and open PR**

```bash
git push --set-upstream origin vmx-121-layout-mcp-tree-api
linear issue pr  # opens PR linked to VMX-121
```

Confirm the PR title is short (under 70 chars); body should mention the spec path and the headline reduction (~45 tools → 2).

---

## Self-Review Notes (carry over into execution)

- The reconciler in Tasks 7–11 is incrementally tested. If a later task discovers a structural mistake in the earlier passes (for example, that prop updates must happen BEFORE structure changes because of `Node::flex_grow` reset on reparent), revise the earlier task's commit rather than papering over it.
- Tasks 9 and 10 reference helpers (`close_pane_entity`, `LayoutSpawnRequest` callable form) that may not exist yet in `vmux_layout`. If you have to add them, do that inside the same task's commit and call it out in the commit message — do not silently expand scope into a separate crate without leaving a trail.
- Bevy 0.18 `ChildOf` and child-order semantics changed from previous versions; verify any reparenting helper actually preserves submitted order. The reorder test in Task 8 will catch obvious failures.
- The MCP schema `oneOf` in Task 13 uses `$ref` for recursion. If your MCP client doesn't follow `$ref` (some do, some don't), inline the recursive shape to one level of nesting — flag that as a follow-up rather than blocking this work.
- Stale-write protection (version tokens) is deferred. If, during implementation, you spot a place where a destructive close could happen because of a stale read, leave a code comment naming the hazard so the follow-up has a starting point.
