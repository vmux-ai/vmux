.PHONY: run-mac run-mac-local run-doctor build-mac-debug build build-local-mac package-mac setup-cef install-debug-render-process doctor-mac ensure-run-mac-deps run-website build-website lint lint-fix fmt clippy test

CARGO_BIN := $(or $(shell command -v cargo 2>/dev/null),$(HOME)/.cargo/bin/cargo)
RUSTUP_BIN := $(or $(shell command -v rustup 2>/dev/null),$(HOME)/.cargo/bin/rustup)
EXPORT_CEF_BIN := $(or $(shell command -v export-cef-dir 2>/dev/null),$(HOME)/.cargo/bin/export-cef-dir)
DX_BIN := $(or $(shell command -v dx 2>/dev/null),$(HOME)/.cargo/bin/dx)
DX_VERSION := 0.7.4
CEF_FRAMEWORK_DIR := $(HOME)/.local/share/Chromium Embedded Framework.framework
CEF_DEBUG_RENDER := $(CEF_FRAMEWORK_DIR)/Libraries/bevy_cef_debug_render_process

# Header / history / UI library `dist/` folders are built by each crate’s `build.rs` via **`dx build`** when you compile `vmux_desktop` (needs `dioxus-cli` on PATH).

# Build then exec the binary instead of `cargo run` so the foreground process is vmux_desktop (not Cargo).
run-mac: build-mac-debug
	exec env -u CEF_PATH ./target/debug/Vmux

build-mac-debug: ensure-run-mac-deps
	env -u CEF_PATH "$(CARGO_BIN)" build -p vmux_desktop --features debug

build: ensure-run-mac-deps
	env -u CEF_PATH "$(CARGO_BIN)" build -p vmux_desktop --release

-include .env
export

run-mac-local: package-mac
	open target/release/Vmux.app

package-mac:
	env -u CEF_PATH cargo packager --release

build-local-mac: package-mac
	@echo "Signing..."
	SKIP_NOTARIZE=1 ./scripts/sign-and-notarize.sh

# One-time CEF download (macOS paths; pin matches bevy_cef 0.5.x)
setup-cef:
	"$(CARGO_BIN)" install export-cef-dir@145.6.1+145.0.28 --force
	"$(EXPORT_CEF_BIN)" --force "$$HOME/.local/share"

# Build from vmux-patched bevy_cef_core (required when adding CEF schemes such as vmux://).
# Installs into the same path `bevy_cef` debug mode loads on macOS.
install-debug-render-process:
	env -u CEF_PATH "$(CARGO_BIN)" build -p bevy_cef_debug_render_process --features debug
	cp "$(CURDIR)/target/debug/bevy_cef_debug_render_process" \
	  "$(CEF_FRAMEWORK_DIR)/Libraries/bevy_cef_debug_render_process"

# Get workspace packages excluding vendored patches
PKGS = $(shell "$(CARGO_BIN)" metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.manifest_path | test("patches") | not) | .name')

lint: fmt clippy

lint-fix:
	@for pkg in $(PKGS); do \
		"$(CARGO_BIN)" fmt -p "$$pkg" || exit 1; \
	done
	@for pkg in $(PKGS); do \
		env -u CEF_PATH "$(CARGO_BIN)" clippy -p "$$pkg" --all-targets --fix --allow-dirty --allow-staged -- -D warnings || exit 1; \
	done

fmt:
	@for pkg in $(PKGS); do \
		"$(CARGO_BIN)" fmt -p "$$pkg" -- --check || exit 1; \
	done

clippy:
	@for pkg in $(PKGS); do \
		env -u CEF_PATH "$(CARGO_BIN)" clippy -p "$$pkg" --all-targets -- -D warnings || exit 1; \
	done

test:
	env -u CEF_PATH "$(CARGO_BIN)" test --workspace --exclude bevy_cef_core

# Website
run-website:
	cd website && "$(DX_BIN)" serve --platform web

build-website:
	cd website && "$(DX_BIN)" build --platform web --release

# Friendly prerequisite report (colors / emoji when terminal); README: make run-doctor
run-doctor: doctor-mac

doctor-mac:
	@chmod +x scripts/doctor-mac.sh
	@CARGO_BIN="$(CARGO_BIN)" RUSTUP_BIN="$(RUSTUP_BIN)" EXPORT_CEF_BIN="$(EXPORT_CEF_BIN)" \
		DX_BIN="$(DX_BIN)" CEF_FRAMEWORK_DIR="$(CEF_FRAMEWORK_DIR)" \
		CEF_DEBUG_RENDER="$(CEF_DEBUG_RENDER)" ./scripts/doctor-mac.sh

# Non-interactive bootstrap so `make run-mac` works even after dependency bumps.
ensure-run-mac-deps:
	@echo "Checking build dependencies for run-mac..."
	@if [ ! -x "$(CARGO_BIN)" ]; then \
		echo "cargo not found at $(CARGO_BIN). Install rustup first: https://rustup.rs/"; \
		exit 1; \
	fi
	@if [ ! -x "$(RUSTUP_BIN)" ]; then \
		echo "rustup not found at $(RUSTUP_BIN). Install rustup first: https://rustup.rs/"; \
		exit 1; \
	fi
	@if ! "$(RUSTUP_BIN)" target list --installed 2>/dev/null | grep -qx "wasm32-unknown-unknown"; then \
		echo "Installing rust target wasm32-unknown-unknown..."; \
		"$(RUSTUP_BIN)" target add wasm32-unknown-unknown; \
	fi
	@dx_version="$$( ("$(DX_BIN)" --version 2>/dev/null || true) | awk '{print $$2}' )"; \
	if [ "$$dx_version" != "$(DX_VERSION)" ]; then \
		echo "Installing dioxus-cli $(DX_VERSION) (found: $${dx_version:-missing})..."; \
		"$(CARGO_BIN)" install dioxus-cli --locked --version "$(DX_VERSION)"; \
	fi
