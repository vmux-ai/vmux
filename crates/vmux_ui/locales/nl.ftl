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
common-running = rennen
common-done = gedaan
common-failed = Mislukt
common-installed = Geïnstalleerd
common-items = { $count ->
    [one] { $count } artikel
   *[other] { $count } artikelen
}
start-title = Begin
start-tagline = Eén prompt. Alles, klaar.

agents-title = Agenten
agents-search = Zoek ACP en CLI agenten…
agents-empty = Geen overeenkomende agenten
agents-empty-detail = Probeer een naam, runtime of ACP/CLI.
agents-install-failed = Installatie mislukt
agents-updating = Updaten…
agents-retrying = Opnieuw proberen…
agents-preparing = Voorbereiden…

extensions-title = Extensies
extensions-search = Zoek geïnstalleerd of Chrome Web Store…
extensions-relaunch = Opnieuw starten om te solliciteren
extensions-empty = Geen extensies geïnstalleerd
extensions-no-match = Geen overeenkomende extensies
extensions-empty-detail = Zoek de Chrome Web Store hierboven en druk op Return.
extensions-no-match-detail = Probeer een andere naam of extensie-ID.
extensions-on = Aan
extensions-off = Uit
extensions-enable-confirm = { $name } inschakelen?
extensions-enable-permissions = Schakel { $name } in en sta het volgende toe:

lsp-title = Taalservers
lsp-search = Zoek taalservers, linters, formatters…
lsp-loading = Catalogus laden…
lsp-empty = Geen overeenkomende taalservers
lsp-empty-detail = Probeer een andere taal, linter of formatter.
lsp-needs = heeft { $tool } nodig
lsp-status-available = Beschikbaar
lsp-status-on-path = Op PATH
lsp-status-installing = Installeren…
lsp-status-installed = Geïnstalleerd
lsp-status-outdated = Update beschikbaar
lsp-status-running = Rennen
lsp-status-failed = Mislukt

spaces-title = Spaties
spaces-new-placeholder = Nieuwe ruimtenaam
spaces-empty = Geen spaties
spaces-default-name = Spatie { $number }
spaces-tabs = { $count ->
    [one] 1 tabblad
   *[other] { $count } tabbladen
}
spaces-delete = Ruimte verwijderen

team-title = Team
team-just-you = Alleen jij in deze ruimte
team-agents = { $count ->
    [one] Jij en 1 agent
   *[other] Jij en { $count } agenten
}
team-empty = Nog niemand hier
team-you = Jij
team-agent = Agent

services-title = Achtergronddiensten
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } processen
}
services-kill-all = Dood allemaal
services-not-running = Service is niet actief
services-start-with = Begin met:
services-empty = Geen actieve processen
services-filter = Processen filteren...
services-no-match = Geen matchingprocessen
services-connected = Verbonden
services-disconnected = Verbinding verbroken
services-attached = bijgevoegd
services-kill = Dood
services-memory = Geheugen
services-size = Grootte
services-shell = Schelp

error-title = Fout

history-search = Zoekgeschiedenis
history-clear-all = Alles wissen
history-clear-confirm = Alle geschiedenis wissen?
history-clear-warning = Dit kan niet ongedaan worden gemaakt.
history-cancel = Annuleer
history-today = Vandaag
history-yesterday = Gisteren
history-days-ago = { $count } dagen geleden
history-day-offset = Dag -{ $count }

settings-title = Instellingen
settings-loading = Instellingen laden…
settings-stored = Opgeslagen in ~/.vmux/settings.ron
settings-other = Anders
settings-software-update = Software-update
settings-check-updates = Controleer op updates
settings-check-updates-hint = Controleert automatisch bij het opstarten en elk uur wanneer automatische update is ingeschakeld.
settings-update-unavailable = Niet beschikbaar
settings-update-unavailable-hint = Updater is niet opgenomen in deze build.
settings-update-checking = Controleren…
settings-update-checking-hint = Controleren op updates…
settings-update-check-again = Controleer opnieuw
settings-update-current = Vmux is up-to-date.
settings-update-downloading = Downloaden…
settings-update-downloading-hint = Vmux { $version } downloaden…
settings-update-installing = Installeren…
settings-update-installing-hint = Vmux { $version } installeren…
settings-update-ready = Update gereed
settings-update-ready-hint = Vmux { $version } is klaar. Start opnieuw om het toe te passen.
settings-update-try-again = Probeer het opnieuw
settings-update-failed = Kan niet controleren op updates.
settings-item = Artikel
settings-item-number = Artikel { $number }
settings-press-key = Druk op een toets…
settings-saved = Opgeslagen
settings-record-key = Klik om een nieuwe toetscombinatie op te nemen

