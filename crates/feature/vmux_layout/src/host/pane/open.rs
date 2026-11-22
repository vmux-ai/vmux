use crate::stack::{ActiveTabParam, LayoutFocus, Stack};
use bevy::prelude::*;
use vmux_api::open_target::{PaneDirection, PaneOpenMode, PaneTarget};
use vmux_ecs::{PageOpenRequest, PageOpenTarget};
use vmux_history::LastActivatedAt;

use super::{
    OpenRequest, PaneStacks,
    focus::PendingCursorWarp,
    tree::{Pane, PaneHierarchy, PaneSplitDirection, PaneTree},
};
use crate::host::command::LayoutRequestSet;

pub(super) struct OpenPlugin;

impl Plugin for OpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((BesideOpenPlugin, DirectionalOpenPlugin));
    }
}

pub(super) struct BesideOpenPlugin;

impl Plugin for BesideOpenPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::placement::PlacementPlugin>() {
            app.add_plugins(crate::placement::PlacementPlugin);
        }
    }
}

pub(super) struct DirectionalOpenPlugin;

impl Plugin for DirectionalOpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OpenRequest>()
            .add_systems(Update, handle_in.in_set(LayoutRequestSet::Handle));
    }
}

#[derive(Message, Clone)]
pub struct OpenBesideRequest {
    pub pane: Entity,
    pub direction: Option<PaneDirection>,
    pub url: String,
    pub request_id: [u8; 16],
    pub focus: bool,
}

fn handle_in(
    mut reader: MessageReader<OpenRequest>,
    active_tab_param: ActiveTabParam,
    focus: LayoutFocus,
    pane_children: Query<&Children, With<Pane>>,
    tab_filter: Query<Entity, With<Stack>>,
    pane_stacks: PaneStacks,
    focused_space: crate::space::FocusedSpace,
    pane_hierarchy: PaneHierarchy,
    mut tree: PaneTree,
    mut commands: Commands,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
) {
    for request in reader.read() {
        let OpenRequest {
            direction,
            target,
            mode,
            url,
        } = request;

        let (_, active_pane_opt, _) = focus.resolve(active_tab_param.get());
        let Some(active) = active_pane_opt else {
            continue;
        };

        let resolved = url
            .clone()
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| focused_space.resolved_startup_url());

        let split_dir = PaneSplitDirection::from(*direction);

        let (target_pane, was_split) = match target {
            PaneTarget::Existing => match pane_hierarchy.sibling(active, *direction) {
                Some(sibling) => (sibling, false),
                None => {
                    let existing_tabs: Vec<Entity> = pane_children
                        .get(active)
                        .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
                        .unwrap_or_default();
                    let p2 = tree.split_leaf(active, split_dir, &existing_tabs, true);
                    (p2, true)
                }
            },
            PaneTarget::NewSplit => {
                let existing_tabs: Vec<Entity> = pane_children
                    .get(active)
                    .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
                    .unwrap_or_default();
                let p2 = tree.split_leaf(active, split_dir, &existing_tabs, true);
                (p2, true)
            }
        };

        if was_split {
            let new_stack = commands
                .spawn((
                    Stack::bundle(),
                    LastActivatedAt::now(),
                    ChildOf(target_pane),
                ))
                .id();
            page_open_requests.write(PageOpenRequest {
                target: PageOpenTarget::Stack(new_stack),
                url: resolved,
                request_id: None,
            });
        } else {
            match mode {
                PaneOpenMode::InPlace => {
                    let active_stack = focus
                        .stack(target_pane)
                        .or_else(|| pane_stacks.first(target_pane));
                    if let Some(stack) = active_stack {
                        page_open_requests.write(PageOpenRequest {
                            target: PageOpenTarget::Stack(stack),
                            url: resolved,
                            request_id: None,
                        });
                    }
                }
                PaneOpenMode::NewStack => {
                    let new_stack = commands
                        .spawn((
                            Stack::bundle(),
                            LastActivatedAt::now(),
                            ChildOf(target_pane),
                        ))
                        .id();
                    page_open_requests.write(PageOpenRequest {
                        target: PageOpenTarget::Stack(new_stack),
                        url: resolved,
                        request_id: None,
                    });
                }
            }
        }
        commands.entity(target_pane).insert(PendingCursorWarp);
    }
}
