use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

use bevy::prelude::*;
use bevy_cef::prelude::{CefKeyboardTarget, CefSystems, WebviewExtendStandardMaterial};
use vmux_core::page::{PageReady, PrewarmPage};
use vmux_core::{PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};

use crate::cef::LayoutCef;
use crate::window::VmuxWindow;

/// A `vmux://` page that can keep hidden, fully-mounted webviews ready for reuse.
pub trait WarmPage: Component {
    const HOST: &'static str;
    const URL: &'static str;
    const TITLE: &'static str;
    const POOL_SIZE: usize = 1;

    fn spawn(
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> Entity;
}

/// Prewarms a page after the layout shell is ready and claims warm webviews on open.
pub struct WarmPagePlugin<M: WarmPage>(PhantomData<fn() -> M>);

impl<M: WarmPage> Default for WarmPagePlugin<M> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<M: WarmPage> Plugin for WarmPagePlugin<M> {
    fn build(&self, app: &mut App) {
        vmux_core::register_host_spawn(app, M::HOST);
        app.init_resource::<WarmPageSpawnBudget>()
            .add_systems(
                Update,
                handle_warm_page_open::<M>.in_set(PageOpenSet::HandleKnownPages),
            )
            .add_systems(
                Update,
                maintain_warm_page_pool::<M>.in_set(WarmPageSet::Fill),
            );
    }
}

/// Marks a hidden prewarmed webview that has not been claimed yet.
#[derive(Component)]
pub struct WarmPageSpare {
    url: &'static str,
}

#[derive(Component)]
struct WarmPagePoolNode {
    url: &'static str,
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum WarmPageSet {
    Reset,
    Fill,
}

#[derive(Resource)]
struct WarmPageSpawnBudget(usize);

impl Default for WarmPageSpawnBudget {
    fn default() -> Self {
        Self(1)
    }
}

impl WarmPageSpawnBudget {
    fn take(&mut self) -> bool {
        if self.0 == 0 {
            return false;
        }
        self.0 -= 1;
        true
    }
}

/// Prewarms plain registered pages that use the standard browser bundle.
pub struct PrewarmPagesPlugin;

impl Plugin for PrewarmPagesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WarmPageSpawnBudget>()
            .configure_sets(
                Update,
                (WarmPageSet::Reset, WarmPageSet::Fill)
                    .chain()
                    .before(CefSystems::CreateAndResize),
            )
            .add_systems(
                Update,
                reset_warm_page_spawn_budget.in_set(WarmPageSet::Reset),
            )
            .add_systems(
                Update,
                handle_registered_page_open.in_set(PageOpenSet::HandleKnownPages),
            )
            .add_systems(
                Update,
                maintain_registered_page_pools.in_set(WarmPageSet::Fill),
            );
    }
}

fn reset_warm_page_spawn_budget(mut budget: ResMut<WarmPageSpawnBudget>) {
    budget.0 = 1;
}

fn handle_registered_page_open(
    pages: Query<&PrewarmPage>,
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    spares: Query<(Entity, &WarmPageSpare), With<PageReady>>,
    children_q: Query<&Children>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let pages: HashMap<&str, &PrewarmPage> = pages
        .iter()
        .filter(|page| page.pool_size > 0)
        .map(|page| (page.url, page))
        .collect();
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
            clear_stack_children(task.stack, &children_q, &mut commands);
            commands.entity(task.stack).insert(PageMetadata {
                url: page.url.to_string(),
                title: page.title.to_string(),
                ..default()
            });
            if let Some(spare) = available.get_mut(page.url).and_then(Vec::pop) {
                commands
                    .entity(spare)
                    .insert((ChildOf(task.stack), CefKeyboardTarget))
                    .remove::<WarmPageSpare>();
            } else {
                let webview = commands
                    .spawn(crate::cef::Browser::new_with_title(
                        &mut meshes,
                        &mut webview_mt,
                        page.url,
                        page.title,
                    ))
                    .id();
                commands
                    .entity(webview)
                    .insert((ChildOf(task.stack), CefKeyboardTarget));
            }
        }
        commands.entity(entity).insert(PageOpenHandled);
    }
}

fn maintain_registered_page_pools(
    pages: Query<&PrewarmPage>,
    pool_nodes: Query<(Entity, &WarmPagePoolNode)>,
    vmux_window: Query<Entity, With<VmuxWindow>>,
    layout_ready: Query<(), (With<LayoutCef>, With<PageReady>)>,
    spares: Query<&WarmPageSpare>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut budget: ResMut<WarmPageSpawnBudget>,
) {
    if layout_ready.is_empty() {
        return;
    }
    let Ok(window) = vmux_window.single() else {
        return;
    };
    for page in &pages {
        if page.pool_size == 0 || page.url.is_empty() {
            continue;
        }
        let node = pool_node_for(page.url, window, &pool_nodes, &mut commands);
        let count = spares.iter().filter(|spare| spare.url == page.url).count();
        for _ in count..page.pool_size {
            if !budget.take() {
                return;
            }
            let webview = commands
                .spawn(crate::cef::Browser::new_with_title(
                    &mut meshes,
                    &mut webview_mt,
                    page.url,
                    page.title,
                ))
                .id();
            commands
                .entity(webview)
                .insert((WarmPageSpare { url: page.url }, ChildOf(node)));
        }
    }
}

