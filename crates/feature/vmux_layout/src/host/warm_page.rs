use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use bevy_cef::prelude::CefSystems;
use bevy_cef::prelude::HostWindow;
use vmux_ecs::KeyboardOwner;
use vmux_ecs::page::{PageReady, PrewarmPage};
use vmux_ecs::{PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};

use crate::cef::LayoutCef;
use crate::window::VmuxWindow;
use vmux_flex::prelude::*;

pub struct PrewarmPagesPlugin;

impl Plugin for PrewarmPagesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            handle_registered_page_open.in_set(PageOpenSet::HandleKnownPages),
        )
        .add_systems(
            Update,
            maintain_registered_page_pools.before(CefSystems::CreateAndResize),
        );
    }
}

#[derive(Component)]
pub struct WarmPageSpare {
    url: &'static str,
}

#[derive(Component)]
struct WarmPagePoolNode {
    url: &'static str,
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn handle_registered_page_open(
    pages: Query<&PrewarmPage>,
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    spares: Query<(Entity, &WarmPageSpare), With<PageReady>>,
    mut commands: Commands,
) {
    let pages: HashMap<&str, &PrewarmPage> = pages.iter().map(|page| (page.url, page)).collect();
    let mut available: HashMap<&str, Vec<Entity>> = HashMap::new();
    for (entity, spare) in &spares {
        available.entry(spare.url).or_default().push(entity);
    }
    let mut handled_stacks = HashSet::new();

    for (entity, task) in &tasks {
        let Some(page) = pages.get(task.url.as_str()).copied() else {
            continue;
        };
        if handled_stacks.insert((task.stack, page.url)) {
            commands.entity(task.stack).despawn_children();
            commands.entity(task.stack).insert(PageMetadata {
                url: page.url.to_string(),
                title: page.title.to_string(),
                ..default()
            });
            if let Some(spare) = available.get_mut(page.url).and_then(Vec::pop) {
                commands
                    .entity(spare)
                    .insert((ChildOf(task.stack), KeyboardOwner))
                    .remove::<WarmPageSpare>();
            } else {
                let webview = commands
                    .spawn(crate::cef::Browser::new_with_title(page.url, page.title))
                    .id();
                commands
                    .entity(webview)
                    .insert((ChildOf(task.stack), KeyboardOwner));
            }
        }
        commands.entity(entity).insert(PageOpenHandled);
    }
}

fn maintain_registered_page_pools(
    pages: Query<&PrewarmPage>,
    pool_nodes: Query<(Entity, &WarmPagePoolNode)>,
    vmux_windows: Query<(Entity, &HostWindow), With<VmuxWindow>>,
    focused_window: crate::window::FocusedWindow,
    layout_ready: Query<(), (With<LayoutCef>, With<PageReady>)>,
    spares: Query<&WarmPageSpare>,
    mut commands: Commands,
) {
    if layout_ready.is_empty() {
        return;
    }
    let Some(window) = focused_window
        .entity()
        .and_then(|focused| {
            vmux_windows
                .iter()
                .find_map(|(root, host)| (host.0 == focused).then_some(root))
        })
        .or_else(|| vmux_windows.iter().next().map(|(root, _)| root))
    else {
        return;
    };
    let mut remaining = 1;
    for page in &pages {
        if page.pool_size == 0 || page.url.is_empty() {
            continue;
        }
        let node = pool_node_for(page.url, window, &pool_nodes, &mut commands);
        let count = spares.iter().filter(|spare| spare.url == page.url).count();
        for _ in count..page.pool_size {
            if remaining == 0 {
                return;
            }
            remaining -= 1;
            let webview = commands
                .spawn(crate::cef::Browser::new_with_title(page.url, page.title))
                .id();
            commands
                .entity(webview)
                .insert((WarmPageSpare { url: page.url }, ChildOf(node)));
        }
    }
}

fn pool_node_for(
    url: &'static str,
    window: Entity,
    pool_nodes: &Query<(Entity, &WarmPagePoolNode)>,
    commands: &mut Commands,
) -> Entity {
    pool_nodes
        .iter()
        .find_map(|(entity, node)| (node.url == url).then_some(entity))
        .unwrap_or_else(|| {
            commands
                .spawn((
                    WarmPagePoolNode { url },
                    Node {
                        width: Val::Px(0.0),
                        height: Val::Px(0.0),
                        position_type: PositionType::Absolute,
                        ..default()
                    },
                    Visibility::Hidden,
                    ChildOf(window),
                ))
                .id()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_ecs::{PageOpenId, PageOpenTask};

    use crate::cef::Browser;

    #[test]
    fn registered_page_claims_ready_spare() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, handle_registered_page_open);
        app.world_mut().spawn(PrewarmPage {
            host: "history",
            url: "vmux://history/",
            title: "History",
            pool_size: 1,
        });
        let stack = app.world_mut().spawn_empty().id();
        let spare = app
            .world_mut()
            .spawn((
                WarmPageSpare {
                    url: "vmux://history/",
                },
                PageReady {},
            ))
            .id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://history/".to_string(),
                request_id: None,
            })
            .id();

        app.update();

        assert_eq!(
            app.world()
                .get::<ChildOf>(spare)
                .map(|child| child.parent()),
            Some(stack)
        );
        assert!(app.world().get::<WarmPageSpare>(spare).is_none());
        assert!(app.world().get::<PageOpenHandled>(task).is_some());
    }

    #[test]
    fn registered_page_without_pool_opens_cold() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, handle_registered_page_open);
        app.world_mut().spawn(PrewarmPage {
            host: "history",
            url: "vmux://history/",
            title: "History",
            pool_size: 0,
        });
        let stack = app.world_mut().spawn_empty().id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://history/".to_string(),
                request_id: None,
            })
            .id();

        app.update();

        assert!(app.world().get::<PageOpenHandled>(task).is_some());
        let pages = app
            .world_mut()
            .query_filtered::<&ChildOf, With<Browser>>()
            .iter(app.world())
            .filter(|child| child.parent() == stack)
            .count();
        assert_eq!(pages, 1);
    }

    #[test]
    fn registered_pools_fill_for_every_page() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, maintain_registered_page_pools);
        let window = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((VmuxWindow, HostWindow(window)));
        app.world_mut().spawn((LayoutCef, PageReady {}));
        for (host, title) in [("history", "History"), ("lsp", "Language Servers")] {
            app.world_mut().spawn(PrewarmPage {
                host,
                url: if host == "history" {
                    "vmux://history/"
                } else {
                    "vmux://lsp/"
                },
                title,
                pool_size: 1,
            });
        }

        app.update();
        app.update();

        assert_eq!(
            app.world_mut()
                .query_filtered::<(), With<WarmPageSpare>>()
                .iter(app.world())
                .count(),
            2
        );
    }
}
