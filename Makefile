.PHONY: run-mac build-mac-debug build bundle-mac setup-cef install-debug-render-process doctor-mac

CARGO_BIN := $(or $(shell command -v cargo 2>/dev/null),$(HOME)/.cargo/bin/cargo)
RUSTUP_BIN := $(or $(shell command -v rustup 2>/dev/null),$(HOME)/.cargo/bin/rustup)
EXPORT_CEF_BIN := $(or $(shell command -v export-cef-dir 2>/dev/null),$(HOME)/.cargo/bin/export-cef-dir)
WASM_BINDGEN_BIN := $(or $(shell command -v wasm-bindgen 2>/dev/null),$(HOME)/.cargo/bin/wasm-bindgen)
CEF_FRAMEWORK_DIR := $(HOME)/.local/share/Chromium Embedded Framework.framework
CEF_DEBUG_RENDER := $(CEF_FRAMEWORK_DIR)/Libraries/bevy_cef_debug_render_process

# Status bar (`dist/`) and history UI (`web_dist/`) are built by each crate’s `build.rs` when you compile `vmux` or `vmux_status_bar` / `vmux_history`.

# Build then exec the binary instead of `cargo run` so the foreground process is vmux (not Cargo).
run-mac: build-mac-debug
	exec env -u CEF_PATH ./target/debug/vmux

build-mac-debug:
	env -u CEF_PATH "$(CARGO_BIN)" build -p vmux --features debug

build:
	env -u CEF_PATH "$(CARGO_BIN)" build -p vmux --release

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

doctor-mac:
	@set -eu; \
	fail=0; \
	echo "Checking macOS dev dependencies for run-mac..."; \
	if [ -x "$(EXPORT_CEF_BIN)" ]; then \
		echo "[ok] export-cef-dir is installed at $(EXPORT_CEF_BIN)"; \
	else \
		echo "[precheck] export-cef-dir is not installed yet"; \
		echo "  This is expected before CEF setup. Run: make setup-cef"; \
	fi; \
	if [ ! -d "$$HOME/.local/share" ]; then \
		echo "[missing] CEF install base directory does not exist: $$HOME/.local/share"; \
		echo "  Run: mkdir -p \"$$HOME/.local/share\""; \
		fail=1; \
	elif [ ! -w "$$HOME/.local/share" ]; then \
		echo "[missing] CEF install base is not writable: $$HOME/.local/share"; \
		echo "  Run: chmod u+rwx \"$$HOME/.local/share\""; \
		fail=1; \
	else \
		echo "[ok] CEF install base is writable: $$HOME/.local/share"; \
	fi; \
	if [ -x "$(CARGO_BIN)" ]; then \
		echo "[ok] cargo found at $(CARGO_BIN)"; \
	else \
		echo "[missing] cargo not found"; \
		echo "  Install Rust toolchain: https://rustup.rs/"; \
		echo "  Then reload shell PATH or run with: PATH=\"$$HOME/.cargo/bin:$$PATH\""; \
		fail=1; \
	fi; \
	if [ -x "$(RUSTUP_BIN)" ]; then \
		if "$(RUSTUP_BIN)" target list --installed | grep -qx "wasm32-unknown-unknown"; then \
			echo "[ok] rust target installed: wasm32-unknown-unknown"; \
		else \
			echo "[missing] rust target not installed: wasm32-unknown-unknown"; \
			echo "  Run: \"$(RUSTUP_BIN)\" target add wasm32-unknown-unknown"; \
			fail=1; \
		fi; \
	else \
		echo "[missing] rustup not found (required to install wasm target)"; \
		echo "  Install Rust toolchain: https://rustup.rs/"; \
		fail=1; \
	fi; \
	if command -v cmake >/dev/null 2>&1; then \
		echo "[ok] cmake found"; \
	else \
		echo "[missing] cmake not found"; \
		echo "  Install: brew install cmake"; \
		fail=1; \
	fi; \
	if command -v ninja >/dev/null 2>&1; then \
		echo "[ok] ninja found"; \
	else \
		echo "[missing] ninja not found"; \
		echo "  Install: brew install ninja"; \
		fail=1; \
	fi; \
	if command -v node >/dev/null 2>&1; then \
		echo "[ok] node found"; \
	else \
		echo "[missing] node not found"; \
		echo "  Install: brew install node"; \
		fail=1; \
	fi; \
	if command -v npm >/dev/null 2>&1; then \
		echo "[ok] npm found"; \
	else \
		echo "[missing] npm not found"; \
		echo "  Install: brew install node"; \
		fail=1; \
	fi; \
	if [ -x "$(WASM_BINDGEN_BIN)" ]; then \
		echo "[ok] wasm-bindgen CLI found at $(WASM_BINDGEN_BIN)"; \
	else \
		echo "[missing] wasm-bindgen CLI not found"; \
		echo "  Run: \"$(CARGO_BIN)\" install wasm-bindgen-cli"; \
		fail=1; \
	fi; \
	if [ -d "$(CEF_FRAMEWORK_DIR)" ]; then \
		echo "[ok] CEF framework found at $(CEF_FRAMEWORK_DIR)"; \
	else \
		echo "[missing] CEF framework not found at $(CEF_FRAMEWORK_DIR)"; \
		echo "  Run: make setup-cef"; \
		fail=1; \
	fi; \
	if [ -x "$(CEF_DEBUG_RENDER)" ]; then \
		echo "[ok] bevy_cef_debug_render_process found"; \
	else \
		echo "[missing] bevy_cef_debug_render_process not found"; \
		echo "  Run: make install-debug-render-process"; \
		fail=1; \
	fi; \
	if [ "$$fail" -eq 0 ]; then \
		echo "doctor-mac passed. You can run: make run-mac"; \
	else \
		echo "doctor-mac failed. Resolve the missing dependencies above."; \
		exit 1; \
	fi
