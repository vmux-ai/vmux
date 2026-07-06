use bevy::prelude::*;
use bevy_cef::prelude::*;

use vmux_agent::AgentRunState;
use vmux_command::{AppCommand, BrowserCommand, OpenCommand};
use vmux_core::agent::SessionId;
use vmux_core::event::team::{
    TEAM_EVENT, TEAM_PAGE_URL, TeamCommandEvent, TeamEvent, TeamMemberRow,
};
use vmux_core::page::PageReady;
use vmux_core::team::{Agent, Profile, User};
use vmux_core::{
    PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask, focus_pane_entity,
};
use vmux_layout::cef::LayoutCef;
use vmux_layout::space::{ActiveSpaceEntity, Space, space_of};
use vmux_layout::stack::Stack;

#[derive(Component)]
struct Team;

#[derive(Component)]
struct TeamListSent;

/// Wires the team domain: spawns the user profile, emits the team-member list (user and
/// agents) to ready views, and handles team commands.
pub struct TeamPlugin;

impl Plugin for TeamPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        vmux_core::register_host_spawn(app, "team");
        app.add_systems(Startup, spawn_user_profile)
            .add_systems(Update, (sync_user_profile_name, emit_team).chain())
            .add_systems(
                Update,
                handle_team_page_open.in_set(PageOpenSet::HandleKnownPages),
            )
            .add_plugins(BinEventEmitterPlugin::<(TeamCommandEvent,)>::for_hosts(&[
                "team", "layout",
            ]))
            .add_observer(on_team_command)
            .add_observer(reset_team_sent_on_page_ready);
    }
}

fn spawn_user_profile(mut commands: Commands) {
    let mut identity = commands.spawn((Profile::user(), User, Name::new("Profile: User")));
    if vmux_core::profile::is_test_session() {
        identity.insert(vmux_core::team::Tester);
    }
}

/// Keep the user profile's name in sync with the active space's profile name
/// (e.g. "Personal").
fn sync_user_profile_name(
    active_space: Option<Res<vmux_space::ActiveSpace>>,
    mut user: Query<&mut Profile, With<User>>,
) {
    let Some(active) = active_space else {
        return;
    };
    let Ok(mut profile) = user.single_mut() else {
        return;
    };
    if profile.name != active.record.profile {
        *profile = Profile::user_named(active.record.profile.clone());
    }
}

#[allow(clippy::too_many_arguments)]
fn team_member_row(
    entity: Entity,
    profile: &Profile,
    icon: String,
    url: String,
    title: String,
    sid: String,
    is_user: bool,
    is_running: bool,
    is_done_unseen: bool,
) -> TeamMemberRow {
    TeamMemberRow {
        id: entity.to_bits().to_string(),
        name: profile.name.clone(),
        initials: profile.avatar.initials.clone(),
        color: profile.avatar.color.clone(),
        icon,
        url,
        title,
        sid,
        is_user,
        is_running,
        is_done_unseen,
    }
}

/// An agent's live favicon URL, page url, and title. These live on the *webview*
/// entity, which is the agent entity itself (CLI terminal), a child of it, or a
/// child of its owning stack (page agent). Probe all three. The page renders the
/// favicon via `favicon_src_for_url(favicon, url)` so URL-mapped agent icons
/// (e.g. Vibe's Mistral logo) match the tab strip.
fn agent_page(
    entity: Entity,
    meta_q: &Query<&PageMetadata>,
    children_q: &Query<&Children>,
    child_of: &Query<&ChildOf>,
) -> (String, String) {
    let mut candidates = vec![entity];
    if let Ok(children) = children_q.get(entity) {
        candidates.extend(children.iter());
    }
    if let Ok(parent) = child_of.get(entity) {
        let stack = parent.parent();
        candidates.push(stack);
        if let Ok(children) = children_q.get(stack) {
            candidates.extend(children.iter());
        }
    }
    let mut favicon = String::new();
    let mut title = String::new();
    for candidate in candidates {
        if let Ok(meta) = meta_q.get(candidate) {
            if favicon.is_empty() && !meta.icon.favicon_url().is_empty() {
                favicon = meta.icon.favicon_url().to_string();
            }
            if title.is_empty() && !meta.title.is_empty() {
                title = meta.title.clone();
            }
        }
    }
    (favicon, title)
}

