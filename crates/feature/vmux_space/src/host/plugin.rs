use bevy::prelude::{Commands, MessageWriter, On, Query, Res, With};
use bevy::{ecs::message::MessageReader, prelude::*};
use bevy_cef::prelude::*;
use std::option::Option;
use vmux_command::{
    CommandBarDismiss, CommandBarOpenRequest, ResolvedLocale, WriteCommandBarRequests,
};
use vmux_command::{CommandInvocation, CommandRuntimePlugin, ReadCommandRequests};
use vmux_ecs::page::{PageReady, SpacesPageSpawnRequest, StartupPageUrl};
use vmux_ecs::{
    ActivateRequest, Active, EffectiveStartupUrl, Order, PageMetadata, PageOpenRequest,
    PageOpenTarget,
};
use vmux_ecs::{CommandBarContribution, CommandBarContributionActivated, CommandBarQueryChanged};
use vmux_ecs::{UiStateWrite, store::WorkspaceRestore};
#[cfg(test)]
use vmux_feature::manifest::FeatureManifest;
use vmux_history::LastActivatedAt;
use vmux_layout::hosted_page::HostedUiPlugin;
use vmux_layout::projection::SpacesProjection;
#[cfg(test)]
use vmux_layout::space::CurrentSpace;
use vmux_layout::space::{
    CurrentSpaceSet, EffectiveStartupDir, EffectiveStartupSet, FocusedSpace, Space, SpaceId,
    SpaceOfPane,
};
use vmux_layout::stack::{FocusedStack, Stack};
use vmux_layout::state::LayoutUiState;
use vmux_layout::tab::Tab;
use vmux_layout::window::{FocusedWindow, Main, WindowHierarchy};
use vmux_layout::{
    LayoutCef, LayoutContractPlugin, LayoutStartupSet, LayoutUiStateUpdates, TabLayoutSpawnContent,
    TabLayoutSpawnRequest,
};
#[cfg(test)]
use vmux_setting::{AgentSettings, SpaceOverrides, SpaceProject, TerminalSettings};
use vmux_setting::{AppSettings, SettingsLoadSet, SettingsSaveRequest};

use super::SpacesUiStateUpdates;
use super::agent::SpaceAgentPlugin;
use super::identity::SpaceIdentity;
use super::spaces::{SpaceSelection, Spaces, SpacesPageSnapshot};
use crate::event::{
    ProjectActivateRequest, ProjectForgetRequest, ProjectPickerRequest, SpaceAttachRequest,
    SpaceCreateRequest, SpaceDeleteRequest, SpaceFormCloseRequest, SpaceFormInputRequest,
    SpaceFormOpenRequest, SpaceFormRegenerateRequest, SpaceFormState, SpaceOpenPageRequest,
    SpaceRenameRequest, SpaceRow, SpacesListEvent, SpacesUiState,
};
use crate::model::BOOTSTRAP_SPACE_ID;
use vmux_api::command_bar::{CommandBarPicker, OpenRequest as CommandBarOpenUrlRequest};
use vmux_ui::i18n::{Locale, TranslationValue, translate, translate_with};

#[vmux_page::page]
pub struct SpacePlugin;

const PICKER_ID: &str = "space";

impl Plugin for SpacePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::SpacesPage::plugin());
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_plugins((
            SpaceAgentPlugin,
            super::composer::SpaceComposerPlugin,
            super::store::WorkspaceStorePlugin,
        ))
        .add_plugins(super::SpaceToolPlugin)
        .add_plugins(LayoutContractPlugin)
        .add_plugins(vmux_ecs::UiStatePlugin::<SpacesUiState>::default())
        .add_plugins(vmux_ecs::UiStatePlugin::<vmux_ecs::event::ProjectNavigation>::default())
        .add_message::<CommandBarOpenRequest>()
        .add_message::<SpaceAttachRequest>()
        .add_message::<SpaceCreateRequest>()
        .add_message::<SpaceDeleteRequest>()
        .add_message::<SpaceOpenPageRequest>()
        .add_message::<SpaceRenameRequest>()
        .add_message::<OpenRequest>()
        .add_message::<CommandBarSpaceOpenRequest>()
        .add_systems(Update, open_command_bar.in_set(WriteCommandBarRequests))
        .add_systems(
            Update,
            (
                relay_requests::<SpaceAttachRequest>,
                relay_requests::<SpaceCreateRequest>,
                relay_requests::<SpaceDeleteRequest>,
                relay_requests::<SpaceOpenPageRequest>,
                relay_requests::<SpaceRenameRequest>,
            ),
        )
        .add_systems(
            Update,
            update_effective_startup
                .in_set(EffectiveStartupSet)
                .after(CurrentSpaceSet)
                .before(ReadCommandRequests),
        )
        .add_systems(
            Startup,
            (ensure_bootstrap, ApplyDeferred, update_effective_startup)
                .chain()
                .after(SettingsLoadSet)
                .after(LayoutStartupSet::Store)
                .before(LayoutStartupSet::DefaultTab),
        )
        .add_message::<SpacesPageSpawnRequest>()
        .add_systems(Update, respond_spaces_spawn.in_set(ReadCommandRequests))
        .add_plugins((
            HostedUiPlugin::<Spaces>::new(Self::manifest()),
            super::key::SpaceKeyPlugin,
            super::project::SpaceProjectPlugin,
            super::snapshot_updater::SnapshotPlugin,
            UiEventPlugin::<(
                SpaceAttachRequest,
                SpaceCreateRequest,
                SpaceDeleteRequest,
                SpaceOpenPageRequest,
                SpaceRenameRequest,
                SpaceFormOpenRequest,
                SpaceFormInputRequest,
                SpaceFormRegenerateRequest,
                SpaceFormCloseRequest,
            )>::default(),
            UiEventPlugin::<(
                ProjectActivateRequest,
                ProjectForgetRequest,
                ProjectPickerRequest,
                vmux_ecs::event::ProjectTreeToggle,
            )>::default(),
        ))
        .add_observer(attach)
        .add_observer(create)
        .add_observer(delete)
        .add_observer(open_page)
        .add_observer(rename)
        .add_observer(open_form)
        .add_observer(edit_form)
        .add_observer(regenerate_form)
        .add_observer(close_form)
        .add_observer(project_activate)
        .add_observer(project_forget)
        .add_observer(contribute)
        .add_observer(activate_contribution)
        .add_observer(reset_sent)
        .add_systems(Update, handle_open_in_new.in_set(ReadCommandRequests))
        .add_systems(Update, open_requested_form)
        .add_systems(Update, broadcast_spaces_to_views);
    }
}

fn ensure_bootstrap(
    spaces: Query<(), With<Space>>,
    restore: Query<&WorkspaceRestore>,
    profile: vmux_ecs::profile::CurrentProfile,
    identity: SpaceIdentity,
    mut commands: Commands,
) {
    if !spaces.is_empty() || restore.single().is_ok_and(|restore| restore.store_present) {
        return;
    }
    commands.spawn((
        Space,
        SpaceId(BOOTSTRAP_SPACE_ID.into()),
        Name::new(identity.suggested(&[])),
        vmux_layout::profile::Profile {
            name: profile.label().unwrap_or("Personal").to_string(),
        },
    ));
}

#[vmux_command::command(message)]
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct OpenRequest {
    pub url: Option<String>,
}

#[vmux_command::command(message)]
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
struct CommandBarSpaceOpenRequest;

impl TryFrom<&CommandInvocation> for CommandBarSpaceOpenRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "space_open").then_some(Self).ok_or(())
    }
}

impl TryFrom<&CommandInvocation> for OpenRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "open_in_new_space")
            .then(|| Self {
                url: invocation.argument("url"),
            })
            .ok_or(())
    }
}

fn open_command_bar(
    mut requests: MessageReader<CommandBarSpaceOpenRequest>,
    locale: Option<Res<ResolvedLocale>>,
    mut open: MessageWriter<CommandBarOpenRequest>,
) {
    if requests.read().count() > 0 {
        let locale = locale
            .as_deref()
            .map(|locale| locale.0.clone())
            .unwrap_or_else(Locale::preferred);
        open.write(CommandBarOpenRequest::picker_with(
            CommandBarPicker::new(PICKER_ID),
            Vec::new(),
            String::new(),
            locale.translate("command-switch-space"),
            false,
            true,
        ));
    }
}

