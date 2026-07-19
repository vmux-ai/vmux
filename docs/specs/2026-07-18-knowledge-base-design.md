# Knowledge Base Design

Date: 2026-07-18
Status: Implemented

## Summary

Add a local-first Markdown knowledge base at `vmux://notes/`. Notes remain ordinary files in a
profile-agnostic vault. The Notes page provides discovery, search, reading, creation, and immediate
editing. Markdown files also gain an editable Note mode beside Editor and Diff.

## Decisions

- Default vault: `~/.vmux/knowledge/`.
- Note files are the source of truth. No proprietary document format.
- Browser/runtime profiles do not own notes.
- Standard editor bindings remain the default. Vim remains a global editor opt-in.
- `file://` Note mode uses the existing editor buffer, saving, undo, standard keymap, optional Vim
  keymap, and file watching.
- The Notes reading pane is click-to-edit and autosaves without an Edit-button handoff.
- The abandoned `feat/markdown-note-mode` branch is reference material only.
- Derived indexes may later live in Application Support; never inside note files.

## First Slice

`vmux_knowledge` adds:

- `vmux://notes/` page manifest and page-open handling.
- Recursive Markdown discovery under the vault.
- Title, excerpt, modified-time, and full-text filtering.
- Mtime/size-aware in-memory index, filesystem invalidation, and paginated results.
- Safe note creation with human-readable unique filenames.
- Rich Markdown preview with immediate source editing and autosave.
- Local vault links and images with remote images blocked by default.
- A `Note | Editor | Diff` control for Markdown `file://` pages.
- Dedicated Knowledge page icon and command-bar entry.
- Dedicated Knowledge card between Space and Stack cards.
- Semantic theme tokens shared with other `vmux://` pages.

## UI

Three surfaces:

1. Library rail: product identity, note count, vault location, create action.
2. Searchable note list: title, excerpt, relative path, modified time.
3. Reading canvas: centered typography, note metadata, click-to-edit source, autosave state.

The page is useful without modal editing. Keyboard and pointer access are first-class. The side
rail suggests organizational uses: skills, decisions, runbooks, project briefs, meetings,
handbook pages, research, and templates.

## Storage

```text
~/.vmux/knowledge/
  Welcome.md
  projects/
  skills/<slug>/SKILL.md
  decisions/
  meetings/
  runbooks/
  handbook/
  research/
  templates/
```

The vault is profile-agnostic because profiles isolate cookies, browser state, recordings, and test
sessions. User knowledge must survive profile changes. A configurable external vault path is a
follow-up.

## Agent Skills

`skills/<slug>/SKILL.md` is the Knowledge convention for user-owned agent skills. At agent startup,
vmux injects a deterministic skill catalog and up to 24 KiB of skill bodies into supported CLI and
ACP instruction surfaces. Additional skills remain discoverable by path. vmux never writes a
managed `AGENTS.md` into user projects.

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
- Agent tools for search, read, create, and append with file/line citations.
- Semantic search as an optional layer over full-text search.
