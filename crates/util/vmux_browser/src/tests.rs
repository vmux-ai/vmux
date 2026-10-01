use super::*;
use crate::host::*;
use vmux_ecs::PageMetadata;
use vmux_ecs::overlay::WindowOverlay;
use vmux_flex::prelude::{ComputedNode, Node, UiRect, Val};
use vmux_layout::stack::Stack;
use vmux_setting::AppSettings;

use vmux_ecs::PageIdentity;

#[test]
fn pending_navigation_updates_keep_only_the_latest_request() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, crate::page::PagePlugin));
    let webview = app.world_mut().spawn_empty().id();
    app.world_mut()
        .resource_mut::<Messages<PendingNavigationUpdate>>()
        .write(PendingNavigationUpdate::set(
            webview,
            [1; 16],
            std::time::Duration::ZERO,
            None,
        ));
    app.world_mut()
        .resource_mut::<Messages<PendingNavigationUpdate>>()
        .write(PendingNavigationUpdate::set(
            webview,
            [2; 16],
            std::time::Duration::ZERO,
            None,
        ));

    app.update();

    let world = app.world_mut();
    let mut pending = world.query::<&PendingNavigationSnapshot>();
    let pending = pending.iter(world).collect::<Vec<_>>();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].request_id, [2; 16]);
}

#[test]
fn cef_disables_bfcache_for_extension_ports() {
    assert!(
        CefStartup::command_line()
            .switch_values
            .contains(&("disable-features", "BackForwardCache"))
    );
}

#[test]
fn reported_title_wins_unless_it_is_absent_or_blank() {
    let meta = PageMetadata {
        title: "host".to_string(),
        ..Default::default()
    };
    assert_eq!(crate::state::PagePresentation::title(&meta, None), "host");
    assert_eq!(
        crate::state::PagePresentation::title(&meta, Some(&PageIdentity::from("reported"))),
        "reported"
    );
    assert_eq!(
        crate::state::PagePresentation::title(&meta, Some(&PageIdentity::from(""))),
        "host",
        "a page that blanks its own title has nothing to say, so the host name stands"
    );
    assert_eq!(
        crate::state::PagePresentation::title(&meta, Some(&PageIdentity::default())),
        "host",
        "an identity reporting only an icon must not blank the title"
    );
}

#[test]
fn agent_cli_url_redirects_tab_to_session_id() {
    let mut app = App::new();
    let (_, receiver) = async_channel::unbounded();
    app.add_plugins((
        MinimalPlugins,
        vmux_layout::LayoutContractPlugin,
        crate::page::PagePlugin,
        crate::navigation::NavigationPlugin,
    ))
    .insert_resource(bevy_cef::prelude::WebviewCommittedNavigationReceiver(
        receiver,
    ));

    let stack = app
        .world_mut()
        .spawn((
            Stack::default(),
            PageMetadata {
                url: "vmux://sessions/vibe/".to_string(),
                ..default()
            },
        ))
        .id();
    let child = app
        .world_mut()
        .spawn((
            Browser,
            PageMetadata {
                url: "vmux://sessions/vibe/".to_string(),
                ..default()
            },
            ChildOf(stack),
        ))
        .id();

    app.update();

    app.world_mut().get_mut::<PageMetadata>(child).unwrap().url =
        "vmux://sessions/vibe/abc-123".to_string();

    app.update();

    let stack_url = app.world().get::<PageMetadata>(stack).unwrap().url.clone();
    assert_eq!(stack_url, "vmux://sessions/vibe/abc-123");
}

#[test]
fn a_pointer_hits_only_interactive_regions() {
    let rect = ComputedNode::from_origin(Vec2::new(100.0, 40.0));

    assert!(
        CefPointerHitRect {
            rect,
            interactive: true
        }
        .contains(Vec2::new(100.0, 40.0))
    );
    assert!(
        !CefPointerHitRect {
            rect,
            interactive: true
        }
        .contains(Vec2::new(100.1, 20.0))
    );
    assert!(
        !CefPointerHitRect {
            rect,
            interactive: false
        }
        .contains(Vec2::new(50.0, 20.0))
    );
}

#[test]
fn layout_fixed_offsets_use_computed_header_rect() {
    let computed = ComputedNode {
        size: Vec2::new(1_544.0, 168.0),
        center: Vec2::new(788.0, 84.0),
        inverse_scale_factor: 0.5,
        ..default()
    };
    let offsets = LayoutFixedOffsets::from_node(&computed, 1_600.0).expect("offsets");

    assert_eq!(offsets.left, 8.0);
    assert_eq!(offsets.top, 0.0);
    assert_eq!(offsets.right, 20.0);
    assert_eq!(offsets.height, 84.0);
}

