use super::*;

fn inputs() -> BootInputs {
    BootInputs {
        space_present: false,
        restore_complete: false,
        layout_ready: false,
        total_pages: 0,
        ready_pages: 0,
    }
}

#[test]
fn starting_when_nothing_ready() {
    let (phase, reveal) = compute(inputs());
    assert_eq!(phase, BootPhase::Starting);
    assert!(!reveal);
}

#[test]
fn restoring_space_when_present_and_not_complete() {
    let (phase, _) = compute(BootInputs {
        space_present: true,
        ..inputs()
    });
    assert_eq!(phase, BootPhase::RestoringSpace);
}

#[test]
fn loading_interface_after_restore_complete() {
    let (phase, _) = compute(BootInputs {
        space_present: true,
        restore_complete: true,
        ..inputs()
    });
    assert_eq!(phase, BootPhase::LoadingInterface);
}

#[test]
fn loading_interface_on_fresh_boot_once_complete() {
    let (phase, _) = compute(BootInputs {
        restore_complete: true,
        ..inputs()
    });
    assert_eq!(phase, BootPhase::LoadingInterface);
}

#[test]
fn loading_pages_counts_when_layout_ready() {
    let (phase, _) = compute(BootInputs {
        layout_ready: true,
        total_pages: 5,
        ready_pages: 2,
        ..inputs()
    });
    assert_eq!(phase, BootPhase::LoadingPages { ready: 2, total: 5 });
}

#[test]
fn not_revealed_until_layout_ready() {
    let (_, reveal) = compute(BootInputs {
        layout_ready: false,
        ..inputs()
    });
    assert!(!reveal);
}

#[test]
fn revealed_when_layout_ready() {
    let (_, reveal) = compute(BootInputs {
        layout_ready: true,
        ..inputs()
    });
    assert!(reveal);
}

#[test]
fn revealed_when_layout_ready_even_while_pages_pending() {
    let (_, reveal) = compute(BootInputs {
        layout_ready: true,
        total_pages: 3,
        ready_pages: 0,
        ..inputs()
    });
    assert!(reveal);
}

#[test]
fn display_strings() {
    assert_eq!(BootPhase::Starting.display(), "Starting...");
    assert_eq!(BootPhase::RestoringSpace.display(), "Restoring space...");
    assert_eq!(
        BootPhase::LoadingInterface.display(),
        "Loading interface..."
    );
    assert_eq!(
        BootPhase::LoadingPages { ready: 2, total: 5 }.display(),
        "Loading page 2/5..."
    );
}

#[test]
fn system_reports_loading_pages_and_reveals_on_layout_ready() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<SplashStatus>()
        .init_resource::<RestoreComplete>()
        .insert_resource(SpaceFilePresent(true))
        .add_systems(Update, compute_boot_status);

    app.world_mut().spawn((LayoutCef, PageReady {}));
    let stack = app.world_mut().spawn(Stack::default()).id();
    app.world_mut().spawn((PageReady {}, ChildOf(stack)));

    app.update();

    let status = app.world().resource::<SplashStatus>();
    assert_eq!(status.phase, BootPhase::LoadingPages { ready: 1, total: 1 });
    assert!(status.reveal_ready);
}

#[test]
fn system_reports_restoring_space_before_layout_ready() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<SplashStatus>()
        .init_resource::<RestoreComplete>()
        .insert_resource(SpaceFilePresent(true))
        .add_systems(Update, compute_boot_status);

    app.update();

    let status = app.world().resource::<SplashStatus>();
    assert_eq!(status.phase, BootPhase::RestoringSpace);
    assert!(!status.reveal_ready);
}
