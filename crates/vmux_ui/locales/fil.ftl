common-open = Buksan
common-close = Isara
common-install = I-install
common-uninstall = I-uninstall
common-update = I-update
common-retry = Subukang muli
common-refresh = I-refresh
common-remove = Alisin
common-enable = Paganahin
common-disable = Huwag paganahin
common-new = Bago
common-active = aktibo
common-running = tumatakbo
common-done = tapos na
common-failed = Nabigo
common-installed = Naka-install
common-items = { $count ->
    [one] { $count } item
   *[other] { $count } mga item
}
start-title = Simula
start-tagline = Isang prompt. Anuman, tapos na.

agents-title = Mga Ahente
agents-search = Maghanap ng mga ACP at CLI na ahente…
agents-empty = Walang katugmang ahente
agents-empty-detail = Subukan ang pangalan, runtime, o ACP/CLI.
agents-install-failed = Nabigo ang pag-install
agents-updating = Ina-update…
agents-retrying = Sinusubukang muli…
agents-preparing = Naghahanda…

extensions-title = Mga Extension
extensions-search = Maghanap ng naka-install o Chrome Web Store…
extensions-relaunch = I-relaunch para mailapat
extensions-empty = Walang naka-install na extension
extensions-no-match = Walang katugmang extension
extensions-empty-detail = Maghanap sa Chrome Web Store sa itaas at pindutin ang Return.
extensions-no-match-detail = Subukan ang ibang pangalan o extension ID.
extensions-on = Bukas
extensions-off = Sarado
extensions-enable-confirm = Paganahin ang { $name }?
extensions-enable-permissions = Paganahin ang { $name } at payagan ang:

lsp-title = Mga Language Server
lsp-search = Maghanap ng mga language server, linter, formatter…
lsp-loading = Naglo-load ng catalog…
lsp-empty = Walang katugmang language server
lsp-empty-detail = Subukan ang ibang wika, linter, o formatter.
lsp-needs = kailangan ang { $tool }
lsp-status-available = Available
lsp-status-on-path = Nasa PATH
lsp-status-installing = Ino-install…
lsp-status-installed = Naka-install
lsp-status-outdated = May available na update
lsp-status-running = Tumatakbo
lsp-status-failed = Nabigo

spaces-title = Mga Espasyo
spaces-new-placeholder = Pangalan ng bagong espasyo
spaces-empty = Walang espasyo
spaces-default-name = Espasyo { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } mga tab
}
spaces-delete = Burahin ang espasyo

team-title = Koponan
team-just-you = Ikaw lang sa espasyong ito
team-agents = { $count ->
    [one] Ikaw at 1 ahente
   *[other] Ikaw at { $count } mga ahente
}
team-empty = Wala pang narito
team-you = Ikaw
team-agent = Ahente

services-title = Mga Background na Serbisyo
services-processes = { $count ->
    [one] 1 proseso
   *[other] { $count } mga proseso
}
services-kill-all = Patayin Lahat
services-not-running = Hindi tumatakbo ang serbisyo
services-start-with = Simulan gamit ang:
services-empty = Walang aktibong proseso
services-filter = I-filter ang mga proseso…
services-no-match = Walang katugmang proseso
services-connected = Nakakonekta
services-disconnected = Hindi nakakonekta
services-attached = nakakabit
services-kill = Patayin
services-memory = Memory
services-size = Laki
services-shell = Shell

error-title = Error

history-search = Maghanap sa kasaysayan
history-clear-all = Burahin lahat
history-clear-confirm = Burahin ang lahat ng kasaysayan?
history-clear-warning = Hindi ito maaaring ibalik.
history-cancel = Kanselahin
history-today = Ngayon
history-yesterday = Kahapon
history-days-ago = { $count } araw na ang nakakaraan
history-day-offset = Araw -{ $count }