pub(crate) fn test_app_settings_with_radius(radius: f32) -> AppSettings {
    AppSettings {
        browser: vmux_setting::BrowserSettings {
            startup_url: "about:blank".to_string(),
            ..Default::default()
        },
        layout: vmux_layout::settings::LayoutSettings {
            radius,
            window: vmux_layout::settings::WindowSettings { padding: 0.0 },
            pane: vmux_layout::settings::PaneSettings { gap: 0.0 },
            side_sheet: vmux_layout::settings::SideSheetSettings::default(),
            focus_ring: vmux_layout::settings::FocusRingSettings::default(),
        },
        shortcuts: vmux_setting::ShortcutSettings::default(),
        terminal: None,
        auto_update: false,
        update_channel: Default::default(),
        agent: vmux_setting::AgentSettings::default(),
        spaces: Default::default(),
        projects: Default::default(),
        recording: Default::default(),
        editor: Default::default(),
        appearance: Default::default(),
    }
}

#[test]
fn appearance_change_updates_cef_color_scheme() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(test_app_settings_with_radius(0.0))
        .init_resource::<CefColorScheme>()
        .add_message::<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>()
        .add_plugins(crate::appearance::AppearancePlugin);
    app.update();
    app.world_mut()
        .resource_mut::<AppSettings>()
        .appearance
        .mode = vmux_setting::ColorScheme::Light;
    app.update();
    assert_eq!(
        app.world().resource::<CefColorScheme>().0,
        CefColorMode::Light
    );
}

#[test]
fn every_cef_browser_is_windowed_with_no_overlay_markers() {
    let mut app = App::new();
    app.world_mut().insert_non_send(Browsers::default());
    BrowserPlugin::configure_backend(&mut app);
    let page = app
        .world_mut()
        .spawn((Browser, WebviewSource::new("https://example.com")))
        .id();
    let modal = app
        .world_mut()
        .spawn((
            Browser,
            WindowOverlay,
            WebviewSource::new("vmux://command-bar/"),
        ))
        .id();

    app.update();

    for entity in [page, modal] {
        assert!(app.world().get::<WebviewWindowed>(entity).is_some());
        assert!(app.world().get::<WebviewNativeOverlay>(entity).is_none());
    }
}

#[test]
fn layout_state_padding_reads_effective_window_node_padding() {
    let node = Node {
        padding: UiRect {
            top: Val::Px(10.0),
            right: Val::Px(11.0),
            bottom: Val::Px(12.0),
            left: Val::Px(13.0),
        },
        ..default()
    };

    assert_eq!(
        LayoutWindowPadding::from_node(&node),
        LayoutWindowPadding {
            top: 10.0,
            right: 11.0,
            bottom: 12.0,
            left: 13.0,
        }
    );
}

mod browser_navigate_flow {
    use crate::Browser;
    use crate::host::AgentBrowserNavigate;
    use crate::host::PendingNavigationSnapshot;
    use crate::input::RecentBrowserInteraction;
    use bevy::ecs::relationship::Relationship;
    use bevy::prelude::*;
    use vmux_api::protocol::{AgentRequest, AgentRequestId};
    use vmux_ecs::agent::{AgentRequestInput, CommandOrigin};
    use vmux_ecs::{
        LastActivatedAt, PageMetadata, PageOpenDeferred, PageOpenError, PageOpenHandled,
        PageOpenId, PageOpenSet, PageOpenTask,
    };
    use vmux_layout::active_pane::ActiveStack;
    use vmux_layout::pane::Pane;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{AppSettings, BrowserSettings, ShortcutSettings};
    use vmux_terminal::Terminal;

