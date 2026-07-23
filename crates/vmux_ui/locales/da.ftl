locale-name = dansk
common-open = Åbn
common-close = Luk
common-install = Installer
common-uninstall = Afinstaller
common-update = Opdater
common-retry = Prøv igen
common-refresh = Opdater
common-remove = Fjern
common-enable = Slå til
common-disable = Slå fra
common-new = Ny
common-active = aktiv
common-running = kører
common-done = færdig
common-failed = Mislykkedes
common-installed = Installeret
common-items = { $count ->
    [one] { $count } element
   *[other] { $count } elementer
}

tools-title = Værktøjer
tools-search = Søg efter pakker, agenter, MCP, sprogværktøjer og konfigurationsfiler…
tools-open = Åbn Værktøjer
tools-fold = Fold værktøjer sammen
tools-unfold = Fold værktøjer ud
tools-scanning = Scanner lokale værktøjer…
tools-no-installed = Ingen installerede værktøjer
tools-empty = Ingen matchende værktøjer
tools-empty-detail = Installer en pakke, eller tilføj en konfigurationsfilpakke i Stow-stil.
tools-apply = Anvend
tools-homebrew = Homebrew
tools-homebrew-sync = Installerede formler og programmer synkroniseres automatisk.
tools-open-brewfile = Åbn Brewfile
tools-managed = administreret
tools-provider-homebrew-formulae = Homebrew-formler
tools-provider-homebrew-casks = Homebrew-programmer
tools-provider-npm = npm-pakker
tools-provider-acp-agents = ACP-agenter
tools-provider-language-tools = Sprogværktøjer
tools-provider-mcp-servers = MCP-servere
tools-provider-dotfiles = Konfigurationsfiler
tools-status-available = Tilgængelig
tools-status-missing = Mangler
tools-status-conflict = Konflikt
tools-forget = Glem
tools-manage = Administrer
tools-link = Tilknyt
tools-unlink = Fjern tilknytning
tools-import = Importer
tools-update-count = { $count ->
    [one] 1 opdatering
   *[other] { $count } opdateringer
}
tools-conflict-count = { $count ->
    [one] 1 konflikt
   *[other] { $count } konflikter
}
tools-result-applied = Værktøjer anvendt
tools-result-imported = Værktøjer importeret
tools-result-installed = { $name } installeret
tools-result-updated = { $name } opdateret
tools-result-uninstalled = { $name } afinstalleret
tools-result-forgotten = { $name } glemt
tools-result-managed = { $name } administreres nu
tools-result-linked = { $name } tilknyttet
tools-result-unlinked = Tilknytningen til { $name } er fjernet

start-title = Start
start-tagline = Ét prompt. Alt bliver klaret.

agents-title = Agenter
agents-search = Søg efter ACP- og CLI-agenter…
agents-empty = Ingen matchende agenter
agents-empty-detail = Prøv et navn, runtime eller ACP/CLI.
agents-install-failed = Installationen mislykkedes
agents-updating = Opdaterer…
agents-retrying = Prøver igen…
agents-preparing = Forbereder…

extensions-title = Udvidelser
extensions-search = Søg i installerede eller Chrome Web Store…
extensions-relaunch = Genstart for at anvende
extensions-empty = Ingen udvidelser installeret
extensions-no-match = Ingen matchende udvidelser
extensions-empty-detail = Søg i Chrome Web Store ovenfor, og tryk på Retur.
extensions-no-match-detail = Prøv et andet navn eller udvidelses-id.
extensions-on = Til
extensions-off = Fra
extensions-enable-confirm = Slå { $name } til?
extensions-enable-permissions = Slå { $name } til, og tillad:

