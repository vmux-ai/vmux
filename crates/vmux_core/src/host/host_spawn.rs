use bevy::prelude::*;
use std::collections::HashSet;

#[derive(Resource, Default, Debug, Clone)]
pub struct HostSpawnRegistry(pub HashSet<String>);

impl HostSpawnRegistry {
    pub fn register(&mut self, host: &str) {
        self.0.insert(host.to_string());
    }

    pub fn needs_host_spawn(&self, url: &str) -> bool {
        if url.starts_with("file:") {
            return true;
        }
        vmux_host(url).is_some_and(|host| self.0.contains(host))
    }
}

fn vmux_host(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("vmux://")?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    (!host.is_empty()).then_some(host)
}

pub fn register_host_spawn(app: &mut App, host: &'static str) {
    app.init_resource::<HostSpawnRegistry>();
    app.world_mut()
        .resource_mut::<HostSpawnRegistry>()
        .register(host);
}

#[cfg(test)]
mod tests {
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
}
