use bevy::{
    ecs::{relationship::Relationship, system::SystemParam},
    prelude::*,
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
use bevy_cef::prelude::{RequestReload, WebviewRendererTerminatedReceiver};
use vmux_api::{
    VmuxRoute,
    error::{ErrorPageData, FAILED_TO_LOAD, NOT_FOUND},
};

use vmux_command::ReadCommandRequests;
use vmux_ecs::page::{HostHistory, PageMetadataCatalog};
use vmux_ecs::store::PageRestore;
use vmux_ecs::{
    CefPageAttachRequest, PageMetadata, PageNavigation, PageOpenDeferred, PageOpenError,
    PageOpenHandled, PageOpenId, PageOpenRequest, PageOpenSet, PageOpenTarget, PageOpenTask,
};
use vmux_flex::prelude::Visibility;
use vmux_history::LastActivatedAt;
use vmux_layout::Browser;
use vmux_layout::hosted_page::SurfaceBlocked;
use vmux_layout::{
    pane::{Pane, PaneSplit},
    presentation::Retiring,
    stack::{LayoutFocus, Stack},
};
use vmux_ui::i18n::translate;

use crate::host::{
    PageOpenAwaitSnapshot, PageOpenFallbackDeferred, PageOpenResponse, PendingNavigationSnapshot,
    PendingNavigationUpdate,
};

pub(crate) struct PagePlugin;

impl Plugin for PagePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<vmux_ecs::service::ServiceRequest>()
            .add_message::<PageOpenRequest>()
            .add_message::<CefPageAttachRequest>()
            .add_message::<PendingNavigationUpdate>()
            .configure_sets(
                Update,
                (
                    PageOpenSet::ResolveTarget,
                    PageOpenSet::HandleKnownPages,
                    PageOpenSet::Fallback,
                    PageOpenSet::Respond,
                )
                    .chain()
                    .after(ReadCommandRequests),
            )
            .add_systems(
                Update,
                (handle_open_requests, discard_superseded_opens)
                    .chain()
                    .in_set(PageOpenSet::ResolveTarget),
            )
            .add_systems(
                Update,
                (
                    queue_cef_attach_requests,
                    classify_unclaimed,
                    attach_cef_pages,
                    attach_error_pages,
                )
                    .chain()
                    .in_set(PageOpenSet::Fallback),
            )
            .add_systems(Update, respond_open_tasks.in_set(PageOpenSet::Respond))
            .add_systems(Update, recover_renderers.before(PageOpenSet::Fallback))
            .add_systems(
                Update,
                apply_pending_navigation
                    .in_set(crate::BrowserSystemSet::ApplyPendingNavigation)
                    .after(PageOpenSet::Respond)
                    .after(crate::BrowserSystemSet::Navigate),
            );
    }
}

#[derive(Component)]
struct CefPageAttachment {
    stack: Entity,
    url: String,
    title: String,
    bg_color: Option<String>,
}

impl From<&CefPageAttachRequest> for CefPageAttachment {
    fn from(request: &CefPageAttachRequest) -> Self {
        Self {
            stack: request.stack,
            url: request.url.clone(),
            title: request.title.clone(),
            bg_color: request.bg_color.clone(),
        }
    }
}

#[derive(Component)]
struct ErrorPageAttachment {
    stack: Entity,
    failure: ErrorPageData,
}

#[derive(Component)]
struct RendererRecovery {
    attempts: u8,
    last_failure: std::time::Instant,
    retry_at: Option<std::time::Instant>,
    exhausted: bool,
}

impl RendererRecovery {
    const MAX_ATTEMPTS: u8 = 3;
    const RESET_AFTER: std::time::Duration = std::time::Duration::from_secs(30);
    const RETRY_BASE: std::time::Duration = std::time::Duration::from_millis(250);