lsp-title = Language Servers
lsp-search = Søg efter language servers, linters og formateringsværktøjer…
lsp-loading = Indlæser katalog…
lsp-empty = Ingen matchende language servers
lsp-empty-detail = Prøv et andet sprog, linter eller formateringsværktøj.
lsp-needs = kræver { $tool }
lsp-status-available = Tilgængelig
lsp-status-on-path = På PATH
lsp-status-installing = Installerer…
lsp-status-installed = Installeret
lsp-status-outdated = Opdatering tilgængelig
lsp-status-running = Kører
lsp-status-failed = Mislykkedes

spaces-title = Arbejdsrum
spaces-new-placeholder = Navn på nyt arbejdsrum
spaces-empty = Ingen arbejdsrum
spaces-default-name = Arbejdsrum { $number }
spaces-tabs = { $count ->
    [one] 1 fane
   *[other] { $count } faner
}
spaces-delete = Slet arbejdsrum

team-title = Team
team-just-you = Kun dig i dette arbejdsrum
team-agents = { $count ->
    [one] Dig og 1 agent
   *[other] Dig og { $count } agenter
}
team-empty = Ingen her endnu
team-you = Dig
team-agent = Agent

services-title = Baggrundstjenester
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } processer
}
services-kill-all = Afslut alle
services-not-running = Tjenesten kører ikke
services-start-with = Start med:
services-empty = Ingen aktive processer
services-filter = Filtrer processer…
services-no-match = Ingen matchende processer
services-connected = Forbundet
services-disconnected = Afbrudt
services-attached = tilknyttet
services-kill = Afslut
services-memory = Hukommelse
services-size = Størrelse
services-shell = Shell

error-title = Fejl

history-search = Søg i historik
history-clear-all = Ryd alt
history-clear-confirm = Ryd hele historikken?
history-clear-warning = Dette kan ikke fortrydes.
history-cancel = Annuller
history-today = I dag
history-yesterday = I går
history-days-ago = for { $count } dage siden
history-day-offset = Dag -{ $count }

settings-title = Indstillinger
settings-loading = Indlæser indstillinger…
settings-stored = Gemt i ~/.vmux/settings.ron
settings-other = Andet
settings-software-update = Softwareopdatering
settings-check-updates = Søg efter opdateringer
settings-check-updates-hint = Tjekker automatisk ved start og hver time, når Auto-update er slået til.
settings-update-unavailable = Ikke tilgængelig
settings-update-unavailable-hint = Opdateringsprogrammet er ikke inkluderet i denne build.
settings-update-checking = Tjekker…
settings-update-checking-hint = Tjekker for opdateringer…
settings-update-check-again = Tjek igen
settings-update-current = Vmux er opdateret.
settings-update-downloading = Downloader…
settings-update-downloading-hint = Downloader Vmux { $version }…
settings-update-installing = Installerer…
settings-update-installing-hint = Installerer Vmux { $version }…
settings-update-ready = Opdatering klar
settings-update-ready-hint = Vmux { $version } er klar. Genstart for at anvende den.
settings-update-try-again = Prøv igen
settings-update-failed = Kunne ikke søge efter opdateringer.
settings-item = Element
settings-item-number = Element { $number }
settings-press-key = Tryk på en tast…
settings-saved = Gemt
settings-record-key = Klik for at optage en ny tastekombination

tray-open-window = Åbn vindue
tray-close-window = Luk vindue
tray-pause-recording = Sæt optagelse på pause
tray-resume-recording = Genoptag optagelse
tray-finish-recording = Afslut optagelse
tray-quit = Afslut Vmux

composer-attach-files = Vedhæft filer (/upload)
composer-remove-attachment = Fjern vedhæftning

layout-back = Tilbage
layout-forward = Frem
layout-reload = Genindlæs
layout-bookmark-page = Føj siden til bogmærker
layout-remove-bookmark = Fjern bogmærke
layout-pin-page = Fastgør siden
layout-unpin-page = Frigør siden
layout-manage-extensions = Administrer udvidelser
layout-new-stack = Ny stak
layout-close-tab = Luk fane
layout-bookmark = Bogmærke
layout-pin = Fastgør
layout-new-tab = Ny fane
layout-team = Team

