use std::path::PathBuf;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use vmux_command::{AppCommand, LayoutCommand, ReadAppCommands, StackCommand};
use vmux_core::agent::{AgentKind, SpawnAgentInStackRequest};
use vmux_core::terminal::{TerminalLaunch, TerminalSpawnRequest};
use vmux_core::{
    ArchivedPage, PageArchiveRequest, PageMetadata, PageOpenRequest, PageOpenTarget, now_millis,
};

use crate::event::TERMINAL_PAGE_URL;
use crate::settings::LayoutSettings;
use crate::space::{ActiveSpaceEntity, Space, SpaceId, space_of};
use crate::stack::{FocusedStack, Stack, StackCommandSet};
use crate::tab::Tab;
use crate::window::spawn_tab_scaffold_in_space;

const MAX_ARCHIVE_ENTRIES: usize = 25;
const ARCHIVE_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

pub struct ArchivePlugin;

impl Plugin for ArchivePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PageArchiveRequest>()
            .add_systems(Update, (capture_archived_pages, maintain_archive))
            .add_systems(
                Update,
                (
                    archive_on_stack_close.before(StackCommandSet),
                    handle_reopen_closed_page,
                )
                    .in_set(ReadAppCommands),
            );
    }
}

fn archive_on_stack_close(
    mut reader: MessageReader<AppCommand>,
    focused: Res<FocusedStack>,
    stack_pages: Query<(&PageMetadata, Option<&TerminalLaunch>), With<Stack>>,
    child_of: Query<&ChildOf>,
    children_q: Query<&Children>,
    spaces: Query<(), With<Space>>,
    space_ids: Query<&SpaceId>,
    tabs: Query<(), With<Tab>>,
    mut writer: MessageWriter<PageArchiveRequest>,
) {
    let mut closing = false;
    for cmd in reader.read() {
        if matches!(
            cmd,
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Close))
        ) {
            closing = true;
        }
    }
    if !closing {
        return;
    }
    let Some(stack) = focused.stack else {
        return;
    };
    let Ok((meta, launch)) = stack_pages.get(stack) else {
        return;
    };
    if meta.url.is_empty() {
        return;
    }
    let space = space_of(stack, &child_of, &spaces);
    let space_id = space
        .and_then(|s| space_ids.get(s).ok())
        .map(|id| id.0.clone())
        .unwrap_or_default();
    let tab_index = space.and_then(|s| tab_index_of(stack, s, &child_of, &children_q, &tabs));
    writer.write(PageArchiveRequest {
        url: meta.url.clone(),
        title: meta.title.clone(),
        space_id,
        launch: launch.cloned(),
        tab_index,
        leaf_pane_id: String::new(),
        stack_index: 0,
        pane_path: Vec::new(),
    });
}

fn tab_index_of(
    stack: Entity,
    space: Entity,
    child_of: &Query<&ChildOf>,
    children_q: &Query<&Children>,
    tabs: &Query<(), With<Tab>>,
) -> Option<usize> {
    let mut cur = stack;
    let tab = loop {
        if tabs.get(cur).is_ok() {
            break cur;
        }
        cur = child_of.get(cur).ok()?.parent();
    };
    children_q
        .get(space)
        .ok()?
        .iter()
        .filter(|e| tabs.get(*e).is_ok())
        .position(|e| e == tab)
}

fn capture_archived_pages(mut reader: MessageReader<PageArchiveRequest>, mut commands: Commands) {
    for req in reader.read() {
        if req.url.is_empty() {
            continue;
        }
        commands.spawn(ArchivedPage {
            url: req.url.clone(),
            title: req.title.clone(),
            space_id: req.space_id.clone(),
            closed_at: now_millis(),
            launch: req.launch.clone(),
            tab_index: req.tab_index,
        });
    }
}

