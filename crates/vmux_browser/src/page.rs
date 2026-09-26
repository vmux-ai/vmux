use bevy::{ecs::relationship::Relationship, prelude::*};
use vmux_api::{VmuxRoute, error::ErrorPageData};

use vmux_command::ReadCommandRequests;
use vmux_core::{
    CefPageAttachRequest, PageMetadata, PageOpenDeferred, PageOpenError, PageOpenHandled,
    PageOpenId, PageOpenRequest, PageOpenSet, PageOpenTarget, PageOpenTask,
};
use vmux_history::LastActivatedAt;
use vmux_layout::Browser;
use vmux_layout::{
    pane::{Pane, PaneSplit, first_stack_in_pane},
    stack::{Stack, active_stack_in_pane, stack_bundle},
};

use crate::{
    PageOpenAwaitSnapshot, PageOpenFallbackDeferred, PendingNavigationUpdate,
    apply_pending_navigation_updates, send_page_open_response,
};

pub(crate) struct PagePlugin;

impl Plugin for PagePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PageOpenRequest>()
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
                handle_page_open_requests.in_set(PageOpenSet::ResolveTarget),
            )
            .add_systems(
                Update,
                (
                    queue_cef_page_attach_requests,
                    classify_unclaimed_page_open_tasks,
                    attach_cef_pages,
                    attach_error_pages,
                )
                    .chain()
                    .in_set(PageOpenSet::Fallback),
            )
            .add_systems(Update, respond_page_open_tasks.in_set(PageOpenSet::Respond))
            .add_systems(
                Update,
                apply_pending_navigation_updates
                    .after(PageOpenSet::Respond)
                    .after(crate::navigation::handle_browser_navigate_requests),
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

fn handle_page_open_requests(
    mut reader: MessageReader<PageOpenRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_filter: Query<Entity, With<Stack>>,
    service: Option<Single<&vmux_service::client::ServiceClient>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let stack = match resolve_page_open_target(
            &request.target,
            &focus,
            &panes,
            &pane_children,
            &stack_ts,
            &stack_filter,
            &mut commands,
        ) {
            Ok(stack) => stack,
            Err(message) => {
                send_page_open_response(&service, request.request_id, Err(message));
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

fn resolve_page_open_target(
    target: &PageOpenTarget,
    focus: &vmux_layout::stack::FocusedStack,
    panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: &Query<&Children, With<Pane>>,
    stack_ts: &Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_filter: &Query<Entity, With<Stack>>,
    commands: &mut Commands,
) -> Result<Entity, String> {
    match *target {
        PageOpenTarget::ActiveStack => focus
            .stack
            .or_else(|| {
                focus.pane.filter(|pane| panes.contains(*pane)).map(|pane| {
                    commands
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                        .id()
                })
            })
            .ok_or_else(|| "page_open: no focused stack or pane".to_string()),
        PageOpenTarget::Stack(stack) => {
            if stack_filter.contains(stack) {
                Ok(stack)
            } else {
                Err("page_open: target stack does not exist".to_string())
            }
        }
        PageOpenTarget::ActiveStackInPane(pane) => {
            if !panes.contains(pane) {
                return Err("page_open: target pane does not exist".to_string());
            }
            Ok(active_stack_in_pane(pane, pane_children, stack_ts)
                .or_else(|| first_stack_in_pane(pane, pane_children, stack_filter))
                .unwrap_or_else(|| {
                    commands
                        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                        .id()
                }))
        }
        PageOpenTarget::NewStackInPane(pane) => {
            if panes.contains(pane) {
                Ok(commands
                    .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                    .id())
            } else {
                Err("page_open: target pane does not exist".to_string())
            }
        }
    }
}

fn queue_cef_page_attach_requests(
    mut reader: MessageReader<CefPageAttachRequest>,
    mut commands: Commands,
) {
    for request in reader.read() {
        commands.spawn(CefPageAttachment::from(request));
    }
}

fn classify_unclaimed_page_open_tasks(
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
            commands.entity(entity).insert(ErrorPageAttachment {
                stack: task.stack,
                failure: ErrorPageData::failed_to_load(&task.url, &error.message),
            });
        } else if VmuxRoute::parse(&task.url).is_some_and(|route| route.is_host("error")) {
            commands.entity(entity).insert(ErrorPageAttachment {
                stack: task.stack,
                failure: ErrorPageData::failed_to_load(&task.url, &task.url),
            });
        } else if VmuxRoute::parse(&task.url).is_some() {
            if deferred_once.is_none() {
                commands.entity(entity).insert(PageOpenFallbackDeferred);
                continue;
            }
            commands.entity(entity).insert((
                ErrorPageAttachment {
                    stack: task.stack,
                    failure: ErrorPageData::not_found(&task.url),
                },
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
    children: Query<&Children>,
    mut commands: Commands,
) {
    for (entity, attachment, task) in &attachments {
        vmux_layout::stack::clear_stack_children(attachment.stack, &children, &mut commands);
        commands.entity(attachment.stack).insert(PageMetadata {
            url: attachment.url.clone(),
            title: attachment.title.clone(),
            bg_color: attachment.bg_color.clone(),
            ..default()
        });
        let browser = commands
            .spawn((
                Browser::new_with_title(&attachment.url, &attachment.title),
                ChildOf(attachment.stack),
            ))
            .id();
        commands.entity(browser).insert(vmux_core::KeyboardOwner);
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
    children: Query<&Children>,
    mut commands: Commands,
) {
    for (entity, attachment, task) in &attachments {
        vmux_layout::stack::clear_stack_children(attachment.stack, &children, &mut commands);
        commands.entity(attachment.stack).insert(PageMetadata {
            url: attachment.failure.url.clone(),
            title: attachment.failure.title.clone(),
            ..default()
        });
        commands.spawn((
            Browser::native_page(vmux_api::error::ERROR_PAGE_URL, &attachment.failure.title),
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

fn respond_page_open_tasks(
    tasks: Query<
        (
            Entity,
            &PageOpenTask,
            Option<&PageOpenError>,
            Option<&PageOpenAwaitSnapshot>,
        ),
        With<PageOpenHandled>,
    >,
    service: Option<Single<&vmux_service::client::ServiceClient>>,
    time: Res<Time>,
    children: Query<&Children>,
    browsers: Query<(), With<Browser>>,
    child_of: Query<&ChildOf>,
    mut pending_navigation: MessageWriter<PendingNavigationUpdate>,
    mut commands: Commands,
) {
    for (entity, task, error, await_snapshot) in &tasks {
        if let Some(error) = error {
            send_page_open_response(&service, task.request_id, Err(error.message.clone()));
            commands.entity(entity).despawn();
            continue;
        }
        let Some(await_snapshot) = await_snapshot else {
            send_page_open_response(&service, task.request_id, Ok(()));
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
            send_page_open_response(
                &service,
                task.request_id,
                Err("page opened without a snapshot-capable webview".to_string()),
            );
            commands.entity(entity).despawn();
        }
    }
}
