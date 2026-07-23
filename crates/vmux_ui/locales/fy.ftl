locale-name = Frysk
common-open = Iepenje
common-close = Slute
common-install = Ynstallearje
common-uninstall = De-ynstallearje
common-update = Bywurkje
common-retry = Opnij besykje
common-refresh = Fernije
common-remove = Fuortsmite
common-enable = Ynskeakelje
common-disable = Utskeakelje
common-new = Nij
common-active = aktyf
common-running = rint
common-done = klear
common-failed = Mislearre
common-installed = Ynstallearre
common-items = { $count ->
    [one] { $count } item
   *[other] { $count } items
}

tools-title = Ark
tools-search = Sykje nei pakketten, aginten, MCP, taalark en konfiguraasjebestannen…
tools-open = Ark iepenje
tools-fold = Ark ynklappe
tools-unfold = Ark útklappe
tools-scanning = Lokale ark wurdt skand…
tools-no-installed = Gjin ark ynstallearre
tools-empty = Gjin oerienkommende ark
tools-empty-detail = Ynstallearje in pakket of foegje in pakket mei konfiguraasjebestannen yn Stow-styl ta.
tools-apply = Tapasse
tools-homebrew = Homebrew
tools-homebrew-sync = Ynstallearre formules en tapassingen wurde automatysk syngronisearre.
tools-open-brewfile = Brewfile iepenje
tools-managed = beheard
tools-provider-homebrew-formulae = Homebrew-formules
tools-provider-homebrew-casks = Homebrew-tapassingen
tools-provider-npm = npm-pakketten
tools-provider-acp-agents = ACP-aginten
tools-provider-language-tools = Taalark
tools-provider-mcp-servers = MCP-tsjinners
tools-provider-dotfiles = Konfiguraasjebestannen
tools-status-available = Beskikber
tools-status-missing = Untbrekt
tools-status-conflict = Konflikt
tools-forget = Ferjitte
tools-manage = Beheare
tools-link = Keppelje
tools-unlink = Untkeppelje
tools-import = Ymportearje
tools-update-count = { $count ->
    [one] 1 fernijing
   *[other] { $count } fernijingen
}
tools-conflict-count = { $count ->
    [one] 1 konflikt
   *[other] { $count } konflikten
}
tools-result-applied = Ark tapast
tools-result-imported = Ark ymportearre
tools-result-installed = { $name } ynstallearre
tools-result-updated = { $name } fernijd
tools-result-uninstalled = { $name } de-ynstallearre
tools-result-forgotten = { $name } fergetten
tools-result-managed = { $name } wurdt no beheard
tools-result-linked = { $name } keppele
tools-result-unlinked = { $name } ûntkeppele
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Syngronisearje ynstellings, ark, dotfiles en kennis mei Git.
vault-sync = Syngronisearje
vault-create = Meitsje
vault-connect = Ferbine
vault-private = Private repository
vault-public-warning = Iepenbiere repositories bleatstelle jo Kennis en konfiguraasje.
vault-choose-repository = Kies in repository ...
vault-empty = leech
vault-clean = Aktueel
vault-not-connected = Net ferbûn
vault-change-count = Feroarings: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Start
start-tagline = Ien prompt. Alles dien.

agents-title = Agents
agents-search = Sykje ACP- en CLI-agents…
agents-empty = Gjin oerienkommende agents
agents-empty-detail = Besykje in namme, runtime of ACP/CLI.
agents-install-failed = Ynstallaasje mislearre
agents-updating = Wurdt bywurke…
agents-retrying = Wurdt opnij besocht…
agents-preparing = Wurdt taret…

extensions-title = Utwreidingen
extensions-search = Sykje ynstallearre of Chrome Web Store…
extensions-relaunch = Opnij starte om ta te passen
extensions-empty = Gjin utwreidingen ynstallearre
extensions-no-match = Gjin oerienkommende utwreidingen
extensions-empty-detail = Sykje hjirboppe yn de Chrome Web Store en druk op Enter.
extensions-no-match-detail = Besykje in oare namme of utwreidings-ID.
extensions-on = Oan
extensions-off = Ut
extensions-enable-confirm = { $name } ynskeakelje?
extensions-enable-permissions = { $name } ynskeakelje en tastean:

