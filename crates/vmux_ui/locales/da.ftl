common-open = Åbn
common-close = Luk
common-install = Installer
common-uninstall = Afinstaller
common-update = Opdatering
common-retry = Prøv igen
common-refresh = Opdater
common-remove = Fjern
common-enable = Aktiver
common-disable = Deaktiver
common-new = Ny
common-active = aktiv
common-running = løb
common-done = gjort
common-failed = Mislykkedes
common-installed = Installeret
common-items = { $count ->
    [one] { $count } element
   *[other] { $count } elementer
}
start-title = Start
start-tagline = En prompt. Hvad som helst, gjort.

agents-title = Agenter
agents-search = Søg efter ACP og CLI agenter...
agents-empty = Ingen matchende agenter
agents-empty-detail = Prøv et navn, runtime eller ACP/CLI.
agents-install-failed = Installationen mislykkedes
agents-updating = Opdaterer...
agents-retrying = Prøver igen...
agents-preparing = Forbereder...

extensions-title = Udvidelser
extensions-search = Søg installeret eller Chrome Web Store...
extensions-relaunch = Genstart for at ansøge
extensions-empty = Ingen udvidelser installeret
extensions-no-match = Ingen matchende udvidelser
extensions-empty-detail = Søg på Chrome Web Store ovenfor, og tryk på Return.
extensions-no-match-detail = Prøv et andet navn eller udvidelses-id.
extensions-on = På
extensions-off = Fra
extensions-enable-confirm = Aktiver { $name }?
extensions-enable-permissions = Aktiver { $name } og tillad:

lsp-title = Sprogservere
lsp-search = Søg efter sprogservere, linters, formatere...
lsp-loading = Indlæser katalog...
lsp-empty = Ingen matchende sprogservere
lsp-empty-detail = Prøv et andet sprog, linter eller formatering.
lsp-needs = har brug for { $tool }
lsp-status-available = Tilgængelig
lsp-status-on-path = På PATH
lsp-status-installing = Installerer...
lsp-status-installed = Installeret
lsp-status-outdated = Opdatering tilgængelig
lsp-status-running = Løb
lsp-status-failed = Mislykkedes

spaces-title = Mellemrum
spaces-new-placeholder = Nyt pladsnavn
spaces-empty = Ingen mellemrum
spaces-default-name = Mellemrum { $number }
spaces-tabs = { $count ->
    [one] 1 fane
   *[other] { $count } faner
}
spaces-delete = Slet mellemrum

team-title = Team
team-just-you = Bare dig i dette rum
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
services-kill-all = Dræb alle
services-not-running = Tjenesten kører ikke
services-start-with = Start med:
services-empty = Ingen aktive processer
services-filter = Filtrer processer...
services-no-match = Ingen matchende processer
services-connected = Forbundet
services-disconnected = Afbrudt
services-attached = vedhæftet
services-kill = Dræb
services-memory = Hukommelse
services-size = Størrelse
services-shell = Shell

error-title = Fejl

history-search = Søgehistorik
history-clear-all = Ryd alle
history-clear-confirm = Vil du rydde al historik?
history-clear-warning = Dette kan ikke fortrydes.
history-cancel = Annuller
history-today = I dag
history-yesterday = I går
history-days-ago = { $count } dage siden
history-day-offset = Dag -{ $count }

settings-title = Indstillinger
settings-loading = Indlæser indstillinger...
settings-stored = Gemt i ~/.vmux/settings.ron
settings-other = Andet
settings-software-update = Softwareopdatering
settings-check-updates = Se efter opdateringer
settings-check-updates-hint = Tjekker automatisk ved lancering og hver time, når automatisk opdatering er aktiveret.
settings-update-unavailable = Ikke tilgængelig
settings-update-unavailable-hint = Updater er ikke inkluderet i denne build.
settings-update-checking = Tjekker...
settings-update-checking-hint = Søger efter opdateringer...
settings-update-check-again = Tjek igen
settings-update-current = Vmux er opdateret.
settings-update-downloading = Downloader...
settings-update-downloading-hint = Downloader Vmux { $version }...
settings-update-installing = Installerer...
settings-update-installing-hint = Installerer Vmux { $version }...
settings-update-ready = Opdatering klar
settings-update-ready-hint = Vmux { $version } er klar. Genstart for at anvende det.
settings-update-try-again = Prøv igen
settings-update-failed = Kan ikke søge efter opdateringer.
settings-item = Vare
settings-item-number = Vare { $number }
settings-press-key = Tryk på en tast...
settings-saved = Gemt
settings-record-key = Klik for at optage en ny nøglekombination

