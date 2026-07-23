locale-name = Gagana Sāmoa
common-open = Tatala
common-close = Tapuni
common-install = Faapipii
common-uninstall = Aveese le faapipiiina
common-update = Faafou
common-retry = Toe taumafai
common-refresh = Toe faafou
common-remove = Aveese
common-enable = Ki
common-disable = Tape
common-new = Fou
common-active = ola
common-running = o loo tamoʻe
common-done = maeʻa
common-failed = Lē manuia
common-installed = Ua faapipii
common-items = { $count ->
    [one] { $count } aitema
   *[other] { $count } aitema
}

tools-title = Meafaigaluega
tools-search = Suʻe afifi, sui, MCP, meafaigaluega o gagana ma faila faatulagaina…
tools-open = Tatala meafaigaluega
tools-fold = Gaugau meafaigaluega
tools-unfold = Tatala faalautele meafaigaluega
tools-scanning = O loo siaki meafaigaluega i le masini…
tools-no-installed = E leai ni meafaigaluega ua faapipii
tools-empty = E leai ni meafaigaluega e fetaui
tools-empty-detail = Faapipii se afifi pe faaopoopo se afifi faila faatulagaina i le faiga Stow.
tools-apply = Faaaoga
tools-homebrew = Homebrew
tools-homebrew-sync = E otometi ona ogatasi fua ma polokalame ua faapipii.
tools-open-brewfile = Tatala Brewfile
tools-managed = pulea
tools-provider-homebrew-formulae = Fua Homebrew
tools-provider-homebrew-casks = Polokalame Homebrew
tools-provider-npm = Afifi npm
tools-provider-acp-agents = Sui ACP
tools-provider-language-tools = Meafaigaluega o gagana
tools-provider-mcp-servers = Sava MCP
tools-provider-dotfiles = Faila faatulagaina
tools-status-available = Avanoa
tools-status-missing = Leiloa
tools-status-conflict = Feteenaʻiga
tools-forget = Faagalo
tools-manage = Pulea
tools-link = Fesootaʻi
tools-unlink = Tatala le sootaga
tools-import = Aumai i totonu
tools-update-count = { $count ->
    [one] 1 faafouga
   *[other] { $count } faafouga
}
tools-conflict-count = { $count ->
    [one] 1 feteenaʻiga
   *[other] { $count } feteenaʻiga
}
tools-result-applied = Ua faaaoga meafaigaluega
tools-result-imported = Ua aumai i totonu meafaigaluega
tools-result-installed = Ua faapipii { $name }
tools-result-updated = Ua faafou { $name }
tools-result-uninstalled = Ua aveese { $name }
tools-result-forgotten = Ua faagalo { $name }
tools-result-managed = Ua pulea nei { $name }
tools-result-linked = Ua fesootaʻi { $name }
tools-result-unlinked = Ua tatala le sootaga o { $name }
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Fa'atonu fa'atulagaina, meafaigaluega, dotfiles, ma le Poto ma le Git.
vault-sync = Fa'atasi
vault-create = Fausia
vault-connect = Feso'ota'i
vault-private = fale teu oloa tumaoti
vault-public-warning = O faleteuoloa lautele e faʻaalia lou Poto ma le faʻatulagaina.
vault-choose-repository = Filifili se faleteuoloa…
vault-empty = gaogao
vault-clean = Faailoa mai
vault-not-connected = Le feso'ota'i
vault-change-count = Suiga: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Amata
start-tagline = Tasi le faatonuga. Uma ona fai.

agents-title = Sui AI
agents-search = Suʻe sui AI ACP ma CLI…
agents-empty = E leai ni sui AI e fetaui
agents-empty-detail = Taumafai i se igoa, runtime, po o le ACP/CLI.
agents-install-failed = Lē manuia le faapipiiina
agents-updating = O loo faafou…
agents-retrying = O loo toe taumafai…
agents-preparing = O loo saunia…

