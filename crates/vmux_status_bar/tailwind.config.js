/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.{rs,html}"],
  theme: {
    extend: {
      colors: {
        tmux: {
          bg: "#4e9a06",
          fg: "#0a0a0a",
          dim: "rgba(10, 10, 10, 0.55)",
          inv: {
            bg: "#0a0a0a",
            fg: "#8ae234",
          },
        },
      },
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
