use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_fold::{
    Collapsed, FileId, FoldSet, LoadCompleted, LoadRequest, Mutation, Preferences, Root,
    SaveCompleted, SaveRequest, Snapshot, State, Ticket, UpdateRequest,
};

#[derive(Component)]
struct View {
    ticket: Ticket,
    file: FileId,
}

#[derive(Component, Default)]
struct Rendered(Collapsed);

fn project(preferences: Preferences, mut views: Query<(&View, &mut Rendered)>) {
    for (view, mut rendered) in &mut views {
        if let Some((_, collapsed, _)) = preferences.get(&view.ticket, &view.file.0) {
            rendered.0 = collapsed.clone();
        }
    }
}

#[test]
fn a_public_fold_plugin_shares_file_preferences_without_a_product_or_io_host() {
    let mut app = App::new();
    app.add_plugins(VmuxPlugin::builder().fold(true).build())
        .add_systems(Update, project.in_set(FoldSet::Project));
    let root = app
        .world_mut()
        .spawn(Root("/consumer/folds.ron".into()))
        .id();
    app.update();
    let ticket = app
        .world_mut()
        .resource_mut::<Messages<LoadRequest>>()
        .drain()
        .next()
        .unwrap()
        .0;
    app.world_mut().write_message(LoadCompleted {
        ticket: ticket.clone(),
        result: Ok(Snapshot {
            files: [("/consumer/file.rs".into(), vec![2])].into(),
        }),
    });
    let mut views = Vec::new();
    for name in ["First view", "Second view"] {
        views.push(
            app.world_mut()
                .spawn((
                    Name::new(name),
                    View {
                        ticket: ticket.clone(),
                        file: FileId("/consumer/file.rs".into()),
                    },
                    Rendered::default(),
                ))
                .id(),
        );
    }
    app.update();
    for (view, line) in [(views[0], 7), (views[1], 11)] {
        let view = app.world().get::<View>(view).unwrap();
        let request = UpdateRequest {
            ticket: view.ticket.clone(),
            file: view.file.clone(),
            mutation: Mutation::Change {
                collapse: [line].into(),
                expand: Default::default(),
            },
        };
        app.world_mut().write_message(request);
    }
    app.update();
    let save = app
        .world_mut()
        .resource_mut::<Messages<SaveRequest>>()
        .drain()
        .next()
        .unwrap();
    assert_eq!(save.snapshot.files["/consumer/file.rs"], [2, 7, 11]);
    for view in views {
        assert_eq!(
            app.world().get::<Rendered>(view).unwrap().0,
            Collapsed([2, 7, 11].into())
        );
    }
    app.world_mut().write_message(SaveCompleted {
        ticket,
        operation: save.operation,
        revision: save.revision,
        result: Ok(()),
    });
    app.update();
    assert_eq!(
        app.world().get::<State>(root).unwrap().loaded(),
        Some(save.revision)
    );
}
