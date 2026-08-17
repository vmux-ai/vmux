//! Opening a page whose components run in this process.
//!
//! The sibling of [`warm_page`](crate::warm_page), and its opposite. That one keeps hidden browsers
//! mounted so a page can be revealed instead of loaded; a page hosted here has nothing to keep warm
//! — no browser to start and no bundle to fetch, because mounting it is building a `VirtualDom`.
//! What is left is the part that was never about warmth: clearing the stack and giving it a view.

use bevy::prelude::*;
use vmux_core::{PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};

use vmux_core::host::page::NativelyHosted;

use crate::cef::Browser;
use crate::warm_page::clear_stack_children;

pub struct NativeOpenPlugin;

impl Plugin for NativeOpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            handle_native_page_open.in_set(PageOpenSet::HandleKnownPages),
        );
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn handle_native_page_open(
    pages: Query<&NativelyHosted>,
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    children_q: Query<&Children>,
    mut commands: Commands,
) {
    let mut opened = std::collections::HashSet::new();

    for (task_entity, task) in &tasks {
        let Some(page) = pages.iter().find(|page| page.url == task.url) else {
            continue;
        };
        // Two tasks can name one stack in a frame; the second would clear what the first put there.
        if opened.insert(task.stack) {
            clear_stack_children(task.stack, &children_q, &mut commands);
            commands.entity(task.stack).insert(PageMetadata {
                url: page.url.to_string(),
                title: page.title.to_string(),
                ..default()
            });
            commands.spawn((
                Browser::native_page(page.url, page.title),
                ChildOf(task.stack),
            ));
        }
        commands.entity(task_entity).insert(PageOpenHandled);
    }
}
