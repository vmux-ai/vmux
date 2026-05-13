# launchd `ensure_running` auto-heals stale plists

**Linear:** VMX-118
**Date:** 2026-05-13

## Problem

`vmux_service::launchd::ensure_running` (in `crates/vmux_service/src/launchd.rs:119-127`) only writes a fresh plist when none exists on disk. When the plist exists but its embedded values have drifted from current reality, `ensure_running` blindly bootstraps the stale plist.

Two real-world drift cases:

1. **Binary path drift** — plist points at a binary in a deleted worktree (e.g., `.worktrees/vmx-109/target/debug/vmux_service` after the worktree was removed).
2. **Env var rename** — plist embeds `VMUX_PROFILE` from before commit `2249492`; current builds expect `VMUX_BUILD_PROFILE`.

When the binary referenced by the plist no longer exists, `launchctl bootstrap` returns `Input/output error` (exit 5). The service never starts. Desktop CEF children sit until the 15s connection timeout fires:

```
Terminating current process after 15 seconds with no connection
```

Manual workaround today:

```bash
launchctl bootout gui/$(id -u)/ai.vmux.service.dev
rm ~/Library/LaunchAgents/ai.vmux.service.dev.plist
```

After that, the missing plist triggers `install()`, which writes fresh contents and bootstraps successfully.

## Goal

`ensure_running` self-heals. Stale plists are detected and rewritten on the next call. No manual `bootout`/`rm` step.

## Design

### Helper: `reconcile_plist_at`

New private function in `launchd.rs`:

```rust
fn reconcile_plist_at(
    plist: &Path,
    profile: &str,
    binary_path: &Path,
    log_path: &Path,
) -> std::io::Result<bool>
```

- Reads `plist` (treats `NotFound` as "no current contents").
- Computes desired contents via `generate_plist(profile, binary_path, log_path)`.
- If equal → returns `Ok(false)`, file untouched.
- Otherwise → ensures parent directory exists, writes desired contents, returns `Ok(true)`.

Pure relative to file IO at the supplied path. Takes `plist` and `log_path` as arguments so tests can drive it with `tempdir` paths without touching `~/Library/LaunchAgents` or `~/Library/Application Support/Vmux`.

A full string equality check covers the AC requirements (`ProgramArguments[0]`, `EnvironmentVariables`, `StandardOutPath`, `StandardErrorPath`) and is robust against any future field additions to the template.

### Rewrite: `ensure_running`

```rust
pub fn ensure_running(profile: &str, binary_path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(crate::service_dir())?;
    let plist = crate::plist_path(profile);
    let log = crate::log_path();
    let rewrote = reconcile_plist_at(&plist, profile, binary_path, &log)?;
    if rewrote {
        // Tear down any prior registration so bootstrap doesn't return
        // "Input/output error" when the previous binary path is gone or
        // the plist has changed under launchd.
        let _ = bootout(profile);
    }
    bootstrap(&plist)?;
    kickstart(profile)
}
```

The `bootout` is best-effort: when no service is registered, it returns nonzero (already logged at `warn`) and we proceed.

### Refactor: `install`

`install` continues to be the public API for "first-time install" (used by `vmux_service install` CLI subcommand). Reroute its plist-writing path through `reconcile_plist_at` so the write logic lives in one place.

```rust
pub fn install(profile: &str, binary_path: &Path) -> std::io::Result<PathBuf> {
    let plist = crate::plist_path(profile);
    std::fs::create_dir_all(crate::service_dir())?;
    let log = crate::log_path();
    let _rewrote = reconcile_plist_at(&plist, profile, binary_path, &log)?;
    bootstrap(&plist)?;
    Ok(plist)
}
```

### Tests

All in `crates/vmux_service/src/launchd.rs` `#[cfg(test)] mod tests`. Use `tempfile::tempdir` (already a workspace dev-dep — verify) so no system paths are touched and no `launchctl` calls are issued.

1. `reconcile_plist_at_writes_when_missing` — plist absent → returns `Ok(true)`, file now matches `generate_plist`.
2. `reconcile_plist_at_rewrites_when_binary_path_drifts` — pre-write a plist with `/old/bin/vmux_service`; call with `/new/bin/vmux_service` → returns `Ok(true)`, file now contains the new path. Regression for the issue's worktree-deletion scenario.
3. `reconcile_plist_at_rewrites_when_env_var_key_drifts` — pre-write a plist whose XML hard-codes `VMUX_PROFILE` (legacy) → returns `Ok(true)`, file now contains `VMUX_BUILD_PROFILE`. Regression for commit `2249492`.
4. `reconcile_plist_at_no_op_when_matching` — pre-write the current `generate_plist` output → returns `Ok(false)`, file mtime unchanged.

`ensure_running` itself is not unit-tested (it shells out to `launchctl`); the reconciler covers the plist-content invariant from the AC.

## Out of scope

- **Doctor surface.** AC #5 marks the `scripts/doctor-mac.sh` "stale plist detected" message as optional. The auto-heal is silent and runs on every `make dev`, so a doctor warning gives the user no actionable step (it would reconcile itself by the next launch). Skipped to avoid noise. Revisit if drift returns through a different path.
