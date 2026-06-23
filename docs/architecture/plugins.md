# Plugins: one crate, one capability

> Part of the [Vmux Architecture](../architecture.md) overview.

A Bevy `Plugin` is the unit of modularity. It bundles everything for one domain — the
components, systems, resources, messages, and observers — and registers them into the `App`
in a single `build()` call. One capability, one crate, one entry point.

## The `build()` contract

A plugin is a struct with one method. Everything the domain needs is wired up there:

```rust
pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_visits, broadcast_history_changed).chain())
            .add_systems(Startup, prune_history)
            .add_systems(Update, prune_history.run_if(on_timer(Duration::from_secs(3600))))
            .add_observer(on_history_query_request)
            .add_observer(on_history_open_request)
            .add_message::<HistoryOpenIntent>();
    }
}
```

Systems run every frame, gated by run conditions (`on_timer`) or ordering (`.chain()`,
system sets). Observers react to events. Messages declare the typed channels the domain
speaks over. Nothing leaks: a reader of `HistoryPlugin` sees the entire history feature in
one place.

## The app is a stack of plugins

The host assembles itself by stacking domain plugins — each from its own crate:

```rust
app.add_plugins((
    TerminalPlugin,
    EditorPlugin,
    ServicePlugin,
    SpacePlugin,
    BrowserPlugin,
    LayoutPlugin,
    AgentPlugin,
    // …one per crate
));
```

The same composition that builds an entity from components builds the *app* from plugins.
Adding a feature means adding a plugin and registering it here — nothing else changes.

## Decoupled by construction

Plugins don't call each other. Cross-crate behavior flows through Bevy **messages** and
**components**, never direct function calls: one plugin writes a typed message, another's
system reads it. Crates stay independent, and each plugin is testable on its own — drop it
into a headless `App`, send a message, run the schedule, assert on the resulting ECS state.

Composition over inheritance, all the way up: components compose an entity, plugins compose
the app.