command-switch-space = Skift arbejdsrum…
command-search-ask = Søg eller spørg…
command-new-tab-placeholder = Søg eller indtast en URL, eller vælg Terminal…
command-placeholder = Indtast en URL, søg i faner, eller brug > til kommandoer…
command-composer-placeholder = Skriv / for kommandoer eller @ for medier
command-send = Send (Enter)
command-terminal = Terminal
command-open-terminal = Åbn i Terminal
command-stack = Stak
command-tabs = { $count ->
    [one] 1 fane
   *[other] { $count } faner
}
command-prompt = Prompt
command-new-tab = Ny fane
command-search = Søg
command-open-value = Åbn “{ $value }”
command-search-value = Søg efter “{ $value }”

schema-appearance = Udseende
schema-general = Generelt
schema-layout = Layout
schema-layout-detail = Vindue, ruder, sidepanel og fokusring.
schema-agent = Agent
schema-agent-detail = Agentadfærd og tilladelser til værktøjer.
schema-shortcuts = Genveje
schema-shortcuts-detail = Skrivebeskyttet visning. Rediger settings.ron direkte for at ændre tastebindinger.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Tilstand
schema-mode-detail = Farveskema for websider. Enhed følger dit system.
schema-device = Enhed
schema-light = Lys
schema-dark = Mørk
schema-language = Sprog
schema-language-detail = Brug systemet, en-US, ja eller et hvilket som helst BCP 47-tag med et matchende ~/.vmux/locales/<tag>.ftl-katalog.
schema-auto-update = Auto-update
schema-auto-update-detail = Søg efter og installer opdateringer ved start og hver time.
schema-startup-url = Start-URL
schema-startup-url-detail = Tom åbner prompten i kommandolinjen.
schema-search-engine = Søgemaskine
schema-search-engine-detail = Bruges til websøgninger fra Start og kommandolinjen.
schema-window = Vindue
schema-pane = Rude
schema-side-sheet = Sideark
schema-focus-ring = Fokusring
schema-run-placement = Tillad tilsidesættelse af kørselsplacering
schema-run-placement-detail = Lad agenter vælge rudetilstand, retning og anker for kørsel.
schema-leader = Leader
schema-leader-detail = Præfikstast til akkordgenveje.
schema-chord-timeout = Akkordtimeout
schema-chord-timeout-detail = Millisekunder før et akkordpræfiks udløber.
schema-bindings = Tastebindinger
schema-confirm-close = Bekræft lukning
schema-confirm-close-detail = Spørg, før en terminal med en kørende proces lukkes.
schema-default-theme = Standardtema
schema-default-theme-detail = Navnet på det aktive tema fra temalisten.

settings-empty = (tom)
settings-none = (ingen)

schema-system = System
schema-editor = Editor
schema-recording = Optagelse
schema-radius = Radius
schema-padding = Indvendig afstand
schema-gap = Mellemrum
schema-width = Bredde
schema-color = Farve
schema-red = Rød
schema-green = Grøn
schema-blue = Blå
schema-follow-files = Følg filer
schema-tidy-files = Ryd op i filer
schema-tidy-files-max = Grænse for filoprydning
schema-tidy-files-auto = Ryd automatisk op i filer
schema-app-providers = Appudbydere
schema-provider = Udbyder
schema-kind = Type
schema-models = Modeller
schema-acp = ACP-agenter
schema-id = ID
schema-name = Navn
schema-command = Kommando
schema-arguments = Argumenter
schema-environment = Miljøvariabler
schema-working-directory = Arbejdsmappe
schema-shell = Skal
schema-font-family = Skrifttypefamilie
schema-startup-directory = Startmappe
schema-themes = Temaer
schema-color-scheme = Farveskema
schema-font-size = Skriftstørrelse
schema-line-height = Linjehøjde
schema-cursor-style = Markørstil
schema-cursor-blink = Blinkende markør
schema-custom-themes = Brugerdefinerede temaer
schema-foreground = Forgrund
schema-background = Baggrund
schema-cursor = Markør
schema-ansi-colors = ANSI-farver
schema-keymap = Tastaturgenveje
schema-explorer = Stifinder
schema-visible = Synlig
schema-language-servers = Sprogservere
schema-servers = Servere
schema-language-id = Sprog-ID
schema-root-markers = Rodmarkører
schema-output-directory = Outputmappe