extensions-title = Faalautelega
extensions-search = Suʻe faalautelega ua faapipii po o le Chrome Web Store…
extensions-relaunch = Toe amata e faaaogā ai
extensions-empty = E leai ni faalautelega ua faapipii
extensions-no-match = E leai ni faalautelega e fetaui
extensions-empty-detail = Suʻe i le Chrome Web Store i luga ma oomi Return.
extensions-no-match-detail = Taumafai i se isi igoa po o le ID o le faalautelega.
extensions-on = Ki
extensions-off = Tape
extensions-enable-confirm = Ki { $name }?
extensions-enable-permissions = Ki { $name } ma faataga:

lsp-title = Auaunaga Gagana
lsp-search = Suʻe auaunaga gagana, linters, formatters…
lsp-loading = O loo uta le lisi…
lsp-empty = E leai ni auaunaga gagana e fetaui
lsp-empty-detail = Taumafai i se isi gagana, linter, po o formatter.
lsp-needs = manaomia { $tool }
lsp-status-available = Avanoa
lsp-status-on-path = I luga o le PATH
lsp-status-installing = O loo faapipii…
lsp-status-installed = Ua faapipii
lsp-status-outdated = E iai se faafouga
lsp-status-running = O loo tamoʻe
lsp-status-failed = Lē manuia

spaces-title = Avanoa
spaces-new-placeholder = Igoa o le avanoa fou
spaces-empty = E leai ni avanoa
spaces-default-name = Avanoa { $number }
spaces-tabs = { $count ->
    [one] 1 lau
   *[other] { $count } lau
}
spaces-delete = Tape le avanoa

team-title = Au
team-just-you = Na o oe i lenei avanoa
team-agents = { $count ->
    [one] Oe ma le 1 sui AI
   *[other] Oe ma sui AI e { $count }
}
team-empty = E lei iai se tasi i inei
team-you = Oe
team-agent = Sui AI

services-title = Auaunaga i Tua
services-processes = { $count ->
    [one] 1 faagasologa
   *[other] { $count } faagasologa
}
services-kill-all = Fasi Uma
services-not-running = E lē o tamoʻe le auaunaga
services-start-with = Amata i le:
services-empty = E leai ni faagasologa ola
services-filter = Faamama faagasologa…
services-no-match = E leai ni faagasologa e fetaui
services-connected = Fesootai
services-disconnected = Motusia
services-attached = pipii
services-kill = Fasi
services-memory = Manatua
services-size = Tele
services-shell = Shell

error-title = Sese

history-search = Suʻe i le talafaasolopito
history-clear-all = Faamama uma
history-clear-confirm = Faamama uma le talafaasolopito?
history-clear-warning = E lē mafai ona toe faafoʻi.
history-cancel = Faaleaogā
history-today = Aso nei
history-yesterday = Ananafi
history-days-ago = { $count } aso ua mavae
history-day-offset = Aso -{ $count }

settings-title = Faatulagaga
settings-loading = O loo uta faatulagaga…
settings-stored = Teuina i ~/.vmux/settings.ron
settings-other = Isi
settings-software-update = Faafouga Polokalame
settings-check-updates = Siaki Faafouga
settings-check-updates-hint = E otometi ona siaki pe a tatala ma itula taitasi pe a ki le Auto-update.
settings-update-unavailable = Lē avanoa
settings-update-unavailable-hint = E lē o aofia le updater i lenei build.
settings-update-checking = O loo siaki…
settings-update-checking-hint = O loo siaki faafouga…
settings-update-check-again = Toe Siaki
settings-update-current = Ua lata mai Vmux.
settings-update-downloading = O loo sii mai…
settings-update-downloading-hint = O loo sii mai Vmux { $version }…
settings-update-installing = O loo faapipii…
settings-update-installing-hint = O loo faapipii Vmux { $version }…
settings-update-ready = Ua Sauni le Faafouga
settings-update-ready-hint = Ua sauni Vmux { $version }. Toe amata e faaaogā ai.
settings-update-try-again = Toe Taumafai
settings-update-failed = Ua lē mafai ona siaki faafouga.
settings-item = Aitema
settings-item-number = Aitema { $number }
settings-press-key = Oomi se ki…
settings-saved = Ua sefe
settings-record-key = Kiliki e pue ai se tuufaatasiga fou o ki

