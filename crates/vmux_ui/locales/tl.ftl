common-open = Buksan
common-close = Isara
common-install = I-install
common-uninstall = I-uninstall
common-update = I-update
common-retry = Subukan muli
common-refresh = I-refresh
common-remove = Alisin
common-enable = I-enable
common-disable = I-disable
common-new = Bago
common-active = aktibo
common-running = tumatakbo
common-done = tapos na
common-failed = Nabigo
common-installed = Naka-install
common-items = { $count ->
    [one] { $count } item
   *[other] { $count } item
}
start-title = Simula
start-tagline = Isang prompt. Kahit ano, tapos.

agents-title = Mga Agent
agents-search = Maghanap ng ACP at CLI agent…
agents-empty = Walang tugmang agent
agents-empty-detail = Subukan ang pangalan, runtime, o ACP/CLI.
agents-install-failed = Nabigo ang pag-install
agents-updating = Ina-update…
agents-retrying = Sinusubukan muli…
agents-preparing = Inihahanda…

extensions-title = Mga Extension
extensions-search = Maghanap sa naka-install o sa Chrome Web Store…
extensions-relaunch = I-relaunch para mailapat
extensions-empty = Walang naka-install na extension
extensions-no-match = Walang tugmang extension
extensions-empty-detail = Maghanap sa Chrome Web Store sa itaas at pindutin ang Return.
extensions-no-match-detail = Subukan ang ibang pangalan o extension ID.
extensions-on = Naka-on
extensions-off = Naka-off
extensions-enable-confirm = I-enable ang { $name }?
extensions-enable-permissions = I-enable ang { $name } at payagan ang:

lsp-title = Mga Language Server
lsp-search = Maghanap ng mga language server, linter, formatter…
lsp-loading = Nilo-load ang catalog…
lsp-empty = Walang tugmang language server
lsp-empty-detail = Subukan ang ibang language, linter, o formatter.
lsp-needs = kailangan ang { $tool }
lsp-status-available = Available
lsp-status-on-path = Nasa PATH
lsp-status-installing = Ini-install…
lsp-status-installed = Naka-install
lsp-status-outdated = May update
lsp-status-running = Tumatakbo
lsp-status-failed = Nabigo

