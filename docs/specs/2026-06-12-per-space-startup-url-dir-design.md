# Per-space startup_url and startup_dir

## Problem

`startup_url` is global-only (`AppSettings.browser.startup_url`, resolved to the
`EffectiveStartupUrl` resource via `resolve_startup_url`). There is no
`startup_dir` at all — new terminals and agent sessions always default their cwd
to the built-in `space_dir(active_id)` (`~/.vmux/<space_id>`, auto-created).

Users want each space to open its browser at a different URL and its terminals
in a different directory, while keeping a global default for spaces that don't
override. The agent must be able to set these (it already edits `settings.ron`
via the `update_settings` / `get_settings` MCP tools).

## Goal

- Per-space override for both `startup_url` and `startup_dir`.
- Global default for both (one already exists for url).
- Resolution: **per-space → global → built-in**.
- Agent-editable through the existing `settings.ron` mechanism (no new tool).

## Scope

- **In:** `startup_url` (browser stacks/tabs/panes + first-stack startup) and
  `startup_dir` (default cwd for new terminals **and** agent sessions).
- **Out:** per-space anything else; a settings/spaces UI for these fields
  (agent + hand-edit only); making `startup_dir` affect relative `file://`
  startup_url resolution; pruning override entries when a space is deleted
  (orphans are left and ignored — safe across re-create).

## Decisions (resolved during brainstorming)

1. `startup_dir` governs **terminal + agent cwd**.
2. Resolution chain is **per-space → global → built-in** for both values.
3. Per-space overrides live in **`settings.ron`** under a `spaces` map (not in
   `spaces.ron`/`SpaceRecord`), so the existing agent tools reach them.
4. Agent can create a new per-space entry because the `spaces` map is **seeded
   from the registry** — every known space id always has an entry, so
   `set_at_path` (which rejects unknown paths) needs no change.

## Data model — `AppSettings` (`vmux_setting/src/plugin/runtime.rs`)

```rust
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppSettings {
    // … existing fields …
    #[serde(default)]
    pub spaces: std::collections::BTreeMap<String, SpaceOverrides>, // NEW
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SpaceOverrides {
    #[serde(default)]
    pub startup_url: Option<String>,
    #[serde(default)]
    pub startup_dir: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BrowserSettings {
    #[serde(default = "default_browser_startup_url")]
    pub startup_url: String,        // existing global
    #[serde(default)]
    pub startup_dir: Option<String>, // NEW global
}
```

- `BTreeMap` for stable serialization order.
- `SpaceOverrides` fields **must not** use `skip_serializing_if`: `None` must
  serialize as `null` so the keys stay present once an entry exists, which is
  what lets `set_at_path` (`spaces.<id>.startup_url`) target them.
- Global `startup_dir` lives in `BrowserSettings` to sit beside `startup_url` and
  because that struct is always present (serde default), keeping the agent path
  `browser.startup_dir` stable. (Naming caveat: "browser" is a slight misnomer
  for a terminal dir; accepted to avoid moving the existing `browser.startup_url`
  and to keep both startup values co-located.)

## Resolution helpers (`vmux_setting/src/plugin/runtime.rs`, pure fns)

```rust
pub fn resolve_startup_url(settings: &AppSettings, space_id: &str) -> String {
    let per_space = settings
        .spaces
        .get(space_id)
        .and_then(|o| o.startup_url.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let chosen = per_space.unwrap_or_else(|| settings.browser.startup_url.trim());
    if chosen.is_empty() || chosen == "vmux://agent/" || chosen == "vmux://agent" {
        default_browser_startup_url()
    } else {
        chosen.to_string()
    }
}

pub fn resolve_startup_dir(settings: &AppSettings, space_id: &str) -> std::path::PathBuf {
    let candidate = settings
        .spaces
        .get(space_id)
        .and_then(|o| o.startup_dir.as_deref())
        .or(settings.browser.startup_dir.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(dir) = candidate {
        let p = std::path::PathBuf::from(dir);
        if p.is_dir() {
            return p;
        }
    }
    vmux_core::profile::space_dir(space_id) // built-in, auto-created
}
```

- `resolve_startup_url` gains a `space_id` param (existing legacy
  `vmux://agent` → google preserved). All call sites pass the active space id.
- `resolve_startup_dir` falls through tiers on empty/missing/non-dir; startup
  never errors.

Also add `pub fn serialize_settings_to_ron(&AppSettings) -> Result<String, String>`
(reusing the same `ron::ser::PrettyConfig` as `apply_settings_update`) for the
seeder to persist.

## Seeding (`vmux_space`, owns `ActiveSpace` + registry, depends on `vmux_setting`)

`set_at_path` rejects unknown paths, so `spaces.<id>` must already exist before
the agent can set `spaces.<id>.startup_url`. A reconcile system guarantees that.

`reconcile_space_overrides` system:
- Runs at `Startup`, and in `Update` when `AppSettings` **or** `ActiveSpace`
  changes. The `AppSettings` trigger covers initial load and hand-edit reloads
  via `reload_settings_on_change` (which replaces the whole resource and would
  otherwise drop seeded entries); the `ActiveSpace` trigger covers a freshly
  created space (`on_space_command "new"` switches the active space) without
  needing the reconcile logic duplicated into the command handler.
- Reads the registry (`read_space_registry_from(shared_data_dir())`); for each
  registry space id missing from `settings.spaces`, inserts
  `SpaceOverrides::default()`.