menu-scene = Scene
menu-layout = Layout
menu-terminal = Terminal
menu-browser = Browser
menu-service = Tjeneste
menu-bookmark = Bogmærke
menu-edit = Rediger

layout-knowledge = Viden
layout-open-knowledge = Åbn Viden
layout-open-welcome-knowledge = Åbn Velkommen til Viden
layout-open-path = Åbn { $path }
layout-fold-knowledge = Fold viden sammen
layout-unfold-knowledge = Fold viden ud
layout-bookmarks = Bogmærker
layout-new-folder = Ny mappe
layout-add-to-bookmarks = Føj til bogmærker
layout-move-to-bookmarks = Flyt til bogmærker
layout-stack-number = Stak { $number }
layout-fold-stack = Fold stak sammen
layout-unfold-stack = Fold stak ud
layout-close-stack = Luk stak
layout-bookmark-in = Bogmærk i { $folder }

common-cancel = Annuller
common-delete = Slet
common-save = Gem
common-rename = Omdøb
common-expand = Udvid
common-collapse = Skjul
common-loading = Indlæser…
common-error = Fejl
common-output = Output
common-pending = Afventer
common-current = aktuel
common-stop = Stop
services-command = Vmux-tjeneste
services-uptime-seconds = { $seconds } s
services-uptime-minutes = { $minutes } min. { $seconds } s
services-uptime-hours = { $hours } t { $minutes } min.
services-uptime-days = { $days } d { $hours } t

error-page-failed-load = Siden kunne ikke indlæses
error-page-not-found = Siden blev ikke fundet
error-unknown-host = Ukendt Vmux-appvært: { $host }

history-title = Historik

