<h1 align="center">Vmux</h1>
<p align="center"><b>One prompt. Anything, done.</b> The browser + IDE that get sh*t done — booking a flight, building a website, opening a PR, all handled by your agents while you watch.</p>

<p align="center">
  <img src="icon.png" alt="Vmux icon" width="256" />
</p>

## Features

- **Co-work with agents** — People and agents build side by side in one shared space — from hands-on pairing to full autonomy, you set the balance.
- **Browser simplicity, tmux power** — Looks like the browser you already know; split, stack, and tile panes like tmux underneath.
- **IDE power underneath** — Keyboard-driven workflows and deep environment control — and agents drive the whole workspace over MCP.
- **3D workspace** — Powered by Bevy. Flip your panes into a live, GPU-rendered 3D scene — same workspace, still interactive.

## Install

```sh
curl -fsSL https://vmux.ai/install | sh
```

Requires macOS 13.0 (Ventura) or later.

## Development

```sh
# Check prerequisites
make doctor

# Run macOS app
make
```

See [Makefile](Makefile) for all targets.

## License

Copyright (c) 2024-2025 Junichi Sugiura

Licensed under the [GNU General Public License v3.0 or later](LICENSE).
