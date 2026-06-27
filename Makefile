.PHONY: dev test-app local release build-local build-release build setup-cef install-debug-render-process doctor ensure-mac-deps ensure-package-deps ensure-codesign-deps website build-website-release build-website-css api-docs lint lint-fix test setup-hooks cleanup

.DEFAULT_GOAL := dev

VMUX_PROFILE ?= personal
VMUX_TEST ?=

CARGO_BIN := $(or $(shell command -v cargo 2>/dev/null),$(HOME)/.cargo/bin/cargo)
RUSTUP_BIN := $(or $(shell command -v rustup 2>/dev/null),$(HOME)/.cargo/bin/rustup)
EXPORT_CEF_BIN := $(or $(shell command -v export-cef-dir 2>/dev/null),$(HOME)/.cargo/bin/export-cef-dir)
DX_BIN := $(or $(shell command -v dx 2>/dev/null),$(HOME)/.cargo/bin/dx)
CARGO_PACKAGER_BIN := $(or $(shell command -v cargo-packager 2>/dev/null),$(HOME)/.cargo/bin/cargo-packager)
BEVY_CEF_BUNDLE_APP_BIN := $(or $(shell command -v bevy_cef_bundle_app 2>/dev/null),$(HOME)/.cargo/bin/bevy_cef_bundle_app)
DX_VERSION := 0.7.4
CARGO_PACKAGER_VERSION := 0.11.9
BEVY_CEF_BUNDLE_APP_VERSION := 0.8.1
CEF_VERSION := $(shell awk -F'"' '/^name = "cef"$$/{getline; print $$2; exit}' Cargo.lock)
CEF_FRAMEWORK_DIR := $(HOME)/.local/share/Chromium Embedded Framework.framework
CEF_DEBUG_RENDER := $(CEF_FRAMEWORK_DIR)/Libraries/bevy_cef_debug_render_process

# Header / history / UI library `dist/` folders are built by each crate’s `build.rs` via **`dx build`** when you compile `vmux_desktop` (needs `dioxus-cli` on PATH).

dev: ensure-mac-deps ensure-codesign-deps install-debug-render-process
	env -u CEF_PATH "$(CARGO_BIN)" build -p vmux_service -p vmux_cli
	env -u CEF_PATH "$(CARGO_BIN)" build -p vmux_desktop --features dev
	@identity="$$(./scripts/ensure-local-codesign-identity.sh)" && \
	APPLE_SIGNING_IDENTITY="$$identity" \
	APP_BINARY="target/debug/vmux_desktop" \
	HELPER_BINARY="$(CEF_DEBUG_RENDER)" \
	./scripts/sign-dev-mac.sh
	@for pid in $$(pgrep -f "target/debug/vmux_desktop" 2>/dev/null); do \
		ps eww -p $$pid 2>/dev/null | grep -q "VMUX_PROFILE=$(VMUX_PROFILE)" && kill $$pid 2>/dev/null || true; \
	done
	@for pid in $$(pgrep -f "bevy_cef_debug_render_process" 2>/dev/null); do \
		ps eww -p $$pid 2>/dev/null | grep -q "VMUX_PROFILE=$(VMUX_PROFILE)" && kill $$pid 2>/dev/null || true; \
	done
	@rust_target_libdir="$$(rustc --print target-libdir)" && \
	dylib_path="$$rust_target_libdir:$(CURDIR)/target/debug/deps" && \
	if [ -n "$${DYLD_LIBRARY_PATH:-}" ]; then \
		dylib_path="$$dylib_path:$$DYLD_LIBRARY_PATH"; \
	fi; \
	exec env -u CEF_PATH DYLD_LIBRARY_PATH="$$dylib_path" VMUX_PROFILE="$(VMUX_PROFILE)" VMUX_TEST="$(VMUX_TEST)" ./target/debug/vmux_desktop

test-app:
	$(MAKE) dev VMUX_PROFILE=gregor VMUX_TEST=1

build: ensure-mac-deps
	env -u CEF_PATH "$(CARGO_BIN)" build -p vmux_desktop -p vmux_cli -p vmux_service --release

