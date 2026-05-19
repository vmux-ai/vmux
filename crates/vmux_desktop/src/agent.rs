pub(crate) use vmux_agent::desktop_plugin::*;

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;
    use bevy_cef::prelude::WebviewExtendStandardMaterial;
    use vmux_agent::events::AgentCommandRequest;
    use vmux_agent::strategy::AgentStrategies;
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

    #[test]
    fn terminal_send_writes_raw_text_to_active_terminal() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(vmux_command::CommandPlugin);
        app.add_plugins(AgentPlugin);
        app.add_systems(Update, vmux_terminal::handle_terminal_send_requests);
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
}
