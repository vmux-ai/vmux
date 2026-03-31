.PHONY: run-mac build-mac-debug build bundle-mac setup-cef install-debug-render-process

# Status bar (`dist/`) and history UI (`web_dist/`) are built by each crate’s `build.rs` when you compile `vmux` or `vmux_status_bar` / `vmux_history`. History: set `VMUX_HISTORY_USE_DX=1` for the Dioxus CLI path (see `crates/vmux_history/build.rs`).

# Build then exec the binary instead of `cargo run` so the foreground process is vmux (not Cargo).
run-mac: build-mac-debug
	exec env -u CEF_PATH ./target/debug/vmux

build-mac-debug:
	env -u CEF_PATH cargo build -p vmux --features debug

build:
	env -u CEF_PATH cargo build -p vmux --release

bundle-mac:
	chmod +x scripts/bundle-macos.sh
	./scripts/bundle-macos.sh

# One-time CEF download (macOS paths; pin matches bevy_cef 0.5.x)
setup-cef:
	cargo install export-cef-dir@145.6.1+145.0.28 --force
	export-cef-dir --force "$$HOME/.local/share"

# After setup-cef: copy debug render helper for macOS dev (see README)
install-debug-render-process:
	cargo install bevy_cef_debug_render_process
	cp "$$HOME/.cargo/bin/bevy_cef_debug_render_process" \
	  "$$HOME/.local/share/Chromium Embedded Framework.framework/Libraries/bevy_cef_debug_render_process"
