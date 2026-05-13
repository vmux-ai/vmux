#!/usr/bin/env bash
# macOS dev environment check for `make dev`.
# Respects NO_COLOR=1 and skips colors when stdout is not a TTY.

set -euo pipefail

CARGO_BIN="${CARGO_BIN:-$(command -v cargo 2>/dev/null || echo "${HOME}/.cargo/bin/cargo")}"
RUSTUP_BIN="${RUSTUP_BIN:-$(command -v rustup 2>/dev/null || echo "${HOME}/.cargo/bin/rustup")}"
EXPORT_CEF_BIN="${EXPORT_CEF_BIN:-$(command -v export-cef-dir 2>/dev/null || echo "${HOME}/.cargo/bin/export-cef-dir")}"
DX_BIN="${DX_BIN:-$(command -v dx 2>/dev/null || echo "${HOME}/.cargo/bin/dx")}"
CARGO_PACKAGER_BIN="${CARGO_PACKAGER_BIN:-$(command -v cargo-packager 2>/dev/null || echo "${HOME}/.cargo/bin/cargo-packager")}"
BEVY_CEF_BUNDLE_APP_BIN="${BEVY_CEF_BUNDLE_APP_BIN:-$(command -v bevy_cef_bundle_app 2>/dev/null || echo "${HOME}/.cargo/bin/bevy_cef_bundle_app")}"
OPENSSL_BIN="${OPENSSL_BIN:-$(command -v openssl 2>/dev/null || true)}"
CEF_FRAMEWORK_DIR="${CEF_FRAMEWORK_DIR:-${HOME}/.local/share/Chromium Embedded Framework.framework}"
VMUX_LOCAL_SIGNING_IDENTITY="${VMUX_LOCAL_SIGNING_IDENTITY:-Vmux Dev}"

if [[ -t 1 ]] && [[ -z "${NO_COLOR:-}" ]]; then
	RED=$'\033[31m'
	GREEN=$'\033[32m'
	YELLOW=$'\033[33m'
	BLUE=$'\033[34m'
	MAGENTA=$'\033[35m'
	CYAN=$'\033[36m'
	BOLD=$'\033[1m'
	DIM=$'\033[2m'
	RESET=$'\033[0m'
else
	RED="" GREEN="" YELLOW="" BLUE="" MAGENTA="" CYAN="" BOLD="" DIM="" RESET=""
fi

pass=0
fail=0
warn=0
current=0
total=14

bar() {
	printf '%s\n' "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
}

section() {
	printf '\n%s %s%s%s\n' "${MAGENTA}▸${RESET}" "${BOLD}" "$1" "${RESET}"
}

tip() {
	printf '  %s %s\n' "${DIM}→${RESET}" "$1"
}

ok_line() {
	((current += 1)) || true
	((pass += 1)) || true
	printf '  %s %s[%2d/%d]%s %s\n' "${GREEN}✓${RESET}" "${DIM}" "${current}" "${total}" "${RESET}" "$1"
}

warn_line() {
	((current += 1)) || true
	((warn += 1)) || true
	printf '  %s %s[%2d/%d]%s %s\n' "${YELLOW}⚠${RESET}" "${DIM}" "${current}" "${total}" "${RESET}" "$1"
}

bad_line() {
	((current += 1)) || true
	((fail += 1)) || true
	printf '  %s %s[%2d/%d]%s %s\n' "${RED}✗${RESET}" "${DIM}" "${current}" "${total}" "${RESET}" "$1"
}

printf '\n'
bar
printf ' %s🩺  Vmux Doctor%s  %s(macOS)%s\n' "${BOLD}" "${RESET}" "${CYAN}" "${RESET}"
printf ' %s%s\n' "${DIM}" "Checking everything you need for ${BOLD}make dev${RESET}${DIM}…${RESET}"
bar
printf '\n'

section "CEF & paths"
if [[ -x "${EXPORT_CEF_BIN}" ]]; then
	ok_line "export-cef-dir — ${EXPORT_CEF_BIN}"
else
	warn_line "export-cef-dir not installed yet (expected before setup-cef)"
	tip "Run: ${BOLD}make setup-cef${RESET}"
fi

if [[ ! -d "${HOME}/.local/share" ]]; then
	bad_line "CEF base dir missing: ${HOME}/.local/share"
	tip "Run: ${BOLD}mkdir -p \"${HOME}/.local/share\"${RESET}"
elif [[ ! -w "${HOME}/.local/share" ]]; then
	warn_line "CEF base dir not writable: ${HOME}/.local/share"
	tip "Only needed when running: ${BOLD}make setup-cef${RESET}"
else
	ok_line "CEF install base is writable — ${HOME}/.local/share"
fi

if [[ -d "${CEF_FRAMEWORK_DIR}" ]]; then
	ok_line "CEF framework — ${CEF_FRAMEWORK_DIR}"
else
	bad_line "CEF framework missing"
	tip "Run: ${BOLD}make setup-cef${RESET}"
fi

section "Rust toolchain"
if [[ -x "${CARGO_BIN}" ]]; then
	ok_line "cargo — ${CARGO_BIN}"
