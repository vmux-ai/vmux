# Dioxus Error Page (no `data://`, original URL preserved) — Design

Date: 2026-06-16

## Problem

Error pages (not-found, page-open failure, agent errors) are rendered as `data:text/html`
webviews via `attach_error_page_to_stack` → `data_url_for_html`. This is the only `data://`
usage. The stack's `PageMetadata.url` becomes the `data:` URL, so the address bar shows
`data:...`/`vmux://error/...` instead of what the user navigated to.

## Goal

- Every embedded page is Dioxus-based; no `data://` anywhere.
- A Dioxus error page at `vmux://error/` renders the error (heading + message + attempted URL).
- The stack's shown URL stays the **original** attempted URL (e.g. `vmux://debug/`).

## Architecture

- **New wasm page** `vmux_layout::error_page` (host `"error"`), registered in the
  `vmux_server` `web_pages!` macro and via `ERROR_PAGE_MANIFEST { host: "error" }` spawned in
  `cef.rs`. It reads `window.location.search` for `title`, `message`, `url` (percent-decoded)
  and renders them with `use_theme`, matching the prior visual.
- **`attach_error_page_to_stack`** (vmux_browser) is the single funnel. It now:
  - Builds `WebviewSource = vmux://error/?title=…&message=…&url=<orig>` (percent-encoded).
  - Spawns the error webview with that source but `PageMetadata.url = <orig>` and
    `title = <title>`, decoupling shown-URL from loaded-content (cannot use `Browser::new`,
    which ties them).
  - `data_url_for_html` and the inline HTML are deleted.
- **Display URL (`<orig>`) per caller** in `handle_unclaimed_page_open_tasks` /
  agent: not-found → `task.url`; page-open failure → `task.url`; agent → its
  `vmux://error/agent/<kind>/`.

### Query encoding

Reuse the existing percent-encoder (unreserved set) extracted as `percent_encode`. The error
page percent-decodes `%XX` manually (no new deps).

## Components

- `crates/vmux_layout/src/error_page.rs` (new, wasm): `Page` reading `title`/`message`/`url`
  from the query string.
- `crates/vmux_layout/src/lib.rs`: `#[cfg(wasm)] pub mod error_page;` + `ERROR_PAGE_MANIFEST`.
- `crates/vmux_layout/src/cef.rs`: spawn `ERROR_PAGE_MANIFEST`.
- `crates/vmux_server/src/lib.rs`: `render_error: "error" => vmux_layout::error_page::Page`.
- `crates/vmux_browser/src/lib.rs`: rewrite `attach_error_page_to_stack` (decoupled spawn +
  query build via `percent_encode`), drop `data_url_for_html`; callers pass display URL.

## Testing

- `percent_encode` round-trips with the page's decoder (shared test vectors): spaces, `&`,
  `/`, unicode.
- `error_page_query` builder produces `vmux://error/?title=…&message=…&url=…`.
- Error host manifest/url consistency (host `"error"`).
- Manual: navigate to a bogus `vmux://nope` → address bar shows `vmux://nope/`, content is the
  Dioxus error; agent error still renders.

## Edge cases

- Reload/persistence reopens `<orig>` → re-errors → error page again (correct; page absent).
- `mirror_metadata_to_url` may copy the error title onto a matching url-entity — benign.

## Scope

One funnel function rewrite + one new wasm page + registration. Removes the only `data://`
usage. No change to non-error page handling.
