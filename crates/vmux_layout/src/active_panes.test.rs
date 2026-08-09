use super::*;

#[test]
fn apply_sets_per_profile_without_cross_contamination() {
    let mut app = App::new();
    app.init_resource::<ActivePanes>()
        .add_message::<ActivatePane>()
        .add_systems(Update, apply_active_panes);

    let (user_pane, agent_pane) = {
        let world = app.world_mut();
        (world.spawn_empty().id(), world.spawn_empty().id())
    };
    app.world_mut().write_message(ActivatePane {
        profile: ProfileId::Local,
        active: ActiveStack {
            tab: None,
            pane: Some(user_pane),
            stack: None,
            kind: None,
        },
    });
    app.world_mut().write_message(ActivatePane {
        profile: ProfileId::Agent("a1".to_string()),
        active: ActiveStack {
            tab: None,
            pane: Some(agent_pane),
            stack: None,
            kind: None,
        },
    });
    app.update();

    let active = app.world().resource::<ActivePanes>();
    assert_eq!(active.get(&ProfileId::Local).unwrap().pane, Some(user_pane));
    assert_eq!(
        active
            .get(&ProfileId::Agent("a1".to_string()))
            .unwrap()
            .pane,
        Some(agent_pane)
    );
}

#[test]
fn agent_activation_does_not_touch_local() {
    let mut app = App::new();
    app.init_resource::<ActivePanes>()
        .add_message::<ActivatePane>()
        .add_systems(Update, apply_active_panes);

    let agent_pane = app.world_mut().spawn_empty().id();
    app.world_mut().resource_mut::<ActivePanes>().0.insert(
        ProfileId::Local,
        ActiveStack {
            tab: None,
            pane: None,
            stack: None,
            kind: None,
        },
    );
    app.world_mut().write_message(ActivatePane {
        profile: ProfileId::Agent("a1".to_string()),
        active: ActiveStack {
            tab: None,
            pane: Some(agent_pane),
            stack: None,
            kind: None,
        },
    });
    app.update();

    let active = app.world().resource::<ActivePanes>();
    assert_eq!(active.get(&ProfileId::Local).unwrap().pane, None);
    assert_eq!(
        active
            .get(&ProfileId::Agent("a1".to_string()))
            .unwrap()
            .pane,
        Some(agent_pane)
    );
}

#[test]
fn activation_without_kind_preserves_profile_kind() {
    let mut app = App::new();
    app.init_resource::<ActivePanes>()
        .add_message::<ActivatePane>()
        .add_systems(Update, apply_active_panes);

    let profile = ProfileId::Agent("a1".to_string());
    let pane = app.world_mut().spawn_empty().id();
    app.world_mut().resource_mut::<ActivePanes>().0.insert(
        profile.clone(),
        ActiveStack {
            tab: None,
            pane: Some(pane),
            stack: None,
            kind: Some(vmux_core::agent::AgentKind::Codex),
        },
    );
    app.world_mut().write_message(ActivatePane {
        profile: profile.clone(),
        active: ActiveStack {
            tab: None,
            pane: Some(pane),
            stack: None,
            kind: None,
        },
    });

    app.update();

    assert_eq!(
        app.world()
            .resource::<ActivePanes>()
            .get(&profile)
            .unwrap()
            .kind,
        Some(vmux_core::agent::AgentKind::Codex)
    );
}
