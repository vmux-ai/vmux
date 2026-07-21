common-open = Ablihi
common-close = Sirad-i
common-install = I-install
common-uninstall = I-uninstall
common-update = I-update
common-retry = Sulayi pag-usab
common-refresh = I-refresh
common-remove = Tangtanga
common-enable = I-enable
common-disable = I-disable
common-new = Bag-o
common-active = aktibo
common-running = nagdagan
common-done = human na
common-failed = Napakyas
common-installed = Na-install
common-items = { $count ->
    [one] { $count } item
   *[other] { $count } item
}
start-title = Sugod
start-tagline = Usa ka prompt. Bisan unsa, mahuman.

agents-title = Mga Agent
agents-search = Pangitaa ang ACP ug CLI agents…
agents-empty = Walay matching nga agents
agents-empty-detail = Sulayi ang ngalan, runtime, o ACP/CLI.
agents-install-failed = Napakyas ang pag-install
agents-updating = Nag-update…
agents-retrying = Gisulayan pag-usab…
agents-preparing = Nag-andam…

extensions-title = Mga Extension
extensions-search = Pangitaa ang na-install o sa Chrome Web Store…
extensions-relaunch = I-relaunch aron ma-apply
extensions-empty = Walay na-install nga extension
extensions-no-match = Walay matching nga extension
extensions-empty-detail = Pangitaa sa Chrome Web Store sa taas ug pindota ang Return.
extensions-no-match-detail = Sulayi ang laing ngalan o extension ID.
extensions-on = On
extensions-off = Off
extensions-enable-confirm = I-enable ang { $name }?
extensions-enable-permissions = I-enable ang { $name } ug tugoti ang:

lsp-title = Mga Language Server
lsp-search = Pangitaa ang language servers, linters, formatters…
lsp-loading = Nag-load sa catalog…
lsp-empty = Walay matching nga language server
lsp-empty-detail = Sulayi ang laing language, linter, o formatter.
lsp-needs = nanginahanglan og { $tool }
lsp-status-available = Available
lsp-status-on-path = Sa PATH
lsp-status-installing = Nag-install…
lsp-status-installed = Na-install
lsp-status-outdated = Adunay update
lsp-status-running = Nagdagan
lsp-status-failed = Napakyas