tray-open-window = Tatala Faamalama
tray-close-window = Tapuni Faamalama
tray-pause-recording = Taofi Lē tumau le Pueina
tray-resume-recording = Faaauau le Pueina
tray-finish-recording = Faauma le Pueina
tray-quit = Tuua Vmux

composer-attach-files = Faapipii faila (/upload)
composer-remove-attachment = Aveese le faapipii

layout-back = Tua
layout-forward = Luma
layout-reload = Toe uta
layout-bookmark-page = Faailoga lenei itulau
layout-remove-bookmark = Aveese le faailoga
layout-pin-page = Pine lenei itulau
layout-unpin-page = Aveese le pine o lenei itulau
layout-manage-extensions = Pulea faalautelega
layout-new-stack = Faaputuga Fou
layout-close-tab = Tapuni le lau
layout-bookmark = Faailoga
layout-pin = Pine
layout-new-tab = Lau fou
layout-team = Au

command-switch-space = Sui avanoa…
command-search-ask = Suʻe pe fesili…
command-new-tab-placeholder = Suʻe pe taina se URL, pe filifili Terminal…
command-placeholder = Taina se URL, suʻe lau, po o le > mo poloaiga…
command-composer-placeholder = Taina / mo poloaiga po o @ mo ala faasalalau
command-send = Lafo (Enter)
command-terminal = Terminal
command-open-terminal = Tatala i le Terminal
command-stack = Faaputuga
command-tabs = { $count ->
    [one] 1 lau
   *[other] { $count } lau
}
command-prompt = Faatonuga
command-new-tab = Lau fou
command-search = Suʻe
command-open-value = Tatala “{ $value }”
command-search-value = Suʻe “{ $value }”

schema-appearance = Foliga
schema-general = Lautele
schema-layout = Faatulagaga o le lau
schema-layout-detail = Faamalama, vaega, pa autafa, ma le mama faamamafa.
schema-agent = Sui AI
schema-agent-detail = Amio a le sui AI ma faatagaga mo meafaigaluega.
schema-shortcuts = Ala pupuu
schema-shortcuts-detail = Na o le vaai. Faasaʻo saʻo settings.ron e sui ai bindings.
schema-terminal = Terminal
schema-browser = Suʻesuʻe
schema-mode = Faiga
schema-mode-detail = Mamanu lanu mo itulau web. E mulimuli Device i lau system.
schema-device = Device
schema-light = Malamalama
schema-dark = Pogisa
schema-language = Gagana
schema-language-detail = Faaaogā le system, en-US, ja, po o so o se tag BCP 47 e iai se catalog ~/.vmux/locales/<tag>.ftl e fetaui.
schema-auto-update = Auto-update
schema-auto-update-detail = Siaki ma faapipii faafouga pe a tatala ma itula taitasi.
schema-startup-url = URL amata
schema-startup-url-detail = Afai e gaogao, e tatala le faatonuga i le pa poloaiga.
schema-search-engine = Masini suʻe
schema-search-engine-detail = Faaaogā mo suʻega web mai le Amata ma le pa poloaiga.
schema-window = Faamalama
schema-pane = Vaega
schema-side-sheet = Pepa autafa
schema-focus-ring = Mama faamamafa
schema-run-placement = Faataga le sui o le nofoaga e tamoʻe ai
schema-run-placement-detail = Faataga sui AI e filifili le faiga o le vaega tamoʻe, itu, ma le taula.
schema-leader = Leader
schema-leader-detail = Ki amata mo ala pupuu chord.
schema-chord-timeout = Taimi faatali chord
schema-chord-timeout-detail = Milisekone a o lei muta le prefix o le chord.
schema-bindings = Bindings
schema-confirm-close = Faamaonia le tapunia
schema-confirm-close-detail = Fesili muamua a o lei tapunia se terminal o loo iai se faagasologa o tamoʻe.
schema-default-theme = Mamanu masani
schema-default-theme-detail = Igoa o le mamanu ola mai le lisi o mamanu.

