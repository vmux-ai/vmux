# Feature demos

Short screen recordings showcasing vmux features. Captured with the
`vmux_record_start` / `vmux_record_stop` MCP tools (see
`docs/specs/2026-06-23-video-recording-mcp-tool-design.md`).

## Recording a demo

From an agent with vmux MCP tools:

1. `vmux_record_start { "gif": true, "max_secs": 60 }`
2. Drive the feature (open pages, run commands, switch tabs).
3. `vmux_record_stop { "dir": "<repo>/docs/features", "name": "<feature>" }`

This writes `<feature>.mp4` (+ `<feature>.gif`) here. Drag the `.mp4` into a PR
description to embed an inline player, or reference the `.gif` from markdown.

## Keep clips small

Committed video bloats git history. Keep demos short (a few seconds), prefer the
mp4, and only commit a GIF when inline autoplay is needed. Large/long clips
should live in the PR upload (GitHub CDN), not the repo.