command-new-app-chat = Ny { $provider }/{ $model }-chat (app)
command-interactive-mode-user = Scene > Interaktiv tilstand > Bruger
command-interactive-mode-player = Scene > Interaktiv tilstand > Afspiller
command-minimize-window = Layout > Vindue > Minimer
command-toggle-layout = Layout > Layout > Skift layout
command-close-tab = Layout > Fane > Luk fane
command-new-task = Layout > Fane > Ny opgave…
command-next-tab = Layout > Fane > Næste fane
command-prev-tab = Layout > Fane > Forrige fane
command-rename-tab = Layout > Fane > Omdøb fane
command-tab-select-1 = Layout > Fane > Vælg fane 1
command-tab-select-2 = Layout > Fane > Vælg fane 2
command-tab-select-3 = Layout > Fane > Vælg fane 3
command-tab-select-4 = Layout > Fane > Vælg fane 4
command-tab-select-5 = Layout > Fane > Vælg fane 5
command-tab-select-6 = Layout > Fane > Vælg fane 6
command-tab-select-7 = Layout > Fane > Vælg fane 7
command-tab-select-8 = Layout > Fane > Vælg fane 8
command-tab-select-last = Layout > Fane > Vælg sidste fane
command-close-pane = Layout > Rude > Luk rude
command-select-pane-left = Layout > Rude > Vælg rude til venstre
command-select-pane-right = Layout > Rude > Vælg rude til højre
command-select-pane-up = Layout > Rude > Vælg rude ovenfor
command-select-pane-down = Layout > Rude > Vælg rude nedenfor
command-swap-pane-prev = Layout > Rude > Byt med forrige rude
command-swap-pane-next = Layout > Rude > Byt med næste rude
command-equalize-pane-size = Layout > Rude > Gør rudestørrelser ens
command-resize-pane-left = Layout > Rude > Tilpas rude mod venstre
command-resize-pane-right = Layout > Rude > Tilpas rude mod højre
command-resize-pane-up = Layout > Rude > Tilpas rude opad
command-resize-pane-down = Layout > Rude > Tilpas rude nedad
command-stack-close = Layout > Stak > Luk stak
command-stack-next = Layout > Stak > Næste stak
command-stack-previous = Layout > Stak > Forrige stak
command-stack-reopen = Layout > Stak > Genåbn lukket side
command-stack-swap-prev = Layout > Stak > Flyt stak til venstre
command-stack-swap-next = Layout > Stak > Flyt stak til højre
command-space-open = Layout > Space > Spaces
command-terminal-close = Terminal > Luk terminal
command-terminal-next = Terminal > Næste terminal
command-terminal-prev = Terminal > Forrige terminal
command-terminal-clear = Terminal > Ryd terminal
command-browser-prev-page = Browser > Navigation > Tilbage
command-browser-next-page = Browser > Navigation > Frem
command-browser-reload = Browser > Navigation > Genindlæs
command-browser-hard-reload = Browser > Navigation > Hård genindlæsning
command-open-in-place = Browser > Åbn > Åbn her
command-open-in-new-stack = Browser > Åbn > Åbn i ny stak
command-open-in-pane-top = Browser > Åbn > Åbn i rude ovenfor
command-open-in-pane-right = Browser > Åbn > Åbn i rude til højre
command-open-in-pane-bottom = Browser > Åbn > Åbn i rude nedenfor
command-open-in-pane-left = Browser > Åbn > Åbn i rude til venstre
command-open-in-new-tab = Browser > Åbn > Åbn i ny fane
command-open-in-new-space = Browser > Åbn > Åbn i nyt Space
command-browser-zoom-in = Browser > Vis > Zoom ind
command-browser-zoom-out = Browser > Vis > Zoom ud
command-browser-zoom-reset = Browser > Vis > Faktisk størrelse
command-browser-dev-tools = Browser > Vis > Udviklerværktøjer
command-browser-open-command-bar = Browser > Linje > Kommandolinje
command-browser-open-page-in-command-bar = Browser > Linje > Rediger side
command-browser-open-path-bar = Browser > Linje > Stifinder
command-browser-open-commands = Browser > Linje > Kommandoer
command-browser-open-history = Browser > Linje > Historik
command-service-open = Service > Åbn tjenesteovervågning
command-bookmark-toggle-active = Bookmark > Bogmærk side
command-bookmark-pin-active = Bookmark > Fastgør side

layout-tab = Fane
layout-no-stacks = Ingen stakke
layout-loading = Indlæser…
layout-no-markdown-files = Ingen Markdown-filer
layout-empty-folder = Tom mappe
layout-worktree = worktree
layout-folder-name = Mappenavn
layout-no-pins-bookmarks = Ingen fastgjorte sider eller bogmærker
layout-move-to = Flyt til { $folder }
layout-bookmark-current-page = Bogmærk aktuel side
layout-rename-folder = Omdøb mappe
layout-remove-folder = Fjern mappe
layout-update-downloading = Henter opdatering
layout-update-installing = Installerer opdatering…
layout-update-ready = Ny version tilgængelig
layout-restart-update = Genstart for at opdatere

