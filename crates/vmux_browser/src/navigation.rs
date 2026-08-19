//! Navigating a browser pane, and what follows once a navigation commits.
//!
//! The request handlers are the whole of the entry side: a URL, a back or forward step, or a
//! history entry. Everything after `drain_committed_navigation` is the consequence — the tab
//! takes the page's title, and the visit is recorded.

use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_cef::prelude::*;
use vmux_command::{AppCommand, BrowserBarCommand, BrowserCommand, ReadAppCommands};
use vmux_core::{PageMetadata, PageOpenRequest, PageOpenTarget};
use vmux_history::{CreatedAt, LastActivatedAt, Visit};
use vmux_layout::Browser;
use vmux_layout::{
    Header,
    pane::{Pane, PaneSplit},
    side_sheet::SideSheet,
    stack::Stack,
};

use vmux_terminal::{self as terminal, Terminal};

use crate::input::RecentBrowserInteraction;
use crate::{NavPending, PendingNavSnapshots, send_page_open_response};

pub(crate) struct NavigationPlugin;

impl Plugin for NavigationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                drain_committed_navigation,
                handle_browser_navigate_requests.after(vmux_terminal::ServiceMessageSet),
                handle_browser_go_back_requests,
                handle_browser_go_forward_requests,
                handle_open_in_new_stack_requests,
                handle_browser_open_history.in_set(ReadAppCommands),
            ),
        )
        .add_systems(
            Update,
            (sync_page_metadata_to_tab, spawn_visit_on_navigation)
                .chain()
                .after(vmux_layout::apply_cef_state_from_webview),
        );
    }
}

fn drain_committed_navigation(
    receiver: Res<WebviewCommittedNavigationReceiver>,
    infrastructure: Res<crate::extensions::bridge_page::ExtensionInfrastructureEntities>,
    mut writer: MessageWriter<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>,
) {
    while let Ok(ev) = receiver.0.try_recv() {
        if infrastructure.contains(ev.webview) {
            continue;
        }
        writer.write(ev);
    }
}

fn spawn_visit_on_navigation(
    changed_tabs: Query<(Entity, &PageMetadata), (With<Stack>, Changed<PageMetadata>)>,
    mut last_urls: Local<std::collections::HashMap<u64, String>>,
    mut commands: Commands,
) {
    for (entity, meta) in &changed_tabs {
        if meta.url.is_empty() || meta.url == "about:blank" {
            continue;
        }

        let key = entity.to_bits();
        let is_new = last_urls
            .get(&key)
            .map(|prev| prev != &meta.url)
            .unwrap_or(true);

        if is_new {
            last_urls.insert(key, meta.url.clone());
            commands.spawn((Visit, meta.clone(), CreatedAt::now()));
        }
    }
}

pub(crate) fn sync_page_metadata_to_tab(
    browser_q: Query<
        (&PageMetadata, Option<&vmux_core::PageIdentity>, &ChildOf),
        (
            With<Browser>,
            Or<(Changed<PageMetadata>, Changed<vmux_core::PageIdentity>)>,
        ),
    >,
    tab_q: Query<Option<&PageMetadata>, With<Stack>>,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    mut commands: Commands,
) {
    for (meta, identity, child_of) in &browser_q {
        let parent = child_of.get();
        let Ok(parent_meta) = tab_q.get(parent) else {
            continue;
        };
        if status_q.contains(parent) || side_sheet_q.contains(parent) {
            continue;
        }
        let content_is_web = meta.url.starts_with("http://") || meta.url.starts_with("https://");
        let content_is_agent = meta.url.starts_with("vmux://agent/");
        if parent_meta
            .as_ref()
            .is_some_and(|m| m.url.starts_with("vmux://agent/"))
            && !content_is_web
            && !content_is_agent
        {
            continue;
        }
        if let Some(parent_url) = parent_meta.as_ref().map(|m| m.url.as_str())
            && parent_url.starts_with("vmux://")
            && (meta.url.starts_with("data:") || meta.url.is_empty())
        {
            continue;
        }
        if let Ok(mut ecmds) = commands.get_entity(parent) {
            ecmds.insert(meta.clone());
            // The tab renders from its own copy, so the reported name has to travel with the
            // metadata or the tab silently falls back to the host-given one.
            match identity {
                Some(identity) => ecmds.insert(identity.clone()),
                None => ecmds.remove::<vmux_core::PageIdentity>(),
            };
        }
    }
}

fn handle_browser_go_back_requests(
    mut reader: MessageReader<vmux_layout::BrowserGoBackRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<terminal::ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    pane_children: Query<&Children, With<Pane>>,
    stacks: Query<Entity, With<Stack>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let target = match request.pane.as_deref() {
            Some(s) => vmux_layout::target::parse_browser_target(s, &panes, &stacks),
            None => focus
                .pane
                .filter(|p| panes.contains(*p))
                .map(vmux_layout::target::BrowserTarget::Pane),
        };
        let Some(target) = target else { continue };
        let Some(webview) = vmux_layout::target::webview_for_target(
            target,
            &pane_children,
            &stack_ts,
            &browsers,
            &terminals,
        ) else {
            continue;
        };
        commands.trigger(bevy_cef::prelude::RequestGoBack { webview });
    }
}