fn build_team_members(
    active_space: &ActiveSpaceEntity,
    user_q: &Query<(Entity, &Profile), With<User>>,
    agent_q: &Query<(
        Entity,
        &Profile,
        &Agent,
        Option<&AgentRunState>,
        Option<&SessionId>,
        Option<&vmux_core::notify::AgentDoneUnseen>,
    )>,
    child_of: &Query<&ChildOf>,
    space_marker: &Query<(), With<Space>>,
    meta_q: &Query<&PageMetadata>,
    children_q: &Query<&Children>,
) -> Vec<TeamMemberRow> {
    let active = active_space.0;

    let mut members = Vec::new();
    if let Ok((entity, profile)) = user_q.single() {
        members.push(team_member_row(
            entity,
            profile,
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            true,
            false,
            false,
        ));
    }
    if let Some(active) = active {
        for (entity, profile, agent, run, session, done) in agent_q {
            if space_of(entity, child_of, space_marker) == Some(active) {
                let is_running = matches!(run, Some(AgentRunState::Streaming));
                let is_done_unseen = done.is_some();
                let (icon, title) = agent_page(entity, meta_q, children_q, child_of);
                let url = agent.kind.map(|k| k.cli_url_prefix()).unwrap_or_default();
                let sid = session
                    .map(|s| s.0.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| agent.sid.clone());
                members.push(team_member_row(
                    entity,
                    profile,
                    icon,
                    url,
                    title,
                    sid,
                    false,
                    is_running,
                    is_done_unseen,
                ));
            }
        }
    }
    members
}

fn emit_team(
    browsers: NonSend<Browsers>,
    pending_layout: Query<Entity, (With<LayoutCef>, With<PageReady>, Without<TeamListSent>)>,
    sent_layout: Query<Entity, (With<LayoutCef>, With<PageReady>, With<TeamListSent>)>,
    pending_team: Query<Entity, (With<Team>, With<PageReady>, Without<TeamListSent>)>,
    sent_team: Query<Entity, (With<Team>, With<PageReady>, With<TeamListSent>)>,
    active_space: Res<ActiveSpaceEntity>,
    user_q: Query<(Entity, &Profile), With<User>>,
    agent_q: Query<(
        Entity,
        &Profile,
        &Agent,
        Option<&AgentRunState>,
        Option<&SessionId>,
        Option<&vmux_core::notify::AgentDoneUnseen>,
    )>,
    child_of: Query<&ChildOf>,
    space_marker: Query<(), With<Space>>,
    meta_q: Query<&PageMetadata>,
    children_q: Query<&Children>,
    mut last: Local<String>,
    mut commands: Commands,
) {
    let pending_total = pending_layout.iter().count() + pending_team.iter().count();
    let sent_total = sent_layout.iter().count() + sent_team.iter().count();
    if pending_total == 0 && sent_total == 0 {
        return;
    }

    let payload = TeamEvent {
        members: build_team_members(
            &active_space,
            &user_q,
            &agent_q,
            &child_of,
            &space_marker,
            &meta_q,
            &children_q,
        ),
    };
    let body = ron::ser::to_string(&payload).unwrap_or_default();
    let body_changed = body != *last;

    for entity in pending_layout.iter().chain(pending_team.iter()) {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(entity, TEAM_EVENT, &payload));
        commands.entity(entity).insert(TeamListSent);
    }
    if body_changed {
        for entity in sent_layout.iter().chain(sent_team.iter()) {
            if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                continue;
            }
            commands.trigger(BinHostEmitEvent::from_rkyv(entity, TEAM_EVENT, &payload));
        }
        *last = body;
    }
}

fn handle_team_page_open(
    tasks: Query<(Entity, &PageOpenTask), (Without<PageOpenHandled>, Without<PageOpenError>)>,
    children_q: Query<&Children>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for (entity, task) in &tasks {
        if task.url != TEAM_PAGE_URL {
            continue;
        }
        if let Ok(children) = children_q.get(task.stack) {
            for child in children.iter() {
                commands.entity(child).try_despawn();
            }
        }
        commands.entity(task.stack).insert(PageMetadata {
            title: "Team".to_string(),
            url: TEAM_PAGE_URL.to_string(),
            bg_color: None,
            ..default()
        });
        commands.spawn((
            vmux_layout::Browser::new_with_title(
                &mut meshes,
                &mut webview_mt,
                TEAM_PAGE_URL,
                "Team",
            ),
            Team,
            ChildOf(task.stack),
        ));
        commands.entity(entity).insert(PageOpenHandled);
    }
}

