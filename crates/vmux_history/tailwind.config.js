/** @type {import('tailwindcss').Config} */
// Run from this directory (`npm run build:css` after `web_dist/index.html` exists; see `build.rs`).
// Markup + utilities: `src/**/*.rs`, shell: `assets/index.html` (copied to `web_dist/` before Tailwind in builds).
module.exports = {
  content: ["./src/**/*.rs", "./assets/index.html", "./web_dist/index.html"],
  theme: {
    extend: {},
  },
  plugins: [],
};