settings-title = Mga Setting
settings-loading = Naglo-load ng mga setting…
settings-stored = Naka-imbak sa ~/.vmux/settings.ron
settings-other = Iba pa
settings-software-update = Software Update
settings-check-updates = Suriin ang mga Update
settings-check-updates-hint = Awtomatikong sumusuri sa paglulunsad at bawat oras kapag naka-enable ang Auto-update.
settings-update-unavailable = Hindi Available
settings-update-unavailable-hint = Hindi kasama ang Updater sa build na ito.
settings-update-checking = Sinusuri…
settings-update-checking-hint = Sinusuri ang mga update…
settings-update-check-again = Suriin Muli
settings-update-current = Napapanahon na ang Vmux.
settings-update-downloading = Dina-download…
settings-update-downloading-hint = Dina-download ang Vmux { $version }…
settings-update-installing = Ino-install…
settings-update-installing-hint = Ino-install ang Vmux { $version }…
settings-update-ready = Handa ang Update
settings-update-ready-hint = Handa na ang Vmux { $version }. I-restart para mailapat.
settings-update-try-again = Subukan Muli
settings-update-failed = Hindi masuri ang mga update.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Pindutin ang isang key…
settings-saved = Nai-save
settings-record-key = Mag-click para mag-record ng bagong key combo

tray-open-window = Buksan ang Window
tray-close-window = Isara ang Window
tray-pause-recording = I-pause ang Pagre-record
tray-resume-recording = Ituloy ang Pagre-record
tray-finish-recording = Tapusin ang Pagre-record
tray-quit = Lumabas sa Vmux

composer-attach-files = Mag-attach ng mga file (/upload)
composer-remove-attachment = Alisin ang attachment

layout-back = Bumalik
layout-forward = Pasulong
layout-reload = I-reload
layout-bookmark-page = I-bookmark ang pahinang ito
layout-remove-bookmark = Alisin ang bookmark
layout-pin-page = I-pin ang pahinang ito
layout-unpin-page = I-unpin ang pahinang ito
layout-manage-extensions = Pamahalaan ang mga extension
layout-new-stack = Bagong Stack
layout-close-tab = Isara ang tab
layout-bookmark = Bookmark
layout-pin = Pin
layout-new-tab = Bagong tab
layout-team = Koponan

command-switch-space = Lumipat ng espasyo…
command-search-ask = Maghanap o magtanong…
command-new-tab-placeholder = Maghanap o mag-type ng URL, o pumili ng Terminal…
command-placeholder = Mag-type ng URL, maghanap ng mga tab, o > para sa mga command…
command-composer-placeholder = Mag-type ng / para sa mga command o @ para sa media
command-send = Ipadala (Enter)
command-terminal = Terminal
command-open-terminal = Buksan sa Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } mga tab
}
command-prompt = Prompt
command-new-tab = Bagong tab
command-search = Maghanap
command-open-value = Buksan ang "{ $value }"
command-search-value = Hanapin ang "{ $value }"

schema-appearance = Hitsura
schema-general = Pangkalahatan
schema-layout = Layout
schema-layout-detail = Window, mga panel, sidebar, at focus ring.
schema-agent = Ahente
schema-agent-detail = Gawi ng ahente at mga pahintulot sa tool.
schema-shortcuts = Mga Shortcut
schema-shortcuts-detail = Read-only na view. Direktang i-edit ang settings.ron para baguhin ang mga binding.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Mode
schema-mode-detail = Color scheme para sa mga web page. Sinusundan ng Device ang iyong sistema.
schema-device = Device
schema-light = Maliwanag
schema-dark = Madilim
schema-language = Wika
schema-language-detail = Gamitin ang system, en-US, ja, o anumang BCP 47 na tag na may katugmang ~/.vmux/locales/<tag>.ftl na catalog.
schema-auto-update = Auto-update
schema-auto-update-detail = Suriin at i-install ang mga update sa paglulunsad at bawat oras.
schema-startup-url = Startup URL
schema-startup-url-detail = Ang walang laman ay nagbubukas ng command bar prompt.
schema-search-engine = Search engine
schema-search-engine-detail = Ginagamit para sa mga web search mula sa Start at command bar.
schema-window = Window
schema-pane = Panel
schema-side-sheet = Side sheet
schema-focus-ring = Focus ring
schema-run-placement = Payagan ang override ng run placement
schema-run-placement-detail = Hayaan ang mga ahente na pumili ng run pane mode, direksyon, at anchor.
schema-leader = Leader
schema-leader-detail = Prefix key para sa mga chord shortcut.
schema-chord-timeout = Chord timeout
schema-chord-timeout-detail = Mga millisecond bago mag-expire ang chord prefix.
schema-bindings = Mga Binding
schema-confirm-close = Kumpirmahin ang pagsasara
schema-confirm-close-detail = Mag-prompt bago isara ang terminal na may tumatakbong proseso.
schema-default-theme = Default na tema
schema-default-theme-detail = Pangalan ng aktibong tema mula sa listahan ng mga tema.
