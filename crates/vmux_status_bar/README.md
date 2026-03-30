# vmux status bar (Dioxus web)

Rust + **Dioxus** web app served from loopback by `vmux` (see `vmux_webview::StatusBarHostedPlugin`) and loaded in the active pane’s CEF strip.

- **`src/main.rs`** — `dioxus::launch` entry.
- **`src/app.rs`** — root **App** component (status strip markup).
- **`src/bridge.rs`** — `document::eval` script for clock + `window.cef.listen("vmux_status", …)`.
- **`src/payload.rs`** — host payload types and `apply_payload`.
- **`assets/input.css`** — Tailwind entry (`@tailwind` + `@layer base`).
- **`tailwind.config.js`** — theme (`tmux` colors, `text-status`, font stack).
- **`assets/status.css`** — Tailwind **build output** (included with `include_str!` in `app.rs`). **Not committed**; run `npm run build:css` once after clone and whenever you change classes or theme (see below).
- **`package.json`** — `tailwindcss` CLI for `build:css`.
- **`dist/`** — **not** hand-edited: produced by the build step below (WASM + `index.html`).

## Build (`dist/`)

Requires **Node.js** (for Tailwind) and [Dioxus CLI](https://dioxuslabs.com/learn/0.6/getting_started/) (`dx`), same major line as the `dioxus` crate (see `Cargo.toml`).

**Styles:** after editing `src/**/*.rs` (Tailwind classes) or `assets/input.css` / `tailwind.config.js`, regenerate CSS:

```bash
cd crates/vmux_status_bar && npm install && npm run build:css
```

From the repo root (runs `npm install`, `build:css`, then `dx build`):

```bash
make status-ui
```

Or manually:

```bash
cd crates/vmux_status_bar && npm install && npm run build:css && dx build --platform web
rm -rf dist && cp -R target/dx/vmux_status_bar/debug/web/public dist
```

Release (smaller WASM, slower build):

```bash
cd crates/vmux_status_bar && npm install && npm run build:css && dx build --platform web --release
rm -rf dist && cp -R target/dx/vmux_status_bar/release/web/public dist
```

`vmux` embeds `crates/vmux_status_bar/dist/` via Axum `ServeDir`; **`index.html`** must exist there.

Set **`VMUX_STATUS_UI_URL`** (e.g. `http://127.0.0.1:8080/`) to use `dx serve` while developing and skip the embedded server for that session.

## `dx` / `dioxus` version

If `dx doctor` reports a version skew, align the CLI (`dx self-update` or `cargo install dioxus-cli@…`) with the `dioxus` version in `Cargo.toml`.
