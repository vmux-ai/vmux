/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.{rs,html}"],
  theme: {
    extend: {
      fontSize: {
        status: ["11px", { lineHeight: "1.2" }],
      },
      fontFamily: {
        status: [
          "ui-monospace",
          "SFMono-Regular",
          "SF Mono",
          "Menlo",
          "Consolas",
          "Liberation Mono",
          "monospace",
        ],
      },
    },
  },
  plugins: [],
};