spaces-title = Mga Espasyo
spaces-new-placeholder = Ngalan sa bag-ong espasyo
spaces-empty = Walay espasyo
spaces-default-name = Espasyo { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
spaces-delete = I-delete ang espasyo

team-title = Team
team-just-you = Ikaw ra dinhi sa espasyong kini
team-agents = { $count ->
    [one] Ikaw ug 1 agent
   *[other] Ikaw ug { $count } agents
}
team-empty = Wala pay tawo dinhi
team-you = Ikaw
team-agent = Agent

services-title = Mga Background Service
services-processes = { $count ->
    [one] 1 proseso
   *[other] { $count } proseso
}
services-kill-all = I-kill Tanan
services-not-running = Wala nagdagan ang service
services-start-with = Sugdi gamit ang:
services-empty = Walay aktibong proseso
services-filter = I-filter ang mga proseso…
services-no-match = Walay matching nga proseso
services-connected = Nakakonekta
services-disconnected = Naputol
services-attached = naka-attach
services-kill = I-kill
services-memory = Memorya
services-size = Gidak-on
services-shell = Shell

error-title = Sayop

history-search = Pangitaa sa history
history-clear-all = Hawani tanan
history-clear-confirm = Hawanan ang tanang history?
history-clear-warning = Dili na kini mabawi.
history-cancel = Kanselahon
history-today = Karon
history-yesterday = Kagahapon
history-days-ago = { $count } ka adlaw ang milabay
history-day-offset = Adlaw -{ $count }

settings-title = Mga Setting
settings-loading = Nag-load sa settings…
settings-stored = Nakatipig sa ~/.vmux/settings.ron
settings-other = Uban pa
settings-software-update = Software Update
settings-check-updates = Susiha ang Updates
settings-check-updates-hint = Awtomatikong mosusi sa pag-launch ug kada oras kung naka-enable ang Auto-update.
settings-update-unavailable = Dili available
settings-update-unavailable-hint = Wala maapil ang updater niining build.
settings-update-checking = Nagsusi…
settings-update-checking-hint = Nagsusi og updates…
settings-update-check-again = Susiha Pag-usab
settings-update-current = Up to date ang Vmux.
settings-update-downloading = Nag-download…
settings-update-downloading-hint = Nag-download sa Vmux { $version }…
settings-update-installing = Nag-install…
settings-update-installing-hint = Nag-install sa Vmux { $version }…
settings-update-ready = Andam na ang Update
settings-update-ready-hint = Andam na ang Vmux { $version }. I-restart aron ma-apply.
settings-update-try-again = Sulayi Pag-usab
settings-update-failed = Dili masusi ang updates.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Pindota ang usa ka key…
settings-saved = Natipig
settings-record-key = I-click aron mag-record og bag-ong key combo

tray-open-window = Ablihi ang Window
tray-close-window = Sirad-i ang Window
tray-pause-recording = I-pause ang Recording
tray-resume-recording = Ipadayon ang Recording
tray-finish-recording = Humana ang Recording
tray-quit = Gawas sa Vmux

composer-attach-files = I-attach ang mga file (/upload)
composer-remove-attachment = Tangtanga ang attachment

layout-back = Balik
layout-forward = Padayon
layout-reload = I-reload
layout-bookmark-page = I-bookmark kining panid
layout-remove-bookmark = Tangtanga ang bookmark
layout-pin-page = I-pin kining panid
layout-unpin-page = I-unpin kining panid
layout-manage-extensions = Dumala ang extensions
layout-new-stack = Bag-ong Stack
layout-close-tab = Sirad-i ang tab
layout-bookmark = Bookmark
layout-pin = I-pin
layout-new-tab = Bag-ong tab
layout-team = Team

command-switch-space = Balhin og espasyo…
command-search-ask = Pangitaa o pangutana…
command-new-tab-placeholder = Pangitaa o i-type ang URL, o pilia ang Terminal…
command-placeholder = I-type ang URL, pangitaa ang tabs, o > para sa commands…
command-composer-placeholder = I-type ang / para sa commands o @ para sa media
command-send = Ipadala (Enter)
command-terminal = Terminal
command-open-terminal = Ablihi sa Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
command-prompt = Prompt
command-new-tab = Bag-ong tab
command-search = Pangitaa
command-open-value = Ablihi ang “{ $value }”
command-search-value = Pangitaa ang “{ $value }”

schema-appearance = Panagway
schema-general = Kinatibuk-an
schema-layout = Layout
schema-layout-detail = Window, panes, sidebar, ug focus ring.
schema-agent = Agent
schema-agent-detail = Kinaiya sa agent ug mga permiso sa tool.
schema-shortcuts = Mga Shortcut
schema-shortcuts-detail = Read-only nga view. Direkta usba ang settings.ron aron mausab ang bindings.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Mode
schema-mode-detail = Color scheme para sa mga web page. Ang Device mosunod sa imong system.
schema-device = Device
schema-light = Hayag
schema-dark = Ngitngit
schema-language = Pinulongan
schema-language-detail = Gamita ang system, en-US, ja, o bisan unsang BCP 47 tag nga naay matching nga ~/.vmux/locales/<tag>.ftl catalog.
schema-auto-update = Auto-update
schema-auto-update-detail = Susihon ug i-install ang updates sa pag-launch ug kada oras.
schema-startup-url = Startup URL
schema-startup-url-detail = Kung walay sulod, ablihan ang prompt sa command bar.
schema-search-engine = Search engine
schema-search-engine-detail = Gigamit sa web search gikan sa Sugod ug command bar.
schema-window = Window
schema-pane = Pane
schema-side-sheet = Side sheet
schema-focus-ring = Focus ring
schema-run-placement = Tugoti ang override sa run placement
schema-run-placement-detail = Tugoti ang agents nga mopili sa run pane mode, direksyon, ug anchor.
schema-leader = Leader
schema-leader-detail = Prefix key para sa chord shortcuts.
schema-chord-timeout = Chord timeout
schema-chord-timeout-detail = Milliseconds sa dili pa mo-expire ang chord prefix.
schema-bindings = Bindings
schema-confirm-close = Kumpirmahi ang pagsira
schema-confirm-close-detail = Mangayo og kumpirmasyon sa dili pa isira ang terminal nga naay nagdagang proseso.
schema-default-theme = Default nga theme
schema-default-theme-detail = Ngalan sa aktibong theme gikan sa listahan sa themes.