-include .env
export

build-local: ensure-mac-deps ensure-package-deps
	./scripts/build-mac-release.sh local

local: build-local
	@sha="$$(git rev-parse --short HEAD)" && \
	open "target/release/Vmux ($$sha).app"

build-release: ensure-mac-deps ensure-package-deps
	./scripts/build-mac-release.sh release

release: build-release
	open "target/release/Vmux.app"

# One-time CEF download (macOS paths; version derived from the cef crate in Cargo.lock)
setup-cef:
	@test -n "$(CEF_VERSION)" || { echo "could not resolve cef crate version from Cargo.lock"; exit 1; }
	"$(CARGO_BIN)" install export-cef-dir@$(CEF_VERSION) --force
	"$(EXPORT_CEF_BIN)" --force "$$HOME/.local/share"

# Build from vmux-patched bevy_cef_core (required when adding CEF schemes such as vmux://).
# Installs into the same path `bevy_cef` debug mode loads on macOS.
install-debug-render-process:
	env -u CEF_PATH "$(CARGO_BIN)" build -p bevy_cef_debug_render_process --features debug
	cp "$(CURDIR)/target/debug/bevy_cef_debug_render_process" \
	  "$(CEF_FRAMEWORK_DIR)/Libraries/bevy_cef_debug_render_process"

# Get workspace packages excluding vendored patches
PKGS = $(shell "$(CARGO_BIN)" metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.manifest_path | test("patches") | not) | .name')

lint:
	@for pkg in $(PKGS); do \
		"$(CARGO_BIN)" fmt -p "$$pkg" -- --check || exit 1; \
	done
	@for pkg in $(PKGS); do \
		env -u CEF_PATH "$(CARGO_BIN)" clippy -p "$$pkg" --all-targets -- -D warnings || exit 1; \
	done

lint-fix:
	@for pkg in $(PKGS); do \
		"$(CARGO_BIN)" fmt -p "$$pkg" || exit 1; \
	done
	@for pkg in $(PKGS); do \
		env -u CEF_PATH "$(CARGO_BIN)" clippy -p "$$pkg" --all-targets --fix --allow-dirty --allow-staged -- -D warnings || exit 1; \
	done

test:
	env -u CEF_PATH "$(CARGO_BIN)" test --workspace --exclude bevy_cef_core

# Reset vmux *dev* storage for a clean test. Removes the layout store, session,
# logs, the saved profile display name, and stale dev service sockets (all
# profiles). KEEPS ~/.vmux (settings + space working dirs) and the dev browser
# profiles (logins/cache).
cleanup:
	@pkill -f "target/debug/vmux_desktop" 2>/dev/null || true
	@pkill -f "target/debug/vmux_service" 2>/dev/null || true
	@pkill -f "bevy_cef_debug_render_process" 2>/dev/null || true
	@case "$$(uname -s)" in \
		Darwin) base="$$HOME/Library/Application Support/Vmux" ;; \
		*) base="$${TMPDIR:-/tmp}/Vmux" ;; \
	esac; dev="$$base/dev"; cfg="$$HOME/.vmux"; \
	rm -f "$$dev/store.ron" "$$dev/store.version"; \
	rm -f "$$dev"/store.ron.*.bak "$$dev"/store.version.bak-*; \
	rm -f "$$dev/profiles/"*/session.ron; \
	rm -f "$$cfg/profiles/"*/display_name; \
	rm -rf "$$dev/logs"; \
	rm -f "$$base/services/"vmux-dev.* "$$base/services/"vmux-dev-*; \
	echo "cleanup: reset vmux dev storage (kept ~/.vmux settings + spaces + dev browser profiles)"

