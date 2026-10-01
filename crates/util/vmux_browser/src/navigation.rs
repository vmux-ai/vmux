use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_cef::prelude::*;
use vmux_api::VmuxRoute;
#[cfg(test)]
use vmux_command::CommandDefinition;
use vmux_command::{
    BindCommands, CommandDispatch, CommandRegistry, CommandRuntimePlugin, ReadCommandRequests,
};
#[cfg(test)]
use vmux_core::host::manifest::FeaturePlugin;
use vmux_core::page::{HostHistory, HostHistoryDelta, HostHistoryStep};
use vmux_core::{PageMetadata, PageOpenRequest, PageOpenTarget};
use vmux_history::{CreatedAt, LastActivatedAt, Visit};
use vmux_layout::Browser;
use vmux_layout::{
    Header,
    pane::{Pane, PaneSplit},
    side_sheet::SideSheet,
    stack::Stack,
};

use crate::host::{PendingNavigationUpdate, page_open_response};
use crate::input::RecentBrowserInteraction;
use crate::snapshot::BrowserTarget;

pub(crate) struct NavigationPlugin;

impl Plugin for NavigationPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(test)]
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        app.add_message::<vmux_core::service::ServiceRequest>();
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_message::<OpenHistoryRequest>()
            .configure_sets(
                Update,
                crate::BrowserSystemSet::Navigate
                    .after(vmux_core::service::ServiceMessageSet)
                    .before(vmux_core::PageOpenSet::ResolveTarget),
            )
            .add_systems(Startup, bind_command.in_set(BindCommands))
            .add_observer(issue_open_history)
            .add_systems(
                Update,
                (
                    drain_committed,
                    navigate.in_set(crate::BrowserSystemSet::Navigate),
                    go_back,
                    go_forward,
                    open_in_new_stack,
                    open_history.in_set(ReadCommandRequests),
                ),
            )
            .add_systems(
                Update,
                (sync_tab_metadata, record_visits)
                    .chain()
                    .after(vmux_layout::LayoutCefStateSet::Apply),
            );
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenHistoryRequest;

#[vmux_command::command(id = "browser_open_history")]
struct OpenHistoryBinding;

fn bind_command(registry: CommandRegistry, mut commands: Commands) {
    registry.bind::<OpenHistoryBinding>(&mut commands);
}

fn issue_open_history(
    trigger: On<CommandDispatch>,
    registered: Query<(), With<OpenHistoryBinding>>,
    mut requests: MessageWriter<OpenHistoryRequest>,
) {
    if registered.contains(trigger.event().command()) {
        requests.write(OpenHistoryRequest);
    }
}

fn drain_committed(
    receiver: Res<WebviewCommittedNavigationReceiver>,
    infrastructure: Query<(), With<crate::InfrastructureWebview>>,
    retired_infrastructure: Query<&crate::RetiredInfrastructureWebview>,
    mut writer: MessageWriter<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>,
) {
    while let Ok(ev) = receiver.0.try_recv() {
        if infrastructure.contains(ev.webview)
            || retired_infrastructure
                .iter()
                .any(|retired| retired.contains(ev.webview))
        {
            continue;
        }
        writer.write(ev);
    }
}

fn record_visits(
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

fn sync_tab_metadata(
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
        let content_is_agent = VmuxRoute::parse(&meta.url).is_some_and(|route| route.is_agent());
        if parent_meta.as_ref().is_some_and(|metadata| {
            VmuxRoute::parse(&metadata.url).is_some_and(|route| route.is_agent())
        }) && !content_is_web
            && !content_is_agent
        {
            continue;
        }
        if let Some(parent_url) = parent_meta.as_ref().map(|m| m.url.as_str())
            && VmuxRoute::parse(parent_url).is_some()
            && (meta.url.starts_with("data:") || meta.url.is_empty())
        {
            continue;
        }
        if let Ok(mut ecmds) = commands.get_entity(parent) {
            ecmds.insert(meta.clone());
            match identity {
                Some(identity) => ecmds.insert(identity.clone()),
                None => ecmds.remove::<vmux_core::PageIdentity>(),
            };
        }
    }
}

