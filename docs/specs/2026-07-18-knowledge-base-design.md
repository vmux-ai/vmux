# Knowledge Base Design

Date: 2026-07-18
Status: Implemented first slice

## Summary

Add a local-first Markdown knowledge base at `vmux://notes/`. Notes remain ordinary files in a
profile-agnostic vault. The Notes page provides discovery, search, reading, and creation. Editing
opens the existing file editor, inheriting its standard default keymap and optional Vim keymap.

## Decisions

- Default vault: `~/.vmux/knowledge/`.
- Note files are the source of truth. No proprietary document format.
- Browser/runtime profiles do not own notes.
- Standard editor bindings remain the default. Vim remains a global editor opt-in.
- Notes page owns library UX. `vmux_editor` owns buffers, saving, undo, keymaps, and file watching.
- The abandoned `feat/markdown-note-mode` branch is reference material only.
- Derived indexes may later live in Application Support; never inside note files.

## First Slice

`vmux_knowledge` adds:

- `vmux://notes/` page manifest and page-open handling.
- Recursive Markdown discovery under the vault.
- Title, excerpt, modified-time, and full-text filtering.
- Mtime/size-aware in-memory index, filesystem invalidation, and paginated results.
- Safe note creation with human-readable unique filenames.
- Rich read-only Markdown preview with raw HTML removed.
- Local vault links and images with remote images blocked by default.
- Open-in-editor action using a normal `file://` page.
- Dedicated Knowledge page icon and command-bar entry.

## UI

Three surfaces:

1. Library rail: product identity, note count, vault location, create action.
2. Searchable note list: title, excerpt, relative path, modified time.
3. Reading canvas: centered typography, note metadata, edit action.

The page is useful without modal editing. Keyboard and pointer access are first-class. Editing is
delegated to the existing editor so keymap behavior cannot diverge.

## Storage

```text
~/.vmux/knowledge/
  Welcome.md
  projects/
  daily/
  attachments/
```

The vault is profile-agnostic because profiles isolate cookies, browser state, recordings, and test
sessions. User knowledge must survive profile changes. A configurable external vault path is a
follow-up.

## Safety

- Skip hidden directories and symlinks during scans.
- Read only `.md`, `.markdown`, and `.mdx` files.
- Cap indexed and previewed file size.
- Canonicalize requested paths and require containment within the vault.
- Use no-follow bounded reads and private Unix permissions.
- Drop Markdown raw HTML before producing preview HTML.
- Block automatic network image requests.
- Sanitize created filenames and avoid overwriting existing files.

## Follow-ups

- Configurable vault path.
- Persisted full-text index for very large vaults.
- Wikilinks, backlinks, tags, and active-Space filters.
- Daily notes and quick capture.
- Inline live-preview editing.
- Agent tools for search, read, create, and append with file/line citations.
- Semantic search as an optional layer over full-text search.
