## v0.1: Browser, Terminal, and Tmux style tiling window manager

### Bug

- `vmux_history`
    - Search icon placement
- `vmux_input`
    - Keybindings doesn't work the same in FR keyboard (e.g. ctrl+b,%)

### Improvement

- `vmux_command`
    - Better suggestion when no search input is typed
        - Most relavent, most popular etc.
    - Assign appropriate icon for each action type
    - [Key mode plan](./plans/keybindings.plan.md)
- `vmux_layout`
    - [Layout refactoring](./plans/layout-naming.plan.md)
- `vmux_webview`
    - Prevent website from getting stretched on NewPane, ClosePane
- `vmux_input`
    - leader key(ctrl+b) should be replacable by settings.ron

### Feature

- `vmux_ui`
    - Refactor existing dioxus based ui plugin (e.g. status_bar, history etc.)
    - Show storybook like ui in pane for debugging
- `vmux_terminal`
    - Add terminal pane to provide terminal emulator
    - MVP
- `vmux_layout`
    - Tmux parity
- `vmux_browser`
    - Chromium Extensions Support
- `vmux_help`
    - UI
- `vmux_settings`
    - UI
- `vmux_onboarding`
    - Welcome flow
    - "Import from Another Browser"
        - Target
            - Bookmarks
            - Logins
            - History
            - Extensions
        - Browser Support
            - Chrome
            - Safari
            - Firefox
            - Brave
            - Edge
            - Opera
            - Opera GX
            - Vivaldi

### Chore

- Deploy to crates.io
- `vmux_desktop`
    - Release bundle
    - Publish app
    - Optional: CI/CD
- `vmux_ui`, `vmux_server`
    - Simplify startup methods
- `vmux_docs`
    - Publish website
- `vmux_cli`
    - Installer
    - `vmux` command

## v0.2: Text Editor, AI Agent

### Feature

- `vmux_editor`
    - Text editor MVP

### Chore

- Updator
    - Handles db migration if necessary

## v0.3: AI Agent

### Feature

- `vmux_ai_agent`
    - Chat based AI agent panel MVP

## v0.X

- Windows support
- Linux support
