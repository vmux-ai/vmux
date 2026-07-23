locale-name = norsk bokmål
common-open = Åpne
common-close = Lukk
common-install = Installer
common-uninstall = Avinstaller
common-update = Oppdater
common-retry = Prøv igjen
common-refresh = Oppdater
common-remove = Fjern
common-enable = Aktiver
common-disable = Deaktiver
common-new = Ny
common-active = aktiv
common-running = kjører
common-done = ferdig
common-failed = Mislyktes
common-installed = Installert
common-items = { $count ->
    [one] { $count } element
   *[other] { $count } elementer
}

tools-title = Verktøy
tools-search = Søk etter pakker, agenter, MCP, språkverktøy og konfigurasjonsfiler…
tools-open = Åpne verktøy
tools-fold = Fold sammen verktøy
tools-unfold = Fold ut verktøy
tools-scanning = Skanner lokale verktøy…
tools-no-installed = Ingen installerte verktøy
tools-empty = Ingen samsvarende verktøy
tools-empty-detail = Installer en pakke, eller legg til en konfigurasjonsfilpakke i Stow-stil.
tools-apply = Bruk
tools-homebrew = Homebrew
tools-homebrew-sync = Installerte formler og programmer synkroniseres automatisk.
tools-open-brewfile = Åpne Brewfile
tools-managed = administrert
tools-provider-homebrew-formulae = Homebrew-formler
tools-provider-homebrew-casks = Homebrew-programmer
tools-provider-npm = npm-pakker
tools-provider-acp-agents = ACP-agenter
tools-provider-language-tools = Språkverktøy
tools-provider-mcp-servers = MCP-servere
tools-provider-dotfiles = Konfigurasjonsfiler
tools-status-available = Tilgjengelig
tools-status-missing = Mangler
tools-status-conflict = Konflikt
tools-forget = Glem
tools-manage = Administrer
tools-link = Koble
tools-unlink = Koble fra
tools-import = Importer
tools-update-count = { $count ->
    [one] 1 oppdatering
   *[other] { $count } oppdateringer
}
tools-conflict-count = { $count ->
    [one] 1 konflikt
   *[other] { $count } konflikter
}
tools-result-applied = Verktøy brukt
tools-result-imported = Verktøy importert
tools-result-installed = { $name } installert
tools-result-updated = { $name } oppdatert
tools-result-uninstalled = { $name } avinstallert
tools-result-forgotten = { $name } glemt
tools-result-managed = { $name } administreres nå
tools-result-linked = { $name } koblet
tools-result-unlinked = { $name } koblet fra

start-title = Start
start-tagline = Én prompt. Alt gjort.

agents-title = Agenter
agents-search = Søk i ACP- og CLI-agenter …
agents-empty = Ingen agenter funnet
agents-empty-detail = Prøv et navn, kjøremiljø eller ACP/CLI.
agents-install-failed = Installasjonen mislyktes
agents-updating = Oppdaterer …
agents-retrying = Prøver igjen …
agents-preparing = Klargjør …

extensions-title = Utvidelser
extensions-search = Søk i installerte eller Chrome Web Store …
extensions-relaunch = Start på nytt for å ta i bruk
extensions-empty = Ingen utvidelser installert
extensions-no-match = Ingen utvidelser funnet
extensions-empty-detail = Søk i Chrome Web Store over og trykk Retur.
extensions-no-match-detail = Prøv et annet navn eller en annen utvidelses-ID.
extensions-on = På
extensions-off = Av
extensions-enable-confirm = Aktiver { $name }?
extensions-enable-permissions = Aktiver { $name } og tillat:

lsp-title = Språkservere
lsp-search = Søk etter språkservere, lintere, formaterere …
lsp-loading = Laster katalog …
lsp-empty = Ingen språkservere funnet
lsp-empty-detail = Prøv et annet språk, en linter eller en formaterer.
lsp-needs = krever { $tool }
lsp-status-available = Tilgjengelig
lsp-status-on-path = På PATH
lsp-status-installing = Installerer …
lsp-status-installed = Installert
lsp-status-outdated = Oppdatering tilgjengelig
lsp-status-running = Kjører
lsp-status-failed = Mislyktes

