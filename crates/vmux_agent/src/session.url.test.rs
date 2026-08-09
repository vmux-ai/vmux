use super::*;
use crate::client::cli::vibe::VibeStrategy;

fn empty_meta() -> PageMetadata {
    PageMetadata {
        title: String::new(),
        url: String::new(),
        icon: vmux_core::PageIcon::None,
        bg_color: None,
    }
}

#[test]
fn format_agent_url_emits_scheme_with_session_id() {
    let mut app = App::new();
    let mut strategies = AgentStrategies::default();
    strategies.register_cli(Box::new(VibeStrategy));
    app.insert_resource(strategies)
        .add_systems(Update, format_agent_url);

    let entity = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Vibe,
            },
            SessionId("abc".into()),
            empty_meta(),
        ))
        .id();
    app.update();
    let url = &app.world().get::<PageMetadata>(entity).unwrap().url;
    assert_eq!(url, "vmux://agent/vibe/cli/abc");
}

#[test]
fn format_agent_url_emits_fresh_cli_url_when_no_session_id() {
    let mut app = App::new();
    let mut strategies = AgentStrategies::default();
    strategies.register_cli(Box::new(VibeStrategy));
    app.insert_resource(strategies)
        .add_systems(Update, format_agent_url);

    let entity = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Vibe,
            },
            empty_meta(),
        ))
        .id();
    app.update();
    let url = &app.world().get::<PageMetadata>(entity).unwrap().url;
    assert_eq!(url, "vmux://agent/vibe/cli");
}

#[test]
fn format_agent_url_sets_title_with_short_session_id() {
    let mut app = App::new();
    let mut strategies = AgentStrategies::default();
    strategies.register_cli(Box::new(VibeStrategy));
    app.insert_resource(strategies)
        .add_systems(Update, format_agent_url);

    let entity = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Vibe,
            },
            SessionId("abc12345".into()),
            empty_meta(),
        ))
        .id();
    app.update();
    let title = &app.world().get::<PageMetadata>(entity).unwrap().title;
    assert_eq!(title, "Vibe CLI (abc12345)");
}

#[test]
fn format_agent_url_truncates_long_session_id_in_title() {
    let mut app = App::new();
    let mut strategies = AgentStrategies::default();
    strategies.register_cli(Box::new(VibeStrategy));
    app.insert_resource(strategies)
        .add_systems(Update, format_agent_url);

    let entity = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Vibe,
            },
            SessionId("550e8400e29b41d4a716446655440000".into()),
            empty_meta(),
        ))
        .id();
    app.update();
    let title = &app.world().get::<PageMetadata>(entity).unwrap().title;
    assert_eq!(title, "Vibe CLI (550e84…0000)");
}

#[test]
fn format_agent_url_sets_bare_name_title_when_no_session_id() {
    let mut app = App::new();
    let mut strategies = AgentStrategies::default();
    strategies.register_cli(Box::new(VibeStrategy));
    app.insert_resource(strategies)
        .add_systems(Update, format_agent_url);

    let entity = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Vibe,
            },
            empty_meta(),
        ))
        .id();
    app.update();
    let title = &app.world().get::<PageMetadata>(entity).unwrap().title;
    assert_eq!(title, "Vibe CLI");
}

#[test]
fn format_agent_url_clears_stale_builtin_icon_so_provider_favicon_resolves() {
    let mut app = App::new();
    let mut strategies = AgentStrategies::default();
    strategies.register_cli(Box::new(VibeStrategy));
    app.insert_resource(strategies)
        .add_systems(Update, format_agent_url);

    let entity = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Vibe,
            },
            PageMetadata {
                title: "Terminal".into(),
                url: vmux_core::event::TERMINAL_PAGE_URL.to_string(),
                icon: vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Terminal),
                bg_color: None,
            },
        ))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<PageMetadata>(entity).unwrap().icon,
        vmux_core::PageIcon::None
    );
}

#[test]
fn truncate_sid_keeps_short_ids() {
    assert_eq!(truncate_sid("abc"), "abc");
    assert_eq!(truncate_sid("abcdefghijkl"), "abcdefghijkl");
}

#[test]
fn truncate_sid_middle_truncates_long_ids() {
    assert_eq!(truncate_sid("abcdefghijklm"), "abcdef…jklm");
    assert_eq!(
        truncate_sid("550e8400e29b41d4a716446655440000"),
        "550e84…0000"
    );
}