#[derive(Component)]
struct SpaceContribution(String);

#[derive(Component)]
struct ManageSpacesContribution;

fn contribute(
    trigger: On<CommandBarQueryChanged>,
    existing: Query<
        (Entity, &ChildOf),
        Or<(With<SpaceContribution>, With<ManageSpacesContribution>)>,
    >,
    rows: SpaceRows,
    space_window: SpaceWindow,
    mut commands: Commands,
) {
    let request = trigger.event();
    for (entity, parent) in &existing {
        if parent.parent() == request.target {
            commands.entity(entity).despawn();
        }
    }

    let picker = request
        .picker
        .as_ref()
        .is_some_and(|picker| picker.is(PICKER_ID));
    if request.picker.is_some() && !picker {
        return;
    }
    let query = request.query.trim();
    let page = query == SpacePlugin::url()
        || query == SpacePlugin::url().trim_end_matches('/')
        || query.starts_with(SpacePlugin::url());
    if !picker
        && !page
        && (query.is_empty()
            || query.starts_with('>')
            || vmux_path::NavigationText::new(query).looks_like_path())
    {
        return;
    }

    let main = space_window
        .for_webview(request.target)
        .map(|(_, main)| main);
    let rows = rows.list(main);
    let needle = query.to_ascii_lowercase();
    let numbered = picker && request.numbered;
    let mut rank = 0;
    for row in rows {
        if !picker
            && !page
            && !row.name.to_ascii_lowercase().contains(&needle)
            && !row.id.to_ascii_lowercase().contains(&needle)
            && !row.profile.to_ascii_lowercase().contains(&needle)
        {
            continue;
        }
        commands.spawn((
            Name::new(format!("Space command-bar row: {}", row.id)),
            CommandBarContribution {
                row: vmux_api::command_bar::CommandBarResultItem {
                    key: format!("space:{}", row.id),
                    leading: if numbered {
                        rank.to_string()
                    } else {
                        String::new()
                    },
                    title: row.name,
                    subtitle: row.profile,
                    trailing: translate_with(
                        "command-tabs",
                        &[("count", TranslationValue::Number(i64::from(row.tab_count)))],
                    ),
                    active: row.is_active,
                    ..Default::default()
                },
                rank,
                close: false,
                picker: request.picker.clone(),
                numbered,
                preferred: row.is_active,
                pre_filtered: true,
                ..Default::default()
            },
            SpaceContribution(row.id),
            ChildOf(request.target),
        ));
        rank += 1;
    }

    if picker {
        commands.spawn((
            Name::new("Manage spaces command-bar row"),
            CommandBarContribution {
                row: vmux_api::command_bar::CommandBarResultItem {
                    key: "space:manage".to_string(),
                    title: translate("command-manage-spaces"),
                    url: SpacePlugin::url().to_string(),
                    ..Default::default()
                },
                rank: i32::MAX,
                close: true,
                picker: request.picker.clone(),
                pre_filtered: true,
                ..Default::default()
            },
            ManageSpacesContribution,
            ChildOf(request.target),
        ));
    }
}

fn activate_contribution(
    trigger: On<CommandBarContributionActivated>,
    spaces: Query<&SpaceContribution>,
    manage: Query<(), With<ManageSpacesContribution>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    if let Ok(space) = spaces.get(event.target) {
        commands.trigger(UiInput {
            webview: event.webview,
            payload: SpaceAttachRequest {
                space_id: space.0.clone(),
            },
        });
        commands.trigger(CommandBarDismiss::new(event.webview, false));
        return;
    }
    if manage.contains(event.target) {
        commands.trigger(UiInput {
            webview: event.webview,
            payload: CommandBarOpenUrlRequest {
                value: SpacePlugin::url().to_string(),
                open: event.open,
            },
        });
    }
}

fn update_effective_startup(
    settings: Option<Res<AppSettings>>,
    startup_page: StartupPageUrl,
    mut spaces: Query<
        (
            Ref<SpaceId>,
            &mut EffectiveStartupUrl,
            &mut EffectiveStartupDir,
        ),
        With<Space>,
    >,
) {
    let settings_changed = settings
        .as_ref()
        .is_some_and(|settings| settings.is_changed());
    for (id, mut effective_url, mut effective_dir) in &mut spaces {
        let next_url = settings
            .as_deref()
            .map(|settings| settings.startup_url(&id.0, startup_page.get().unwrap_or_default()))
            .unwrap_or_else(|| startup_page.get().unwrap_or_default().to_string());
        if effective_url.0 != next_url {
            effective_url.0 = next_url;
        }
        if !settings_changed
            && !id.is_changed()
            && !effective_dir.is_added()
            && effective_dir.0.as_ref().is_none_or(|path| path.is_dir())
        {
            continue;
        }
        let next = settings
            .as_deref()
            .and_then(|settings| settings.startup_dir(&id.0));
        if effective_dir.0 != next {
            effective_dir.0 = next;
        }
    }
}

#[derive(Component)]
struct SpacesListSent;

#[derive(Component)]
struct SpaceForm(SpaceFormState);

#[derive(Component)]
struct OpenSpaceForm;

fn reset_sent(
    trigger: On<UiInput<PageReady>>,
    spaces_views: Query<(), With<Spaces>>,
    cef_views: Query<(), With<LayoutCef>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    if spaces_views.get(entity).is_err() && cef_views.get(entity).is_err() {
        return;
    }
    commands.entity(entity).remove::<SpacesListSent>();
}

fn open_form(
    trigger: On<UiInput<SpaceFormOpenRequest>>,
    identity: SpaceIdentity,
    mut commands: Commands,
) {
    let suggestion = if trigger.event().payload.space_id.is_none() {
        identity.suggested(&[])
    } else {
        String::new()
    };
    let draft = if trigger.event().payload.draft.trim().is_empty() {
        suggestion.clone()
    } else {
        trigger.event().payload.draft.clone()
    };
    let form = SpaceFormState {
        open: true,
        space_id: trigger.event().payload.space_id.clone(),
        draft,
        suggestion,
    };
    commands
        .entity(trigger.event().webview)
        .insert(SpaceForm(form.clone()));
    commands.trigger(UiStateWrite::<SpacesUiState>::from_event(
        trigger.event().webview,
        &form,
    ));
    commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
        trigger.event().webview,
        &form,
    ));
}

fn edit_form(
    trigger: On<UiInput<SpaceFormInputRequest>>,
    mut forms: Query<&mut SpaceForm>,
    mut commands: Commands,
) {
    let Ok(mut form) = forms.get_mut(trigger.event().webview) else {
        return;
    };
    form.0.draft.clone_from(&trigger.event().payload.draft);
    commands.trigger(UiStateWrite::<SpacesUiState>::from_event(
        trigger.event().webview,
        &form.0,
    ));
    commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
        trigger.event().webview,
        &form.0,
    ));
}

fn regenerate_form(
    trigger: On<UiInput<SpaceFormRegenerateRequest>>,
    identity: SpaceIdentity,
    mut forms: Query<&mut SpaceForm>,
    mut commands: Commands,
) {
    let Ok(mut form) = forms.get_mut(trigger.event().webview) else {
        return;
    };
    let suggestion = identity.suggested(&[&form.0.draft, &form.0.suggestion]);
    form.0.draft.clone_from(&suggestion);
    form.0.suggestion = suggestion;
    commands.trigger(UiStateWrite::<SpacesUiState>::from_event(
        trigger.event().webview,
        &form.0,
    ));
    commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
        trigger.event().webview,
        &form.0,
    ));
}

