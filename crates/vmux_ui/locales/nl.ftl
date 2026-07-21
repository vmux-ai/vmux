common-open = Openen
common-close = Sluiten
common-install = Installeren
common-uninstall = Verwijderen
common-update = Bijwerken
common-retry = Opnieuw proberen
common-refresh = Vernieuwen
common-remove = Verwijderen
common-enable = Inschakelen
common-disable = Uitschakelen
common-new = Nieuw
common-active = actief
common-running = actief
common-done = klaar
common-failed = Mislukt
common-installed = Geïnstalleerd
common-items = { $count ->
    [one] { $count } item
   *[other] { $count } items
}
start-title = Start
start-tagline = Eén prompt. Alles geregeld.

agents-title = Agents
agents-search = ACP- en CLI-agents zoeken…
agents-empty = Geen overeenkomende agents
agents-empty-detail = Probeer een naam, runtime of ACP/CLI.
agents-install-failed = Installatie mislukt
agents-updating = Bijwerken…
agents-retrying = Opnieuw proberen…
agents-preparing = Voorbereiden…

extensions-title = Extensies
extensions-search = Geïnstalleerde extensies of Chrome Web Store doorzoeken…
extensions-relaunch = Herstarten om toe te passen
extensions-empty = Geen extensies geïnstalleerd
extensions-no-match = Geen overeenkomende extensies
extensions-empty-detail = Doorzoek hierboven de Chrome Web Store en druk op Return.
extensions-no-match-detail = Probeer een andere naam of extensie-ID.
extensions-on = Aan
extensions-off = Uit
extensions-enable-confirm = { $name } inschakelen?
extensions-enable-permissions = { $name } inschakelen en toestaan:

lsp-title = Taalservers
lsp-search = Taalservers, linters, formatters zoeken…
lsp-loading = Catalogus laden…
lsp-empty = Geen overeenkomende taalservers
lsp-empty-detail = Probeer een andere taal, linter of formatter.
lsp-needs = heeft { $tool } nodig
lsp-status-available = Beschikbaar
lsp-status-on-path = Op PATH
lsp-status-installing = Installeren…
lsp-status-installed = Geïnstalleerd
lsp-status-outdated = Update beschikbaar
lsp-status-running = Actief
lsp-status-failed = Mislukt

spaces-title = Werkruimten
spaces-new-placeholder = Naam nieuwe werkruimte
spaces-empty = Geen werkruimten
spaces-default-name = Werkruimte { $number }
spaces-tabs = { $count ->
    [one] 1 tabblad
   *[other] { $count } tabbladen
}
spaces-delete = Werkruimte verwijderen

team-title = Team
team-just-you = Alleen jij in deze werkruimte
team-agents = { $count ->
    [one] Jij en 1 agent
   *[other] Jij en { $count } agents
}
team-empty = Nog niemand hier
team-you = Jij
team-agent = Agent

services-title = Achtergrondservices
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } processen
}
services-kill-all = Alles stoppen
services-not-running = Service is niet actief
services-start-with = Starten met:
services-empty = Geen actieve processen
services-filter = Processen filteren…
services-no-match = Geen overeenkomende processen
services-connected = Verbonden
services-disconnected = Niet verbonden
services-attached = gekoppeld
services-kill = Stoppen
services-memory = Geheugen
services-size = Grootte
services-shell = Shell

error-title = Fout

history-search = Geschiedenis zoeken
history-clear-all = Alles wissen
history-clear-confirm = Alle geschiedenis wissen?
history-clear-warning = Dit kan niet ongedaan worden gemaakt.
history-cancel = Annuleren
history-today = Vandaag
history-yesterday = Gisteren
history-days-ago = { $count } dagen geleden
history-day-offset = Dag -{ $count }

