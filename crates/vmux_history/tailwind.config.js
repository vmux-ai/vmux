/** @type {import('tailwindcss').Config} */
// Run from this directory (`npm run build:css` after `dist/index.html` exists; see `build.rs`).
// Markup + utilities: `src/**/*.rs`, shell: `assets/index.html` (copied to `dist/` before Tailwind in builds).
module.exports = {
  content: ["./src/**/*.rs", "./assets/index.html", "./dist/index.html"],
  theme: {
    extend: {},
  },
  plugins: [],
};
