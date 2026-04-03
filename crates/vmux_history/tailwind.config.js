/** @type {import('tailwindcss').Config} */
// Consumed by `dioxus-cli` (`dx build`) when compiling `assets/input.css` (see `build.rs`).
// Markup + utilities: `src/**/*.rs`, shell: `assets/index.html`, optional `dist/index.html` when present.
module.exports = {
  presets: [require("../vmux_ui/tailwind.preset.js")],
  content: [
    "./src/**/*.rs",
    "./assets/index.html",
    "./dist/index.html",
    // Shared webview helpers (`UiInputShell`, `Button`, …) live in vmux_ui; scan them so utilities are not purged.
    "../vmux_ui/src/**/*.rs",
  ],
  plugins: [],
};