settings-empty = (gaogao)
settings-none = (leai)

schema-system = Faiga
schema-editor = Fa'atonu
schema-recording = Pu'eina
schema-radius = Fa'ata'amilosaga
schema-padding = Avanoa i totonu
schema-gap = Va
schema-width = Lautele
schema-color = Lanu
schema-red = Mumu
schema-green = Lanumeamata
schema-blue = Lanumoana
schema-follow-files = Mulimuli i faila
schema-tidy-files = Fa'amama faila
schema-tidy-files-max = Tapula'a fa'amama faila
schema-tidy-files-auto = Fa'amama otometi faila
schema-app-providers = 'Au'aunaga polokalame
schema-provider = 'Au'aunaga
schema-kind = Ituaiga
schema-models = Fa'ata'ita'iga
schema-acp = Sui ACP
schema-id = ID
schema-name = Igoa
schema-command = Fa'atonuga
schema-arguments = Finauga
schema-environment = Si'osi'omaga
schema-working-directory = Pusa faila galue
schema-shell = Atigi
schema-font-family = Aiga vai
schema-startup-directory = Pusa faila amata
schema-themes = Autū
schema-color-scheme = Fuafuaga lanu
schema-font-size = Tele o vai
schema-line-height = Maualuga o laina
schema-cursor-style = Sitaili fa'ailo
schema-cursor-blink = Emo o le fa'ailo
schema-custom-themes = Autū fa'apitoa
schema-foreground = Luma
schema-background = Tala'aga
schema-cursor = Fa'ailo
schema-ansi-colors = Lanu ANSI
schema-keymap = Fa'afanua ki
schema-explorer = Su'esu'e faila
schema-visible = Va'aia
schema-language-servers = Tūmau gagana
schema-servers = Tūmau
schema-language-id = ID gagana
schema-root-markers = Fa'ailoga a'a
schema-output-directory = Pusa faila o taunu'uga

menu-scene = Va'aiga
menu-layout = Fa'atulagaga
menu-terminal = Pusa fa'atonu
menu-browser = Su'esu'e
menu-service = Auaunaga
menu-bookmark = Fa'ailoga tusi
menu-edit = Teuteu

layout-knowledge = Malamalama
layout-open-knowledge = Tatala Malamalama
layout-open-welcome-knowledge = Tatala Afio mai i le Malamalama
layout-open-path = Tatala { $path }
layout-fold-knowledge = Gaugau le malamalama
layout-unfold-knowledge = Tatala le malamalama
layout-bookmarks = Fa'ailoga tusi
layout-new-folder = Pusa faila fou
layout-add-to-bookmarks = Fa'aopoopo i Fa'ailoga tusi
layout-move-to-bookmarks = Si'i i Fa'ailoga tusi
layout-stack-number = Fa'aputuga { $number }
layout-fold-stack = Gaugau le fa'aputuga
layout-unfold-stack = Tatala le fa'aputuga
layout-close-stack = Tapuni le fa'aputuga
layout-bookmark-in = Fa'ailoga tusi i { $folder }

common-cancel = Fa‘aleaogā
common-delete = Tape
common-save = Sefe
common-rename = Toe fa‘aigoa
common-expand = Fa‘alautele
common-collapse = Gaugau
common-loading = O lo‘o utaina…
common-error = Sese
common-output = Taunu‘uga
common-pending = O lo‘o fa‘atali
common-current = o lo‘o iai nei
common-stop = Taofi
services-command = ‘Au‘aunaga Vmux
services-uptime-seconds = { $seconds }s
services-uptime-minutes = { $minutes }m { $seconds }s
services-uptime-hours = { $hours }h { $minutes }m
services-uptime-days = { $days }d { $hours }h

error-page-failed-load = Ua lē mafai ona uta le itulau
error-page-not-found = E le‘i maua le itulau
error-unknown-host = E lē iloa le talimalo o le app Vmux: { $host }

