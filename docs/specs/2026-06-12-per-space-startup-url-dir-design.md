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
  startup_url resolution; auto-seeding/auto-creating per-space entries (the map
  stays minimal — absent ⇒ global fallback).

## Decisions (resolved during brainstorming)

1. `startup_dir` governs **terminal + agent cwd**.
2. Resolution chain is **per-space → global → built-in** for both values.
3. Per-space overrides live in **`settings.ron`** under a `spaces` map (not in
   `spaces.ron`/`SpaceRecord`).
4. The `spaces` map is **optional and never auto-populated**. A space not listed
   resolves to the global fallback. Config files stay minimal/hand-curated — no
   reconcile/seed system writes empty entries. (Reversed an earlier
   seed-from-registry decision after it polluted `settings.ron` with an empty
   block per space.)
5. Global fallbacks: url → `browser.startup_url`, dir → **`terminal.startup_dir`**.
   The agent edits these globals (and existing/whole `spaces`) via the existing
   `update_settings` tool; creating a brand-new per-space entry granularly is not
   supported (humans hand-edit, or the agent replaces the whole `spaces` value).

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

// BrowserSettings.startup_url unchanged (existing global).
// Global startup_dir lives on TerminalSettings (the terminal/agent concern):
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TerminalSettings {
    // … existing fields …
    #[serde(default)]
    pub startup_dir: Option<String>, // NEW global
}
// TerminalSettings gains `impl Default` so tests/agents can build it ergonomically.
```

- `BTreeMap` for stable serialization order; map is empty by default.
- `SpaceOverrides` fields use plain `Option` (serialize as `null` when present),
  but the map is never auto-seeded — an entry exists only if a human/agent wrote it.
- Global `startup_dir` lives on `TerminalSettings` (not `browser`) since it is a
  terminal/agent concern. `terminal` is `Some(...)` in the shipped `settings.ron`,
  so the agent path `terminal.startup_dir` is reachable.

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
    // `pick` applies trim + non-empty + is_dir at EACH tier, so an invalid
    // higher tier cascades to a valid lower one.
    let pick = |opt: Option<&str>| { /* trim, non-empty, PathBuf, is_dir */ };
    pick(settings.spaces.get(space_id).and_then(|o| o.startup_dir.as_deref()))
        .or_else(|| pick(settings.terminal.as_ref().and_then(|t| t.startup_dir.as_deref())))
        .unwrap_or_else(|| vmux_core::profile::space_dir(space_id)) // built-in ~/.vmux/<id>, auto-created
}
```

- `resolve_startup_url` gains a `space_id` param (existing legacy
  `vmux://agent` → google preserved). All call sites pass the active space id.
- `resolve_startup_dir` cascades tiers, validating `is_dir` at each; startup
  never errors. Built-in tier auto-creates `~/.vmux/<space>/`.

## No seeding

`set_at_path` rejects unknown paths, so the agent cannot create a brand-new
`spaces.<id>` granularly. We accept that rather than seed: the `spaces` map is
left empty and resolvers fall back to the global tier when an id is absent. The
agent can still edit globals (`browser.startup_url`, `terminal.startup_dir`) and
can replace the whole `spaces` value (the `spaces` key always exists); humans
hand-edit `settings.ron` to add a per-space override. This keeps config files
minimal — no empty `{startup_url: None, startup_dir: None}` block per space.

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
- Per-space dir set but non-existent / not a dir → cascades to global
  (`terminal.startup_dir`), then built-in `~/.vmux/<id>`.
- A space absent from `spaces` → resolves entirely from the global tier.
- Agent sets `spaces.<unknown-id>.startup_url` → `set_at_path` returns
  "unknown setting path" (no such entry; add it by hand or set the whole map).
- Active space switches → `EffectiveStartupUrl` recomputes; already-open stacks
  keep their content (only new opens use the new value), matching today.

## Testing

Resolver (`vmux_setting`):
- url: per-space wins; falls to global; falls to google; blank per-space →
  global; legacy `vmux://agent` → google.
- dir: per-space wins; cascades to global (`terminal.startup_dir`); falls to
  `space_dir(id)`; invalid per-space cascades to a valid global; all-invalid →
  built-in.
- `AppSettings` RON roundtrip with a populated `spaces` map.

URL updater (`vmux_space`):
- `EffectiveStartupUrl` reflects the active space's override and flips when
  `ActiveSpace` changes.

Dir wiring (`vmux_terminal`, message+system integration per AGENTS.md):
- drive `handle_terminal_page_open` with an explicit `ActiveSpace` (hermetic —
  do not rely on `ActiveSpace::default()` reading the on-disk registry); with a
  per-space `startup_dir` configured the spawned terminal's `TerminalLaunch.cwd`
  is that dir.

## Out of scope / follow-ups

- Spaces-page or settings-page UI for these fields.
- Granular agent-create of a new `spaces.<id>` entry (would need `set_at_path`
  auto-vivification for the `spaces` map only) — deferred to keep config minimal.
- Per-space overrides for any other setting.
- Adopting `startup_dir` for the cwd paths that default to `$HOME` today
  (command-bar empty-path terminal via the `respond_terminal_*` responders, and
  agent `RunShell`) — would make the space default consistent everywhere, but is
  a separate behavior change.