agent-preparing = Forbereder agent…
agent-send-all-queued = Send alle prompts i kø nu (Esc)
agent-send = Send (Enter)
agent-ready = Klar, når du er.
agent-loading-older = Indlæser ældre beskeder…
agent-load-older = Indlæs ældre beskeder
agent-continued-from = Fortsat fra { $source }
agent-older-context-omitted = ældre kontekst udeladt
agent-interrupted = afbrudt
agent-allow-tool = Tillad { $tool }?
agent-deny = Afvis
agent-allow-always = Tillad altid
agent-allow = Tillad
agent-loading-sessions = Indlæser sessioner…
agent-no-resumable-sessions = Ingen sessioner, der kan genoptages
agent-no-matching-sessions = Ingen matchende sessioner
agent-no-matching-models = Ingen matchende modeller
agent-choice-help = ↑/↓ eller Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Vælg repository-mappe
agent-choose-repository-detail = Vælg det lokale Git-repository, agenten skal bruge.
agent-choosing = Vælger…
agent-choose-folder = Vælg mappe
agent-queued = i kø
agent-attached = Vedhæftet:
agent-cancel-queued = Annuller prompt i kø
agent-resume-queued = Genoptag prompts i kø
agent-clear-queue = Ryd kø
agent-send-all-now = send alle nu
agent-choose-option = Vælg en mulighed ovenfor
agent-loading-media = Indlæser medier…
agent-no-matching-media = Ingen matchende medier
agent-prompt-context = Promptkontekst
agent-details = Detaljer
agent-path = Sti
agent-tool = Værktøj
agent-server = Server
agent-bytes = { $count } bytes
agent-worked-for = Arbejdede i { $duration }
agent-worked-for-steps = { $count ->
    [one] Arbejdede i { $duration } · 1 trin
   *[other] Arbejdede i { $duration } · { $count } trin
}
agent-tool-guardian-review = Guardian-gennemgang
agent-tool-read-files = Læste filer
agent-tool-viewed-image = Viste billede
agent-tool-used-browser = Brugte browser
agent-tool-searched-files = Søgte i filer
agent-tool-ran-commands = Kørte kommandoer
agent-thinking = Tænker
agent-subagent = Underagent
agent-prompt = Prompt
agent-thread = Tråd
agent-parent = Overordnet
agent-children = Underordnede
agent-call = Kald
agent-raw-event = Rå hændelse
agent-plan = Plan
agent-tasks = { $count ->
    [one] 1 opgave
   *[other] { $count } opgaver
}
agent-edited = Redigeret
agent-reconnecting = Genopretter forbindelse { $attempt }/{ $total }
agent-status-running = Kører
agent-status-done = Færdig
agent-status-failed = Mislykkedes
agent-status-pending = Afventer
agent-slash-attach-files = Vedhæft filer
agent-slash-resume-session = Genoptag en tidligere session
agent-slash-select-model = Vælg model
agent-slash-continue-cli = Fortsæt denne session i CLI
agent-session-just-now = lige nu
agent-session-minutes-ago = for { $count } min. siden
agent-session-hours-ago = for { $count } t siden
agent-session-days-ago = for { $count } d siden
agent-working-working = Arbejder
agent-working-thinking = Tænker
agent-working-pondering = Overvejer
agent-working-noodling = Nørkler
agent-working-percolating = Modner
agent-working-conjuring = Fremtryller
agent-working-cooking = Kokkererer
agent-working-brewing = Brygger
agent-working-musing = Fundrer
agent-working-ruminating = Grubler
agent-working-scheming = Lægger planer
agent-working-synthesizing = Syntetiserer
agent-working-tinkering = Piller ved det
agent-working-churning = Arbejder på sagen
agent-working-vibing = Finder rytmen
agent-working-simmering = Simrer
agent-working-crafting = Udformer
agent-working-divining = Udforsker
agent-working-mulling = Tygger på det
agent-working-spelunking = Dykker ned