fn maintain_archive(archived: Query<(Entity, &ArchivedPage)>, mut commands: Commands) {
    let now = now_millis();
    let mut live: Vec<(Entity, i64)> = Vec::new();
    for (entity, page) in &archived {
        if now - page.closed_at > ARCHIVE_TTL_MS {
            commands.entity(entity).despawn();
        } else {
            live.push((entity, page.closed_at));
        }
    }
    if live.len() > MAX_ARCHIVE_ENTRIES {
        live.sort_by_key(|(_, closed_at)| *closed_at);
        let overflow = live.len() - MAX_ARCHIVE_ENTRIES;
        for (entity, _) in live.into_iter().take(overflow) {
            commands.entity(entity).despawn();
        }
    }
}

fn handle_reopen_closed_page(
    mut reader: MessageReader<AppCommand>,
    archived: Query<(Entity, &ArchivedPage)>,
    spaces: Query<(Entity, &SpaceId), With<Space>>,
    any_space: Query<Entity, With<Space>>,
    active_space: Res<ActiveSpaceEntity>,
    settings: Res<LayoutSettings>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    mut page_open: MessageWriter<PageOpenRequest>,
    mut spawn_agent: MessageWriter<SpawnAgentInStackRequest>,
    mut terminal_spawn: MessageWriter<TerminalSpawnRequest>,
    mut commands: Commands,
) {
    let mut reopen = false;
    for cmd in reader.read() {
        if matches!(
            cmd,
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Reopen))
        ) {
            reopen = true;
        }
    }
    if !reopen {
        return;
    }

    let Some((entry_entity, page)) = archived
        .iter()
        .max_by_key(|(_, p)| p.closed_at)
        .map(|(e, p)| (e, p.clone()))
    else {
        return;
    };

    let origin_space = spaces
        .iter()
        .find(|(_, id)| id.0 == page.space_id)
        .map(|(e, _)| e);
    let target_space = origin_space
        .or_else(|| active_space.0.filter(|e| any_space.get(*e).is_ok()))
        .or_else(|| any_space.iter().next());
    let Some(space) = target_space else {
        return;
    };

    let scaffold =
        spawn_tab_scaffold_in_space(&mut commands, space, *primary_window, settings.pane.gap);
    commands.entity(scaffold.stack).insert(PageMetadata {
        url: page.url.clone(),
        title: page.title.clone(),
        ..default()
    });
    if origin_space == Some(space)
        && let Some(idx) = page.tab_index
    {
        commands.entity(space).insert_children(idx, &[scaffold.tab]);
    }
    commands
        .entity(space)
        .insert(vmux_history::LastActivatedAt::now());

    if let Some(kind) = AgentKind::all()
        .into_iter()
        .find(|k| page.url.starts_with(&k.cli_url_prefix()))
    {
        let cwd = page
            .launch
            .as_ref()
            .map(|l| PathBuf::from(&l.cwd))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        let id_part = page.url.strip_prefix(&kind.cli_url_prefix()).unwrap_or("");
        let session_id = (!id_part.is_empty()).then(|| id_part.to_string());
        spawn_agent.write(SpawnAgentInStackRequest {
            kind,
            cwd,
            session_id,
            stack: scaffold.stack,
        });
    } else if page.url.starts_with(TERMINAL_PAGE_URL) {
        let cwd = page
            .launch
            .as_ref()
            .map(|l| l.cwd.clone())
            .filter(|c| !c.is_empty())
            .map(PathBuf::from);
        terminal_spawn.write(TerminalSpawnRequest {
            cwd,
            target_stack: Some(scaffold.stack),
        });
    } else {
        page_open.write(PageOpenRequest {
            target: PageOpenTarget::Stack(scaffold.stack),
            url: page.url.clone(),
            request_id: None,
        });
    }

    commands.entity(entry_entity).despawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::relationship::Relationship;
    use vmux_core::terminal::TerminalKind;

    fn page(url: &str, closed_at: i64) -> ArchivedPage {
        ArchivedPage {
            url: url.to_string(),
            title: String::new(),
            space_id: "s".to_string(),
            closed_at,
            launch: None,
            tab_index: None,
        }
    }

    #[test]
    fn capture_spawns_archived_page() {
        let mut app = App::new();
        app.add_message::<PageArchiveRequest>()
            .add_systems(Update, capture_archived_pages);
        app.world_mut()
            .resource_mut::<Messages<PageArchiveRequest>>()
            .write(PageArchiveRequest {
                url: "https://a.example".to_string(),
                title: "A".to_string(),
                space_id: "s".to_string(),
                launch: None,
                tab_index: None,
                leaf_pane_id: String::new(),
                stack_index: 0,
                pane_path: Vec::new(),
            });
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        let all: Vec<_> = q.iter(app.world()).collect();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].url, "https://a.example");
    }

    #[test]
    fn capture_skips_empty_url() {
        let mut app = App::new();
        app.add_message::<PageArchiveRequest>()
            .add_systems(Update, capture_archived_pages);
        app.world_mut()
            .resource_mut::<Messages<PageArchiveRequest>>()
            .write(PageArchiveRequest {
                url: String::new(),
                title: String::new(),
                space_id: "s".to_string(),
                launch: None,
                tab_index: None,
                leaf_pane_id: String::new(),
                stack_index: 0,
                pane_path: Vec::new(),
            });
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        assert_eq!(q.iter(app.world()).count(), 0);
    }

    #[test]
    fn maintain_enforces_cap_dropping_oldest() {
        let mut app = App::new();
        app.add_systems(Update, maintain_archive);
        let now = now_millis();
        for i in 0..(MAX_ARCHIVE_ENTRIES as i64 + 1) {
            app.world_mut().spawn(page(&format!("u{i}"), now - i));
        }
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        let urls: Vec<String> = q.iter(app.world()).map(|p| p.url.clone()).collect();
        assert_eq!(urls.len(), MAX_ARCHIVE_ENTRIES);
        let oldest = format!("u{}", MAX_ARCHIVE_ENTRIES);
        assert!(!urls.contains(&oldest));
    }

    #[test]
    fn maintain_purges_expired() {
        let mut app = App::new();
        app.add_systems(Update, maintain_archive);
        let now = now_millis();
        app.world_mut().spawn(page("fresh", now));
        app.world_mut()
            .spawn(page("stale", now - ARCHIVE_TTL_MS - 1));
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        let urls: Vec<String> = q.iter(app.world()).map(|p| p.url.clone()).collect();
        assert_eq!(urls, vec!["fresh".to_string()]);
    }

    fn drain_archive_reqs(app: &mut App) -> Vec<PageArchiveRequest> {
        app.world_mut()
            .resource_mut::<Messages<PageArchiveRequest>>()
            .drain()
            .collect()
    }

    #[test]
    fn close_command_archives_focused_stack() {
        let mut app = App::new();
        app.add_message::<AppCommand>()
            .add_message::<PageArchiveRequest>()
            .init_resource::<FocusedStack>()
            .add_systems(Update, super::archive_on_stack_close);
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: "https://gone.example".to_string(),
                    title: "Gone".to_string(),
                    ..default()
                },
                ChildOf(space),
            ))
            .id();
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));
        app.update();
        let reqs = drain_archive_reqs(&mut app);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].url, "https://gone.example");
        assert_eq!(reqs[0].space_id, "s1");
    }

    #[test]
    fn close_command_skips_empty_url_stack() {
        let mut app = App::new();
        app.add_message::<AppCommand>()
            .add_message::<PageArchiveRequest>()
            .init_resource::<FocusedStack>()
            .add_systems(Update, super::archive_on_stack_close);
        let stack = app
            .world_mut()
            .spawn((Stack::default(), PageMetadata::default()))
            .id();
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));
        app.update();
        assert!(drain_archive_reqs(&mut app).is_empty());
    }

    #[test]
    fn close_records_tab_index_of_closing_stack() {
        let mut app = App::new();
        app.add_message::<AppCommand>()
            .add_message::<PageArchiveRequest>()
            .init_resource::<FocusedStack>()
            .add_systems(Update, super::archive_on_stack_close);
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        app.world_mut().spawn((Tab::default(), ChildOf(space)));
        let tab1 = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let pane = app.world_mut().spawn(ChildOf(tab1)).id();
        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: "https://gone.example".to_string(),
                    ..default()
                },
                ChildOf(pane),
            ))
            .id();
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Close,
            )));
        app.update();
        let reqs = drain_archive_reqs(&mut app);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].tab_index, Some(1));
    }

    fn reopen_app() -> App {
        let mut app = App::new();
        app.add_message::<AppCommand>()
            .add_message::<PageOpenRequest>()
            .add_message::<SpawnAgentInStackRequest>()
            .add_message::<TerminalSpawnRequest>()
            .init_resource::<crate::space::ActiveSpaceEntity>()
            .init_resource::<crate::settings::LayoutSettings>()
            .add_systems(Update, super::handle_reopen_closed_page);
        app.world_mut()
            .spawn((bevy::window::Window::default(), bevy::window::PrimaryWindow));
        app
    }

    fn dispatch_reopen(app: &mut App) {
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(
                StackCommand::Reopen,
            )));
        app.update();
    }

    fn drain_opens(app: &mut App) -> Vec<PageOpenRequest> {
        app.world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect()
    }

    #[test]
    fn reopen_web_opens_in_origin_space_and_consumes_entry() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: "https://a.example".to_string(),
            title: "A".to_string(),
            space_id: "s1".to_string(),
            closed_at: 5,
            launch: None,
            tab_index: None,
        });
        dispatch_reopen(&mut app);

        let opens = drain_opens(&mut app);
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "https://a.example");
        assert!(matches!(opens[0].target, PageOpenTarget::Stack(_)));
        let mut q = app.world_mut().query::<&ArchivedPage>();
        assert_eq!(q.iter(app.world()).count(), 0);
        let mut metas = app
            .world_mut()
            .query::<(&crate::stack::Stack, &PageMetadata)>();
        assert!(
            metas
                .iter(app.world())
                .any(|(_, m)| m.url == "https://a.example")
        );
    }

    #[test]
    fn reopen_picks_newest_first() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: "https://old.example".to_string(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 1,
            launch: None,
            tab_index: None,
        });
        app.world_mut().spawn(ArchivedPage {
            url: "https://new.example".to_string(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 2,
            launch: None,
            tab_index: None,
        });
        dispatch_reopen(&mut app);
        let opens = drain_opens(&mut app);
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "https://new.example");
    }

    #[test]
    fn reopen_terminal_respawns_at_cwd() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: "vmux://terminal/".to_string(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 5,
            launch: Some(TerminalLaunch {
                command: "/bin/zsh".to_string(),
                args: vec![],
                cwd: "/work".to_string(),
                env: vec![],
                kind: TerminalKind::Plain,
            }),
            tab_index: None,
        });
        dispatch_reopen(&mut app);
        assert!(drain_opens(&mut app).is_empty());
        let spawns: Vec<TerminalSpawnRequest> = app
            .world_mut()
            .resource_mut::<Messages<TerminalSpawnRequest>>()
            .drain()
            .collect();
        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].cwd, Some(PathBuf::from("/work")));
        assert!(spawns[0].target_stack.is_some());
    }

    fn drain_agent_spawns(app: &mut App) -> Vec<SpawnAgentInStackRequest> {
        app.world_mut()
            .resource_mut::<Messages<SpawnAgentInStackRequest>>()
            .drain()
            .collect()
    }

    #[test]
    fn reopen_agent_starts_fresh_when_no_session_id() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: AgentKind::Claude.cli_url_prefix(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 5,
            launch: Some(TerminalLaunch {
                command: "claude".to_string(),
                args: vec![],
                cwd: "/proj".to_string(),
                env: vec![],
                kind: TerminalKind::Claude,
            }),
            tab_index: None,
        });
        dispatch_reopen(&mut app);
        assert!(drain_opens(&mut app).is_empty());
        let spawns = drain_agent_spawns(&mut app);
        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].kind, AgentKind::Claude);
        assert_eq!(spawns[0].cwd, PathBuf::from("/proj"));
        assert!(spawns[0].session_id.is_none());
    }

    #[test]
    fn reopen_agent_recovers_session_id_from_url() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: format!("{}sess-123", AgentKind::Claude.cli_url_prefix()),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 5,
            launch: Some(TerminalLaunch {
                command: "claude".to_string(),
                args: vec![],
                cwd: "/proj".to_string(),
                env: vec![],
                kind: TerminalKind::Claude,
            }),
            tab_index: None,
        });
        dispatch_reopen(&mut app);
        assert!(drain_opens(&mut app).is_empty());
        let spawns = drain_agent_spawns(&mut app);
        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].kind, AgentKind::Claude);
        assert_eq!(spawns[0].session_id.as_deref(), Some("sess-123"));
    }

    #[test]
    fn reopen_empty_archive_is_noop() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        dispatch_reopen(&mut app);
        assert!(drain_opens(&mut app).is_empty());
    }

    #[test]
    fn reopen_falls_back_to_active_space_when_origin_gone() {
        let mut app = reopen_app();
        let active = app
            .world_mut()
            .spawn((
                crate::space::Space,
                crate::space::SpaceId("active".to_string()),
            ))
            .id();
        app.world_mut()
            .insert_resource(crate::space::ActiveSpaceEntity(Some(active)));
        app.world_mut().spawn(ArchivedPage {
            url: "https://x.example".to_string(),
            title: String::new(),
            space_id: "ghost".to_string(),
            closed_at: 5,
            launch: None,
            tab_index: None,
        });
        dispatch_reopen(&mut app);
        let opens = drain_opens(&mut app);
        assert_eq!(opens.len(), 1);
        let mut tabs = app.world_mut().query::<(&crate::tab::Tab, &ChildOf)>();
        assert!(tabs.iter(app.world()).any(|(_, co)| co.get() == active));
    }

    #[test]
    fn reopen_restores_tab_at_original_index() {
        let mut app = reopen_app();
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string())))
            .id();
        let t0 = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        let t1 = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
        app.world_mut().spawn(ArchivedPage {
            url: "https://z.example".to_string(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 5,
            launch: None,
            tab_index: Some(0),
        });
        dispatch_reopen(&mut app);

        let tabs_q = app.world().entity(space).get::<Children>().unwrap();
        let tab_order: Vec<Entity> = tabs_q.iter().collect();
        assert_eq!(tab_order.len(), 3);
        assert_ne!(tab_order[0], t0);
        assert_ne!(tab_order[0], t1);
        assert_eq!(tab_order[1], t0);
        assert_eq!(tab_order[2], t1);
    }

    #[test]
    fn reopen_appends_when_origin_space_gone() {
        let mut app = reopen_app();
        let active = app
            .world_mut()
            .spawn((Space, SpaceId("active".to_string())))
            .id();
        app.world_mut()
            .insert_resource(crate::space::ActiveSpaceEntity(Some(active)));
        let t0 = app
            .world_mut()
            .spawn((Tab::default(), ChildOf(active)))
            .id();
        let t1 = app
            .world_mut()
            .spawn((Tab::default(), ChildOf(active)))
            .id();
        app.world_mut().spawn(ArchivedPage {
            url: "https://z.example".to_string(),
            title: String::new(),
            space_id: "ghost".to_string(),
            closed_at: 5,
            launch: None,
            tab_index: Some(0),
        });
        dispatch_reopen(&mut app);

        let tabs_q = app.world().entity(active).get::<Children>().unwrap();
        let tab_order: Vec<Entity> = tabs_q.iter().collect();
        assert_eq!(tab_order.len(), 3);
        assert_eq!(tab_order[0], t0);
        assert_eq!(tab_order[1], t1);
        assert_ne!(tab_order[2], t0);
        assert_ne!(tab_order[2], t1);
    }
}
