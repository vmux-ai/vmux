locale-name = Lëtzebuergesch
common-open = Opmaachen
common-close = Zoumaachen
common-install = Installéieren
common-uninstall = Deinstalléieren
common-update = Aktualiséieren
common-retry = Nach eng Kéier probéieren
common-refresh = Nei lueden
common-remove = Ewechhuelen
common-enable = Aktivéieren
common-disable = Desaktivéieren
common-new = Nei
common-active = aktiv
common-running = leeft
common-done = fäerdeg
common-failed = Ausgefall
common-installed = Installéiert
common-items = { $count ->
    [one] { $count } Element
   *[other] { $count } Elementer
}

tools-title = Instrumenter
tools-search = Sich no Packagen, Agenten, MCP, Sproochtools a Konfiguratiounsdateien…
tools-open = Tools opmaachen
tools-fold = Tools zouklappen
tools-unfold = Tools opklappen
tools-scanning = Lokal Tools ginn duerchsicht…
tools-no-installed = Keng Tools installéiert
tools-empty = Keng passend Tools
tools-empty-detail = Installéiert e Package oder füügt e Stow-ähnleche Package mat Konfiguratiounsdateien derbäi.
tools-apply = Uwenden
tools-homebrew = Homebrew
tools-homebrew-sync = Installéiert Formelen an Applikatioune ginn automatesch synchroniséiert.
tools-open-brewfile = Brewfile opmaachen
tools-managed = verwalt
tools-provider-homebrew-formulae = Homebrew-Formelen
tools-provider-homebrew-casks = Homebrew-Applikatiounen
tools-provider-npm = npm-Packagen
tools-provider-acp-agents = ACP-Agenten
tools-provider-language-tools = Sproochtools
tools-provider-mcp-servers = MCP-Serveren
tools-provider-dotfiles = Konfiguratiounsdateien
tools-status-available = Verfügbar
tools-status-missing = Feelt
tools-status-conflict = Konflikt
tools-forget = Vergiessen
tools-manage = Verwalten
tools-link = Verknëppen
tools-unlink = Verknëppung léisen
tools-import = Importéieren
tools-update-count = { $count ->
    [one] 1 Aktualiséierung
   *[other] { $count } Aktualiséierungen
}
tools-conflict-count = { $count ->
    [one] 1 Konflikt
   *[other] { $count } Konflikter
}
tools-result-applied = Tools ugewannt
tools-result-imported = Tools importéiert
tools-result-installed = { $name } installéiert
tools-result-updated = { $name } aktualiséiert
tools-result-uninstalled = { $name } deinstalléiert
tools-result-forgotten = { $name } vergiess
tools-result-managed = { $name } gëtt elo verwalt
tools-result-linked = { $name } verknëppt
tools-result-unlinked = { $name } net méi verknëppt
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Synchroniséiert Astellungen, Tools, Dotfiles, a Wëssen mat Git.
vault-sync = Synchroniséiert
vault-create = Schafen
vault-connect = Connect
vault-private = Privat Repository
vault-public-warning = Ëffentlech Repositories beliicht Äert Wëssen a Konfiguratioun.
vault-choose-repository = Wielt e Repository ...
vault-empty = eidel
vault-clean = Aktuell
vault-not-connected = Net ugeschloss
vault-change-count = Ännerungen: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Start
start-tagline = Eng Uweisung. Alles gemaach.

agents-title = Agenten
agents-search = ACP- a CLI-Agenten sichen…
agents-empty = Keng passend Agenten
agents-empty-detail = Probéier en Numm, Runtime oder ACP/CLI.
agents-install-failed = Installatioun ausgefall
agents-updating = Gëtt aktualiséiert…
agents-retrying = Gëtt nach eng Kéier probéiert…
agents-preparing = Gëtt virbereet…

extensions-title = Erweiderungen
extensions-search = Installéiert Erweiderungen oder Chrome Web Store duerchsichen…
extensions-relaunch = Neistarten, fir z’iwwerhuelen
extensions-empty = Keng Erweiderungen installéiert
extensions-no-match = Keng passend Erweiderungen
extensions-empty-detail = Sicht uewen am Chrome Web Store an dréckt Enter.
extensions-no-match-detail = Probéier en aneren Numm oder eng Erweiderungs-ID.
extensions-on = Un
extensions-off = Aus
extensions-enable-confirm = { $name } aktivéieren?
extensions-enable-permissions = { $name } aktivéieren an erlaben:

