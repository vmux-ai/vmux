use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemState;
use vmux_api::extension::ExtToggleRequest;
use vmux_app::VmuxPlugin;
use vmux_extension::snapshot::{ExtEntry, Index};
use vmux_extension::{Catalog, Mutation, Request, Root, Seed, WriteCompleted, WriteTaskRequest};

#[test]
fn an_independent_extension_core_keeps_profile_grants_separate_and_commits_after_io() {
    let mut app = App::new();
    app.add_plugins(VmuxPlugin::builder().extension_core(true).build());
    let mut snapshot = Index::default();
    let mut package = ExtEntry {
        id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        name: "Shared package".into(),
        version: "1".into(),
        permissions: vec!["tabs".into()],
        ..Default::default()
    };
    package.profile_enabled.insert("personal".into(), false);
    package.profile_enabled.insert("work".into(), false);
    snapshot.entries.push(package);
    let root = app
        .world_mut()
        .spawn((Root("/consumer/extensions".into()), Seed(Ok(snapshot))))
        .id();
    app.update();
    let mut state = SystemState::<Catalog>::new(app.world_mut());
    let ticket = state
        .get(app.world())
        .unwrap()
        .ticket(root)
        .unwrap()
        .clone();
    app.world_mut().spawn(
        Request {
            ticket,
            profile: "work".into(),
            mutation: Mutation::Toggle(ExtToggleRequest {
                id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                enabled: true,
                approve_permissions: true,
            }),
        }
        .bundle(),
    );
    app.update();
    let write = app
        .world_mut()
        .resource_mut::<Messages<WriteTaskRequest>>()
        .drain()
        .next()
        .unwrap();
    assert!(
        !state
            .get(app.world())
            .unwrap()
            .snapshot(root)
            .unwrap()
            .entries[0]
            .enabled_for("work")
    );
    app.world_mut().write_message(WriteCompleted {
        ticket: write.ticket,
        operation: write.operation,
        revision: write.revision,
        result: Ok(()),
    });
    app.update();
    let snapshot = state.get(app.world()).unwrap().snapshot(root).unwrap();
    assert!(snapshot.entries[0].enabled_for("work"));
    assert!(!snapshot.entries[0].enabled_for("personal"));
    assert_eq!(snapshot.entries[0].grants_for("work").permissions, ["tabs"]);
    assert!(
        snapshot.entries[0]
            .grants_for("personal")
            .permissions
            .is_empty()
    );
}
