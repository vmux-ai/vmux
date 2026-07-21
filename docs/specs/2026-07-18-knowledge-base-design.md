# Knowledge Base Design

Date: 2026-07-18
Status: Implemented

## Summary

Add a local-first Markdown knowledge base backed by ordinary files in a profile-agnostic vault.
The side sheet exposes its folder tree. Selecting a file opens its existing `file://` page, where
Markdown has an editable Note mode beside Editor and Diff.

## Decisions

- Default vault: `~/.vmux/knowledge/`.
- Note files are the source of truth. No proprietary document format.
- Note filenames use kebab-case. Reserved integration filenames such as `SKILL.md` keep their
  required spelling.
- Page titles live in YAML frontmatter instead of being derived from filenames.
- Browser/runtime profiles do not own notes.
- Standard editor bindings remain the default. Vim remains a global editor opt-in.
- `file://` Note mode uses the existing editor buffer, saving, undo, standard keymap, optional Vim
  keymap, and file watching.
- Knowledge reuses `file://` instead of introducing a second editor surface.
- `vmux://notes/` is excluded from the first slice. Search, summaries, backlinks, and graphs may
  justify a dedicated page later.
- The abandoned `feat/markdown-note-mode` branch is reference material only.
- Derived indexes may later live in Application Support; never inside note files.

## First Slice

`vmux_knowledge` adds:

- Recursive Markdown folder discovery under the vault.
- Filesystem invalidation and asynchronous tree rebuilding.
- A collapsible Knowledge tree between the Space and Stack cards.
- Folder and file links that open through `file://` in a new stack.
- A `Note | Editor | Diff` control for Markdown `file://` pages.

## UI

The side sheet is the primary Knowledge surface. Its header opens `welcome.md` through `file://`
when present and falls back to the vault directory. The body mirrors folders and Markdown links,
using the same collapsible tree interaction as bookmarks. Empty expanded folders show an explicit
empty state. Top-level folders provide useful organization without a separate “Build with” list.

The `file://` Note mode is the reading and writing surface. It stays editable, uses the shared
buffer and save path, and supports standard bindings by default with optional Vim bindings. The
entire rendered block activates editing, and clicks within the active block keep editing active.

## Storage

```text
~/.vmux/knowledge/
  welcome.md
  memories/claude/projects/<project>/
  memories/codex/local/
  memories/codex/extensions/
  projects/
  skills/<slug>/SKILL.md
  meetings/
  handbook/
```

```markdown
---
title: Welcome to Knowledge
---
```

The tree shows the frontmatter title while `file://` keeps the kebab-case path as the source of
truth. Note mode hides frontmatter delimiters and renders `title` as the page heading.

The vault is profile-agnostic because profiles isolate cookies, browser state, recordings, and test
sessions. User knowledge must survive profile changes. A configurable external vault path is a
follow-up.

## Agent Skills

`skills/<slug>/SKILL.md` is the Knowledge convention for user-owned agent skills. At agent startup,
vmux injects a deterministic skill catalog and up to 24 KiB of skill bodies into supported CLI and
ACP instruction surfaces. Additional skills remain discoverable by path. vmux never writes a
managed `AGENTS.md` into user projects.

## Agent Memories

At startup and before building agent context, vmux copies previously unseen Markdown memories from
Claude Code and Codex into `~/.vmux/knowledge/memories/`. Claude project memories preserve their
encoded project directory. Codex local and extension memories remain separate. Existing Knowledge
files are never overwritten, so the imported copy becomes user-owned after migration.

Every ACP session receives the complete migrated memory set through its appended system prompt.
Claude and Codex CLI sessions receive the same context through their native appended-instruction
surfaces. Vibe CLI receives a vmux-managed block in the user-level `~/.vibe/AGENTS.md`; vmux never
writes agent instruction files into project directories and preserves user-authored Vibe content.

## Safety

- Skip hidden directories and symlinks during scans.
- List only `.md`, `.markdown`, and `.mdx` files.
- Skip hidden entries and symlinks.
- Cap tree depth and entry count.
- Canonicalize opened paths and require containment within the vault.
- Keep vault directories private on Unix.

## Follow-ups

- Configurable vault path.
- Dedicated overview page if summaries, full-text search, backlinks, or graphs warrant it.
- Persisted full-text index for very large vaults.
- Wikilinks, backlinks, tags, and active-Space filters.
- Daily notes and quick capture.
- Agent tools for search, read, create, and append with file/line citations.
- Semantic search as an optional layer over full-text search.
