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
start-title = Start
start-tagline = Én melding. Hva som helst, ferdig.

agents-title = Agenter
agents-search = Søk etter ACP- og CLI-agenter…
agents-empty = Ingen samsvarende agenter
agents-empty-detail = Prøv et navn, kjøretid eller ACP/CLI.
agents-install-failed = Installasjon mislyktes
agents-updating = Oppdaterer…
agents-retrying = Prøver igjen…
agents-preparing = Forbereder…

extensions-title = Utvidelser
extensions-search = Søk installerte eller Chrome Nettmarked…
extensions-relaunch = Start på nytt for å bruke
extensions-empty = Ingen utvidelser installert
extensions-no-match = Ingen samsvarende utvidelser
extensions-empty-detail = Søk i Chrome Nettmarked ovenfor og trykk Return.
extensions-no-match-detail = Prøv et annet navn eller utvidelse-ID.
extensions-on = På
extensions-off = Av
extensions-enable-confirm = Aktiver { $name }?
extensions-enable-permissions = Aktiver { $name } og tillat:

lsp-title = Språkservere
lsp-search = Søk etter språkservere, lintere, formaterere…
lsp-loading = Laster katalog…
lsp-empty = Ingen samsvarende språkservere
lsp-empty-detail = Prøv et annet språk, linter eller formaterer.
lsp-needs = krever { $tool }
lsp-status-available = Tilgjengelig
lsp-status-on-path = I PATH
lsp-status-installing = Installerer…
lsp-status-installed = Installert
lsp-status-outdated = Oppdatering tilgjengelig
lsp-status-running = Kjører
lsp-status-failed = Mislyktes

spaces-title = Arbeidsområder
spaces-new-placeholder = Nytt arbeidsområdenavn
spaces-empty = Ingen arbeidsområder
spaces-default-name = Arbeidsområde { $number }
spaces-tabs = { $count ->
    [one] 1 fane
   *[other] { $count } faner
}
spaces-delete = Slett arbeidsområde

team-title = Team
team-just-you = Bare deg i dette arbeidsområdet
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
services-filter = Filtrer prosesser…
services-no-match = Ingen samsvarende prosesser
services-connected = Tilkoblet
services-disconnected = Frakoblet
services-attached = tilknyttet
services-kill = Avslutt
services-memory = Minne
services-size = Størrelse
services-shell = Skall

error-title = Feil

history-search = Søk i historikk
history-clear-all = Slett alt
history-clear-confirm = Slett all historikk?
history-clear-warning = Dette kan ikke angres.
history-cancel = Avbryt
history-today = I dag
history-yesterday = I går
history-days-ago = { $count } dager siden
history-day-offset = Dag -{ $count }

settings-title = Innstillinger
settings-loading = Laster innstillinger…
settings-stored = Lagret i ~/.vmux/settings.ron
settings-other = Annet
settings-software-update = Programvareoppdatering
settings-check-updates = Se etter oppdateringer
settings-check-updates-hint = Sjekker automatisk ved oppstart og hver time når automatisk oppdatering er aktivert.
settings-update-unavailable = Ikke tilgjengelig
settings-update-unavailable-hint = Oppdatering er ikke inkludert i denne versjonen.
settings-update-checking = Sjekker…
settings-update-checking-hint = Ser etter oppdateringer…
settings-update-check-again = Sjekk igjen
settings-update-current = Vmux er oppdatert.
settings-update-downloading = Laster ned…
settings-update-downloading-hint = Laster ned Vmux { $version }…
settings-update-installing = Installerer…
settings-update-installing-hint = Installerer Vmux { $version }…
settings-update-ready = Oppdatering klar
settings-update-ready-hint = Vmux { $version } er klar. Start på nytt for å bruke den.
settings-update-try-again = Prøv igjen
settings-update-failed = Kan ikke se etter oppdateringer.
settings-item = Element
settings-item-number = Element { $number }
settings-press-key = Trykk en tast…
settings-saved = Lagret
settings-record-key = Klikk for å registrere en ny tastekombinasjon

tray-open-window = Åpne vindu
tray-close-window = Lukk vindu
tray-pause-recording = Sett opptak på pause
tray-resume-recording = Gjenoppta opptak
tray-finish-recording = Fullfør opptak
tray-quit = Avslutt Vmux

composer-attach-files = Legg ved filer (/upload)
composer-remove-attachment = Fjern vedlegg

layout-back = Tilbake
layout-forward = Fremover
layout-reload = Last inn på nytt
layout-bookmark-page = Legg til bokmerke
layout-remove-bookmark = Fjern bokmerke
layout-pin-page = Fest denne siden
layout-unpin-page = Løsne denne siden
layout-manage-extensions = Administrer utvidelser
layout-new-stack = Ny stabel
layout-close-tab = Lukk fane
layout-bookmark = Bokmerke
layout-pin = Fest
layout-new-tab = Ny fane
layout-team = Team

command-switch-space = Bytt arbeidsområde…
command-search-ask = Søk eller spør…
command-new-tab-placeholder = Søk eller skriv en URL, eller velg Terminal…
command-placeholder = Skriv en URL, søk i faner, eller > for kommandoer…
command-composer-placeholder = Skriv / for kommandoer eller @ for media
command-send = Send (Enter)
command-terminal = Terminal
command-open-terminal = Åpne i Terminal
command-stack = Stabel
command-tabs = { $count ->
    [one] 1 fane
   *[other] { $count } faner
}
command-prompt = Melding
command-new-tab = Ny fane
command-search = Søk
command-open-value = Åpne «{ $value }»
command-search-value = Søk etter «{ $value }»

schema-appearance = Utseende
schema-general = Generelt
schema-layout = Oppsett
schema-layout-detail = Vindu, paneler, sidefelt og fokusring.
schema-agent = Agent
schema-agent-detail = Agentadferd og verktøytillatelser.
schema-shortcuts = Snarveier
schema-shortcuts-detail = Skrivebeskyttet visning. Rediger settings.ron direkte for å endre bindinger.
schema-terminal = Terminal
schema-browser = Nettleser
schema-mode = Modus
schema-mode-detail = Fargevalg for nettsider. Enheten følger systemet ditt.
schema-device = Enhet
schema-light = Lys
schema-dark = Mørk
schema-language = Språk
schema-language-detail = Bruk system, en-US, ja, eller en BCP 47-kode med en tilsvarende ~/.vmux/locales/<tag>.ftl-katalog.
schema-auto-update = Automatisk oppdatering
schema-auto-update-detail = Se etter og installer oppdateringer ved oppstart og hver time.
schema-startup-url = Oppstarts-URL
schema-startup-url-detail = Tom åpner kommandofeltet.
schema-search-engine = Søkemotor
schema-search-engine-detail = Brukes for nettsøk fra Start og kommandofeltet.
schema-window = Vindu
schema-pane = Panel
schema-side-sheet = Sideark
schema-focus-ring = Fokusring
schema-run-placement = Tillat overstyring av kjøreplassering
schema-run-placement-detail = La agenter velge kjørepanelmodus, retning og ankerpunkt.
schema-leader = Leder
schema-leader-detail = Prefikstast for akkordvalg.
schema-chord-timeout = Akkordtidsavbrudd
schema-chord-timeout-detail = Millisekunder før et akkordprefiks utløper.
schema-bindings = Bindinger
schema-confirm-close = Bekreft lukking
schema-confirm-close-detail = Spør før lukking av en terminal med en kjørende prosess.
schema-default-theme = Standardtema
schema-default-theme-detail = Navn på det aktive temaet fra temalisten.
