use bevy::prelude::*;
use std::collections::HashSet;

/// Set of `vmux://` hosts whose pages must be created through the
/// [`PageOpenSet::HandleKnownPages`](crate::page_open::PageOpenSet) spawn pipeline
/// rather than navigated in place.
///
/// Pages that render backend-pushed data gate their host emits on a per-page marker
/// component (e.g. `ProcessesMonitor`, `Settings`, `Team`, `Spaces`). That marker is
/// only attached when the page is spawned by its known-page handler; navigating an
/// existing generic webview in place leaves it markerless, so the backend never
/// targets it and the page stays empty. Each owning crate registers its host via
/// [`register_host_spawn`] next to its `HandleKnownPages` handler.
#[derive(Resource, Default, Debug, Clone)]
pub struct HostSpawnRegistry(pub HashSet<String>);

impl HostSpawnRegistry {
    /// Register a `vmux://` host (e.g. `"services"`) as requiring host-spawn.
    pub fn register(&mut self, host: &str) {
        self.0.insert(host.to_string());
    }

    /// Whether opening `url` must route through the host-spawn pipeline: either the
    /// `file:` scheme (editor file viewer) or a registered `vmux://<host>`.
    pub fn needs_host_spawn(&self, url: &str) -> bool {
        if url.starts_with("file:") {
            return true;
        }
        vmux_host(url).is_some_and(|host| self.0.contains(host))
    }
}

/// Extract the host of a `vmux://<host>[/…]` URL, matching only on a host boundary so
/// `vmux://terminals/` is not treated as host `terminal`.
fn vmux_host(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("vmux://")?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    (!host.is_empty()).then_some(host)
}

/// Register `host` as host-spawned. Call from a plugin `build()` alongside the crate's
/// [`PageOpenSet::HandleKnownPages`](crate::page_open::PageOpenSet) handler.
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
