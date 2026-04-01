# Keybinding presets (vmux): families, comparison, direction

Planning note for **multiple built-in binding presets** (selectable via `VMUX_BINDING_PRESET` / `settings.ron` `input:`) and how each family maps to vmux’s fixed [`KeyAction`](../../crates/vmux_command/src/lib.rs) surface. Not a full implementation checklist by itself.

## Scope

**What presets control today**

- **Globals**: palette, URL bar, history, quit (see [`VmuxBindingSettings::global`](../../crates/vmux_settings/src/bindings.rs)).
- **Prefix chord**: lead key + second key for splits, pane focus, swap, zoom, rotate, close (see `prefix` + `PrefixSecondBinding`).
- **Optional**: `ctrl_arrow_focus` for direct Ctrl+Arrow pane focus without prefix.

**What presets do *not* change (without new work)**

- Actions are enumerated; presets only **remap** chords to the same [`KeyAction`](../../crates/vmux_command/src/lib.rs) variants.
- Web content (CEF) still receives keys unless a binding is classified as “global” for suppression — chord design must stay compatible with that model.

---

## Preset families (mental models)

### 1. Tmux-style (current default)

**Idea:** One **prefix** (e.g. `Ctrl+b`), then **mnemonic second keys** (`%` / `"` for splits, arrows or vim-like `hjkl` for focus, etc.).

| Pros | Cons |
|------|------|
| Familiar to **terminal multiplexer** users; matches vmux’s tiling story | Prefix is **two keystrokes** for most layout ops |
| Second keys can mirror **tmux** docs and muscle memory | **`Ctrl+b` conflicts** with browser “bold” / editor bindings when focus semantics evolve |
| Easy to document as a **table** (prefix + key → action) | New users may not know the prefix without onboarding |

**Fit for vmux:** Strong — already implemented as [`preset_bindings("tmux")`](../../crates/vmux_settings/src/bindings.rs).

---

### 2. Vim-style (leader / “modal-adjacent”)

**Idea:** A **leader** key (often `Space`, `\`, or `` Ctrl+` ``) instead of tmux’s `Ctrl+b`, but still **chord-based** (not a full Vim insert/normal mode inside the shell). Optional: align second keys with **vim window** commands (`Ctrl+w` then `hjkl`, etc.) where it does not fight globals.

