use bevy::ecs::relationship::Relationship;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_api::command_bar::CommandBarTab;
use vmux_core::PageMetadata;
use vmux_history::LastActivatedAt;
use vmux_ui::i18n::{Locale, TranslationValue};

use crate::cef::Browser;
use crate::pane::{Pane, PaneSplit};
use crate::stack::{ActiveTabParam, Stack, collect_leaf_panes, focused_stack};

#[derive(SystemParam)]
pub struct TabGather<'w, 's> {
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

impl TabGather<'_, '_> {
    pub fn tabs(
        &self,
        active_tab: Option<Entity>,
        space_name: &str,
        locale: &Locale,
    ) -> Vec<CommandBarTab> {
        let mut bar_tabs = Vec::new();
        let Some(active_tab_e) = active_tab else {
            return bar_tabs;
        };
        let (_, _, active_stack) = focused_stack(
            active_tab,
            &self.all_children,
            &self.leaf_panes,
            &self.pane_ts,
            &self.pane_children,
            &self.stack_ts,
        );
        let active_pane = active_stack.and_then(|stack| {
            self.child_of_q
                .get(stack)
                .ok()
                .map(|child_of| child_of.get())
        });
        let mut tab_panes = Vec::new();
        collect_leaf_panes(
            active_tab_e,
            &self.all_children,
            &self.leaf_panes,
            &mut tab_panes,
        );
        for (pane_pos, &pane_e) in tab_panes.iter().enumerate() {
            let is_active_pane = active_pane == Some(pane_e);
            let Ok(children) = self.pane_children.get(pane_e) else {
                continue;
            };
            let mut tab_index = 0usize;
            for child in children.iter() {
                if !self.stack_q.contains(child) {
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
                if let Ok(tab_kids) = self.all_children.get(child) {
                    for browser_e in tab_kids.iter() {
                        if let Ok(meta) = self.browser_meta.get(browser_e) {
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
}
