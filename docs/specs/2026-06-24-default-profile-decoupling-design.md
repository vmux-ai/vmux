# Default-Profile Decoupling Design

**Goal:** Stop treating the literal string `"personal"` as a behavioral sentinel. The default profile becomes a renamable identity; ephemeral test sessions are marked explicitly in the entity model; the layout store stays space-owned and profile-agnostic.

**Depends on:** PR #145 (VMUX_PROFILE isolation). Implement on top of merged main.

---

## Background (current state after #145)

`VMUX_PROFILE` selects a runtime isolation profile. `"personal"` is both the default value **and** a behavioral sentinel in several places:

- `profile.rs::store_dir_for` — `personal` → base dir, else `profiles/<p>/`
- `persistence.rs` (`load_space_on_startup`, `save_space_to_path`) — non-personal skips store load/save (ephemeral)
- `vibe.rs::vibe_auto_approve_flag` — non-personal → `--auto-approve`
- `mcp.rs::mcp_subcommand_args` — non-personal → append `--profile`
- `profile.rs::migrate_legacy_personal_layout` — runs only for `personal`
- `model.rs::bootstrap_profile_name` — identity derived from the profile name

Two distinct concepts get conflated under "profile":

1. **Isolation profile** (`VMUX_PROFILE`) — which `profiles/<p>/` dir holds cookies, socket, recordings.
2. **Team attribution** (`team::Profile { name, avatar }` on a space / user) — *who* owns a space, shown in the facepile.

**The space owns its layout.** The team profile is just a label inside the layout; it must not shard the store.

---

## Design

### 1. Layout is space-owned and profile-agnostic
The layout store (`store.ron`) and per-space working dirs live in **one** profile-agnostic location under the shared data dir (e.g. `…/Vmux/<build>/store.ron` and `…/Vmux/<build>/spaces/`), **not** under `profiles/<p>/`. Spaces with any team attribution coexist in this single store. (This relocates the layout that #145 nested per-profile; recordings stay per-profile — see §5.)

### 2. Test identity is a bot (`Tester`), not a `User`
A normal instance bootstraps a `User` (the human). A **test session bootstraps its identity with `vmux_core::team::Tester` instead of `User`** — an automated QA persona ("Gregor"), not a human — so the facepile renders it as a bot, not the human "You". The pane-agent type `Agent { sid, kind: AgentKind }` stays reserved for vibe/claude/codex and is unchanged.

### 3. `is_test_session()` drives identity + behavior
`vmux_core::profile::is_test_session() -> bool` reads `VMUX_TEST` (truthy env). `make test-app` sets `VMUX_TEST=1` alongside `VMUX_PROFILE=gregor`. It drives:

- **Identity:** bootstrap spawns the `Tester` identity instead of `User`.
- **Fresh layout, never saved:** `load_space_on_startup` skips load and spawns a fresh bootstrap space; `save_space_to_path` returns without writing. Replaces every `active_profile_name() != "personal"` persistence check.
- **Auto-approve:** vibe `build_args` adds `--auto-approve`.

Non-ECS launch code (vibe `build_args`) calls `is_test_session()` directly; ECS code can also query `With<Tester>` on the active identity.

### 4. Profile = stable id + renamable display name
- **Storage id** = `VMUX_PROFILE` (default `"personal"`), names the `profiles/<id>/` isolation dir. Stable; never moved by rename.
- **Display name** lives in config (settings), seeded from the id (`"personal"` → "Personal") on first run. This is what the facepile pill shows.
- `active_profile_name()` = `VMUX_PROFILE` if set, else the default id `"personal"`. `"personal"` survives **only** as this seed default — no behavior branches on it.

### 5. Isolation stays per id
CEF cache (cookies/login), service socket/pid/identity/log, and recordings remain under `profiles/<id>/` (per `VMUX_PROFILE`). Unchanged from #145.

### 6. Rename via MCP (display-name only)
`vmux_rename_profile { name }` tool:
- Update the stored display name in config and the live `team::Profile { name, avatar }` on the user entity (avatar initials recomputed).
- **No directory moves** — the storage id and all `profiles/<id>/` data are untouched, so there is no live-file/CEF-lock risk and rename is instant.

Rationale: the user asked for a name that "can change at any moment." Decoupling the display name from the storage id makes rename a cheap, safe metadata update.

### 7. Remove `"personal"` sentinels
Replace all `== "personal"` / `!= "personal"` behavioral branches with `is_test_session()` (auto-approve), the `Tester` marker (persistence), and always-pass-the-id (`--profile`). `store_dir_for` no longer special-cases `personal`.

---

## Migration
- Relocate the layout (`store.ron` + `spaces/`) to the profile-agnostic location if #145 left it under `profiles/<p>/`. One-shot move on first run, guarded (skip if the target exists).
- Recordings + CEF cache stay per-profile; no move.

## Testing
- **Unit:** `is_test_session()` truthiness; default-id resolution when `VMUX_PROFILE` unset; display-name seed from id; rename updates config + `team::Profile`.
- **ECS:** a test session bootstraps `Tester` (not `User`); `load`/`save` systems skip under `is_test_session()`; a normal session bootstraps `User` and persists.
- **Integration:** a test session boots a fresh space and never writes the store; the default session persists; `vmux_rename_profile` changes the pill without moving any dir.

## Decisions locked (call out at review)
- **Test identity = `Tester` (bot), not `User`** — `Agent { kind }` stays reserved for pane agents (vibe/claude/codex).
- **Rename = display-name only** (storage id stable). If full storage rename is wanted, that's a larger follow-up.
- **Layout location = shared data dir base** (profile-agnostic), not `profiles/<p>/`, not top-level `~/.vmux`.
- **Test signal = `VMUX_TEST=1` env** driving the `Tester` identity + fresh-layout behavior (not a name check).
