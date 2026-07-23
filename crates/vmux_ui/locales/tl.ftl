locale-name = Tagalog
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

tools-title = Mga kagamitan
tools-search = Maghanap ng mga pakete, ahente, MCP, kagamitang pangwika at talaksan ng pagsasaayos…
tools-open = Buksan ang mga kagamitan
tools-fold = Itiklop ang mga kagamitan
tools-unfold = Iladlad ang mga kagamitan
tools-scanning = Sinusuri ang mga lokal na kagamitan…
tools-no-installed = Walang nakatalagang kagamitan
tools-empty = Walang tumutugmang kagamitan
tools-empty-detail = Maglagay ng pakete o magdagdag ng Stow-style na pakete ng mga talaksan ng pagsasaayos.
tools-apply = Ilapat
tools-homebrew = Homebrew
tools-homebrew-sync = Awtomatikong pinagtutugma ang mga nakatalagang pormula at aplikasyon.
tools-open-brewfile = Buksan ang Brewfile
tools-managed = pinamamahalaan
tools-provider-homebrew-formulae = Mga pormula ng Homebrew
tools-provider-homebrew-casks = Mga aplikasyon ng Homebrew
tools-provider-npm = Mga pakete ng npm
tools-provider-acp-agents = Mga ahente ng ACP
tools-provider-language-tools = Mga kagamitang pangwika
tools-provider-mcp-servers = Mga tagapagsilbi ng MCP
tools-provider-dotfiles = Mga talaksan ng pagsasaayos
tools-status-available = Magagamit
tools-status-missing = Nawawala
tools-status-conflict = Salungatan
tools-forget = Kalimutan
tools-manage = Pamahalaan
tools-link = Iugnay
tools-unlink = Alisin ang ugnay
tools-import = Ipasok
tools-update-count = { $count ->
    [one] 1 pagbabago
   *[other] { $count } pagbabago
}
tools-conflict-count = { $count ->
    [one] 1 salungatan
   *[other] { $count } salungatan
}
tools-result-applied = Nailapat ang mga kagamitan
tools-result-imported = Naipasok ang mga kagamitan
tools-result-installed = Nailagay ang { $name }
tools-result-updated = Nabago ang { $name }
tools-result-uninstalled = Inalis ang { $name }
tools-result-forgotten = Kinalimutan ang { $name }
tools-result-managed = Pinamamahalaan na ang { $name }
tools-result-linked = Iniugnay ang { $name }
tools-result-unlinked = Inalis ang ugnay ng { $name }

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

settings-empty = (walang laman)
settings-none = (wala)

schema-system = Sistema
schema-editor = Editor
schema-recording = Pagre-record
schema-radius = Radius
schema-padding = Padding
schema-gap = Agwat
schema-width = Lapad
schema-color = Kulay
schema-red = Pula
schema-green = Berde
schema-blue = Asul
schema-follow-files = Subaybayan ang mga file
schema-tidy-files = Linisin ang mga file
schema-tidy-files-max = Hangganan sa paglilinis ng file
schema-tidy-files-auto = Awtomatikong linisin ang mga file
schema-app-providers = Mga provider ng app
schema-provider = Provider
schema-kind = Uri
schema-models = Mga modelo
schema-acp = Mga agent ng ACP
schema-id = ID
schema-name = Pangalan
schema-command = Utos
schema-arguments = Mga argumento
schema-environment = Environment
schema-working-directory = Working directory
schema-shell = Shell
schema-font-family = Font family
schema-startup-directory = Startup directory
schema-themes = Mga tema
schema-color-scheme = Color scheme
schema-font-size = Laki ng font
schema-line-height = Taas ng linya
schema-cursor-style = Estilo ng cursor
schema-cursor-blink = Pagkurap ng cursor
schema-custom-themes = Mga custom na tema
schema-foreground = Foreground
schema-background = Background
schema-cursor = Cursor
schema-ansi-colors = Mga kulay ng ANSI
schema-keymap = Keymap
schema-explorer = Explorer
schema-visible = Nakikita
schema-language-servers = Mga language server
schema-servers = Mga server
schema-language-id = Language ID
schema-root-markers = Mga root marker
schema-output-directory = Output directory