    fn after(previous: Option<&Self>, now: std::time::Instant) -> Self {
        let attempts = previous
            .filter(|recovery| {
                now.saturating_duration_since(recovery.last_failure) < Self::RESET_AFTER
            })
            .map_or(1, |recovery| recovery.attempts.saturating_add(1));
        let exhausted = attempts > Self::MAX_ATTEMPTS;
        let retry_at = if exhausted {
            None
        } else {
            let multiplier = 1u32 << attempts.saturating_sub(1);
            now.checked_add(Self::RETRY_BASE.saturating_mul(multiplier))
        };
        Self {
            attempts,
            last_failure: now,
            retry_at,
            exhausted,
        }
    }
}

fn recover_renderers(
    receiver: Option<Res<WebviewRendererTerminatedReceiver>>,
    recoveries: Query<(Entity, &RendererRecovery)>,
    parents: Query<&ChildOf>,
    metadata: Query<&PageMetadata>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Some(receiver) = receiver else {
        return;
    };
    let now = std::time::Instant::now();
    let mut terminated = bevy::ecs::entity::EntityHashMap::default();
    while let Ok(event) = receiver.0.try_recv() {
        terminated.insert(event.webview, event);
    }
    for (webview, event) in terminated {
        let previous = recoveries.get(webview).ok().map(|(_, recovery)| recovery);
        if previous.is_some_and(|recovery| recovery.exhausted) {
            continue;
        }
        let recovery = RendererRecovery::after(previous, now);
        let retry_at = recovery.retry_at;
        let exhausted = recovery.exhausted;
        let Ok(mut webview_commands) = commands.get_entity(webview) else {
            continue;
        };
        webview_commands.try_insert(recovery);
        if exhausted {
            let Ok(parent) = parents.get(webview) else {
                continue;
            };
            let url = metadata
                .get(webview)
                .map(|metadata| metadata.url.as_str())
                .unwrap_or_default();
            let message = format!(
                "CEF renderer terminated repeatedly (status {}, code {}): {}",
                event.status, event.error_code, event.error
            );
            commands.spawn(ErrorPageAttachment::failed(parent.parent(), url, &message));
            continue;
        }
        let (Some(retry_at), Some(proxy)) = (retry_at, proxy.as_deref()) else {
            continue;
        };
        let proxy = (**proxy).clone();
        std::thread::spawn(move || {
            std::thread::sleep(retry_at.saturating_duration_since(std::time::Instant::now()));
            let _ = proxy.send_event(WinitUserEvent::WakeUp);
        });
    }

    for (webview, recovery) in &recoveries {
        if recovery.retry_at.is_none_or(|retry_at| retry_at > now) {
            continue;
        }
        commands.trigger(RequestReload { webview });
        commands.entity(webview).try_insert(RendererRecovery {
            attempts: recovery.attempts,
            last_failure: recovery.last_failure,
            retry_at: None,
            exhausted: false,
        });
    }
}

fn apply_pending_navigation(
    mut updates: MessageReader<PendingNavigationUpdate>,
    existing: Query<(Entity, &PendingNavigationSnapshot)>,
    mut commands: Commands,
    mut service_requests: MessageWriter<vmux_ecs::service::ServiceRequest>,
) {
    let mut pending = existing
        .iter()
        .map(|(entity, operation)| (operation.webview, (entity, operation.clone())))
        .collect::<bevy::ecs::entity::EntityHashMap<_>>();
    for update in updates.read() {
        if let Some((entity, displaced)) = pending.remove(&update.webview) {
            commands.entity(entity).despawn();
            if let Some(response) = PageOpenResponse::from_result(
                Some(displaced.request_id),
                Err("navigation snapshot superseded by a newer request".to_string()),
            ) {
                service_requests.write(response);
            }
        }
        if let Some(next) = update.pending.clone() {
            let entity = commands.spawn(next.clone()).id();
            pending.insert(update.webview, (entity, next));
        }
    }
}

impl ErrorPageAttachment {
    fn failed(stack: Entity, url: &str, message: &str) -> Self {
        Self {
            stack,
            failure: ErrorPageData {
                title_message_id: FAILED_TO_LOAD.to_string(),
                message: message.to_string(),
                url: url.to_string(),
            },
        }
    }