lsp-title = Sproochserveren
lsp-search = Sproochserveren, Linter a Formatter sichen…
lsp-loading = Katalog gëtt gelueden…
lsp-empty = Keng passend Sproochserveren
lsp-empty-detail = Probéier eng aner Sprooch, e Linter oder e Formatter.
lsp-needs = brauch { $tool }
lsp-status-available = Verfügbar
lsp-status-on-path = Am PATH
lsp-status-installing = Gëtt installéiert…
lsp-status-installed = Installéiert
lsp-status-outdated = Aktualiséierung verfügbar
lsp-status-running = Leeft
lsp-status-failed = Ausgefall

spaces-title = Beräicher
spaces-new-placeholder = Numm vum neie Beräich
spaces-empty = Keng Beräicher
spaces-default-name = Beräich { $number }
spaces-tabs = { $count ->
    [one] 1 Tab
   *[other] { $count } Tabs
}
spaces-delete = Beräich läschen

team-title = Team
team-just-you = Just du an dësem Beräich
team-agents = { $count ->
    [one] Du an 1 Agent
   *[other] Du an { $count } Agenten
}
team-empty = Nach keen hei
team-you = Du
team-agent = Agent

services-title = Hannergronddéngschter
services-processes = { $count ->
    [one] 1 Prozess
   *[other] { $count } Prozesser
}
services-kill-all = All ofbriechen
services-not-running = Déngscht leeft net
services-start-with = Starten mat:
services-empty = Keng aktiv Prozesser
services-filter = Prozesser filteren…
services-no-match = Keng passend Prozesser
services-connected = Verbonnen
services-disconnected = Net verbonnen
services-attached = ugeschloss
services-kill = Ofbriechen
services-memory = Späicher
services-size = Gréisst
services-shell = Shell

error-title = Feeler

history-search = Verlaf sichen
history-clear-all = Alles läschen
history-clear-confirm = Ganze Verlaf läschen?
history-clear-warning = Dat kann net réckgängeg gemaach ginn.
history-cancel = Ofbriechen
history-today = Haut
history-yesterday = Gëschter
history-days-ago = Viru { $count } Deeg
history-day-offset = Dag -{ $count }

settings-title = Astellungen
settings-loading = Astellunge ginn gelueden…
settings-stored = Gespäichert an ~/.vmux/settings.ron
settings-other = Aneres
settings-software-update = Software-Aktualiséierung
settings-check-updates = No Aktualiséierunge sichen
settings-check-updates-hint = Sicht automatesch beim Start an all Stonn, wann Auto-update aktivéiert ass.
settings-update-unavailable = Net verfügbar
settings-update-unavailable-hint = Den Updater ass net an dësem Build dran.
settings-update-checking = Gëtt gepréift…
settings-update-checking-hint = Aktualiséierunge ginn iwwerpréift…
settings-update-check-again = Nach eng Kéier préiwen
settings-update-current = Vmux ass aktuell.
settings-update-downloading = Gëtt erofgelueden…
settings-update-downloading-hint = Vmux { $version } gëtt erofgelueden…
settings-update-installing = Gëtt installéiert…
settings-update-installing-hint = Vmux { $version } gëtt installéiert…
settings-update-ready = Aktualiséierung prett
settings-update-ready-hint = Vmux { $version } ass prett. Neistarten, fir se z’iwwerhuelen.
settings-update-try-again = Nach eng Kéier probéieren
settings-update-failed = Aktualiséierunge konnten net iwwerpréift ginn.
settings-item = Element
settings-item-number = Element { $number }
settings-press-key = Dréck eng Tast…
settings-saved = Gespäichert
settings-record-key = Klicken, fir eng nei Tastekombinatioun opzehuelen

tray-open-window = Fënster opmaachen
tray-close-window = Fënster zoumaachen
tray-pause-recording = Opnam pauséieren
tray-resume-recording = Opnam weiderféieren
tray-finish-recording = Opnam ofschléissen
tray-quit = Vmux ophalen

composer-attach-files = Dateien unhänken (/upload)
composer-remove-attachment = Unhang ewechhuelen