fn go_back(
    mut reader: MessageReader<vmux_layout::BrowserGoBackRequest>,
    targets: BrowserTarget,
    host_histories: Query<(), With<HostHistory>>,
    mut host_history_steps: MessageWriter<HostHistoryStep>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let Some(webview) = targets.resolve_pane(request.pane.as_deref()) else {
            continue;
        };
        if host_histories.contains(webview) {
            host_history_steps.write(HostHistoryStep {
                webview,
                delta: HostHistoryDelta::Back,
            });
            continue;
        }
        commands.trigger(bevy_cef::prelude::RequestGoBack { webview });
    }
}

fn go_forward(
    mut reader: MessageReader<vmux_layout::BrowserGoForwardRequest>,
    targets: BrowserTarget,
    host_histories: Query<(), With<HostHistory>>,
    mut host_history_steps: MessageWriter<HostHistoryStep>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let Some(webview) = targets.resolve_pane(request.pane.as_deref()) else {
            continue;
        };
        if host_histories.contains(webview) {
            host_history_steps.write(HostHistoryStep {
                webview,
                delta: HostHistoryDelta::Forward,
            });
            continue;
        }
        commands.trigger(bevy_cef::prelude::RequestGoForward { webview });
    }
}

fn open_history(
    mut reader: MessageReader<OpenHistoryRequest>,
    focus: vmux_layout::stack::FocusedStack,
    mut writer: MessageWriter<PageOpenRequest>,
) {
    for _ in reader.read() {
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

fn open_in_new_stack(
    mut reader: MessageReader<vmux_layout::OpenInNewStackRequest>,
    focus: vmux_layout::stack::FocusedStack,
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

fn navigate(
    mut reader: MessageReader<vmux_layout::BrowserNavigateRequest>,
    focus: vmux_layout::stack::FocusedStack,
    targets: vmux_layout::target::BrowserTargets<Browser>,
    mut commands: Commands,
    mut page_open_writer: MessageWriter<PageOpenRequest>,
    mut pending_navigation: MessageWriter<PendingNavigationUpdate>,
    time: Res<Time>,
    stack_metadata: Query<&PageMetadata, With<Stack>>,
    recent_interactions: Query<&RecentBrowserInteraction>,
    mut activate: MessageWriter<vmux_layout::active_pane::ActivatePane>,
    mut service_requests: MessageWriter<vmux_core::service::ServiceRequest>,
) {
    for request in reader.read() {
        let vmux_layout::BrowserNavigateRequest {
            url,
            pane,
            request_id,
            new_stack,
            profile,
        } = request.clone();
        let is_vmux_route = VmuxRoute::parse(&url).is_some();

        if let Some(s) = pane.as_deref() {
            if let Some(target) = targets.pane(s) {
                let active_stack = targets.active_stack(target);
                let replace_start = !new_stack
                    && active_stack.is_some_and(|stack| {
                        stack_metadata.get(stack).is_ok_and(|metadata| {
                            VmuxRoute::parse(&metadata.url).is_some_and(|route| {
                                VmuxRoute::parse(vmux_start::StartPlugin::URL)
                                    .is_some_and(|start| route.same_page(&start))
                            })
                        })
                    });
                if new_stack && !is_vmux_route && !url.starts_with("file:") {
                    let activate_new = active_stack.is_none_or(|stack| {
                        let Ok(interaction) = recent_interactions.get(stack) else {
                            return true;
                        };
                        !interaction.active()
                    });
                    let stack = commands
                        .spawn((
                            Stack::bundle(),
                            if activate_new {
                                LastActivatedAt::now()
                            } else {
                                LastActivatedAt(0)
                            },
                            ChildOf(target),
                        ))
                        .id();
                    if let Some(profile) = profile {
                        activate.write(vmux_layout::active_pane::ActivatePane {
                            profile: vmux_layout::active_pane::ProfileId::Agent(profile),
                            active: vmux_layout::active_pane::ActiveStack {
                                tab: None,
                                pane: Some(target),
                                stack: Some(stack),
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
                let in_place = if replace_start || is_vmux_route || url.starts_with("file:") {
                    None
                } else {
                    targets.active_webview(active_stack)
                };
                if let Some(webview) = in_place {
                    commands.trigger(RequestNavigate {
                        webview,
                        url: url.clone(),
                    });
                    let update = match request_id {
                        Some(request_id) => PendingNavigationUpdate::set(
                            webview,
                            request_id,
                            time.elapsed(),
                            Some(target.to_bits().to_string()),
                        ),
                        None => PendingNavigationUpdate::clear(webview),
                    };
                    pending_navigation.write(update);
                } else {
                    let target = active_stack
                        .filter(|_| replace_start)
                        .map(PageOpenTarget::Stack)
                        .unwrap_or(PageOpenTarget::NewStackInPane(target));
                    page_open_writer.write(PageOpenRequest {
                        target,
                        url,
                        request_id,
                    });
                }
            } else {
                if let Some(response) = page_open_response(
                    request_id,
                    Err(format!("browser_navigate: invalid pane id '{s}'")),
                ) {
                    service_requests.write(response);
                }
            }
        } else if let Some(stack) = focus.stack.filter(|stack| {
            !new_stack
                && stack_metadata.get(*stack).is_ok_and(|metadata| {
                    VmuxRoute::parse(&metadata.url).is_some_and(|route| {
                        VmuxRoute::parse(vmux_start::StartPlugin::URL)
                            .is_some_and(|start| route.same_page(&start))
                    })
                })
        }) {
            page_open_writer.write(PageOpenRequest {
                target: PageOpenTarget::Stack(stack),
                url,
                request_id,
            });
        } else if let Some(webview) = targets.active_webview(focus.stack) {
            if is_vmux_route || url.starts_with("file:") {
                let Some(pane) = focus.pane.filter(|pane| targets.contains_pane(*pane)) else {
                    if let Some(response) = page_open_response(
                        request_id,
                        Err("browser_navigate: no focused pane for vmux URL".to_string()),
                    ) {
                        service_requests.write(response);
                    }
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
                let update = match request_id {
                    Some(request_id) => PendingNavigationUpdate::set(
                        webview,
                        request_id,
                        time.elapsed(),
                        focus.pane.map(|pane| pane.to_bits().to_string()),
                    ),
                    None => PendingNavigationUpdate::clear(webview),
                };
                pending_navigation.write(update);
            }
        } else if let Some(pane) = focus.pane.filter(|pane| targets.contains_pane(*pane)) {
            page_open_writer.write(PageOpenRequest {
                target: PageOpenTarget::NewStackInPane(pane),
                url,
                request_id,
            });
        } else {
            if let Some(response) = page_open_response(
                request_id,
                Err("browser_navigate: no focused pane".to_string()),
            ) {
                service_requests.write(response);
            }
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
        let infrastructure = app.world_mut().spawn((crate::InfrastructureWebview,)).id();
        let visible = app.world_mut().spawn_empty().id();
        let (sender, receiver) = async_channel::unbounded();
        app.insert_resource(WebviewCommittedNavigationReceiver(receiver))
            .init_resource::<Collected>()
            .add_message::<WebviewCommittedNavigationEvent>()
            .add_systems(Update, (drain_committed, collect).chain());
        app.world_mut().despawn(infrastructure);
        app.world_mut()
            .spawn(crate::RetiredInfrastructureWebview::new(infrastructure));
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

#[cfg(test)]
mod command_definition_tests {
    use super::*;
    use vmux_command::CommandInvocation;

    #[test]
    fn history_mcp_definition_dispatches_to_the_typed_request() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(FeaturePlugin::<crate::Feature>::default())
            .add_plugins(CommandRuntimePlugin)
            .add_message::<OpenHistoryRequest>()
            .add_systems(Startup, bind_command.in_set(BindCommands))
            .add_observer(issue_open_history);
        app.update();

        let mut query = app.world_mut().query::<&CommandDefinition>();
        let tools = query
            .iter(app.world())
            .filter_map(CommandDefinition::agent_tool)
            .collect::<Vec<_>>();
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            ["browser_open_history"],
        );

        app.world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .write(CommandInvocation::new(
                Entity::PLACEHOLDER,
                "browser_open_history",
            ));
        app.update();
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<OpenHistoryRequest>>()
                .drain()
                .count(),
            1,
        );
    }
}
