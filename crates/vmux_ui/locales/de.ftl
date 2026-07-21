locale-name = Deutsch
common-open = Öffnen
common-close = Schließen
common-install = Installieren
common-uninstall = Deinstallieren
common-update = Aktualisieren
common-retry = Erneut versuchen
common-refresh = Aktualisieren
common-remove = Entfernen
common-enable = Aktivieren
common-disable = Deaktivieren
common-new = Neu
common-active = aktiv
common-running = läuft
common-done = fertig
common-failed = Fehlgeschlagen
common-installed = Installiert
common-items = { $count ->
    [one] { $count } Element
   *[other] { $count } Elemente
}
start-title = Start
start-tagline = Ein Prompt. Alles erledigt.

agents-title = Agenten
agents-search = ACP- und CLI-Agenten suchen…
agents-empty = Keine passenden Agenten
agents-empty-detail = Versuchen Sie es mit Name, Laufzeit oder ACP/CLI.
agents-install-failed = Installation fehlgeschlagen
agents-updating = Wird aktualisiert…
agents-retrying = Wird erneut versucht…
agents-preparing = Wird vorbereitet…

extensions-title = Erweiterungen
extensions-search = Installierte Erweiterungen oder Chrome Web Store durchsuchen…
extensions-relaunch = Zum Anwenden neu starten
extensions-empty = Keine Erweiterungen installiert
extensions-no-match = Keine passenden Erweiterungen
extensions-empty-detail = Durchsuchen Sie oben den Chrome Web Store und drücken Sie die Eingabetaste.
extensions-no-match-detail = Versuchen Sie einen anderen Namen oder eine andere Erweiterungs-ID.
extensions-on = Ein
extensions-off = Aus
extensions-enable-confirm = { $name } aktivieren?
extensions-enable-permissions = { $name } aktivieren und erlauben:

lsp-title = Sprachserver
lsp-search = Sprachserver, Linter, Formatter suchen…
lsp-loading = Katalog wird geladen…
lsp-empty = Keine passenden Sprachserver
lsp-empty-detail = Versuchen Sie es mit einer anderen Sprache, einem Linter oder Formatter.
lsp-needs = benötigt { $tool }
lsp-status-available = Verfügbar
lsp-status-on-path = Im PATH
lsp-status-installing = Wird installiert…
lsp-status-installed = Installiert
lsp-status-outdated = Update verfügbar
lsp-status-running = Läuft
lsp-status-failed = Fehlgeschlagen

spaces-title = Spaces
spaces-new-placeholder = Name des neuen Space
spaces-empty = Keine Spaces
spaces-default-name = Space { $number }
spaces-tabs = { $count ->
    [one] 1 Tab
   *[other] { $count } Tabs
}
spaces-delete = Space löschen

team-title = Team
team-just-you = Nur Sie in diesem Space
team-agents = { $count ->
    [one] Sie und 1 Agent
   *[other] Sie und { $count } Agenten
}
team-empty = Noch niemand hier
team-you = Sie
team-agent = Agent

services-title = Hintergrunddienste
services-processes = { $count ->
    [one] 1 Prozess
   *[other] { $count } Prozesse
}
services-kill-all = Alle beenden
services-not-running = Dienst läuft nicht
services-start-with = Starten mit:
services-empty = Keine aktiven Prozesse
services-filter = Prozesse filtern…
services-no-match = Keine passenden Prozesse
services-connected = Verbunden
services-disconnected = Getrennt
services-attached = angehängt
services-kill = Beenden erzwingen
services-memory = Arbeitsspeicher
services-size = Größe
services-shell = Shell

error-title = Fehler

history-search = Verlauf durchsuchen
history-clear-all = Alles löschen
history-clear-confirm = Gesamten Verlauf löschen?
history-clear-warning = Dies kann nicht rückgängig gemacht werden.
history-cancel = Abbrechen
history-today = Heute
history-yesterday = Gestern
history-days-ago = vor { $count } Tagen
history-day-offset = Tag -{ $count }