spaces-title = Områder
spaces-new-placeholder = Navn på nytt område
spaces-empty = Ingen områder
spaces-default-name = Område { $number }
spaces-tabs = { $count ->
    [one] 1 fane
   *[other] { $count } faner
}
spaces-delete = Slett område

team-title = Team
team-just-you = Bare deg i dette området
team-agents = { $count ->
    [one] Du og 1 agent
   *[other] Du og { $count } agenter
}
team-empty = Ingen her ennå
team-you = Du
team-agent = Agent

services-title = Bakgrunnstjenester
services-processes = { $count ->
    [one] 1 prosess
   *[other] { $count } prosesser
}
services-kill-all = Avslutt alle
services-not-running = Tjenesten kjører ikke
services-start-with = Start med:
services-empty = Ingen aktive prosesser
services-filter = Filtrer prosesser …
services-no-match = Ingen prosesser funnet
services-connected = Tilkoblet
services-disconnected = Frakoblet
services-attached = tilknyttet
services-kill = Tvangsavslutt
services-memory = Minne
services-size = Størrelse
services-shell = Skall

error-title = Feil

history-search = Søk i historikk
history-clear-all = Tøm alt
history-clear-confirm = Tømme all historikk?
history-clear-warning = Dette kan ikke angres.
history-cancel = Avbryt
history-today = I dag
history-yesterday = I går
history-days-ago = { $count } dager siden
history-day-offset = Dag -{ $count }

settings-title = Innstillinger
settings-loading = Laster innstillinger …
settings-stored = Lagret i ~/.vmux/settings.ron
settings-other = Annet
settings-software-update = Programvareoppdatering
settings-check-updates = Se etter oppdateringer
settings-check-updates-hint = Sjekker automatisk ved oppstart og hver time når automatisk oppdatering er aktivert.
settings-update-unavailable = Utilgjengelig
settings-update-unavailable-hint = Oppdateringsprogrammet er ikke inkludert i denne byggversjonen.
settings-update-checking = Ser etter oppdateringer …
settings-update-checking-hint = Ser etter oppdateringer …
settings-update-check-again = Sjekk på nytt
settings-update-current = Vmux er oppdatert.
settings-update-downloading = Laster ned …
settings-update-downloading-hint = Laster ned Vmux { $version } …
settings-update-installing = Installerer …
settings-update-installing-hint = Installerer Vmux { $version } …
settings-update-ready = Oppdatering klar
settings-update-ready-hint = Vmux { $version } er klar. Start på nytt for å ta den i bruk.
settings-update-try-again = Prøv igjen
settings-update-failed = Kunne ikke se etter oppdateringer.
settings-item = Element
settings-item-number = Element { $number }
settings-press-key = Trykk på en tast …
settings-saved = Lagret
settings-record-key = Klikk for å registrere en ny tastekombinasjon

tray-open-window = Åpne vindu
tray-close-window = Lukk vindu
tray-pause-recording = Sett opptak på pause
tray-resume-recording = Fortsett opptak
tray-finish-recording = Fullfør opptak
tray-quit = Avslutt Vmux

composer-attach-files = Legg ved filer (/upload)
composer-remove-attachment = Fjern vedlegg

layout-back = Tilbake
layout-forward = Frem
layout-reload = Last inn på nytt
layout-bookmark-page = Legg til bokmerke for denne siden
layout-remove-bookmark = Fjern bokmerke
layout-pin-page = Fest denne siden
layout-unpin-page = Løsne denne siden
layout-manage-extensions = Administrer utvidelser
layout-new-stack = Ny stakk
layout-close-tab = Lukk fane
layout-bookmark = Bokmerke
layout-pin = Fest
layout-new-tab = Ny fane
layout-team = Team