spaces-title = Mga Workspace
spaces-new-placeholder = Pangalan ng bagong workspace
spaces-empty = Walang workspace
spaces-default-name = Workspace { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
spaces-delete = I-delete ang workspace

team-title = Team
team-just-you = Ikaw lang sa workspace na ito
team-agents = { $count ->
    [one] Ikaw at 1 agent
   *[other] Ikaw at { $count } agent
}
team-empty = Wala pang tao rito
team-you = Ikaw
team-agent = Agent

services-title = Mga Background Service
services-processes = { $count ->
    [one] 1 proseso
   *[other] { $count } proseso
}
services-kill-all = I-kill Lahat
services-not-running = Hindi tumatakbo ang service
services-start-with = Simulan gamit ang:
services-empty = Walang aktibong proseso
services-filter = I-filter ang mga proseso…
services-no-match = Walang tugmang proseso
services-connected = Konektado
services-disconnected = Disconnected
services-attached = naka-attach
services-kill = I-kill
services-memory = Memory
services-size = Laki
services-shell = Shell

error-title = Error

history-search = Maghanap sa history
history-clear-all = I-clear lahat
history-clear-confirm = I-clear ang buong history?
history-clear-warning = Hindi na ito mababawi.
history-cancel = Kanselahin
history-today = Ngayon
history-yesterday = Kahapon
history-days-ago = { $count } araw ang nakalipas
history-day-offset = Araw -{ $count }

settings-title = Mga Setting
settings-loading = Nilo-load ang mga setting…
settings-stored = Naka-store sa ~/.vmux/settings.ron
settings-other = Iba pa
settings-software-update = Software Update
settings-check-updates = Tingnan kung may update
settings-check-updates-hint = Awtomatikong tumitingin sa launch at kada oras kapag naka-enable ang Auto-update.
settings-update-unavailable = Hindi available
settings-update-unavailable-hint = Hindi kasama ang updater sa build na ito.
settings-update-checking = Tinitingnan…
settings-update-checking-hint = Tinitingnan kung may update…
settings-update-check-again = Tingnan Muli
settings-update-current = Up to date ang Vmux.
settings-update-downloading = Dina-download…
settings-update-downloading-hint = Dina-download ang Vmux { $version }…
settings-update-installing = Ini-install…
settings-update-installing-hint = Ini-install ang Vmux { $version }…
settings-update-ready = Handa na ang Update
settings-update-ready-hint = Handa na ang Vmux { $version }. I-restart para mailapat.
settings-update-try-again = Subukan Muli
settings-update-failed = Hindi matingnan kung may update.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Pindutin ang isang key…
settings-saved = Na-save
settings-record-key = I-click para mag-record ng bagong key combo

tray-open-window = Buksan ang Window
tray-close-window = Isara ang Window
tray-pause-recording = I-pause ang Recording
tray-resume-recording = I-resume ang Recording
tray-finish-recording = Tapusin ang Recording
tray-quit = I-quit ang Vmux

composer-attach-files = Mag-attach ng mga file (/upload)
composer-remove-attachment = Alisin ang attachment

layout-back = Bumalik
layout-forward = Pasulong
layout-reload = I-reload
layout-bookmark-page = I-bookmark ang page na ito
layout-remove-bookmark = Alisin ang bookmark
layout-pin-page = I-pin ang page na ito
layout-unpin-page = I-unpin ang page na ito
layout-manage-extensions = Pamahalaan ang mga extension
layout-new-stack = Bagong Stack
layout-close-tab = Isara ang tab
layout-bookmark = Bookmark
layout-pin = I-pin
layout-new-tab = Bagong tab
layout-team = Team

command-switch-space = Lumipat ng workspace…
command-search-ask = Maghanap o magtanong…
command-new-tab-placeholder = Maghanap o mag-type ng URL, o piliin ang Terminal…
command-placeholder = Mag-type ng URL, maghanap ng tab, o > para sa mga command…
command-composer-placeholder = Mag-type ng / para sa mga command o @ para sa media
command-send = Ipadala (Enter)
command-terminal = Terminal
command-open-terminal = Buksan sa Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
command-prompt = Prompt
command-new-tab = Bagong tab
command-search = Maghanap
command-open-value = Buksan ang “{ $value }”
command-search-value = Hanapin ang “{ $value }”

schema-appearance = Hitsura
schema-general = General
schema-layout = Layout
schema-layout-detail = Window, pane, sidebar, at focus ring.
schema-agent = Agent
schema-agent-detail = Gawi ng agent at mga pahintulot sa tool.
schema-shortcuts = Mga Shortcut
schema-shortcuts-detail = Read-only view. Direktang i-edit ang settings.ron para baguhin ang mga binding.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Mode
schema-mode-detail = Color scheme para sa mga web page. Sinusunod ng Device ang system mo.
schema-device = Device
schema-light = Light
schema-dark = Dark
schema-language = Wika
schema-language-detail = Gamitin ang system, en-US, ja, o anumang BCP 47 tag na may katugmang ~/.vmux/locales/<tag>.ftl catalog.
schema-auto-update = Auto-update
schema-auto-update-detail = Tingnan at i-install ang mga update sa launch at kada oras.
schema-startup-url = Startup URL
schema-startup-url-detail = Kapag walang laman, bubuksan ang prompt ng command bar.
schema-search-engine = Search engine
schema-search-engine-detail = Ginagamit para sa web search mula sa Start at command bar.
schema-window = Window
schema-pane = Pane
schema-side-sheet = Side sheet
schema-focus-ring = Focus ring
schema-run-placement = Payagan ang run placement override
schema-run-placement-detail = Payagang pumili ang mga agent ng run pane mode, direksyon, at anchor.
schema-leader = Leader
schema-leader-detail = Prefix key para sa mga chord shortcut.
schema-chord-timeout = Chord timeout
schema-chord-timeout-detail = Milliseconds bago mag-expire ang chord prefix.
schema-bindings = Mga Binding
schema-confirm-close = Kumpirmahin ang pagsara
schema-confirm-close-detail = Mag-prompt bago isara ang terminal na may tumatakbong proseso.
schema-default-theme = Default na theme
schema-default-theme-detail = Pangalan ng aktibong theme mula sa listahan ng mga theme.
