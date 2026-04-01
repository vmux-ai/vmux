.PHONY: run-mac run-doctor build-mac-debug build bundle-mac setup-cef install-debug-render-process doctor-mac

CARGO_BIN := $(or $(shell command -v cargo 2>/dev/null),$(HOME)/.cargo/bin/cargo)
RUSTUP_BIN := $(or $(shell command -v rustup 2>/dev/null),$(HOME)/.cargo/bin/rustup)
EXPORT_CEF_BIN := $(or $(shell command -v export-cef-dir 2>/dev/null),$(HOME)/.cargo/bin/export-cef-dir)
WASM_BINDGEN_BIN := $(or $(shell command -v wasm-bindgen 2>/dev/null),$(HOME)/.cargo/bin/wasm-bindgen)
CEF_FRAMEWORK_DIR := $(HOME)/.local/share/Chromium Embedded Framework.framework
CEF_DEBUG_RENDER := $(CEF_FRAMEWORK_DIR)/Libraries/bevy_cef_debug_render_process

# Status bar (`dist/`) and history UI (`web_dist/`) are built by each crate’s `build.rs` when you compile `vmux_desktop` or `vmux_status_bar` / `vmux_history`.

# Build then exec the binary instead of `cargo run` so the foreground process is vmux_desktop (not Cargo).
run-mac: build-mac-debug
	exec env -u CEF_PATH ./target/debug/vmux_desktop

build-mac-debug:
	env -u CEF_PATH "$(CARGO_BIN)" build -p vmux_desktop --features debug

build:
	env -u CEF_PATH "$(CARGO_BIN)" build -p vmux_desktop --release

bundle-mac:
	chmod +x scripts/bundle-macos.sh
	./scripts/bundle-macos.sh

# One-time CEF download (macOS paths; pin matches bevy_cef 0.5.x)
setup-cef:
	"$(CARGO_BIN)" install export-cef-dir@145.6.1+145.0.28 --force
	"$(EXPORT_CEF_BIN)" --force "$$HOME/.local/share"

# After setup-cef: copy debug render helper for macOS dev (see README)
install-debug-render-process:
	"$(CARGO_BIN)" install bevy_cef_debug_render_process
	cp "$$HOME/.cargo/bin/bevy_cef_debug_render_process" \
	  "$$HOME/.local/share/Chromium Embedded Framework.framework/Libraries/bevy_cef_debug_render_process"

# Friendly prerequisite report (colors / emoji when terminal); README: make run-doctor
run-doctor: doctor-mac

doctor-mac:
	@chmod +x scripts/doctor-mac.sh
	@CARGO_BIN="$(CARGO_BIN)" RUSTUP_BIN="$(RUSTUP_BIN)" EXPORT_CEF_BIN="$(EXPORT_CEF_BIN)" \
		WASM_BINDGEN_BIN="$(WASM_BINDGEN_BIN)" CEF_FRAMEWORK_DIR="$(CEF_FRAMEWORK_DIR)" \
		CEF_DEBUG_RENDER="$(CEF_DEBUG_RENDER)" ./scripts/doctor-mac.sh