command-switch-space = Bytt område …
command-search-ask = Søk eller spør …
command-new-tab-placeholder = Søk, skriv inn en URL eller velg Terminal …
command-placeholder = Skriv inn en URL, søk i faner eller > for kommandoer …
command-composer-placeholder = Skriv / for kommandoer eller @ for medier
command-send = Send (Enter)
command-terminal = Terminal
command-open-terminal = Åpne i Terminal
command-stack = Stakk
command-tabs = { $count ->
    [one] 1 fane
   *[other] { $count } faner
}
command-prompt = Prompt
command-new-tab = Ny fane
command-search = Søk
command-open-value = Åpne «{ $value }»
command-search-value = Søk etter «{ $value }»

schema-appearance = Utseende
schema-general = Generelt
schema-layout = Oppsett
schema-layout-detail = Vindu, ruter, sidepanel og fokusring.
schema-agent = Agent
schema-agent-detail = Agentatferd og tillatelser for verktøy.
schema-shortcuts = Snarveier
schema-shortcuts-detail = Skrivebeskyttet visning. Rediger settings.ron direkte for å endre tastebindinger.
schema-terminal = Terminal
schema-browser = Nettleser
schema-mode = Modus
schema-mode-detail = Fargeskjema for nettsider. Enhet følger systemet ditt.
schema-device = Enhet
schema-light = Lys
schema-dark = Mørk
schema-language = Språk
schema-language-detail = Bruk systemet, en-US, ja eller en BCP 47-tagg med en tilhørende ~/.vmux/locales/<tag>.ftl-katalog.
schema-auto-update = Automatisk oppdatering
schema-auto-update-detail = Se etter og installer oppdateringer ved oppstart og hver time.
schema-startup-url = Oppstarts-URL
schema-startup-url-detail = Tomt åpner prompten i kommandolinjen.
schema-search-engine = Søkemotor
schema-search-engine-detail = Brukes til nettsøk fra Start og kommandolinjen.
schema-window = Vindu
schema-pane = Rute
schema-side-sheet = Sideark
schema-focus-ring = Fokusring
schema-run-placement = Tillat overstyring av kjøreplassering
schema-run-placement-detail = La agenter velge rutemodus, retning og forankring for kjøring.
schema-leader = Leder
schema-leader-detail = Prefikstast for akkordsnarveier.
schema-chord-timeout = Tidsavbrudd for akkord
schema-chord-timeout-detail = Millisekunder før et akkordprefiks utløper.
schema-bindings = Bindinger
schema-confirm-close = Bekreft lukking
schema-confirm-close-detail = Spør før en terminal med en kjørende prosess lukkes.
schema-default-theme = Standardtema
schema-default-theme-detail = Navnet på det aktive temaet fra temalisten.

settings-empty = (tom)
settings-none = (ingen)

schema-system = System
schema-editor = Redigering
schema-recording = Opptak
schema-radius = Radius
schema-padding = Utfylling
schema-gap = Avstand
schema-width = Bredde
schema-color = Farge
schema-red = Rød
schema-green = Grønn
schema-blue = Blå
schema-follow-files = Følg filer
schema-tidy-files = Rydd filer
schema-tidy-files-max = Terskel for filrydding
schema-tidy-files-auto = Rydd filer automatisk
schema-app-providers = App-leverandører
schema-provider = Leverandør
schema-kind = Type
schema-models = Modeller
schema-acp = ACP-agenter
schema-id = ID
schema-name = Navn
schema-command = Kommando
schema-arguments = Argumenter
schema-environment = Miljø
schema-working-directory = Arbeidsmappe
schema-shell = Skall
schema-font-family = Skriftfamilie
schema-startup-directory = Oppstartsmappe
schema-themes = Temaer
schema-color-scheme = Fargeoppsett
schema-font-size = Skriftstørrelse
schema-line-height = Linjehøyde
schema-cursor-style = Markørstil
schema-cursor-blink = Markørblink
schema-custom-themes = Egendefinerte temaer
schema-foreground = Forgrunn
schema-background = Bakgrunn
schema-cursor = Markør
schema-ansi-colors = ANSI-farger
schema-keymap = Tasteoppsett
schema-explorer = Utforsker
schema-visible = Synlig
schema-language-servers = Språkservere
schema-servers = Servere
schema-language-id = Språk-ID
schema-root-markers = Rotmarkører
schema-output-directory = Utdatamappe

