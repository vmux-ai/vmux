use std::collections::HashSet;

use bevy::prelude::*;

use crate::protocol::{LayoutNode, LayoutSnapshot};

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutRenderer {
    Cef,
    #[default]
    Native,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(pub String);

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutView {
    pub tabs: Vec<TabView>,
    pub address: String,
    pub panes: Vec<PaneView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabView {
    pub id: NodeId,
    pub name: String,
    pub title: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaneView {
    pub id: NodeId,
    pub is_active: bool,
    pub stacks: Vec<StackView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StackView {
    pub id: NodeId,
    pub title: String,
    pub is_active: bool,
}

impl LayoutView {
    pub fn from_snapshot(snapshot: &LayoutSnapshot) -> Self {
        let tabs = snapshot
            .tabs
            .iter()
            .filter_map(|t| {
                let id = t.id.clone()?;
                Some(TabView {
                    id: NodeId(id),
                    name: t.name.clone(),
                    title: tab_display_title(&t.name, &t.root),
                    is_active: t.is_active,
                })
            })
            .collect();
        let active_pane = snapshot.focused.pane.as_deref();
        let active_stack = snapshot.focused.stack.as_deref();
        let panes = snapshot
            .tabs
            .iter()
            .find(|t| t.is_active)
            .map(|t| collect_panes(&t.root, active_pane, active_stack))
            .unwrap_or_default();
        LayoutView {
            tabs,
            address: focused_stack_url(snapshot),
            panes,
        }
    }
}

fn collect_panes(
    root: &LayoutNode,
    active_pane: Option<&str>,
    active_stack: Option<&str>,
) -> Vec<PaneView> {
    let mut out = Vec::new();
    collect_panes_into(root, active_pane, active_stack, &mut out);
    out
}

fn collect_panes_into(
    node: &LayoutNode,
    active_pane: Option<&str>,
    active_stack: Option<&str>,
    out: &mut Vec<PaneView>,
) {
    match node {
        LayoutNode::Pane { id, stacks, .. } => {
            let Some(pane_id) = id.clone() else {
                return;
            };
            let pages = stacks
                .iter()
                .filter(|s| !(s.url.trim().is_empty() && s.title.trim() == "New Stack"))
                .filter_map(|s| {
                    let sid = s.id.clone()?;
                    Some(StackView {
                        id: NodeId(sid.clone()),
                        title: stack_display_title(s),
                        is_active: active_stack == Some(sid.as_str()),
                    })
                })
                .collect();
            out.push(PaneView {
                is_active: active_pane == Some(pane_id.as_str()),
                id: NodeId(pane_id),
                stacks: pages,
            });
        }
        LayoutNode::Split { children, .. } => {
            for c in children {
                collect_panes_into(c, active_pane, active_stack, out);
            }
        }
    }
}

fn stack_display_title(s: &crate::protocol::Stack) -> String {
    let title = s.title.trim();
    if !title.is_empty() {
        return title.to_string();
    }
    let url = s.url.trim();
    if !url.is_empty() {
        return url.to_string();
    }
    "Stack".to_string()
}

fn focused_stack_url(snapshot: &LayoutSnapshot) -> String {
    let Some(stack_id) = snapshot.focused.stack.as_deref() else {
        return String::new();
    };
    snapshot
        .tabs
        .iter()
        .find_map(|t| find_stack_url(&t.root, stack_id))
        .unwrap_or_default()
}

fn find_stack_url(node: &LayoutNode, stack_id: &str) -> Option<String> {
    match node {
        LayoutNode::Pane { stacks, .. } => stacks
            .iter()
            .find(|s| s.id.as_deref() == Some(stack_id))
            .map(|s| s.url.clone()),
        LayoutNode::Split { children, .. } => {
            children.iter().find_map(|c| find_stack_url(c, stack_id))
        }
    }
}

fn tab_display_title(name: &str, root: &LayoutNode) -> String {
    let name = name.trim();
    if !name.is_empty() {
        return name.to_string();
    }
    first_stack_title(root).unwrap_or_else(|| "New Tab".to_string())
}

fn first_stack_title(node: &LayoutNode) -> Option<String> {
    match node {
        LayoutNode::Pane { stacks, .. } => stacks.iter().find_map(|s| {
            let t = s.title.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }),
        LayoutNode::Split { children, .. } => children.iter().find_map(first_stack_title),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ViewOp {
    CreateTab {
        id: NodeId,
        name: String,
        is_active: bool,
    },
    UpdateTab {
        id: NodeId,
        name: String,
        is_active: bool,
    },
    RemoveTab {
        id: NodeId,
    },
    SetTabOrder {
        ids: Vec<NodeId>,
    },
}

pub fn diff_tabs(old: &LayoutView, new: &LayoutView) -> Vec<ViewOp> {
    let mut ops = Vec::new();
    let new_ids: HashSet<&NodeId> = new.tabs.iter().map(|t| &t.id).collect();

    for t in &old.tabs {
        if !new_ids.contains(&t.id) {
            ops.push(ViewOp::RemoveTab { id: t.id.clone() });
        }
    }
    for t in &new.tabs {
        match old.tabs.iter().find(|o| o.id == t.id) {
            None => ops.push(ViewOp::CreateTab {
                id: t.id.clone(),
                name: t.name.clone(),
                is_active: t.is_active,
            }),
            Some(o) => {
                if o.name != t.name || o.is_active != t.is_active {
                    ops.push(ViewOp::UpdateTab {
                        id: t.id.clone(),
                        name: t.name.clone(),
                        is_active: t.is_active,
                    });
                }
            }
        }
    }
    let old_order: Vec<&NodeId> = old.tabs.iter().map(|t| &t.id).collect();
    let new_order: Vec<&NodeId> = new.tabs.iter().map(|t| &t.id).collect();
    if old_order != new_order {
        ops.push(ViewOp::SetTabOrder {
            ids: new.tabs.iter().map(|t| t.id.clone()).collect(),
        });
    }
    ops
}

#[derive(Resource, Default)]
pub struct CurrentLayoutView(pub LayoutView);

#[derive(Resource, Default)]
pub struct LastRenderedView(pub Option<LayoutView>);

#[derive(Resource, Default)]
pub struct RecordedViewOps(pub Vec<ViewOp>);

pub fn diff_into_ops(
    current: Res<CurrentLayoutView>,
    mut last: ResMut<LastRenderedView>,
    mut recorded: ResMut<RecordedViewOps>,
) {
    if !current.is_changed() {
        return;
    }
    let empty = LayoutView::default();
    let prev = last.0.as_ref().unwrap_or(&empty);
    let ops = diff_tabs(prev, &current.0);
    if ops.is_empty() {
        return;
    }
    recorded.0 = ops;
    last.0 = Some(current.0.clone());
}

fn renderer_is_native(renderer: Res<LayoutRenderer>) -> bool {
    *renderer == LayoutRenderer::Native
}

pub fn update_current_layout_view(
    tabs_q: Query<(Entity, &crate::tab::Tab, Option<&Children>)>,
    splits_q: Query<(Entity, &crate::pane::PaneSplit, Option<&Children>), With<crate::pane::Pane>>,
    leaves_q: Query<
        (Entity, Option<&Children>),
        (With<crate::pane::Pane>, Without<crate::pane::PaneSplit>),
    >,
    stacks_q: Query<
        (Entity, Option<&Children>, Option<&vmux_core::PageMetadata>),
        With<crate::stack::Stack>,
    >,
    pane_sizes_q: Query<&crate::pane::PaneSize>,
    zoomed_q: Query<&crate::pane::Zoomed>,
    focused: Option<Res<crate::stack::FocusedStack>>,
    mut current: ResMut<CurrentLayoutView>,
) {
    let Some(focused) = focused else {
        return;
    };
    let snapshot = crate::snapshot::build_layout_snapshot(
        &tabs_q,
        &splits_q,
        &leaves_q,
        &stacks_q,
        &pane_sizes_q,
        &zoomed_q,
        &focused,
        None,
    );
    let view = LayoutView::from_snapshot(&snapshot);
    if current.0 != view {
        current.0 = view;
    }
}

pub struct NativeViewPlugin;

impl Plugin for NativeViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LayoutRenderer>()
            .init_resource::<CurrentLayoutView>()
            .init_resource::<LastRenderedView>()
            .init_resource::<RecordedViewOps>()
            .add_systems(
                Update,
                (
                    update_current_layout_view.run_if(renderer_is_native),
                    diff_into_ops,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Focus, LayoutNode, LayoutSnapshot, Tab};

    #[test]
    fn layout_renderer_defaults_to_native() {
        assert_eq!(LayoutRenderer::default(), LayoutRenderer::Native);
    }

    fn tab(id: &str, name: &str, is_active: bool) -> Tab {
        Tab {
            id: Some(id.into()),
            name: name.into(),
            is_active,
            root: LayoutNode::Pane {
                id: Some("pane:1".into()),
                is_zoomed: false,
                stacks: vec![],
            },
        }
    }

    #[test]
    fn from_snapshot_projects_tabs_in_order() {
        let snapshot = LayoutSnapshot {
            tabs: vec![tab("tab:1", "A", true), tab("tab:2", "B", false)],
            focused: Focus::default(),
        };
        let view = LayoutView::from_snapshot(&snapshot);
        assert_eq!(
            view.tabs,
            vec![
                TabView {
                    id: NodeId("tab:1".into()),
                    name: "A".into(),
                    title: "A".into(),
                    is_active: true
                },
                TabView {
                    id: NodeId("tab:2".into()),
                    name: "B".into(),
                    title: "B".into(),
                    is_active: false
                },
            ]
        );
    }

    #[test]
    fn from_snapshot_skips_tabs_without_id() {
        let mut t = tab("tab:1", "A", true);
        t.id = None;
        let snapshot = LayoutSnapshot {
            tabs: vec![t],
            focused: Focus::default(),
        };
        assert!(LayoutView::from_snapshot(&snapshot).tabs.is_empty());
    }

    #[test]
    fn layout_view_default_is_empty() {
        assert!(LayoutView::default().tabs.is_empty());
    }

    #[test]
    fn from_snapshot_uses_stack_title_when_tab_unnamed() {
        let snapshot = LayoutSnapshot {
            tabs: vec![Tab {
                id: Some("tab:1".into()),
                name: String::new(),
                is_active: true,
                root: LayoutNode::Pane {
                    id: Some("pane:1".into()),
                    is_zoomed: false,
                    stacks: vec![crate::protocol::Stack {
                        id: Some("stack:2".into()),
                        title: "GitHub".into(),
                        ..Default::default()
                    }],
                },
            }],
            focused: Focus::default(),
        };
        let view = LayoutView::from_snapshot(&snapshot);
        assert_eq!(view.tabs[0].title, "GitHub");
        assert_eq!(view.tabs[0].name, "");
    }

    #[test]
    fn from_snapshot_address_from_focused_stack() {
        let snapshot = LayoutSnapshot {
            tabs: vec![Tab {
                id: Some("tab:1".into()),
                name: "T".into(),
                is_active: true,
                root: LayoutNode::Pane {
                    id: Some("pane:1".into()),
                    is_zoomed: false,
                    stacks: vec![crate::protocol::Stack {
                        id: Some("stack:9".into()),
                        url: "https://example.com/".into(),
                        ..Default::default()
                    }],
                },
            }],
            focused: Focus {
                tab: Some("tab:1".into()),
                pane: Some("pane:1".into()),
                stack: Some("stack:9".into()),
            },
        };
        assert_eq!(
            LayoutView::from_snapshot(&snapshot).address,
            "https://example.com/"
        );
    }

    #[test]
    fn from_snapshot_collects_active_tab_stacks_with_active_flag() {
        let snapshot = LayoutSnapshot {
            tabs: vec![Tab {
                id: Some("tab:1".into()),
                name: "T".into(),
                is_active: true,
                root: LayoutNode::Pane {
                    id: Some("pane:1".into()),
                    is_zoomed: false,
                    stacks: vec![
                        crate::protocol::Stack {
                            id: Some("stack:2".into()),
                            title: "One".into(),
                            ..Default::default()
                        },
                        crate::protocol::Stack {
                            id: Some("stack:3".into()),
                            title: "Two".into(),
                            ..Default::default()
                        },
                    ],
                },
            }],
            focused: Focus {
                tab: Some("tab:1".into()),
                pane: Some("pane:1".into()),
                stack: Some("stack:3".into()),
            },
        };
        let view = LayoutView::from_snapshot(&snapshot);
        assert_eq!(view.panes.len(), 1);
        let pane = &view.panes[0];
        assert!(pane.is_active);
        assert_eq!(pane.stacks.len(), 2);
        assert_eq!(pane.stacks[0].title, "One");
        assert!(!pane.stacks[0].is_active);
        assert_eq!(pane.stacks[1].title, "Two");
        assert!(pane.stacks[1].is_active);
    }

    #[test]
    fn from_snapshot_groups_stacks_per_pane_in_split() {
        let pane = |pid: &str, sid: &str, title: &str| LayoutNode::Pane {
            id: Some(pid.into()),
            is_zoomed: false,
            stacks: vec![crate::protocol::Stack {
                id: Some(sid.into()),
                title: title.into(),
                ..Default::default()
            }],
        };
        let snapshot = LayoutSnapshot {
            tabs: vec![Tab {
                id: Some("tab:1".into()),
                name: "T".into(),
                is_active: true,
                root: LayoutNode::Split {
                    id: Some("split:1".into()),
                    direction: crate::protocol::SplitDirection::Row,
                    flex_weights: vec![],
                    children: vec![
                        pane("pane:1", "stack:1", "Left"),
                        pane("pane:2", "stack:2", "Right"),
                    ],
                },
            }],
            focused: Focus {
                tab: Some("tab:1".into()),
                pane: Some("pane:2".into()),
                stack: Some("stack:2".into()),
            },
        };
        let view = LayoutView::from_snapshot(&snapshot);
        assert_eq!(view.panes.len(), 2);
        assert_eq!(view.panes[0].id, NodeId("pane:1".into()));
        assert!(!view.panes[0].is_active);
        assert_eq!(view.panes[0].stacks[0].title, "Left");
        assert_eq!(view.panes[1].id, NodeId("pane:2".into()));
        assert!(view.panes[1].is_active);
        assert_eq!(view.panes[1].stacks[0].title, "Right");
    }

    #[test]
    fn from_snapshot_filters_empty_new_stack_placeholder() {
        let snapshot = LayoutSnapshot {
            tabs: vec![Tab {
                id: Some("tab:1".into()),
                name: "T".into(),
                is_active: true,
                root: LayoutNode::Pane {
                    id: Some("pane:1".into()),
                    is_zoomed: false,
                    stacks: vec![
                        crate::protocol::Stack {
                            id: Some("stack:1".into()),
                            title: "Real".into(),
                            url: "https://example.com/".into(),
                            ..Default::default()
                        },
                        crate::protocol::Stack {
                            id: Some("stack:2".into()),
                            title: "New Stack".into(),
                            ..Default::default()
                        },
                    ],
                },
            }],
            focused: Focus::default(),
        };
        let view = LayoutView::from_snapshot(&snapshot);
        assert_eq!(view.panes[0].stacks.len(), 1);
        assert_eq!(view.panes[0].stacks[0].title, "Real");
    }

    fn view(tabs: &[(&str, &str, bool)]) -> LayoutView {
        LayoutView {
            tabs: tabs
                .iter()
                .map(|(id, name, active)| TabView {
                    id: NodeId((*id).into()),
                    name: (*name).into(),
                    title: (*name).into(),
                    is_active: *active,
                })
                .collect(),
            address: String::new(),
            panes: Vec::new(),
        }
    }

    #[test]
    fn diff_no_change_emits_nothing() {
        let v = view(&[("tab:1", "A", true)]);
        assert!(diff_tabs(&v, &v).is_empty());
    }

    #[test]
    fn diff_added_tab_emits_create() {
        let old = view(&[("tab:1", "A", true)]);
        let new = view(&[("tab:1", "A", true), ("tab:2", "B", false)]);
        assert_eq!(
            diff_tabs(&old, &new),
            vec![
                ViewOp::CreateTab {
                    id: NodeId("tab:2".into()),
                    name: "B".into(),
                    is_active: false
                },
                ViewOp::SetTabOrder {
                    ids: vec![NodeId("tab:1".into()), NodeId("tab:2".into())]
                },
            ]
        );
    }

    #[test]
    fn diff_removed_tab_emits_remove() {
        let old = view(&[("tab:1", "A", true), ("tab:2", "B", false)]);
        let new = view(&[("tab:1", "A", true)]);
        assert_eq!(
            diff_tabs(&old, &new),
            vec![
                ViewOp::RemoveTab {
                    id: NodeId("tab:2".into())
                },
                ViewOp::SetTabOrder {
                    ids: vec![NodeId("tab:1".into())]
                },
            ]
        );
    }

    #[test]
    fn diff_renamed_or_activated_tab_emits_update() {
        let old = view(&[("tab:1", "A", false)]);
        let new = view(&[("tab:1", "A2", true)]);
        assert_eq!(
            diff_tabs(&old, &new),
            vec![ViewOp::UpdateTab {
                id: NodeId("tab:1".into()),
                name: "A2".into(),
                is_active: true
            }]
        );
    }

    #[test]
    fn diff_reorder_emits_set_order_only() {
        let old = view(&[("tab:1", "A", true), ("tab:2", "B", false)]);
        let new = view(&[("tab:2", "B", false), ("tab:1", "A", true)]);
        assert_eq!(
            diff_tabs(&old, &new),
            vec![ViewOp::SetTabOrder {
                ids: vec![NodeId("tab:2".into()), NodeId("tab:1".into())]
            }]
        );
    }

    #[test]
    fn diff_orders_ops_remove_before_create() {
        let old = view(&[("tab:1", "A", true)]);
        let new = view(&[("tab:2", "B", true)]);
        assert_eq!(
            diff_tabs(&old, &new),
            vec![
                ViewOp::RemoveTab {
                    id: NodeId("tab:1".into())
                },
                ViewOp::CreateTab {
                    id: NodeId("tab:2".into()),
                    name: "B".into(),
                    is_active: true
                },
                ViewOp::SetTabOrder {
                    ids: vec![NodeId("tab:2".into())]
                },
            ]
        );
    }

    #[test]
    fn diff_into_ops_records_create_then_update_across_changes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(NativeViewPlugin);

        app.world_mut().resource_mut::<CurrentLayoutView>().0 = view(&[("tab:1", "A", true)]);
        app.update();
        assert_eq!(
            app.world().resource::<RecordedViewOps>().0,
            vec![
                ViewOp::CreateTab {
                    id: NodeId("tab:1".into()),
                    name: "A".into(),
                    is_active: true
                },
                ViewOp::SetTabOrder {
                    ids: vec![NodeId("tab:1".into())]
                },
            ]
        );

        app.world_mut().resource_mut::<CurrentLayoutView>().0 = view(&[("tab:1", "B", true)]);
        app.update();
        assert_eq!(
            app.world().resource::<RecordedViewOps>().0,
            vec![ViewOp::UpdateTab {
                id: NodeId("tab:1".into()),
                name: "B".into(),
                is_active: true
            }]
        );
    }

    #[test]
    fn diff_into_ops_idle_when_unchanged() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(NativeViewPlugin);
        app.world_mut().resource_mut::<CurrentLayoutView>().0 = view(&[("tab:1", "A", true)]);
        app.update();
        app.world_mut().resource_mut::<RecordedViewOps>().0.clear();
        app.update();
        assert!(app.world().resource::<RecordedViewOps>().0.is_empty());
    }

    #[test]
    fn native_view_plugin_registers_default_renderer() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(NativeViewPlugin);
        assert_eq!(
            *app.world().resource::<LayoutRenderer>(),
            LayoutRenderer::Native
        );
    }

    #[test]
    fn producer_builds_layout_view_from_ecs_when_native() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(NativeViewPlugin);
        *app.world_mut().resource_mut::<LayoutRenderer>() = LayoutRenderer::Native;
        let t = app
            .world_mut()
            .spawn(crate::tab::Tab {
                name: "Work".into(),
            })
            .id();
        app.world_mut().insert_resource(crate::stack::FocusedStack {
            tab: Some(t),
            ..default()
        });
        app.update();
        let view = &app.world().resource::<CurrentLayoutView>().0;
        assert_eq!(view.tabs.len(), 1);
        assert_eq!(view.tabs[0].name, "Work");
        assert!(view.tabs[0].is_active);
    }

    #[test]
    fn producer_skips_when_cef() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(NativeViewPlugin);
        *app.world_mut().resource_mut::<LayoutRenderer>() = LayoutRenderer::Cef;
        let t = app
            .world_mut()
            .spawn(crate::tab::Tab {
                name: "Work".into(),
            })
            .id();
        app.world_mut().insert_resource(crate::stack::FocusedStack {
            tab: Some(t),
            ..default()
        });
        app.update();
        assert!(
            app.world()
                .resource::<CurrentLayoutView>()
                .0
                .tabs
                .is_empty()
        );
    }
}
