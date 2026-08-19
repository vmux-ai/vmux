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

/// A page hosted in this process, and the marker its own host systems find its view by.
///
/// The counterpart of [`WarmPage`](crate::warm_page::WarmPage), which declares the same things and
/// then keeps a pool of browsers warm. There is nothing to keep warm here, so what is left is the
/// marker: `WarmPage::spawn` put one on every view it made, and a page's host systems query it
/// rather than matching urls.
pub trait HostedPage: Component + Default {
    const HOST: &'static str;
    const URL: &'static str;
    const TITLE: &'static str;
}

/// Declares a page as hosted here, and marks its view when one opens.
pub struct HostedPagePlugin<M: HostedPage>(std::marker::PhantomData<fn() -> M>);

impl<M: HostedPage> Default for HostedPagePlugin<M> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<M: HostedPage> Plugin for HostedPagePlugin<M> {
    fn build(&self, app: &mut App) {
        vmux_core::register_host_spawn(app, M::HOST);
        app.world_mut().spawn(NativelyHosted {
            url: M::URL,
            title: M::TITLE,
        });
        // After the open, so a view opened this frame is marked this frame: the page's own systems
        // find it by this marker, and a frame where it is missing is a frame they skip it.
        app.add_systems(
            Update,
            mark_hosted_view::<M>.after(PageOpenSet::HandleKnownPages),
        );
    }
}

/// Put the page's own marker on the view that was opened for it.
///
/// `Without<M>` rather than a hook, so it costs one filtered query per frame and lands however the
/// view arrived — opened by a task, or restored with the window.
fn mark_hosted_view<M: HostedPage>(
    views: Query<(Entity, &PageMetadata), (With<vmux_core::host::page::HostsPage>, Without<M>)>,
    mut commands: Commands,
) {
    for (entity, page) in &views {
        if page.url == M::URL {
            commands.entity(entity).try_insert(M::default());
        }
    }
}

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

/// Whether a task is asking for this page, ignoring a trailing slash.
///
/// A url reaches a task from a bookmark, a typed address or another page's link, and those do not
/// agree about the slash. The debug page had its own opener and its own comparison for exactly
/// this reason; every page hosted here inherits it rather than each one remembering.
fn names_the_same_page(page: &str, asked_for: &str) -> bool {
    page.trim_end_matches('/') == asked_for.trim_end_matches('/')
}

fn handle_native_page_open(
    pages: Query<&NativelyHosted>,
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    children_q: Query<&Children>,
    mut commands: Commands,
) {
    let mut opened = std::collections::HashSet::new();

    for (task_entity, task) in &tasks {
        let Some(page) = pages
            .iter()
            .find(|page| names_the_same_page(page.url, &task.url))
        else {
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

#[cfg(test)]
mod tests {
    use super::names_the_same_page;

    /// A bookmark, a typed address and a link do not agree about the trailing slash, and a page
    /// that fails to match is answered with "Page not found" by the fallback.
    #[test]
    fn a_trailing_slash_does_not_decide_which_page_was_asked_for() {
        assert!(names_the_same_page("vmux://debug/", "vmux://debug/"));
        assert!(names_the_same_page("vmux://debug/", "vmux://debug"));
        assert!(names_the_same_page("vmux://debug", "vmux://debug/"));
    }

    /// One page's url must never claim another's, and a prefix is the way that happens.
    #[test]
    fn a_longer_url_is_a_different_page() {
        assert!(!names_the_same_page("vmux://debug/", "vmux://debugger/"));
        assert!(!names_the_same_page("vmux://debug/", "vmux://debug/panel"));
    }
}