menu-scene = Scene
menu-layout = Oppsett
menu-terminal = Terminal
menu-browser = Nettleser
menu-service = Tjeneste
menu-bookmark = Bokmerke
menu-edit = Rediger

layout-knowledge = Kunnskap
layout-open-knowledge = Åpne Kunnskap
layout-open-welcome-knowledge = Åpne Velkommen til Kunnskap
layout-open-path = Åpne { $path }
layout-fold-knowledge = Slå sammen kunnskap
layout-unfold-knowledge = Utvid kunnskap
layout-bookmarks = Bokmerker
layout-new-folder = Ny mappe
layout-add-to-bookmarks = Legg til i Bokmerker
layout-move-to-bookmarks = Flytt til Bokmerker
layout-stack-number = Stabel { $number }
layout-fold-stack = Slå sammen stabel
layout-unfold-stack = Utvid stabel
layout-close-stack = Lukk stabel
layout-bookmark-in = Bokmerke i { $folder }

common-cancel = Avbryt
common-delete = Slett
common-save = Arkiver
common-rename = Gi nytt navn
common-expand = Utvid
common-collapse = Slå sammen
common-loading = Laster inn …
common-error = Feil
common-output = Utdata
common-pending = Venter
common-current = gjeldende
common-stop = Stopp
services-command = Vmux-tjeneste
services-uptime-seconds = { $seconds } s
services-uptime-minutes = { $minutes } m { $seconds } s
services-uptime-hours = { $hours } t { $minutes } m
services-uptime-days = { $days } d { $hours } t

error-page-failed-load = Siden kunne ikke lastes inn
error-page-not-found = Fant ikke siden
error-unknown-host = Ukjent Vmux-appvert: { $host }

history-title = Logg