    fn not_found(stack: Entity, url: &str) -> Self {
        Self {
            stack,
            failure: ErrorPageData {
                title_message_id: NOT_FOUND.to_string(),
                message: String::new(),
                url: url.to_string(),
            },
        }
    }
}

fn handle_open_requests(
    mut reader: MessageReader<PageOpenRequest>,
    target: PageOpenTargetResolver,
    metadata: PageMetadataCatalog,
    restoring_stacks: Query<(), (With<Stack>, With<PageRestore>)>,
    histories: Query<(), With<HostHistory>>,
    time: Res<Time>,
    mut service_requests: MessageWriter<vmux_ecs::service::ServiceRequest>,
    mut commands: Commands,
) {
    let mut restore_owners = bevy::ecs::entity::EntityHashMap::<Entity>::default();
    for request in reader.read() {
        let url =
            VmuxRoute::canonical(&request.url).unwrap_or_else(|| request.url.trim().to_string());
        let stack = match target.resolve(&request.target) {
            Ok(PageOpenTargetResolution::Existing(stack)) => stack,
            Ok(PageOpenTargetResolution::CreateIn { pane, previous }) => {
                let mut stack =
                    commands.spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(pane)));
                stack.insert(metadata.metadata_for(&url, &url));
                if let Some(previous) = previous {
                    stack.insert(vmux_layout::presentation::Transition::retain(previous));
                }
                stack.id()
            }
            Err(message) => {
                if let Some(response) =
                    PageOpenResponse::from_result(request.request_id, Err(message))
                {
                    service_requests.write(response);
                }
                continue;
            }
        };
        let task = PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url,
            request_id: request.request_id,
        };
        commands.entity(stack).insert((
            PageNavigation::new(task.id),
            metadata.metadata_for(&task.url, &task.url),
        ));
        if !histories.contains(stack) {
            commands.entity(stack).insert(HostHistory::default());
        }
        let restoring = restoring_stacks.contains(stack);
        if restoring {
            commands.entity(stack).remove::<PageRestore>();
        }
        let task_entity = commands.spawn(task).id();
        if request.request_id.is_some() {
            commands.entity(task_entity).insert(PageOpenAwaitSnapshot {
                started: time.elapsed(),
            });
        }
        if restoring {
            if let Some(previous) = restore_owners.insert(stack, task_entity) {
                commands.entity(previous).remove::<PageRestore>();
            }
            commands.entity(task_entity).insert(PageRestore);
        }
    }
}

fn discard_superseded_opens(
    tasks: Query<(Entity, &PageOpenTask), Without<PageOpenHandled>>,
    stacks: Query<(), (With<Stack>, Without<Retiring>)>,
    navigations: Query<&PageNavigation>,
    mut commands: Commands,
    mut service_requests: MessageWriter<vmux_ecs::service::ServiceRequest>,
) {
    for (entity, task) in &tasks {
        let superseded = !stacks.contains(task.stack)
            || !navigations
                .get(task.stack)
                .is_ok_and(|navigation| navigation.matches(task.id));
        if superseded {
            if let Some(response) = PageOpenResponse::from_result(
                task.request_id,
                Err("page open superseded by a newer request".to_string()),
            ) {
                service_requests.write(response);
            }
            commands.entity(entity).despawn();
        }
    }
}

enum PageOpenTargetResolution {
    Existing(Entity),
    CreateIn {
        pane: Entity,
        previous: Option<Entity>,
    },
}

#[derive(SystemParam)]
struct PageOpenTargetResolver<'w, 's> {
    focus: vmux_layout::stack::FocusedStack<'w, 's>,
    layout: LayoutFocus<'w, 's>,
    parents: Query<'w, 's, &'static ChildOf>,
    panes: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>)>,
    stacks: Query<'w, 's, Entity, (With<Stack>, Without<Retiring>)>,
    presentation: vmux_layout::presentation::Projection<'w, 's>,
}