history-title = Talafa‘asolopito

command-new-app-chat = Talanoaga fou { $provider }/{ $model } (App)
command-interactive-mode-user = Scene > Faiga Fegalegaleai > Tagata fa‘aaogā
command-interactive-mode-player = Scene > Faiga Fegalegaleai > Ta‘alo
command-minimize-window = Layout > Fa‘amalama > Fa‘aitiiti
command-toggle-layout = Layout > Layout > Fesuia‘i Layout
command-close-tab = Layout > Tab > Tapuni Tab
command-new-task = Layout > Tab > Galuega Fou…
command-next-tab = Layout > Tab > Tab Sosoo
command-prev-tab = Layout > Tab > Tab Muamua
command-rename-tab = Layout > Tab > Toe Fa‘aigoa Tab
command-tab-select-1 = Layout > Tab > Filifili Tab 1
command-tab-select-2 = Layout > Tab > Filifili Tab 2
command-tab-select-3 = Layout > Tab > Filifili Tab 3
command-tab-select-4 = Layout > Tab > Filifili Tab 4
command-tab-select-5 = Layout > Tab > Filifili Tab 5
command-tab-select-6 = Layout > Tab > Filifili Tab 6
command-tab-select-7 = Layout > Tab > Filifili Tab 7
command-tab-select-8 = Layout > Tab > Filifili Tab 8
command-tab-select-last = Layout > Tab > Filifili le Tab Mulimuli
command-close-pane = Layout > Pane > Tapuni Pane
command-select-pane-left = Layout > Pane > Filifili Pane Agavale
command-select-pane-right = Layout > Pane > Filifili Pane Taumatau
command-select-pane-up = Layout > Pane > Filifili Pane Luga
command-select-pane-down = Layout > Pane > Filifili Pane Lalo
command-swap-pane-prev = Layout > Pane > Sui ma le Pane Muamua
command-swap-pane-next = Layout > Pane > Sui ma le Pane Sosoo
command-equalize-pane-size = Layout > Pane > Fa‘atutusa Tele o Pane
command-resize-pane-left = Layout > Pane > Toe Fua Pane i le Agavale
command-resize-pane-right = Layout > Pane > Toe Fua Pane i le Taumatau
command-resize-pane-up = Layout > Pane > Toe Fua Pane i Luga
command-resize-pane-down = Layout > Pane > Toe Fua Pane i Lalo
command-stack-close = Layout > Stack > Tapuni Stack
command-stack-next = Layout > Stack > Stack Sosoo
command-stack-previous = Layout > Stack > Stack Muamua
command-stack-reopen = Layout > Stack > Toe Tatala Itulau na Tapunia
command-stack-swap-prev = Layout > Stack > Si‘i Stack i le Agavale
command-stack-swap-next = Layout > Stack > Si‘i Stack i le Taumatau
command-space-open = Layout > Space > Spaces
command-terminal-close = Terminal > Tapuni Terminal
command-terminal-next = Terminal > Terminal Sosoo
command-terminal-prev = Terminal > Terminal Muamua
command-terminal-clear = Terminal > Fa‘amamā Terminal
command-browser-prev-page = Browser > Folauga > Toe fo‘i
command-browser-next-page = Browser > Folauga > Aga‘i i luma
command-browser-reload = Browser > Folauga > Toe uta
command-browser-hard-reload = Browser > Folauga > Toe uta atoatoa
command-open-in-place = Browser > Tatala > Tatala i Inei
command-open-in-new-stack = Browser > Tatala > Tatala i se Stack Fou
command-open-in-pane-top = Browser > Tatala > Tatala i le Pane i Luga
command-open-in-pane-right = Browser > Tatala > Tatala i le Pane Taumatau
command-open-in-pane-bottom = Browser > Tatala > Tatala i le Pane i Lalo
command-open-in-pane-left = Browser > Tatala > Tatala i le Pane Agavale
command-open-in-new-tab = Browser > Tatala > Tatala i se Tab Fou
command-open-in-new-space = Browser > Tatala > Tatala i se Space Fou
command-browser-zoom-in = Browser > Va‘aiga > Fa‘atele
command-browser-zoom-out = Browser > Va‘aiga > Fa‘aitiiti
command-browser-zoom-reset = Browser > Va‘aiga > Tele Moni
command-browser-dev-tools = Browser > Va‘aiga > Meafaigaluega a Atina‘e
command-browser-open-command-bar = Browser > Pa > Pa Poloa‘iga
command-browser-open-page-in-command-bar = Browser > Pa > Fa‘asa‘o Itulau
command-browser-open-path-bar = Browser > Pa > Folauga Ala
command-browser-open-commands = Browser > Pa > Poloa‘iga
command-browser-open-history = Browser > Pa > Talafa‘asolopito
command-service-open = Service > Tatala Mata‘itu ‘Au‘aunaga
command-bookmark-toggle-active = Bookmark > Fa‘ailoga Itulau
command-bookmark-pin-active = Bookmark > Pine Itulau