menu-scene = Eksena
menu-layout = Ayos
menu-terminal = Terminal
menu-browser = Browser
menu-service = Serbisyo
menu-bookmark = Pananda
menu-edit = I-edit

layout-knowledge = Kaalaman
layout-open-knowledge = Buksan ang Kaalaman
layout-open-welcome-knowledge = Buksan ang Maligayang Pagdating sa Kaalaman
layout-open-path = Buksan ang { $path }
layout-fold-knowledge = Itupi ang kaalaman
layout-unfold-knowledge = Ibuka ang kaalaman
layout-bookmarks = Mga pananda
layout-new-folder = Bagong Folder
layout-add-to-bookmarks = Idagdag sa mga Pananda
layout-move-to-bookmarks = Ilipat sa mga Pananda
layout-stack-number = Salansan { $number }
layout-fold-stack = Itupi ang salansan
layout-unfold-stack = Ibuka ang salansan
layout-close-stack = Isara ang salansan
layout-bookmark-in = Ipananda sa { $folder }

common-cancel = Kanselahin
common-delete = Burahin
common-save = I-save
common-rename = Palitan ang pangalan
common-expand = Palawakin
common-collapse = I-collapse
common-loading = Naglo-load…
common-error = Error
common-output = Output
common-pending = Nakabinbin
common-current = kasalukuyan
common-stop = Ihinto
services-command = Serbisyo ng Vmux
services-uptime-seconds = { $seconds }s
services-uptime-minutes = { $minutes }m { $seconds }s
services-uptime-hours = { $hours }h { $minutes }m
services-uptime-days = { $days }d { $hours }h

error-page-failed-load = Hindi na-load ang page
error-page-not-found = Hindi nahanap ang page
error-unknown-host = Hindi kilalang Vmux app host: { $host }

history-title = Kasaysayan

