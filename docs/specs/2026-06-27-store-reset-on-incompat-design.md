# Store reset on incompatible layout — design

Date: 2026-06-27
Status: approved
Crate: `vmux_desktop` (`src/persistence.rs`)

## Problem

After an in-place auto-update (e.g. 0.0.18 → 0.0.19), the app launches to a
blank layout: native menu bar present, page area empty. Settings and the
Chromium profile are intact; only the layout is gone, and it does not recover on
relaunch.

## Root cause

The layout scene is persisted to `store.ron` in `store_dir()` (the
profile-agnostic shared data dir) via `moonshine-save`. On startup
`load_space_on_startup` decides whether to load it.

When a release renames, removes, or moves a component type that appears in a
previously-saved `store.ron`, the stored scene references a type path the new
binary no longer registers. `moonshine-save`'s `load_on` (load.rs:329) hits
`LoadError::Scene(SceneSpawnError)`, logs `error!("load failed: …")`, and
**swallows the error** — the world is left empty.

Two existing facts then make the blank state permanent:

1. `load_space_on_startup` set `SpaceFilePresent(true)` because the file
   existed, so the fresh-default branch (`space_profile_bundle`) never runs — no
   default space is spawned.
2. The periodic/debounced `auto_save_system` then writes the empty world back to
   `store.ron`, clobbering the original. Relaunch now loads an empty (but valid)
   scene → still blank, forever.

`load_space_on_startup` already drops `store.ron` for two narrower cases
(`remove_stale_space_if_needed` for stale agent URLs / prompt-only empty URLs,
and a manual `STORE_SCHEMA_VERSION` bump). Neither covers a renamed/removed
component type, which is the actual break and is easy to forget to bump.

## Goals

- Recover automatically when the stored layout is incompatible with the running
  binary: drop it and start fresh, instead of showing a permanent blank.
- Preserve `settings.ron` and the Chromium profile.
- Never let an empty/failed world overwrite a good `store.ron`.

## Non-goals (explicit user choices)

- **Not** an unconditional wipe on every version change. Reset only when the
  stored scene is genuinely incompatible, so compatible updates keep the layout.
- **No** partial scene surgery to preserve in-layout vmux history. When a reset
  happens, the whole `store.ron` is dropped (vmux's own history layer resets with
  it; the Chromium profile's history is unaffected).
- No broad per-component migration framework.

## Design

Two focused changes in `vmux_desktop/src/persistence.rs`.

### 1. Pre-load registry validation (primary)

Detect incompatibility *before* loading, by checking the stored type paths
against the live `AppTypeRegistry`. This is deterministic and never attempts the
doomed load, so the clobber path is never entered.

- Add `registry: Res<AppTypeRegistry>` to `load_space_on_startup`.
- Add `space_has_unregistered_types(body: &str, registry: &TypeRegistry) -> bool`:
  extract every component/resource type path from the scene (the quoted
  `"crate::path::Type"` map keys under `resources:` and `components:`) and return
  `true` if any path is absent from the registry.
- Fold it into the existing pre-load drop logic alongside
  `remove_stale_space_if_needed`: if the body has unregistered types, delete
  `store.ron` and `store.version`, which routes startup into the existing
  fresh-default branch (`space_profile_bundle`). `warn!` what was dropped.

Type extraction reuses the line-oriented parsing style already present
(`page_metadata_urls`): match quoted keys containing `::`. It does not need a
full RON parse.

### 2. Empty-save guard (defense-in-depth)

Make a failed/empty world incapable of clobbering a good store, so any
incompatibility that ever slips past validation is recoverable on relaunch
rather than permanent.

- In `save_space_to_path` (or its caller), skip the save when the world contains
  no `Space` entity. A healthy layout always has at least one `Space`; zero means
  the world is degenerate (failed load or pre-spawn window) and must not be
  persisted.

## Testing

- `space_has_unregistered_types`:
  - returns `true` for a body containing a bogus key like
    `"vmux_desktop::does_not_exist::Ghost"`.
  - returns `false` for a body whose keys are all registered types.
- Empty-save guard: saving with no `Space` in the world leaves `store.ron`
  untouched (no write).
- Integration (App + observers, mirroring existing persistence tests): trigger a
  load against a `store.ron` containing an unregistered component path; assert the
  file is removed and exactly one fresh `Space` is spawned.

## Risks

- **False positive drop:** a valid type momentarily unregistered (e.g. a plugin
  not yet added at check time) would wipe layout. Mitigated by running the check
  in `load_space_on_startup`, which is ordered after plugin/registration setup,
  and by validating only against the same registry the loader will use.
- **Type-path extraction misses a key:** a missed key means a missed reset (fall
  back to old behavior), not a wrongful wipe. The empty-save guard still prevents
  a permanent blank.