layout-back = Zeréck
layout-forward = Vir
layout-reload = Nei lueden
layout-bookmark-page = Dës Säit als Lieszeeche späicheren
layout-remove-bookmark = Lieszeechen ewechhuelen
layout-pin-page = Dës Säit uheften
layout-unpin-page = Dës Säit lassmaachen
layout-manage-extensions = Erweiderunge verwalten
layout-new-stack = Neie Stack
layout-close-tab = Tab zoumaachen
layout-bookmark = Lieszeechen
layout-pin = Uheften
layout-new-tab = Neien Tab
layout-team = Team

command-switch-space = Beräich wiesselen…
command-search-ask = Sichen oder froen…
command-new-tab-placeholder = Sich oder URL aginn, oder Terminal auswielen…
command-placeholder = URL aginn, Tabs sichen oder > fir Befeele benotzen…
command-composer-placeholder = / fir Befeele oder @ fir Medien aginn
command-send = Schécken (Enter)
command-terminal = Terminal
command-open-terminal = Am Terminal opmaachen
command-stack = Stack
command-tabs = { $count ->
    [one] 1 Tab
   *[other] { $count } Tabs
}
command-prompt = Uweisung
command-new-tab = Neien Tab
command-search = Sichen
command-open-value = „{ $value }“ opmaachen
command-search-value = „{ $value }“ sichen

schema-appearance = Ausgesinn
schema-general = Allgemeng
schema-layout = Layout
schema-layout-detail = Fënster, Fënsterdeeler, Säiteleeschten a Fokusram.
schema-agent = Agent
schema-agent-detail = Agent-Verhalen an Tool-Berechtegungen.
schema-shortcuts = Tastaturkierzel
schema-shortcuts-detail = Nëmme Liesusiicht. Änner d’Beleeungen direkt an settings.ron.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Modus
schema-mode-detail = Faarfschema fir Websäiten. Apparat follegt dengem System.
schema-device = Apparat
schema-light = Hell
schema-dark = Donkel
schema-language = Sprooch
schema-language-detail = Benotz System, en-US, ja oder all BCP 47-Tag mat engem passende ~/.vmux/locales/<tag>.ftl-Katalog.
schema-auto-update = Auto-update
schema-auto-update-detail = Beim Start an all Stonn no Aktualiséierunge sichen an se installéieren.
schema-startup-url = Start-URL
schema-startup-url-detail = Eidel mécht d’Uweisungsfeld vun der Befehlsleeschten op.
schema-search-engine = Sichmaschinn
schema-search-engine-detail = Fir Websich vu Start an aus der Befehlsleeschten.
schema-window = Fënster
schema-pane = Fënsterdeel
schema-side-sheet = Säitepanel
schema-focus-ring = Fokusram
schema-run-placement = Iwwerschreiwe vun der Ausféierungsplaz erlaben
schema-run-placement-detail = Agenten däerfen Modus, Richtung an Anker vum Ausféierungs-Fënsterdeel wielen.
schema-leader = Leader
schema-leader-detail = Präfix-Tast fir Akkord-Kierzelen.
schema-chord-timeout = Akkord-Timeout
schema-chord-timeout-detail = Millisekonnen, bis en Akkord-Präfix ofleeft.
schema-bindings = Beleeungen
schema-confirm-close = Zoumaache bestätegen
schema-confirm-close-detail = Nofroen, ier en Terminal mat lafendem Prozess zougemaach gëtt.
schema-default-theme = Standard-Theme
schema-default-theme-detail = Numm vum aktive Theme aus der Theme-Lëscht.

settings-empty = (eidel)
settings-none = (keen)

