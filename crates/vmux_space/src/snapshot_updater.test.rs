use super::*;

#[test]
fn writes_active_name_and_url() {
    let mut app = App::new();
    app.init_resource::<CommandBarSpacesSnapshot>()
        .insert_resource(ActiveSpaceId(Some("space-1".to_string())))
        .add_systems(Update, update_spaces_snapshot);
    app.world_mut().spawn((
        Space,
        SpaceId("space-1".to_string()),
        Name::new("Space 1"),
        vmux_core::Active,
    ));
    app.update();
    let snap = app.world().resource::<CommandBarSpacesSnapshot>();
    assert_eq!(snap.spaces_page_url, SPACES_PAGE_URL);
    assert_eq!(snap.active_space_id, "space-1");
    assert_eq!(snap.active_space_name, "Space 1");
    assert_eq!(snap.spaces.len(), 1);
}