fn close_form(trigger: On<UiInput<SpaceFormCloseRequest>>, mut commands: Commands) {
    commands
        .entity(trigger.event().webview)
        .remove::<SpaceForm>();
    commands.trigger(UiStateWrite::<SpacesUiState>::from_event(
        trigger.event().webview,
        &SpaceFormState::default(),
    ));
    commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
        trigger.event().webview,
        &SpaceFormState::default(),
    ));
}

type SpaceListQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static SpaceId,
        &'static Name,
        Has<Active>,
        Option<&'static Order>,
        Option<&'static Children>,
        &'static ChildOf,
    ),
    With<Space>,
>;

#[derive(bevy::ecs::system::SystemParam)]
struct SpaceRows<'w, 's> {
    spaces: SpaceListQuery<'w, 's>,
    tabs: Query<'w, 's, (), With<Tab>>,
    settings: Option<Res<'w, AppSettings>>,
    profile: vmux_ecs::profile::CurrentProfile<'w, 's>,
}

impl SpaceRows<'_, '_> {
    fn list(&self, main: Option<Entity>) -> Vec<SpaceRow> {
        let mut rows: Vec<(u32, SpaceRow)> = Vec::new();
        for (_, sid, name, is_active, order, children, parent) in &self.spaces {
            let local = main.is_none_or(|main| parent.parent() == main);
            let local_tab_count = children
                .map(|children| {
                    children
                        .iter()
                        .filter(|entity| self.tabs.contains(*entity))
                        .count()
                })
                .unwrap_or_default() as u32;
            if let Some((existing_order, row)) =
                rows.iter_mut().find(|(_, existing)| existing.id == sid.0)
            {
                *existing_order =
                    (*existing_order).min(order.map(|order| order.0).unwrap_or(u32::MAX));
                if local {
                    row.is_active = is_active;
                    row.tab_count = local_tab_count;
                }
                continue;
            }
            let startup_dir = self
                .settings
                .as_deref()
                .and_then(|settings| settings.startup_dir(&sid.0))
                .map(|path| Self::display_dir(&path))
                .unwrap_or_default();
            rows.push((
                order.map(|order| order.0).unwrap_or(u32::MAX),
                SpaceRow {
                    id: sid.0.clone(),
                    name: name.to_string(),
                    profile: self.profile.label().unwrap_or("Personal").to_string(),
                    is_active: local && is_active,
                    tab_count: if local { local_tab_count } else { 0 },
                    startup_dir,
                },
            ));
        }
        rows.sort_by_key(|(order, _)| *order);
        rows.into_iter().map(|(_, row)| row).collect()
    }

    fn display_dir(path: &std::path::Path) -> String {
        if let Some(home) = std::env::home_dir()
            && let Ok(relative) = path.strip_prefix(&home)
        {
            return format!("~/{}", relative.to_string_lossy());
        }
        path.to_string_lossy().to_string()
    }
}

fn broadcast_spaces_to_views(
    rows: SpaceRows,
    mut views: Query<
        (
            Entity,
            Option<&mut SpaceSelection>,
            Option<&SpacesPageSnapshot>,
            Has<SpacesListSent>,
        ),
        (With<PageReady>, Or<(With<Spaces>, With<LayoutCef>)>),
    >,
    browsers: NonSend<Browsers>,
    space_window: SpaceWindow,
    layout_ui: Query<(), With<LayoutUiStateUpdates>>,
    spaces_ui: Query<(), With<SpacesUiStateUpdates>>,
    mut commands: Commands,
) {
    for (entity, selection, snapshot, sent) in &mut views {
        let main = space_window
            .window_of(entity)
            .and_then(|window| space_window.main(window));
        let projected = rows.list(main);
        let active = projected
            .iter()
            .position(|space| space.is_active)
            .unwrap_or_default();
        let selected = if let Some(mut selection) = selection {
            if !sent {
                selection.0 = active;
            } else {
                selection.0 = selection.0.min(projected.len().saturating_sub(1));
            }
            selection.0
        } else {
            active
        };
        let payload = SpacesListEvent {
            spaces: projected,
            selected: selected as u32,
        };
        if sent && snapshot.is_some_and(|snapshot| snapshot.0 == payload) {
            continue;
        }
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        if layout_ui.contains(entity) {
            commands
                .entity(entity)
                .insert(SpacesProjection(payload.clone()));
            commands.trigger(UiStateWrite::<LayoutUiState>::from_event(entity, &payload));
        }
        if spaces_ui.contains(entity) {
            commands.trigger(UiStateWrite::<SpacesUiState>::from_event(entity, &payload));
        }
        commands
            .entity(entity)
            .insert((SpacesListSent, SpacesPageSnapshot(payload)));
    }
}

fn project_activate(
    trigger: On<UiInput<ProjectActivateRequest>>,
    active: FocusedSpace,
    space_of_pane: SpaceOfPane,
    scope: vmux_layout::window::WindowScope,
    spaces: Query<&SpaceId, With<Space>>,
    settings: Option<ResMut<AppSettings>>,
    mut saves: MessageWriter<SettingsSaveRequest>,
) {
    let webview = trigger.event().webview;
    let pane_id = trigger.event().payload.pane_id;
    let space = if webview == Entity::PLACEHOLDER {
        match pane_id {
            Some(pane_id) => space_of_pane.resolve(&pane_id.to_string()),
            None => active.entity(),
        }
    } else {
        let origin = scope.current_space(webview);
        match pane_id {
            Some(pane_id) => space_of_pane
                .resolve(&pane_id.to_string())
                .filter(|space| Some(*space) == origin),
            None => origin,
        }
    };
    let space_id = space.and_then(|space| spaces.get(space).ok());
    let (Some(space_id), Some(mut settings)) = (space_id, settings) else {
        return;
    };
    let changed = settings
        .bypass_change_detection()
        .activate_space_project(&space_id.0, trigger.event().payload.path.trim());
    if changed {
        settings.set_changed();
        saves.write(SettingsSaveRequest);
    }
}

fn project_forget(
    trigger: On<UiInput<ProjectForgetRequest>>,
    active: FocusedSpace,
    scope: vmux_layout::window::WindowScope,
    spaces: Query<&SpaceId, With<Space>>,
    settings: Option<ResMut<AppSettings>>,
    mut saves: MessageWriter<SettingsSaveRequest>,
) {
    let webview = trigger.event().webview;
    let space = if webview == Entity::PLACEHOLDER {
        active.entity()
    } else {
        scope.current_space(webview)
    };
    let space_id = space.and_then(|space| spaces.get(space).ok());
    let (Some(space_id), Some(mut settings)) = (space_id, settings) else {
        return;
    };
    let changed = settings
        .bypass_change_detection()
        .forget_space_project(&space_id.0, trigger.event().payload.path.trim());
    if changed {
        settings.set_changed();
        saves.write(SettingsSaveRequest);
    }
}

fn relay_requests<T: Message + Clone>(mut reader: MessageReader<T>, mut commands: Commands) {
    for request in reader.read() {
        commands.trigger(UiInput {
            webview: Entity::PLACEHOLDER,
            payload: request.clone(),
        });
    }
}

type SpaceQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static SpaceId,
        Has<Active>,
        Option<&'static Order>,
        &'static ChildOf,
    ),
    With<Space>,
>;

type SpaceTabQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static SpaceId,
        &'static LastActivatedAt,
        &'static ChildOf,
    ),
    With<Tab>,
>;

#[derive(bevy::ecs::system::SystemParam)]
struct SpaceGraph<'w, 's> {
    spaces: SpaceQuery<'w, 's>,
    rows: SpaceListQuery<'w, 's>,
    tabs: SpaceTabQuery<'w, 's>,
}

impl SpaceGraph<'_, '_> {
    fn view_template(&self, id: &str, matching: bool) -> Option<(String, String, u32)> {
        let mut selected: Option<(String, String, u32)> = None;
        for (_, candidate, name, _, order, _, _) in &self.rows {
            if (candidate.0 == id) != matching {
                continue;
            }
            let order = order.map(|order| order.0).unwrap_or(u32::MAX);
            if selected.as_ref().is_some_and(|current| current.2 <= order) {
                continue;
            }
            selected = Some((candidate.0.clone(), name.to_string(), order));
        }
        selected
    }

