/** @type {import('tailwindcss').Config} */
module.exports = {
  presets: [require("../vmux_ui/tailwind.preset.js")],
  content: ["./src/**/*.rs", "./assets/**/*.html"],
  theme: {
    extend: {
      fontFamily: {
        mono: [
          '"JetBrainsMono NF"',
          'ui-monospace',
          'SFMono-Regular',
          'Menlo',
          'Monaco',
          'Consolas',
          '"Liberation Mono"',
          '"Courier New"',
          'monospace',
        ],
      },
    },
  },
  plugins: [],
};