lsp-title = Taalservers
lsp-search = Sykje taalservers, linters, formatters…
lsp-loading = Katalogus wurdt laden…
lsp-empty = Gjin oerienkommende taalservers
lsp-empty-detail = Besykje in oare taal, linter of formatter.
lsp-needs = hat { $tool } nedich
lsp-status-available = Beskikber
lsp-status-on-path = Op PATH
lsp-status-installing = Wurdt ynstallearre…
lsp-status-installed = Ynstallearre
lsp-status-outdated = Update beskikber
lsp-status-running = Rint
lsp-status-failed = Mislearre

spaces-title = Romten
spaces-new-placeholder = Namme fan nije romte
spaces-empty = Gjin romten
spaces-default-name = Romte { $number }
spaces-tabs = { $count ->
    [one] 1 ljepper
   *[other] { $count } ljeppers
}
spaces-delete = Romte wiskje

team-title = Team
team-just-you = Allinnich do yn dizze romte
team-agents = { $count ->
    [one] Do en 1 agent
   *[other] Do en { $count } agents
}
team-empty = Hjir is noch nimmen
team-you = Do
team-agent = Agent

services-title = Eftergrûntsjinsten
services-processes = { $count ->
    [one] 1 proses
   *[other] { $count } prosessen
}
services-kill-all = Alles forsearre stopje
services-not-running = Tsjinst rint net
services-start-with = Starte mei:
services-empty = Gjin aktive prosessen
services-filter = Prosessen filterje…
services-no-match = Gjin oerienkommende prosessen
services-connected = Ferbûn
services-disconnected = Net ferbûn
services-attached = keppele
services-kill = Forsearre stopje
services-memory = Unthâld
services-size = Grutte
services-shell = Shell

error-title = Flater

history-search = Skiednis sykje
history-clear-all = Alles wiskje
history-clear-confirm = Hiele skiednis wiskje?
history-clear-warning = Dit kin net ûngedien makke wurde.
history-cancel = Annulearje
history-today = Hjoed
history-yesterday = Juster
history-days-ago = { $count } dagen lyn
history-day-offset = Dei -{ $count }

settings-title = Ynstellingen
settings-loading = Ynstellingen wurde laden…
settings-stored = Opslein yn ~/.vmux/settings.ron
settings-other = Oars
settings-software-update = Software-update
settings-check-updates = Kontrolearje op updates
settings-check-updates-hint = Kontrolearret automatysk by it starten en elk oere as Auto-update ynskeakele is.
settings-update-unavailable = Net beskikber
settings-update-unavailable-hint = De updater sit net yn dizze build.
settings-update-checking = Wurdt kontrolearre…
settings-update-checking-hint = Kontrolearret op updates…
settings-update-check-again = Opnij kontrolearje
settings-update-current = Vmux is by de tiid.
settings-update-downloading = Wurdt ynladen…
settings-update-downloading-hint = Vmux { $version } wurdt ynladen…
settings-update-installing = Wurdt ynstallearre…
settings-update-installing-hint = Vmux { $version } wurdt ynstallearre…
settings-update-ready = Update klear
settings-update-ready-hint = Vmux { $version } is klear. Start opnij om ta te passen.
settings-update-try-again = Opnij besykje
settings-update-failed = Kin net op updates kontrolearje.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Druk op in toets…
settings-saved = Opslein
settings-record-key = Klik om in nije toetskombinaasje op te nimmen

tray-open-window = Finster iepenje
tray-close-window = Finster slute
tray-pause-recording = Opname pauzearje
tray-resume-recording = Opname ferfetsje
tray-finish-recording = Opname ôfmeitsje
tray-quit = Vmux ôfslute

composer-attach-files = Bestannen taheakje (/upload)
composer-remove-attachment = Taheakke bestân fuortsmite

layout-back = Tebek
layout-forward = Foarút
layout-reload = Opnij lade
layout-bookmark-page = Dizze side blêdwizerje
layout-remove-bookmark = Blêdwizer fuortsmite
layout-pin-page = Dizze side fêstsette
layout-unpin-page = Dizze side losmeitsje
layout-manage-extensions = Utwreidingen beheare
layout-new-stack = Nije steapel
layout-close-tab = Ljepper slute
layout-bookmark = Blêdwizer
layout-pin = Fêstsette
layout-new-tab = Nije ljepper
layout-team = Team

