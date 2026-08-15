//! Describing the workspace in the shape the launcher asks for.
//!
//! Only the layout knows what tabs, stacks and panes exist, so the layout answers — in the shared
//! `vmux_wire::command_bar` vocabulary, which is a wire contract rather than the launcher itself.
//! Everything that assembles a payload *around* this lives with the launcher, downstream in
//! `vmux_browser`, because it needs the command list and this does not.

use bevy::ecs::relationship::Relationship;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_core::PageMetadata;
use vmux_history::LastActivatedAt;
use vmux_ui::i18n::{Locale, TranslationValue};
use vmux_wire::command_bar::CommandBarTab;

use crate::cef::Browser;
use crate::pane::{Pane, PaneSplit};
use crate::stack::{ActiveTabParam, Stack, collect_leaf_panes, focused_stack};

#[derive(SystemParam)]
/// Bundled ECS queries for walking the active tab's panes/stacks into command-bar tab entries.
pub struct TabGatherParams<'w, 's> {
    pub active_tab: ActiveTabParam<'w, 's>,
    pub all_children: Query<'w, 's, &'static Children>,
    pub leaf_panes: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>)>,
    pub pane_ts: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Pane>>,
    pub pane_children: Query<'w, 's, &'static Children, With<Pane>>,
    pub stack_ts: Query<'w, 's, (Entity, &'static LastActivatedAt), With<Stack>>,
    pub stack_q: Query<'w, 's, Entity, With<Stack>>,
    pub browser_meta: Query<'w, 's, &'static PageMetadata, With<Browser>>,
    pub child_of_q: Query<'w, 's, &'static ChildOf>,
}

/// Collect the active tab's open stacks as [`CommandBarTab`] entries, shared by the
/// command-bar modal and the home launcher.
#[allow(clippy::too_many_arguments)]
pub fn gather_command_bar_tabs(
    active_tab: Option<Entity>,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: &Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: &Query<&Children, With<Pane>>,
    stack_ts: &Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_q: &Query<Entity, With<Stack>>,
    browser_meta: &Query<&PageMetadata, With<Browser>>,
    child_of_q: &Query<&ChildOf>,
    space_name: &str,
    locale: &Locale,
) -> Vec<CommandBarTab> {
    let mut bar_tabs = Vec::new();
    let Some(active_tab_e) = active_tab else {
        return bar_tabs;
    };
    let (_, _, active_stack) = focused_stack(
        active_tab,
        all_children,
        leaf_panes,
        pane_ts,
        pane_children,
        stack_ts,
    );
    let active_pane = active_stack.and_then(|t| child_of_q.get(t).ok().map(|co| co.get()));
    let mut tab_panes = Vec::new();
    collect_leaf_panes(active_tab_e, all_children, leaf_panes, &mut tab_panes);
    for (pane_pos, &pane_e) in tab_panes.iter().enumerate() {
        let is_active_pane = active_pane == Some(pane_e);
        let Ok(children) = pane_children.get(pane_e) else {
            continue;
        };
        let mut tab_index = 0usize;
        for child in children.iter() {
            if !stack_q.contains(child) {
                continue;
            }
            let stack_is_active = active_stack == Some(child) && is_active_pane;
            let pane_number = pane_pos as i64 + 1;
            let stack_number = tab_index as i64 + 1;
            let location = if space_name.is_empty() {
                locale.translate_with(
                    "command-pane-stack-location",
                    &[
                        ("pane", TranslationValue::Number(pane_number)),
                        ("stack", TranslationValue::Number(stack_number)),
                    ],
                )
            } else {
                locale.translate_with(
                    "command-space-pane-stack-location",
                    &[
                        ("space", TranslationValue::String(space_name)),
                        ("pane", TranslationValue::Number(pane_number)),
                        ("stack", TranslationValue::Number(stack_number)),
                    ],
                )
            };
            if let Ok(tab_kids) = all_children.get(child) {
                for browser_e in tab_kids.iter() {
                    if let Ok(meta) = browser_meta.get(browser_e) {
                        bar_tabs.push(CommandBarTab {
                            title: meta.title.clone(),
                            url: meta.url.clone(),
                            pane_id: pane_e.to_bits(),
                            tab_index: tab_index as u32,
                            is_active: stack_is_active,
                            location: location.clone(),
                        });
                    }
                }
            }
            tab_index += 1;
        }
    }
    bar_tabs
}