layout-tab = Tab
layout-no-stacks = E leai ni stack
layout-loading = O lo‘o utaina…
layout-no-markdown-files = E leai ni faila Markdown
layout-empty-folder = Failaola gaogao
layout-worktree = worktree
layout-folder-name = Igoa failaola
layout-no-pins-bookmarks = E leai ni pine po‘o bookmark
layout-move-to = Si‘i i { $folder }
layout-bookmark-current-page = Fa‘ailoga le Itulau o Iai Nei
layout-rename-folder = Toe Fa‘aigoa Failaola
layout-remove-folder = Ave‘ese Failaola
layout-update-downloading = O lo‘o la‘u mai le fa‘afouga
layout-update-installing = O lo‘o fa‘apipi‘i le fa‘afouga…
layout-update-ready = Ua avanoa se lomiga fou
layout-restart-update = Toe amata e fa‘afou

agent-preparing = O lo‘o sauni le agent…
agent-send-all-queued = Lafo uma prompt o lo‘o i le laina nei (Esc)
agent-send = Lafo (Enter)
agent-ready = Ua sauni pe a e sauni.
agent-loading-older = O lo‘o utaina fe‘au tuai…
agent-load-older = Uta fe‘au tuai
agent-continued-from = Fa‘aauau mai { $source }
agent-older-context-omitted = ua ave‘esea le context tuai
agent-interrupted = motusia
agent-allow-tool = Fa‘ataga { $tool }?
agent-deny = Te‘ena
agent-allow-always = Fa‘ataga i taimi uma
agent-allow = Fa‘ataga
agent-loading-sessions = O lo‘o utaina session…
agent-no-resumable-sessions = E leai ni session e mafai ona toe fa‘aauau
agent-no-matching-sessions = E leai ni session e fetaui
agent-no-matching-models = E leai ni model e fetaui
agent-choice-help = ↑/↓ po‘o Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Filifili failaola repository
agent-choose-repository-detail = Filifili le repository Git i lau masini e fa‘aaogā e le agent.
agent-choosing = O lo‘o filifili…
agent-choose-folder = Filifili failaola
agent-queued = i le laina
agent-attached = Fa‘apipi‘i:
agent-cancel-queued = Fa‘aleaogā prompt o lo‘o i le laina
agent-resume-queued = Toe fa‘aauau prompt o lo‘o i le laina
agent-clear-queue = Fa‘amamā le laina
agent-send-all-now = lafo uma nei
agent-choose-option = Filifili se filifiliga i luga
agent-loading-media = O lo‘o utaina media…
agent-no-matching-media = E leai ni media e fetaui
agent-prompt-context = Context o le prompt
agent-details = Fa‘amatalaga
agent-path = Ala
agent-tool = Meafaigaluega
agent-server = Server
agent-bytes = { $count } bytes
agent-worked-for = Galue mo { $duration }
agent-worked-for-steps = { $count ->
    [one] Galue mo { $duration } · 1 la‘asaga
   *[other] Galue mo { $duration } · { $count } la‘asaga
}
agent-tool-guardian-review = Iloiloga Guardian
agent-tool-read-files = Faitau faila
agent-tool-viewed-image = Matamata i le ata
agent-tool-used-browser = Fa‘aaogā browser
agent-tool-searched-files = Sa‘ili faila
agent-tool-ran-commands = Fa‘agaioi poloa‘iga
agent-thinking = O lo‘o mafaufau
agent-subagent = Subagent
agent-prompt = Prompt
agent-thread = Thread
agent-parent = Matua
agent-children = Fanau
agent-call = Vala‘au
agent-raw-event = Mea na tupu mata
agent-plan = Fuafuaga
agent-tasks = { $count ->
    [one] 1 galuega
   *[other] { $count } galuega
}
agent-edited = Fa‘asa‘oina
agent-reconnecting = O lo‘o toe feso‘ota‘i { $attempt }/{ $total }
agent-status-running = O lo‘o tamo‘e
agent-status-done = Ua mae‘a
agent-status-failed = Ua lē manuia
agent-status-pending = O lo‘o fa‘atali
agent-slash-attach-files = Fa‘apipi‘i faila
agent-slash-resume-session = Toe fa‘aauau se session tuai
agent-slash-select-model = Filifili model
agent-slash-continue-cli = Fa‘aauau lenei session i le CLI
agent-session-just-now = i le taimi nei
agent-session-minutes-ago = { $count }m talu ai
agent-session-hours-ago = { $count }h talu ai
agent-session-days-ago = { $count }d talu ai
agent-working-working = O lo‘o galue
agent-working-thinking = O lo‘o mafaufau
agent-working-pondering = O lo‘o tomanatu
agent-working-noodling = O lo‘o su‘esu‘e manatu
agent-working-percolating = O lo‘o fa‘atupu manatu
agent-working-conjuring = O lo‘o fau mai
agent-working-cooking = O lo‘o kuka
agent-working-brewing = O lo‘o saunia
agent-working-musing = O lo‘o mafaufau loloto
agent-working-ruminating = O lo‘o toe mafaufau
agent-working-scheming = O lo‘o fuafua
agent-working-synthesizing = O lo‘o tu‘ufa‘atasia
agent-working-tinkering = O lo‘o fa‘afofoga
agent-working-churning = O lo‘o gaosia
agent-working-vibing = O lo‘o sologa lelei
agent-working-simmering = O lo‘o fa‘apuna mālie
agent-working-crafting = O lo‘o fau
agent-working-divining = O lo‘o sailiili
agent-working-mulling = O lo‘o iloilo
agent-working-spelunking = O lo‘o su‘e loloto