command-switch-space = Wikselje fan romte…
command-search-ask = Sykje of freegje…
command-new-tab-placeholder = Sykje of typ in URL, of kies Terminal…
command-placeholder = Typ in URL, sykje ljeppers, of > foar kommando’s…
command-composer-placeholder = Typ / foar kommando’s of @ foar media
command-send = Ferstjoere (Enter)
command-terminal = Terminal
command-open-terminal = Iepenje yn Terminal
command-stack = Steapel
command-tabs = { $count ->
    [one] 1 ljepper
   *[other] { $count } ljeppers
}
command-prompt = Prompt
command-new-tab = Nije ljepper
command-search = Sykje
command-open-value = “{ $value }” iepenje
command-search-value = “{ $value }” sykje

schema-appearance = Uterlik
schema-general = Algemien
schema-layout = Yndieling
schema-layout-detail = Finster, panielen, sydbalke en fokusring.
schema-agent = Agent
schema-agent-detail = Gedrach fan agents en tastimmingen foar ark.
schema-shortcuts = Fluchtoetsen
schema-shortcuts-detail = Allinnich lêze. Bewurkje settings.ron direkt om toetsbiningen te feroarjen.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Modus
schema-mode-detail = Kleurenskema foar websiden. Apparaat folget dyn systeem.
schema-device = Apparaat
schema-light = Ljocht
schema-dark = Donker
schema-language = Taal
schema-language-detail = Brûk systeem, en-US, ja, of in BCP 47-tag mei in oerienkommende ~/.vmux/locales/<tag>.ftl-katalogus.
schema-auto-update = Auto-update
schema-auto-update-detail = Kontrolearje op en ynstallearje updates by it starten en elk oere.
schema-startup-url = Opstart-URL
schema-startup-url-detail = Leech iepenet de prompt yn de kommandobalke.
schema-search-engine = Sykmasine
schema-search-engine-detail = Brûkt foar websykjen fanút Start en de kommandobalke.
schema-window = Finster
schema-pane = Paniel
schema-side-sheet = Sydblêd
schema-focus-ring = Fokusring
schema-run-placement = Oerskriuwen fan útfierpleatsing tastean
schema-run-placement-detail = Lit agents panielmodus, rjochting en anker foar útfiering kieze.
schema-leader = Leader
schema-leader-detail = Foarheaktoets foar chord-fluchtoetsen.
schema-chord-timeout = Chord-time-out
schema-chord-timeout-detail = Millisekonden oant in chord-foarheak ferrint.
schema-bindings = Toetsbiningen
schema-confirm-close = Sluten befêstigje
schema-confirm-close-detail = Freegje om befêstiging foar it sluten fan in terminal mei in rinnend proses.
schema-default-theme = Standerttema
schema-default-theme-detail = Namme fan it aktive tema út de temalist.

settings-empty = (leech)
settings-none = (gjin)

schema-system = Systeem
schema-editor = Bewurker
schema-recording = Opname
schema-radius = Radius
schema-padding = Opfolling
schema-gap = Tuskenskoft
schema-width = Breedte
schema-color = Kleur
schema-red = Read
schema-green = Grien
schema-blue = Blau
schema-follow-files = Bestannen folgje
schema-tidy-files = Bestannen opskjinje
schema-tidy-files-max = Drompel foar bestânsopskjinjen
schema-tidy-files-auto = Bestannen automatysk opskjinje
schema-app-providers = App-oanbieders
schema-provider = Oanbieder
schema-kind = Soarte
schema-models = Modellen
schema-acp = ACP-aginten
schema-id = ID
schema-name = Namme
schema-command = Kommando
schema-arguments = Arguminten
schema-environment = Omjouwingsfariabelen
schema-working-directory = Wurkmap
schema-shell = Shell
schema-font-family = Lettertypefamylje
schema-startup-directory = Startmap
schema-themes = Tema's
schema-color-scheme = Kleureskema
schema-font-size = Lettergrutte
schema-line-height = Rigelhichte
schema-cursor-style = Rinnerkestyl
schema-cursor-blink = Rinnerke knipperje
schema-custom-themes = Oanpaste tema's
schema-foreground = Foargrûn
schema-background = Eftergrûn
schema-cursor = Rinnerke
schema-ansi-colors = ANSI-kleuren
schema-keymap = Toetsekaart
schema-explorer = Ferkenner
schema-visible = Sichtber
schema-language-servers = Taalservers
schema-servers = Servers
schema-language-id = Taal-ID
schema-root-markers = Root-markearders
schema-output-directory = Utfiermap

