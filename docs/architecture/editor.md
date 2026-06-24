# Editor: a syntax-highlighted files surface

> Part of the [Vmux Architecture](../architecture.md) overview. One of the
> [pages](pages.md) Vmux renders in a pane.

The files Editor (`crates/vmux_editor`) is a fast, syntax-highlighted file browser and viewer. It
opens local files on the **`file://`** scheme — `file:///path/to/main.rs` — not on `vmux://`. Today
it is **read-first**: open, navigate, and read code with full highlighting — in-place editing and
language servers aren't wired up yet.

## Highlighting: syntect + two-face

Highlighting is pure Rust, no native toolchain and no separate process:

- **[`syntect`](https://crates.io/crates/syntect)** highlights line by line (`HighlightLines`)
  against a theme (`base16-ocean.dark`).
- **[`two-face`](https://crates.io/crates/two-face)** swaps syntect's tiny default grammar set for
  the **~200 syntaxes** from the `bat` project, chosen by file extension.

Each line becomes a list of **`StyledSpan`** (text + RGB + bold/italic) — a small, render-ready
model. That same model is the shared thread across surfaces: the file viewer, the file **preview**,
and **git diffs** (`crates/vmux_git`) all run the same syntect + two-face engine and emit the same
spans, so code looks identical wherever it appears. There's no tree-sitter and no LSP — highlighting
is grammar-based.

## The file browser

- **Miller-column browser** — parent · current · preview, navigated with vim keys (`j`/`k`/`h`/`l`;
  `.` toggles hidden files).
- **Typed previews** — directories, **images** (downscaled to thumbnails), and **text**
  (highlighted, capped for snappiness); binary files are sniffed and shown as info rather than
  garbage.
- **SVG language icons** — file types render as monochrome [simple-icons](https://simpleicons.org)
  path data (`currentColor`), not a glyph font.
- **Line virtualization** — the host ships only the **visible slice** of a file to the page and
  re-slices as you scroll, so a huge file opens instantly.

Like the other pages, it's a Dioxus app inside CEF, fed host→page over zero-copy **rkyv** events,
and it reloads automatically when a file changes on disk.