schema-system = System
schema-editor = Editeur
schema-recording = Opnam
schema-radius = Radius
schema-padding = Banneofstand
schema-gap = Ofstand
schema-width = Breet
schema-color = Faarf
schema-red = Rout
schema-green = Gréng
schema-blue = Blo
schema-follow-files = Dateien suivéieren
schema-tidy-files = Dateien opraumen
schema-tidy-files-max = Schwellwäert fir Dateienopraumen
schema-tidy-files-auto = Dateien automatesch opraumen
schema-app-providers = App-Ubidder
schema-provider = Ubidder
schema-kind = Aart
schema-models = Modeller
schema-acp = ACP-Agenten
schema-id = ID
schema-name = Numm
schema-command = Kommando
schema-arguments = Argumenter
schema-environment = Ëmfeldvariabelen
schema-working-directory = Aarbechtsverzeechnes
schema-shell = Shell
schema-font-family = Schrëftfamill
schema-startup-directory = Startverzeechnes
schema-themes = Themen
schema-color-scheme = Faarfschema
schema-font-size = Schrëftgréisst
schema-line-height = Zeilenhéicht
schema-cursor-style = Cursor-Stil
schema-cursor-blink = Cursor blénkt
schema-custom-themes = Benotzerdefinéiert Themen
schema-foreground = Virdergrond
schema-background = Hannergrond
schema-cursor = Cursor
schema-ansi-colors = ANSI-Faarwen
schema-keymap = Tastebeleeung
schema-explorer = Explorer
schema-visible = Siichtbar
schema-language-servers = Sproochserveren
schema-servers = Serveren
schema-language-id = Sprooch-ID
schema-root-markers = Root-Markéierer
schema-output-directory = Ausgabeverzeechnes

menu-scene = Zeen
menu-layout = Layout
menu-terminal = Terminal
menu-browser = Browser
menu-service = Déngscht
menu-bookmark = Lieszeechen
menu-edit = Änneren

layout-knowledge = Wëssen
layout-open-knowledge = Wëssen opmaachen
layout-open-welcome-knowledge = Wëllkomm am Wëssen opmaachen
layout-open-path = { $path } opmaachen
layout-fold-knowledge = Wëssen aklappen
layout-unfold-knowledge = Wëssen opklappen
layout-bookmarks = Lieszeechen
layout-new-folder = Neien Dossier
layout-add-to-bookmarks = Bei d'Lieszeechen derbäisetzen
layout-move-to-bookmarks = An d'Lieszeeche réckelen
layout-stack-number = Stapel { $number }
layout-fold-stack = Stapel aklappen
layout-unfold-stack = Stapel opklappen
layout-close-stack = Stapel zoumaachen
layout-bookmark-in = Als Lieszeechen an { $folder } späicheren

common-cancel = Ofbriechen
common-delete = Läschen
common-save = Späicheren
common-rename = Ëmbenennen
common-expand = Ausklappen
common-collapse = Zesummeklappen
common-loading = Lued…
common-error = Feeler
common-output = Ausgab
common-pending = Ausstoend
common-current = aktuell
common-stop = Stoppen
services-command = Vmux-Déngscht
services-uptime-seconds = { $seconds }s
services-uptime-minutes = { $minutes }m { $seconds }s
services-uptime-hours = { $hours }h { $minutes }m
services-uptime-days = { $days }d { $hours }h

error-page-failed-load = Säit konnt net geluede ginn
error-page-not-found = Säit net fonnt
error-unknown-host = Onbekannte Vmux-App-Host: { $host }

history-title = Verlaf

