use super::*;

fn build_test_cef(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let host = commands.spawn_empty().id();
    commands.spawn(layout_cef_bundle(host, &mut meshes, &mut webview_mt));
}

fn build_test_page(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    commands.spawn(Browser::new(
        &mut meshes,
        &mut webview_mt,
        "https://example.com",
    ));
}

#[test]
fn layout_cef_uses_manual_pointer_routing() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Startup, build_test_cef);
    app.update();

    let pickable = app
        .world_mut()
        .query_filtered::<&Pickable, With<LayoutCef>>()
        .single(app.world())
        .expect("layout CEF shell pickable");

    assert_eq!(pickable, &Pickable::IGNORE);
}

#[test]
fn page_cef_uses_opaque_dark_initial_background() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Startup, build_test_page);
    app.update();

    let page = app
        .world_mut()
        .query_filtered::<Entity, (With<Browser>, Without<LayoutCef>)>()
        .single(app.world())
        .expect("page CEF");

    assert!(
        app.world()
            .get::<WebviewOpaqueWindowedBackground>(page)
            .is_some()
    );
}

#[test]
fn page_cef_allows_native_first_responder() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Startup, build_test_page);
    app.update();

    let page = app
        .world_mut()
        .query_filtered::<Entity, (With<Browser>, Without<LayoutCef>)>()
        .single(app.world())
        .expect("page CEF");

    assert!(
        app.world()
            .get::<WebviewWindowedNativeFocus>(page)
            .is_some(),
        "windowed web pages must allow native first-responder so they are typeable without a click"
    );
}
