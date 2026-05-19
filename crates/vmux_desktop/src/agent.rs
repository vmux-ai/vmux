pub(crate) use vmux_agent::desktop_plugin::*;

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::relationship::Relationship;
    use bevy::prelude::*;
    use bevy_cef::prelude::WebviewExtendStandardMaterial;
    use vmux_agent::events::AgentCommandRequest;
    use vmux_agent::strategy::AgentStrategies;
    use vmux_core::PageMetadata;
    use vmux_layout::pane::Pane;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_layout::stack::FocusedStack;
    use vmux_service::protocol::{AgentCommand as ServiceAgentCommand, AgentRequestId};
    use vmux_settings::{AppSettings, BrowserSettings, ShortcutSettings};
    use vmux_terminal::PendingTerminalInput;
    use vmux_terminal::Terminal;

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
            },
            layout: LayoutSettings {
                radius: 0.0,
                window: WindowSettings {
                    padding: 0.0,
                    padding_top: None,
                    padding_right: None,
                    padding_bottom: None,
                    padding_left: None,
                },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            startup_url: None,
            agent: vmux_settings::AgentSettings::default(),
        }
    }

    fn add_consumer_systems(app: &mut App) {
        app.add_systems(
            Update,
            (
                crate::browser::handle_browser_navigate_requests,
                vmux_terminal::handle_terminal_send_requests,
                vmux_terminal::handle_run_shell_requests,
            ),
        );
    }

    #[derive(Resource, Default)]
    struct CapturedNavigateUrls(Vec<String>);

    #[test]
    fn browser_navigate_triggers_request_navigate_with_url() {
        use bevy_cef::prelude::RequestNavigate;
        use vmux_layout::Browser;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(vmux_command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        add_consumer_systems(&mut app);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();
        app.init_resource::<CapturedNavigateUrls>();

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .insert(ChildOf(pane))
            .id();
        app.world_mut().spawn(Browser).insert(ChildOf(stack));

        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);

        app.add_observer(
            |trigger: On<RequestNavigate>, mut captured: ResMut<CapturedNavigateUrls>| {
                captured.0.push(trigger.url.clone());
            },
        );

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "https://example.com".to_string(),
                    pane: None,
                },
            });

        app.update();
        app.update();

        let captured = app.world().resource::<CapturedNavigateUrls>();
        assert_eq!(captured.0, vec!["https://example.com".to_string()]);
    }

    #[test]
    fn terminal_send_writes_raw_text_to_active_terminal() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(vmux_command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        add_consumer_systems(&mut app);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .insert(ChildOf(pane))
            .id();
        let terminal = app.world_mut().spawn(Terminal).insert(ChildOf(stack)).id();

        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::TerminalSend {
                    text: "ls".to_string(),
                    terminal: None,
                },
            });

        app.update();
        app.update();

        let pending = app
            .world()
            .get::<PendingTerminalInput>(terminal)
            .expect("PendingTerminalInput inserted");
        assert_eq!(pending.data, b"ls".to_vec());
    }

    #[test]
    fn browser_navigate_auto_spawns_tab_when_pane_is_empty() {
        use vmux_layout::Browser;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(vmux_command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        add_consumer_systems(&mut app);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();

        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
        app.world_mut().resource_mut::<FocusedStack>().stack = None;

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "https://example.com".to_string(),
                    pane: None,
                },
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
    fn browser_navigate_targets_specific_pane_when_id_provided() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(vmux_command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        add_consumer_systems(&mut app);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane_a = app.world_mut().spawn(Pane).id();
        let pane_b = app.world_mut().spawn(Pane).id();

        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane_a);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "https://example.com".to_string(),
                    pane: Some(pane_b.to_bits().to_string()),
                },
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
        app.add_plugins(MinimalPlugins);
        app.add_plugins(vmux_command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        add_consumer_systems(&mut app);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://terminal/".to_string(),
                    pane: None,
                },
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let terminal_count = world.query::<&Terminal>().iter(world).count();
        assert!(
            terminal_count >= 1,
            "terminal should be spawned in focused pane"
        );
    }

    #[test]
    fn browser_navigate_with_terminal_url_and_target_pane_uses_target() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(vmux_command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        add_consumer_systems(&mut app);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane_a = app.world_mut().spawn(Pane).id();
        let pane_b = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane_a);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://terminal/".to_string(),
                    pane: Some(pane_b.to_bits().to_string()),
                },
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
        use vmux_layout::Browser;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(vmux_command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        add_consumer_systems(&mut app);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://nonsense/".to_string(),
                    pane: None,
                },
            });

        app.update();
        app.update();

        let world = app.world_mut();
        let browser_count = world.query::<&Browser>().iter(world).count();
        let terminal_count = world.query::<&Terminal>().iter(world).count();
        assert_eq!(
            browser_count, 0,
            "no browser should be spawned for unknown vmux URL"
        );
        assert_eq!(
            terminal_count, 0,
            "no terminal should be spawned for unknown vmux URL"
        );
    }

    #[test]
    fn browser_navigate_with_claude_url_does_not_spawn_standalone_browser() {
        use vmux_layout::Browser;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(vmux_command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        add_consumer_systems(&mut app);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://agent/claude/cli/".into(),
                    pane: None,
                },
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
        use vmux_layout::Browser;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(vmux_command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        add_consumer_systems(&mut app);
        app.init_resource::<AgentStrategies>();
        app.insert_resource(FocusedStack::default());
        app.insert_resource(test_settings());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();

        let pane = app.world_mut().spawn(Pane).id();
        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                command: ServiceAgentCommand::BrowserNavigate {
                    url: "vmux://agent/codex/cli/".into(),
                    pane: None,
                },
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