command-new-app-chat = Bagong chat sa { $provider }/{ $model } (App)
command-interactive-mode-user = Scene > Interactive Mode > User
command-interactive-mode-player = Scene > Interactive Mode > Player
command-minimize-window = Layout > Window > I-minimize
command-toggle-layout = Layout > Layout > I-toggle ang Layout
command-close-tab = Layout > Tab > Isara ang Tab
command-new-task = Layout > Tab > Bagong Gawain…
command-next-tab = Layout > Tab > Susunod na Tab
command-prev-tab = Layout > Tab > Nakaraang Tab
command-rename-tab = Layout > Tab > Palitan ang Pangalan ng Tab
command-tab-select-1 = Layout > Tab > Piliin ang Tab 1
command-tab-select-2 = Layout > Tab > Piliin ang Tab 2
command-tab-select-3 = Layout > Tab > Piliin ang Tab 3
command-tab-select-4 = Layout > Tab > Piliin ang Tab 4
command-tab-select-5 = Layout > Tab > Piliin ang Tab 5
command-tab-select-6 = Layout > Tab > Piliin ang Tab 6
command-tab-select-7 = Layout > Tab > Piliin ang Tab 7
command-tab-select-8 = Layout > Tab > Piliin ang Tab 8
command-tab-select-last = Layout > Tab > Piliin ang Huling Tab
command-close-pane = Layout > Pane > Isara ang Pane
command-select-pane-left = Layout > Pane > Piliin ang Kaliwang Pane
command-select-pane-right = Layout > Pane > Piliin ang Kanang Pane
command-select-pane-up = Layout > Pane > Piliin ang Pane sa Itaas
command-select-pane-down = Layout > Pane > Piliin ang Pane sa Ibaba
command-swap-pane-prev = Layout > Pane > Ipagpalit sa Nakaraang Pane
command-swap-pane-next = Layout > Pane > Ipagpalit sa Susunod na Pane
command-equalize-pane-size = Layout > Pane > Pantayin ang Laki ng Pane
command-resize-pane-left = Layout > Pane > Baguhin ang Laki ng Pane Pakaliwa
command-resize-pane-right = Layout > Pane > Baguhin ang Laki ng Pane Pakanan
command-resize-pane-up = Layout > Pane > Baguhin ang Laki ng Pane Pataas
command-resize-pane-down = Layout > Pane > Baguhin ang Laki ng Pane Pababa
command-stack-close = Layout > Stack > Isara ang Stack
command-stack-next = Layout > Stack > Susunod na Stack
command-stack-previous = Layout > Stack > Nakaraang Stack
command-stack-reopen = Layout > Stack > Buksan Muli ang Isinarang Page
command-stack-swap-prev = Layout > Stack > Ilipat ang Stack Pakaliwa
command-stack-swap-next = Layout > Stack > Ilipat ang Stack Pakanan
command-space-open = Layout > Space > Mga Space
command-terminal-close = Terminal > Isara ang Terminal
command-terminal-next = Terminal > Susunod na Terminal
command-terminal-prev = Terminal > Nakaraang Terminal
command-terminal-clear = Terminal > I-clear ang Terminal
command-browser-prev-page = Browser > Pag-navigate > Bumalik
command-browser-next-page = Browser > Pag-navigate > Sumulong
command-browser-reload = Browser > Pag-navigate > I-reload
command-browser-hard-reload = Browser > Pag-navigate > Hard Reload
command-open-in-place = Browser > Buksan > Buksan Dito
command-open-in-new-stack = Browser > Buksan > Buksan sa Bagong Stack
command-open-in-pane-top = Browser > Buksan > Buksan sa Pane sa Itaas
command-open-in-pane-right = Browser > Buksan > Buksan sa Kanang Pane
command-open-in-pane-bottom = Browser > Buksan > Buksan sa Pane sa Ibaba
command-open-in-pane-left = Browser > Buksan > Buksan sa Kaliwang Pane
command-open-in-new-tab = Browser > Buksan > Buksan sa Bagong Tab
command-open-in-new-space = Browser > Buksan > Buksan sa Bagong Space
command-browser-zoom-in = Browser > View > Mag-zoom In
command-browser-zoom-out = Browser > View > Mag-zoom Out
command-browser-zoom-reset = Browser > View > Aktuwal na Laki
command-browser-dev-tools = Browser > View > Developer Tools
command-browser-open-command-bar = Browser > Bar > Command Bar
command-browser-open-page-in-command-bar = Browser > Bar > I-edit ang Page
command-browser-open-path-bar = Browser > Bar > Path Navigator
command-browser-open-commands = Browser > Bar > Mga Command
command-browser-open-history = Browser > Bar > Kasaysayan
command-service-open = Service > Buksan ang Service Monitor
command-bookmark-toggle-active = Bookmark > I-bookmark ang Page
command-bookmark-pin-active = Bookmark > I-pin ang Page

layout-tab = Tab
layout-no-stacks = Walang stack
layout-loading = Naglo-load…
layout-no-markdown-files = Walang Markdown file
layout-empty-folder = Walang laman ang folder
layout-worktree = worktree
layout-folder-name = Pangalan ng folder
layout-no-pins-bookmarks = Walang pin o bookmark
layout-move-to = Ilipat sa { $folder }
layout-bookmark-current-page = I-bookmark ang Kasalukuyang Page
layout-rename-folder = Palitan ang Pangalan ng Folder
layout-remove-folder = Alisin ang Folder
layout-update-downloading = Dina-download ang update
layout-update-installing = Ini-install ang update…
layout-update-ready = May bagong bersyon
layout-restart-update = I-restart para mag-update

