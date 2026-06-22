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
use vmux_core::{PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};
use vmux_layout::cef::LayoutCef;
use vmux_layout::space::{ActiveSpaceEntity, Space, space_of};

#[derive(Component, Clone, Copy, Debug)]
pub struct ActiveProfile(pub Entity);

#[derive(Component)]
struct Team;

#[derive(Component)]
struct TeamListSent;

pub struct TeamPlugin;

impl Plugin for TeamPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        app.add_systems(Startup, spawn_user_profile)
            .add_systems(
                Update,
                (
                    ensure_active_profile,
                    update_active_profile,
                    revert_active_profile_on_agent_exit,
                )
                    .chain(),
            )
            .add_systems(Update, emit_team)
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
    commands.spawn((Profile::user(), User, Name::new("Profile: You")));
}

fn ensure_active_profile(
    mut commands: Commands,
    user: Query<Entity, With<User>>,
    spaces: Query<Entity, (With<Space>, Without<ActiveProfile>)>,
) {
    let Ok(user) = user.single() else {
        return;
    };
    for space in &spaces {
        commands.entity(space).insert(ActiveProfile(user));
    }
}

fn update_active_profile(
    mut reader: MessageReader<vmux_command::CommandIssued>,
    mut spaces: Query<&mut ActiveProfile, With<Space>>,
    space_marker: Query<(), With<Space>>,
    child_of: Query<&ChildOf>,
    agents: Query<(), With<Agent>>,
    users: Query<(), With<User>>,
    active_space: Res<ActiveSpaceEntity>,
) {
    for ev in reader.read() {
        let caller = ev.caller;
        let space = if agents.get(caller).is_ok() {
            space_of(caller, &child_of, &space_marker)
        } else if users.get(caller).is_ok() {
            active_space.0
        } else {
            None
        };
        if let Some(space) = space
            && let Ok(mut active) = spaces.get_mut(space)
            && active.0 != caller
        {
            active.0 = caller;
        }
    }
}

fn revert_active_profile_on_agent_exit(
    mut removed: RemovedComponents<Agent>,
    user: Query<Entity, With<User>>,
    mut spaces: Query<&mut ActiveProfile, With<Space>>,
) {
    let gone: Vec<Entity> = removed.read().collect();
    if gone.is_empty() {
        return;
    }
    let Ok(user) = user.single() else {
        return;
    };
    for mut active in &mut spaces {
        if gone.contains(&active.0) {
            active.0 = user;
        }
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
    active: Option<Entity>,
    is_running: bool,
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
        is_active: active == Some(entity),
        is_running,
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
            if favicon.is_empty() && !meta.favicon_url.is_empty() {
                favicon = meta.favicon_url.clone();
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
    space_profiles: &Query<&ActiveProfile, With<Space>>,
    user_q: &Query<(Entity, &Profile), With<User>>,
    agent_q: &Query<(
        Entity,
        &Profile,
        &Agent,
        Option<&AgentRunState>,
        Option<&SessionId>,
    )>,
    child_of: &Query<&ChildOf>,
    space_marker: &Query<(), With<Space>>,
    meta_q: &Query<&PageMetadata>,
    children_q: &Query<&Children>,
) -> Vec<TeamMemberRow> {
    let active = active_space.0;
    let active_profile = active
        .and_then(|space| space_profiles.get(space).ok())
        .map(|ap| ap.0);

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
            active_profile,
            false,
        ));
    }
    if let Some(active) = active {
        for (entity, profile, agent, run, session) in agent_q {
            if space_of(entity, child_of, space_marker) == Some(active) {
                let is_running = matches!(run, Some(AgentRunState::Streaming));
                let (icon, title) = agent_page(entity, meta_q, children_q, child_of);
                let url = agent.kind.cli_url_prefix();
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
                    active_profile,
                    is_running,
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
    space_profiles: Query<&ActiveProfile, With<Space>>,
    user_q: Query<(Entity, &Profile), With<User>>,
    agent_q: Query<(
        Entity,
        &Profile,
        &Agent,
        Option<&AgentRunState>,
        Option<&SessionId>,
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
            &space_profiles,
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
            vmux_layout::Browser::new(&mut meshes, &mut webview_mt, TEAM_PAGE_URL),
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

fn on_team_command(
    trigger: On<BinReceive<TeamCommandEvent>>,
    mut messages: ResMut<bevy::ecs::message::Messages<AppCommand>>,
    active_space: Res<ActiveSpaceEntity>,
    mut spaces: Query<&mut ActiveProfile, With<Space>>,
) {
    let event = &trigger.event().payload;
    match event.command.as_str() {
        "activate" => {
            if let Some(id) = event.member_id.as_ref()
                && let Ok(bits) = id.parse::<u64>()
                && let Some(member) = Entity::try_from_bits(bits)
                && let Some(space) = active_space.0
                && let Ok(mut active) = spaces.get_mut(space)
            {
                active.0 = member;
            }
        }
        _ => {
            messages.write(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewStack {
                    url: Some(TEAM_PAGE_URL.to_string()),
                },
            )));
        }
    }
}
