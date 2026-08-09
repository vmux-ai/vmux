use super::*;

#[test]
fn archived_page_defaults_are_empty() {
    let a = ArchivedPage::default();
    assert!(a.url.is_empty());
    assert!(a.launch.is_none());
    assert_eq!(a.closed_at, 0);
}

#[test]
fn archived_page_is_registered_by_core_plugin() {
    let mut app = App::new();
    app.add_plugins(crate::CorePlugin);
    let registry = app.world().resource::<AppTypeRegistry>().read();
    assert!(
        registry
            .get(std::any::TypeId::of::<ArchivedPage>())
            .is_some()
    );
}

#[test]
fn archived_position_types_registered_by_core_plugin() {
    let mut app = App::new();
    app.add_plugins(crate::CorePlugin);
    let registry = app.world().resource::<AppTypeRegistry>().read();
    assert!(
        registry
            .get(std::any::TypeId::of::<ArchivedPagePosition>())
            .is_some()
    );
    assert!(
        registry
            .get(std::any::TypeId::of::<ArchivedTabPage>())
            .is_some()
    );
    assert!(registry.get(std::any::TypeId::of::<PaneStep>()).is_some());
    assert!(registry.get(std::any::TypeId::of::<SplitAxis>()).is_some());
}
