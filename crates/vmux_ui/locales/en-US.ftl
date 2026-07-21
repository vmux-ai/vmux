locale-name = English (US)
common-open = Open
common-close = Close
common-install = Install
common-uninstall = Uninstall
common-update = Update
common-retry = Retry
common-refresh = Refresh
common-remove = Remove
common-enable = Enable
common-disable = Disable
common-new = New
common-active = active
common-running = running
common-done = done
common-failed = Failed
common-installed = Installed
common-items = { $count ->
    [one] { $count } item
   *[other] { $count } items
}
start-title = Start
start-tagline = One prompt. Anything, done.

agents-title = Agents
agents-search = Search ACP and CLI agents…
agents-empty = No matching agents
agents-empty-detail = Try a name, runtime, or ACP/CLI.
agents-install-failed = Install failed
agents-updating = Updating…
agents-retrying = Retrying…
agents-preparing = Preparing…

extensions-title = Extensions
extensions-search = Search installed or Chrome Web Store…
extensions-relaunch = Relaunch to apply
extensions-empty = No extensions installed
extensions-no-match = No matching extensions
extensions-empty-detail = Search the Chrome Web Store above and press Return.
extensions-no-match-detail = Try another name or extension ID.
extensions-on = On
extensions-off = Off
extensions-enable-confirm = Enable { $name }?
extensions-enable-permissions = Enable { $name } and allow:

lsp-title = Language Servers
lsp-search = Search language servers, linters, formatters…
lsp-loading = Loading catalog…
lsp-empty = No matching language servers
lsp-empty-detail = Try another language, linter, or formatter.
lsp-needs = needs { $tool }
lsp-status-available = Available
lsp-status-on-path = On PATH
lsp-status-installing = Installing…
lsp-status-installed = Installed
lsp-status-outdated = Update available
lsp-status-running = Running
lsp-status-failed = Failed

spaces-title = Spaces
spaces-new-placeholder = New space name
spaces-empty = No spaces
spaces-default-name = Space { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tabs
}
spaces-delete = Delete space

team-title = Team
team-just-you = Just you in this space
team-agents = { $count ->
    [one] You and 1 agent
   *[other] You and { $count } agents
}
team-empty = No one here yet
team-you = You
team-agent = Agent

services-title = Background Services
services-processes = { $count ->
    [one] 1 process
   *[other] { $count } processes
}
services-kill-all = Kill All
services-not-running = Service is not running
services-start-with = Start with:
services-empty = No active processes
services-filter = Filter processes…
services-no-match = No matching processes
services-connected = Connected
services-disconnected = Disconnected
services-attached = attached
services-kill = Kill
services-memory = Memory
services-size = Size
services-shell = Shell

error-title = Error

history-search = Search history
history-clear-all = Clear all
history-clear-confirm = Clear all history?
history-clear-warning = This cannot be undone.
history-cancel = Cancel
history-today = Today
history-yesterday = Yesterday
history-days-ago = { $count } days ago
history-day-offset = Day -{ $count }

settings-title = Settings
settings-loading = Loading settings…
settings-stored = Stored in ~/.vmux/settings.ron
settings-other = Other
settings-software-update = Software Update
settings-check-updates = Check for Updates
settings-check-updates-hint = Checks automatically on launch and every hour when Auto-update is enabled.
settings-update-unavailable = Unavailable
settings-update-unavailable-hint = Updater is not included in this build.
settings-update-checking = Checking…
settings-update-checking-hint = Checking for updates…
settings-update-check-again = Check Again
settings-update-current = Vmux is up to date.
settings-update-downloading = Downloading…
settings-update-downloading-hint = Downloading Vmux { $version }…
settings-update-installing = Installing…
settings-update-installing-hint = Installing Vmux { $version }…
settings-update-ready = Update Ready
settings-update-ready-hint = Vmux { $version } is ready. Restart to apply it.
settings-update-try-again = Try Again
settings-update-failed = Unable to check for updates.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Press a key…
settings-saved = Saved
settings-record-key = Click to record a new key combo

tray-open-window = Open Window
tray-close-window = Close Window
tray-pause-recording = Pause Recording
tray-resume-recording = Resume Recording
tray-finish-recording = Finish Recording
tray-quit = Quit Vmux

composer-attach-files = Attach files (/upload)
composer-remove-attachment = Remove attachment
composer-send-hint = Enter to send · Shift+Enter for new line

layout-back = Back
layout-forward = Forward
layout-reload = Reload
layout-bookmark-page = Bookmark this page
layout-remove-bookmark = Remove bookmark
layout-pin-page = Pin this page
layout-unpin-page = Unpin this page
layout-manage-extensions = Manage extensions
layout-new-stack = New Stack
layout-close-tab = Close tab
layout-bookmark = Bookmark
layout-pin = Pin
layout-new-tab = New tab
layout-team = Team

command-switch-space = Switch space…
command-search-ask = Search or ask…
command-new-tab-placeholder = Search or type a URL, or select Terminal…
command-placeholder = Type a URL, search tabs, or > for commands…
command-composer-placeholder = Type / for commands or @ for media
command-send = Send (Enter)
command-terminal = Terminal
command-open-terminal = Open in Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tabs
}
command-prompt = Prompt
command-new-tab = New tab
command-search = Search
command-open-value = Open “{ $value }”
command-search-value = Search “{ $value }”

schema-appearance = Appearance
schema-general = General
schema-layout = Layout
schema-layout-detail = Window, panes, sidebar, and focus ring.
schema-agent = Agent
schema-agent-detail = Agent behavior and tool permissions.
schema-shortcuts = Shortcuts
schema-shortcuts-detail = Read-only view. Edit settings.ron directly to change bindings.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Mode
schema-mode-detail = Color scheme for web pages. Device follows your system.
schema-device = Device
schema-light = Light
schema-dark = Dark
schema-language = Language
schema-language-detail = Use system, en-US, ja, or any BCP 47 tag with a matching ~/.vmux/locales/<tag>.ftl catalog.
schema-auto-update = Auto-update
schema-auto-update-detail = Check for and install updates on launch and every hour.
schema-startup-url = Startup URL
schema-startup-url-detail = Empty opens the command bar prompt.
schema-search-engine = Search engine
schema-search-engine-detail = Used for web searches from Start and the command bar.
schema-window = Window
schema-pane = Pane
schema-side-sheet = Side sheet
schema-focus-ring = Focus ring
schema-run-placement = Allow run placement override
schema-run-placement-detail = Let agents choose run pane mode, direction, and anchor.
schema-leader = Leader
schema-leader-detail = Prefix key for chord shortcuts.
schema-chord-timeout = Chord timeout
schema-chord-timeout-detail = Milliseconds before a chord prefix expires.
schema-bindings = Bindings
schema-confirm-close = Confirm close
schema-confirm-close-detail = Prompt before closing a terminal with a running process.
schema-default-theme = Default theme
schema-default-theme-detail = Name of the active theme from the themes list.
