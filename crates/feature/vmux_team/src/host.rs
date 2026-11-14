use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_app::Plugin;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use vmux_api::avatar::AvatarSpec;
use vmux_api::protocol::{AgentCommandResult, AgentListTeam};
use vmux_api::space::SpacesUiState;
use vmux_ecs::agent::{
    AgentCommandResponse, AgentReply, AgentRequestAppExt, AgentRequestMessage, AgentRequestRouteSet,
};
use vmux_ecs::event::team::{
    ProfileRow, TeamMemberFocusRequest, TeamMemberRow, TeamOpenRequest, TeamProfileCreateRequest,
    TeamProfileError, TeamProfileForm, TeamProfileFormCloseRequest, TeamProfileFormInputRequest,
    TeamProfileFormOpenRequest, TeamProfileSwitchRequest, TeamProfileUpdateRequest,
    TeamProfilesRetryRequest, TeamUiState,
};
use vmux_ecs::notify::AgentDoneUnseen;
use vmux_ecs::page::PageReady;
use vmux_ecs::profile::{
    ActiveProfile, Profile as StoredProfile, ProfileId, ProfileLabel, SessionEnvironment,
};
use vmux_ecs::team::{Agent, Profile, Tester, User};
use vmux_ecs::{ActivateRequest, Active, EntityTarget, PageMetadata};
use vmux_ecs::{UiStatePlugin, UiStateWrite};
use vmux_layout::cef::LayoutCef;
use vmux_layout::hosted_page::HostedUiPlugin;
use vmux_layout::profile::Profile as SpaceProfile;
use vmux_layout::projection::TeamProjection as LayoutTeamProjection;
use vmux_layout::space::{CurrentSpace, Space, SpaceHierarchy};
use vmux_layout::stack::{OpenRequest, Stack};
use vmux_layout::window::WindowHierarchy;
use vmux_session::{RunState, Session};
use vmux_space::Spaces;

use crate::projection::TeamStateProjection;
use uuid::Uuid;
use vmux_account::{CreateRequest, MutationCompleted, RenameRequest};

#[vmux_page::page]
pub struct TeamPlugin;

impl Plugin for TeamPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::TeamPage::plugin());
        app.add_plugins((
            HostedUiPlugin::<Team>::new(Self::MANIFEST),
            ProjectionPlugin,
            IntentPlugin,
            crate::TeamToolPlugin,
        ))
        .add_agent_request::<AgentRenameProfile>()
        .add_systems(Startup, spawn_user_profile);
    }
}

struct ProjectionPlugin;

impl Plugin for ProjectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiStatePlugin::<TeamUiState>::default())
            .add_observer(replay)
            .add_systems(
                Update,
                (sync_profiles, sync_user_profile_name, project, publish)
                    .chain()
                    .after(vmux_account::Set::Publish),
            )
            .add_systems(Update, list.after(AgentRequestRouteSet))
            .add_agent_request::<AgentListTeam>();
    }
}

struct IntentPlugin;

impl Plugin for IntentPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ProfileSwitchRequested>()
            .add_plugins(UiEventPlugin::<(
                TeamOpenRequest,
                TeamMemberFocusRequest,
                TeamProfileCreateRequest,
                TeamProfileSwitchRequest,
                TeamProfileUpdateRequest,
                TeamProfileFormOpenRequest,
                TeamProfileFormInputRequest,
                TeamProfileFormCloseRequest,
                TeamProfilesRetryRequest,
            )>::default())
            .add_observer(open_request)
            .add_observer(member_focus_request)
            .add_observer(profile_create_request)
            .add_observer(profile_switch_request)
            .add_observer(profile_update_request)
            .add_observer(open_form)
            .add_observer(edit_form)
            .add_observer(close_form)
            .add_observer(retry_profiles)
            .add_systems(Update, rename_from_agent.after(AgentRequestRouteSet))
            .add_systems(
                Update,
                (complete_profile, complete_agent).after(vmux_account::Set::Reply),
            );
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct ProfileSwitchRequested {
    pub profile_id: String,
}

#[vmux_api::agent]
pub struct AgentRenameProfile {
    pub name: String,
}

#[derive(Component, Default)]
struct Team;

#[derive(Component, Clone, Debug, Default, PartialEq)]
struct TeamPresentation(TeamUiState);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct ProfileFormState(TeamProfileForm);