editor-toggle-explorer = Slå Explorer til/fra (Cmd+B)
editor-unsaved = ikke gemt
editor-rendered-markdown = Renderet Markdown med live-redigering
editor-note = Note
editor-source-editor = Kildeeditor
editor-editor = Editor
editor-git-diff = Git-diff
editor-diff = Diff
editor-tidy = Ryd op
editor-always = Altid
editor-unchanged-previews = { $count ->
    [one] ✦ 1 uændret forhåndsvisning
   *[other] ✦ { $count } uændrede forhåndsvisninger
}
editor-open-externally = Åbn eksternt
editor-changed-line = Ændret linje
editor-go-to-definition = Gå til definition
editor-find-references = Find referencer
editor-references = { $count ->
    [one] 1 reference
   *[other] { $count } referencer
}
editor-lsp-starting = { $server } starter…
editor-lsp-not-installed = { $server } — ikke installeret
editor-explorer = Explorer
editor-open-editors = Åbne editorer
editor-outline = Oversigt
editor-new-file = Ny fil
editor-new-folder = Ny mappe
editor-delete-confirm = Slet “{ $name }”? Dette kan ikke fortrydes.
editor-created-folder = Oprettede mappen { $name }
editor-created-file = Oprettede filen { $name }
editor-renamed-to = Omdøbt til { $name }
editor-deleted = Slettede { $name }
editor-failed-decode-image = Kunne ikke afkode billede
editor-preview-large-image = billede (for stort til forhåndsvisning)
editor-preview-binary = binær
editor-preview-file = fil

git-status-clean = ren
git-status-modified = ændret
git-status-staged = staget
git-status-staged-modified = staget*
git-status-untracked = ikke sporet
git-status-deleted = slettet
git-status-conflict = konflikt
git-accept-all = ✓ acceptér alle
git-unstage = Fjern fra staging
git-confirm-deny-all = Bekræft afvis alle
git-deny-all = ✗ afvis alle
git-commit-message = commit-besked
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Indlæser diff…
git-no-changes = Ingen ændringer at vise
git-accept = ✓ acceptér
git-deny = ✗ afvis
git-show-unchanged-lines = Vis { $count } uændrede linjer

terminal-loading = Indlæser…
terminal-runs-when-ready = kører, når den er klar · Ctrl+C rydder · Esc springer over
terminal-booting = starter
terminal-type-command = skriv en kommando · kører, når den er klar · Esc springer over

setup-tagline-claude = Anthropics kodeagent, i Vmux
setup-tagline-codex = OpenAIs kodeagent, i Vmux
setup-tagline-vibe = Mistrals kodeagent, i Vmux
setup-install-title = Installer { $name } CLI
setup-homebrew-required = Homebrew kræves for at installere { $command } og er ikke sat op endnu. Vmux installerer først Homebrew og derefter { $name }.
setup-terminal-instructions = Tryk på Retur i terminalen for at starte, og indtast derefter din Mac-adgangskode, når du bliver bedt om det.
setup-command-missing = Vmux åbnede denne side, fordi den lokale { $command }-kommando ikke er installeret endnu. Kør kommandoen nedenfor for at hente den.
setup-install-failed = Installationen blev ikke gennemført. Se terminalen for detaljer, og prøv igen.
setup-installing = Installerer…
setup-install-homebrew = Installer Homebrew + { $name }
setup-run-install = Kør installationskommando
setup-auto-reload = Vmux kører den i en terminal og genindlæser, når { $command } er klar.

debug-title = Fejlfinding
debug-auto-update = Automatisk opdatering
debug-simulate-update = Simuler tilgængelig opdatering
debug-simulate-download = Simuler download
debug-clear-update = Ryd opdatering
debug-trigger-restart = Udløs genstart

command-manage-spaces = Administrer spaces…
command-pane-stack-location = rude { $pane } / stak { $stack }
command-space-pane-stack-location = { $space } / rude { $pane } / stak { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Interaktiv tilstand
command-group-window = Vindue
command-group-tab = Fane
command-group-pane = Rude
command-group-stack = Stak
command-group-space = Space
command-group-navigation = Navigation
command-group-open = Åbn
command-group-view = Vis
command-group-bar = Linje

menu-close-vmux = Luk Vmux

agents-terminal-coding-agent = Terminalbaseret kodningsagent
