# Terminal: a PTY in the daemon, a grid in the page

> Part of the [Vmux Architecture](../architecture.md) overview. One of the
> [pages](pages.md) Vmux renders in a pane.

A Vmux terminal is split in two: the **shell runs in the background daemon**, and the **screen
renders in a web page**. The halves talk over the daemon's Unix socket — which is why a shell
survives the window closing (see **[Background Service](background-service.md)**).

## The backend: PTY + VTE, in the daemon

In `crates/vmux_service`, each terminal is a real OS pseudo-terminal:

- **[`portable_pty`](https://crates.io/crates/portable-pty)** opens the PTY and spawns the child
  shell; a dedicated reader thread pumps its output.
- **[`alacritty_terminal`](https://crates.io/crates/alacritty_terminal)** — the VTE engine behind
  Alacritty — advances that byte stream into an in-memory **cell grid**.
- On each poll the daemon **diffs the grid by row hash** and broadcasts only the **changed lines**
  as a `ViewportPatch` (or a full `Snapshot` for a freshly attached viewer) over a Tokio broadcast
  channel.

The daemon also owns the parts that should outlive any single frame: **OSC 133** command-lifecycle
tracking (so Vmux knows when a command starts and how it exits), per-shell **shell-integration**
injection (bash/zsh/fish/nu), tmux-style **copy-mode** motions, and title/bell events.

## The frontend: patches to a Dioxus grid

`crates/vmux_terminal` is the other half:

- A native Bevy plugin subscribes to the daemon and relays each `ViewportPatch` to the page as a
  zero-copy **rkyv** event.
- The `vmux://terminal` Dioxus app applies the patch to **per-row signals**, so only the lines that
  changed repaint — the same incremental model the daemon used to send them.
- Keystrokes flow back the other way: page → rkyv key event → plugin → `ProcessInput` → the daemon
  writes the PTY. Selection and `Cmd-C/V` round-trip through the system clipboard.

The result: the heavy, stateful work — a real shell, scrollback, command tracking — lives in a
durable process, while the UI is a thin, reactive view that can detach and re-attach at any time.