tray-open-window = Åbn vindue
tray-close-window = Luk vinduet
tray-pause-recording = Sæt optagelse på pause
tray-resume-recording = Genoptag optagelse
tray-finish-recording = Afslut optagelsen
tray-quit = Afslut Vmux

composer-attach-files = Vedhæft filer (/upload)
composer-remove-attachment = Fjern vedhæftet fil

layout-back = Tilbage
layout-forward = Fremad
layout-reload = Genindlæs
layout-bookmark-page = Bogmærk denne side
layout-remove-bookmark = Fjern bogmærke
layout-pin-page = Fastgør denne side
layout-unpin-page = Frigør denne side
layout-manage-extensions = Administrer udvidelser
layout-new-stack = Ny stak
layout-close-tab = Luk fanen
layout-bookmark = Bogmærke
layout-pin = Pin
layout-new-tab = Ny fane
layout-team = Team

command-switch-space = Skift mellemrum...
command-search-ask = Søg eller spørg...
command-new-tab-placeholder = Søg eller skriv en URL, eller vælg Terminal...
command-placeholder = Indtast en URL, søgefaner eller > for kommandoer...
command-composer-placeholder = Skriv / for kommandoer eller @ for medier
command-send = Send (Enter)
command-terminal = Terminal
command-open-terminal = Åbn i Terminal
command-stack = Stak
command-tabs = { $count ->
    [one] 1 fane
   *[other] { $count } faner
}
command-prompt = Spørg
command-new-tab = Ny fane
command-search = Søg
command-open-value = Åbn "{ $value }"
command-search-value = Søg efter "{ $value }"

schema-appearance = Udseende
schema-general = Generelt
schema-layout = Layout
schema-layout-detail = Vindue, ruder, sidebjælke og fokusring.
schema-agent = Agent
schema-agent-detail = Agentadfærd og værktøjstilladelser.
schema-shortcuts = Genveje
schema-shortcuts-detail = Skrivebeskyttet visning. Rediger settings.ron direkte for at ændre bindinger.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = tilstand
schema-mode-detail = Farveskema til websider. Enheden følger dit system.
schema-device = Enhed
schema-light = Lys
schema-dark = Mørk
schema-language = Sprog
schema-language-detail = Brug system, en-US, ja eller et hvilket som helst BCP 47 tag med et matchende ~/.vmux/locales/<tag>.ftl katalog.
schema-auto-update = Automatisk opdatering
schema-auto-update-detail = Se efter og installer opdateringer ved lancering og hver time.
schema-startup-url = Opstart URL
schema-startup-url-detail = Tom åbner kommandolinjeprompten.
schema-search-engine = Søgemaskine
schema-search-engine-detail = Bruges til websøgninger fra Start og kommandolinjen.
schema-window = vindue
schema-pane = Rude
schema-side-sheet = Sideark
schema-focus-ring = Fokusring
schema-run-placement = Tillad tilsidesættelse af køreplacering
schema-run-placement-detail = Lad agenter vælge kørselsrudetilstand, retning og anker.
schema-leader = Leder
schema-leader-detail = Præfiksetast for akkordgenveje.
schema-chord-timeout = Akkord timeout
schema-chord-timeout-detail = Millisekunder før et akkordpræfiks udløber.
schema-bindings = Indbindinger
schema-confirm-close = Bekræft luk
schema-confirm-close-detail = Spørg, før du lukker en terminal med en kørende proces.
schema-default-theme = Standard tema
schema-default-theme-detail = Navn på det aktive tema fra temalisten.
