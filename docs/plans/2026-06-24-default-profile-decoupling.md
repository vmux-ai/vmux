# Default-Profile Decoupling Implementation Plan

**Goal:** Remove the hardcoded `"personal"` behavioral sentinel — make the default profile a renamable identity, mark ephemeral test sessions with a `Tester` bot identity, and keep the layout store space-owned and profile-agnostic.

**Architecture:** A single env-derived predicate `is_test_session()` (`VMUX_TEST=1`) drives ephemerality (skip layout load/save), auto-approve, and which identity the bootstrap spawns (`Tester` vs `User`). The layout store moves to one profile-agnostic location; `profiles/<id>/` keeps only isolation state (cookies/socket/recordings). The default profile's storage id stays stable; its display name lives in config and is renamed via an MCP tool.

**Tech Stack:** Rust, Bevy ECS (`vmux_core`, `vmux_desktop`, `vmux_space`, `vmux_agent`, `vmux_mcp`), moonshine_save (`store.ron`), clap (CLI), Makefile.

**Prerequisite:** PR #145 merged to `main`; this branch rebased onto it. Verify these symbols exist before starting (they come from #145): `vmux_core::profile::{active_profile_name, store_dir, recording_dir, spaces_root_for, migrate_legacy_personal_layout}`, `vmux_desktop::persistence::{load_space_on_startup, save_space_to_path}`, `vmux_agent::client::cli::vibe::vibe_auto_approve_flag`, `vmux_agent::mcp::mcp_subcommand_args`, `vmux_core::team::{Profile, User, Agent}`.

---

## File map

- `crates/vmux_core/src/profile.rs` — add `is_test_session()`; make `store_dir`/spaces profile-agnostic; resolve default id; drop `== "personal"` from path logic.
- `crates/vmux_core/src/team.rs` — add `Tester` marker component.
- `crates/vmux_space/src/model.rs` / `spaces.rs` — bootstrap spawns `Tester` vs `User`.
- `crates/vmux_desktop/src/persistence.rs` — gate load/save on `is_test_session()`.
- `crates/vmux_agent/src/client/cli/vibe.rs` — auto-approve on `is_test_session()`.
- `crates/vmux_agent/src/mcp.rs` — always pass `--profile <id>`.
- `crates/vmux_setting` + `crates/vmux_core/src/profile.rs` — default display name in config.
- `crates/vmux_mcp/src/tools.rs` + `vmux_service`/`vmux_space` — `vmux_rename_profile` tool + handler.
- `Makefile` — `test-app` sets `VMUX_TEST=1`.

---

## Task 1: `is_test_session()` predicate

**Files:** Modify `crates/vmux_core/src/profile.rs`

- [ ] **Step 1: Failing test** (append to `profile.rs` tests)

```rust
#[test]
fn is_test_session_reads_env() {
    let prev = std::env::var("VMUX_TEST").ok();
    unsafe { std::env::set_var("VMUX_TEST", "1") };
    assert!(is_test_session());
    unsafe { std::env::remove_var("VMUX_TEST") };
    assert!(!is_test_session());
    if let Some(p) = prev { unsafe { std::env::set_var("VMUX_TEST", p) }; }
}
```

- [ ] **Step 2:** `cargo test -p vmux_core is_test_session_reads_env` → FAIL (undefined).
- [ ] **Step 3: Implement**

```rust
pub fn is_test_session() -> bool {
    matches!(
        std::env::var("VMUX_TEST").ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}
```

- [ ] **Step 4:** `cargo test -p vmux_core is_test_session_reads_env` → PASS.
- [ ] **Step 5: Commit** `feat(core): is_test_session() predicate from VMUX_TEST`

---

## Task 2: Profile-agnostic layout store location

**Files:** Modify `crates/vmux_core/src/profile.rs` (`store_dir_for`/`store_dir`)

Layout must not live under `profiles/<id>/`. `store.ron` goes at the shared-data-dir base for every profile.

