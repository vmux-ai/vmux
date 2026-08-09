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
#[path = "host_spawn.test.rs"]
mod tests;