command-new-app-chat = Neien { $provider }/{ $model }-Chat (App)
command-interactive-mode-user = Zeen > Interaktive Modus > Benotzer
command-interactive-mode-player = Zeen > Interaktive Modus > Spiller
command-minimize-window = Layout > Fënster > Miniméieren
command-toggle-layout = Layout > Layout > Layout wiesselen
command-close-tab = Layout > Tab > Tab zoumaachen
command-new-task = Layout > Tab > Nei Aufgab…
command-next-tab = Layout > Tab > Nächsten Tab
command-prev-tab = Layout > Tab > Viregten Tab
command-rename-tab = Layout > Tab > Tab ëmbenennen
command-tab-select-1 = Layout > Tab > Tab 1 auswielen
command-tab-select-2 = Layout > Tab > Tab 2 auswielen
command-tab-select-3 = Layout > Tab > Tab 3 auswielen
command-tab-select-4 = Layout > Tab > Tab 4 auswielen
command-tab-select-5 = Layout > Tab > Tab 5 auswielen
command-tab-select-6 = Layout > Tab > Tab 6 auswielen
command-tab-select-7 = Layout > Tab > Tab 7 auswielen
command-tab-select-8 = Layout > Tab > Tab 8 auswielen
command-tab-select-last = Layout > Tab > Leschten Tab auswielen
command-close-pane = Layout > Beräich > Beräich zoumaachen
command-select-pane-left = Layout > Beräich > Lénkse Beräich auswielen
command-select-pane-right = Layout > Beräich > Rietse Beräich auswielen
command-select-pane-up = Layout > Beräich > Beräich uewen auswielen
command-select-pane-down = Layout > Beräich > Beräich ënnen auswielen
command-swap-pane-prev = Layout > Beräich > Mat viregtem Beräich tauschen
command-swap-pane-next = Layout > Beräich > Mat nächstem Beräich tauschen
command-equalize-pane-size = Layout > Beräich > Beräichgréissten ausgläichen
command-resize-pane-left = Layout > Beräich > Beräich no lénks vergréisseren
command-resize-pane-right = Layout > Beräich > Beräich no riets vergréisseren
command-resize-pane-up = Layout > Beräich > Beräich no uewen vergréisseren
command-resize-pane-down = Layout > Beräich > Beräich no ënnen vergréisseren
command-stack-close = Layout > Stack > Stack zoumaachen
command-stack-next = Layout > Stack > Nächste Stack
command-stack-previous = Layout > Stack > Viregte Stack
command-stack-reopen = Layout > Stack > Zougemaachte Säit nei opmaachen
command-stack-swap-prev = Layout > Stack > Stack no lénks réckelen
command-stack-swap-next = Layout > Stack > Stack no riets réckelen
command-space-open = Layout > Space > Spaces
command-terminal-close = Terminal > Terminal zoumaachen
command-terminal-next = Terminal > Nächsten Terminal
command-terminal-prev = Terminal > Viregten Terminal
command-terminal-clear = Terminal > Terminal läschen
command-browser-prev-page = Browser > Navigatioun > Zréck
command-browser-next-page = Browser > Navigatioun > Virun
command-browser-reload = Browser > Navigatioun > Nei lueden
command-browser-hard-reload = Browser > Navigatioun > Komplett nei lueden
command-open-in-place = Browser > Opmaachen > Hei opmaachen
command-open-in-new-stack = Browser > Opmaachen > An neiem Stack opmaachen
command-open-in-pane-top = Browser > Opmaachen > Am Beräich driwwer opmaachen
command-open-in-pane-right = Browser > Opmaachen > Am rietse Beräich opmaachen
command-open-in-pane-bottom = Browser > Opmaachen > Am Beräich drënner opmaachen
command-open-in-pane-left = Browser > Opmaachen > Am lénkse Beräich opmaachen
command-open-in-new-tab = Browser > Opmaachen > An neiem Tab opmaachen
command-open-in-new-space = Browser > Opmaachen > An neiem Space opmaachen
command-browser-zoom-in = Browser > Usicht > Vergréisseren
command-browser-zoom-out = Browser > Usicht > Verklengeren
command-browser-zoom-reset = Browser > Usicht > Tatsächlech Gréisst
command-browser-dev-tools = Browser > Usicht > Entwéckler-Tools
command-browser-open-command-bar = Browser > Bar > Kommando-Bar
command-browser-open-page-in-command-bar = Browser > Bar > Säit änneren
command-browser-open-path-bar = Browser > Bar > Pad-Navigator
command-browser-open-commands = Browser > Bar > Kommandoen
command-browser-open-history = Browser > Bar > Verlaf
command-service-open = Déngscht > Déngschtmonitor opmaachen
command-bookmark-toggle-active = Lieszeechen > Säit als Lieszeeche späicheren
command-bookmark-pin-active = Lieszeechen > Säit festmaachen

layout-tab = Tab
layout-no-stacks = Keng Stacks
layout-loading = Lued…
layout-no-markdown-files = Keng Markdown-Dateien
layout-empty-folder = Eidelen Dossier
layout-worktree = worktree
layout-folder-name = Dossiersnumm
layout-no-pins-bookmarks = Keng Pins oder Lieszeechen
layout-move-to = Op { $folder } réckelen
layout-bookmark-current-page = Aktuell Säit als Lieszeeche späicheren
layout-rename-folder = Dossier ëmbenennen
layout-remove-folder = Dossier ewechhuelen
layout-update-downloading = Update gëtt erofgelueden
layout-update-installing = Update gëtt installéiert…
layout-update-ready = Nei Versioun disponibel
layout-restart-update = Neistarten, fir ze aktualiséieren

