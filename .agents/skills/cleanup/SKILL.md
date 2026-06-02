---
name: cleanup
description: "Use when testing vmux (vmux_desktop / Vmux.app) from a clean slate. Wipes local profile data so it starts at first-run. Fixes startup crashes or blank/dark windows caused by stale space.ron, dead agent sessions, or old type paths from a different build sharing the same profile dir."
---

# Cleanup

Reset vmux's local state. All builds (released `Vmux.app` **and** `make dev`) share one data dir — `~/Library/Application Support/Vmux/` — so a space saved by one build can crash/blank another (stale type paths, dead agent sessions). These commands clear that.

Data layout:
- `profiles/personal/` — Chromium/CEF profile: cookies, login, caches, **`spaces/`** (the tab/pane/stack layout)
- `spaces.ron` — space index · `settings.ron` — app settings · `services/` — vmux_service state
- launchd daemons: `ai.vmux.service` (prod) + `ai.vmux.service.dev` (dev)

## Space reset (keep login + settings) — the common one

Use when the window starts **blank/dark** or **crashes on load** with `no registration found for …` or `Path not found: agent/…`. Clears only the saved layout; keeps cookies/login/settings.

```bash
bash -c '
pkill -f vmux_desktop 2>/dev/null; sleep 1
rm -rf ~/Library/"Application Support/Vmux"/profiles/*/spaces \
       ~/Library/"Application Support/Vmux"/spaces.ron
echo "spaces reset (login kept)"
'
```

Relaunch (`make dev`) → fresh default space, still logged in.

## Full fresh (first-run: logged out, default settings)

Quit everything, stop both daemons, wipe the whole data dir.

```bash
bash -c '
pkill -f vmux_desktop 2>/dev/null; pkill -f "Vmux.app" 2>/dev/null; pkill -f vmux_service 2>/dev/null; sleep 1
launchctl bootout "gui/$(id -u)/ai.vmux.service" 2>/dev/null
launchctl bootout "gui/$(id -u)/ai.vmux.service.dev" 2>/dev/null
rm -rf ~/Library/"Application Support/Vmux"
echo "full reset (first-run)"
'
```

Relaunch → first-run: fresh CEF profile (logged out), default settings, empty space. The app re-registers its launchd service on launch.

## Notes

- **Quit vmux first** — `pkill -f vmux_desktop` catches both the dev binary (`target/debug/vmux_desktop`) and the released `Vmux.app`. The space reset includes it.
- **Backups vs delete:** to keep a recovery copy instead of deleting, `mv` the file aside: `mv .../spaces/space-1/space.ron space.ron.bak`.
- **Mixed builds clobber each other:** running released `Vmux.app` and `make dev` against the shared dir is what produces stale-space crashes — full reset clears it.
- **CEF single-instance lock:** only one vmux can run per data dir; a second instance logs "Opening in existing browser session" and won't render. Quit the first.
