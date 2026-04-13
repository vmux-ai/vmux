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

### Feature

- `vmux_ui`
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

- Setup CI: lint, test, build
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

### Architecture refactoring

- Tab
- Pane
- Browser
    - Loading indicator
    - Navigation
    - History
- Native Menu
- Keyboard Input / Key bindings
- Mouse Input
- Profile / Session
