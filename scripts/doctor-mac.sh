#!/usr/bin/env bash
# macOS dev environment check for `make run-mac`.
# Respects NO_COLOR=1 and skips colors when stdout is not a TTY.

set -euo pipefail

CARGO_BIN="${CARGO_BIN:-$(command -v cargo 2>/dev/null || echo "${HOME}/.cargo/bin/cargo")}"
RUSTUP_BIN="${RUSTUP_BIN:-$(command -v rustup 2>/dev/null || echo "${HOME}/.cargo/bin/rustup")}"
EXPORT_CEF_BIN="${EXPORT_CEF_BIN:-$(command -v export-cef-dir 2>/dev/null || echo "${HOME}/.cargo/bin/export-cef-dir")}"
WASM_BINDGEN_BIN="${WASM_BINDGEN_BIN:-$(command -v wasm-bindgen 2>/dev/null || echo "${HOME}/.cargo/bin/wasm-bindgen")}"
CEF_FRAMEWORK_DIR="${CEF_FRAMEWORK_DIR:-${HOME}/.local/share/Chromium Embedded Framework.framework}"
CEF_DEBUG_RENDER="${CEF_DEBUG_RENDER:-${CEF_FRAMEWORK_DIR}/Libraries/bevy_cef_debug_render_process}"

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
total=11

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
printf ' %s%s\n' "${DIM}" "Checking everything you need for ${BOLD}make run-mac${RESET}${DIM}…${RESET}"
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
	bad_line "CEF base dir not writable: ${HOME}/.local/share"
	tip "Run: ${BOLD}chmod u+rwx \"${HOME}/.local/share\"${RESET}"
else
	ok_line "CEF install base is writable — ${HOME}/.local/share"
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

section "Node & WASM"
if command -v node >/dev/null 2>&1; then
	ok_line "node"
else
	bad_line "node not found"
	tip "Run: ${BOLD}brew install node${RESET}"
fi

if command -v npm >/dev/null 2>&1; then
	ok_line "npm"
else
	bad_line "npm not found"
	tip "Run: ${BOLD}brew install node${RESET}"
fi

if [[ -x "${WASM_BINDGEN_BIN}" ]]; then
	ok_line "wasm-bindgen CLI — ${WASM_BINDGEN_BIN}"
else
	bad_line "wasm-bindgen CLI not found"
	tip "Run: ${BOLD}\"${CARGO_BIN}\" install wasm-bindgen-cli${RESET}"
fi

section "CEF runtime (debug)"
if [[ -d "${CEF_FRAMEWORK_DIR}" ]]; then
	ok_line "CEF framework — ${CEF_FRAMEWORK_DIR}"
else
	bad_line "CEF framework missing"
	tip "Run: ${BOLD}make setup-cef${RESET}"
fi

if [[ -x "${CEF_DEBUG_RENDER}" ]]; then
	ok_line "bevy_cef_debug_render_process"
else
	bad_line "bevy_cef_debug_render_process missing"
	tip "Run: ${BOLD}make install-debug-render-process${RESET}"
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
	printf ' %sYou are clear to run:%s ${BOLD}make run-mac${RESET}\n' "${DIM}" "${RESET}"
	if [[ "${warn}" -gt 0 ]]; then
		printf '\n %s(One optional item above — finish CEF setup when you are ready.)%s\n' "${YELLOW}" "${RESET}"
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