fn handle_browser_go_forward_requests(
    mut reader: MessageReader<vmux_layout::BrowserGoForwardRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<terminal::ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    pane_children: Query<&Children, With<Pane>>,
    stacks: Query<Entity, With<Stack>>,
    stack_ts: Query<(Entity, &LastActivatedAt), With<Stack>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let target = match request.pane.as_deref() {
            Some(s) => vmux_layout::target::parse_browser_target(s, &panes, &stacks),
            None => focus
                .pane
                .filter(|p| panes.contains(*p))
                .map(vmux_layout::target::BrowserTarget::Pane),
        };
        let Some(target) = target else { continue };
        let Some(webview) = vmux_layout::target::webview_for_target(
            target,
            &pane_children,
            &stack_ts,
            &browsers,
            &terminals,
        ) else {
            continue;
        };
        commands.trigger(bevy_cef::prelude::RequestGoForward { webview });
    }
}

fn handle_browser_open_history(
    mut reader: MessageReader<AppCommand>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    mut writer: MessageWriter<PageOpenRequest>,
) {
    for cmd in reader.read() {
        if matches!(
            cmd,
            AppCommand::Browser(BrowserCommand::Bar(BrowserBarCommand::OpenHistory))
        ) {
            let Some(pane) = focus.pane else {
                continue;
            };
            writer.write(PageOpenRequest {
                target: PageOpenTarget::NewStackInPane(pane),
                url: "vmux://history/".to_string(),
                request_id: None,
            });
        }
    }
}

fn handle_open_in_new_stack_requests(
    mut reader: MessageReader<vmux_layout::OpenInNewStackRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut page_open_writer: MessageWriter<PageOpenRequest>,
) {
    for request in reader.read() {
        let Some(pane) = focus.pane.filter(|p| panes.contains(*p)) else {
            continue;
        };
        page_open_writer.write(PageOpenRequest {
            target: PageOpenTarget::NewStackInPane(pane),
            url: request.url.clone(),
            request_id: None,
        });
    }
}

