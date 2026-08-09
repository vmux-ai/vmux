use super::*;

fn registry(hosts: &[&str]) -> HostSpawnRegistry {
    let mut r = HostSpawnRegistry::default();
    for h in hosts {
        r.register(h);
    }
    r
}

#[test]
fn file_scheme_always_needs_host_spawn() {
    assert!(HostSpawnRegistry::default().needs_host_spawn("file:///tmp/x.rs"));
}

#[test]
fn registered_host_matches_on_boundary() {
    let r = registry(&["services", "terminal"]);
    assert!(r.needs_host_spawn("vmux://services/"));
    assert!(r.needs_host_spawn("vmux://services"));
    assert!(r.needs_host_spawn("vmux://terminal/?pid=1"));
}

#[test]
fn unregistered_or_partial_host_does_not_match() {
    let r = registry(&["services", "terminal"]);
    assert!(!r.needs_host_spawn("vmux://settings/"));
    assert!(!r.needs_host_spawn("vmux://terminals/"));
    assert!(!r.needs_host_spawn("vmux://services-x/"));
    assert!(!r.needs_host_spawn("https://example.com"));
}

#[test]
fn registering_settings_makes_it_match() {
    let r = registry(&["settings"]);
    assert!(r.needs_host_spawn("vmux://settings/"));
}

#[test]
fn register_is_idempotent() {
    let mut r = HostSpawnRegistry::default();
    r.register("team");
    r.register("team");
    assert_eq!(r.0.len(), 1);
}

#[test]
fn register_host_spawn_inserts_resource() {
    let mut app = App::new();
    register_host_spawn(&mut app, "spaces");
    register_host_spawn(&mut app, "team");
    let reg = app.world().resource::<HostSpawnRegistry>();
    assert!(reg.needs_host_spawn("vmux://spaces/"));
    assert!(reg.needs_host_spawn("vmux://team/"));
}