command-new-app-chat = Ny { $provider }/{ $model }-chat (app)
command-interactive-mode-user = Scene > Interaktiv modus > Bruker
command-interactive-mode-player = Scene > Interaktiv modus > Spiller
command-minimize-window = Layout > Vindu > Minimer
command-toggle-layout = Layout > Layout > Bytt layout
command-close-tab = Layout > Fane > Lukk fane
command-new-task = Layout > Fane > Ny oppgave …
command-next-tab = Layout > Fane > Neste fane
command-prev-tab = Layout > Fane > Forrige fane
command-rename-tab = Layout > Fane > Gi fanen nytt navn
command-tab-select-1 = Layout > Fane > Velg fane 1
command-tab-select-2 = Layout > Fane > Velg fane 2
command-tab-select-3 = Layout > Fane > Velg fane 3
command-tab-select-4 = Layout > Fane > Velg fane 4
command-tab-select-5 = Layout > Fane > Velg fane 5
command-tab-select-6 = Layout > Fane > Velg fane 6
command-tab-select-7 = Layout > Fane > Velg fane 7
command-tab-select-8 = Layout > Fane > Velg fane 8
command-tab-select-last = Layout > Fane > Velg siste fane
command-close-pane = Layout > Rute > Lukk rute
command-select-pane-left = Layout > Rute > Velg ruten til venstre
command-select-pane-right = Layout > Rute > Velg ruten til høyre
command-select-pane-up = Layout > Rute > Velg ruten over
command-select-pane-down = Layout > Rute > Velg ruten under
command-swap-pane-prev = Layout > Rute > Bytt med forrige rute
command-swap-pane-next = Layout > Rute > Bytt med neste rute
command-equalize-pane-size = Layout > Rute > Gjør rutestørrelser like
command-resize-pane-left = Layout > Rute > Endre rute mot venstre
command-resize-pane-right = Layout > Rute > Endre rute mot høyre
command-resize-pane-up = Layout > Rute > Endre rute oppover
command-resize-pane-down = Layout > Rute > Endre rute nedover
command-stack-close = Layout > Stabel > Lukk stabel
command-stack-next = Layout > Stabel > Neste stabel
command-stack-previous = Layout > Stabel > Forrige stabel
command-stack-reopen = Layout > Stabel > Åpne lukket side på nytt
command-stack-swap-prev = Layout > Stabel > Flytt stabel til venstre
command-stack-swap-next = Layout > Stabel > Flytt stabel til høyre
command-space-open = Layout > Område > Områder
command-terminal-close = Terminal > Lukk terminal
command-terminal-next = Terminal > Neste terminal
command-terminal-prev = Terminal > Forrige terminal
command-terminal-clear = Terminal > Tøm terminal
command-browser-prev-page = Nettleser > Navigering > Tilbake
command-browser-next-page = Nettleser > Navigering > Frem
command-browser-reload = Nettleser > Navigering > Last inn på nytt
command-browser-hard-reload = Nettleser > Navigering > Tvungen ny innlasting
command-open-in-place = Nettleser > Åpne > Åpne her
command-open-in-new-stack = Nettleser > Åpne > Åpne i ny stabel
command-open-in-pane-top = Nettleser > Åpne > Åpne i rute over
command-open-in-pane-right = Nettleser > Åpne > Åpne i rute til høyre
command-open-in-pane-bottom = Nettleser > Åpne > Åpne i rute under
command-open-in-pane-left = Nettleser > Åpne > Åpne i rute til venstre
command-open-in-new-tab = Nettleser > Åpne > Åpne i ny fane
command-open-in-new-space = Nettleser > Åpne > Åpne i nytt område
command-browser-zoom-in = Nettleser > Visning > Zoom inn
command-browser-zoom-out = Nettleser > Visning > Zoom ut
command-browser-zoom-reset = Nettleser > Visning > Faktisk størrelse
command-browser-dev-tools = Nettleser > Visning > Utviklerverktøy
command-browser-open-command-bar = Nettleser > Linje > Kommandolinje
command-browser-open-page-in-command-bar = Nettleser > Linje > Rediger side
command-browser-open-path-bar = Nettleser > Linje > Stinavigering
command-browser-open-commands = Nettleser > Linje > Kommandoer
command-browser-open-history = Nettleser > Linje > Logg
command-service-open = Tjeneste > Åpne tjenesteovervåking
command-bookmark-toggle-active = Bokmerke > Legg til bokmerke for siden
command-bookmark-pin-active = Bokmerke > Fest side

layout-tab = Fane
layout-no-stacks = Ingen stabler
layout-loading = Laster inn …
layout-no-markdown-files = Ingen Markdown-filer
layout-empty-folder = Tom mappe
layout-worktree = arbeidstre
layout-folder-name = Mappenavn
layout-no-pins-bookmarks = Ingen festede sider eller bokmerker
layout-move-to = Flytt til { $folder }
layout-bookmark-current-page = Legg til bokmerke for gjeldende side
layout-rename-folder = Gi mappen nytt navn
layout-remove-folder = Fjern mappe
layout-update-downloading = Laster ned oppdatering
layout-update-installing = Installerer oppdatering …
layout-update-ready = Ny versjon tilgjengelig
layout-restart-update = Start på nytt for å oppdatere