- [ ] **Step 1: Failing test**

```rust
#[test]
fn store_dir_is_profile_agnostic_base() {
    let base = std::path::Path::new("/data/Vmux/dev");
    assert_eq!(store_dir_for(base, "personal"), PathBuf::from("/data/Vmux/dev"));
    assert_eq!(store_dir_for(base, "gregor"), PathBuf::from("/data/Vmux/dev"));
}
```

- [ ] **Step 2:** `cargo test -p vmux_core store_dir_is_profile_agnostic_base` → FAIL (gregor still nests).
- [ ] **Step 3: Implement** — drop the profile branch:

```rust
fn store_dir_for(base: &std::path::Path, _profile: &str) -> PathBuf {
    base.to_path_buf()
}
```

- [ ] **Step 4:** Update/replace the old `store_dir_personal_is_base_and_test_is_nested` test to the new behavior; `cargo test -p vmux_core` → PASS.
- [ ] **Step 5: Commit** `refactor(core): layout store is profile-agnostic (base dir)`

---

## Task 3: Profile-agnostic spaces dir + migration

**Files:** Modify `crates/vmux_core/src/profile.rs` (`spaces_root_for`, `migrate_legacy_personal_layout_in`)

Spaces are layout → one profile-agnostic root (mirror the store). Keep recordings per-profile.

- [ ] **Step 1: Failing test**

```rust
#[test]
fn spaces_root_is_profile_agnostic() {
    let home = std::path::Path::new("/home/u");
    assert_eq!(spaces_root_for(home, "personal"), PathBuf::from("/home/u/.vmux/spaces"));
    assert_eq!(spaces_root_for(home, "gregor"), PathBuf::from("/home/u/.vmux/spaces"));
}
```

- [ ] **Step 2:** `cargo test -p vmux_core spaces_root_is_profile_agnostic` → FAIL.
- [ ] **Step 3: Implement** — drop the profile segment:

```rust
fn spaces_root_for(home: &std::path::Path, _profile: &str) -> PathBuf {
    home.join(".vmux").join("spaces")
}
```

- [ ] **Step 4:** Extend `migrate_legacy_personal_layout_in` to move any `profiles/<p>/spaces` left by #145 back to `~/.vmux/spaces` (guarded: skip if target exists). Update the per-profile-spaces tests to the agnostic path.
- [ ] **Step 5:** `cargo test -p vmux_core` → PASS.
- [ ] **Step 6: Commit** `refactor(core): spaces dir is profile-agnostic + migrate`

---

## Task 4: `Tester` identity + bootstrap

**Files:** Modify `crates/vmux_core/src/team.rs`, `crates/vmux_space/src/spaces.rs` (`space_profile_bundle`)

- [ ] **Step 1: Failing test** (`vmux_space` spaces tests) — assert a test session bootstraps `Tester`, a normal one `User`:

```rust
#[test]
fn bootstrap_spawns_tester_under_test_session() {
    let prev = std::env::var("VMUX_TEST").ok();
    unsafe { std::env::set_var("VMUX_TEST", "1") };
    let mut app = App::new();
    app.world_mut().spawn(space_profile_bundle(&bootstrap_space_record()));
    let mut q = app.world_mut().query_filtered::<(), With<vmux_core::team::Tester>>();
    assert_eq!(q.iter(app.world()).count(), 1);
    let mut u = app.world_mut().query_filtered::<(), With<vmux_core::team::User>>();
    assert_eq!(u.iter(app.world()).count(), 0);
    unsafe { std::env::remove_var("VMUX_TEST") };
    if let Some(p) = prev { unsafe { std::env::set_var("VMUX_TEST", p) }; }
}
```

- [ ] **Step 2:** `cargo test -p vmux_space bootstrap_spawns_tester_under_test_session` → FAIL.
- [ ] **Step 3: Implement** — add the marker + branch the bundle:

