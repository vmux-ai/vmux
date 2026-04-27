# Path Navigator for Command Bar

## Overview

Smart path completion in the command bar. When user types path-like input, the desktop reads the filesystem and sends completions back to the WASM command bar. Inline ghost text shows the top suggestion; Tab accepts it. Path completions appear as a top section in the result list.

## Architecture

```
WASM (command bar)                    Desktop (Bevy)
─────────────────                     ──────────────
User types "Pro"
  → detects path-like input
  → debounce 50ms
  → cef.emit(PathCompleteRequest)
                            ─────────>
                                      splits at last '/'
                                      resolves ~ and relative paths
                                      reads parent dir
                                      filters by prefix
                                      sorts: dirs first, alpha
                                      caps at 20 entries
                            <─────────
  ← HostEmitEvent(PathCompleteResponse)
  → renders ghost text in input
  → shows completions as top section

User presses Tab
  → accepts ghost text
  → updates input value
  → triggers new completion request
```

## Events

### PathCompleteRequest (WASM -> Desktop)

```rust
struct PathCompleteRequest {
    query: String,  // current input value
}
```

### PathCompleteResponse (Desktop -> WASM)

```rust
struct PathEntry {
    name: String,      // e.g. "Projects/"
    is_dir: bool,
    full_path: String,  // e.g. "Projects/github.com/"
}

struct PathCompleteResponse {
    completions: Vec<PathEntry>,
}
```

## Desktop Handler

In `command_bar.rs`, observe `PathCompleteRequest`:

1. Split query at last `/` to get parent_dir and prefix
2. Resolve paths:
   - `~/...` -> `$HOME/...`
   - `/...` -> absolute
   - `foo/...` -> `$HOME/foo/...`
3. Read parent directory entries
4. Filter entries starting with prefix (case-insensitive)
5. Sort: directories first, then alphabetical
6. Append `/` to directory names
7. Cap at 20 entries
8. Send `PathCompleteResponse` via `HostEmitEvent`

## WASM Rendering

### Ghost text

Render the first completion's remaining characters as muted text overlaid on the input field. Use a positioned span after the input text with `text-muted-foreground` / lower opacity.

### Result list

When path completions are available:

```
>_ Open in Terminal  "Projects/"           ← completions (max 5)
>_ Open in Terminal  "Projects/personal/"
─────────────────────────────────────────
🔍 Search "Pro"                            ← normal results
Tab: "Some Tab"
```

Each path completion is a `ResultItem::Terminal { path }` — executing it opens a terminal at that directory.

### Keyboard

| Key | Action |
|-----|--------|
| Tab | Accept ghost text, update input, trigger new completion |
| Enter | Execute selected result item |
| Ctrl+N/P | Navigate result list (unchanged) |
| `/` after accept | Naturally triggers next-level completion |
| Escape | Dismiss command bar |

### Debounce

Wait 50ms after the last keystroke before sending `PathCompleteRequest`. Cancel pending requests when new input arrives.

## Scope

- Directories only in completions (files excluded for terminal cd)
- Hidden files/dirs (starting with `.`) included but sorted after visible entries
- Symlinks followed
- No recursive scanning — only immediate children of the resolved parent