impl PageOpenTargetResolver<'_, '_> {
    fn create_in(&self, pane: Entity) -> PageOpenTargetResolution {
        let previous = self
            .layout
            .stack(pane)
            .map(|stack| self.reveal_previous(stack));
        PageOpenTargetResolution::CreateIn { pane, previous }
    }

    fn reveal_previous(&self, stack: Entity) -> Entity {
        self.presentation.retained(stack).unwrap_or(stack)
    }

    fn resolve(&self, target: &PageOpenTarget) -> Result<PageOpenTargetResolution, String> {
        match *target {
            PageOpenTarget::ActiveStack => {
                if let Some(stack) = self
                    .focus
                    .stack
                    .filter(|stack| self.stacks.contains(*stack))
                {
                    return Ok(PageOpenTargetResolution::Existing(stack));
                }
                let Some(pane) = self.focus.pane.filter(|pane| self.panes.contains(*pane)) else {
                    return Err("page_open: no focused stack or pane".to_string());
                };
                Ok(self.create_in(pane))
            }
            PageOpenTarget::NewStack => {
                let Some(pane) = self.focus.pane.filter(|pane| self.panes.contains(*pane)) else {
                    return Err("page_open: no focused pane".to_string());
                };
                Ok(self.create_in(pane))
            }
            PageOpenTarget::Stack(stack) => self
                .stacks
                .contains(stack)
                .then_some(PageOpenTargetResolution::Existing(stack))
                .ok_or_else(|| "page_open: target stack does not exist".to_string()),
            PageOpenTarget::ContainingStack(entity) => self
                .containing_stack(entity)
                .map(PageOpenTargetResolution::Existing),
            PageOpenTarget::ActiveStackInPane(pane) => {
                if !self.panes.contains(pane) {
                    return Err("page_open: target pane does not exist".to_string());
                }
                Ok(match self.layout.stack(pane) {
                    Some(stack) => PageOpenTargetResolution::Existing(stack),
                    None => self.create_in(pane),
                })
            }
            PageOpenTarget::NewStackInPane(pane) => {
                if !self.panes.contains(pane) {
                    return Err("page_open: target pane does not exist".to_string());
                }
                Ok(self.create_in(pane))
            }
        }
    }

    fn containing_stack(&self, entity: Entity) -> Result<Entity, String> {
        let mut current = entity;
        loop {
            if self.stacks.contains(current) {
                return Ok(current);
            }
            let Ok(parent) = self.parents.get(current) else {
                return Err("page_open: target has no containing stack".to_string());
            };
            current = parent.parent();
        }
    }
}

fn queue_cef_attach_requests(
    mut reader: MessageReader<CefPageAttachRequest>,
    mut commands: Commands,
) {
    for request in reader.read() {
        commands.spawn(CefPageAttachment::from(request));
    }
}

fn classify_unclaimed(
    tasks: Query<
        (
            Entity,
            &PageOpenTask,
            Option<&PageOpenError>,
            Option<&PageOpenFallbackDeferred>,
        ),
        (
            Without<PageOpenHandled>,
            Without<PageOpenDeferred>,
            Without<CefPageAttachment>,
            Without<ErrorPageAttachment>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, task, error, deferred_once) in &tasks {
        if let Some(error) = error {
            commands.entity(entity).insert(ErrorPageAttachment::failed(
                task.stack,
                &task.url,
                &error.message,
            ));
        } else if VmuxRoute::parse(&task.url).is_some_and(|route| route.is_host("error")) {
            commands.entity(entity).insert(ErrorPageAttachment::failed(
                task.stack, &task.url, &task.url,
            ));
        } else if VmuxRoute::parse(&task.url).is_some() {
            if deferred_once.is_none() {
                commands.entity(entity).insert(PageOpenFallbackDeferred);
                continue;
            }
            commands.entity(entity).insert((
                ErrorPageAttachment::not_found(task.stack, &task.url),
                PageOpenError {
                    message: format!("unknown vmux URL '{}'", task.url),
                },
            ));
        } else {
            commands.entity(entity).insert(CefPageAttachment {
                stack: task.stack,
                url: task.url.clone(),
                title: task.url.clone(),
                bg_color: None,
            });
        }
    }
}

