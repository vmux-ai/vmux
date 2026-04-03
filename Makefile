.PHONY: run-mac run-windows run-doctor build-mac-debug build-windows-debug build bundle-mac setup-cef setup-windows install-debug-render-process doctor-mac doctor-windows ensure-run-mac-deps ensure-run-windows-deps ensure-run-windows-build-deps

CARGO_BIN := $(or $(shell command -v cargo 2>/dev/null),$(HOME)/.cargo/bin/cargo)
RUSTUP_BIN := $(or $(shell command -v rustup 2>/dev/null),$(HOME)/.cargo/bin/rustup)
EXPORT_CEF_BIN := $(or $(shell command -v export-cef-dir 2>/dev/null),$(HOME)/.cargo/bin/export-cef-dir)
DX_BIN := $(or $(shell command -v dx 2>/dev/null),$(HOME)/.cargo/bin/dx)
DX_VERSION := 0.7.4
CEF_FRAMEWORK_DIR := $(HOME)/.local/share/Chromium Embedded Framework.framework
CEF_DEBUG_RENDER := $(CEF_FRAMEWORK_DIR)/Libraries/bevy_cef_debug_render_process

run-mac: build-mac-debug
	exec env -u CEF_PATH ./target/debug/vmux_desktop

build-mac-debug: ensure-run-mac-deps
	env -u CEF_PATH "$(CARGO_BIN)" build -p vmux_desktop --features debug

ifeq ($(OS),Windows_NT)
ensure-run-windows-build-deps:
	powershell -NoProfile -ExecutionPolicy Bypass -File scripts/doctor-windows.ps1 -BuildDeps

build-windows-debug: ensure-run-windows-build-deps
	cmd /C "set CEF_PATH=&& "$(CARGO_BIN)" build -p vmux_desktop --features debug"

run-windows: build-windows-debug
	powershell -NoProfile -ExecutionPolicy Bypass -Command "Remove-Item Env:CEF_PATH -ErrorAction SilentlyContinue; & (Join-Path -Path (Get-Location) -ChildPath 'target/debug/vmux_desktop.exe')"
endif

build: ensure-run-mac-deps
	env -u CEF_PATH "$(CARGO_BIN)" build -p vmux_desktop --release

bundle-mac:
	chmod +x scripts/bundle-macos.sh
	./scripts/bundle-macos.sh

setup-cef:
	"$(CARGO_BIN)" install export-cef-dir@145.6.1+145.0.28 --force
	"$(EXPORT_CEF_BIN)" --force "$$HOME/.local/share"

install-debug-render-process:
	"$(CARGO_BIN)" install bevy_cef_debug_render_process
	cp "$$HOME/.cargo/bin/bevy_cef_debug_render_process" \
	  "$$HOME/.local/share/Chromium Embedded Framework.framework/Libraries/bevy_cef_debug_render_process"

ifeq ($(OS),Windows_NT)
run-doctor: doctor-windows
else
run-doctor: doctor-mac
endif

doctor-windows:
	powershell -NoProfile -ExecutionPolicy Bypass -File scripts/doctor-windows.ps1

ensure-run-windows-deps:
	powershell -NoProfile -ExecutionPolicy Bypass -File scripts/doctor-windows.ps1 -Install

setup-windows:
	powershell -NoProfile -ExecutionPolicy Bypass -File scripts/doctor-windows.ps1 -CefOnly

doctor-mac:
	@chmod +x scripts/doctor-mac.sh
	@CARGO_BIN="$(CARGO_BIN)" RUSTUP_BIN="$(RUSTUP_BIN)" EXPORT_CEF_BIN="$(EXPORT_CEF_BIN)" \
		DX_BIN="$(DX_BIN)" CEF_FRAMEWORK_DIR="$(CEF_FRAMEWORK_DIR)" \
		CEF_DEBUG_RENDER="$(CEF_DEBUG_RENDER)" ./scripts/doctor-mac.sh

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
