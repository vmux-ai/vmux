use bevy::prelude::*;
use bevy_cef::prelude::*;

use vmux_agent::AgentRunState;
use vmux_command::{AppCommand, BrowserCommand, OpenCommand};
use vmux_core::event::team::{TEAM_EVENT, TeamCommandEvent, TeamEvent, TeamMemberRow};
use vmux_core::page::PageReady;
use vmux_core::team::{Agent, Profile, User};
use vmux_layout::cef::LayoutCef;
use vmux_layout::space::{ActiveSpaceEntity, Space, space_of};

#[derive(Component, Clone, Copy, Debug)]
pub struct ActiveProfile(pub Entity);

pub struct TeamPlugin;

impl Plugin for TeamPlugin {
    fn build(&self, app: &mut App) {
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
            .add_systems(Update, push_team_emit)
            .add_plugins(BinEventEmitterPlugin::<(TeamCommandEvent,)>::for_hosts(
                &["layout"],
            ))
            .add_observer(on_team_command);
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

fn team_member_row(
    entity: Entity,
    profile: &Profile,
    is_user: bool,
    active: Option<Entity>,
    is_running: bool,
) -> TeamMemberRow {
    TeamMemberRow {
        id: entity.to_bits().to_string(),
        name: profile.name.clone(),
        initials: profile.avatar.initials.clone(),
        color: profile.avatar.color.clone(),
        is_user,
        is_active: active == Some(entity),
        is_running,
    }
}

fn push_team_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    cef_q: Query<(Entity, Ref<PageReady>), With<LayoutCef>>,
    active_space: Res<ActiveSpaceEntity>,
    space_profiles: Query<&ActiveProfile, With<Space>>,
    user_q: Query<(Entity, &Profile), With<User>>,
    agent_q: Query<(Entity, &Profile, &Agent, Option<&AgentRunState>)>,
    child_of: Query<&ChildOf>,
    space_marker: Query<(), With<Space>>,
    mut last: Local<String>,
) {
    let Ok((cef_e, page_ready)) = cef_q.single() else {
        return;
    };
    if !browsers.has_browser(cef_e) || !browsers.host_emit_ready(&cef_e) {
        return;
    }

    let active = active_space.0;
    let active_profile = active
        .and_then(|space| space_profiles.get(space).ok())
        .map(|ap| ap.0);

    let mut members = Vec::new();
    if let Ok((entity, profile)) = user_q.single() {
        members.push(team_member_row(entity, profile, true, active_profile, false));
    }
    if let Some(active) = active {
        for (entity, profile, _agent, run) in &agent_q {
            if space_of(entity, &child_of, &space_marker) == Some(active) {
                let is_running = matches!(run, Some(AgentRunState::Streaming));
                members.push(team_member_row(
                    entity,
                    profile,
                    false,
                    active_profile,
                    is_running,
                ));
            }
        }
    }

    let payload = TeamEvent { members };
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if ron_body == *last && !page_ready.is_changed() {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(cef_e, TEAM_EVENT, &payload));
    *last = ron_body;
}

fn on_team_command(
    _trigger: On<BinReceive<TeamCommandEvent>>,
    mut messages: ResMut<bevy::ecs::message::Messages<AppCommand>>,
) {
    messages.write(AppCommand::Browser(BrowserCommand::Open(
        OpenCommand::InNewStack {
            url: Some(vmux_core::event::team::TEAM_PAGE_URL.to_string()),
        },
    )));
}