tray-open-window = Open raam
tray-close-window = Sluit venster
tray-pause-recording = Pauzeer de opname
tray-resume-recording = Hervat de opname
tray-finish-recording = Beëindig de opname
tray-quit = Sluit Vmux af

composer-attach-files = Bestanden bijvoegen (/upload)
composer-remove-attachment = Bijlage verwijderen

layout-back = Terug
layout-forward = Vooruit
layout-reload = Herladen
layout-bookmark-page = Maak een bladwijzer van deze pagina
layout-remove-bookmark = Bladwijzer verwijderen
layout-pin-page = Zet deze pagina vast
layout-unpin-page = Maak deze pagina los
layout-manage-extensions = Beheer extensies
layout-new-stack = Nieuwe stapel
layout-close-tab = Tabblad sluiten
layout-bookmark = Bladwijzer
layout-pin = Vastzetten
layout-new-tab = Nieuw tabblad
layout-team = Team

command-switch-space = Wissel ruimte…
command-search-ask = Zoek of vraag…
command-new-tab-placeholder = Zoek of typ een URL, of selecteer Terminal…
command-placeholder = Typ een URL, zoektabbladen of > voor opdrachten...
command-composer-placeholder = Typ / voor opdrachten of @ voor media
command-send = Verzenden (Enter)
command-terminal = Terminal
command-open-terminal = Openen in Terminal
command-stack = Stapel
command-tabs = { $count ->
    [one] 1 tabblad
   *[other] { $count } tabbladen
}
command-prompt = Prompt
command-new-tab = Nieuw tabblad
command-search = Zoeken
command-open-value = Open “{ $value }”
command-search-value = Zoek naar “{ $value }”

schema-appearance = Uiterlijk
schema-general = Algemeen
schema-layout = Indeling
schema-layout-detail = Venster, ruiten, zijbalk en scherpstelring.
schema-agent = Agent
schema-agent-detail = Agentgedrag en toolmachtigingen.
schema-shortcuts = Snelkoppelingen
schema-shortcuts-detail = Alleen-lezen weergave. Bewerk settings.ron rechtstreeks om bindingen te wijzigen.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Modus
schema-mode-detail = Kleurenschema voor webpagina's. Apparaat volgt uw systeem.
schema-device = Apparaat
schema-light = Licht
schema-dark = Donker
schema-language = Taal
schema-language-detail = Gebruik systeem-, en-US-, ja- of een andere BCP 47-tag met een overeenkomende ~/.vmux/locales/<tag>.ftl-catalogus.
schema-auto-update = Automatische update
schema-auto-update-detail = Controleer op updates en installeer deze bij de lancering en elk uur.
schema-startup-url = Opstarten URL
schema-startup-url-detail = Leeg opent de opdrachtbalkprompt.
schema-search-engine = Zoekmachine
schema-search-engine-detail = Wordt gebruikt voor zoekopdrachten op internet vanuit Start en de opdrachtbalk.
schema-window = Venster
schema-pane = Paneel
schema-side-sheet = Zijblad
schema-focus-ring = Focusring
schema-run-placement = Overschrijven van runplaatsing toestaan
schema-run-placement-detail = Laat agenten de uitvoeringsvenstermodus, -richting en -anker kiezen.
schema-leader = Leider
schema-leader-detail = Prefix-toets voor akkoordsnelkoppelingen.
schema-chord-timeout = Time-out van akkoord
schema-chord-timeout-detail = Milliseconden voordat een akkoordvoorvoegsel vervalt.
schema-bindings = Bindingen
schema-confirm-close = Bevestig het sluiten
schema-confirm-close-detail = Prompt voordat een terminal met een lopend proces wordt gesloten.
schema-default-theme = Standaardthema
schema-default-theme-detail = Naam van het actieve thema uit de themalijst.