fn attach_cef_pages(
    attachments: Query<(Entity, &CefPageAttachment, Option<&PageOpenTask>)>,
    stack_metadata: Query<&PageMetadata, With<Stack>>,
    children: Query<&Children>,
    time: Res<Time>,
    mut commands: Commands,
) {
    for (entity, attachment, task) in &attachments {
        let navigation = task.map(|task| task.id).unwrap_or_default();
        let previous = children
            .get(attachment.stack)
            .map(|children| children.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for surface in &previous {
            commands.entity(*surface).try_insert(SurfaceBlocked);
        }
        let metadata = task
            .and_then(|_| stack_metadata.get(attachment.stack).ok())
            .filter(|metadata| metadata.url == attachment.url)
            .cloned()
            .unwrap_or_else(|| PageMetadata {
                url: attachment.url.clone(),
                title: attachment.title.clone(),
                bg_color: attachment.bg_color.clone(),
                ..default()
            });
        let committed = stack_metadata
            .get(attachment.stack)
            .ok()
            .filter(|metadata| !metadata.url.is_empty() && metadata.url != attachment.url)
            .cloned();
        if task.is_none() {
            commands
                .entity(attachment.stack)
                .insert(PageNavigation::new(navigation));
        }
        commands.entity(attachment.stack).insert(metadata.clone());
        let browser = commands
            .spawn((
                Browser::new_with_title(&attachment.url, &metadata.title),
                ChildOf(attachment.stack),
            ))
            .id();
        commands.entity(browser).insert((
            metadata,
            Visibility::Hidden,
            vmux_layout::hosted_page::SurfaceHandoff::new(previous, navigation, 0, time.elapsed())
                .with_fallback(committed),
        ));
        if task.is_some() {
            commands
                .entity(entity)
                .remove::<CefPageAttachment>()
                .insert(PageOpenHandled);
        } else {
            commands.entity(entity).despawn();
        }
    }
}

fn attach_error_pages(
    attachments: Query<(Entity, &ErrorPageAttachment, Option<&PageOpenTask>)>,
    stack_metadata: Query<&PageMetadata, With<Stack>>,
    children: Query<&Children>,
    time: Res<Time>,
    mut commands: Commands,
) {
    for (entity, attachment, task) in &attachments {
        let title = if attachment.failure.title_message_id.is_empty() {
            translate("error-title")
        } else {
            translate(&attachment.failure.title_message_id)
        };
        let navigation = task.map(|task| task.id).unwrap_or_default();
        let previous = children
            .get(attachment.stack)
            .map(|children| children.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for surface in &previous {
            commands.entity(*surface).try_insert(SurfaceBlocked);
        }
        let metadata = PageMetadata {
            url: attachment.failure.url.clone(),
            title: title.clone(),
            ..default()
        };
        let committed = stack_metadata
            .get(attachment.stack)
            .ok()
            .filter(|current| !current.url.is_empty() && current.url != metadata.url)
            .cloned();
        if task.is_none() {
            commands
                .entity(attachment.stack)
                .insert(PageNavigation::new(navigation));
        }
        commands.entity(attachment.stack).insert(metadata.clone());
        let browser = commands
            .spawn((
                Browser::hosted_page(vmux_layout::ErrorPage::URL, &title),
                attachment.failure.clone(),
                ChildOf(attachment.stack),
            ))
            .id();
        commands.entity(browser).insert((
            metadata,
            Visibility::Hidden,
            vmux_layout::hosted_page::SurfaceHandoff::new(previous, navigation, 0, time.elapsed())
                .with_fallback(committed),
        ));
        if task.is_some() {
            commands
                .entity(entity)
                .remove::<ErrorPageAttachment>()
                .insert(PageOpenHandled);
        } else {
            commands.entity(entity).despawn();
        }
    }
}

fn respond_open_tasks(
    tasks: Query<
        (
            Entity,
            &PageOpenTask,
            Option<&PageOpenError>,
            Option<&PageOpenAwaitSnapshot>,
        ),
        With<PageOpenHandled>,
    >,
    time: Res<Time>,
    surfaces: vmux_layout::hosted_page::StackSurface,
    navigations: Query<&PageNavigation>,
    child_of: Query<&ChildOf>,
    mut pending_navigation: MessageWriter<PendingNavigationUpdate>,
    mut commands: Commands,
    mut service_requests: MessageWriter<vmux_ecs::service::ServiceRequest>,
) {
    for (entity, task, error, await_snapshot) in &tasks {
        let current = navigations
            .get(task.stack)
            .is_ok_and(|navigation| navigation.matches(task.id));
        if !current {
            if let Some(response) = PageOpenResponse::from_result(
                task.request_id,
                Err("page open superseded by a newer request".to_string()),
            ) {
                service_requests.write(response);
            }
            commands.entity(entity).despawn();
            continue;
        }
        commands.entity(task.stack).remove::<PageRestore>();
        if let Some(error) = error {
            if let Some(response) =
                PageOpenResponse::from_result(task.request_id, Err(error.message.clone()))
            {
                service_requests.write(response);
            }
            commands.entity(entity).despawn();
            continue;
        }
        let Some(await_snapshot) = await_snapshot else {
            if let Some(response) = PageOpenResponse::from_result(task.request_id, Ok(())) {
                service_requests.write(response);
            }
            commands.entity(entity).despawn();
            continue;
        };
        let webview = surfaces.incoming_for(task.stack, task.id);
        if let (Some(webview), Some(request_id)) = (webview, task.request_id) {
            let pane = child_of
                .get(task.stack)
                .ok()
                .map(|child_of| child_of.get().to_bits().to_string());
            pending_navigation.write(PendingNavigationUpdate::for_page_open(
                webview,
                request_id,
                await_snapshot.started,
                pane,
                task.stack,
                task.id,
            ));
            commands.entity(entity).despawn();
        } else if time
            .elapsed()
            .saturating_sub(await_snapshot.started)
            .as_secs_f32()
            > 10.0
        {
            if let Some(response) = PageOpenResponse::from_result(
                task.request_id,
                Err("page opened without a snapshot-capable webview".to_string()),
            ) {
                service_requests.write(response);
            }
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::SystemState;

    #[test]
    fn active_target_never_reuses_a_retiring_stack() {
        let mut app = App::new();
        let pane = app.world_mut().spawn(Pane).id();
        let retiring = app
            .world_mut()
            .spawn((Stack::bundle(), Retiring, ChildOf(pane)))
            .id();
        app.world_mut().spawn(
            vmux_layout::active_pane::ActiveStack {
                tab: None,
                pane: Some(pane),
                stack: Some(retiring),
            }
            .local_bundle(),
        );

        let result = {
            let world = app.world_mut();
            let mut state = SystemState::<PageOpenTargetResolver>::new(world);
            state
                .get(world)
                .unwrap()
                .resolve(&PageOpenTarget::ActiveStack)
        };

        assert!(matches!(
            result,
            Ok(PageOpenTargetResolution::CreateIn {
                pane: target,
                previous: None,
            }) if target == pane
        ));
    }

    #[test]
    fn latest_same_frame_page_open_owns_restore() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<PageOpenRequest>()
            .add_message::<vmux_ecs::service::ServiceRequest>()
            .add_systems(Update, handle_open_requests);
        let stack = app.world_mut().spawn((Stack::default(), PageRestore)).id();
        for url in ["vmux://sessions/first", "vmux://sessions/second"] {
            app.world_mut().write_message(PageOpenRequest {
                target: PageOpenTarget::Stack(stack),
                url: url.into(),
                request_id: None,
            });
        }

        app.update();

        let world = app.world_mut();
        let mut restored = world.query_filtered::<&PageOpenTask, With<PageRestore>>();
        let restored = restored.iter(world).collect::<Vec<_>>();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].url, "vmux://sessions/second");
        assert!(world.get::<PageRestore>(stack).is_none());
    }

    #[test]
    fn superseded_task_does_not_remove_a_newer_restore_owner() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<vmux_ecs::service::ServiceRequest>()
            .add_systems(Update, discard_superseded_opens);
        let stale = PageOpenId::new();
        let current = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((PageNavigation::new(current), PageRestore))
            .id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: stale,
                stack,
                url: "vmux://projects/".into(),
                request_id: None,
            })
            .id();

        app.update();

        assert!(app.world().get_entity(task).is_err());
        assert!(app.world().get::<PageRestore>(stack).is_some());
    }

    #[test]
    fn page_open_is_canceled_when_its_stack_is_gone() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<vmux_ecs::service::ServiceRequest>()
            .add_systems(Update, discard_superseded_opens);
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack: Entity::PLACEHOLDER,
                url: "vmux://sessions/".into(),
                request_id: Some([7; 16]),
            })
            .id();

        app.update();

        assert!(app.world().get_entity(task).is_err());
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<vmux_ecs::service::ServiceRequest>>()
                .drain()
                .count(),
            1
        );
    }

    #[test]
    fn page_open_is_canceled_when_its_navigation_is_missing() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<vmux_ecs::service::ServiceRequest>()
            .add_systems(Update, discard_superseded_opens);
        let stack = app.world_mut().spawn(Stack::default()).id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://sessions/".into(),
                request_id: Some([8; 16]),
            })
            .id();

        app.update();

        assert!(app.world().get_entity(task).is_err());
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<vmux_ecs::service::ServiceRequest>>()
                .drain()
                .count(),
            1
        );
    }

    #[test]
    fn page_open_snapshot_waits_for_its_requested_surface() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<PendingNavigationUpdate>()
            .add_message::<vmux_ecs::service::ServiceRequest>()
            .add_systems(Update, respond_open_tasks);
        let request = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((Stack::bundle(), PageNavigation::new(request)))
            .id();
        let old_surface = app.world_mut().spawn((Browser, ChildOf(stack))).id();
        app.world_mut()
            .entity_mut(stack)
            .insert(vmux_layout::page_slot::Current::new(
                Some(old_surface),
                PageMetadata::default(),
            ));
        let task = app
            .world_mut()
            .spawn((
                PageOpenTask {
                    id: request,
                    stack,
                    url: "vmux://projects/".into(),
                    request_id: Some([9; 16]),
                },
                PageOpenHandled,
                PageOpenAwaitSnapshot {
                    started: std::time::Duration::ZERO,
                },
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(task).is_ok());
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<PendingNavigationUpdate>>()
                .drain()
                .count(),
            0
        );
    }

    #[test]
    fn renderer_recovery_uses_bounded_exponential_backoff() {
        let now = std::time::Instant::now();
        let first = RendererRecovery::after(None, now);
        assert_eq!(
            first.retry_at.unwrap().duration_since(now),
            std::time::Duration::from_millis(250)
        );

        let second = RendererRecovery::after(Some(&first), now);
        assert_eq!(
            second.retry_at.unwrap().duration_since(now),
            std::time::Duration::from_millis(500)
        );

        let third = RendererRecovery::after(Some(&second), now);
        assert_eq!(
            third.retry_at.unwrap().duration_since(now),
            std::time::Duration::from_secs(1)
        );

        let exhausted = RendererRecovery::after(Some(&third), now);
        assert!(exhausted.exhausted);
        assert!(exhausted.retry_at.is_none());
    }

    #[test]
    fn renderer_recovery_resets_after_a_stable_window() {
        let started = std::time::Instant::now();
        let previous = RendererRecovery::after(None, started);
        let recovered =
            RendererRecovery::after(Some(&previous), started + RendererRecovery::RESET_AFTER);

        assert_eq!(recovered.attempts, 1);
        assert!(!recovered.exhausted);
    }
}
