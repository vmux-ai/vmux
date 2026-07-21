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
