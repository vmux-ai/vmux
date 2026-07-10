# CEF cookie-encryption keychain persistence

Status: implemented · 2026-07-08 (revised 2026-07-12)

## Problem

Website logins vanish after "every update." Cookies are not deleted — they become
undecryptable.

## Root cause

On macOS, CEF/Chromium encrypts saved cookies and passwords (`v10` records) with a
key stored in the login **Keychain**, not in the profile. vmux's cookie DB and its
`root_cache_path` are stable across updates:

```
~/Library/Application Support/Vmux/profiles/<profile>/Default/Cookies   (persists fine)
```

The key lives in the Keychain item **`Chromium Safe Storage`** (account `Chromium`),
shared by every CEF app that does not set a product name. vmux never sets one —
`CefSettings` (`_cef_settings_t`) has **no** field for it, and CEF exposes no override
(Bitbucket #2692 is an unresolved feature request; the framework is a prebuilt binary
we cannot patch). So the name is the framework default.

Access to that item is gated by the requesting binary's **code-signing identity** via
the item's ACL. The key itself is stable (created once, never rewritten), so the loss
is at *decrypt time*: when a binary whose identity is not "Always Allow"-trusted asks
for the key, macOS prompts; the browser process performs OSCrypt and, if the prompt is
missed/denied (or the binary's designated requirement no longer matches), the key is
unreadable and every login appears gone.

What breaks the trust:

- **Ad-hoc / unstable signatures.** An ad-hoc binary's designated requirement is
  pinned to its exact code hash, so it changes on every rebuild and re-prompts. A plain
  `cargo build` / IDE-run `target/debug/vmux_desktop` is ad-hoc (`flags=…adhoc`).
- **Throwaway test runs** (`VMUX_TEST`, e.g. `make test-app` with the `gregor` profile)
  touching the shared item under a disposable identity churn its ACL.
- Historically, an early ad-hoc build created the item (2026-04-29), leaving a stale ACL.

Interactive, stably-signed builds do persist across updates once "Always Allow"-ed:
`release`/`local` are Developer-ID signed; `dev` is signed by `make dev` with the reused
self-signed `Vmux Dev` certificate. A cert-signed binary's designated requirement is
pinned to the *certificate*, not the code hash, so it survives rebuilds.

Not the cause (ruled out): version-keyed paths, data wiped on update, empty `os_crypt`
in `Local State` (normal on macOS — the key is in the Keychain), and the
`process_requirement.cc -67030` log line (a benign CEF self-validation warning).

## Constraints

- Prebuilt CEF: cannot rename `Chromium Safe Storage` → a dedicated `Vmux Safe Storage`.
- `release` and `local` **share one data dir** (`Vmux`), so they must use the *same*
  cookie-encryption scheme. `dev` has its own dir (`Vmux/dev`). The Keychain item is
  global (one key per user, shared by all profiles — the normal Chromium model).
- Safe at-rest encryption must be preserved for every profile a real account logs into,
  including `dev`.

## Decision

Encrypt cookies with the real Keychain for all interactive use; use a mock keychain only
for automated test runs.

| Session                    | Keychain              | Rationale |
|----------------------------|-----------------------|-----------|
| `release` / `local` (interactive) | real (secure) | Developer-ID DR is stable → persists across updates after one "Always Allow". |
| `dev` (interactive, `make dev`)   | real (secure) | Signed with the stable `Vmux Dev` cert → cert-pinned DR survives rebuilds; safe for real logins (e.g. testing a Google sign-in). |
| any profile with `VMUX_TEST` set  | `--use-mock-keychain` | Headless (no one to approve the prompt) and disposable; keeps throwaway/ad-hoc test identities off the shared item so they can't churn the ACL real logins depend on. |

Mock keychain is deliberately **not** used for any interactive build: its key is a public
constant, i.e. cookies would be effectively unencrypted at rest.

## Implementation

- `vmux_core::profile::cef_keychain_switches()` → `["use-mock-keychain"]` when
  `is_test_session()` (i.e. `VMUX_TEST`), else `[]` (pure
  `cef_keychain_switches_for(is_test_session: bool)` + unit tests, no CEF build).
- `vmux_browser::BrowserPlugin` builds `CommandLineConfig` from those switches and
  passes it as `CefPlugin.command_line_config`. Applied in the browser process via
  `OnBeforeCommandLineProcessing`; the browser process obtains the OSCrypt key and
  forwards it to the network service, so no child-process propagation is needed.

## Operational requirements

- **Always launch `dev` via `make dev`** (signs with `Vmux Dev`). A bare `cargo run` /
  IDE-debug binary is ad-hoc and will re-prompt / lose the key on every rebuild.
- **One dev identity.** Keep a single self-signed dev cert (`Vmux Dev`); delete stale
  duplicates (e.g. `Vmux Development`) so `make dev` signing is consistent.

## One-time reset (existing installs)

The current `Chromium Safe Storage` ACL is poisoned by mixed historical identities.
Once, after installing a build with this change:

1. Quit vmux.
2. Delete the item: `security delete-generic-password -s "Chromium Safe Storage"`.
3. Relaunch each interactive build you use and click **Always Allow** on the prompt —
   once for the Developer-ID identity (`release`/`local`) and once for `Vmux Dev`
   (`dev`). Existing cookies encrypted with the old key are lost once; logins persist
   across updates thereafter.