editor-toggle-explorer = Fesuia‘i Explorer (Cmd+B)
editor-unsaved = e le‘i sefe
editor-rendered-markdown = Markdown ua fa‘aalia ma le fa‘asa‘oga ola
editor-note = Fa‘amatalaga
editor-source-editor = Fa‘asa‘o puna
editor-editor = Fa‘asa‘o
editor-git-diff = Diff Git
editor-diff = Diff
editor-tidy = Fa‘amāmā
editor-always = I taimi uma
editor-unchanged-previews = { $count ->
    [one] ✦ 1 preview e le‘i suia
   *[other] ✦ { $count } preview e le‘i suia
}
editor-open-externally = Tatala i fafo
editor-changed-line = Laina ua suia
editor-go-to-definition = Alu i le Fa‘auigaga
editor-find-references = Su‘e References
editor-references = { $count ->
    [one] 1 reference
   *[other] { $count } references
}
editor-lsp-starting = O lo‘o amata { $server }…
editor-lsp-not-installed = { $server } — e le‘i fa‘apipi‘iina
editor-explorer = Explorer
editor-open-editors = Fa‘asa‘oga Tatala
editor-outline = Auivi
editor-new-file = Faila Fou
editor-new-folder = Failaola Fou
editor-delete-confirm = Tape “{ $name }”? E lē mafai ona toe fa‘afo‘i.
editor-created-folder = Ua fai le failaola { $name }
editor-created-file = Ua fai le faila { $name }
editor-renamed-to = Ua toe fa‘aigoa i { $name }
editor-deleted = Ua tape { $name }
editor-failed-decode-image = Ua lē mafai ona decode le ata
editor-preview-large-image = ata (tele tele mo le preview)
editor-preview-binary = binary
editor-preview-file = faila