agent-preparing = Agent gëtt virbereet…
agent-send-all-queued = All gewaart Prompts elo schécken (Esc)
agent-send = Schécken (Enter)
agent-ready = Prett, wann Dir et sidd.
agent-loading-older = Eeler Messagë ginn gelueden…
agent-load-older = Eeler Messagë lueden
agent-continued-from = Weidergefouert vu(n) { $source }
agent-older-context-omitted = eelere Kontext ausgelooss
agent-interrupted = ënnerbrach
agent-allow-tool = { $tool } erlaben?
agent-deny = Ofleenen
agent-allow-always = Ëmmer erlaben
agent-allow = Erlaben
agent-loading-sessions = Sessioune ginn gelueden…
agent-no-resumable-sessions = Keng Sessioune fonnt, déi weidergefouert kënne ginn
agent-no-matching-sessions = Keng passend Sessiounen
agent-no-matching-models = Keng passend Modeller
agent-choice-help = ↑/↓ oder Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Repository-Dossier auswielen
agent-choose-repository-detail = Wielt de lokale Git-Repository, deen den Agent benotze soll.
agent-choosing = Wielt…
agent-choose-folder = Dossier auswielen
agent-queued = an der Waardeschlaang
agent-attached = Ugehaangen:
agent-cancel-queued = Prompt aus der Waardeschlaang ofbriechen
agent-resume-queued = Prompts aus der Waardeschlaang weiderféieren
agent-clear-queue = Waardeschlaang läschen
agent-send-all-now = elo all schécken
agent-choose-option = Wielt eng Optioun uewen
agent-loading-media = Medien gi gelueden…
agent-no-matching-media = Keng passend Medien
agent-prompt-context = Prompt-Kontext
agent-details = Detailer
agent-path = Pad
agent-tool = Tool
agent-server = Server
agent-bytes = { $count } Bytes
agent-worked-for = { $duration } geschafft
agent-worked-for-steps = { $count ->
    [one] { $duration } geschafft · 1 Schrëtt
   *[other] { $duration } geschafft · { $count } Schrëtt
}
agent-tool-guardian-review = Guardian-Iwwerpréiwung
agent-tool-read-files = Dateien gelies
agent-tool-viewed-image = Bild gekuckt
agent-tool-used-browser = Browser benotzt
agent-tool-searched-files = Dateien duerchsicht
agent-tool-ran-commands = Kommandoen ausgefouert
agent-thinking = Denkt
agent-subagent = Ënneragent
agent-prompt = Prompt
agent-thread = Thread
agent-parent = Elter
agent-children = Kanner
agent-call = Opruff
agent-raw-event = Réi Event
agent-plan = Plang
agent-tasks = { $count ->
    [one] 1 Aufgab
   *[other] { $count } Aufgaben
}
agent-edited = Geännert
agent-reconnecting = Nees verbannen { $attempt }/{ $total }
agent-status-running = Leeft
agent-status-done = Fäerdeg
agent-status-failed = Gescheitert
agent-status-pending = Ausstoend
agent-slash-attach-files = Dateien uhaangen
agent-slash-resume-session = Eng fréier Sessioun weiderféieren
agent-slash-select-model = Modell auswielen
agent-slash-continue-cli = Dës Sessioun am CLI weiderféieren
agent-session-just-now = grad elo
agent-session-minutes-ago = viru(n) { $count }m
agent-session-hours-ago = viru(n) { $count }h
agent-session-days-ago = viru(n) { $count }d
agent-working-working = Schafft
agent-working-thinking = Denkt
agent-working-pondering = Iwwerleet
agent-working-noodling = Tüftelt
agent-working-percolating = Bridd
agent-working-conjuring = Zaubert
agent-working-cooking = Kacht
agent-working-brewing = Bréit
agent-working-musing = Sinnéiert
agent-working-ruminating = Gruebelt
agent-working-scheming = Plangt
agent-working-synthesizing = Synthetiséiert
agent-working-tinkering = Bastelt
agent-working-churning = Schafft sech duerch
agent-working-vibing = Vibet
agent-working-simmering = Simmeréiert
agent-working-crafting = Formt
agent-working-divining = Spiert no
agent-working-mulling = Iwwerleet
agent-working-spelunking = Grueft sech eran