# Website
build-website-css:
	cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify
	rm -rf website/public/api && mkdir -p website/public/api && cp docs/api/*.json website/public/api/

# Regenerate the committed API model from in-code rustdoc (nightly).
api-docs:
	cd vmux_docs && cargo run -- --out ../docs/api

website: build-website-css
	@cd website && { \
		tailwindcss -i tailwind.input.css -o public/style.css --watch & \
		WATCHER_PID=$$!; \
		trap "kill $$WATCHER_PID 2>/dev/null || true" EXIT INT TERM; \
		"$(DX_BIN)" serve --platform web; \
	}

build-website-release: build-website-css
	cd website && rm -rf target/dx && "$(DX_BIN)" build --platform web --ssg --release
	cp website/target/dx/vmux_website/release/web/public/_home/index.html website/target/dx/vmux_website/release/web/public/index.html
	rm -rf website/target/dx/vmux_website/release/web/public/_home

# Friendly prerequisite report (colors / emoji when terminal); README: make doctor
doctor:
	@chmod +x scripts/doctor-mac.sh
	@CARGO_BIN="$(CARGO_BIN)" RUSTUP_BIN="$(RUSTUP_BIN)" EXPORT_CEF_BIN="$(EXPORT_CEF_BIN)" \
		DX_BIN="$(DX_BIN)" CARGO_PACKAGER_BIN="$(CARGO_PACKAGER_BIN)" \
		BEVY_CEF_BUNDLE_APP_BIN="$(BEVY_CEF_BUNDLE_APP_BIN)" CEF_FRAMEWORK_DIR="$(CEF_FRAMEWORK_DIR)" \
		CEF_DEBUG_RENDER="$(CEF_DEBUG_RENDER)" ./scripts/doctor-mac.sh

# Non-interactive bootstrap so `make dev` works even after dependency bumps.
ensure-mac-deps:
	@echo "Checking build dependencies..."
	@if [ ! -x "$(CARGO_BIN)" ]; then \
		echo "cargo not found at $(CARGO_BIN). Install rustup first: https://rustup.rs/"; \
		exit 1; \
	fi
	@if [ ! -x "$(RUSTUP_BIN)" ]; then \
		echo "rustup not found at $(RUSTUP_BIN). Install rustup first: https://rustup.rs/"; \
		exit 1; \
	fi
	@if [ ! -d "$(CEF_FRAMEWORK_DIR)" ]; then \
		echo "CEF framework not found at $(CEF_FRAMEWORK_DIR). Run: make setup-cef"; \
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

ensure-package-deps:
	@echo "Checking packaging dependencies..."
	@cp_version="$$( ("$(CARGO_PACKAGER_BIN)" --version 2>/dev/null || true) | awk '{print $$2}' )"; \
	if [ "$$cp_version" != "$(CARGO_PACKAGER_VERSION)" ]; then \
		echo "Installing cargo-packager $(CARGO_PACKAGER_VERSION) (found: $${cp_version:-missing})..."; \
		"$(CARGO_BIN)" install --path patches/cargo-packager-0.11.8 --locked --force; \
	fi
	@bcb_version="$$( ("$(BEVY_CEF_BUNDLE_APP_BIN)" --version 2>/dev/null || true) | awk '{print $$2}' )"; \
	if [ "$$bcb_version" != "$(BEVY_CEF_BUNDLE_APP_VERSION)" ]; then \
		echo "Installing bevy_cef_bundle_app $(BEVY_CEF_BUNDLE_APP_VERSION) (found: $${bcb_version:-missing})..."; \
		"$(CARGO_BIN)" install bevy_cef_bundle_app --locked --version "$(BEVY_CEF_BUNDLE_APP_VERSION)"; \
	fi

ensure-codesign-deps:
	@echo "Checking codesigning dependencies..."
	@if ! command -v openssl >/dev/null 2>&1; then \
		echo "openssl not found. Install it with: brew install openssl"; \
		exit 1; \
	fi
	@if ! command -v security >/dev/null 2>&1; then \
		echo "security not found. This target requires macOS Keychain tooling."; \
		exit 1; \
	fi
	@if ! command -v codesign >/dev/null 2>&1; then \
		echo "codesign not found. Install Xcode Command Line Tools first."; \
		exit 1; \
	fi

setup-hooks:
	./scripts/setup-hooks.sh
