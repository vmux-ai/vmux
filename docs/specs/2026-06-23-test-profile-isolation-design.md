# Test profile isolation (`VMUX_PROFILE`) — design

- **Date:** 2026-06-23
- **Status:** Approved (pending spec review)
- **Branch:** `feat/agent-test-space` (extends the `vmux agent` seeded-test feature)

## Summary

Introduce a **runtime** profile selector, the `VMUX_PROFILE` env var (default `personal`), so test runs of the `vmux agent` harness are fully isolated from the user's real state. The `personal` profile resolves to today's exact paths (no migration); any other profile (e.g. `test`) redirects its spaces, session/spaces-list, recordings, CEF cache, and service socket under a per-profile location, and gets its own socket so a test instance can run **alongside** a normal vmux.

`VMUX_PROFILE` is orthogonal to the compile-time `VMUX_BUILD_PROFILE` (dev/release/local). The same binaries serve any runtime profile — no rebuild.

## Motivation

The seeded-agent test command (`vmux agent vibe -p "..."`) currently opens its throwaway space in the user's real `personal` profile: it pollutes the spaces list (`session.ron`), creates a `~/.vmux/spaces/space-N` dir, and writes screenshots to the shared `~/.vmux/recording`. A dedicated, disposable test profile keeps deterministic test runs from touching real state, and a separate socket lets the user keep their normal vmux open while testing.

## Path resolution (`vmux_core/src/profile.rs`)

`personal` keeps current paths exactly; other profiles redirect. `<build>` = `VMUX_BUILD_PROFILE`, `<p>` = sanitized `VMUX_PROFILE`.

| Resource | `personal` (unchanged) | other (`test`) |
|---|---|---|
| session / spaces-list | `…/Vmux/<build>/profiles/personal/session.ron` | `…/profiles/test/session.ron` (already via `active_profile_name`) |
| CEF cache | `…/profiles/personal` | `…/profiles/test` (already) |
| space dirs | `~/.vmux/spaces/<id>` | `~/.vmux/profiles/test/spaces/<id>` |
| recording | `~/.vmux/recording` | `~/.vmux/profiles/test/recording` |
| socket / pid / identity / log | `…/services/vmux-<build>.{sock,pid,identity}`, `…/logs/vmux-<build>.log` | `…/vmux-<build>-test.{…}`, `vmux-<build>-test.log` |

### Components changed

1. **`active_profile_name()`** — change return type `&'static str` → `String`; read `VMUX_PROFILE` (default `personal`) and pass through `sanitize_profile`. (`profile_dir().join(active_profile_name())` already accepts `String`.) Verify the re-export consumer `vmux_layout/src/profile.rs` still compiles with `String`.
2. **`sanitize_profile(raw: &str) -> String`** — lowercase; keep `[a-z0-9_-]`; collapse everything else; if empty, fall back to `personal`. Prevents path traversal / nested segments. Used by every profile-derived path.
3. **`spaces_root(home)`** — `personal` → `home/.vmux/spaces`; else `home/.vmux/profiles/<p>/spaces`. (`space_dir`, `default_space_dir`, rename/prune helpers all flow through `spaces_root`, so they isolate automatically.)
4. **`recording_dir()`** — `personal` → `config_dir()/recording`; else `config_dir()/profiles/<p>/recording`.
5. **`paths.rs::profile_file(ext)`** (vmux_service) — filename `vmux-<build>` → `vmux-<build>-<p>` when `<p> != personal`. Affects `socket_path`/`pid_path`/`identity_path`. Apply the same suffix in `log_path` (uses `log_dir`, not `service_dir`).
   - `paths.rs` reads the runtime profile via `vmux_core::profile::active_profile_name()` (sanitized) to build the suffix.

`personal` branches MUST produce byte-identical paths to today so existing installs are untouched.

## Activation

- **App (`make`)**: add `VMUX_PROFILE ?= personal` to the Makefile; the `dev` target's final `exec env …` passes `VMUX_PROFILE="$(VMUX_PROFILE)"` to `vmux_desktop`. So `make dev VMUX_PROFILE=test` launches a test instance. Add a `test:` target = `$(MAKE) dev VMUX_PROFILE=test`.
- **CLI (`vmux agent`)**: add `--profile <name>` as `Option<String>` (no clap `env` feature needed). In `commands/agent::run`, resolve `profile = flag.or_else(|| std::env::var("VMUX_PROFILE").ok()).filter(non-empty).unwrap_or("personal")`, then `std::env::set_var("VMUX_PROFILE", &profile)` **before** `ServiceConnection::connect()` so socket resolution targets the right instance. So `vmux agent --profile test vibe -p "…"` or `VMUX_PROFILE=test vmux agent vibe -p "…"` both hit the test instance; bare `vmux agent …` stays `personal`.

## Data flow

`make test` → app launched with `VMUX_PROFILE=test` → resolves `…-test` socket + `profiles/test/...` paths. `vmux agent --profile test vibe -p "…"` → CLI sets `VMUX_PROFILE=test` → connects to the `…-test` socket → opens the seeded space, which persists under `~/.vmux/profiles/test/spaces`; the inner vibe's screenshots land in `~/.vmux/profiles/test/recording`. Your `personal` vmux (if running) is untouched on its own socket.

## Testing strategy

Unit (`cargo test -p vmux_core`, `-p vmux_service`), env-guarded:
- `active_profile_name` defaults to `personal` when `VMUX_PROFILE` unset; returns sanitized value when set.
- `sanitize_profile`: `"test"`→`test`; `"../evil"`/`"a/b"`→ collapsed single segment; `""`→`personal`.
- `personal` resolves to the exact legacy paths for `spaces_root`, `recording_dir`, `profile_file`/`socket_path` (regression guard).
- `test` redirects `spaces_root`→`~/.vmux/profiles/test/spaces`, `recording_dir`→`~/.vmux/profiles/test/recording`, `socket_path` filename ends `-test.sock`.

CLI (`cargo test -p vmux_cli`): `agent --profile test` parses; default is `personal` when env unset.

Manual: `make test` + `vmux agent --profile test vibe -p "screenshot the space"` → space appears in a test instance; artifacts under `~/.vmux/profiles/test/`; personal `~/.vmux/spaces` and `~/.vmux/recording` unchanged.

## Out of scope
- Settings stay build-profile-based (shared across runtime profiles); tests don't pollute settings.
- No auto-cleanup of the test profile dir (it's a self-contained `~/.vmux/profiles/test/` you can delete).
- `vmux mcp` keeps reading `VMUX_PROFILE` from the environment only (no new flag).

## Concurrency note
Tests that set `VMUX_PROFILE` via `std::env::set_var` must serialize (shared process env) — reuse the existing `HomeEnvGuard`-style serialization pattern or a dedicated env mutex, mirroring how `HomeEnvGuard` tests already guard `HOME`.