    use bevy_cef::prelude::RequestNavigate;

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
                ..Default::default()
            },
            layout: LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 0.0 },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            update_channel: Default::default(),
            agent: vmux_setting::AgentSettings::default(),
            spaces: Default::default(),
            projects: Default::default(),
            recording: Default::default(),
            editor: Default::default(),
            appearance: Default::default(),
        }
    }

    fn set_focus(app: &mut App, pane: Option<Entity>, stack: Option<Entity>) {
        let focus = ActiveStack {
            pane,
            stack,
            ..default()
        };
        let world = app.world_mut();
        let mut query = world.query::<&mut ActiveStack>();
        if let Ok(mut current) = query.single_mut(world) {
            *current = focus;
        } else {
            world.spawn(focus.local_bundle());
        }
    }

    struct ConsumerPlugin;

    impl Plugin for ConsumerPlugin {
        fn build(&self, app: &mut App) {
            app.add_plugins((
                vmux_layout::LayoutContractPlugin,
                vmux_terminal::TerminalRequestPlugin,
                crate::page::PagePlugin,
                crate::navigation::NavigationPlugin,
                crate::host::AgentBrowserPlugin,
            ))
            .insert_resource(bevy_cef::prelude::WebviewCommittedNavigationReceiver(
                async_channel::unbounded().1,
            ))
            .add_message::<vmux_space::SpaceAttachRequest>()
            .add_message::<vmux_space::SpaceCreateRequest>()
            .add_message::<vmux_space::SpaceDeleteRequest>()
            .add_message::<vmux_space::SpaceOpenPageRequest>()
            .add_message::<vmux_space::SpaceRenameRequest>()
            .add_message::<vmux_history::HistoryOpenIntent>()
            .add_message::<vmux_ecs::page::HostHistoryStep>()
            .add_message::<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>()
            .add_systems(
                Update,
                handle_test_known_page_open.in_set(PageOpenSet::HandleKnownPages),
            );
        }
    }

    type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

    fn handle_test_known_page_open(
        tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
        mut commands: Commands,
    ) {
        for (entity, task) in &tasks {
            if task.url.starts_with("vmux://terminal/") {
                commands.entity(task.stack).despawn_children();
                commands.spawn((Browser, Terminal, ChildOf(task.stack)));
                commands.entity(entity).insert(PageOpenHandled);
            } else if task.url.starts_with("vmux://sessions/") {
                commands.entity(task.stack).despawn_children();
                commands.entity(entity).insert(PageOpenHandled);
            }
        }
    }

    #[derive(Resource, Default)]
    struct CapturedNavigateUrls(Vec<String>);

    #[test]
    fn browser_navigate_triggers_request_navigate_with_url() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, ConsumerPlugin));
        app.insert_resource(test_settings())
            .init_resource::<CapturedNavigateUrls>();

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::Stack::bundle())
            .insert(ChildOf(pane))
            .id();
        app.world_mut().spawn(Browser).insert(ChildOf(stack));

        set_focus(&mut app, Some(pane), Some(stack));

        app.add_observer(
            |trigger: On<RequestNavigate>, mut captured: ResMut<CapturedNavigateUrls>| {
                captured.0.push(trigger.url.clone());
            },
        );

        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::User,
                request: AgentRequest::encode(&AgentBrowserNavigate {
                    url: "https://example.com".to_string(),
                    pane: None,
                })
                .unwrap(),
            });

        app.update();
        app.update();

        let captured = app.world().resource::<CapturedNavigateUrls>();
        assert_eq!(captured.0, vec!["https://example.com".to_string()]);
    }

    #[test]
    fn browser_navigate_auto_spawns_tab_when_pane_is_empty() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, ConsumerPlugin));
        app.insert_resource(test_settings());

        let pane = app.world_mut().spawn(Pane).id();

        set_focus(&mut app, Some(pane), None);

        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::User,
                request: AgentRequest::encode(&AgentBrowserNavigate {
                    url: "https://example.com".to_string(),
                    pane: None,
                })
                .unwrap(),
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let mut tabs = world.query_filtered::<&ChildOf, With<vmux_layout::stack::Stack>>();
        let tab_count_under_pane = tabs
            .iter(world)
            .filter(|child_of| child_of.get() == pane)
            .count();
        assert_eq!(
            tab_count_under_pane, 1,
            "browser_navigate should have spawned exactly one tab in the focused pane"
        );

        let mut tab_metadata =
            world.query_filtered::<&PageMetadata, With<vmux_layout::stack::Stack>>();
        let tab_urls: Vec<String> = tab_metadata.iter(world).map(|p| p.url.clone()).collect();
        assert!(
            tab_urls.contains(&"https://example.com".to_string()),
            "tab entity should have PageMetadata with the URL; found {tab_urls:?}"
        );

        let mut browsers = world.query::<(&Browser, &PageMetadata)>();
        let urls: Vec<String> = browsers.iter(world).map(|(_, p)| p.url.clone()).collect();
        assert!(
            urls.contains(&"https://example.com".to_string()),
            "browser entity with the URL should exist; found {urls:?}"
        );
    }

    #[test]
    fn agent_browser_navigate_stacks_new_page_and_waits_for_snapshot() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ConsumerPlugin));
        app.insert_resource(test_settings());

        let pane = app.world_mut().spawn(Pane).id();
        let first_stack = app
            .world_mut()
            .spawn((
                vmux_layout::stack::Stack::bundle(),
                LastActivatedAt(1),
                ChildOf(pane),
            ))
            .id();
        app.world_mut().spawn((Browser, ChildOf(first_stack)));
        let request_id = [7; 16];
        app.world_mut()
            .resource_mut::<Messages<vmux_layout::BrowserNavigateRequest>>()
            .write(vmux_layout::BrowserNavigateRequest {
                url: "https://second.example".into(),
                pane: Some(pane.to_bits().to_string()),
                request_id: Some(request_id),
                new_stack: true,
                profile: Some("agent-1".into()),
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let mut stacks = world.query_filtered::<
                (Entity, &PageMetadata, &LastActivatedAt),
                With<vmux_layout::stack::Stack>,
            >();
        let second = stacks
            .iter(world)
            .find(|(_, metadata, _)| metadata.url == "https://second.example")
            .map(|(entity, _, activated)| (entity, activated.0))
            .expect("new browser stack");
        assert_ne!(second.0, first_stack);
        assert!(second.1 > 1);
        let mut pending = world.query::<&PendingNavigationSnapshot>();
        let pending = pending.iter(world).collect::<Vec<_>>();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].request_id, request_id);
    }

    #[test]
    fn agent_browser_navigate_does_not_raise_new_stack_during_user_interaction() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ConsumerPlugin));
        app.insert_resource(test_settings());

        let pane = app.world_mut().spawn(Pane).id();
        let first_stack = app
            .world_mut()
            .spawn((
                vmux_layout::stack::Stack::bundle(),
                LastActivatedAt(10),
                ChildOf(pane),
            ))
            .id();
        app.world_mut().spawn((Browser, ChildOf(first_stack)));
        app.world_mut()
            .entity_mut(first_stack)
            .insert(RecentBrowserInteraction::now());
        app.world_mut()
            .resource_mut::<Messages<vmux_layout::BrowserNavigateRequest>>()
            .write(vmux_layout::BrowserNavigateRequest {
                url: "https://second.example".into(),
                pane: Some(pane.to_bits().to_string()),
                request_id: None,
                new_stack: true,
                profile: Some("agent-1".into()),
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let mut stacks = world
            .query_filtered::<(&PageMetadata, &LastActivatedAt), With<vmux_layout::stack::Stack>>();
        let activated = stacks
            .iter(world)
            .find(|(metadata, _)| metadata.url == "https://second.example")
            .map(|(_, activated)| activated.0)
            .expect("new browser stack");
        assert_eq!(activated, 0);
    }

    #[test]
    fn browser_navigate_targets_specific_pane_when_id_provided() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, ConsumerPlugin));
        app.insert_resource(test_settings());

        let pane_a = app.world_mut().spawn(Pane).id();
        let pane_b = app.world_mut().spawn(Pane).id();

        set_focus(&mut app, Some(pane_a), None);

        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::User,
                request: AgentRequest::encode(&AgentBrowserNavigate {
                    url: "https://example.com".to_string(),
                    pane: Some(pane_b.to_bits().to_string()),
                })
                .unwrap(),
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let mut tabs = world.query_filtered::<&ChildOf, With<vmux_layout::stack::Stack>>();
        let tabs_in_b = tabs
            .iter(world)
            .filter(|child_of| child_of.get() == pane_b)
            .count();
        let tabs_in_a = tabs
            .iter(world)
            .filter(|child_of| child_of.get() == pane_a)
            .count();
        assert_eq!(tabs_in_b, 1, "tab should be spawned in target pane B");
        assert_eq!(tabs_in_a, 0, "no tab should be spawned in focused pane A");
    }

    #[test]
    fn browser_navigate_with_terminal_url_spawns_terminal_in_focused_pane() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, ConsumerPlugin));
        app.insert_resource(test_settings());

        let pane = app.world_mut().spawn(Pane).id();
        set_focus(&mut app, Some(pane), None);
        let request_id = AgentRequestId::new();

        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id,
                origin: CommandOrigin::User,
                request: AgentRequest::encode(&AgentBrowserNavigate {
                    url: "vmux://terminal/".to_string(),
                    pane: None,
                })
                .unwrap(),
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let terminal_count = world.query::<&Terminal>().iter(world).count();
        assert!(
            terminal_count >= 1,
            "terminal should be spawned in focused pane"
        );
        let mut pending = world.query::<&PendingNavigationSnapshot>();
        assert!(
            pending
                .iter(world)
                .any(|pending| pending.request_id == request_id.0),
            "terminal navigation should wait for its snapshot"
        );
    }

    #[test]
    fn browser_navigate_replaces_the_start_page_stack() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ConsumerPlugin));
        app.insert_resource(test_settings());

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn((
                vmux_layout::stack::Stack::bundle(),
                LastActivatedAt(1),
                ChildOf(pane),
            ))
            .insert(PageMetadata {
                title: "Start".into(),
                url: "vmux://start/".into(),
                ..default()
            })
            .id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        set_focus(&mut app, Some(pane), Some(stack));
        app.world_mut()
            .resource_mut::<Messages<vmux_layout::BrowserNavigateRequest>>()
            .write(vmux_layout::BrowserNavigateRequest {
                url: "vmux://terminal/".into(),
                pane: None,
                request_id: None,
                new_stack: false,
                profile: None,
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let stacks = world
            .query_filtered::<Entity, With<vmux_layout::stack::Stack>>()
            .iter(world)
            .collect::<Vec<_>>();
        assert_eq!(stacks, vec![stack]);
        assert_eq!(
            world
                .query::<&ChildOf>()
                .iter(world)
                .filter(|child| child.get() == stack)
                .count(),
            1
        );
    }

    #[test]
    fn browser_navigate_replaces_the_start_page_stack_with_a_web_page() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ConsumerPlugin));
        app.insert_resource(test_settings())
            .init_resource::<CapturedNavigateUrls>()
            .add_observer(
                |trigger: On<bevy_cef::prelude::RequestNavigate>,
                 mut captured: ResMut<CapturedNavigateUrls>| {
                    captured.0.push(trigger.url.clone());
                },
            );

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn((
                vmux_layout::stack::Stack::bundle(),
                LastActivatedAt(1),
                ChildOf(pane),
            ))
            .insert(PageMetadata {
                title: "Start".into(),
                url: "vmux://start/".into(),
                ..default()
            })
            .id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        set_focus(&mut app, Some(pane), Some(stack));
        app.world_mut()
            .resource_mut::<Messages<vmux_layout::BrowserNavigateRequest>>()
            .write(vmux_layout::BrowserNavigateRequest {
                url: "https://example.com".into(),
                pane: None,
                request_id: None,
                new_stack: false,
                profile: None,
            });

        app.update();
        app.update();

        let world = app.world_mut();
        assert_eq!(
            world
                .query_filtered::<Entity, With<vmux_layout::stack::Stack>>()
                .iter(world)
                .count(),
            1
        );
        assert!(world.resource::<CapturedNavigateUrls>().0.is_empty());
    }

    #[test]
    fn browser_navigate_keeps_the_start_page_for_an_explicit_new_stack() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ConsumerPlugin));
        app.insert_resource(test_settings());

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn((
                vmux_layout::stack::Stack::bundle(),
                LastActivatedAt(1),
                ChildOf(pane),
            ))
            .insert(PageMetadata {
                title: "Start".into(),
                url: "vmux://start/".into(),
                ..default()
            })
            .id();
        app.world_mut().spawn((Browser, ChildOf(stack)));
        set_focus(&mut app, Some(pane), Some(stack));
        app.world_mut()
            .resource_mut::<Messages<vmux_layout::BrowserNavigateRequest>>()
            .write(vmux_layout::BrowserNavigateRequest {
                url: "vmux://terminal/".into(),
                pane: None,
                request_id: None,
                new_stack: true,
                profile: None,
            });

        app.update();
        app.update();

        let world = app.world_mut();
        assert_eq!(
            world
                .query_filtered::<Entity, With<vmux_layout::stack::Stack>>()
                .iter(world)
                .count(),
            2
        );
    }

    #[test]
    fn browser_navigate_with_terminal_url_and_target_pane_uses_target() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, ConsumerPlugin));
        app.insert_resource(test_settings());

        let pane_a = app.world_mut().spawn(Pane).id();
        let pane_b = app.world_mut().spawn(Pane).id();
        set_focus(&mut app, Some(pane_a), None);

        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::User,
                request: AgentRequest::encode(&AgentBrowserNavigate {
                    url: "vmux://terminal/".to_string(),
                    pane: Some(pane_b.to_bits().to_string()),
                })
                .unwrap(),
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let mut terminals = world.query_filtered::<&ChildOf, With<Terminal>>();
        let term_parents: Vec<Entity> = terminals.iter(world).map(|c| c.get()).collect();
        let mut found_in_b = 0;
        let mut found_in_a = 0;
        for tab in &term_parents {
            if let Some(co) = world.get::<ChildOf>(*tab) {
                if co.get() == pane_b {
                    found_in_b += 1;
                } else if co.get() == pane_a {
                    found_in_a += 1;
                }
            }
        }
        assert_eq!(found_in_b, 1, "terminal should be in target pane B");
        assert_eq!(found_in_a, 0, "no terminal in focused pane A");
    }

    #[test]
    fn browser_navigate_with_unknown_vmux_url_errors() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, ConsumerPlugin));
        app.insert_resource(test_settings());

        let pane = app.world_mut().spawn(Pane).id();
        set_focus(&mut app, Some(pane), None);

        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::User,
                request: AgentRequest::encode(&AgentBrowserNavigate {
                    url: "vmux://nonsense/".to_string(),
                    pane: None,
                })
                .unwrap(),
            });

        app.update();
        app.update();
        app.update();

        let world = app.world_mut();
        let mut browsers = world.query_filtered::<&PageMetadata, With<Browser>>();
        let browser_titles: Vec<String> = browsers
            .iter(world)
            .map(|meta| meta.title.clone())
            .collect();
        let terminal_count = world.query::<&Terminal>().iter(world).count();
        assert_eq!(
            browser_titles,
            vec!["Page not found".to_string()],
            "unknown vmux URL should render an error page"
        );
        assert_eq!(
            terminal_count, 0,
            "no terminal should be spawned for unknown vmux URL"
        );
    }

    #[test]
    fn deferred_page_open_is_not_claimed_by_fallback() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(crate::page::PagePlugin);
        let stack = app.world_mut().spawn_empty().id();
        let task = app
            .world_mut()
            .spawn((
                PageOpenTask {
                    id: PageOpenId::new(),
                    stack,
                    url: "vmux://sessions/claude".to_string(),
                    request_id: None,
                },
                PageOpenDeferred,
            ))
            .id();

        app.update();
        app.update();

        assert!(app.world().get::<PageOpenHandled>(task).is_none());
        assert!(app.world().get::<PageOpenError>(task).is_none());
    }

    #[test]
    fn page_open_error_renders_error_page() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, ConsumerPlugin));
        app.insert_resource(test_settings());

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn((
                vmux_layout::stack::Stack::bundle(),
                vmux_history::LastActivatedAt::now(),
                ChildOf(pane),
            ))
            .id();

        app.world_mut().spawn((
            PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://terminal/bad".to_string(),
                request_id: None,
            },
            PageOpenError {
                message: "malformed terminal URL".to_string(),
            },
        ));

        app.update();
        app.update();

        let world = app.world_mut();
        let mut browsers = world.query_filtered::<&PageMetadata, With<Browser>>();
        let browser_titles: Vec<String> = browsers
            .iter(world)
            .map(|meta| meta.title.clone())
            .collect();
        assert_eq!(
            browser_titles,
            vec!["Page failed to load".to_string()],
            "page handler errors should render an error page"
        );
    }

    #[test]
    fn browser_navigate_with_claude_url_does_not_spawn_standalone_browser() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, ConsumerPlugin));
        app.insert_resource(test_settings());

        let pane = app.world_mut().spawn(Pane).id();
        set_focus(&mut app, Some(pane), None);

        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::User,
                request: AgentRequest::encode(&AgentBrowserNavigate {
                    url: "vmux://sessions/claude/cli/".into(),
                    pane: None,
                })
                .unwrap(),
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let standalone_browser_count = world
            .query_filtered::<&Browser, Without<Terminal>>()
            .iter(world)
            .count();
        assert_eq!(
            standalone_browser_count, 0,
            "claude URL should never spawn a standalone browser tab"
        );
    }

    #[test]
    fn browser_navigate_with_codex_url_does_not_spawn_standalone_browser() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin, ConsumerPlugin));
        app.insert_resource(test_settings());

        let pane = app.world_mut().spawn(Pane).id();
        set_focus(&mut app, Some(pane), None);

        app.world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .write(AgentRequestInput {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::User,
                request: AgentRequest::encode(&AgentBrowserNavigate {
                    url: "vmux://sessions/codex/cli/".into(),
                    pane: None,
                })
                .unwrap(),
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let standalone_browser_count = world
            .query_filtered::<&Browser, Without<Terminal>>()
            .iter(world)
            .count();
        assert_eq!(
            standalone_browser_count, 0,
            "codex URL should never spawn a standalone browser tab"
        );
    }
}