menu-scene = Sêne
menu-layout = Yndieling
menu-terminal = Terminal
menu-browser = Blêder
menu-service = Tsjinst
menu-bookmark = Blêdwizer
menu-edit = Bewurkje

layout-knowledge = Kennis
layout-open-knowledge = Kennis iepenje
layout-open-welcome-knowledge = Wolkom by Kennis iepenje
layout-open-path = { $path } iepenje
layout-fold-knowledge = Kennis ynklappe
layout-unfold-knowledge = Kennis útklappe
layout-bookmarks = Blêdwizers
layout-new-folder = Nije map
layout-add-to-bookmarks = Tafoegje oan Blêdwizers
layout-move-to-bookmarks = Ferpleatse nei Blêdwizers
layout-stack-number = Steapel { $number }
layout-fold-stack = Steapel ynklappe
layout-unfold-stack = Steapel útklappe
layout-close-stack = Steapel slute
layout-bookmark-in = Blêdwizer yn { $folder }

common-cancel = Annulearje
common-delete = Wiskje
common-save = Bewarje
common-rename = Omneame
common-expand = Utklappe
common-collapse = Ynklappe
common-loading = Lade…
common-error = Flater
common-output = Utfier
common-pending = Yn ôfwachting
common-current = aktueel
common-stop = Stopje
services-command = Vmux-tsjinst
services-uptime-seconds = { $seconds }s
services-uptime-minutes = { $minutes }m { $seconds }s
services-uptime-hours = { $hours }o { $minutes }m
services-uptime-days = { $days }d { $hours }o

error-page-failed-load = Side koe net laden wurde
error-page-not-found = Side net fûn
error-unknown-host = Unbekende Vmux-apphost: { $host }

history-title = Skiednis