| Pros | Cons |
|------|------|
| Comfortable for **heavy Vim/Neovim** users | “Vim” means different things (leader vs `Ctrl+w` subtree) — **ambiguous** unless one canonical table is chosen |
| Leader can be chosen to **avoid tmux’s Ctrl+b** | Heavy overlap with **web apps** that bind `Space` or `\` |
| Can share second-key layout with tmux preset for **implementation reuse** | True Vim **modal** editing inside CEF is out of scope for a preset file |

**Fit for vmux:** Partial — repo already has [`vim_preset()`](../../crates/vmux_settings/src/bindings.rs) as a **thin variant** (different prefix lead, same second keys as tmux). A richer “vim” preset would remap seconds toward `hjkl` / `Ctrl+w` idioms.

---

### 3. Emacs-style (prefix chains / “C-x”)

**Idea:** **Emacs-like prefix** sequences, e.g. `C-x` as a **first chord**, then a second key for window commands — analogous to `C-x 1`, `C-x 2`, `C-x 3`, `C-x o` (conceptually; map to vmux actions that exist).

| Pros | Cons |
|------|------|
| Familiar to **Emacs** users who live in chords | Many **browser and OS** shortcuts already use `Ctrl`/`Alt` — higher **collision risk** |
| Chains map naturally to a **fixed action set** if kept short | **Longer sequences** feel heavier than a single prefix + one key |
| Can reserve **rare leaders** to reduce clashes | Teaching Emacs idioms to non-Emacs users is harder than tmux tables |

**Fit for vmux:** Moderate — needs careful choice of **lead chord** so globals (palette, URL, history) remain usable and CEF suppression stays correct.

---

### 4. Browser / “Chrome-style” (platform conventions)

**Idea:** Prefer **platform browser shortcuts**: e.g. macOS `⌘T` / `⌘L` / `⌘Y` (already reflected in globals), **no prefix** for common navigation; tiling might use **less common** chords (`Ctrl+Shift+…`) or a **dedicated prefix** only for layout.

| Pros | Cons |
|------|------|
| **Zero learning curve** for “open tab / address bar / history” | **Tiling** operations have **no universal standard** — still need a prefix or obscure chords |
| Aligns vmux with **Arc / Chrome** expectations | Linux/Windows differ from macOS (`Ctrl` vs `⌘`) — already handled with `cfg` in code, but presets must stay clear |
| Good default for **browser-first** users | **Power users** from tmux/vim may want a stronger layout story |

**Fit for vmux:** Strong for **globals**; tiling still pairs well with **tmux** or **i3**-style second layer.

---

### 5. Tiling WM–style (i3 / Sway–like)

**Idea:** **`Mod` + key**: `Mod+Enter` (new/split), `Mod+hjkl` or `Mod+arrows` for focus, `Mod+Shift+…` for move/swap, `Mod+f` fullscreen/zoom analog, `Mod+e` or similar for layout toggle if ever added.

| Pros | Cons |
|------|------|
| Familiar to **i3/Sway/Hyprland** users | **`Mod` is OS-owned** on macOS (`⌘`); `Alt`/`Ctrl` choices collide with browsers and terminals differently per OS |
| **Spatial** model (focus = directions) maps cleanly to [`SelectPane`](../../crates/vmux_command/src/lib.rs) | May **duplicate** OS window shortcuts unless scoped to “vmux global” only |
| One modifier + one key — often **faster** than prefix-then-key | Needs explicit **platform matrix** (macOS vs Linux vs Windows) |

**Fit for vmux:** Good as a **distinct preset** if documented per-OS; implementation is mostly new **global** or **prefix-second** tables, not new `KeyAction`s.

---

### 6. “cmux” / minimal / custom shorthand

**Clarify the name:** If **cmux** means a **specific tool** or in-house convention, document its chord table here. If the intent is **minimal keystrokes** or **Chrome-like + mux**, treat it as a **hybrid**: strong browser globals + **short** prefix (`Ctrl+Space`, `Alt+Space`, etc.) for layout.

| Pros | Cons |
|------|------|
| Can optimize for **lowest chord count** for daily splits/focus | **High collision risk** with IME, Spotlight, window managers |
| Useful as **power-user** preset | Hardest to **discover** without cheatsheet |

**Fit for vmux:** Optional fourth/fifth preset after tmux / vim / emacs or i3 are defined.

---

## Comparison summary

| Family | Learning curve | Speed (chords) | Collision risk (browser/OS) | Best for |
|--------|----------------|----------------|-----------------------------|----------|
| Tmux | Low (for tmux users) | Medium | Medium | Terminal-first, current default |
| Vim leader | Medium | Medium | Medium–high | Vim users; leader choice matters |
| Emacs | Medium–high | Medium (sometimes longer) | High | Emacs-first desktop workflows |
| Browser-native | Lowest for web | Fast for nav | Low for nav, high for tiling | General browser users |
| i3/Sway-like | Medium | Fast | Medium–high (OS-dependent) | Tiling WM users |
| Minimal / “cmux” | High | Fastest if tuned | Highest | Experts with a cheatsheet |

---

## Implementation direction (when building presets)

1. **Keep one canonical `KeyAction` enum**; add presets only as **data** in [`preset_bindings`](../../crates/vmux_settings/src/bindings.rs) (or split per-preset modules if the file grows).
2. **Document each preset** with a single table: globals + prefix lead + second keys (per OS if needed).
3. **Test** focus vs CEF: globals that must fire when a webview is focused should remain classified consistently in input handling.
4. **Avoid** duplicating the same physical chord on **globals** and **prefix seconds** without intent.
5. **Order of work** that matches impact: solidify **tmux** → flesh out **vim** (second keys) → add **i3-style** or **emacs-style** based on user demand → optional **minimal** hybrid.

---

## References

- [`VmuxBindingSettings`](../../crates/vmux_settings/src/bindings.rs), [`VMUX_BINDING_PRESET`](../../crates/vmux_settings/src/lib.rs) / `input:` in settings.
- [`KeyAction`](../../crates/vmux_command/src/lib.rs) — full list of bindable actions.