mod open_in_place_flow {
    use crate::{OpenRequest, ZoomRequest};
    use bevy::ecs::message::Messages;
    use bevy::prelude::*;
    use bevy_cef::prelude::RequestNavigate;
    use vmux_ecs::{PageOpenRequest, PageOpenTarget};
    use vmux_history::LastActivatedAt;
    use vmux_layout::Browser;
    use vmux_layout::pane::Pane;
    use vmux_layout::stack::Stack;
    use vmux_layout::tab::Tab;
    use vmux_terminal::Terminal;

    #[derive(Resource, Default)]
    struct CapturedNavigateUrls(Vec<String>);

    #[derive(Resource, Default)]
    struct CapturedPageOpenRequests(Vec<PageOpenRequest>);

    fn build_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_ecs::EcsPlugin,
            vmux_command::CommandPlugin,
            vmux_terminal::TerminalContractPlugin,
            crate::command::CommandPlugin,
        ))
        .add_message::<PageOpenRequest>()
        .add_systems(
            Update,
            capture_page_open_requests.after(vmux_command::ReadCommandRequests),
        )
        .init_resource::<CapturedNavigateUrls>()
        .init_resource::<CapturedPageOpenRequests>()
        .add_observer(
            |trigger: On<RequestNavigate>, mut captured: ResMut<CapturedNavigateUrls>| {
                captured.0.push(trigger.url.clone());
            },
        );
        app.world_mut()
            .spawn(vmux_ecs::HostSpawnRoute::page("vmux://terminal/"));
        app.world_mut()
            .spawn(vmux_ecs::HostSpawnRoute::subtree("vmux://sessions/"));
        for (url, title) in [
            ("vmux://services/", "Services"),
            ("vmux://settings/", "Settings"),
            ("vmux://team/", "Team"),
            ("vmux://spaces/", "Spaces"),
        ] {
            app.world_mut()
                .spawn(vmux_ecs::host::page::NativelyHosted::subtree(url, title));
        }
        app.world_mut()
            .spawn(vmux_ecs::HostSpawnRoute::scheme("file"));
        app
    }

    fn capture_page_open_requests(
        mut reader: MessageReader<PageOpenRequest>,
        mut captured: ResMut<CapturedPageOpenRequests>,
    ) {
        captured.0.extend(reader.read().cloned());
    }

    fn build_focused_stack(app: &mut App) -> Entity {
        let space = app
            .world_mut()
            .spawn((
                vmux_layout::space::Space,
                vmux_layout::space::CurrentSpace,
                vmux_layout::space::SpaceId("test".to_string()),
                vmux_layout::profile::Profile::default(),
                vmux_ecs::Active,
            ))
            .id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(space)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn(Stack::bundle())
            .insert((ChildOf(pane), LastActivatedAt(1)))
            .id();
        app.world_mut().spawn(Browser).insert(ChildOf(stack));
        space
    }

    fn build_focused_terminal_stack(app: &mut App) -> Entity {
        let space = app
            .world_mut()
            .spawn((
                vmux_layout::space::Space,
                vmux_layout::space::CurrentSpace,
                vmux_layout::space::SpaceId("test".to_string()),
                vmux_layout::profile::Profile::default(),
                vmux_ecs::Active,
            ))
            .id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(space)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn(Stack::bundle())
            .insert((ChildOf(pane), LastActivatedAt(1)))
            .id();
        app.world_mut()
            .spawn((Browser, Terminal))
            .insert(ChildOf(stack));
        space
    }

    fn build_focused_native_stack(app: &mut App, native_url: &str) -> Entity {
        let space = app
            .world_mut()
            .spawn((
                vmux_layout::space::Space,
                vmux_layout::space::CurrentSpace,
                vmux_layout::space::SpaceId("test".to_string()),
                vmux_layout::profile::Profile::default(),
                vmux_ecs::Active,
            ))
            .id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1), ChildOf(space)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn(Stack::bundle())
            .insert((ChildOf(pane), LastActivatedAt(1)))
            .id();
        app.world_mut()
            .spawn((
                Browser,
                vmux_ecs::PageMetadata {
                    url: native_url.to_string(),
                    title: native_url.to_string(),
                    icon: vmux_ecs::PageIcon::None,
                    bg_color: None,
                },
            ))
            .insert(ChildOf(stack));
        space
    }

    #[test]
    fn in_place_with_explicit_url_triggers_request_navigate() {
        let mut app = build_app();
        build_focused_stack(&mut app);

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("https://example.com".into()),
            });

        app.update();

        let captured = app.world().resource::<CapturedNavigateUrls>();
        assert_eq!(captured.0, vec!["https://example.com".to_string()]);
    }

    #[test]
    fn in_place_with_vmux_url_routes_through_page_open() {
        let mut app = build_app();
        build_focused_stack(&mut app);

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("vmux://sessions/vibe".into()),
            });

        app.update();

        let navigates = app.world().resource::<CapturedNavigateUrls>();
        assert!(navigates.0.is_empty());
        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert_eq!(page_opens.0.len(), 1);
        assert_eq!(page_opens.0[0].url, "vmux://sessions/vibe");
        assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
    }

    #[test]
    fn in_place_from_plain_vmux_to_web_navigates_in_place() {
        let mut app = build_app();
        build_focused_native_stack(&mut app, "vmux://history/");

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("https://mistral.ai".into()),
            });

        app.update();

        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert!(page_opens.0.is_empty());
        let navigates = app.world().resource::<CapturedNavigateUrls>();
        assert_eq!(navigates.0, vec!["https://mistral.ai".to_string()]);
    }

    #[test]
    fn in_place_from_web_to_plain_vmux_navigates_in_place() {
        let mut app = build_app();
        build_focused_native_stack(&mut app, "https://example.com/");

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("vmux://history/".into()),
            });

        app.update();

        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert!(page_opens.0.is_empty());
        let navigates = app.world().resource::<CapturedNavigateUrls>();
        assert_eq!(navigates.0, vec!["vmux://history/".to_string()]);
    }

    #[test]
    fn in_place_to_settings_routes_through_page_open() {
        let mut app = build_app();
        build_focused_native_stack(&mut app, "https://example.com/");

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("vmux://settings/".into()),
            });

        app.update();

        let navigates = app.world().resource::<CapturedNavigateUrls>();
        assert!(navigates.0.is_empty());
        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert_eq!(page_opens.0.len(), 1);
        assert_eq!(page_opens.0[0].url, "vmux://settings/");
        assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
    }

    #[test]
    fn in_place_to_terminal_routes_through_page_open() {
        let mut app = build_app();
        build_focused_native_stack(&mut app, "vmux://settings/");

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("vmux://terminal/".into()),
            });

        app.update();

        let navigates = app.world().resource::<CapturedNavigateUrls>();
        assert!(navigates.0.is_empty());
        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert_eq!(page_opens.0.len(), 1);
        assert_eq!(page_opens.0[0].url, "vmux://terminal/");
        assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
    }

    #[test]
    fn in_place_to_file_routes_through_page_open() {
        let mut app = build_app();
        build_focused_native_stack(&mut app, "https://example.com/");

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("file:///tmp/x".into()),
            });

        app.update();

        let navigates = app.world().resource::<CapturedNavigateUrls>();
        assert!(navigates.0.is_empty());
        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert_eq!(page_opens.0.len(), 1);
        assert_eq!(page_opens.0[0].url, "file:///tmp/x");
        assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
    }

    #[test]
    fn in_place_from_terminal_to_web_routes_through_page_open() {
        let mut app = build_app();
        build_focused_terminal_stack(&mut app);

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest {
                url: Some("https://google.com".into()),
            });

        app.update();

        let navigates = app.world().resource::<CapturedNavigateUrls>();
        assert!(navigates.0.is_empty());
        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert_eq!(page_opens.0.len(), 1);
        assert_eq!(page_opens.0[0].url, "https://google.com");
        assert!(matches!(page_opens.0[0].target, PageOpenTarget::Stack(_)));
    }

    #[test]
    fn zoom_in_on_terminal_emits_font_size_increase() {
        let mut app = build_app();
        build_focused_terminal_stack(&mut app);

        app.world_mut()
            .resource_mut::<Messages<ZoomRequest>>()
            .write(ZoomRequest::In);

        app.update();

        let cmds: Vec<vmux_terminal::TerminalFontSizeCommand> = app
            .world_mut()
            .resource_mut::<Messages<vmux_terminal::TerminalFontSizeCommand>>()
            .drain()
            .collect();
        assert_eq!(cmds, vec![vmux_terminal::TerminalFontSizeCommand::Increase]);
    }

    #[test]
    fn zoom_reset_on_terminal_emits_font_size_reset() {
        let mut app = build_app();
        build_focused_terminal_stack(&mut app);

        app.world_mut()
            .resource_mut::<Messages<ZoomRequest>>()
            .write(ZoomRequest::Reset);

        app.update();

        let cmds: Vec<vmux_terminal::TerminalFontSizeCommand> = app
            .world_mut()
            .resource_mut::<Messages<vmux_terminal::TerminalFontSizeCommand>>()
            .drain()
            .collect();
        assert_eq!(cmds, vec![vmux_terminal::TerminalFontSizeCommand::Reset]);
    }

    #[test]
    fn in_place_with_none_url_uses_startup_setting() {
        let mut app = build_app();
        let space = build_focused_stack(&mut app);
        app.world_mut()
            .entity_mut(space)
            .insert(vmux_ecs::EffectiveStartupUrl(
                "https://startup.example".into(),
            ));

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest { url: None });

        app.update();

        let captured = app.world().resource::<CapturedNavigateUrls>();
        assert_eq!(captured.0, vec!["https://startup.example".to_string()]);
    }

    #[test]
    fn in_place_with_none_url_and_no_startup_does_not_navigate() {
        let mut app = build_app();
        build_focused_stack(&mut app);

        app.world_mut()
            .resource_mut::<Messages<OpenRequest>>()
            .write(OpenRequest { url: None });

        app.update();

        let captured = app.world().resource::<CapturedNavigateUrls>();
        assert!(captured.0.is_empty());
        let page_opens = app.world().resource::<CapturedPageOpenRequests>();
        assert!(page_opens.0.is_empty());
    }
}