agent-preparing = Klargjør agent …
agent-send-all-queued = Send alle forespørsler i kø nå (Esc)
agent-send = Send (Enter)
agent-ready = Klar når du er det.
agent-loading-older = Laster inn eldre meldinger …
agent-load-older = Last inn eldre meldinger
agent-continued-from = Fortsatt fra { $source }
agent-older-context-omitted = eldre kontekst utelatt
agent-interrupted = avbrutt
agent-allow-tool = Tillat { $tool }?
agent-deny = Avvis
agent-allow-always = Tillat alltid
agent-allow = Tillat
agent-loading-sessions = Laster inn økter …
agent-no-resumable-sessions = Fant ingen økter som kan gjenopptas
agent-no-matching-sessions = Ingen samsvarende økter
agent-no-matching-models = Ingen samsvarende modeller
agent-choice-help = ↑/↓ eller Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Velg repositoriummappe
agent-choose-repository-detail = Velg det lokale Git-repositoriet agenten skal bruke.
agent-choosing = Velger …
agent-choose-folder = Velg mappe
agent-queued = i kø
agent-attached = Vedlagt:
agent-cancel-queued = Avbryt forespørsel i kø
agent-resume-queued = Gjenoppta forespørsler i kø
agent-clear-queue = Tøm kø
agent-send-all-now = send alle nå
agent-choose-option = Velg et alternativ over
agent-loading-media = Laster inn medier …
agent-no-matching-media = Ingen samsvarende medier
agent-prompt-context = Forespørselskontekst
agent-details = Detaljer
agent-path = Sti
agent-tool = Verktøy
agent-server = Tjener
agent-bytes = { $count } byte
agent-worked-for = Arbeidet i { $duration }
agent-worked-for-steps = { $count ->
    [one] Arbeidet i { $duration } · 1 trinn
   *[other] Arbeidet i { $duration } · { $count } trinn
}
agent-tool-guardian-review = Guardian-gjennomgang
agent-tool-read-files = Leste filer
agent-tool-viewed-image = Viste bilde
agent-tool-used-browser = Brukte nettleser
agent-tool-searched-files = Søkte i filer
agent-tool-ran-commands = Kjørte kommandoer
agent-thinking = Tenker
agent-subagent = Underagent
agent-prompt = Forespørsel
agent-thread = Tråd
agent-parent = Overordnet
agent-children = Underordnede
agent-call = Kall
agent-raw-event = Råhendelse
agent-plan = Plan
agent-tasks = { $count ->
    [one] 1 oppgave
   *[other] { $count } oppgaver
}
agent-edited = Redigert
agent-reconnecting = Kobler til på nytt { $attempt }/{ $total }
agent-status-running = Kjører
agent-status-done = Ferdig
agent-status-failed = Mislyktes
agent-status-pending = Venter
agent-slash-attach-files = Legg ved filer
agent-slash-resume-session = Gjenoppta en tidligere økt
agent-slash-select-model = Velg modell
agent-slash-continue-cli = Fortsett denne økten i CLI
agent-session-just-now = akkurat nå
agent-session-minutes-ago = for { $count } m siden
agent-session-hours-ago = for { $count } t siden
agent-session-days-ago = for { $count } d siden
agent-working-working = Arbeider
agent-working-thinking = Tenker
agent-working-pondering = Grubler
agent-working-noodling = Fundrer
agent-working-percolating = Lar det modne
agent-working-conjuring = Tryller frem
agent-working-cooking = Kokkelérer
agent-working-brewing = Brygger
agent-working-musing = Reflekterer
agent-working-ruminating = Grunner
agent-working-scheming = Pønsker
agent-working-synthesizing = Syntetiserer
agent-working-tinkering = Fikser
agent-working-churning = Kverner
agent-working-vibing = Vibber
agent-working-simmering = Småkoker
agent-working-crafting = Utformer
agent-working-divining = Spår
agent-working-mulling = Tenker gjennom
agent-working-spelunking = Utforsker