- If it added ≥1 entry: `serialize_settings_to_ron` + send
  `SettingsWriteRequest` so disk mirrors the registry. `persist_settings_to_disk`
  records `last_hash`, so the resulting file event is suppressed by
  `reload_settings_on_change` (no reload loop).
- Delete: orphan entry left in `settings.spaces` (ignored by resolvers).

Registration must order after settings load at `Startup`
(`reconcile_space_overrides.after(SettingsLoadSet)`), referencing the
`SettingsLoadSet` re-exported from `vmux_setting`.

## URL wiring — move the updater into `vmux_space`

`EffectiveStartupUrl` (defined in `vmux_layout::settings`) is read by layout
consumers (`window.rs:476`, `tab.rs:80`, `pane.rs:443/822`, `stack.rs:191/579`).
`vmux_layout` does **not** depend on `vmux_space`, so those consumers cannot read
`ActiveSpace` — the resource must stay, and its updater must live where both
`AppSettings` and `ActiveSpace` are visible.

- Remove `update_effective_startup_url` registration from `SettingsPlugin`
  (`vmux_setting/src/plugin.rs:33,41`). Keep `resolve_startup_url` pub there.
- Add a space-aware updater in `vmux_space` that recomputes `EffectiveStartupUrl`
  when `AppSettings` **or** `ActiveSpace` changes, using
  `resolve_startup_url(&settings, &active.record.id)`. Order it the same way the
  old one was: at `Startup` after settings load and before
  `LayoutStartupSet::Post`, plus in `Update`.
- `EffectiveStartupUrl` resource init stays in `SettingsPlugin`
  (`init_resource::<EffectiveStartupUrl>()`), which is fine — it just needs to
  exist before the `vmux_space` updater runs.
- Layout consumers are unchanged.

## Dir wiring — replace `space_dir(active_id)` at the existing default sites

Exactly the three sites that default a terminal/agent cwd to
`space_dir(active_id)` today switch to `resolve_startup_dir(&settings, active_id)`.
The built-in fallback tier of `resolve_startup_dir` **is** `space_dir(id)`, so
with no config set the behavior is byte-for-byte identical to today — only the
per-space/global override is new.

- `vmux_terminal/src/plugin.rs`
  - `spawn_layout_requested_content` (`:370`) — has `settings` + `active_space`;
    swap `space_dir` → `resolve_startup_dir`. (Main "new terminal tab" path.)
  - `open_terminal_page` (`:475`) — default branch (`cwd_param` absent); swap
    `space_dir` → `resolve_startup_dir`. The explicit `?cwd=` branch is unchanged.
- `vmux_agent/src/plugin.rs`
  - `NewTerminalTab` default (`:322`) — swap the
    `space_dir(id)` / `default_space_dir()` default → `resolve_startup_dir`.

Explicit user-typed cwd (command-bar path entry, `?cwd=` query, MCP `cwd` arg)
is unchanged — only the existing space default changes.

Deliberately **not** changed (preserve current behavior; see follow-ups): the
`respond_terminal_spawn` / `respond_terminal_stack_spawn` responders, which today
treat a `None` cwd as "process default" (≈ `$HOME`), and agent `RunShell`, which
passes its cwd through verbatim. These already do **not** use `space_dir`, so
leaving them avoids an unrelated `$HOME → startup_dir` behavior change.

## Edge cases

- Empty per-space url/dir string → treated as unset → next tier.
- Per-space dir set but non-existent / not a dir → falls through to global, then
  built-in `space_dir(id)`.
- Hand-edit of `settings.ron` removing `spaces` → reconcile re-seeds in-memory
  and re-persists.
- Agent sets `spaces.<unknown-id>.startup_url` → `set_at_path` returns
  "unknown setting path" (correct: no such space).
- Active space switches → `EffectiveStartupUrl` recomputes; already-open stacks
  keep their content (only new opens use the new value), matching today.

## Testing

Resolver (`vmux_setting`):
- url: per-space wins; falls to global; falls to google; empty per-space →
  global; legacy `vmux://agent` → google.
- dir: per-space wins; falls to global; falls to `space_dir(id)`; non-dir
  per-space → fallthrough; empty → fallthrough.
- `AppSettings` JSON + RON roundtrip with a populated `spaces` map.
- `serialize_settings_to_ron` reparses.

Seeding (`vmux_space`, Bevy app/messages per AGENTS.md):
- reconcile seeds an id present in registry but missing from settings.
- reconcile leaves an existing entry (and its values) untouched.
- reconcile with nothing missing sends no `SettingsWriteRequest`.

URL updater (`vmux_space`):
- `EffectiveStartupUrl` reflects the active space's override and flips when
  `ActiveSpace` changes.

Dir wiring (`vmux_terminal`, message+system integration per AGENTS.md):
- send `LayoutSpawnRequest::Terminal`, run the schedule; with a per-space
  `startup_dir` configured, the spawned terminal's `TerminalLaunch.cwd` is that
  dir; with an invalid/unset dir it is `space_dir(id)` (today's value).

## Out of scope / follow-ups

- Spaces-page or settings-page UI for these fields.
- Pruning override entries on space delete.
- Per-space overrides for any other setting.
- Adopting `startup_dir` for the cwd paths that default to `$HOME` today
  (command-bar empty-path terminal via the `respond_terminal_*` responders, and
  agent `RunShell`) — would make the space default consistent everywhere, but is
  a separate behavior change.
