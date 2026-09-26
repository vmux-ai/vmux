use bevy::prelude::*;
use bevy_cef::prelude::{HostWindow, UiEventPlugin, UiInput};

use vmux_agent::AgentRunState;
use vmux_core::agent::SessionId;
use vmux_core::event::team::{
    ProfileRow, TEAM_PAGE_URL, TeamEvent, TeamMemberFocusRequest, TeamMemberRow, TeamOpenRequest,
    TeamProfileCreateRequest, TeamProfileSwitchRequest, TeamProfileUpdateRequest, TeamRequest,
};
use vmux_core::host::{UiStatePlugin, UiStateWrite};
use vmux_core::profile::{ProfileId, ProfileLabel};
use vmux_core::team::{Agent, Profile, User};
use vmux_core::{PageMetadata, focus_pane_entity};
use vmux_layout::LayoutUiStateUpdates;
use vmux_layout::cef::LayoutCef;
use vmux_layout::native_open::{HostedPage, HostedPagePlugin};
use vmux_layout::projection::TeamProjection as LayoutTeamProjection;
use vmux_layout::space::{CurrentSpace, Space, space_of};
use vmux_layout::stack::Stack;
use vmux_service::agent_events::AgentCommandRequest;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{AgentCommand, AgentCommandResult, ClientMessage, SharedAgentCommand};

pub struct TeamPlugin;

impl Plugin for TeamPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::TeamPage::plugin());
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        app.add_plugins((
            HostedPagePlugin::<Team>::default(),
            TeamProjectionPlugin,
            TeamIntentPlugin,
        ))
        .add_systems(Startup, (spawn_user_profile, spawn_profile_labels));
    }
}

struct TeamProjectionPlugin;

impl Plugin for TeamProjectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiStatePlugin::<TeamEvent>::default())
            .add_observer(replay_team)
            .add_systems(
                Update,
                (sync_user_profile_name, project_team, publish_team).chain(),
            )
            .add_systems(Update, answer_list_team);
    }
}

struct TeamIntentPlugin;

impl Plugin for TeamIntentPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ProfileSwitchRequested>()
            .add_plugins(UiEventPlugin::<(
                TeamRequest,
                TeamOpenRequest,
                TeamMemberFocusRequest,
                TeamProfileCreateRequest,
                TeamProfileSwitchRequest,
                TeamProfileUpdateRequest,
            )>::default())
            .add_observer(forward_legacy_team_open_request)
            .add_observer(forward_legacy_team_member_focus_request)
            .add_observer(forward_legacy_team_profile_create_request)
            .add_observer(forward_legacy_team_profile_switch_request)
            .add_observer(forward_legacy_team_profile_update_request)
            .add_observer(on_team_open_request)
            .add_observer(on_team_member_focus_request)
            .add_observer(on_team_profile_create_request)
            .add_observer(on_team_profile_switch_request)
            .add_observer(on_team_profile_update_request);
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct ProfileSwitchRequested {
    pub profile_id: String,
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "team",
    title: "Team",
    title_message_id: Some("team-title"),
    replaces_command: None,
    keywords: &["team", "agents", "profile"],
    icon: Some(vmux_core::BuiltinIcon::Users),
    command_bar: true,
};

#[derive(Component, Default)]
struct Team;

impl HostedPage for Team {
    const HOST: &'static str = "team";
    const URL: &'static str = TEAM_PAGE_URL;
    const TITLE: &'static str = "Team";
}

#[derive(Component, Clone, Debug, Default, PartialEq)]
struct TeamPresentation(TeamEvent);

fn spawn_user_profile(mut commands: Commands) {
    let mut identity = commands.spawn((Profile::user(), User, Name::new("Profile: User")));
    if vmux_core::profile::is_test_session() {
        identity.insert(vmux_core::team::Tester);
    }
}