command-new-app-chat = Nij { $provider }/{ $model }-petear (App)
command-interactive-mode-user = Sêne > Ynteraktive modus > Brûker
command-interactive-mode-player = Sêne > Ynteraktive modus > Spiler
command-minimize-window = Yndieling > Finster > Minimalisearje
command-toggle-layout = Yndieling > Yndieling > Yndieling wikselje
command-close-tab = Yndieling > Ljepper > Ljepper slute
command-new-task = Yndieling > Ljepper > Nije taak…
command-next-tab = Yndieling > Ljepper > Folgjende ljepper
command-prev-tab = Yndieling > Ljepper > Foarige ljepper
command-rename-tab = Yndieling > Ljepper > Ljepper omneame
command-tab-select-1 = Yndieling > Ljepper > Ljepper 1 selektearje
command-tab-select-2 = Yndieling > Ljepper > Ljepper 2 selektearje
command-tab-select-3 = Yndieling > Ljepper > Ljepper 3 selektearje
command-tab-select-4 = Yndieling > Ljepper > Ljepper 4 selektearje
command-tab-select-5 = Yndieling > Ljepper > Ljepper 5 selektearje
command-tab-select-6 = Yndieling > Ljepper > Ljepper 6 selektearje
command-tab-select-7 = Yndieling > Ljepper > Ljepper 7 selektearje
command-tab-select-8 = Yndieling > Ljepper > Ljepper 8 selektearje
command-tab-select-last = Yndieling > Ljepper > Lêste ljepper selektearje
command-close-pane = Yndieling > Paniel > Paniel slute
command-select-pane-left = Yndieling > Paniel > Linkerpaniel selektearje
command-select-pane-right = Yndieling > Paniel > Rjochterpaniel selektearje
command-select-pane-up = Yndieling > Paniel > Boppeste paniel selektearje
command-select-pane-down = Yndieling > Paniel > Underste paniel selektearje
command-swap-pane-prev = Yndieling > Paniel > Paniel mei foarige wikselje
command-swap-pane-next = Yndieling > Paniel > Paniel mei folgjende wikselje
command-equalize-pane-size = Yndieling > Paniel > Panielgrutte lykmeitsje
command-resize-pane-left = Yndieling > Paniel > Paniel nei links fergrutsje
command-resize-pane-right = Yndieling > Paniel > Paniel nei rjochts fergrutsje
command-resize-pane-up = Yndieling > Paniel > Paniel omheech fergrutsje
command-resize-pane-down = Yndieling > Paniel > Paniel omleech fergrutsje
command-stack-close = Yndieling > Steapel > Steapel slute
command-stack-next = Yndieling > Steapel > Folgjende steapel
command-stack-previous = Yndieling > Steapel > Foarige steapel
command-stack-reopen = Yndieling > Steapel > Sluten side opnij iepenje
command-stack-swap-prev = Yndieling > Steapel > Steapel nei links ferpleatse
command-stack-swap-next = Yndieling > Steapel > Steapel nei rjochts ferpleatse
command-space-open = Yndieling > Romte > Romten
command-terminal-close = Terminal > Terminal slute
command-terminal-next = Terminal > Folgjende terminal
command-terminal-prev = Terminal > Foarige terminal
command-terminal-clear = Terminal > Terminal leegje
command-browser-prev-page = Blêder > Navigaasje > Tebek
command-browser-next-page = Blêder > Navigaasje > Foarút
command-browser-reload = Blêder > Navigaasje > Opnij lade
command-browser-hard-reload = Blêder > Navigaasje > Hurd opnij lade
command-open-in-place = Blêder > Iepenje > Hjir iepenje
command-open-in-new-stack = Blêder > Iepenje > Yn nije steapel iepenje
command-open-in-pane-top = Blêder > Iepenje > Yn paniel boppe iepenje
command-open-in-pane-right = Blêder > Iepenje > Yn paniel rjochts iepenje
command-open-in-pane-bottom = Blêder > Iepenje > Yn paniel ûnder iepenje
command-open-in-pane-left = Blêder > Iepenje > Yn paniel links iepenje
command-open-in-new-tab = Blêder > Iepenje > Yn nije ljepper iepenje
command-open-in-new-space = Blêder > Iepenje > Yn nije romte iepenje
command-browser-zoom-in = Blêder > Byld > Ynzoome
command-browser-zoom-out = Blêder > Byld > Utzoome
command-browser-zoom-reset = Blêder > Byld > Werklike grutte
command-browser-dev-tools = Blêder > Byld > Untwikkelersark
command-browser-open-command-bar = Blêder > Balke > Kommandobalke
command-browser-open-page-in-command-bar = Blêder > Balke > Side bewurkje
command-browser-open-path-bar = Blêder > Balke > Paadnavigator
command-browser-open-commands = Blêder > Balke > Kommando’s
command-browser-open-history = Blêder > Balke > Skiednis
command-service-open = Tsjinst > Tsjinstmonitor iepenje
command-bookmark-toggle-active = Blêdwizer > Side as blêdwizer bewarje
command-bookmark-pin-active = Blêdwizer > Side fêstsette

layout-tab = Ljepper
layout-no-stacks = Gjin steapels
layout-loading = Lade…
layout-no-markdown-files = Gjin Markdown-bestannen
layout-empty-folder = Lege map
layout-worktree = worktree
layout-folder-name = Mapnamme
layout-no-pins-bookmarks = Gjin fêstsetten siden of blêdwizers
layout-move-to = Ferpleatse nei { $folder }
layout-bookmark-current-page = Aktuele side as blêdwizer bewarje
layout-rename-folder = Map omneame
layout-remove-folder = Map fuortsmite
layout-update-downloading = Update wurdt ynladen
layout-update-installing = Update wurdt ynstallearre…
layout-update-ready = Nije ferzje beskikber
layout-restart-update = Opnij starte om by te wurkjen