pub(crate) fn handle_browser_navigate_requests(
    mut reader: MessageReader<vmux_layout::BrowserNavigateRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    terminals: Query<(Entity, &ChildOf), (With<Terminal>, Without<terminal::ProcessExited>)>,
    browsers: Query<(Entity, &ChildOf), With<Browser>>,
    service: Option<Res<vmux_service::client::ServiceClient>>,
    mut commands: Commands,
    mut page_open_writer: MessageWriter<PageOpenRequest>,
    mut pending_nav: ResMut<PendingNavSnapshots>,
    time: Res<Time>,
    pane_children: Query<&Children, With<Pane>>,
    stack_ts: Query<(Entity, &vmux_core::LastActivatedAt), With<vmux_layout::stack::Stack>>,
    recent_interaction: Res<RecentBrowserInteraction>,
    mut activate: MessageWriter<vmux_layout::active_panes::ActivatePane>,
) {
    for request in reader.read() {
        let vmux_layout::BrowserNavigateRequest {
            url,
            pane,
            request_id,
            new_stack,
            profile,
        } = request.clone();

        if let Some(s) = pane.as_deref() {
            if let Some(target) = vmux_layout::target::parse_pane_target(s, &panes) {
                if new_stack && !url.starts_with("vmux://") && !url.starts_with("file:") {
                    let active_stack =
                        vmux_layout::stack::active_stack_in_pane(target, &pane_children, &stack_ts);
                    let activate_new =
                        active_stack.is_none_or(|stack| !recent_interaction.active(stack));
                    let stack = commands
                        .spawn((
                            vmux_layout::stack::stack_bundle(),
                            if activate_new {
                                LastActivatedAt::now()
                            } else {
                                LastActivatedAt(0)
                            },
                            ChildOf(target),
                        ))
                        .id();
                    if let Some(profile) = profile {
                        activate.write(vmux_layout::active_panes::ActivatePane {
                            profile: vmux_layout::active_panes::ProfileId::Agent(profile),
                            active: vmux_layout::active_panes::ActiveStack {
                                tab: None,
                                pane: Some(target),
                                stack: Some(stack),
                                kind: None,
                            },
                        });
                    }
                    page_open_writer.write(PageOpenRequest {
                        target: PageOpenTarget::Stack(stack),
                        url,
                        request_id,
                    });
                    continue;
                }
                let in_place = if url.starts_with("vmux://") || url.starts_with("file:") {
                    None
                } else {
                    vmux_layout::target::active_webview_for_tab(
                        vmux_layout::stack::active_stack_in_pane(target, &pane_children, &stack_ts),
                        &browsers,
                        &terminals,
                    )
                };
                if let Some(webview) = in_place {
                    commands.trigger(RequestNavigate {
                        webview,
                        url: url.clone(),
                    });
                    let displaced = match request_id {
                        Some(rid) => pending_nav.0.insert(
                            webview,
                            NavPending {
                                request_id: rid,
                                started: time.elapsed(),
                                saw_loading: false,
                                pane: Some(target.to_bits().to_string()),
                            },
                        ),
                        None => pending_nav.0.remove(&webview),
                    };
                    if let Some(old) = displaced {
                        send_page_open_response(&service, Some(old.request_id), Ok(()));
                    }
                    if request_id.is_none() {
                        send_page_open_response(&service, None, Ok(()));
                    }
                } else {
                    page_open_writer.write(PageOpenRequest {
                        target: PageOpenTarget::NewStackInPane(target),
                        url,
                        request_id,
                    });
                }
            } else {
                send_page_open_response(
                    &service,
                    request_id,
                    Err(format!("browser_navigate: invalid pane id '{s}'")),
                );
            }
        } else if let Some(webview) =
            vmux_layout::target::active_webview_for_tab(focus.stack, &browsers, &terminals)
        {
            if url.starts_with("vmux://") || url.starts_with("file:") {
                let Some(pane) = focus.pane.filter(|p| panes.contains(*p)) else {
                    send_page_open_response(
                        &service,
                        request_id,
                        Err("browser_navigate: no focused pane for vmux URL".to_string()),
                    );
                    continue;
                };
                page_open_writer.write(PageOpenRequest {
                    target: PageOpenTarget::NewStackInPane(pane),
                    url,
                    request_id,
                });
            } else {
                commands.trigger(RequestNavigate {
                    webview,
                    url: url.clone(),
                });
                let displaced = match request_id {
                    Some(rid) => pending_nav.0.insert(
                        webview,
                        NavPending {
                            request_id: rid,
                            started: time.elapsed(),
                            saw_loading: false,
                            pane: focus.pane.map(|p| p.to_bits().to_string()),
                        },
                    ),
                    None => pending_nav.0.remove(&webview),
                };
                if let Some(old) = displaced {
                    send_page_open_response(&service, Some(old.request_id), Ok(()));
                }
                if request_id.is_none() {
                    send_page_open_response(&service, None, Ok(()));
                }
            }
        } else if let Some(pane) = focus.pane.filter(|p| panes.contains(*p)) {
            page_open_writer.write(PageOpenRequest {
                target: PageOpenTarget::NewStackInPane(pane),
                url,
                request_id,
            });
        } else {
            send_page_open_response(
                &service,
                request_id,
                Err("browser_navigate: no focused pane".to_string()),
            );
        }
    }
}

#[cfg(test)]
mod committed_navigation_tests {
    use super::*;
    use bevy_cef::prelude::WebviewCommittedNavigationReceiver;
    use bevy_cef_core::prelude::{
        CefTransitionCore, CefTransitionQualifiers, WebviewCommittedNavigationEvent,
    };

    #[derive(Resource, Default)]
    struct Collected(Vec<Entity>);

    fn collect(
        mut events: MessageReader<WebviewCommittedNavigationEvent>,
        mut collected: ResMut<Collected>,
    ) {
        collected.0.extend(events.read().map(|event| event.webview));
    }

    #[test]
    fn infrastructure_navigation_is_not_forwarded() {
        let mut app = App::new();
        let infrastructure = app
            .world_mut()
            .spawn(crate::extensions::bridge_page::ExtensionBridgeWebview {
                extension_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                role: crate::extensions::bridge_page::ExtensionBridgeRole::Transport,
            })
            .id();
        let visible = app.world_mut().spawn_empty().id();
        let (sender, receiver) = async_channel::unbounded();
        app.insert_resource(WebviewCommittedNavigationReceiver(receiver))
            .init_resource::<crate::extensions::bridge_page::ExtensionInfrastructureEntities>()
            .init_resource::<Collected>()
            .add_message::<WebviewCommittedNavigationEvent>()
            .add_systems(Update, (drain_committed_navigation, collect).chain());
        app.world_mut()
            .resource_mut::<crate::extensions::bridge_page::ExtensionInfrastructureEntities>()
            .insert(infrastructure);
        app.world_mut().despawn(infrastructure);
        for webview in [infrastructure, visible] {
            sender
                .send_blocking(WebviewCommittedNavigationEvent {
                    webview,
                    url: "https://example.com".into(),
                    is_main_frame: true,
                    transition: CefTransitionCore::Link,
                    qualifiers: CefTransitionQualifiers::default(),
                })
                .unwrap();
        }

        app.update();

        assert_eq!(app.world().resource::<Collected>().0, [visible]);
    }
}