agent-preparing = Inihahanda ang agent…
agent-send-all-queued = Ipadala ngayon ang lahat ng naka-queue na prompt (Esc)
agent-send = Ipadala (Enter)
agent-ready = Handa na kapag handa ka na.
agent-loading-older = Nilo-load ang mas lumang mga mensahe…
agent-load-older = I-load ang mas lumang mga mensahe
agent-continued-from = Ipinagpatuloy mula sa { $source }
agent-older-context-omitted = inalis ang mas lumang konteksto
agent-interrupted = naantala
agent-allow-tool = Payagan ang { $tool }?
agent-deny = Tanggihan
agent-allow-always = Palaging payagan
agent-allow = Payagan
agent-loading-sessions = Nilo-load ang mga session…
agent-no-resumable-sessions = Walang nahanap na session na maipagpapatuloy
agent-no-matching-sessions = Walang tugmang session
agent-no-matching-models = Walang tugmang model
agent-choice-help = ↑/↓ o Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Pumili ng repository folder
agent-choose-repository-detail = Piliin ang lokal na Git repository na gagamitin ng agent.
agent-choosing = Pumipili…
agent-choose-folder = Pumili ng folder
agent-queued = naka-queue
agent-attached = Naka-attach:
agent-cancel-queued = Kanselahin ang naka-queue na prompt
agent-resume-queued = Ipagpatuloy ang mga naka-queue na prompt
agent-clear-queue = I-clear ang queue
agent-send-all-now = ipadala lahat ngayon
agent-choose-option = Pumili ng opsyon sa itaas
agent-loading-media = Nilo-load ang media…
agent-no-matching-media = Walang tugmang media
agent-prompt-context = Konteksto ng prompt
agent-details = Mga detalye
agent-path = Path
agent-tool = Tool
agent-server = Server
agent-bytes = { $count } bytes
agent-worked-for = Gumawa nang { $duration }
agent-worked-for-steps = { $count ->
    [one] Gumawa nang { $duration } · 1 hakbang
   *[other] Gumawa nang { $duration } · { $count } hakbang
}
agent-tool-guardian-review = Guardian Review
agent-tool-read-files = Nagbasa ng mga file
agent-tool-viewed-image = Tiningnan ang larawan
agent-tool-used-browser = Gumamit ng browser
agent-tool-searched-files = Naghanap sa mga file
agent-tool-ran-commands = Nagpatakbo ng mga command
agent-thinking = Nag-iisip
agent-subagent = Subagent
agent-prompt = Prompt
agent-thread = Thread
agent-parent = Parent
agent-children = Mga child
agent-call = Call
agent-raw-event = Raw event
agent-plan = Plano
agent-tasks = { $count ->
    [one] 1 gawain
   *[other] { $count } gawain
}
agent-edited = Na-edit
agent-reconnecting = Kumokonekta muli { $attempt }/{ $total }
agent-status-running = Tumatakbo
agent-status-done = Tapos na
agent-status-failed = Nabigo
agent-status-pending = Nakabinbin
agent-slash-attach-files = Mag-attach ng mga file
agent-slash-resume-session = Ipagpatuloy ang dating session
agent-slash-select-model = Pumili ng model
agent-slash-continue-cli = Ipagpatuloy ang session na ito sa CLI
agent-session-just-now = ngayon lang
agent-session-minutes-ago = { $count }m ang nakalipas
agent-session-hours-ago = { $count }h ang nakalipas
agent-session-days-ago = { $count }d ang nakalipas
agent-working-working = Gumagawa
agent-working-thinking = Nag-iisip
agent-working-pondering = Nagmumuni-muni
agent-working-noodling = Nag-eeksperimento
agent-working-percolating = Pinapahinog
agent-working-conjuring = Bumubuo
agent-working-cooking = Niluluto
agent-working-brewing = Tinitimpla
agent-working-musing = Nagmumuni
agent-working-ruminating = Pinag-iisipan
agent-working-scheming = Nagpaplano
agent-working-synthesizing = Sinasala
agent-working-tinkering = Kinakalikut
agent-working-churning = Pinoproseso
agent-working-vibing = Kumakapa
agent-working-simmering = Pinapakulo
agent-working-crafting = Binubuo
agent-working-divining = Sinisilip
agent-working-mulling = Pinag-iisipan
agent-working-spelunking = Sinusuyod

