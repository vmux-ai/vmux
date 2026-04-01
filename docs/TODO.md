## v0.1: Browser, Terminal, and Tmux style tiling window manager

### Bug

- `vmux_history`
    - Search icon placement
- `vmux_input`
    - Keybindings doesn't work the same in FR keyboard (e.g. ctrl+b,%)
    - leader key(ctrl+b) should be replacable by settings.ron

### Improvement

- `vmux_command`
    - Better suggestion algo when no search input is typed
        - Most relavent, most popular etc.
    - Assign appropriate icon for each action type
    - [Key mode plan](./plans/keybindings.plan.md)
- `vmux_layout`
    - [Layout refactoring](./plans/layout-naming.plan.md)
- `vmux_webview`
    - Prevent website from getting stretched on NewPane, ClosePane

### Feature

- `vmux_ui`
    - Refactor existing dioxus based ui plugin (e.g. status_bar, history etc.)
    - Show storybook like ui in pane for debugging
- `vmux_terminal`
    - Add terminal pane to provide terminal emulator
    - PoC
- `vmux_layout`
    - Tmux parity
- `vmux_browser`
    - Chromium plugin support

## v0.2: Text Editor, AI Agent

### Feature

- `vmux_editor`
    - Text editor
- `vmux_ai_agent`
    - Chat based AI agent panel
