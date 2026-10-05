use bevy::{
    ecs::{relationship::Relationship, system::SystemParam},
    prelude::*,
};
use vmux_api::{
    VmuxRoute,
    error::{ErrorPageData, FAILED_TO_LOAD, NOT_FOUND},
};

use vmux_command::ReadCommandRequests;
use vmux_ecs::persistence::PageRestore;
use vmux_ecs::{
    CefPageAttachRequest, PageMetadata, PageOpenDeferred, PageOpenError, PageOpenHandled,
    PageOpenId, PageOpenRequest, PageOpenSet, PageOpenTarget, PageOpenTask,
};
use vmux_history::LastActivatedAt;
use vmux_layout::Browser;
use vmux_layout::{
    pane::{Pane, PaneSplit},
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
                handle_open_requests.in_set(PageOpenSet::ResolveTarget),
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
            if let Some(response) =
                PageOpenResponse::from_result(Some(displaced.request_id), Ok(()))
            {
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
    time: Res<Time>,
    mut service_requests: MessageWriter<vmux_ecs::service::ServiceRequest>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let stack = match target.resolve(&request.target) {
            Ok(PageOpenTargetResolution::Existing(stack)) => stack,
            Ok(PageOpenTargetResolution::CreateIn(pane)) => commands
                .spawn((Stack::bundle(), LastActivatedAt::now(), ChildOf(pane)))
                .id(),
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
            url: VmuxRoute::canonical(&request.url)
                .unwrap_or_else(|| request.url.trim().to_string()),
            request_id: request.request_id,
        };
        if request.request_id.is_some() {
            commands.spawn((
                task,
                PageOpenAwaitSnapshot {
                    started: time.elapsed(),
                },
            ));
        } else {
            commands.spawn(task);
        }
    }
}

enum PageOpenTargetResolution {
    Existing(Entity),
    CreateIn(Entity),
}

#[derive(SystemParam)]
struct PageOpenTargetResolver<'w, 's> {
    focus: vmux_layout::stack::FocusedStack<'w, 's>,
    layout: LayoutFocus<'w, 's>,
    parents: Query<'w, 's, &'static ChildOf>,
    panes: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>)>,
    stacks: Query<'w, 's, Entity, With<Stack>>,
}

impl PageOpenTargetResolver<'_, '_> {
    fn resolve(&self, target: &PageOpenTarget) -> Result<PageOpenTargetResolution, String> {
        match *target {
            PageOpenTarget::ActiveStack => {
                if let Some(stack) = self.focus.stack {
                    return Ok(PageOpenTargetResolution::Existing(stack));
                }
                let Some(pane) = self.focus.pane.filter(|pane| self.panes.contains(*pane)) else {
                    return Err("page_open: no focused stack or pane".to_string());
                };
                Ok(PageOpenTargetResolution::CreateIn(pane))
            }
            PageOpenTarget::NewStack => {
                let Some(pane) = self.focus.pane.filter(|pane| self.panes.contains(*pane)) else {
                    return Err("page_open: no focused pane".to_string());
                };
                Ok(PageOpenTargetResolution::CreateIn(pane))
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
                    None => PageOpenTargetResolution::CreateIn(pane),
                })
            }
            PageOpenTarget::NewStackInPane(pane) => {
                if !self.panes.contains(pane) {
                    return Err("page_open: target pane does not exist".to_string());
                }
                Ok(PageOpenTargetResolution::CreateIn(pane))
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
    mut commands: Commands,
) {
    for (entity, attachment, task) in &attachments {
        commands.entity(attachment.stack).despawn_children();
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
        commands.entity(attachment.stack).insert(metadata.clone());
        let browser = commands
            .spawn((
                Browser::new_with_title(&attachment.url, &metadata.title),
                ChildOf(attachment.stack),
            ))
            .id();
        commands
            .entity(browser)
            .insert((vmux_ecs::KeyboardOwner, metadata));
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
    mut commands: Commands,
) {
    for (entity, attachment, task) in &attachments {
        let title = if attachment.failure.title_message_id.is_empty() {
            translate("error-title")
        } else {
            translate(&attachment.failure.title_message_id)
        };
        commands.entity(attachment.stack).despawn_children();
        commands.entity(attachment.stack).insert(PageMetadata {
            url: attachment.failure.url.clone(),
            title: title.clone(),
            ..default()
        });
        commands.spawn((
            Browser::hosted_page(vmux_layout::ErrorPage::URL, &title),
            attachment.failure.clone(),
            ChildOf(attachment.stack),
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
    children: Query<&Children>,
    browsers: Query<(), With<Browser>>,
    child_of: Query<&ChildOf>,
    mut pending_navigation: MessageWriter<PendingNavigationUpdate>,
    mut commands: Commands,
    mut service_requests: MessageWriter<vmux_ecs::service::ServiceRequest>,
) {
    for (entity, task, error, await_snapshot) in &tasks {
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
        let webview = children
            .get(task.stack)
            .ok()
            .and_then(|children| children.iter().find(|child| browsers.contains(*child)));
        if let (Some(webview), Some(request_id)) = (webview, task.request_id) {
            let pane = child_of
                .get(task.stack)
                .ok()
                .map(|child_of| child_of.get().to_bits().to_string());
            pending_navigation.write(PendingNavigationUpdate::set(
                webview,
                request_id,
                await_snapshot.started,
                pane,
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