else
	bad_line "cargo not found"
	tip "Install: ${BLUE}https://rustup.rs/${RESET}"
	tip "Or prepend PATH: ${BOLD}PATH=\"\${HOME}/.cargo/bin:\$PATH\"${RESET}"
fi

if [[ -x "${RUSTUP_BIN}" ]]; then
	if "${RUSTUP_BIN}" target list --installed | grep -qx "wasm32-unknown-unknown"; then
		ok_line "rust target wasm32-unknown-unknown"
	else
		bad_line "rust target wasm32-unknown-unknown missing"
		tip "Run: ${BOLD}\"${RUSTUP_BIN}\" target add wasm32-unknown-unknown${RESET}"
	fi
else
	bad_line "rustup not found (needed for wasm target)"
	tip "Install: ${BLUE}https://rustup.rs/${RESET}"
fi

section "Native build tools"
if command -v cmake >/dev/null 2>&1; then
	ok_line "cmake"
else
	bad_line "cmake not found"
	tip "Run: ${BOLD}brew install cmake${RESET}"
fi

if command -v ninja >/dev/null 2>&1; then
	ok_line "ninja"
else
	bad_line "ninja not found"
	tip "Run: ${BOLD}brew install ninja${RESET}"
fi

section "Dioxus CLI (dx) & WASM bundles"
if [[ -x "${DX_BIN}" ]]; then
	ok_line "dx (dioxus-cli) — ${DX_BIN}"
else
	bad_line "dx not found"
	tip "Run: ${BOLD}\"${CARGO_BIN}\" install dioxus-cli --locked --version 0.7.4${RESET}"
	tip "Dioxus UIs build via ${BOLD}dx build${RESET} (Tailwind + wasm-bindgen are bundled; Node.js not required)"
fi

section "Packaging & signing"
if [[ -x "${CARGO_PACKAGER_BIN}" ]]; then
	ok_line "cargo-packager — ${CARGO_PACKAGER_BIN}"
else
	bad_line "cargo-packager not found"
	tip "Run: ${BOLD}\"${CARGO_BIN}\" install cargo-packager --locked${RESET}"
fi

if [[ -x "${BEVY_CEF_BUNDLE_APP_BIN}" ]]; then
	ok_line "bevy_cef_bundle_app — ${BEVY_CEF_BUNDLE_APP_BIN}"
else
	bad_line "bevy_cef_bundle_app not found"
	tip "Run: ${BOLD}\"${CARGO_BIN}\" install bevy_cef_bundle_app --locked${RESET}"
fi

if [[ -n "${OPENSSL_BIN}" && -x "${OPENSSL_BIN}" ]]; then
	ok_line "openssl — ${OPENSSL_BIN}"
else
	bad_line "openssl not found"
	tip "Run: ${BOLD}brew install openssl${RESET}"
fi

if command -v security >/dev/null 2>&1; then
	ok_line "security"
else
	bad_line "security not found"
	tip "This target requires macOS Keychain tooling"
fi

if command -v codesign >/dev/null 2>&1; then
	ok_line "codesign"
else
	bad_line "codesign not found"
	tip "Install Xcode Command Line Tools first"
fi

if security find-identity -v -p codesigning 2>/dev/null | grep -Fq "\"${VMUX_LOCAL_SIGNING_IDENTITY}\""; then
	ok_line "local codesigning identity — ${VMUX_LOCAL_SIGNING_IDENTITY}"
else
	warn_line "local codesigning identity will be created on first make dev"
	tip "Expected prompt: allow Keychain/codesign setup once, preferably with Touch ID"
fi

printf '\n'
bar
printf ' %sSummary%s  ' "${BOLD}" "${RESET}"
printf '%s✓ %d ok%s' "${GREEN}" "${pass}" "${RESET}"
if [[ "${warn}" -gt 0 ]]; then
	printf '  %s⚠ %d note%s' "${YELLOW}" "${warn}" "${RESET}"
fi
if [[ "${fail}" -gt 0 ]]; then
	printf '  %s✗ %d fix%s' "${RED}" "${fail}" "${RESET}"
fi
printf '\n'
bar
printf '\n'

if [[ "${fail}" -eq 0 ]]; then
	printf ' %s🎉 All required checks passed!%s\n' "${GREEN}${BOLD}" "${RESET}"
	printf ' %sYou are clear to run:%s %smake dev%s\n' "${DIM}" "${RESET}" "${BOLD}" "${RESET}"
	if [[ "${warn}" -gt 0 ]]; then
		printf '\n %s(Warning items above are optional or handled on first run.)%s\n' "${YELLOW}" "${RESET}"
	fi
	printf '\n'
	exit 0
fi

printf ' %sSome requirements are missing.%s Fix the %s✗%s items above, then run this again.\n' "${RED}" "${RESET}" "${RED}" "${RESET}"
printf '\n'
if [[ -t 0 ]] && [[ -t 1 ]] && [[ -z "${VMUX_DOCTOR_NO_WAIT:-}" ]]; then
	printf ' %s%sPress Enter when you are ready to continue…%s ' "${DIM}" "${RESET}"
	read -r _ || true
fi
exit 1