fn reset_team_sent_on_page_ready(
    trigger: On<BinReceive<PageReady>>,
    team_views: Query<(), With<Team>>,
    layout_views: Query<(), With<LayoutCef>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    if team_views.get(entity).is_err() && layout_views.get(entity).is_err() {
        return;
    }
    commands.entity(entity).remove::<TeamListSent>();
}

fn open_team_stack_in_space(
    space: Entity,
    stacks: &Query<(Entity, &PageMetadata), With<Stack>>,
    child_of: &Query<&ChildOf>,
    spaces: &Query<(), With<Space>>,
) -> Option<Entity> {
    stacks.iter().find_map(|(stack, meta)| {
        (meta.url == TEAM_PAGE_URL && space_of(stack, child_of, spaces) == Some(space))
            .then_some(stack)
    })
}

fn parse_member_entity(member_id: &str) -> Option<Entity> {
    let bits = member_id.parse::<u64>().ok()?;
    Entity::try_from_bits(bits)
}

fn on_team_command(
    trigger: On<BinReceive<TeamCommandEvent>>,
    mut messages: ResMut<bevy::ecs::message::Messages<AppCommand>>,
    mut issued: ResMut<bevy::ecs::message::Messages<vmux_command::CommandIssued>>,
    user: Query<Entity, With<User>>,
    active_space: Res<ActiveSpaceEntity>,
    stacks: Query<(Entity, &PageMetadata), With<Stack>>,
    agents: Query<Entity, With<Agent>>,
    child_of: Query<&ChildOf>,
    spaces: Query<(), With<Space>>,
    mut commands: Commands,
) {
    if let Some(member_id) = trigger.event().payload.member_id.as_deref() {
        if let Some(entity) = parse_member_entity(member_id)
            && agents.get(entity).is_ok()
        {
            focus_pane_entity(entity, &mut commands, &child_of);
        }
        return;
    }

    if let Some(space) = active_space.0
        && let Some(stack) = open_team_stack_in_space(space, &stacks, &child_of, &spaces)
    {
        focus_pane_entity(stack, &mut commands, &child_of);
        return;
    }

    let caller = user.single().unwrap_or(Entity::PLACEHOLDER);
    let cmd = AppCommand::Browser(BrowserCommand::Open(OpenCommand::InNewStack {
        url: Some(TEAM_PAGE_URL.to_string()),
    }));
    issued.write(vmux_command::CommandIssued {
        caller,
        command: cmd.clone(),
    });
    messages.write(cmd);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_core::LastActivatedAt;
    use vmux_core::agent::AgentKind;

    fn spawn_team_stack(world: &mut World, space: Entity) -> Entity {
        world
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: TEAM_PAGE_URL.to_string(),
                    ..default()
                },
                ChildOf(space),
            ))
            .id()
    }

    fn lookup(app: &mut App, space: Entity) -> Option<Entity> {
        app.world_mut()
            .run_system_once(
                move |stacks: Query<(Entity, &PageMetadata), With<Stack>>,
                      child_of: Query<&ChildOf>,
                      spaces: Query<(), With<Space>>| {
                    open_team_stack_in_space(space, &stacks, &child_of, &spaces)
                },
            )
            .unwrap()
    }

    #[test]
    fn done_unseen_sets_row_flag() {
        let row = team_member_row(
            Entity::PLACEHOLDER,
            &Profile::agent(AgentKind::Claude),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            false,
            false,
            true,
        );
        assert!(row.is_done_unseen);
    }

    #[test]
    fn finds_open_team_stack_in_active_space() {
        let mut app = App::new();
        let space = app.world_mut().spawn(Space).id();
        let stack = spawn_team_stack(app.world_mut(), space);
        assert_eq!(lookup(&mut app, space), Some(stack));
    }

    #[test]
    fn ignores_team_stack_in_other_space() {
        let mut app = App::new();
        let active = app.world_mut().spawn(Space).id();
        let other = app.world_mut().spawn(Space).id();
        spawn_team_stack(app.world_mut(), other);
        assert_eq!(lookup(&mut app, active), None);
    }

    #[test]
    fn ignores_non_team_stack_in_active_space() {
        let mut app = App::new();
        let space = app.world_mut().spawn(Space).id();
        app.world_mut().spawn((
            Stack::default(),
            PageMetadata {
                url: "https://example.com".to_string(),
                ..default()
            },
            ChildOf(space),
        ));
        assert_eq!(lookup(&mut app, space), None);
    }

    #[test]
    fn parse_member_entity_roundtrips_and_rejects_garbage() {
        let mut app = App::new();
        let entity = app.world_mut().spawn_empty().id();
        let bits = entity.to_bits().to_string();
        assert_eq!(parse_member_entity(&bits), Some(entity));
        assert_eq!(parse_member_entity("not-a-number"), None);
        assert_eq!(parse_member_entity(""), None);
    }

    #[test]
    fn team_page_open_titles_webview_team() {
        use vmux_core::page_open::PageOpenId;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, handle_team_page_open);

        let stack = app.world_mut().spawn(Stack::default()).id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: TEAM_PAGE_URL.to_string(),
            request_id: None,
        });
        app.update();

        let title = app
            .world_mut()
            .query_filtered::<&PageMetadata, With<Team>>()
            .single(app.world())
            .expect("team webview spawned")
            .title
            .clone();
        assert_eq!(title, "Team");
    }

    fn command_app() -> App {
        let mut app = App::new();
        app.add_message::<AppCommand>()
            .add_message::<vmux_command::CommandIssued>()
            .add_observer(on_team_command);
        app
    }

    #[test]
    fn agent_avatar_click_focuses_agent_stack() {
        let mut app = command_app();
        let space = app.world_mut().spawn(Space).id();
        app.insert_resource(ActiveSpaceEntity(Some(space)));
        let stack = app
            .world_mut()
            .spawn((
                Stack::default(),
                Agent {
                    sid: "s".to_string(),
                    kind: Some(AgentKind::Claude),
                },
                ChildOf(space),
            ))
            .id();

        app.world_mut().trigger(BinReceive::<TeamCommandEvent> {
            webview: Entity::PLACEHOLDER,
            payload: TeamCommandEvent {
                command: "focus".to_string(),
                member_id: Some(stack.to_bits().to_string()),
            },
        });
        app.world_mut().flush();

        assert!(app.world().get::<LastActivatedAt>(stack).is_some());
        assert_eq!(lookup(&mut app, space), None);
    }

    #[test]
    fn user_click_reuses_open_team_stack() {
        let mut app = command_app();
        let space = app.world_mut().spawn(Space).id();
        app.insert_resource(ActiveSpaceEntity(Some(space)));
        let team = spawn_team_stack(app.world_mut(), space);

        app.world_mut().trigger(BinReceive::<TeamCommandEvent> {
            webview: Entity::PLACEHOLDER,
            payload: TeamCommandEvent {
                command: "open".to_string(),
                member_id: None,
            },
        });
        app.world_mut().flush();

        assert!(app.world().get::<LastActivatedAt>(team).is_some());
    }

    #[test]
    fn acp_agent_appears_in_roster_with_registry_icon() {
        let mut app = App::new();
        let space = app.world_mut().spawn(Space).id();
        app.insert_resource(ActiveSpaceEntity(Some(space)));
        app.world_mut().spawn((Profile::user(), User));
        app.world_mut().spawn((
            Profile::registry("Mistral Vibe", "mistral-vibe"),
            Agent {
                sid: "sid-1".to_string(),
                kind: None,
            },
            PageMetadata {
                url: "vmux://agent/mistral-vibe".to_string(),
                icon: vmux_core::PageIcon::favicon("https://cdn.example/vibe.svg"),
                ..default()
            },
            ChildOf(space),
        ));

        let rows = app
            .world_mut()
            .run_system_once(
                |active: Res<ActiveSpaceEntity>,
                 user_q: Query<(Entity, &Profile), With<User>>,
                 agent_q: Query<(
                    Entity,
                    &Profile,
                    &Agent,
                    Option<&AgentRunState>,
                    Option<&SessionId>,
                    Option<&vmux_core::notify::AgentDoneUnseen>,
                )>,
                 child_of: Query<&ChildOf>,
                 space_marker: Query<(), With<Space>>,
                 meta_q: Query<&PageMetadata>,
                 children_q: Query<&Children>| {
                    build_team_members(
                        &active,
                        &user_q,
                        &agent_q,
                        &child_of,
                        &space_marker,
                        &meta_q,
                        &children_q,
                    )
                },
            )
            .unwrap();

        let agent = rows
            .iter()
            .find(|r| !r.is_user)
            .expect("acp agent in roster");
        assert_eq!(agent.name, "Mistral Vibe");
        assert_eq!(agent.icon, "https://cdn.example/vibe.svg");
        assert_eq!(agent.url, "");
    }
}