agent-preparing = Agent tariede…
agent-send-all-queued = Alle wachtrige-prompts no ferstjoere (Esc)
agent-send = Ferstjoere (Enter)
agent-ready = Klear as jo safier binne.
agent-loading-older = Aldere berjochten lade…
agent-load-older = Aldere berjochten lade
agent-continued-from = Fuortset fan { $source }
agent-older-context-omitted = âldere kontekst weilitten
agent-interrupted = ûnderbrutsen
agent-allow-tool = { $tool } tastean?
agent-deny = Wegerje
agent-allow-always = Altyd tastean
agent-allow = Tastean
agent-loading-sessions = Sesjes lade…
agent-no-resumable-sessions = Gjin te ferfetsjen sesjes fûn
agent-no-matching-sessions = Gjin oerienkommende sesjes
agent-no-matching-models = Gjin oerienkommende modellen
agent-choice-help = ↑/↓ of Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Repositorymap kieze
agent-choose-repository-detail = Selektearje it lokale Git-repository dat de agent brûke moat.
agent-choosing = Kieze…
agent-choose-folder = Map kieze
agent-queued = yn wachtrige
agent-attached = Taheakke:
agent-cancel-queued = Prompt yn wachtrige annulearje
agent-resume-queued = Prompts yn wachtrige ferfetsje
agent-clear-queue = Wachtrige leegje
agent-send-all-now = alles no ferstjoere
agent-choose-option = Kies hjirboppe in opsje
agent-loading-media = Media lade…
agent-no-matching-media = Gjin oerienkommende media
agent-prompt-context = Promptkontekst
agent-details = Details
agent-path = Paad
agent-tool = Ark
agent-server = Server
agent-bytes = { $count } bytes
agent-worked-for = Wurke foar { $duration }
agent-worked-for-steps = { $count ->
    [one] Wurke foar { $duration } · 1 stap
   *[other] Wurke foar { $duration } · { $count } stappen
}
agent-tool-guardian-review = Guardian-beoardieling
agent-tool-read-files = Bestannen lêzen
agent-tool-viewed-image = Ofbylding besjoen
agent-tool-used-browser = Blêder brûkt
agent-tool-searched-files = Bestannen trochsocht
agent-tool-ran-commands = Kommando’s útfierd
agent-thinking = Tinkt nei
agent-subagent = Subagent
agent-prompt = Prompt
agent-thread = Tried
agent-parent = Boppelizzend
agent-children = Underlizzenden
agent-call = Oprop
agent-raw-event = Rûch barren
agent-plan = Plan
agent-tasks = { $count ->
    [one] 1 taak
   *[other] { $count } taken
}
agent-edited = Bewurke
agent-reconnecting = Opnij ferbine { $attempt }/{ $total }
agent-status-running = Draait
agent-status-done = Klear
agent-status-failed = Mislearre
agent-status-pending = Yn ôfwachting
agent-slash-attach-files = Bestannen taheakje
agent-slash-resume-session = Eardere sesje ferfetsje
agent-slash-select-model = Model selektearje
agent-slash-continue-cli = Dizze sesje trochsette yn de CLI
agent-session-just-now = krekt no
agent-session-minutes-ago = { $count }m lyn
agent-session-hours-ago = { $count }o lyn
agent-session-days-ago = { $count }d lyn
agent-working-working = Wurket
agent-working-thinking = Tinkt nei
agent-working-pondering = Prakkesearret
agent-working-noodling = Prutselet
agent-working-percolating = Siedet troch
agent-working-conjuring = Toveret
agent-working-cooking = Kookt
agent-working-brewing = Briedt
agent-working-musing = Mimeret
agent-working-ruminating = Oertinkt
agent-working-scheming = Smedet plannen
agent-working-synthesizing = Syntetisearret
agent-working-tinkering = Túnket
agent-working-churning = Draait troch
agent-working-vibing = Vibet
agent-working-simmering = Simeret
agent-working-crafting = Makket
agent-working-divining = Peilet
agent-working-mulling = Oerweaget
agent-working-spelunking = Djipdûkt