fn spawn_user_profile(mut commands: Commands) {
    let mut identity = commands.spawn((Profile::user(), User, Name::new("Profile: User")));
    if SessionEnvironment::is_test() {
        identity.insert(Tester);
    }
}

fn sync_user_profile_name(
    current: vmux_ecs::profile::CurrentProfile,
    mut user: Query<&mut Profile, With<User>>,
) {
    let Some(name) = current.label() else {
        return;
    };
    let Ok(mut profile) = user.single_mut() else {
        return;
    };
    if profile.name != name {
        *profile = Profile::user_named(name.to_string());
    }
}

#[allow(clippy::too_many_arguments)]
#[derive(SystemParam)]
struct TeamProjector<'w, 's> {
    current_space: Query<'w, 's, Entity, With<CurrentSpace>>,
    active_spaces: Query<'w, 's, Entity, (With<Space>, With<Active>)>,
    user: Query<'w, 's, (Entity, &'static Profile), With<User>>,
    agents: Query<
        'w,
        's,
        (
            Entity,
            &'static Profile,
            &'static Agent,
            Option<&'static RunState>,
            Option<&'static AgentDoneUnseen>,
        ),
    >,
    child_of: Query<'w, 's, &'static ChildOf>,
    window_hierarchy: WindowHierarchy<'w, 's>,
    space_hierarchy: SpaceHierarchy<'w, 's>,
    metadata: Query<'w, 's, &'static PageMetadata>,
    children: Query<'w, 's, &'static Children>,
    session_views: Query<'w, 's, (Entity, &'static EntityTarget<Session>)>,
    profile_labels:
        Query<'w, 's, (&'static ProfileId, &'static Name, Has<Active>), With<ProfileLabel>>,
}

impl TeamProjector<'_, '_> {
    fn current(&self) -> Option<Entity> {
        self.current_space.iter().next()
    }

    fn target(&self, entity: Entity) -> Option<Entity> {
        self.space_hierarchy.get(entity).or_else(|| {
            let window = self.window_hierarchy.get(entity)?;
            self.active_spaces
                .iter()
                .find(|space| self.window_hierarchy.get(*space) == Some(window))
        })
    }

    fn page(&self, entity: Entity) -> (String, String, String) {
        let mut candidates = vec![entity];
        for (stack, target) in &self.session_views {
            if target.entity() != entity {
                continue;
            }
            candidates.push(stack);
            if let Ok(children) = self.children.get(stack) {
                candidates.extend(children.iter());
            }
        }
        if let Ok(children) = self.children.get(entity) {
            candidates.extend(children.iter());
        }
        if let Ok(parent) = self.child_of.get(entity) {
            let stack = parent.parent();
            candidates.push(stack);
            if let Ok(children) = self.children.get(stack) {
                candidates.extend(children.iter());
            }
        }
        let mut icon = String::new();
        let mut url = String::new();
        let mut title = String::new();
        for candidate in candidates {
            if let Ok(metadata) = self.metadata.get(candidate) {
                if icon.is_empty() && !metadata.icon.favicon_url().is_empty() {
                    icon = metadata.icon.favicon_url().to_string();
                }
                if url.is_empty() && !metadata.url.is_empty() {
                    url = metadata.url.clone();
                }
                if title.is_empty() && !metadata.title.is_empty() {
                    title = metadata.title.clone();
                }
            }
        }
        (icon, url, title)
    }

    fn belongs_to(&self, entity: Entity, space: Entity) -> bool {
        if self.space_hierarchy.get(entity) == Some(space) {
            return true;
        }
        self.session_views.iter().any(|(stack, target)| {
            target.entity() == entity && self.space_hierarchy.get(stack) == Some(space)
        })
    }

    fn members(&self, active_space: Option<Entity>) -> Vec<TeamMemberRow> {
        let mut members = Vec::new();
        if let Ok((entity, profile)) = self.user.single() {
            members.push(TeamMemberRow {
                id: entity.to_bits().to_string(),
                name: profile.name.clone(),
                initials: profile.avatar.initials.clone(),
                color: profile.avatar.color.clone(),
                is_user: true,
                ..Default::default()
            });
        }
        let Some(active_space) = active_space else {
            return members;
        };
        for (entity, profile, agent, run, done) in &self.agents {
            if !self.belongs_to(entity, active_space) {
                continue;
            }
            let (icon, url, title) = self.page(entity);
            members.push(TeamMemberRow {
                id: entity.to_bits().to_string(),
                name: profile.name.clone(),
                initials: profile.avatar.initials.clone(),
                color: profile.avatar.color.clone(),
                icon,
                url,
                title,
                sid: agent.sid.clone(),
                is_user: false,
                is_running: matches!(run, Some(RunState::Streaming)),
                is_done_unseen: done.is_some(),
            });
        }
        members
    }

    fn profiles(&self) -> Vec<ProfileRow> {
        let mut profiles = Vec::new();
        for (id, name, is_active) in &self.profile_labels {
            profiles.push(ProfileRow {
                id: id.0.clone(),
                name: name.as_str().to_string(),
                color: AvatarSpec::color(&id.0),
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
}

fn list(
    mut reader: MessageReader<AgentRequestMessage<AgentListTeam>>,
    projector: TeamProjector,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in reader.read() {
        let members = projector.members(projector.current());
        let result = match serde_json::to_string(&members) {
            Ok(json) => AgentCommandResult::Text(json),
            Err(error) => AgentCommandResult::Error(format!("list_team: {error}")),
        };
        responses.write(request.reply.response(result));
    }
}

fn project(
    views: Query<Entity, Or<(With<LayoutCef>, With<Team>, With<Spaces>)>>,
    presentations: Query<&TeamPresentation>,
    forms: Query<&ProfileFormState>,
    projector: TeamProjector,
    mut commands: Commands,
) {
    for entity in &views {
        let target_space = projector.target(entity);
        let mut state = TeamStateProjection::build(
            projector.members(target_space.or_else(|| projector.current())),
            projector.profiles(),
        );
        state.profile_form = forms.get(entity).ok().map(|form| form.0.clone());
        let presentation = TeamPresentation(state);
        if presentations
            .get(entity)
            .is_ok_and(|current| current == &presentation)
        {
            continue;
        }
        commands.entity(entity).insert(presentation);
    }
}

fn publish(
    presentations: Query<(Entity, &TeamPresentation), Changed<TeamPresentation>>,
    team_views: Query<(), With<Team>>,
    spaces_views: Query<(), With<Spaces>>,
    layout_cefs: Query<(), With<LayoutCef>>,
    mut commands: Commands,
) {
    for (entity, presentation) in &presentations {
        if team_views.contains(entity) {
            commands.trigger(UiStateWrite::<TeamUiState>::from_event(
                entity,
                &presentation.0,
            ));
        }
        if spaces_views.contains(entity) {
            commands.trigger(UiStateWrite::<SpacesUiState>::from_event(
                entity,
                &presentation.0,
            ));
        }
        if layout_cefs.contains(entity) {
            commands
                .entity(entity)
                .insert(LayoutTeamProjection(presentation.0.clone()));
        }
    }
}

fn replay(
    trigger: On<UiInput<PageReady>>,
    presentations: Query<&TeamPresentation>,
    team_views: Query<(), With<Team>>,
    spaces_views: Query<(), With<Spaces>>,
    layout_cefs: Query<(), With<LayoutCef>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(presentation) = presentations.get(entity) else {
        return;
    };
    if team_views.contains(entity) {
        commands.trigger(UiStateWrite::<TeamUiState>::from_event(
            entity,
            &presentation.0,
        ));
    }
    if spaces_views.contains(entity) {
        commands.trigger(UiStateWrite::<SpacesUiState>::from_event(
            entity,
            &presentation.0,
        ));
    }
    if layout_cefs.contains(entity) {
        commands
            .entity(entity)
            .insert(LayoutTeamProjection(presentation.0.clone()));
    }
}

fn open_team_stack_in_space(
    space: Entity,
    stacks: &Query<(Entity, &PageMetadata), With<Stack>>,
    hierarchy: &SpaceHierarchy,
) -> Option<Entity> {
    stacks.iter().find_map(|(stack, meta)| {
        (meta.url == TeamPlugin::URL && hierarchy.get(stack) == Some(space)).then_some(stack)
    })
}

fn parse_member_entity(member_id: &str) -> Option<Entity> {
    let bits = member_id.parse::<u64>().ok()?;
    Entity::try_from_bits(bits)
}

fn open_request(
    _trigger: On<UiInput<TeamOpenRequest>>,
    mut stack_requests: MessageWriter<OpenRequest>,
    current_space: Query<Entity, With<CurrentSpace>>,
    stacks: Query<(Entity, &PageMetadata), With<Stack>>,
    hierarchy: SpaceHierarchy,
    mut commands: Commands,
) {
    if let Some(space) = current_space.iter().next()
        && let Some(stack) = open_team_stack_in_space(space, &stacks, &hierarchy)
    {
        commands.trigger(ActivateRequest { entity: stack });
        return;
    }

    stack_requests.write(OpenRequest {
        url: Some(TeamPlugin::URL.to_string()),
        policy: vmux_layout::stack::OpenPolicy::ReplaceStartup,
    });
}

fn member_focus_request(
    trigger: On<UiInput<TeamMemberFocusRequest>>,
    agents: Query<Entity, With<Agent>>,
    mut commands: Commands,
) {
    let Some(entity) = parse_member_entity(&trigger.event().payload.member_id) else {
        return;
    };
    if agents.get(entity).is_ok() {
        commands.trigger(ActivateRequest { entity });
    }
}

fn profile_create_request(
    trigger: On<UiInput<TeamProfileCreateRequest>>,
    binding: AccountBinding,
    mut forms: Query<&mut ProfileFormState>,
    pending: Query<(), With<PendingProfile>>,
    mut requests: MessageWriter<CreateRequest>,
    mut commands: Commands,
) {
    let view = trigger.event().webview;
    if pending.contains(view) || forms.get(view).is_ok_and(|form| form.0.pending) {
        return;
    }
    let Some(root) = binding.root() else {
        if let Ok(mut form) = forms.get_mut(view) {
            form.0.error = Some(TeamProfileError::Unavailable);
        }
        return;
    };
    let id = Uuid::new_v4();
    let name = trigger.event().payload.name.trim().to_string();
    if let Ok(mut form) = forms.get_mut(view) {
        form.0.pending = true;
        form.0.error = None;
    }
    commands.entity(view).insert(PendingProfile {
        root,
        id,
        submitted: name.clone(),
    });
    requests.write(CreateRequest {
        root,
        operation_id: id,
        name,
    });
}

fn profile_switch_request(
    trigger: On<UiInput<TeamProfileSwitchRequest>>,
    active: Query<&ActiveProfile>,
    profile_labels: Query<&ProfileId, With<ProfileLabel>>,
    mut profile_switches: MessageWriter<ProfileSwitchRequested>,
) {
    let profile_id = StoredProfile::named(&trigger.event().payload.profile_id).into_id();
    let Ok(active) = active.single() else {
        return;
    };
    if profile_id == active.0.id() {
        return;
    }
    if profile_labels.iter().any(|id| id.0 == profile_id) {
        profile_switches.write(ProfileSwitchRequested { profile_id });
    }
}

fn profile_update_request(
    trigger: On<UiInput<TeamProfileUpdateRequest>>,
    binding: AccountBinding,
    mut forms: Query<&mut ProfileFormState>,
    pending: Query<(), With<PendingProfile>>,
    mut requests: MessageWriter<RenameRequest>,
    mut commands: Commands,
) {
    let view = trigger.event().webview;
    if pending.contains(view) || forms.get(view).is_ok_and(|form| form.0.pending) {
        return;
    }
    let Some((root, profile_id)) = binding.profile(&trigger.event().payload.profile_id) else {
        if let Ok(mut form) = forms.get_mut(view) {
            form.0.error = Some(TeamProfileError::Unavailable);
        }
        return;
    };
    let id = Uuid::new_v4();
    let name = trigger.event().payload.name.trim().to_string();
    if let Ok(mut form) = forms.get_mut(view) {
        form.0.pending = true;
        form.0.error = None;
    }
    commands.entity(view).insert(PendingProfile {
        root,
        id,
        submitted: name.clone(),
    });
    requests.write(RenameRequest {
        root,
        operation_id: id,
        profile_id,
        name,
    });
}

fn open_form(trigger: On<UiInput<TeamProfileFormOpenRequest>>, mut commands: Commands) {
    commands
        .entity(trigger.event().webview)
        .insert(ProfileFormState(TeamProfileForm {
            profile_id: trigger.event().payload.profile_id.clone(),
            draft: trigger.event().payload.draft.clone(),
            pending: false,
            error: None,
        }))
        .remove::<PendingProfile>();
}

fn edit_form(
    trigger: On<UiInput<TeamProfileFormInputRequest>>,
    mut forms: Query<&mut ProfileFormState>,
) {
    let Ok(mut form) = forms.get_mut(trigger.event().webview) else {
        return;
    };
    form.0.draft.clone_from(&trigger.event().payload.draft);
    form.0.error = None;
}

fn close_form(trigger: On<UiInput<TeamProfileFormCloseRequest>>, mut commands: Commands) {
    commands
        .entity(trigger.event().webview)
        .remove::<ProfileFormState>()
        .remove::<PendingProfile>();
}

fn retry_profiles(
    _trigger: On<UiInput<TeamProfilesRetryRequest>>,
    roots: Query<Entity, (With<vmux_account::Root>, With<Active>)>,
    mut requests: MessageWriter<vmux_account::RetryRequest>,
) {
    if let Ok(root) = roots.single() {
        requests.write(vmux_account::RetryRequest(root));
    }
}

#[derive(Component)]
struct PendingProfile {
    root: Entity,
    id: Uuid,
    submitted: String,
}

#[derive(Component)]
struct PendingAgent {
    root: Entity,
    id: Uuid,
    reply: AgentReply,
}

#[derive(SystemParam)]
struct AccountBinding<'w, 's> {
    roots: Query<
        'w,
        's,
        Entity,
        (
            With<vmux_account::Root>,
            With<Active>,
            With<vmux_ecs::Ready>,
        ),
    >,
    accounts: Query<'w, 's, &'static ChildOf, With<vmux_account::Account>>,
    profiles: Query<
        'w,
        's,
        (
            &'static vmux_ecs::Uuid,
            &'static ProfileId,
            &'static Name,
            &'static vmux_account::Owner,
        ),
        With<vmux_account::Profile>,
    >,
}

impl AccountBinding<'_, '_> {
    fn root(&self) -> Option<Entity> {
        self.roots.single().ok()
    }

    fn profile(&self, directory: &str) -> Option<(Entity, Uuid)> {
        let root = self.root()?;
        for (id, profile_id, _, owner) in &self.profiles {
            if profile_id.0 != directory {
                continue;
            }
            let parent = self.accounts.get(owner.0).ok()?;
            if parent.parent() == root {
                return Some((root, id.0.parse().ok()?));
            }
        }
        None
    }

    fn label(&self, root: Entity, directory: &str) -> Option<&str> {
        if self.root()? != root {
            return None;
        }
        for (_, profile_id, name, owner) in &self.profiles {
            if profile_id.0 != directory {
                continue;
            }
            let parent = self.accounts.get(owner.0).ok()?;
            if parent.parent() == root {
                return Some(name.as_str());
            }
        }
        None
    }
}

fn rename_from_agent(
    mut requests: MessageReader<AgentRequestMessage<AgentRenameProfile>>,
    active: Query<&ActiveProfile>,
    binding: AccountBinding,
    mut renames: MessageWriter<RenameRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
    mut commands: Commands,
) {
    let Ok(active) = active.single() else { return };
    for request in requests.read() {
        let Some((root, profile_id)) = binding.profile(active.0.id()) else {
            responses.write(
                request
                    .reply
                    .response(AgentCommandResult::Error("Profiles are unavailable".into())),
            );
            continue;
        };
        let id = Uuid::new_v4();
        commands.spawn((
            PendingAgent {
                root,
                id,
                reply: request.reply,
            },
            ChildOf(root),
        ));
        renames.write(RenameRequest {
            root,
            operation_id: id,
            profile_id,
            name: request.payload.name.clone(),
        });
    }
}

fn complete_profile(
    mut events: MessageReader<MutationCompleted>,
    mut views: Query<(Entity, &PendingProfile, Option<&mut ProfileFormState>)>,
    binding: AccountBinding,
    mut switches: MessageWriter<ProfileSwitchRequested>,
    mut commands: Commands,
) {
    for event in events.read() {
        for (entity, pending, form) in &mut views {
            if pending.id != event.operation_id || pending.root != event.root {
                continue;
            }
            commands.entity(entity).remove::<PendingProfile>();
            if let Some(mut form) = form {
                form.0.pending = false;
                match &event.result {
                    Ok(_) if form.0.draft.trim() == pending.submitted => {
                        commands.entity(entity).remove::<ProfileFormState>();
                    }
                    Ok(_) => {
                        form.0.error = None;
                    }
                    Err(failure) => {
                        form.0.error = Some(match failure {
                            vmux_account::Failure::InvalidName => TeamProfileError::InvalidName,
                            vmux_account::Failure::Conflict => TeamProfileError::Conflict,
                            vmux_account::Failure::UnknownProfile
                            | vmux_account::Failure::NotReady => TeamProfileError::Unavailable,
                            _ => TeamProfileError::SaveFailed,
                        });
                    }
                }
            }
            if event.created
                && event.result.is_ok()
                && binding.root() == Some(event.root)
                && let Some(directory) = &event.directory_id
            {
                switches.write(ProfileSwitchRequested {
                    profile_id: directory.clone(),
                });
            }
        }
    }
}

fn complete_agent(
    mut events: MessageReader<MutationCompleted>,
    agents: Query<(Entity, &PendingAgent)>,
    mut responses: MessageWriter<AgentCommandResponse>,
    mut commands: Commands,
) {
    for event in events.read() {
        for (entity, pending) in &agents {
            if pending.id != event.operation_id || pending.root != event.root {
                continue;
            }
            let result = match &event.result {
                Ok(_) => AgentCommandResult::Ok,
                Err(error) => {
                    AgentCommandResult::Error(format!("Profile update failed: {error:?}"))
                }
            };
            responses.write(pending.reply.response(result));
            commands.entity(entity).despawn();
        }
    }
}

fn sync_profiles(
    active: Query<&ActiveProfile>,
    binding: AccountBinding,
    mut space_profiles: Query<&mut SpaceProfile, With<Space>>,
) {
    let (Ok(active), Some(root)) = (active.single(), binding.root()) else {
        return;
    };
    let Some(name) = binding.label(root, active.0.id()) else {
        return;
    };
    for mut profile in &mut space_profiles {
        if profile.name != name {
            profile.name = name.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_ecs::LastActivatedAt;

    use vmux_ecs::PageNavigation;
    use vmux_ecs::page_open::{PageOpenId, PageOpenTask};

    fn spawn_team_stack(world: &mut World, space: Entity) -> Entity {
        world
            .spawn((
                Stack::default(),
                PageMetadata {
                    url: TeamPlugin::URL.to_string(),
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
                      hierarchy: SpaceHierarchy| {
                    open_team_stack_in_space(space, &stacks, &hierarchy)
                },
            )
            .unwrap()
    }

    #[test]
    fn done_unseen_sets_row_flag() {
        let mut app = App::new();
        let space = app.world_mut().spawn((Space, CurrentSpace)).id();
        app.world_mut().spawn((Profile::user(), User));
        app.world_mut().spawn((
            Profile::registry("Claude", "claude-acp"),
            Agent { sid: String::new() },
            AgentDoneUnseen,
            ChildOf(space),
        ));
        let rows = app
            .world_mut()
            .run_system_once(|projector: TeamProjector| projector.members(projector.current()))
            .unwrap();
        assert!(rows.iter().any(|row| !row.is_user && row.is_done_unseen));
    }

    #[test]
    fn user_profile_name_does_not_follow_space_changes() {
        let mut app = App::new();
        app.add_systems(Update, sync_user_profile_name);
        app.world_mut().spawn((
            vmux_ecs::profile::ActiveProfile(vmux_ecs::profile::Profile::current()),
            Name::new("Personal"),
            vmux_ecs::profile::ProfileDirectories(vmux_ecs::profile::ProfilePaths::current()),
        ));
        let user = app
            .world_mut()
            .spawn((Profile::user_named("Old".to_string()), User))
            .id();
        app.world_mut().spawn((
            Space,
            CurrentSpace,
            SpaceProfile {
                name: String::new(),
            },
        ));

        app.update();

        assert_eq!(app.world().get::<Profile>(user).unwrap().name, "Personal");
    }

    #[test]
    fn space_profile_labels_follow_restored_and_refreshed_canonical_names() {
        let mut app = App::new();
        app.add_plugins(ProjectionPlugin);
        let root = app
            .world_mut()
            .spawn((vmux_account::Root, Active, vmux_ecs::Ready))
            .id();
        let account = app
            .world_mut()
            .spawn((vmux_account::Account, ChildOf(root)))
            .id();
        app.world_mut()
            .spawn(ActiveProfile(StoredProfile::named("personal")));
        let canonical = app
            .world_mut()
            .spawn((
                vmux_account::Profile,
                vmux_ecs::Uuid(Uuid::new_v4().to_string()),
                ProfileId("personal".into()),
                Name::new("Restored"),
                vmux_account::Owner(account),
            ))
            .id();
        let space = app
            .world_mut()
            .spawn((
                Space,
                SpaceProfile {
                    name: "Legacy".into(),
                },
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<SpaceProfile>(space).unwrap().name,
            "Restored"
        );
        app.world_mut()
            .entity_mut(canonical)
            .insert(Name::new("Refreshed"));
        let later = app
            .world_mut()
            .spawn((
                Space,
                SpaceProfile {
                    name: "Legacy".into(),
                },
            ))
            .id();
        app.update();
        for entity in [space, later] {
            assert_eq!(
                app.world().get::<SpaceProfile>(entity).unwrap().name,
                "Refreshed"
            );
        }
    }

    #[test]
    fn team_view_owns_active_profile_and_agent_presentation() {
        let mut app = App::new();
        app.add_plugins(ProjectionPlugin);
        let space = app
            .world_mut()
            .spawn((Space, vmux_ecs::Active, CurrentSpace))
            .id();
        app.world_mut().spawn((Profile::user(), User));
        app.world_mut().spawn((
            Profile::registry("Codex", "codex-acp"),
            Agent { sid: String::new() },
            ChildOf(space),
        ));
        app.world_mut().spawn((
            ProfileLabel,
            ProfileId("work".to_string()),
            Name::new("Work"),
            vmux_ecs::Active,
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
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(vmux_layout::hosted_page::HostedPagePlugin)
            .add_plugins(HostedUiPlugin::<Team>::new(TeamPlugin::MANIFEST));

        let navigation = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), PageNavigation::new(navigation)))
            .id();
        app.world_mut().spawn(PageOpenTask {
            id: navigation,
            stack,
            url: TeamPlugin::URL.to_string(),
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
            .add_plugins(IntentPlugin);
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
            },
            PageMetadata {
                url: "vmux://sessions/mistral-vibe".to_string(),
                icon: vmux_api::PageIcon::favicon("https://cdn.example/vibe.svg"),
                ..default()
            },
            ChildOf(space),
        ));

        let rows = app
            .world_mut()
            .run_system_once(|projector: TeamProjector| projector.members(projector.current()))
            .unwrap();

        let agent = rows
            .iter()
            .find(|r| !r.is_user)
            .expect("acp agent in roster");
        assert_eq!(agent.name, "Mistral Vibe");
        assert_eq!(agent.icon, "https://cdn.example/vibe.svg");
        assert_eq!(agent.url, "vmux://sessions/mistral-vibe");
    }

    #[test]
    fn session_agent_appears_in_the_space_that_contains_its_view() {
        let mut app = App::new();
        let space = app.world_mut().spawn((Space, CurrentSpace)).id();
        app.world_mut().spawn((Profile::user(), User));
        let session = app
            .world_mut()
            .spawn((
                Session,
                Profile::registry("Codex", "codex-acp"),
                Agent {
                    sid: "session-1".into(),
                },
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                EntityTarget::<Session>::new(session),
                PageMetadata {
                    url: "vmux://sessions/session-1".into(),
                    icon: vmux_api::PageIcon::favicon("https://cdn.example/codex.svg"),
                    ..default()
                },
                ChildOf(space),
            ))
            .id();
        app.world_mut().spawn((
            PageMetadata {
                url: "vmux://sessions/session-1".into(),
                title: "Task".into(),
                ..default()
            },
            ChildOf(stack),
        ));

        let rows = app
            .world_mut()
            .run_system_once(|projector: TeamProjector| projector.members(projector.current()))
            .unwrap();

        let agent = rows.iter().find(|row| !row.is_user).unwrap();
        assert_eq!(agent.name, "Codex");
        assert_eq!(agent.url, "vmux://sessions/session-1");
        assert_eq!(agent.title, "Task");
    }
}
