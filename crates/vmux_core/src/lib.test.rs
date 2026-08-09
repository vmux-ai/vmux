use super::*;

#[test]
fn registers_new_history_components() {
    let mut app = App::new();
    app.add_plugins(CorePlugin);

    let registry = app.world().resource::<AppTypeRegistry>().read();
    assert!(registry.get(std::any::TypeId::of::<Url>()).is_some());
    assert!(registry.get(std::any::TypeId::of::<VisitCount>()).is_some());
    assert!(
        registry
            .get(std::any::TypeId::of::<LastVisitedAt>())
            .is_some()
    );
    assert!(registry.get(std::any::TypeId::of::<VisitedUrl>()).is_some());
    assert!(
        registry
            .get(std::any::TypeId::of::<TransitionType>())
            .is_some()
    );
}

#[test]
fn registers_bookmark_components() {
    let mut app = App::new();
    app.add_plugins(CorePlugin);
    let registry = app.world().resource::<AppTypeRegistry>().read();
    assert!(
        registry
            .get(std::any::TypeId::of::<BookmarkOrder>())
            .is_some()
    );
    assert!(registry.get(std::any::TypeId::of::<Pin>()).is_some());
    assert!(registry.get(std::any::TypeId::of::<Bookmark>()).is_some());
    assert!(registry.get(std::any::TypeId::of::<Folder>()).is_some());
    assert!(registry.get(std::any::TypeId::of::<Collapsed>()).is_some());
    assert!(registry.get(std::any::TypeId::of::<Uuid>()).is_some());
}

#[test]
fn active_marker_is_registered_and_reflectable() {
    let mut app = App::new();
    app.add_plugins(CorePlugin);

    let registry = app.world().resource::<AppTypeRegistry>().read();
    assert!(registry.get(std::any::TypeId::of::<Active>()).is_some());
}