editor-toggle-explorer = Explorer wikselje (Cmd+B)
editor-unsaved = net bewarre
editor-rendered-markdown = Werjûne Markdown mei live bewurkjen
editor-note = Notysje
editor-source-editor = Boarnebewurker
editor-editor = Bewurker
editor-git-diff = Git-diff
editor-diff = Diff
editor-tidy = Opromje
editor-always = Altyd
editor-unchanged-previews = { $count ->
    [one] ✦ 1 net-wizige foarbyld
   *[other] ✦ { $count } net-wizige foarbylden
}
editor-open-externally = Ekstern iepenje
editor-changed-line = Wizige rigel
editor-go-to-definition = Gean nei definysje
editor-find-references = Referinsjes sykje
editor-references = { $count ->
    [one] 1 referinsje
   *[other] { $count } referinsjes
}
editor-lsp-starting = { $server } start…
editor-lsp-not-installed = { $server } — net ynstallearre
editor-explorer = Explorer
editor-open-editors = Iepen bewurkers
editor-outline = Oersjoch
editor-new-file = Nij bestân
editor-new-folder = Nije map
editor-delete-confirm = “{ $name }” wiskje? Dit kin net ûngedien makke wurde.
editor-created-folder = Map { $name } oanmakke
editor-created-file = Bestân { $name } oanmakke
editor-renamed-to = Omneamd nei { $name }
editor-deleted = { $name } wiske
editor-failed-decode-image = Ofbylding koe net dekodearre wurde
editor-preview-large-image = ôfbylding (te grut foar foarbyld)
editor-preview-binary = binêr
editor-preview-file = bestân

git-status-clean = skjin
git-status-modified = wizige
git-status-staged = staged
git-status-staged-modified = staged*
git-status-untracked = net folge
git-status-deleted = wiske
git-status-conflict = konflikt
git-accept-all = ✓ alles akseptearje
git-unstage = Unstage
git-confirm-deny-all = Alles wegerjen befêstigje
git-deny-all = ✗ alles wegerje
git-commit-message = commitberjocht
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Diff lade…
git-no-changes = Gjin wizigingen om te toanen
git-accept = ✓ akseptearje
git-deny = ✗ wegerje
git-show-unchanged-lines = { $count } net-wizige rigels toane

terminal-loading = Lade…
terminal-runs-when-ready = draait as klear · Ctrl+C leget · Esc slacht oer
terminal-booting = opstarte
terminal-type-command = typ in kommando · draait as klear · Esc slacht oer

setup-tagline-claude = De coding-agent fan Anthropic, yn Vmux
setup-tagline-codex = De coding-agent fan OpenAI, yn Vmux
setup-tagline-vibe = De coding-agent fan Mistral, yn Vmux
setup-install-title = { $name } CLI ynstallearje
setup-homebrew-required = Homebrew is nedich om { $command } te ynstallearjen en is noch net ynsteld. Vmux ynstallearret earst Homebrew, en dêrnei { $name }.
setup-terminal-instructions = Druk yn de terminal op Return om te starten, en fier dan jo Mac-wachtwurd yn as dêr om frege wurdt.
setup-command-missing = Vmux hat dizze side iepene omdat it lokale kommando { $command } noch net ynstallearre is. Fier it kommando hjirûnder út om it op te heljen.
setup-install-failed = Ynstallaasje is net foltôge. Kontrolearje de terminal foar details en besykje it opnij.
setup-installing = Ynstallearje…
setup-install-homebrew = Homebrew + { $name } ynstallearje
setup-run-install = Ynstallaasjekommando útfiere
setup-auto-reload = Vmux draait it yn in terminal en laadt opnij as { $command } klear is.

debug-title = Debug
debug-auto-update = Automatysk bywurkje
debug-simulate-update = Beskikbere update simulearje
debug-simulate-download = Download simulearje
debug-clear-update = Update wiskje
debug-trigger-restart = Opnij starten aktivearje

command-manage-spaces = Romten beheare…
command-pane-stack-location = finsterdiel { $pane } / steapel { $stack }
command-space-pane-stack-location = { $space } / finsterdiel { $pane } / steapel { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Ynteraktive modus
command-group-window = Finster
command-group-tab = Ljepper
command-group-pane = Finsterdiel
command-group-stack = Steapel
command-group-space = Romte
command-group-navigation = Navigaasje
command-group-open = Iepenje
command-group-view = Werjefte
command-group-bar = Balke

menu-close-vmux = Vmux slute

agents-terminal-coding-agent = Terminal-basearre kodearagent