git-status-clean = mamā
git-status-modified = suia
git-status-staged = staged
git-status-staged-modified = staged*
git-status-untracked = lē tulitataoina
git-status-deleted = tape
git-status-conflict = fete‘ena‘i
git-accept-all = ✓ talia uma
git-unstage = Ave‘ese mai stage
git-confirm-deny-all = Fa‘amaonia le te‘ena uma
git-deny-all = ✗ te‘ena uma
git-commit-message = fe‘au commit
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = O lo‘o utaina diff…
git-no-changes = E leai ni suiga e fa‘aali
git-accept = ✓ talia
git-deny = ✗ te‘ena
git-show-unchanged-lines = Fa‘aali { $count } laina e le‘i suia

terminal-loading = O lo‘o utaina…
terminal-runs-when-ready = e tamo‘e pe a sauni · Ctrl+C e fa‘amamā · Esc e fa‘amisi
terminal-booting = o lo‘o amata
terminal-type-command = ta se poloa‘iga · tamo‘e pe a sauni · Esc e fa‘amisi

setup-tagline-claude = Agent coding a Anthropic, i Vmux
setup-tagline-codex = Agent coding a OpenAI, i Vmux
setup-tagline-vibe = Agent coding a Mistral, i Vmux
setup-install-title = Fa‘apipi‘i { $name } CLI
setup-homebrew-required = E mana‘omia Homebrew e fa‘apipi‘i ai { $command }, ae e le‘i setiina. O le a fa‘apipi‘i muamua e Vmux Homebrew, ona fa‘apipi‘i lea o { $name }.
setup-terminal-instructions = I le terminal, oomi Return e amata ai, ona tu‘u lea o lau upu fa‘alilolilo Mac pe a fesiligia.
setup-command-missing = Na tatala e Vmux lenei itulau ona e le‘i fa‘apipi‘iina le poloa‘iga { $command } i lau masini. Fa‘agaioi le poloa‘iga i lalo e maua ai.
setup-install-failed = E le‘i mae‘a le fa‘apipi‘iga. Siaki le terminal mo fa‘amatalaga, ona toe taumafai lea.
setup-installing = O lo‘o fa‘apipi‘i…
setup-install-homebrew = Fa‘apipi‘i Homebrew + { $name }
setup-run-install = Fa‘agaioi le poloa‘iga fa‘apipi‘i
setup-auto-reload = E fa‘agaioi e Vmux i se terminal ma toe uta pe a sauni { $command }.

debug-title = Debug
debug-auto-update = Fa‘afou otometi
debug-simulate-update = Fa‘ata‘ita‘i ua avanoa se fa‘afouga
debug-simulate-download = Fa‘ata‘ita‘i le la‘u mai
debug-clear-update = Fa‘amamā fa‘afouga
debug-trigger-restart = Fa‘atupu toe amata

command-manage-spaces = Pulea avanoa…
command-pane-stack-location = vaega { $pane } / faaputuga { $stack }
command-space-pane-stack-location = { $space } / vaega { $pane } / faaputuga { $stack }
command-terminal-path = Temina ({ $path })
command-group-interactive-mode = Faiga fegalegaleaʻi
command-group-window = Faamalama
command-group-tab = Laupepa
command-group-pane = Vaega
command-group-stack = Faaputuga
command-group-space = Avanoa
command-group-navigation = Folauga
command-group-open = Tatala
command-group-view = Vaʻai
command-group-bar = Pa

menu-close-vmux = Tapuni Vmux

agents-terminal-coding-agent = Sui tusikōdē e faʻavae i le Temina
