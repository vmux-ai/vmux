/** @type {import('tailwindcss').Config} */
module.exports = {
  presets: [require("../vmux_ui/tailwind.preset.js")],
  content: ["./src/**/*.rs", "./assets/**/*.html"],
  theme: {
    extend: {},
  },
  plugins: [],
};