editor-toggle-explorer = Vis/skjul Utforsker (Cmd+B)
editor-unsaved = ikke arkivert
editor-rendered-markdown = Gjengitt Markdown med direkteredigering
editor-note = Notat
editor-source-editor = Kilderedigering
editor-editor = Redigering
editor-git-diff = Git-diff
editor-diff = Diff
editor-tidy = Rydd
editor-always = Alltid
editor-unchanged-previews = { $count ->
    [one] ✦ 1 uendret forhåndsvisning
   *[other] ✦ { $count } uendrede forhåndsvisninger
}
editor-open-externally = Åpne eksternt
editor-changed-line = Endret linje
editor-go-to-definition = Gå til definisjon
editor-find-references = Finn referanser
editor-references = { $count ->
    [one] 1 referanse
   *[other] { $count } referanser
}
editor-lsp-starting = { $server } starter …
editor-lsp-not-installed = { $server } — ikke installert
editor-explorer = Utforsker
editor-open-editors = Åpne redigerere
editor-outline = Disposisjon
editor-new-file = Ny fil
editor-new-folder = Ny mappe
editor-delete-confirm = Slett «{ $name }»? Dette kan ikke angres.
editor-created-folder = Opprettet mappen { $name }
editor-created-file = Opprettet filen { $name }
editor-renamed-to = Endret navn til { $name }
editor-deleted = Slettet { $name }
editor-failed-decode-image = Kunne ikke dekode bildet
editor-preview-large-image = bilde (for stort til å forhåndsvise)
editor-preview-binary = binærfil
editor-preview-file = fil

git-status-clean = ren
git-status-modified = endret
git-status-staged = staged
git-status-staged-modified = staged*
git-status-untracked = ikke sporet
git-status-deleted = slettet
git-status-conflict = konflikt
git-accept-all = ✓ godta alle
git-unstage = Fjern fra staging
git-confirm-deny-all = Bekreft avvis alle
git-deny-all = ✗ avvis alle
git-commit-message = commit-melding
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Laster inn diff …
git-no-changes = Ingen endringer å vise
git-accept = ✓ godta
git-deny = ✗ avvis
git-show-unchanged-lines = Vis { $count } uendrede linjer

terminal-loading = Laster inn …
terminal-runs-when-ready = kjører når klar · Ctrl+C tømmer · Esc hopper over
terminal-booting = starter
terminal-type-command = skriv en kommando · kjører når klar · Esc hopper over

setup-tagline-claude = Anthropic sin kodeagent, i Vmux
setup-tagline-codex = OpenAI sin kodeagent, i Vmux
setup-tagline-vibe = Mistral sin kodeagent, i Vmux
setup-install-title = Installer { $name } CLI
setup-homebrew-required = Homebrew kreves for å installere { $command } og er ikke satt opp ennå. Vmux installerer Homebrew først, deretter { $name }.
setup-terminal-instructions = Trykk på Retur i terminalen for å starte, og oppgi deretter Mac-passordet når du blir bedt om det.
setup-command-missing = Vmux åpnet denne siden fordi den lokale kommandoen { $command } ikke er installert ennå. Kjør kommandoen nedenfor for å hente den.
setup-install-failed = Installasjonen ble ikke fullført. Se terminalen for detaljer, og prøv på nytt.
setup-installing = Installerer …
setup-install-homebrew = Installer Homebrew + { $name }
setup-run-install = Kjør installasjonskommando
setup-auto-reload = Vmux kjører den i en terminal og laster inn på nytt når { $command } er klar.

debug-title = Feilsøking
debug-auto-update = Automatisk oppdatering
debug-simulate-update = Simuler tilgjengelig oppdatering
debug-simulate-download = Simuler nedlasting
debug-clear-update = Fjern oppdatering
debug-trigger-restart = Utløs omstart

command-manage-spaces = Administrer arbeidsområder …
command-pane-stack-location = rute { $pane } / stabel { $stack }
command-space-pane-stack-location = { $space } / rute { $pane } / stabel { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Interaktiv modus
command-group-window = Vindu
command-group-tab = Fane
command-group-pane = Rute
command-group-stack = Stabel
command-group-space = Arbeidsområde
command-group-navigation = Navigering
command-group-open = Åpne
command-group-view = Visning
command-group-bar = Linje

menu-close-vmux = Lukk Vmux

agents-terminal-coding-agent = Terminalbasert kodeagent
