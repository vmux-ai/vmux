---
name: cleanup
description: "Use when testing vmux (vmux_desktop / Vmux.app) from a clean slate. Resets saved layout state that can restore dead sessions, missing files, or stale component types."
---

# Cleanup

Local and release builds share `~/Library/Application Support/Vmux/`. Dev builds use `~/Library/Application Support/Vmux/dev/`.

Data layout:

- `store.ron` — local/release tab, pane, stack, history, and window state
- `dev/store.ron` — dev tab, pane, stack, history, and window state
- `profiles/personal/` and `dev/profiles/personal/` — Chromium/CEF cookies, login, and caches
- `~/.vmux/settings.ron` — shared app settings
- `services/` — vmux_service state

## Layout reset

For `make dev`:

```bash
make cleanup
```

For `make local` or installed/released Vmux:

```bash
make cleanup-local
```

Both keep browser profiles and settings. `cleanup-local` backs up the old layout as `store.ron.cleanup-<timestamp>`.

## Full fresh

Quit everything, stop daemons, and wipe all Vmux application data:

```bash
bash -c '
pkill -f vmux_desktop 2>/dev/null; pkill -f "Vmux.app" 2>/dev/null; pkill -f vmux_service 2>/dev/null; sleep 1
launchctl bootout "gui/$(id -u)/ai.vmux.service" 2>/dev/null
launchctl bootout "gui/$(id -u)/ai.vmux.service.dev" 2>/dev/null
launchctl list 2>/dev/null | awk '$3 ~ /^ai\.vmux\.service\./ {print $3}' | while IFS= read -r label; do
  launchctl bootout "gui/$(id -u)/$label" 2>/dev/null
done
rm -rf ~/Library/"Application Support/Vmux"
echo "full reset (first-run)"
'
```

Relaunch to create a fresh profile and default layout.

## Notes

- Use the matching target. `make cleanup` resets dev only. `make cleanup-local` resets the store shared by local and release builds.
- A second app instance using the same CEF profile will not render. Stop the first instance before relaunching.