settings-title = Instellingen
settings-loading = Instellingen laden…
settings-stored = Opgeslagen in ~/.vmux/settings.ron
settings-other = Overig
settings-software-update = Software-update
settings-check-updates = Controleren op updates
settings-check-updates-hint = Controleert automatisch bij het starten en elk uur wanneer automatische updates zijn ingeschakeld.
settings-update-unavailable = Niet beschikbaar
settings-update-unavailable-hint = De updater is niet opgenomen in deze build.
settings-update-checking = Controleren…
settings-update-checking-hint = Controleren op updates…
settings-update-check-again = Opnieuw controleren
settings-update-current = Vmux is up-to-date.
settings-update-downloading = Downloaden…
settings-update-downloading-hint = Vmux { $version } downloaden…
settings-update-installing = Installeren…
settings-update-installing-hint = Vmux { $version } installeren…
settings-update-ready = Update gereed
settings-update-ready-hint = Vmux { $version } staat klaar. Herstart om toe te passen.
settings-update-try-again = Opnieuw proberen
settings-update-failed = Kan niet controleren op updates.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Druk op een toets…
settings-saved = Opgeslagen
settings-record-key = Klik om een nieuwe toetscombinatie op te nemen

tray-open-window = Venster openen
tray-close-window = Venster sluiten
tray-pause-recording = Opname pauzeren
tray-resume-recording = Opname hervatten
tray-finish-recording = Opname afronden
tray-quit = Vmux afsluiten

composer-attach-files = Bestanden toevoegen (/upload)
composer-remove-attachment = Bijlage verwijderen

layout-back = Terug
layout-forward = Vooruit
layout-reload = Herladen
layout-bookmark-page = Bladwijzer voor deze pagina maken
layout-remove-bookmark = Bladwijzer verwijderen
layout-pin-page = Deze pagina vastzetten
layout-unpin-page = Deze pagina losmaken
layout-manage-extensions = Extensies beheren
layout-new-stack = Nieuwe stack
layout-close-tab = Tabblad sluiten
layout-bookmark = Bladwijzer
layout-pin = Vastzetten
layout-new-tab = Nieuw tabblad
layout-team = Team

command-switch-space = Van werkruimte wisselen…
command-search-ask = Zoeken of vragen…
command-new-tab-placeholder = Zoek of typ een URL, of selecteer Terminal…
command-placeholder = Typ een URL, zoek in tabbladen of gebruik > voor opdrachten…
command-composer-placeholder = Typ / voor opdrachten of @ voor media
command-send = Versturen (Enter)
command-terminal = Terminal
command-open-terminal = Openen in Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] 1 tabblad
   *[other] { $count } tabbladen
}
command-prompt = Prompt
command-new-tab = Nieuw tabblad
command-search = Zoeken
command-open-value = “{ $value }” openen
command-search-value = Zoeken naar “{ $value }”

schema-appearance = Weergave
schema-general = Algemeen
schema-layout = Indeling
schema-layout-detail = Venster, panelen, zijbalk en focusring.
schema-agent = Agent
schema-agent-detail = Agentgedrag en machtigingen voor tools.
schema-shortcuts = Sneltoetsen
schema-shortcuts-detail = Alleen-lezenweergave. Bewerk settings.ron rechtstreeks om bindingen te wijzigen.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Modus
schema-mode-detail = Kleurenschema voor webpagina's. Apparaat volgt je systeem.
schema-device = Apparaat
schema-light = Licht
schema-dark = Donker
schema-language = Taal
schema-language-detail = Gebruik systeem, en-US, ja of een BCP 47-tag met een overeenkomende ~/.vmux/locales/<tag>.ftl-catalogus.
schema-auto-update = Automatische updates
schema-auto-update-detail = Controleer op updates en installeer ze bij het starten en elk uur.
schema-startup-url = Opstart-URL
schema-startup-url-detail = Leeg opent de opdrachtbalkprompt.
schema-search-engine = Zoekmachine
schema-search-engine-detail = Gebruikt voor zoekopdrachten op het web vanuit Start en de opdrachtbalk.
schema-window = Venster
schema-pane = Paneel
schema-side-sheet = Zijpaneel
schema-focus-ring = Focusring
schema-run-placement = Plaatsing van runs overschrijven toestaan
schema-run-placement-detail = Laat agents de modus, richting en anker van het run-paneel kiezen.
schema-leader = Leader
schema-leader-detail = Prefix-toets voor chord-sneltoetsen.
schema-chord-timeout = Chord-time-out
schema-chord-timeout-detail = Milliseconden voordat een chord-prefix verloopt.
schema-bindings = Bindingen
schema-confirm-close = Sluiten bevestigen
schema-confirm-close-detail = Vragen vóór het sluiten van een terminal met een actief proces.
schema-default-theme = Standaardthema
schema-default-theme-detail = Naam van het actieve thema uit de themalijst.