fn handle_warm_page_open<M: WarmPage>(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    spares: Query<(Entity, &WarmPageSpare), With<PageReady>>,
    children_q: Query<&Children>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let mut available: Vec<Entity> = spares
        .iter()
        .filter_map(|(entity, spare)| (spare.url == M::URL).then_some(entity))
        .collect();
    let mut handled_stacks = HashSet::new();

    for (entity, task) in &tasks {
        if task.url != M::URL {
            continue;
        }
        if handled_stacks.insert(task.stack) {
            clear_stack_children(task.stack, &children_q, &mut commands);
            commands.entity(task.stack).insert(PageMetadata {
                url: M::URL.to_string(),
                title: M::TITLE.to_string(),
                ..default()
            });
            if let Some(spare) = available.pop() {
                commands
                    .entity(spare)
                    .insert((ChildOf(task.stack), CefKeyboardTarget))
                    .remove::<WarmPageSpare>();
            } else {
                let page = M::spawn(&mut commands, &mut meshes, &mut webview_mt);
                commands
                    .entity(page)
                    .insert((ChildOf(task.stack), CefKeyboardTarget));
            }
        }
        commands.entity(entity).insert(PageOpenHandled);
    }
}

fn maintain_warm_page_pool<M: WarmPage>(
    pool_nodes: Query<(Entity, &WarmPagePoolNode)>,
    vmux_window: Query<Entity, With<VmuxWindow>>,
    layout_ready: Query<(), (With<LayoutCef>, With<PageReady>)>,
    spares: Query<&WarmPageSpare>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut budget: ResMut<WarmPageSpawnBudget>,
) {
    if layout_ready.is_empty() || M::POOL_SIZE == 0 {
        return;
    }
    let Ok(window) = vmux_window.single() else {
        return;
    };
    let node = pool_node_for(M::URL, window, &pool_nodes, &mut commands);
    let count = spares.iter().filter(|spare| spare.url == M::URL).count();
    for _ in count..M::POOL_SIZE {
        if !budget.take() {
            return;
        }
        let page = M::spawn(&mut commands, &mut meshes, &mut webview_mt);
        commands
            .entity(page)
            .insert((WarmPageSpare { url: M::URL }, ChildOf(node)));
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

fn clear_stack_children(stack: Entity, children_q: &Query<&Children>, commands: &mut Commands) {
    if let Ok(children) = children_q.get(stack) {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::{PageOpenId, PageOpenTask};

    use crate::cef::Browser;

    #[derive(Component)]
    struct TestPage;

    impl WarmPage for TestPage {
        const HOST: &'static str = "test";
        const URL: &'static str = "vmux://test/";
        const TITLE: &'static str = "Test";

        fn spawn(
            commands: &mut Commands,
            meshes: &mut ResMut<Assets<Mesh>>,
            webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
        ) -> Entity {
            commands
                .spawn((
                    TestPage,
                    Browser::new_with_title(meshes, webview_mt, Self::URL, Self::TITLE),
                ))
                .id()
        }
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, handle_warm_page_open::<TestPage>);
        app
    }

    fn task(stack: Entity) -> PageOpenTask {
        PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: TestPage::URL.to_string(),
            request_id: None,
        }
    }

    #[test]
    fn ready_spare_is_reparented() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        let spare = app
            .world_mut()
            .spawn((TestPage, WarmPageSpare { url: TestPage::URL }, PageReady {}))
            .id();
        let task = app.world_mut().spawn(task(stack)).id();

        app.update();

        assert_eq!(
            app.world()
                .get::<ChildOf>(spare)
                .map(|child| child.parent()),
            Some(stack)
        );
        assert!(app.world().get::<WarmPageSpare>(spare).is_none());
        assert!(app.world().get::<CefKeyboardTarget>(spare).is_some());
        assert!(app.world().get::<PageOpenHandled>(task).is_some());
    }

    #[test]
    fn unready_spare_falls_back_to_cold_page() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        let spare = app
            .world_mut()
            .spawn((TestPage, WarmPageSpare { url: TestPage::URL }))
            .id();
        app.world_mut().spawn(task(stack));

        app.update();

        assert!(app.world().get::<WarmPageSpare>(spare).is_some());
        let pages = app
            .world_mut()
            .query_filtered::<Entity, (With<TestPage>, With<ChildOf>)>()
            .iter(app.world())
            .count();
        assert_eq!(pages, 1);
    }

    #[test]
    fn pool_waits_for_layout_then_fills() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .init_resource::<WarmPageSpawnBudget>()
            .add_systems(Update, maintain_warm_page_pool::<TestPage>);
        app.world_mut().spawn(VmuxWindow);

        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<(), With<WarmPageSpare>>()
                .iter(app.world())
                .count(),
            0
        );

        app.world_mut().spawn((LayoutCef, PageReady {}));
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<(), With<WarmPageSpare>>()
                .iter(app.world())
                .count(),
            TestPage::POOL_SIZE
        );
    }

    #[test]
    fn registered_page_claims_ready_spare() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
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
    fn registered_pools_fill_for_every_page() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .init_resource::<WarmPageSpawnBudget>()
            .add_systems(Update, maintain_registered_page_pools);
        app.world_mut().spawn(VmuxWindow);
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
        app.world_mut().resource_mut::<WarmPageSpawnBudget>().0 = 1;
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
