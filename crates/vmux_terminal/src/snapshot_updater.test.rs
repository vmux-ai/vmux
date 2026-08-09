use super::*;

#[test]
fn writes_url_and_empty_pid_map() {
    let mut app = App::new();
    app.init_resource::<CommandBarTerminalsSnapshot>()
        .add_systems(Update, update_terminals_snapshot);
    app.update();
    let snap = app.world().resource::<CommandBarTerminalsSnapshot>();
    assert_eq!(snap.terminal_page_url, TERMINAL_PAGE_URL);
    assert!(snap.pid_to_entity.is_empty());
}