editor-toggle-explorer = Explorer wiesselen (Cmd+B)
editor-unsaved = net gespäichert
editor-rendered-markdown = Gerendert Markdown mat Live-Änneren
editor-note = Notiz
editor-source-editor = Quelltext-Editor
editor-editor = Editor
editor-git-diff = Git-Diff
editor-diff = Diff
editor-tidy = Opriichten
editor-always = Ëmmer
editor-unchanged-previews = { $count ->
    [one] ✦ 1 onverännert Virschau
   *[other] ✦ { $count } onverännert Virschauen
}
editor-open-externally = Extern opmaachen
editor-changed-line = Geännert Zeil
editor-go-to-definition = Bei d'Definitioun goen
editor-find-references = Referenze fannen
editor-references = { $count ->
    [one] 1 Referenz
   *[other] { $count } Referenzen
}
editor-lsp-starting = { $server } start…
editor-lsp-not-installed = { $server } — net installéiert
editor-explorer = Explorer
editor-open-editors = Oppe Editoren
editor-outline = Iwwersiicht
editor-new-file = Nei Datei
editor-new-folder = Neien Dossier
editor-delete-confirm = „{ $name }“ läschen? Dat kann net réckgängeg gemaach ginn.
editor-created-folder = Dossier { $name } erstallt
editor-created-file = Datei { $name } erstallt
editor-renamed-to = Ëmbenannt op { $name }
editor-deleted = { $name } geläscht
editor-failed-decode-image = Bild konnt net dekodéiert ginn
editor-preview-large-image = Bild (ze grouss fir d'Virschau)
editor-preview-binary = binär
editor-preview-file = Datei

git-status-clean = propper
git-status-modified = geännert
git-status-staged = staged
git-status-staged-modified = staged*
git-status-untracked = net verfollegt
git-status-deleted = geläscht
git-status-conflict = Konflikt
git-accept-all = ✓ all unhuelen
git-unstage = Aus dem Stage huelen
git-confirm-deny-all = Alles ofleenen confirméieren
git-deny-all = ✗ alles ofleenen
git-commit-message = Commit-Message
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Diff gëtt gelueden…
git-no-changes = Keng Ännerungen ze weisen
git-accept = ✓ unhuelen
git-deny = ✗ ofleenen
git-show-unchanged-lines = { $count } onverännert Zeile weisen

terminal-loading = Lued…
terminal-runs-when-ready = leeft wann et prett ass · Ctrl+C läscht · Esc spréngt iwwer
terminal-booting = start
terminal-type-command = Kommando aginn · leeft wann et prett ass · Esc spréngt iwwer

setup-tagline-claude = Dem Anthropic säi Coding-Agent, a Vmux
setup-tagline-codex = Dem OpenAI säi Coding-Agent, a Vmux
setup-tagline-vibe = Dem Mistral säi Coding-Agent, a Vmux
setup-install-title = { $name } CLI installéieren
setup-homebrew-required = Homebrew ass néideg, fir { $command } ze installéieren, an ass nach net ageriicht. Vmux installéiert fir d'éischt Homebrew, duerno { $name }.
setup-terminal-instructions = Dréckt am Terminal op Return, fir ze starten, a gitt dann Äert Mac-Passwuert an, wann Dir gefrot gitt.
setup-command-missing = Vmux huet dës Säit opgemaach, well de lokale Kommando { $command } nach net installéiert ass. Féiert de Kommando hei ënnen aus, fir en ze kréien.
setup-install-failed = Installatioun gouf net ofgeschloss. Kuckt am Terminal no Detailer a probéiert et nach eng Kéier.
setup-installing = Installéiert…
setup-install-homebrew = Homebrew + { $name } installéieren
setup-run-install = Installatiounskommando ausféieren
setup-auto-reload = Vmux féiert en an engem Terminal aus a lued nei, wann { $command } prett ass.

debug-title = Debug
debug-auto-update = Automatesch aktualiséieren
debug-simulate-update = Disponibelen Update simuléieren
debug-simulate-download = Download simuléieren
debug-clear-update = Update läschen
debug-trigger-restart = Neistart ausléisen

command-manage-spaces = Spacë verwalten…
command-pane-stack-location = Beräich { $pane } / Stack { $stack }
command-space-pane-stack-location = { $space } / Beräich { $pane } / Stack { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Interaktive Modus
command-group-window = Fënster
command-group-tab = Tab
command-group-pane = Beräich
command-group-stack = Stack
command-group-space = Space
command-group-navigation = Navigatioun
command-group-open = Opmaachen
command-group-view = Vue
command-group-bar = Bar

menu-close-vmux = Vmux zoumaachen

agents-terminal-coding-agent = Terminal-baséierte Coding-Agent