    fn template_bundle(&self, template: &(String, String, u32), main: Entity) -> impl Bundle {
        (
            SpaceId(template.0.clone()),
            Name::new(template.1.clone()),
            Order(template.2),
            Active,
            LastActivatedAt::now(),
            Space::bundle(),
            ChildOf(main),
        )
    }

    fn template_layout_request(
        &self,
        template: &(String, String, u32),
        space: Entity,
        window: Entity,
        settings: Option<&AppSettings>,
        startup_page: &str,
    ) -> TabLayoutSpawnRequest {
        let startup_dir = settings.and_then(|settings| settings.startup_dir(&template.0));
        let content = settings
            .map(|settings| settings.startup_url(&template.0, startup_page))
            .or_else(|| Some(startup_page.to_string()))
            .filter(|url| !url.is_empty())
            .map(|url| TabLayoutSpawnContent::Url {
                url,
                pending_prompt: None,
            })
            .unwrap_or(TabLayoutSpawnContent::StartupUrlOrPrompt);
        TabLayoutSpawnRequest {
            space,
            primary_window: window,
            name: None,
            startup_dir,
            content,
            clear_pending_stack: true,
            focus: true,
        }
    }

    fn bump_tab(&self, space: Entity, commands: &mut Commands) {
        if let Some((tab, _, _, _)) = self
            .tabs
            .iter()
            .filter(|(_, _, _, parent)| parent.parent() == space)
            .max_by_key(|(_, _, timestamp, _)| timestamp.0)
        {
            commands.entity(tab).insert(LastActivatedAt::now());
        }
    }

    fn deactivate_in_main(&self, main: Entity, commands: &mut Commands) {
        for (entity, _, active, _, parent) in &self.spaces {
            if parent.parent() == main && active {
                commands.entity(entity).remove::<Active>();
            }
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct SpaceWindow<'w, 's> {
    mains: Query<'w, 's, Entity, With<Main>>,
    host_windows: Query<'w, 's, &'static HostWindow>,
    focused: FocusedWindow<'w, 's>,
    hierarchy: WindowHierarchy<'w, 's>,
}

impl SpaceWindow<'_, '_> {
    fn for_webview(&self, webview: Entity) -> Option<(Entity, Entity)> {
        let window = self
            .host_windows
            .get(webview)
            .ok()
            .map(|host| host.0)
            .or_else(|| self.focused.entity())?;
        Some((window, self.main(window)?))
    }

    fn focused(&self) -> Option<(Entity, Entity)> {
        let window = self.focused.entity()?;
        Some((window, self.main(window)?))
    }

    fn main(&self, window: Entity) -> Option<Entity> {
        self.mains
            .iter()
            .find(|main| self.hierarchy.get(*main) == Some(window))
    }

    fn window_of(&self, entity: Entity) -> Option<Entity> {
        self.hierarchy.get(entity)
    }
}

fn rename(trigger: On<UiInput<SpaceRenameRequest>>, graph: SpaceGraph, mut commands: Commands) {
    let request = &trigger.event().payload;
    let name = request.name.trim();
    if name.is_empty() {
        return;
    }
    if !graph
        .spaces
        .iter()
        .any(|(_, id, _, _, _)| id.0 == request.space_id)
    {
        return;
    }
    for (entity, id, _, _, _) in &graph.spaces {
        if id.0 != request.space_id {
            continue;
        }
        commands.entity(entity).insert(Name::new(name.to_string()));
    }
}

fn open_page(
    trigger: On<UiInput<SpaceOpenPageRequest>>,
    space_window: SpaceWindow,
    focus: FocusedStack,
    mut spawn_requests: Option<MessageWriter<PageOpenRequest>>,
    stacks: Query<(Entity, &PageMetadata), With<Stack>>,
    mut commands: Commands,
) {
    let Some((window, _)) = space_window.for_webview(trigger.event().webview) else {
        return;
    };
    if let Some((existing, _)) = stacks.iter().find(|(stack, metadata)| {
        metadata.url == SpacePlugin::manifest().url
            && space_window.window_of(*stack) == Some(window)
    }) {
        if trigger.event().payload.create {
            commands.entity(existing).insert(OpenSpaceForm);
        }
        commands.trigger(ActivateRequest { entity: existing });
        return;
    }
    let Some(pane) = focus.as_deref().and_then(|focus| focus.pane) else {
        return;
    };
    let Some(spawn_requests) = spawn_requests.as_mut() else {
        return;
    };
    let mut stack = commands.spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(pane)));
    if trigger.event().payload.create {
        stack.insert(OpenSpaceForm);
    }
    let stack = stack.id();
    spawn_requests.write(PageOpenRequest {
        target: PageOpenTarget::Stack(stack),
        url: SpacePlugin::manifest().url.to_string(),
        request_id: None,
    });
}

fn open_requested_form(
    views: Query<Entity, (With<Spaces>, With<PageReady>)>,
    pending: Query<(), With<OpenSpaceForm>>,
    parents: Query<&ChildOf>,
    mut commands: Commands,
) {
    for view in &views {
        let mut current = view;
        loop {
            if pending.contains(current) {
                commands.entity(current).remove::<OpenSpaceForm>();
                commands.trigger(UiInput {
                    webview: view,
                    payload: SpaceFormOpenRequest::default(),
                });
                break;
            }
            let Ok(parent) = parents.get(current) else {
                break;
            };
            current = parent.parent();
        }
    }
}

fn delete(
    trigger: On<UiInput<SpaceDeleteRequest>>,
    graph: SpaceGraph,
    space_window: SpaceWindow,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    settings: Option<Res<AppSettings>>,
    startup_page: StartupPageUrl,
    mut commands: Commands,
) {
    if space_window.for_webview(trigger.event().webview).is_none() {
        return;
    }
    let id = trigger.event().payload.space_id.as_str();
    let logical_ids: std::collections::HashSet<&str> = graph
        .spaces
        .iter()
        .map(|(_, id, _, _, _)| id.0.as_str())
        .collect();
    if logical_ids.len() <= 1 {
        return;
    }
    let Some(fallback) = graph.view_template(id, false) else {
        return;
    };
    let mut affected_mains = Vec::new();
    for (entity, space_id, _, _, parent) in &graph.spaces {
        if space_id.0 != id {
            continue;
        }
        if !affected_mains.contains(&parent.parent()) {
            affected_mains.push(parent.parent());
        }
        commands.entity(entity).despawn();
    }
    if affected_mains.is_empty() {
        return;
    }
    for affected_main in affected_mains {
        graph.deactivate_in_main(affected_main, &mut commands);
        if let Some((target_entity, _, _, _, _)) = graph
            .spaces
            .iter()
            .filter(|(_, space_id, _, _, parent)| {
                space_id.0 != id && parent.parent() == affected_main
            })
            .min_by_key(|(_, _, _, order, _)| order.map(|order| order.0).unwrap_or(u32::MAX))
        {
            commands
                .entity(target_entity)
                .insert((Active, LastActivatedAt::now()));
            graph.bump_tab(target_entity, &mut commands);
            continue;
        }
        let Some(affected_window) = space_window.window_of(affected_main) else {
            continue;
        };
        let space = commands
            .spawn(graph.template_bundle(&fallback, affected_main))
            .id();
        layout_requests.write(graph.template_layout_request(
            &fallback,
            space,
            affected_window,
            settings.as_deref(),
            startup_page.get().unwrap_or_default(),
        ));
    }
}

fn attach(
    trigger: On<UiInput<SpaceAttachRequest>>,
    graph: SpaceGraph,
    space_window: SpaceWindow,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    settings: Option<Res<AppSettings>>,
    startup_page: StartupPageUrl,
    mut commands: Commands,
) {
    let Some((window, main)) = space_window.for_webview(trigger.event().webview) else {
        return;
    };
    let id = trigger.event().payload.space_id.as_str();
    let local = graph
        .spaces
        .iter()
        .find(|(_, space_id, _, _, parent)| space_id.0 == id && parent.parent() == main);
    let Some((entity, _, is_active, _, _)) = local else {
        let Some(template) = graph.view_template(id, true) else {
            return;
        };
        graph.deactivate_in_main(main, &mut commands);
        let space = commands.spawn(graph.template_bundle(&template, main)).id();
        layout_requests.write(graph.template_layout_request(
            &template,
            space,
            window,
            settings.as_deref(),
            startup_page.get().unwrap_or_default(),
        ));
        return;
    };
    if is_active {
        return;
    }
    graph.deactivate_in_main(main, &mut commands);
    commands
        .entity(entity)
        .insert((Active, LastActivatedAt::now()));
    graph.bump_tab(entity, &mut commands);
}

fn create(
    trigger: On<UiInput<SpaceCreateRequest>>,
    identity: SpaceIdentity,
    graph: SpaceGraph,
    space_window: SpaceWindow,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    settings: Option<Res<AppSettings>>,
    mut commands: Commands,
) {
    let Some((window, main)) = space_window.for_webview(trigger.event().webview) else {
        return;
    };
    let requested_name = trigger.event().payload.name.trim();
    let name = if requested_name.is_empty() {
        identity.suggested(&[])
    } else {
        requested_name.to_string()
    };
    let id = identity.unique(&name).0;
    let order = graph
        .spaces
        .iter()
        .filter(|(_, _, _, _, parent)| parent.parent() == main)
        .filter_map(|(_, _, _, order, _)| order.map(|order| order.0))
        .max()
        .map(|max| max + 1)
        .unwrap_or(0);
    graph.deactivate_in_main(main, &mut commands);
    let space = commands
        .spawn((
            SpaceId(id.clone()),
            Name::new(id.clone()),
            Order(order),
            Active,
            LastActivatedAt::now(),
            Space::bundle(),
            ChildOf(main),
        ))
        .id();
    let startup_dir = settings
        .as_deref()
        .and_then(|settings| settings.startup_dir(&id));
    layout_requests.write(TabLayoutSpawnRequest {
        space,
        primary_window: window,
        name: None,
        startup_dir,
        content: TabLayoutSpawnContent::Url {
            url: SpacePlugin::manifest().url.to_string(),
            pending_prompt: None,
        },
        clear_pending_stack: true,
        focus: true,
    });
}

fn handle_open_in_new(
    mut reader: MessageReader<OpenRequest>,
    identity: SpaceIdentity,
    graph: SpaceGraph,
    space_window: SpaceWindow,
    settings: Option<Res<AppSettings>>,
    startup_page: StartupPageUrl,
    mut layout_requests: MessageWriter<TabLayoutSpawnRequest>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let Some((window, main)) = space_window.focused() else {
            continue;
        };

        let name = identity.suggested(&[]);
        let id = identity.unique(&name).0;
        let order = graph
            .spaces
            .iter()
            .filter(|(_, _, _, _, parent)| parent.parent() == main)
            .filter_map(|(_, _, _, order, _)| order.map(|o| o.0))
            .max()
            .map(|max| max + 1)
            .unwrap_or(0);
        graph.deactivate_in_main(main, &mut commands);
        let space = commands
            .spawn((
                SpaceId(id.clone()),
                Name::new(id.clone()),
                Order(order),
                Active,
                LastActivatedAt::now(),
                Space::bundle(),
                ChildOf(main),
            ))
            .id();
        let startup_dir = settings
            .as_deref()
            .and_then(|settings| settings.startup_dir(&id));
        let startup_url = settings
            .as_deref()
            .map(|settings| settings.startup_url(&id, startup_page.get().unwrap_or_default()))
            .unwrap_or_else(|| startup_page.get().unwrap_or_default().to_string());
        commands.entity(space).insert((
            EffectiveStartupDir(startup_dir.clone()),
            EffectiveStartupUrl(startup_url.clone()),
        ));
        let content = request
            .url
            .as_deref()
            .filter(|url| !url.is_empty())
            .map(|url| TabLayoutSpawnContent::Url {
                url: url.to_string(),
                pending_prompt: None,
            })
            .or_else(|| {
                (!startup_url.is_empty()).then(|| TabLayoutSpawnContent::Url {
                    url: startup_url.clone(),
                    pending_prompt: None,
                })
            })
            .unwrap_or(TabLayoutSpawnContent::StartupUrlOrPrompt);
        layout_requests.write(TabLayoutSpawnRequest {
            space,
            primary_window: window,
            name: None,
            startup_dir,
            content,
            clear_pending_stack: true,
            focus: true,
        });
    }
}