fn spawn_profile_labels(mut commands: Commands) {
    let active = vmux_core::profile::active_profile_name();
    for id in vmux_core::profile::profile_ids() {
        let name = vmux_core::profile::profile_display_name(&id);
        let mut entity = commands.spawn((ProfileLabel, ProfileId(id.clone()), Name::new(name)));
        if id == active {
            entity.insert(vmux_core::Active);
        }
    }
}

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
    active_space: Option<Entity>,
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
    if let Some(active) = active_space {
        for (entity, profile, agent, run, session, done) in agent_q {
            if space_of(entity, child_of, space_marker) == Some(active) {
                let is_running = matches!(run, Some(AgentRunState::Streaming));
                let is_done_unseen = done.is_some();
                let (icon, title) = agent_page(entity, meta_q, children_q, child_of);
                let url = agent.kind.map(|k| k.cli_url_prefix()).unwrap_or_else(|| {
                    meta_q
                        .get(entity)
                        .map(|m| m.url.clone())
                        .unwrap_or_default()
                });
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

fn build_profiles(
    labels: &Query<(&ProfileId, &Name, Has<vmux_core::Active>), With<ProfileLabel>>,
) -> Vec<ProfileRow> {
    let mut profiles = Vec::new();
    for (id, name, is_active) in labels {
        profiles.push(ProfileRow {
            id: id.0.clone(),
            name: name.as_str().to_string(),
            color: vmux_api::avatar::hash_color(&id.0),
            is_active,
        });
    }
    profiles.sort_by(|left, right| {
        right
            .is_active
            .cmp(&left.is_active)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.id.cmp(&right.id))
    });
    profiles
}

fn answer_list_team(
    mut reader: MessageReader<AgentCommandRequest>,
    service: Option<Single<&ServiceClient>>,
    current_space: Query<Entity, With<CurrentSpace>>,
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
) {
    for request in reader.read() {
        if !matches!(
            request.command,
            AgentCommand::Shared(SharedAgentCommand::ListTeam)
        ) {
            continue;
        }
        let Some(service) = service.as_ref() else {
            continue;
        };
        let members = build_team_members(
            current_space.iter().next(),
            &user_q,
            &agent_q,
            &child_of,
            &space_marker,
            &meta_q,
            &children_q,
        );
        let result = match serde_json::to_string(&members) {
            Ok(json) => AgentCommandResult::Text(json),
            Err(error) => AgentCommandResult::Error(format!("list_team: {error}")),
        };
        service.0.send(ClientMessage::AgentCommandResponse {
            request_id: request.request_id,
            result,
        });
    }
}

fn project_team(
    views: Query<Entity, Or<(With<LayoutCef>, With<Team>, With<vmux_space::Spaces>)>>,
    presentations: Query<&TeamPresentation>,
    current_space: Query<Entity, With<CurrentSpace>>,
    active_spaces: Query<Entity, (With<Space>, With<vmux_core::Active>)>,
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
    host_windows: Query<&HostWindow>,
    space_marker: Query<(), With<Space>>,
    meta_q: Query<&PageMetadata>,
    children_q: Query<&Children>,
    profile_labels: Query<(&ProfileId, &Name, Has<vmux_core::Active>), With<ProfileLabel>>,
    mut commands: Commands,
) {
    for entity in &views {
        let target_space = space_of(entity, &child_of, &space_marker).or_else(|| {
            let window = vmux_layout::window::host_window_of(entity, &child_of, &host_windows)?;
            active_spaces.iter().find(|space| {
                vmux_layout::window::host_window_of(*space, &child_of, &host_windows)
                    == Some(window)
            })
        });
        let presentation = TeamPresentation(TeamEvent::project(
            build_team_members(
                target_space.or_else(|| current_space.iter().next()),
                &user_q,
                &agent_q,
                &child_of,
                &space_marker,
                &meta_q,
                &children_q,
            ),
            build_profiles(&profile_labels),
        ));
        if presentations
            .get(entity)
            .is_ok_and(|current| current == &presentation)
        {
            continue;
        }
        commands.entity(entity).insert(presentation);
    }
}

fn publish_team(
    presentations: Query<(Entity, &TeamPresentation), Changed<TeamPresentation>>,
    direct_views: Query<(), Or<(With<Team>, With<vmux_space::Spaces>)>>,
    layout_cefs: Query<(), With<LayoutCef>>,
    layout_ui: Query<(), With<LayoutUiStateUpdates>>,
    mut commands: Commands,
) {
    for (entity, presentation) in &presentations {
        if direct_views.contains(entity) {
            commands.trigger(UiStateWrite::<TeamEvent>::from_event(
                entity,
                &presentation.0,
            ));
        }
        if layout_cefs.contains(entity) {
            if layout_ui.contains(entity) {
                commands.trigger(
                    UiStateWrite::<vmux_layout::state::LayoutUiState>::from_event(
                        entity,
                        &presentation.0,
                    ),
                );
            }
            commands
                .entity(entity)
                .insert(LayoutTeamProjection(presentation.0.clone()));
        }
    }
}

fn replay_team(
    trigger: On<UiInput<vmux_core::page::PageReady>>,
    presentations: Query<&TeamPresentation>,
    direct_views: Query<(), Or<(With<Team>, With<vmux_space::Spaces>)>>,
    layout_cefs: Query<(), With<LayoutCef>>,
    layout_ui: Query<(), With<LayoutUiStateUpdates>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(presentation) = presentations.get(entity) else {
        return;
    };
    if direct_views.contains(entity) {
        commands.trigger(UiStateWrite::<TeamEvent>::from_event(
            entity,
            &presentation.0,
        ));
    }
    if layout_cefs.contains(entity) {
        if layout_ui.contains(entity) {
            commands.trigger(
                UiStateWrite::<vmux_layout::state::LayoutUiState>::from_event(
                    entity,
                    &presentation.0,
                ),
            );
        }
        commands
            .entity(entity)
            .insert(LayoutTeamProjection(presentation.0.clone()));
    }
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

fn forward_legacy_team_open_request(trigger: On<UiInput<TeamRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    if request.command == "create_profile"
        || request.command == "switch_profile"
        || request.command == "update_profile"
        || request.member_id.is_some()
    {
        return;
    }
    commands.trigger(UiInput {
        webview: trigger.event().webview,
        payload: TeamOpenRequest,
    });
}

fn forward_legacy_team_member_focus_request(
    trigger: On<UiInput<TeamRequest>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    if request.command == "create_profile"
        || request.command == "switch_profile"
        || request.command == "update_profile"
    {
        return;
    }
    let Some(member_id) = request.member_id.clone() else {
        return;
    };
    commands.trigger(UiInput {
        webview: trigger.event().webview,
        payload: TeamMemberFocusRequest { member_id },
    });
}

fn forward_legacy_team_profile_create_request(
    trigger: On<UiInput<TeamRequest>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    if request.command != "create_profile" {
        return;
    }
    let Some(name) = request.profile_name.clone() else {
        return;
    };
    commands.trigger(UiInput {
        webview: trigger.event().webview,
        payload: TeamProfileCreateRequest { name },
    });
}

fn forward_legacy_team_profile_switch_request(
    trigger: On<UiInput<TeamRequest>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    if request.command != "switch_profile" {
        return;
    }
    let Some(profile_id) = request.profile_id.clone() else {
        return;
    };
    commands.trigger(UiInput {
        webview: trigger.event().webview,
        payload: TeamProfileSwitchRequest { profile_id },
    });
}

fn forward_legacy_team_profile_update_request(
    trigger: On<UiInput<TeamRequest>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    if request.command != "update_profile" {
        return;
    }
    let (Some(profile_id), Some(name)) = (request.profile_id.clone(), request.profile_name.clone())
    else {
        return;
    };
    commands.trigger(UiInput {
        webview: trigger.event().webview,
        payload: TeamProfileUpdateRequest { profile_id, name },
    });
}

fn on_team_open_request(
    _trigger: On<UiInput<TeamOpenRequest>>,
    mut stack_requests: MessageWriter<vmux_layout::stack::OpenRequest>,
    current_space: Query<Entity, With<CurrentSpace>>,
    stacks: Query<(Entity, &PageMetadata), With<Stack>>,
    child_of: Query<&ChildOf>,
    spaces: Query<(), With<Space>>,
    mut commands: Commands,
) {
    if let Some(space) = current_space.iter().next()
        && let Some(stack) = open_team_stack_in_space(space, &stacks, &child_of, &spaces)
    {
        focus_pane_entity(stack, &mut commands, &child_of);
        return;
    }

    stack_requests.write(vmux_layout::stack::OpenRequest {
        url: Some(TEAM_PAGE_URL.to_string()),
    });
}

fn on_team_member_focus_request(
    trigger: On<UiInput<TeamMemberFocusRequest>>,
    agents: Query<Entity, With<Agent>>,
    child_of: Query<&ChildOf>,
    mut commands: Commands,
) {
    let Some(entity) = parse_member_entity(&trigger.event().payload.member_id) else {
        return;
    };
    if agents.get(entity).is_ok() {
        focus_pane_entity(entity, &mut commands, &child_of);
    }
}

fn on_team_profile_create_request(
    trigger: On<UiInput<TeamProfileCreateRequest>>,
    mut profile_switches: MessageWriter<ProfileSwitchRequested>,
    mut commands: Commands,
) {
    let name = trigger.event().payload.name.trim().to_string();
    match vmux_core::profile::create_profile(&name) {
        Ok(profile_id) => {
            commands.spawn((ProfileLabel, ProfileId(profile_id.clone()), Name::new(name)));
            profile_switches.write(ProfileSwitchRequested { profile_id });
        }
        Err(error) => bevy::log::warn!("profile create failed: {error}"),
    }
}

fn on_team_profile_switch_request(
    trigger: On<UiInput<TeamProfileSwitchRequest>>,
    profile_labels: Query<&ProfileId, With<ProfileLabel>>,
    mut profile_switches: MessageWriter<ProfileSwitchRequested>,
) {
    let profile_id = vmux_core::profile::sanitize_profile(&trigger.event().payload.profile_id);
    if profile_id == vmux_core::profile::active_profile_name()
        || !vmux_core::profile::profile_exists(&profile_id)
    {
        return;
    }
    if profile_labels.iter().any(|id| id.0 == profile_id) {
        profile_switches.write(ProfileSwitchRequested { profile_id });
    }
}

fn on_team_profile_update_request(
    trigger: On<UiInput<TeamProfileUpdateRequest>>,
    user: Query<Entity, With<User>>,
    mut space_profiles: Query<&mut vmux_layout::profile::Profile, With<Space>>,
    mut active_record: Option<ResMut<vmux_space::ActiveSpace>>,
    mut profile_labels: Query<(&ProfileId, &mut Name), With<ProfileLabel>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let profile_id = vmux_core::profile::sanitize_profile(&request.profile_id);
    if let Err(error) = vmux_core::profile::set_profile_display_name(&profile_id, &request.name) {
        bevy::log::warn!("profile update failed: {error}");
        return;
    }
    let name = request.name.trim().to_string();
    for (id, mut label) in &mut profile_labels {
        if id.0 == profile_id {
            *label = Name::new(name.clone());
        }
    }
    if profile_id != vmux_core::profile::active_profile_name() {
        return;
    }
    for mut profile in &mut space_profiles {
        profile.name.clone_from(&name);
    }
    if let Some(active) = active_record.as_deref_mut() {
        active.record.profile.clone_from(&name);
    }
    if let Ok(entity) = user.single() {
        commands.entity(entity).insert(Profile::user_named(name));
    }
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
    fn team_view_owns_active_profile_and_agent_presentation() {
        let mut app = App::new();
        app.add_message::<AgentCommandRequest>()
            .add_plugins(TeamProjectionPlugin);
        let space = app
            .world_mut()
            .spawn((Space, vmux_core::Active, CurrentSpace))
            .id();
        app.world_mut().spawn((Profile::user(), User));
        app.world_mut().spawn((
            Profile::agent(AgentKind::Codex),
            Agent {
                sid: String::new(),
                kind: Some(AgentKind::Codex),
            },
            ChildOf(space),
        ));
        app.world_mut().spawn((
            ProfileLabel,
            ProfileId("work".to_string()),
            Name::new("Work"),
            vmux_core::Active,
        ));
        let view = app.world_mut().spawn(Team).id();

        app.update();

        let presentation = app.world().get::<TeamPresentation>(view).unwrap();
        assert_eq!(
            presentation
                .0
                .active_profile
                .as_ref()
                .map(|row| row.id.as_str()),
            Some("work")
        );
        assert_eq!(presentation.0.agents.len(), 1);
        assert_eq!(
            presentation.0.agents[0].subtitle,
            vmux_api::team::TeamAgentSubtitle::Role
        );
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
    fn team_page_open_titles_webview_profiles() {
        use vmux_core::page_open::{PageOpenId, PageOpenTask};
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(vmux_layout::native_open::NativeOpenPlugin)
            .add_plugins(HostedPagePlugin::<Team>::default());

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
        app.add_message::<vmux_layout::stack::OpenRequest>()
            .add_plugins(TeamIntentPlugin);
        app
    }

    #[test]
    fn agent_avatar_click_focuses_agent_stack() {
        let mut app = command_app();
        let space = app.world_mut().spawn((Space, CurrentSpace)).id();
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

        app.world_mut().trigger(UiInput::<TeamMemberFocusRequest> {
            webview: Entity::PLACEHOLDER,
            payload: TeamMemberFocusRequest {
                member_id: stack.to_bits().to_string(),
            },
        });
        app.world_mut().flush();

        assert!(app.world().get::<LastActivatedAt>(stack).is_some());
        assert_eq!(lookup(&mut app, space), None);
    }

    #[test]
    fn user_click_reuses_open_team_stack() {
        let mut app = command_app();
        let space = app.world_mut().spawn((Space, CurrentSpace)).id();
        let team = spawn_team_stack(app.world_mut(), space);

        app.world_mut().trigger(UiInput::<TeamOpenRequest> {
            webview: Entity::PLACEHOLDER,
            payload: TeamOpenRequest,
        });
        app.world_mut().flush();

        assert!(app.world().get::<LastActivatedAt>(team).is_some());
    }

    #[test]
    fn acp_agent_appears_in_roster_with_registry_icon() {
        let mut app = App::new();
        let space = app.world_mut().spawn((Space, CurrentSpace)).id();
        app.world_mut().spawn((Profile::user(), User));
        app.world_mut().spawn((
            Profile::registry("Mistral Vibe", "mistral-vibe"),
            Agent {
                sid: "sid-1".to_string(),
                kind: None,
            },
            PageMetadata {
                url: "vmux://sessions/mistral-vibe".to_string(),
                icon: vmux_core::PageIcon::favicon("https://cdn.example/vibe.svg"),
                ..default()
            },
            ChildOf(space),
        ));

        let rows = app
            .world_mut()
            .run_system_once(
                |current_space: Query<Entity, With<CurrentSpace>>,
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
                        current_space.iter().next(),
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
        assert_eq!(agent.url, "vmux://sessions/mistral-vibe");
    }
}
