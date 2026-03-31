#!/usr/bin/env node
/**
 * Inlines dist/history.css into dist/index.html so CEF loads one document shell.
 * A separate <link href="history.css"> often 404s (missing file, incomplete dist) and yields
 * unstyled blue links (browser defaults) even when Tailwind output exists.
 */
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(__dirname, "..", "dist");
const cssPath = path.join(root, "history.css");
const htmlPath = path.join(root, "index.html");

if (!fs.existsSync(cssPath)) {
  console.error(
    "inline-history-css: dist/history.css missing — run tailwind first (tailwindcss … -o dist/history.css)",
  );
  process.exit(1);
}
if (!fs.existsSync(htmlPath)) {
  console.error(
    "inline-history-css: dist/index.html missing — run a native `cargo build -p vmux_history` (see crates/vmux_history/build.rs)",
  );
  process.exit(1);
}

const css = fs.readFileSync(cssPath, "utf8");
let html = fs.readFileSync(htmlPath, "utf8");

const LINK_RE = /\s*<link\s+rel="stylesheet"\s+href="history\.css"\s*\/?>\s*/;

if (/<style id="vmux-history-inline">/.test(html)) {
  html = html.replace(
    /<style id="vmux-history-inline">[\s\S]*?<\/style>/,
    `<style id="vmux-history-inline">\n${css}\n</style>`,
  );
} else if (LINK_RE.test(html)) {
  html = html.replace(
    LINK_RE,
    `\n  <style id="vmux-history-inline">\n${css}\n  </style>\n`,
  );
} else {
  console.error(
    "inline-history-css: index.html has neither <link href=\"history.css\"> nor <style id=\"vmux-history-inline\">",
  );
  process.exit(1);
}

fs.writeFileSync(htmlPath, html);
console.log("inline-history-css: inlined history.css into index.html");
