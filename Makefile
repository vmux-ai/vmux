.PHONY: run-mac run-mac-local run-doctor build-mac-debug sign-mac-debug build build-mac-local build-mac-release package-local-mac package-release-mac setup-cef install-debug-render-process doctor-mac ensure-run-mac-deps ensure-package-deps ensure-codesign-deps run-website build-website build-website-css lint lint-fix fmt clippy test

CARGO_BIN := $(or $(shell command -v cargo 2>/dev/null),$(HOME)/.cargo/bin/cargo)
RUSTUP_BIN := $(or $(shell command -v rustup 2>/dev/null),$(HOME)/.cargo/bin/rustup)
EXPORT_CEF_BIN := $(or $(shell command -v export-cef-dir 2>/dev/null),$(HOME)/.cargo/bin/export-cef-dir)
DX_BIN := $(or $(shell command -v dx 2>/dev/null),$(HOME)/.cargo/bin/dx)
CARGO_PACKAGER_BIN := $(or $(shell command -v cargo-packager 2>/dev/null),$(HOME)/.cargo/bin/cargo-packager)
BEVY_CEF_BUNDLE_APP_BIN := $(or $(shell command -v bevy_cef_bundle_app 2>/dev/null),$(HOME)/.cargo/bin/bevy_cef_bundle_app)
DX_VERSION := 0.7.4
CARGO_PACKAGER_VERSION := 0.11.8
BEVY_CEF_BUNDLE_APP_VERSION := 0.8.1
CEF_FRAMEWORK_DIR := $(HOME)/.local/share/Chromium Embedded Framework.framework
CEF_DEBUG_RENDER := $(CEF_FRAMEWORK_DIR)/Libraries/bevy_cef_debug_render_process

# Header / history / UI library `dist/` folders are built by each crate’s `build.rs` via **`dx build`** when you compile `vmux_desktop` (needs `dioxus-cli` on PATH).

run-mac: build-mac-debug
	exec env -u CEF_PATH ./target/debug/vmux_desktop

build-mac-debug: ensure-run-mac-deps ensure-codesign-deps install-debug-render-process
	env -u CEF_PATH "$(CARGO_BIN)" build -p vmux_desktop --features debug
	@$(MAKE) sign-mac-debug

build: ensure-run-mac-deps
	env -u CEF_PATH "$(CARGO_BIN)" build -p vmux_desktop --release

-include .env
export

run-mac-local: build-mac-local
	open "target/release/Vmux Local.app"

package-local-mac: ensure-run-mac-deps ensure-package-deps
	./scripts/package.sh local

package-release-mac: ensure-run-mac-deps ensure-package-deps
	./scripts/package.sh release

build-mac-local: package-local-mac ensure-codesign-deps
	@identity="$$(./scripts/ensure-local-codesign-identity.sh)" && \
	APPLE_SIGNING_IDENTITY="$$identity" \
	SKIP_NOTARIZE=1 \
	APP_BUNDLE="target/release/Vmux Local.app" \
	./scripts/sign-and-notarize.sh

sign-mac-debug: ensure-codesign-deps
	@identity="$$(./scripts/ensure-local-codesign-identity.sh)" && \
	APPLE_SIGNING_IDENTITY="$$identity" \
	APP_BINARY="target/debug/vmux_desktop" \
	HELPER_BINARY="$(CEF_DEBUG_RENDER)" \
	./scripts/sign-debug-mac.sh

build-mac-release:
	./scripts/build-mac-release.sh release

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
build-website-css:
	cd website && tailwindcss -i tailwind.input.css -o public/style.css --minify

run-website: build-website-css
	@cd website && { \
		tailwindcss -i tailwind.input.css -o public/style.css --watch & \
		WATCHER_PID=$$!; \
		trap "kill $$WATCHER_PID 2>/dev/null || true" EXIT INT TERM; \
		"$(DX_BIN)" serve --platform web; \
	}

build-website: build-website-css
	cd website && "$(DX_BIN)" build --platform web --release

# Friendly prerequisite report (colors / emoji when terminal); README: make run-doctor
run-doctor: doctor-mac

doctor-mac:
	@chmod +x scripts/doctor-mac.sh
	@CARGO_BIN="$(CARGO_BIN)" RUSTUP_BIN="$(RUSTUP_BIN)" EXPORT_CEF_BIN="$(EXPORT_CEF_BIN)" \
		DX_BIN="$(DX_BIN)" CARGO_PACKAGER_BIN="$(CARGO_PACKAGER_BIN)" \
		BEVY_CEF_BUNDLE_APP_BIN="$(BEVY_CEF_BUNDLE_APP_BIN)" CEF_FRAMEWORK_DIR="$(CEF_FRAMEWORK_DIR)" \
		CEF_DEBUG_RENDER="$(CEF_DEBUG_RENDER)" ./scripts/doctor-mac.sh

# Non-interactive bootstrap so `make run-mac` works even after dependency bumps.
ensure-run-mac-deps:
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
		"$(CARGO_BIN)" install cargo-packager --locked --version "$(CARGO_PACKAGER_VERSION)"; \
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