editor-toggle-explorer = I-toggle ang Explorer (Cmd+B)
editor-unsaved = hindi na-save
editor-rendered-markdown = Na-render na Markdown na may live editing
editor-note = Tala
editor-source-editor = Source editor
editor-editor = Editor
editor-git-diff = Git diff
editor-diff = Diff
editor-tidy = Linisin
editor-always = Palagi
editor-unchanged-previews = { $count ->
    [one] ✦ 1 hindi nabagong preview
   *[other] ✦ { $count } hindi nabagong preview
}
editor-open-externally = Buksan sa labas
editor-changed-line = Binagong linya
editor-go-to-definition = Pumunta sa Depinisyon
editor-find-references = Hanapin ang mga Reference
editor-references = { $count ->
    [one] 1 reference
   *[other] { $count } reference
}
editor-lsp-starting = Nagsisimula ang { $server }…
editor-lsp-not-installed = { $server } — hindi naka-install
editor-explorer = Explorer
editor-open-editors = Mga Bukas na Editor
editor-outline = Outline
editor-new-file = Bagong File
editor-new-folder = Bagong Folder
editor-delete-confirm = Burahin ang “{ $name }”? Hindi na ito mababawi.
editor-created-folder = Nagawa ang folder na { $name }
editor-created-file = Nagawa ang file na { $name }
editor-renamed-to = Pinalitan ang pangalan sa { $name }
editor-deleted = Binura ang { $name }
editor-failed-decode-image = Hindi na-decode ang larawan
editor-preview-large-image = larawan (masyadong malaki para i-preview)
editor-preview-binary = binary
editor-preview-file = file

git-status-clean = malinis
git-status-modified = binago
git-status-staged = naka-stage
git-status-staged-modified = naka-stage*
git-status-untracked = hindi naka-track
git-status-deleted = binura
git-status-conflict = conflict
git-accept-all = ✓ tanggapin lahat
git-unstage = Alisin sa stage
git-confirm-deny-all = Kumpirmahin ang pagtanggi sa lahat
git-deny-all = ✗ tanggihan lahat
git-commit-message = mensahe ng commit
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Nilo-load ang diff…
git-no-changes = Walang pagbabagong ipapakita
git-accept = ✓ tanggapin
git-deny = ✗ tanggihan
git-show-unchanged-lines = Ipakita ang { $count } hindi nabagong linya

terminal-loading = Naglo-load…
terminal-runs-when-ready = tatakbo kapag handa na · Ctrl+C para i-clear · Esc para laktawan
terminal-booting = nagbo-boot
terminal-type-command = mag-type ng command · tatakbo kapag handa na · Esc para laktawan

setup-tagline-claude = Coding agent ng Anthropic, sa Vmux
setup-tagline-codex = Coding agent ng OpenAI, sa Vmux
setup-tagline-vibe = Coding agent ng Mistral, sa Vmux
setup-install-title = I-install ang { $name } CLI
setup-homebrew-required = Kailangan ang Homebrew para i-install ang { $command } at hindi pa ito naka-set up. Ii-install muna ng Vmux ang Homebrew, pagkatapos ang { $name }.
setup-terminal-instructions = Sa terminal, pindutin ang Return para magsimula, pagkatapos ilagay ang password ng Mac mo kapag hiniling.
setup-command-missing = Binuksan ng Vmux ang page na ito dahil hindi pa naka-install ang lokal na command na { $command }. Patakbuhin ang command sa ibaba para makuha ito.
setup-install-failed = Hindi natapos ang pag-install. Tingnan ang terminal para sa mga detalye, pagkatapos subukang muli.
setup-installing = Ini-install…
setup-install-homebrew = I-install ang Homebrew + { $name }
setup-run-install = Patakbuhin ang install command
setup-auto-reload = Pinapatakbo ito ng Vmux sa terminal at nire-reload kapag handa na ang { $command }.

debug-title = Debug
debug-auto-update = Auto-update
debug-simulate-update = Gayahin na may available na update
debug-simulate-download = Gayahin ang download
debug-clear-update = I-clear ang update
debug-trigger-restart = I-trigger ang restart

command-manage-spaces = Pamahalaan ang mga space…
command-pane-stack-location = pane { $pane } / stack { $stack }
command-space-pane-stack-location = { $space } / pane { $pane } / stack { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Interactive Mode
command-group-window = Window
command-group-tab = Tab
command-group-pane = Pane
command-group-stack = Stack
command-group-space = Space
command-group-navigation = Navigation
command-group-open = Buksan
command-group-view = View
command-group-bar = Bar

menu-close-vmux = Isara ang Vmux

agents-terminal-coding-agent = Coding agent na nakabase sa terminal