settings-title = Einstellungen
settings-loading = Einstellungen werden geladen…
settings-stored = Gespeichert in ~/.vmux/settings.ron
settings-other = Sonstiges
settings-software-update = Softwareupdate
settings-check-updates = Nach Updates suchen
settings-check-updates-hint = Prüft beim Start und bei aktivierten automatischen Updates stündlich automatisch.
settings-update-unavailable = Nicht verfügbar
settings-update-unavailable-hint = Der Updater ist in diesem Build nicht enthalten.
settings-update-checking = Suche läuft…
settings-update-checking-hint = Es wird nach Updates gesucht…
settings-update-check-again = Erneut suchen
settings-update-current = Vmux ist auf dem neuesten Stand.
settings-update-downloading = Wird heruntergeladen…
settings-update-downloading-hint = Vmux { $version } wird heruntergeladen…
settings-update-installing = Wird installiert…
settings-update-installing-hint = Vmux { $version } wird installiert…
settings-update-ready = Update bereit
settings-update-ready-hint = Vmux { $version } ist bereit. Zum Anwenden neu starten.
settings-update-try-again = Erneut versuchen
settings-update-failed = Updates konnten nicht geprüft werden.
settings-item = Element
settings-item-number = Element { $number }
settings-press-key = Taste drücken…
settings-saved = Gespeichert
settings-record-key = Klicken, um eine neue Tastenkombination aufzunehmen

tray-open-window = Fenster öffnen
tray-close-window = Fenster schließen
tray-pause-recording = Aufnahme pausieren
tray-resume-recording = Aufnahme fortsetzen
tray-finish-recording = Aufnahme beenden
tray-quit = Vmux beenden

composer-attach-files = Dateien anhängen (/upload)
composer-remove-attachment = Anhang entfernen

layout-back = Zurück
layout-forward = Vorwärts
layout-reload = Neu laden
layout-bookmark-page = Diese Seite als Lesezeichen speichern
layout-remove-bookmark = Lesezeichen entfernen
layout-pin-page = Diese Seite anheften
layout-unpin-page = Diese Seite lösen
layout-manage-extensions = Erweiterungen verwalten
layout-new-stack = Neuer Stack
layout-close-tab = Tab schließen
layout-bookmark = Lesezeichen
layout-pin = Anheften
layout-new-tab = Neuer Tab
layout-team = Team

command-switch-space = Space wechseln…
command-search-ask = Suchen oder fragen…
command-new-tab-placeholder = Suchen oder URL eingeben oder Terminal auswählen…
command-placeholder = URL eingeben, Tabs suchen oder > für Befehle…
command-composer-placeholder = / für Befehle oder @ für Medien eingeben
command-send = Senden (Eingabetaste)
command-terminal = Terminal
command-open-terminal = Im Terminal öffnen
command-stack = Stack
command-tabs = { $count ->
    [one] 1 Tab
   *[other] { $count } Tabs
}
command-prompt = Prompt
command-new-tab = Neuer Tab
command-search = Suchen
command-open-value = „{ $value }“ öffnen
command-search-value = „{ $value }“ suchen

schema-appearance = Darstellung
schema-general = Allgemein
schema-layout = Layout
schema-layout-detail = Fenster, Bereiche, Seitenleiste und Fokusring.
schema-agent = Agent
schema-agent-detail = Verhalten des Agenten und Tool-Berechtigungen.
schema-shortcuts = Tastenkürzel
schema-shortcuts-detail = Schreibgeschützte Ansicht. Bearbeiten Sie settings.ron direkt, um Belegungen zu ändern.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Modus
schema-mode-detail = Farbschema für Webseiten. „Gerät“ folgt Ihrem System.
schema-device = Gerät
schema-light = Hell
schema-dark = Dunkel
schema-language = Sprache
schema-language-detail = System verwenden, en-US, ja oder ein beliebiges BCP 47-Tag mit passendem ~/.vmux/locales/<tag>.ftl-Katalog.
schema-auto-update = Automatische Updates
schema-auto-update-detail = Beim Start und stündlich nach Updates suchen und sie installieren.
schema-startup-url = Start-URL
schema-startup-url-detail = Leer öffnet die Eingabeaufforderung der Befehlsleiste.
schema-search-engine = Suchmaschine
schema-search-engine-detail = Wird für Websuchen über Start und die Befehlsleiste verwendet.
schema-window = Fenster
schema-pane = Bereich
schema-side-sheet = Seitenpanel
schema-focus-ring = Fokusring
schema-run-placement = Platzierung von Ausführungen überschreiben erlauben
schema-run-placement-detail = Agenten dürfen Bereichsmodus, Richtung und Anker für Ausführungen wählen.
schema-leader = Leader
schema-leader-detail = Präfixtaste für Akkord-Tastenkürzel.
schema-chord-timeout = Akkord-Timeout
schema-chord-timeout-detail = Millisekunden, bevor ein Akkord-Präfix abläuft.
schema-bindings = Belegungen
schema-confirm-close = Schließen bestätigen
schema-confirm-close-detail = Vor dem Schließen eines Terminals mit laufendem Prozess nachfragen.
schema-default-theme = Standardtheme
schema-default-theme-detail = Name des aktiven Themes aus der Theme-Liste.
