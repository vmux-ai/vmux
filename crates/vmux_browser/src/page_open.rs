use bevy::{ecs::relationship::Relationship, prelude::*};
use vmux_wire::error::ErrorPageData;

use vmux_core::{
    CefPageAttachRequest, PageOpenError, PageOpenHandled, PageOpenId, PageOpenRequest, PageOpenSet,
    PageOpenTarget, PageOpenTask,
};
use vmux_history::LastActivatedAt;
use vmux_layout::Browser;
use vmux_layout::{
    pane::{Pane, PaneSplit, first_stack_in_pane},
    stack::{Stack, active_stack_in_pane, stack_bundle},
};

use crate::{
    NavPending, PageOpenAwaitSnapshot, PageOpenFallbackDeferred, PendingNavSnapshots,
    attach_cef_page_to_stack, attach_error_page_to_stack, normalize_vmux_url,
    send_page_open_response,
};

pub(crate) struct PageOpenPlugin;

impl Plugin for PageOpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                handle_page_open_requests.in_set(PageOpenSet::ResolveTarget),
                attach_cef_page_requests.in_set(PageOpenSet::Fallback),
                handle_unclaimed_page_open_tasks.in_set(PageOpenSet::Fallback),
                respond_page_open_tasks.in_set(PageOpenSet::Respond),
            ),
        );
    }
}

pub(crate) fn handle_page_open_requests(
    mut reader: MessageReader<PageOpenRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    stack_filter: Query<Entity, With<Stack>>,
    service: Option<Res<vmux_service::client::ServiceClient>>,
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
            url: normalize_vmux_url(&request.url),
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

pub(crate) fn attach_cef_page_requests(
    mut reader: MessageReader<CefPageAttachRequest>,
    children_q: Query<&Children>,
    mut commands: Commands,
) {
    for request in reader.read() {
        attach_cef_page_to_stack(
            request.stack,
            &request.url,
            &request.title,
            request.bg_color.clone(),
            &children_q,
            &mut commands,
        );
    }
}

pub(crate) fn handle_unclaimed_page_open_tasks(
    mut tasks: Query<
        (
            Entity,
            &PageOpenTask,
            Option<&PageOpenError>,
            Option<&PageOpenFallbackDeferred>,
        ),
        Without<PageOpenHandled>,
    >,
    children_q: Query<&Children>,
    mut commands: Commands,
) {
    for (entity, task, error, deferred_once) in &mut tasks {
        if let Some(error) = error {
            attach_error_page_to_stack(
                task.stack,
                ErrorPageData::failed_to_load(&task.url, &error.message),
                &children_q,
                &mut commands,
            );
            commands.entity(entity).insert(PageOpenHandled);
        } else if task.url.starts_with("vmux://error/") {
            attach_error_page_to_stack(
                task.stack,
                ErrorPageData::failed_to_load(&task.url, &task.url),
                &children_q,
                &mut commands,
            );
            commands.entity(entity).insert(PageOpenHandled);
        } else if task.url.starts_with("vmux://") {
            if deferred_once.is_none() {
                commands.entity(entity).insert(PageOpenFallbackDeferred);
                continue;
            }
            attach_error_page_to_stack(
                task.stack,
                ErrorPageData::not_found(&task.url),
                &children_q,
                &mut commands,
            );
            commands.entity(entity).insert((
                PageOpenHandled,
                PageOpenError {
                    message: format!("unknown vmux URL '{}'", task.url),
                },
            ));
        } else {
            attach_cef_page_to_stack(
                task.stack,
                &task.url,
                &task.url,
                None,
                &children_q,
                &mut commands,
            );
            commands.entity(entity).insert(PageOpenHandled);
        }
    }
}

pub(crate) fn respond_page_open_tasks(
    tasks: Query<
        (
            Entity,
            &PageOpenTask,
            Option<&PageOpenError>,
            Option<&PageOpenAwaitSnapshot>,
        ),
        With<PageOpenHandled>,
    >,
    service: Option<Res<vmux_service::client::ServiceClient>>,
    time: Res<Time>,
    children: Query<&Children>,
    browsers: Query<(), With<Browser>>,
    child_of: Query<&ChildOf>,
    mut pending_nav: ResMut<PendingNavSnapshots>,
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
            pending_nav.0.insert(
                webview,
                NavPending {
                    request_id,
                    started: await_snapshot.started,
                    saw_loading: false,
                    pane,
                },
            );
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