```rust
// team.rs
#[derive(Component, Clone, Copy, Debug)]
pub struct Tester;
```

In `space_profile_bundle`, replace the unconditional `User` with: spawn `Tester` when `vmux_core::profile::is_test_session()`, else `User`. (Keep `team::Profile { name, avatar }` on both.)

- [ ] **Step 4:** `cargo test -p vmux_space` → PASS.
- [ ] **Step 5: Commit** `feat(team): Tester bot identity for test sessions`

---

## Task 5: Persistence gates on `is_test_session()`

**Files:** Modify `crates/vmux_desktop/src/persistence.rs` (`load_space_on_startup`, `save_space_to_path`)

- [ ] **Step 1:** Replace both `active_profile_name() != "personal"` guards with `vmux_core::profile::is_test_session()`.

```rust
// in both functions
if vmux_core::profile::is_test_session() {
    // existing early-return/skip body
}
```

- [ ] **Step 2: Test** — `crates/vmux_desktop/tests/` add an integration check (or unit on a helper) asserting `is_test_session()` short-circuits save. Run `cargo test -p vmux_desktop persistence` (or the new test) → PASS.
- [ ] **Step 3: Commit** `refactor(desktop): persistence skips on is_test_session, not name`

---

## Task 6: Auto-approve gates on `is_test_session()`

**Files:** Modify `crates/vmux_agent/src/client/cli/vibe.rs`

- [ ] **Step 1: Failing test** — replace `auto_approve_flag_only_for_non_personal_profile`:

```rust
#[test]
fn auto_approve_flag_follows_test_session() {
    let prev = std::env::var("VMUX_TEST").ok();
    unsafe { std::env::set_var("VMUX_TEST", "1") };
    assert!(VibeStrategy.build_args(&mcp(), None).iter().any(|a| a == "--auto-approve"));
    unsafe { std::env::remove_var("VMUX_TEST") };
    assert!(!VibeStrategy.build_args(&mcp(), None).iter().any(|a| a == "--auto-approve"));
    if let Some(p) = prev { unsafe { std::env::set_var("VMUX_TEST", p) }; }
}
```
(Add a small `fn mcp() -> McpServerConfig` test helper if absent.)

- [ ] **Step 2:** Run it → FAIL.
- [ ] **Step 3: Implement** — `build_args` appends `--auto-approve` when `vmux_core::profile::is_test_session()`; delete `vibe_auto_approve_flag`.
- [ ] **Step 4:** `cargo test -p vmux_agent` → PASS.
- [ ] **Step 5: Commit** `refactor(agent): vibe auto-approve on is_test_session`

---

## Task 7: Always propagate `--profile`

**Files:** Modify `crates/vmux_agent/src/mcp.rs` (`mcp_subcommand_args`)

- [ ] **Step 1: Failing test** — update `mcp_args_append_profile_only_for_non_personal` to expect `--profile` for every id:

```rust
#[test]
fn mcp_args_always_append_profile() {
    let a = ProcessId::new();
    let args = mcp_subcommand_args(a, "personal");
    assert!(args.windows(2).any(|w| w[0] == "--profile" && w[1] == "personal"));
}
```

- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3: Implement** — drop the `if profile != "personal"` guard; always push `--profile`, `profile`.
- [ ] **Step 4:** `cargo test -p vmux_agent` → PASS.
- [ ] **Step 5: Commit** `refactor(agent): always pass --profile to spawned mcp`

---

## Task 8: Default display name in config

**Files:** Modify `crates/vmux_setting` (settings struct) + `crates/vmux_core/src/profile.rs` + `crates/vmux_space/src/model.rs` (`bootstrap_profile_name`)

- [ ] **Step 1: Failing test** — display name falls back to capitalized id, override from config:

```rust
#[test]
fn display_name_defaults_to_capitalized_id_or_config() {
    assert_eq!(display_name_for("personal", None), "Personal");
    assert_eq!(display_name_for("personal", Some("Junichi")), "Junichi");
}
```

- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3: Implement** — add `profile_display_name: Option<String>` to settings; `fn display_name_for(id: &str, configured: Option<&str>) -> String` (configured or capitalize(id)); have `bootstrap_profile_name` read settings then fall back. Keep `"personal"` only as the id seed.
- [ ] **Step 4:** `cargo test -p vmux_core -p vmux_space` → PASS.
- [ ] **Step 5: Commit** `feat(core): default profile display name from config`

---

## Task 9: `vmux_rename_profile` MCP tool

**Files:** Modify `crates/vmux_mcp/src/tools.rs`, `crates/vmux_service/src/protocol.rs` (`AgentCommand`), `crates/vmux_space/src/plugin.rs` (handler)

- [ ] **Step 1: Failing test** (`tools.rs`) — dispatch maps to the command:

```rust
#[test]
fn rename_profile_dispatches() {
    let c = dispatch_command("rename_profile", serde_json::json!({"name": "Junichi"})).unwrap();
    assert!(matches!(c, AgentCommand::RenameProfile { name } if name == "Junichi"));
}
```

- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3: Implement**
  - `McpParamTool::RenameProfile { name: String }` (+ description) → `AgentCommand::RenameProfile { name }` (reject empty).
  - `AgentCommand::RenameProfile { name }` in `vmux_service::protocol` (+ rkyv + validate non-empty).
  - Handler (vmux_space plugin): write the settings update (`profile_display_name = name`) and update the live `team::Profile { name, avatar }` on the `User`/`Tester` identity (recompute avatar initials). No directory moves.
- [ ] **Step 4:** `cargo test -p vmux_mcp -p vmux_service -p vmux_space` → PASS.
- [ ] **Step 5: Commit** `feat(mcp): vmux_rename_profile (display-name only)`

---

## Task 10: `make test-app` sets `VMUX_TEST=1`

**Files:** Modify `Makefile`, `crates/vmux_desktop/tests/release_invariants.rs`

- [ ] **Step 1:** Change `test-app:` to `$(MAKE) dev VMUX_PROFILE=gregor VMUX_TEST=1`, and ensure the `dev` exec line forwards `VMUX_TEST="$(VMUX_TEST)"` (with `VMUX_TEST ?=` default empty at the top).
- [ ] **Step 2:** Add an invariant assertion in `release_invariants.rs` that `test-app` passes `VMUX_TEST=1`. `cargo test -p vmux_desktop --test release_invariants` → PASS.
- [ ] **Step 3: Commit** `build: test-app sets VMUX_TEST=1`

---

## Task 11: Sentinel sweep + full check

**Files:** repo-wide

- [ ] **Step 1:** `grep -rn '"personal"' crates/ | grep -v test` — confirm the only remaining uses are the **id seed** (`sanitize_profile` empty→"personal", default-id resolution). No behavioral branches remain.
- [ ] **Step 2:** `cargo fmt`; `git checkout -- patches/`.
- [ ] **Step 3:** `cargo test -p vmux_core -p vmux_service -p vmux_mcp -p vmux_terminal -p vmux_agent -p vmux_cli -p vmux_space` and `cargo check -p vmux_desktop` → all green.
- [ ] **Step 4: Commit** `chore: drop residual personal sentinels`

---

## Self-review notes
- **Spec coverage:** §1 layout-agnostic → Tasks 2-3; §2 Tester identity → Task 4; §3 is_test_session behavior → Tasks 1,5,6,10; §4 default id+display name → Task 8; §5 isolation unchanged → (no task; recordings/cef untouched); §6 rename → Task 9; §7 sentinel removal → Tasks 2,5,6,7,11.
- **Test signal:** `VMUX_TEST` truthy = `1|true|yes` (Task 1) — used consistently in Tasks 5,6,10.
- **No dir moves on rename** (Task 9) — matches spec decision.
