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
