# Tmux(1) commands vs vmux

## Sources

- **Full command set**: [OpenBSD tmux(1)](https://man.openbsd.org/tmux.1) (fetched March 2026). Listed here are **primary client commands** (sections *Clients and sessions*, *Windows and panes*, *Key bindings*, *Options*, *Hooks*, *Global and session environment*, *Status line*, *Buffers*, *Miscellaneous*).
- **Default keys**: Same manual, *DEFAULT KEY BINDINGS* (prefix **C-b** unless changed). Shown as **prefix + key** → usual bound command.
- **Vmux bindings**: `[crates/vmux_settings/src/bindings.rs](crates/vmux_settings/src/bindings.rs)` (`tmux_preset`, `vim_preset`), `[crates/vmux_layout/src/pane_ops.rs](crates/vmux_layout/src/pane_ops.rs)` (`split_active_pane`, `cycle_pane_focus`), `[crates/vmux_input/src/system.rs](crates/vmux_input/src/system.rs)` (`ctrl_arrow_focus_commands`).
- **Copy-mode**: All **92** copy-mode command names from the manual are listed in their own table (below *Windows and panes*).
- **Aliases**: All **75** `(alias:…)` short names from the same manual pass are listed; behavior matches the primary command.
- **Vmux reality**: Single GUI app with **hierarchical pane layout** and **CEF webviews**, not a terminal multiplexer server. “**Yes**” means a close analogue exists; “**Partial**” means subset or different model; “**No**” means outside current scope.

## Legend


| Status      | Meaning                                                                |
| ----------- | ---------------------------------------------------------------------- |
| **Yes**     | Comparable behavior in vmux today                                      |
| **Partial** | Subset, different target (e.g. webview vs pty), or rough analogue only |
| **No**      | Not implemented / not applicable to vmux’s model                       |


---

## Clients and sessions


| Command          | Vmux |
| ---------------- | ---- |
| `attach-session` | No   |
| `detach-client`  | No   |
| `has-session`    | No   |
| `kill-server`    | No   |
| `kill-session`   | No   |
| `list-clients`   | No   |
| `list-commands`  | No   |
| `list-sessions`  | No   |
| `lock-client`    | No   |
| `lock-session`   | No   |
| `new-session`    | No   |
| `refresh-client` | No   |
| `rename-session` | No   |
| `server-access`  | No   |
| `show-messages`  | No   |
| `source-file`    | No   |
| `start-server`   | No   |
| `suspend-client` | No   |
| `switch-client`  | No   |


---

## Windows and panes


| Command           | Vmux    | Notes                                                                                  |
| ----------------- | ------- | -------------------------------------------------------------------------------------- |
| `break-pane`      | No      |                                                                                        |
| `capture-pane`    | No      |                                                                                        |
| `choose-client`   | No      |                                                                                        |
| `choose-tree`     | No      |                                                                                        |
| `copy-mode`       | No      | Parent of **92** subcommands listed in [Copy-mode subcommands](#copy-mode-subcommands) |
| `customize-mode`  | No      |                                                                                        |
| `display-panes`   | No      |                                                                                        |
| `find-window`     | Partial | Command palette / search is a loose analogue, not content search across panes          |
| `join-pane`       | No      |                                                                                        |
| `kill-pane`       | Yes     | `try_kill_active_pane`                                                                 |
| `kill-window`     | No      |                                                                                        |
| `last-pane`       | No      |                                                                                        |
| `last-window`     | No      |                                                                                        |
| `link-window`     | No      |                                                                                        |
| `list-panes`      | No      |                                                                                        |
| `list-windows`    | No      |                                                                                        |
| `move-pane`       | No      |                                                                                        |
| `move-window`     | No      |                                                                                        |
| `new-window`      | No      | New “window” in tmux ≠ new browser pane/tab model in vmux                              |
| `next-layout`     | No      |                                                                                        |
| `next-window`     | No      |                                                                                        |
| `pipe-pane`       | No      |                                                                                        |
| `previous-layout` | No      |                                                                                        |
| `previous-window` | No      |                                                                                        |
| `rename-window`   | No      |                                                                                        |
| `resize-pane`     | Partial | `-Z` (zoom) only; no incremental `-L/-R/-U/-D`, `-x`/`-y`, `-M`                        |
| `resize-window`   | No      |                                                                                        |
| `respawn-pane`    | No      |                                                                                        |
| `respawn-window`  | No      |                                                                                        |
| `rotate-window`   | Yes     | `try_rotate_window`                                                                    |
| `select-layout`   | No      | No preset layouts (`even-horizontal`, `tiled`, …)                                      |
| `select-pane`     | Partial | **Directional `-L/-R/-U/-D` only**; no `-l`, `-n`, `-p`, `-t`, `-m`/`-M`, …            |
| `select-window`   | No      |                                                                                        |
| `split-window`    | Partial | Splits exist; fixed **50/50** ratio; no tmux flags (`-h`/`-v`/`-l`/`-c`/shell, …)      |
| `swap-pane`       | Yes     | Directional neighbour swap (`try_swap_active_pane`)                                    |
| `swap-window`     | No      |                                                                                        |
| `unlink-window`   | No      |                                                                                        |


## Copy-mode subcommands

Invoked from copy mode (e.g. via `send-keys -X`). **92** names from [tmux(1)](https://man.openbsd.org/tmux.1) *Windows and panes* / copy-mode list; excluded mistaken tokens `char`, `word`, `line`, and the prose reference `command-prompt`. **All: Vmux No.**


| Command                            | Vmux | Notes          |
| ---------------------------------- | ---- | -------------- |
| `append-selection`                 | No   | Copy-mode only |
| `append-selection-and-cancel`      | No   | Copy-mode only |
| `back-to-indentation`              | No   | Copy-mode only |
| `begin-selection`                  | No   | Copy-mode only |
| `bottom-line`                      | No   | Copy-mode only |
| `cancel`                           | No   | Copy-mode only |
| `clear-selection`                  | No   | Copy-mode only |
| `copy-end-of-line`                 | No   | Copy-mode only |
| `copy-end-of-line-and-cancel`      | No   | Copy-mode only |
| `copy-line`                        | No   | Copy-mode only |
| `copy-line-and-cancel`             | No   | Copy-mode only |
| `copy-pipe`                        | No   | Copy-mode only |
| `copy-pipe-and-cancel`             | No   | Copy-mode only |
| `copy-pipe-end-of-line`            | No   | Copy-mode only |
| `copy-pipe-end-of-line-and-cancel` | No   | Copy-mode only |
| `copy-pipe-line`                   | No   | Copy-mode only |
| `copy-pipe-line-and-cancel`        | No   | Copy-mode only |
| `copy-pipe-no-clear`               | No   | Copy-mode only |
| `copy-selection`                   | No   | Copy-mode only |
| `copy-selection-and-cancel`        | No   | Copy-mode only |
| `copy-selection-no-clear`          | No   | Copy-mode only |
| `cursor-centre-horizontal`         | No   | Copy-mode only |
| `cursor-centre-vertical`           | No   | Copy-mode only |
| `cursor-down`                      | No   | Copy-mode only |
| `cursor-down-and-cancel`           | No   | Copy-mode only |
| `cursor-left`                      | No   | Copy-mode only |
| `cursor-right`                     | No   | Copy-mode only |
| `cursor-up`                        | No   | Copy-mode only |
| `end-of-line`                      | No   | Copy-mode only |
| `goto-line`                        | No   | Copy-mode only |
| `halfpage-down`                    | No   | Copy-mode only |
| `halfpage-down-and-cancel`         | No   | Copy-mode only |
| `halfpage-up`                      | No   | Copy-mode only |
| `history-bottom`                   | No   | Copy-mode only |
| `history-top`                      | No   | Copy-mode only |
| `jump-again`                       | No   | Copy-mode only |
| `jump-backward`                    | No   | Copy-mode only |
| `jump-forward`                     | No   | Copy-mode only |
| `jump-reverse`                     | No   | Copy-mode only |
| `jump-to-backward`                 | No   | Copy-mode only |
| `jump-to-forward`                  | No   | Copy-mode only |
| `jump-to-mark`                     | No   | Copy-mode only |
| `middle-line`                      | No   | Copy-mode only |
| `next-matching-bracket`            | No   | Copy-mode only |
| `next-paragraph`                   | No   | Copy-mode only |
| `next-prompt`                      | No   | Copy-mode only |
| `next-space`                       | No   | Copy-mode only |
| `next-space-end`                   | No   | Copy-mode only |
| `next-word`                        | No   | Copy-mode only |
| `next-word-end`                    | No   | Copy-mode only |
| `other-end`                        | No   | Copy-mode only |
| `page-down`                        | No   | Copy-mode only |
| `page-down-and-cancel`             | No   | Copy-mode only |
| `page-up`                          | No   | Copy-mode only |
| `pipe`                             | No   | Copy-mode only |
| `pipe-and-cancel`                  | No   | Copy-mode only |
| `pipe-no-clear`                    | No   | Copy-mode only |
| `previous-matching-bracket`        | No   | Copy-mode only |
| `previous-paragraph`               | No   | Copy-mode only |
| `previous-prompt`                  | No   | Copy-mode only |
| `previous-space`                   | No   | Copy-mode only |
| `previous-word`                    | No   | Copy-mode only |
| `rectangle-off`                    | No   | Copy-mode only |
| `rectangle-on`                     | No   | Copy-mode only |
| `rectangle-toggle`                 | No   | Copy-mode only |
| `refresh-from-pane`                | No   | Copy-mode only |
| `scroll-bottom`                    | No   | Copy-mode only |
| `scroll-down`                      | No   | Copy-mode only |
| `scroll-down-and-cancel`           | No   | Copy-mode only |
| `scroll-exit-off`                  | No   | Copy-mode only |
| `scroll-exit-on`                   | No   | Copy-mode only |
| `scroll-exit-toggle`               | No   | Copy-mode only |
| `scroll-middle`                    | No   | Copy-mode only |
| `scroll-to-mouse`                  | No   | Copy-mode only |
| `scroll-top`                       | No   | Copy-mode only |
| `scroll-up`                        | No   | Copy-mode only |
| `search-again`                     | No   | Copy-mode only |
| `search-backward`                  | No   | Copy-mode only |
| `search-backward-incremental`      | No   | Copy-mode only |
| `search-backward-text`             | No   | Copy-mode only |
| `search-forward`                   | No   | Copy-mode only |
| `search-forward-incremental`       | No   | Copy-mode only |
| `search-forward-text`              | No   | Copy-mode only |
| `search-reverse`                   | No   | Copy-mode only |
| `select-line`                      | No   | Copy-mode only |
| `select-word`                      | No   | Copy-mode only |
| `selection-mode`                   | No   | Copy-mode only |
| `set-mark`                         | No   | Copy-mode only |
| `start-of-line`                    | No   | Copy-mode only |
| `stop-selection`                   | No   | Copy-mode only |
| `toggle-position`                  | No   | Copy-mode only |
| `top-line`                         | No   | Copy-mode only |


## Official command aliases

Shorter names accepted by tmux (same behavior as primary). **Vmux** column matches the **primary** row in the sections above.


| Alias        | Primary command        | Vmux (same as primary)     |
| ------------ | ---------------------- | -------------------------- |
| `attach`     | `attach-session`       | see `attach-session`       |
| `detach`     | `detach-client`        | see `detach-client`        |
| `has`        | `has-session`          | see `has-session`          |
| `lsc`        | `list-clients`         | see `list-clients`         |
| `lscm`       | `list-commands`        | see `list-commands`        |
| `ls`         | `list-sessions`        | see `list-sessions`        |
| `lockc`      | `lock-client`          | see `lock-client`          |
| `locks`      | `lock-session`         | see `lock-session`         |
| `new`        | `new-session`          | see `new-session`          |
| `refresh`    | `refresh-client`       | see `refresh-client`       |
| `rename`     | `rename-session`       | see `rename-session`       |
| `showmsgs`   | `show-messages`        | see `show-messages`        |
| `source`     | `source-file`          | see `source-file`          |
| `start`      | `start-server`         | see `start-server`         |
| `suspendc`   | `suspend-client`       | see `suspend-client`       |
| `switchc`    | `switch-client`        | see `switch-client`        |
| `breakp`     | `break-pane`           | see `break-pane`           |
| `capturep`   | `capture-pane`         | see `capture-pane`         |
| `displayp`   | `display-panes`        | see `display-panes`        |
| `findw`      | `find-window`          | see `find-window`          |
| `joinp`      | `join-pane`            | see `join-pane`            |
| `killp`      | `kill-pane`            | see `kill-pane`            |
| `killw`      | `kill-window`          | see `kill-window`          |
| `lastp`      | `last-pane`            | see `last-pane`            |
| `last`       | `last-window`          | see `last-window`          |
| `linkw`      | `link-window`          | see `link-window`          |
| `lsp`        | `list-panes`           | see `list-panes`           |
| `lsw`        | `list-windows`         | see `list-windows`         |
| `movep`      | `move-pane`            | see `move-pane`            |
| `movew`      | `move-window`          | see `move-window`          |
| `neww`       | `new-window`           | see `new-window`           |
| `nextl`      | `next-layout`          | see `next-layout`          |
| `next`       | `next-window`          | see `next-window`          |
| `pipep`      | `pipe-pane`            | see `pipe-pane`            |
| `prevl`      | `previous-layout`      | see `previous-layout`      |
| `prev`       | `previous-window`      | see `previous-window`      |
| `renamew`    | `rename-window`        | see `rename-window`        |
| `resizep`    | `resize-pane`          | see `resize-pane`          |
| `resizew`    | `resize-window`        | see `resize-window`        |
| `respawnp`   | `respawn-pane`         | see `respawn-pane`         |
| `respawnw`   | `respawn-window`       | see `respawn-window`       |
| `rotatew`    | `rotate-window`        | see `rotate-window`        |
| `selectl`    | `select-layout`        | see `select-layout`        |
| `selectp`    | `select-pane`          | see `select-pane`          |
| `selectw`    | `select-window`        | see `select-window`        |
| `splitw`     | `split-window`         | see `split-window`         |
| `swapp`      | `swap-pane`            | see `swap-pane`            |
| `swapw`      | `swap-window`          | see `swap-window`          |
| `unlinkw`    | `unlink-window`        | see `unlink-window`        |
| `bind`       | `bind-key`             | see `bind-key`             |
| `lsk`        | `list-keys`            | see `list-keys`            |
| `send`       | `send-keys`            | see `send-keys`            |
| `unbind`     | `unbind-key`           | see `unbind-key`           |
| `set`        | `set-option`           | see `set-option`           |
| `show`       | `show-options`         | see `show-options`         |
| `setenv`     | `set-environment`      | see `set-environment`      |
| `showenv`    | `show-environment`     | see `show-environment`     |
| `clearphist` | `clear-prompt-history` | see `clear-prompt-history` |
| `confirm`    | `confirm-before`       | see `confirm-before`       |
| `menu`       | `display-menu`         | see `display-menu`         |
| `display`    | `display-message`      | see `display-message`      |
| `popup`      | `display-popup`        | see `display-popup`        |
| `showphist`  | `show-prompt-history`  | see `show-prompt-history`  |
| `clearhist`  | `clear-history`        | see `clear-history`        |
| `deleteb`    | `delete-buffer`        | see `delete-buffer`        |
| `lsb`        | `list-buffers`         | see `list-buffers`         |
| `loadb`      | `load-buffer`          | see `load-buffer`          |
| `pasteb`     | `paste-buffer`         | see `paste-buffer`         |
| `saveb`      | `save-buffer`          | see `save-buffer`          |
| `setb`       | `set-buffer`           | see `set-buffer`           |
| `showb`      | `show-buffer`          | see `show-buffer`          |
| `if`         | `if-shell`             | see `if-shell`             |
| `lock`       | `lock-server`          | see `lock-server`          |
| `run`        | `run-shell`            | see `run-shell`            |
| `wait`       | `wait-for`             | see `wait-for`             |


---

## Key bindings


| Command       | Vmux    | Notes                                                                   |
| ------------- | ------- | ----------------------------------------------------------------------- |
| `bind-key`    | Partial | Keybindings via `settings.ron` / presets; not tmux key tables           |
| `list-keys`   | No      |                                                                         |
| `send-keys`   | No      | Input goes to embedded browsers / UI, not generic pty injection         |
| `send-prefix` | Partial | Prefix **chord** exists (e.g. Ctrl+B); not tmux `send-prefix` to a pane |
| `unbind-key`  | Partial | Via settings overrides                                                  |


---

## Options (commands, not option names)


| Command        | Vmux    | Notes                                                      |
| -------------- | ------- | ---------------------------------------------------------- |
| `set-option`   | Partial | App/layout **settings** (e.g. RON); not tmux option scopes |
| `show-options` | No      |                                                            |


---

## Hooks


| Command      | Vmux |
| ------------ | ---- |
| `set-hook`   | No   |
| `show-hooks` | No   |


---

## Global and session environment


| Command            | Vmux |
| ------------------ | ---- |
| `set-environment`  | No   |
| `show-environment` | No   |


---

## Status line and prompts


| Command                | Vmux    | Notes                                                      |
| ---------------------- | ------- | ---------------------------------------------------------- |
| `clear-prompt-history` | No      |                                                            |
| `command-prompt`       | Partial | **Command palette** is the analogue, not tmux’s `:` prompt |
| `confirm-before`       | No      |                                                            |
| `display-menu`         | No      |                                                            |
| `display-message`      | No      |                                                            |
| `display-popup`        | No      |                                                            |
| `show-prompt-history`  | No      |                                                            |


---

## Buffers


| Command         | Vmux |
| --------------- | ---- |
| `choose-buffer` | No   |
| `clear-history` | No   |
| `delete-buffer` | No   |
| `list-buffers`  | No   |
| `load-buffer`   | No   |
| `paste-buffer`  | No   |
| `save-buffer`   | No   |
| `set-buffer`    | No   |
| `show-buffer`   | No   |


---

## Miscellaneous


| Command       | Vmux |
| ------------- | ---- |
| `clock-mode`  | No   |
| `if-shell`    | No   |
| `lock-server` | No   |
| `run-shell`   | No   |
| `wait-for`    | No   |


---

## Default tmux key bindings (prefix **C-b** + key)

Usual default action from [tmux(1) DEFAULT KEY BINDINGS](https://man.openbsd.org/tmux.1#DEFAULT_KEY_BINDINGS). **Vmux** column: what the `tmux` preset does for the same idea (not necessarily same key).


| Prefix + key            | Tmux default (typical command / behavior) | Vmux `tmux` preset                                                                                              |
| ----------------------- | ----------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| **C-b** (again)         | `send-prefix` (literal C-b to app)        | Second **C-b** cancels armed prefix (`[prefix_lead_just_pressed_double](crates/vmux_settings/src/bindings.rs)`) |
| **C-b** C-o             | `rotate-window` (forwards)                | No                                                                                                              |
| **C-b** C-z             | `suspend-client`                          | No                                                                                                              |
| **C-b** !               | `break-pane`                              | No                                                                                                              |
| **C-b** `"`             | `split-window` -v (top / bottom)          | **C-b** then **Shift+Quote** → vertical split (`SplitVertical`) — same key idiom as `"`                         |
| **C-b** #               | `list-buffers`                            | No                                                                                                              |
| **C-b** $               | `command-prompt` → rename session         | No                                                                                                              |
| **C-b** %               | `split-window` -h (left / right)          | **C-b** then **Shift+5** (`%` on US) → horizontal split (`SplitHorizontal`)                                     |
| **C-b** &               | `kill-window` (confirm)                   | No                                                                                                              |
| **C-b** '               | prompt → `select-window`                  | No                                                                                                              |
| **C-b** (               | `switch-client` -p                        | No                                                                                                              |
| **C-b** )               | `switch-client` -n                        | No                                                                                                              |
| **C-b** ,               | `command-prompt` → `rename-window`        | No                                                                                                              |
| **C-b** -               | `delete-buffer`                           | No                                                                                                              |
| **C-b** .               | `command-prompt` → `move-window`          | No                                                                                                              |
| **C-b** 0–9             | `select-window` -t                        | No                                                                                                              |
| **C-b** :               | `command-prompt`                          | **Partial**: **⌘T** / **Ctrl+T** → command palette (macOS / non-macOS)                                          |
| **C-b** ;               | `last-pane`                               | No                                                                                                              |
| **C-b** =               | `choose-buffer`                           | No                                                                                                              |
| **C-b** ?               | `list-keys`                               | No                                                                                                              |
| **C-b** D               | `choose-client`                           | No                                                                                                              |
| **C-b** L               | `switch-client` -l                        | No                                                                                                              |
| **C-b** [               | `copy-mode`                               | No                                                                                                              |
| **C-b** ]               | `paste-buffer`                            | No                                                                                                              |
| **C-b** c               | `new-window`                              | No                                                                                                              |
| **C-b** d               | `detach-client`                           | No                                                                                                              |
| **C-b** f               | `command-prompt` → `find-window`          | No                                                                                                              |
| **C-b** i               | `display-message`                         | No                                                                                                              |
| **C-b** l               | `last-window`                             | No                                                                                                              |
| **C-b** m               | `select-pane` -m (mark)                   | **C-b** **m** → **mirror split** (`MirrorLayout`) — **not** tmux mark-pane                                      |
| **C-b** M               | `select-pane` -M (clear mark)             | No                                                                                                              |
| **C-b** n               | `next-window`                             | No                                                                                                              |
| **C-b** o               | `select-pane` -t:.+ (next pane)           | **C-b** **o** → cycle pane (`CycleNextPane`)                                                                    |
| **C-b** p               | `previous-window`                         | No                                                                                                              |
| **C-b** q               | `display-panes`                           | No                                                                                                              |
| **C-b** r               | `refresh-client`                          | **C-b** **r** / **C-b** **Shift+r** → rotate layout (backward / forward) — **not** tmux refresh                 |
| **C-b** s               | `choose-tree` (sessions)                  | No                                                                                                              |
| **C-b** t               | `clock-mode`                              | No                                                                                                              |
| **C-b** w               | `choose-tree` (windows)                   | No                                                                                                              |
| **C-b** x               | `kill-pane`                               | **C-b** **x** → close pane (`ClosePane`)                                                                        |
| **C-b** z               | `resize-pane` -Z                          | **C-b** **z** → toggle zoom (`ToggleZoom`)                                                                      |
| **C-b** {               | `swap-pane` -U                            | No (vmux uses **C-b** **Shift+[** / **]** for **rotate**, not tmux `{`/`}` swap)                                |
| **C-b** }               | `swap-pane` -D                            | No                                                                                                              |
| **C-b** ~               | `show-messages`                           | No                                                                                                              |
| **C-b** Page Up         | `copy-mode` -u                            | No                                                                                                              |
| **C-b** ↑ ↓ ← →         | `select-pane` -U / -D / -L / -R           | **C-b** + arrow → same focus idea (`SelectPane`)                                                                |
| **C-b** C-↑ C-↓ C-← C-→ | `resize-pane` by 1 cell                   | **C-b** **Ctrl+arrow** → **swap** pane (`SwapPane`) — **not** tmux resize                                       |
| **C-b** M-↑ …           | `resize-pane` by 5 cells                  | No                                                                                                              |
| **C-b** M-1 … M-7       | `select-layout` presets                   | No                                                                                                              |
| **C-b** Space           | `next-layout`                             | No                                                                                                              |
| **C-b** M-n             | `next-window` -a                          | No                                                                                                              |
| **C-b** M-o             | `rotate-window` -U                        | No                                                                                                              |
| **C-b** M-p             | `previous-window` -a                      | No                                                                                                              |


---

## Vmux `tmux` preset: prefix lead


| Role         | Physical chord (`[ChordStep](crates/vmux_settings/src/bindings.rs)`) | Notes                                                     |
| ------------ | -------------------------------------------------------------------- | --------------------------------------------------------- |
| Prefix lead  | **Ctrl+B** (`KeyB`)                                                  | Matches tmux default prefix.                              |
| Timeout      | `PREFIX_TIMEOUT_SECS` from settings                                  | Same resource as tmux-like “wait for second key”.         |
| `vim` preset | **Ctrl+`** (`Backquote`)                                             | Only the **lead** changes; second-key table is unchanged. |


---

## Vmux `tmux` preset: global chords (no prefix)

These run **without** **C-b**. **macOS** uses **⌘** (`command: true`); **non-macOS** uses **Ctrl** (and **Ctrl+Shift+H** for history).


| Chord                     | `BindingCommandId`       | Tmux analogue (if any)         |
| ------------------------- | ------------------------ | ------------------------------ |
| **⌘Q** / **Ctrl+Q**       | `Quit`                   | No                             |
| **⌘T** / **Ctrl+T**       | `ToggleCommandPalette`   | **Partial** → `command-prompt` |
| **⌘L** / **Ctrl+L**       | `FocusCommandPaletteUrl` | No                             |
| **⌘Y** / **Ctrl+Shift+H** | `OpenHistory`            | No                             |


---

## Vmux `tmux` preset: prefix + second key

From `[tmux_prefix_second_defaults](crates/vmux_settings/src/bindings.rs)`.


| After **C-b**         | `BindingCommandId`             | Tmux-oriented note                                               |
| --------------------- | ------------------------------ | ---------------------------------------------------------------- |
| **Shift+5** (`%`)     | `SplitHorizontal`              | Same physical key as default **%** split                         |
| **Shift+Quote** (`"`) | `SplitVertical`                | Same physical key as default `"` split                           |
| **o**                 | `CycleNextPane`                | Same as **o**                                                    |
| **Arrow**             | `SelectPaneLeft/Right/Up/Down` | Same as arrow bindings                                           |
| **Ctrl+Arrow**        | `SwapPaneLeft/Right/Up/Down`   | **Not** tmux default (tmux uses **C-arrow** for **resize-pane**) |
| **z**                 | `ToggleZoom`                   | Same as **z** → `resize-pane -Z`                                 |
| **m**                 | `MirrorLayout`                 | **Not** tmux **m** (mark pane)                                   |
| **Shift+[**           | `RotateForward`                | **Partial** vs tmux `{`/`}` swap, **C-o** / **M-o** rotate       |
| **Shift+]**           | `RotateBackward`               |                                                                  |
| **r**                 | `RotateBackward`               |                                                                  |
| **Shift+r**           | `RotateForward`                |                                                                  |
| **x**                 | `ClosePane`                    | Same as **x** → `kill-pane`                                      |


---

## Vmux extra chords (no prefix; not tmux defaults)

Registered in `[pane_ops.rs](crates/vmux_layout/src/pane_ops.rs)` / input. Skipped while prefix is **armed** (`tmux_prefix_armed`).


| Chord              | Behavior                                                                            | Tmux default?            |
| ------------------ | ----------------------------------------------------------------------------------- | ------------------------ |
| **Ctrl+Shift+Tab** | Cycle pane focus (`cycle_pane_focus` → `try_cycle_pane_focus`)                      | No (tmux uses **C-b o**) |
| **Ctrl+Shift+V**   | Split active pane, horizontal axis (`split_active_pane` → `LayoutAxis::Horizontal`) | No                       |
| **Ctrl+Shift+H**   | Split active pane, vertical axis                                                    | No                       |


---

## Vmux **Ctrl+Arrow** focus (optional; default **on** for `tmux` preset)

`[ctrl_arrow_focus_commands](crates/vmux_input/src/system.rs)`: when `ctrl_arrow_focus` is true and prefix is **not** awaiting, **Ctrl+Arrow** moves focus (`try_select_pane_direction`) — like tmux **prefix + arrow** but **without** prefix. Tmux has no identical default (tmux **C-arrow after prefix resizes).

---

## Summary counts

**Primary command names** (tables *Clients* through *Miscellaneous*, excluding copy-mode subs and aliases): **88**.

**Copy-mode subcommands** (dedicated table): **92** — all **No** in vmux.

**Official aliases** (dedicated table): **75** — vmux status matches the mapped primary.

**Vmux parity on primaries only**:

- **Yes**: 3 (`kill-pane`, `rotate-window`, `swap-pane`)
- **Partial**: 9 (`find-window`, `resize-pane`, `select-pane`, `split-window`, `bind-key`, `send-prefix`, `unbind-key`, `set-option`, `command-prompt`)
- **No**: all other primary commands, all copy-mode subcommands, and any alias whose primary is **No**

Plus vmux-only helper `mirror_window` in `[crates/vmux_layout/src/tmux.rs](crates/vmux_layout/src/tmux.rs)` — **not** a tmux(1) command name.

**Not listed as separate commands**: preset layout names (`even-horizontal`, `tiled`, …) are **arguments** to `select-layout`, not extra command verbs in tmux(1).

---

## Commands explicitly named in vmux `crates/` source

Tighter list: only what the repo **links or names** as tmux (see `[crates/vmux_layout/src/tmux.rs](crates/vmux_layout/src/tmux.rs)`, `[crates/vmux_layout/src/pane_ops.rs](crates/vmux_layout/src/pane_ops.rs)`, `[crates/vmux_layout/src/lib.rs](crates/vmux_layout/src/lib.rs)`):


| Tmux                             | Vmux                         |
| -------------------------------- | ---------------------------- |
| `select-pane`                    | Partial (`-L/-R/-U/-D` only) |
| `swap-pane`                      | Yes                          |
| `split-window`                   | Partial                      |
| `kill-pane`                      | Yes                          |
| `rotate-window`                  | Yes                          |
| `resize-pane`                    | Partial (`-Z` only)          |
| “next / previous pane” (doc row) | Yes (cycle / prefix `o`)     |


`[crates/vmux_settings/src/lib.rs](crates/vmux_settings/src/lib.rs)` uses tmux **option** naming (`pane-border-`*, `window-`*, etc.) for layout fields — not commands.