fn respond_spaces_spawn(
    mut reader: MessageReader<SpacesPageSpawnRequest>,
    mut page_open: MessageWriter<PageOpenRequest>,
) {
    for req in reader.read() {
        page_open.write(PageOpenRequest {
            target: PageOpenTarget::Stack(req.target_stack),
            url: SpacePlugin::manifest().url.to_string(),
            request_id: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{BrowserSettings, ShortcutSettings};

    #[test]
    fn space_mcp_definition_dispatches_to_the_typed_request() {
        let definitions =
            vmux_command::CommandDefinition::from_manifest(FeatureManifest::of::<crate::Feature>())
                .collect::<Vec<_>>();
        let tools = definitions
            .iter()
            .filter_map(vmux_command::CommandDefinition::agent_tool)
            .filter(|tool| {
                let invocation = CommandInvocation::new(Entity::PLACEHOLDER, &tool.name);
                OpenRequest::try_from(&invocation).is_ok()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            ["open_in_new_space"],
        );
        let invocation = CommandInvocation::new(Entity::PLACEHOLDER, "open_in_new_space")
            .with_arguments(serde_json::json!({"url": "https://vmux.ai"}));
        assert!(OpenRequest::try_from(&invocation).is_ok());
    }

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
                ..Default::default()
            },
            layout: LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 0.0 },
                pane: PaneSettings::default(),
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            update_channel: Default::default(),
            agent: AgentSettings::default(),
            spaces: Default::default(),
            projects: Default::default(),
            recording: Default::default(),
            editor: Default::default(),
            appearance: Default::default(),
        }
    }

    #[test]
    fn registers_spaces_host_before_cef_embedded_hosts_are_read() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, SpacePlugin));
        let mut query = app.world_mut().query::<&vmux_ecs::page::PageManifest>();
        let hosts = bevy_cef_core::prelude::CefEmbeddedHosts(
            query
                .iter(app.world())
                .map(vmux_ecs::page::PageManifest::embedded_host)
                .collect(),
        );

        let entry = hosts.entry_for_host("spaces").unwrap();
        assert_eq!(entry.default_document, "spaces/index.html");
    }

    struct IdentityFixture {
        app: App,
        window: Entity,
        main: Entity,
        webview: Entity,
    }

    impl IdentityFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, SpacePlugin))
                .add_message::<TabLayoutSpawnRequest>()
                .add_message::<SettingsSaveRequest>()
                .add_message::<PageOpenRequest>()
                .insert_resource(test_settings());
            app.world_mut().insert_non_send(Browsers::default());
            let window = app.world_mut().spawn((Window::default(), Active)).id();
            let root = app.world_mut().spawn(HostWindow(window)).id();
            let main = app.world_mut().spawn((Main, ChildOf(root))).id();
            let webview = app.world_mut().spawn(HostWindow(window)).id();
            Self {
                app,
                window,
                main,
                webview,
            }
        }

        fn with_projects(active: &str, projects: Vec<SpaceProject>) -> Self {
            let mut fixture = Self::new();
            let mut settings = test_settings();
            settings.spaces.insert(
                "work".into(),
                SpaceOverrides {
                    projects,
                    active_project: Some(active.into()),
                    ..Default::default()
                },
            );
            fixture.app.insert_resource(settings);
            fixture.app.world_mut().spawn((
                Space,
                SpaceId("work".into()),
                Name::new("Work"),
                vmux_layout::profile::Profile {
                    name: "Personal".into(),
                },
                CurrentSpace,
            ));
            fixture
        }

        fn request<T: Send + Sync + 'static>(&mut self, payload: T) {
            self.app.world_mut().trigger(UiInput {
                webview: Entity::PLACEHOLDER,
                payload,
            });
            self.app.world_mut().flush();
        }

        fn project_state(&self) -> (Vec<String>, Option<String>) {
            let space = self
                .app
                .world()
                .resource::<AppSettings>()
                .space("work")
                .expect("space");
            (
                space
                    .projects
                    .iter()
                    .map(|project| project.path.clone())
                    .collect(),
                space.active_project.clone(),
            )
        }

        fn create(&mut self, name: &str) {
            self.app.world_mut().trigger(UiInput {
                webview: self.webview,
                payload: SpaceCreateRequest { name: name.into() },
            });
            self.app.world_mut().flush();
        }

        fn rows(&mut self) -> Vec<(String, String)> {
            let mut rows = Vec::new();
            let world = self.app.world_mut();
            for (id, name) in world
                .query_filtered::<(&SpaceId, &Name), With<Space>>()
                .iter(world)
            {
                rows.push((id.0.clone(), name.as_str().into()));
            }
            rows.sort();
            rows
        }
    }

    #[test]
    fn creation_uses_canonical_global_ids_and_preserves_nested_namespaces() {
        let mut fixture = IdentityFixture::new();
        fixture.app.world_mut().spawn((
            Space,
            SpaceId("org-name/repo".into()),
            Name::new("existing"),
        ));
        fixture.app.world_mut().spawn((
            Space,
            SpaceId("org-name/repo-2".into()),
            Name::new("existing suffix"),
        ));
        fixture.create("Org Name/Repo!");
        fixture.create("a//b/");
        fixture.create("!!!");
        fixture.create("Client A!");
        fixture.create("Client A!");
        assert_eq!(
            fixture.rows(),
            [
                ("a/b".into(), "a/b".into()),
                ("client-a".into(), "client-a".into()),
                ("client-a-2".into(), "client-a-2".into()),
                ("org-name/repo".into(), "existing".into()),
                ("org-name/repo-2".into(), "existing suffix".into()),
                ("org-name/repo-3".into(), "org-name/repo-3".into()),
                ("space".into(), "space".into()),
            ]
        );
        let world = fixture.app.world_mut();
        let (entity, id, parent) = world
            .query_filtered::<(Entity, &SpaceId, &ChildOf), (With<Space>, With<Active>)>()
            .single(world)
            .unwrap();
        assert_eq!(id.0, "client-a-2");
        assert_eq!(parent.parent(), fixture.main);
        let requests = world.resource::<bevy::ecs::message::Messages<TabLayoutSpawnRequest>>();
        let mut cursor = requests.get_cursor();
        let request = cursor.read(requests).last().unwrap();
        assert_eq!(request.space, entity);
        assert_eq!(request.primary_window, fixture.window);
    }

    #[test]
    fn creation_uses_the_new_identity_for_startup_overrides() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_string_lossy().to_string();
        let mut fixture = IdentityFixture::new();
        fixture
            .app
            .world_mut()
            .resource_mut::<AppSettings>()
            .spaces
            .insert(
                "work-2".into(),
                SpaceOverrides {
                    startup_dir: Some(path.clone()),
                    ..Default::default()
                },
            );
        fixture.create("Work");
        fixture.create("Work");
        let requests = fixture
            .app
            .world()
            .resource::<bevy::ecs::message::Messages<TabLayoutSpawnRequest>>();
        let mut cursor = requests.get_cursor();
        let request = cursor.read(requests).last().unwrap();
        assert_eq!(request.startup_dir.as_deref(), Some(directory.path()));
    }

    #[test]
    fn open_in_new_space_uses_registered_creation_and_the_requested_url() {
        let mut fixture = IdentityFixture::new();
        fixture.app.world_mut().write_message(OpenRequest {
            url: Some("https://example.com/requested".into()),
        });
        fixture.app.world_mut().run_schedule(Update);
        assert_eq!(fixture.rows().len(), 1);
        let world = fixture.app.world_mut();
        let (space, parent) = world
            .query_filtered::<(Entity, &ChildOf), (With<Space>, With<Active>)>()
            .single(world)
            .unwrap();
        assert_eq!(parent.parent(), fixture.main);
        let requests = world.resource::<bevy::ecs::message::Messages<TabLayoutSpawnRequest>>();
        let mut cursor = requests.get_cursor();
        let request = cursor.read(requests).last().unwrap();
        assert_eq!(request.space, space);
        assert_eq!(request.primary_window, fixture.window);
        assert!(
            matches!(&request.content, TabLayoutSpawnContent::Url { url, .. } if url == "https://example.com/requested")
        );
    }

    #[test]
    fn form_regeneration_excludes_canonical_names_and_current_drafts() {
        let mut fixture = IdentityFixture::new();
        for name in [
            "mercury", "venus", "earth", "mars", "jupiter", "saturn", "uranus", "neptune", "ceres",
            "pluto", "haumea", "makemake", "eris",
        ] {
            fixture
                .app
                .world_mut()
                .spawn((Space, SpaceId(name.into()), Name::new(name)));
        }
        fixture.app.world_mut().trigger(UiInput {
            webview: fixture.webview,
            payload: SpaceFormOpenRequest {
                space_id: None,
                draft: String::new(),
            },
        });
        fixture.app.world_mut().flush();
        for _ in 0..20 {
            let previous = fixture
                .app
                .world()
                .get::<SpaceForm>(fixture.webview)
                .unwrap()
                .0
                .clone();
            fixture.app.world_mut().trigger(UiInput {
                webview: fixture.webview,
                payload: SpaceFormRegenerateRequest,
            });
            fixture.app.world_mut().flush();
            let next = &fixture
                .app
                .world()
                .get::<SpaceForm>(fixture.webview)
                .unwrap()
                .0;
            assert_ne!(next.suggestion, previous.suggestion);
            assert_ne!(next.suggestion, previous.draft);
            assert_eq!(next.draft, next.suggestion);
            let name = next.suggestion.clone();
            assert!(!fixture.rows().iter().any(|(_, existing)| existing == &name));
        }
    }

    #[test]
    fn bootstrap_preserves_profile_identity_and_refuses_existing_or_restored_spaces() {
        if std::env::var_os("VMUX_SPACE_BOOTSTRAP_ORACLE").is_none() {
            let directory = tempfile::tempdir().unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "host::plugin::tests::bootstrap_preserves_profile_identity_and_refuses_existing_or_restored_spaces", "--nocapture"])
                .env("VMUX_SPACE_BOOTSTRAP_ORACLE", "1")
                .env("VMUX_TEST", "1")
                .env("VMUX_DATA_DIR", directory.path())
                .output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        for case in 0..3 {
            let mut fixture = IdentityFixture::new();
            fixture.app.world_mut().spawn((
                vmux_ecs::profile::ActiveProfile(vmux_ecs::profile::Profile::current()),
                vmux_ecs::profile::ProfileDirectories(vmux_ecs::profile::ProfilePaths::current()),
                Name::new("Team"),
            ));
            if case == 1 {
                fixture
                    .app
                    .world_mut()
                    .spawn((Space, SpaceId("saved".into()), Name::new("Saved")));
            } else if case == 2 {
                fixture.app.add_systems(
                    Startup,
                    (|mut restore: Single<&mut WorkspaceRestore>| {
                        restore.store_present = true;
                    })
                    .after(LayoutStartupSet::Store)
                    .before(ensure_bootstrap),
                );
            }
            fixture.app.world_mut().run_schedule(Startup);
            match case {
                0 => {
                    let world = fixture.app.world_mut();
                    let (id, profile) = world
                        .query_filtered::<(&SpaceId, &vmux_layout::profile::Profile), With<Space>>()
                        .single(world)
                        .unwrap();
                    assert_eq!(id.0, "space-1");
                    assert_eq!(profile.name, "Team");
                }
                1 => assert_eq!(fixture.rows(), [("saved".into(), "Saved".into())]),
                _ => assert!(fixture.rows().is_empty()),
            }
        }
    }

    #[test]
    fn main_for_window_walks_through_the_main_column() {
        let mut app = App::new();
        let window = app.world_mut().spawn_empty().id();
        let root = app.world_mut().spawn(HostWindow(window)).id();
        let column = app.world_mut().spawn(ChildOf(root)).id();
        let main = app.world_mut().spawn((Main, ChildOf(column))).id();

        let resolved = app
            .world_mut()
            .run_system_once(move |space_window: SpaceWindow| space_window.main(window))
            .unwrap();

        assert_eq!(resolved, Some(main));
    }

    #[test]
    fn effective_startup_url_reflects_active_space_override() {
        let mut settings = test_settings();
        settings.browser.startup_url = "https://global.example".into();
        settings.spaces.insert(
            "work".into(),
            SpaceOverrides {
                startup_url: Some("https://work.example".into()),
                startup_dir: None,
                ..Default::default()
            },
        );

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutContractPlugin))
            .insert_resource(settings)
            .add_systems(Update, update_effective_startup);
        let space = app
            .world_mut()
            .spawn((
                Space,
                SpaceId("work".into()),
                Name::new("Work"),
                vmux_layout::profile::Profile {
                    name: "Personal".into(),
                },
                CurrentSpace,
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<EffectiveStartupUrl>(space).unwrap().0,
            "https://work.example"
        );
    }

    #[test]
    fn attaching_a_shared_space_creates_a_window_local_view() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutContractPlugin))
            .add_message::<TabLayoutSpawnRequest>()
            .add_observer(attach);
        let first_window = app.world_mut().spawn_empty().id();
        let second_window = app.world_mut().spawn_empty().id();
        app.world_mut().entity_mut(second_window).insert(Active);
        let first_root = app.world_mut().spawn(HostWindow(first_window)).id();
        let second_root = app.world_mut().spawn(HostWindow(second_window)).id();
        let first_column = app.world_mut().spawn(ChildOf(first_root)).id();
        let second_column = app.world_mut().spawn(ChildOf(second_root)).id();
        let first_main = app.world_mut().spawn((Main, ChildOf(first_column))).id();
        let second_main = app.world_mut().spawn((Main, ChildOf(second_column))).id();
        app.world_mut().spawn((
            SpaceId("shared".to_string()),
            Name::new("shared"),
            Order(0),
            Active,
            Space::bundle(),
            ChildOf(first_main),
        ));
        let previous = app
            .world_mut()
            .spawn((
                SpaceId("second".to_string()),
                Name::new("second"),
                Order(0),
                Active,
                Space::bundle(),
                ChildOf(second_main),
            ))
            .id();
        let webview = app.world_mut().spawn(HostWindow(second_window)).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: SpaceAttachRequest {
                space_id: "shared".to_string(),
            },
        });
        app.update();

        let shared_views: Vec<(Entity, Entity, bool)> = app
            .world_mut()
            .query_filtered::<(Entity, &ChildOf, Has<Active>), With<Space>>()
            .iter(app.world())
            .filter_map(|(entity, parent, active)| {
                let id = app.world().get::<SpaceId>(entity)?;
                (id.0 == "shared").then_some((entity, parent.parent(), active))
            })
            .collect();
        assert_eq!(shared_views.len(), 2);
        assert!(
            shared_views
                .iter()
                .any(|(_, parent, active)| *parent == first_main && *active)
        );
        assert!(
            shared_views
                .iter()
                .any(|(_, parent, active)| *parent == second_main && *active)
        );
        assert!(!app.world().entity(previous).contains::<Active>());
    }

    #[test]
    fn deleting_a_shared_space_removes_every_view_and_activates_local_fallbacks() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutContractPlugin))
            .add_message::<TabLayoutSpawnRequest>()
            .add_observer(delete);
        let first_window = app.world_mut().spawn_empty().id();
        let second_window = app.world_mut().spawn_empty().id();
        app.world_mut().entity_mut(second_window).insert(Active);
        let first_root = app.world_mut().spawn(HostWindow(first_window)).id();
        let second_root = app.world_mut().spawn(HostWindow(second_window)).id();
        let first_main = app.world_mut().spawn((Main, ChildOf(first_root))).id();
        let second_main = app.world_mut().spawn((Main, ChildOf(second_root))).id();
        for main in [first_main, second_main] {
            app.world_mut().spawn((
                SpaceId("shared".to_string()),
                Name::new("shared"),
                Order(0),
                Active,
                Space::bundle(),
                ChildOf(main),
            ));
        }
        app.world_mut().spawn((
            SpaceId("fallback".to_string()),
            Name::new("fallback"),
            Order(1),
            Space::bundle(),
            ChildOf(first_main),
        ));
        let webview = app.world_mut().spawn(HostWindow(second_window)).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: SpaceDeleteRequest {
                space_id: "shared".to_string(),
            },
        });
        app.update();

        let views: Vec<(String, Entity, bool)> = app
            .world_mut()
            .query_filtered::<(&SpaceId, &ChildOf, Has<Active>), With<Space>>()
            .iter(app.world())
            .map(|(id, parent, active)| (id.0.clone(), parent.parent(), active))
            .collect();
        assert!(views.iter().all(|(id, _, _)| id != "shared"));
        assert_eq!(
            views
                .iter()
                .filter(|(id, _, active)| id == "fallback" && *active)
                .count(),
            2
        );
        assert!(
            views
                .iter()
                .any(|(id, parent, _)| id == "fallback" && *parent == first_main)
        );
        assert!(
            views
                .iter()
                .any(|(id, parent, _)| id == "fallback" && *parent == second_main)
        );
    }

    #[test]
    fn legacy_tab_without_startup_dir_is_not_migrated() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let mut settings = test_settings();
        settings.spaces.insert(
            "work".into(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(first.path().to_string_lossy().into_owned()),
                ..Default::default()
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).insert_resource(settings);
        let space = app
            .world_mut()
            .spawn((
                Space,
                SpaceId("work".into()),
                Name::new("Work"),
                vmux_layout::profile::Profile {
                    name: "Personal".into(),
                },
                CurrentSpace,
            ))
            .id();
        let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();

        app.update();

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            None
        );
        app.world_mut()
            .resource_mut::<AppSettings>()
            .spaces
            .get_mut("work")
            .unwrap()
            .startup_dir = Some(second.path().to_string_lossy().into_owned());

        app.update();

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            None
        );
    }

    #[test]
    fn effective_startup_dir_is_stored_on_each_space() {
        let active_dir = tempfile::tempdir().unwrap();
        let inactive_dir = tempfile::tempdir().unwrap();
        let mut settings = test_settings();
        settings.spaces.insert(
            "active".into(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(active_dir.path().to_string_lossy().into_owned()),
                ..Default::default()
            },
        );
        settings.spaces.insert(
            "inactive".into(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(inactive_dir.path().to_string_lossy().into_owned()),
                ..Default::default()
            },
        );
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutContractPlugin))
            .insert_resource(settings)
            .add_systems(Update, update_effective_startup);
        let inactive = app
            .world_mut()
            .spawn((Space, SpaceId("inactive".into())))
            .id();
        let active = app
            .world_mut()
            .spawn((Space, SpaceId("active".into()), Active))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<EffectiveStartupDir>(active).unwrap().0,
            Some(active_dir.path().to_path_buf())
        );
        assert_eq!(
            app.world().get::<EffectiveStartupDir>(inactive).unwrap().0,
            Some(inactive_dir.path().to_path_buf())
        );
    }

    #[test]
    fn activating_a_project_moves_only_the_space_default() {
        let mut fixture = IdentityFixture::with_projects(
            "/repo/alpha",
            vec![
                SpaceProject::at("/repo/alpha"),
                SpaceProject::at("/repo/beta"),
            ],
        );
        let tabs: Vec<Entity> = ["/repo/alpha", "/repo/beta"]
            .iter()
            .map(|dir| {
                fixture
                    .app
                    .world_mut()
                    .spawn(Tab {
                        startup_dir: Some((*dir).to_string()),
                        ..Default::default()
                    })
                    .id()
            })
            .collect();
        let before: Vec<Option<String>> = tabs
            .iter()
            .map(|tab| {
                fixture
                    .app
                    .world()
                    .get::<Tab>(*tab)
                    .and_then(|t| t.startup_dir.clone())
            })
            .collect();

        fixture.request(ProjectActivateRequest {
            path: "/repo/beta".into(),
            branch: String::new(),
            checkout: String::new(),
            pane_id: None,
        });

        assert_eq!(fixture.project_state().1.as_deref(), Some("/repo/beta"));
        let after: Vec<Option<String>> = tabs
            .iter()
            .map(|tab| {
                fixture
                    .app
                    .world()
                    .get::<Tab>(*tab)
                    .and_then(|t| t.startup_dir.clone())
            })
            .collect();
        assert_eq!(
            before, after,
            "choosing the space's default must leave every open tab where it was"
        );
    }

    #[test]
    fn project_activation_targets_the_space_that_owns_the_pane() {
        let mut settings = test_settings();
        settings.spaces.insert(
            "work".into(),
            SpaceOverrides {
                projects: vec![SpaceProject::at("/repo/work")],
                active_project: Some("/repo/work".into()),
                ..Default::default()
            },
        );
        settings.spaces.insert(
            "other".into(),
            SpaceOverrides {
                projects: vec![
                    SpaceProject::at("/repo/other-a"),
                    SpaceProject::at("/repo/other-b"),
                ],
                active_project: Some("/repo/other-a".into()),
                ..Default::default()
            },
        );
        let mut fixture = IdentityFixture::new();
        fixture.app.insert_resource(settings);
        fixture
            .app
            .world_mut()
            .spawn((Space, SpaceId("work".into()), CurrentSpace));
        let other = fixture
            .app
            .world_mut()
            .spawn((Space, SpaceId("other".into())))
            .id();
        let tab = fixture
            .app
            .world_mut()
            .spawn((Tab::default(), ChildOf(other)))
            .id();
        let pane = fixture
            .app
            .world_mut()
            .spawn((vmux_layout::pane::Pane, ChildOf(tab)))
            .id();

        fixture.request(ProjectActivateRequest {
            path: "/repo/other-b".into(),
            branch: String::new(),
            checkout: String::new(),
            pane_id: Some(pane.to_bits()),
        });

        let settings = fixture.app.world().resource::<AppSettings>();
        assert_eq!(
            settings.space("work").unwrap().active_project.as_deref(),
            Some("/repo/work")
        );
        assert_eq!(
            settings.space("other").unwrap().active_project.as_deref(),
            Some("/repo/other-b")
        );
    }

    #[test]
    fn forgetting_the_active_project_falls_back_to_one_that_remains() {
        let mut fixture = IdentityFixture::with_projects(
            "/repo/beta",
            vec![
                SpaceProject::at("/repo/alpha"),
                SpaceProject::at("/repo/beta"),
            ],
        );

        fixture.request(ProjectForgetRequest {
            path: "/repo/beta".into(),
        });

        assert_eq!(
            fixture.project_state(),
            (vec!["/repo/alpha".to_string()], Some("/repo/alpha".into()))
        );
    }

    #[test]
    fn activating_a_project_the_space_does_not_hold_is_ignored() {
        let mut fixture =
            IdentityFixture::with_projects("/repo/alpha", vec![SpaceProject::at("/repo/alpha")]);

        fixture.request(ProjectActivateRequest {
            path: "/repo/elsewhere".into(),
            branch: String::new(),
            checkout: String::new(),
            pane_id: None,
        });

        assert_eq!(fixture.project_state().1.as_deref(), Some("/repo/alpha"));
    }

    #[test]
    fn missing_startup_dir_remains_unset() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings())
            .add_systems(Update, update_effective_startup);
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("work".into()), Active))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<EffectiveStartupDir>(space).unwrap().0,
            None
        );
    }

    #[test]
    fn unset_startup_dir_is_unchanged_without_relevant_updates() {
        #[derive(Resource, Default)]
        struct ChangeCount(u32);

        fn count_changes(
            effective: Single<Ref<EffectiveStartupDir>, With<Space>>,
            mut count: ResMut<ChangeCount>,
        ) {
            if effective.is_changed() {
                count.0 += 1;
            }
        }

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings())
            .init_resource::<ChangeCount>()
            .add_systems(
                Update,
                (
                    update_effective_startup,
                    count_changes.after(update_effective_startup),
                ),
            );
        app.world_mut()
            .spawn((Space, SpaceId("work".into()), Active));

        app.update();
        app.update();

        assert_eq!(app.world().resource::<ChangeCount>().0, 1);
    }

    #[test]
    fn effective_startup_dir_is_unchanged_without_relevant_updates() {
        #[derive(Resource, Default)]
        struct ChangeCount(u32);

        fn count_changes(
            effective: Single<Ref<EffectiveStartupDir>, With<Space>>,
            mut count: ResMut<ChangeCount>,
        ) {
            if effective.is_changed() {
                count.0 += 1;
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let mut settings = test_settings();
        settings.spaces.insert(
            "work".into(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(dir.path().to_string_lossy().into_owned()),
                ..Default::default()
            },
        );
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutContractPlugin))
            .insert_resource(settings)
            .init_resource::<ChangeCount>()
            .add_systems(
                Update,
                (
                    update_effective_startup,
                    count_changes.after(update_effective_startup),
                ),
            );
        app.world_mut()
            .spawn((Space, SpaceId("work".into()), Active));

        app.update();
        app.update();

        assert_eq!(app.world().resource::<ChangeCount>().0, 1);
    }

    #[test]
    fn effective_startup_dir_re_resolves_when_current_directory_disappears() {
        let primary = tempfile::tempdir().unwrap();
        let fallback = tempfile::tempdir().unwrap();
        let primary_path = primary.path().to_path_buf();
        let mut settings = test_settings();
        settings.terminal = Some(TerminalSettings {
            startup_dir: Some(fallback.path().to_string_lossy().into_owned()),
            ..Default::default()
        });
        settings.spaces.insert(
            "work".into(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(primary_path.to_string_lossy().into_owned()),
                ..Default::default()
            },
        );
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutContractPlugin))
            .insert_resource(settings)
            .add_systems(Update, update_effective_startup);
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("work".into()), Active))
            .id();

        app.update();
        assert_eq!(
            app.world().get::<EffectiveStartupDir>(space).unwrap().0,
            Some(primary_path)
        );

        primary.close().unwrap();
        app.update();

        assert_eq!(
            app.world().get::<EffectiveStartupDir>(space).unwrap().0,
            Some(fallback.path().to_path_buf())
        );
    }

    #[test]
    fn rename_preserves_space_identity_and_tab_membership() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutContractPlugin))
            .add_message::<TabLayoutSpawnRequest>()
            .add_observer(rename);
        app.world_mut().spawn(bevy::window::PrimaryWindow);
        let main = app.world_mut().spawn(Main).id();
        let space = app
            .world_mut()
            .spawn((
                Space,
                SpaceId("rename-src-test".to_string()),
                Name::new("rename-src-test"),
                Active,
                CurrentSpace,
                ChildOf(main),
            ))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab::default(),
                SpaceId("rename-src-test".to_string()),
                LastActivatedAt::now(),
                ChildOf(space),
            ))
            .id();

        app.world_mut().trigger(UiInput {
            webview: Entity::PLACEHOLDER,
            payload: SpaceRenameRequest {
                space_id: "rename-src-test".to_string(),
                name: "Vmux Ai/Vmux".to_string(),
            },
        });
        app.update();

        assert_eq!(
            app.world().get::<SpaceId>(space).map(|s| s.0.clone()),
            Some("rename-src-test".to_string())
        );
        assert_eq!(
            app.world().get::<Name>(space).map(|n| n.to_string()),
            Some("Vmux Ai/Vmux".to_string())
        );
        assert_eq!(
            app.world().get::<SpaceId>(tab).map(|s| s.0.clone()),
            Some("rename-src-test".to_string())
        );
        assert!(app.world().get::<CurrentSpace>(space).is_some());
    }
}