mod update_notice_tests {
    use super::UpdateProjection;
    use vmux_layout::UpdateState;

    fn downloading(v: &str) -> UpdateState {
        UpdateState::Downloading {
            version: v.into(),
            downloaded: 1,
            total: 2,
        }
    }

    #[test]
    fn emits_on_change() {
        assert!(UpdateProjection::should_emit(
            &UpdateState::Ready {
                version: "v2".into()
            },
            &None,
            false
        ));
        assert!(UpdateProjection::should_emit(
            &UpdateState::Idle,
            &Some(downloading("v2")),
            false
        ));
    }

    #[test]
    fn no_emit_when_unchanged_and_no_page_ready() {
        assert!(!UpdateProjection::should_emit(
            &UpdateState::Idle,
            &Some(UpdateState::Idle),
            false
        ));
        let r = UpdateState::Ready {
            version: "v2".into(),
        };
        assert!(!UpdateProjection::should_emit(&r, &Some(r.clone()), false));
    }

    #[test]
    fn re_emits_non_idle_on_page_ready() {
        let r = UpdateState::Ready {
            version: "v2".into(),
        };
        assert!(UpdateProjection::should_emit(&r, &Some(r.clone()), true));
        assert!(!UpdateProjection::should_emit(
            &UpdateState::Idle,
            &Some(UpdateState::Idle),
            true
        ));
    }
}
