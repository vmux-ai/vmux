common-open = Offen
common-close = Schließen
common-install = Installieren
common-uninstall = Deinstallieren
common-update = Aktualisieren
common-retry = Versuchen Sie es noch einmal
common-refresh = Aktualisieren
common-remove = Entfernen
common-enable = Aktivieren
common-disable = Deaktivieren
common-new = Neu
common-active = aktiv
common-running = laufen
common-done = erledigt
common-failed = Fehlgeschlagen
common-installed = Installiert
common-items = { $count ->
    [one] { $count } Element
   *[other] { $count } Elemente
}
start-title = Starten
start-tagline = Eine Aufforderung. Alles klar, fertig.

agents-title = Agenten
agents-search = Durchsuchen Sie die Agenten ACP und CLI…
agents-empty = Keine passenden Agenten
agents-empty-detail = Versuchen Sie es mit einem Namen, einer Laufzeit oder ACP/CLI.
agents-install-failed = Die Installation ist fehlgeschlagen
agents-updating = Aktualisierung…
agents-retrying = Erneuter Versuch…
agents-preparing = Vorbereiten…

extensions-title = Erweiterungen
extensions-search = Suche installiert oder Chrome Web Store…
extensions-relaunch = Zur Bewerbung neu starten
extensions-empty = Keine Erweiterungen installiert
extensions-no-match = Keine passenden Erweiterungen
extensions-empty-detail = Suchen Sie oben nach Chrome Web Store und drücken Sie Return.
extensions-no-match-detail = Versuchen Sie es mit einem anderen Namen oder einer anderen Durchwahl-ID.
extensions-on = Auf
extensions-off = Aus
extensions-enable-confirm = { $name } aktivieren?
extensions-enable-permissions = Aktivieren Sie { $name } und erlauben Sie:

lsp-title = Sprachserver
lsp-search = Durchsuchen Sie Sprachserver, Linters, Formatter ...
lsp-loading = Katalog wird geladen…
lsp-empty = Keine passenden Sprachserver
lsp-empty-detail = Probieren Sie eine andere Sprache, einen anderen Linter oder einen anderen Formatierer aus.
lsp-needs = benötigt { $tool }
lsp-status-available = Verfügbar
lsp-status-on-path = Am PATH
lsp-status-installing = Installieren…
lsp-status-installed = Installiert
lsp-status-outdated = Update verfügbar
lsp-status-running = Laufen
lsp-status-failed = Fehlgeschlagen

spaces-title = Räume
spaces-new-placeholder = Neuer Raumname
spaces-empty = Keine Leerzeichen
spaces-default-name = Speicherplatz { $number }
spaces-tabs = { $count ->
    [one] 1 Registerkarte
   *[other] { $count } Registerkarten
}
spaces-delete = Leerzeichen löschen

team-title = Team
team-just-you = Nur du in diesem Raum
team-agents = { $count ->
    [one] Sie und 1 Agent
   *[other] Sie und { $count } Agenten
}
team-empty = Noch niemand hier
team-you = Du
team-agent = Agent

services-title = Hintergrunddienste
services-processes = { $count ->
    [one] 1 Prozess
   *[other] { $count } Prozesse
}
services-kill-all = Töte alle
services-not-running = Der Dienst läuft nicht
services-start-with = Beginnen Sie mit:
services-empty = Keine aktiven Prozesse
services-filter = Prozesse filtern…
services-no-match = Keine passenden Prozesse
services-connected = Verbunden
services-disconnected = Nicht verbunden
services-attached = beigefügt
services-kill = Töte
services-memory = Erinnerung
services-size = Größe
services-shell = Muschel

error-title = Fehler

history-search = Suchverlauf
history-clear-all = Alles löschen
history-clear-confirm = Gesamten Verlauf löschen?
history-clear-warning = Dies kann nicht rückgängig gemacht werden.
history-cancel = Abbrechen
history-today = Heute
history-yesterday = Gestern
history-days-ago = Vor { $count } Tagen
history-day-offset = Tag -{ $count }

settings-title = Einstellungen
settings-loading = Einstellungen werden geladen…
settings-stored = Gespeichert in ~/.vmux/settings.ron
settings-other = Andere
settings-software-update = Software-Update
settings-check-updates = Suchen Sie nach Updates
settings-check-updates-hint = Prüft automatisch beim Start und jede Stunde, wenn die automatische Aktualisierung aktiviert ist.
settings-update-unavailable = Nicht verfügbar
settings-update-unavailable-hint = Der Updater ist in diesem Build nicht enthalten.
settings-update-checking = Überprüfen…
settings-update-checking-hint = Suche nach Updates…
settings-update-check-again = Überprüfen Sie es erneut
settings-update-current = Vmux ist aktuell.
settings-update-downloading = Herunterladen…
settings-update-downloading-hint = Vmux { $version } wird heruntergeladen…
settings-update-installing = Installieren…
settings-update-installing-hint = Installation von Vmux { $version }…
settings-update-ready = Bereit für das Update
settings-update-ready-hint = Vmux { $version } ist bereit. Starten Sie neu, um es anzuwenden.
settings-update-try-again = Versuchen Sie es erneut
settings-update-failed = Es kann nicht nach Updates gesucht werden.
settings-item = Artikel
settings-item-number = Artikel { $number }
settings-press-key = Drücken Sie eine Taste…
settings-saved = Gespeichert
settings-record-key = Klicken Sie, um eine neue Tastenkombination aufzuzeichnen

tray-open-window = Fenster öffnen
tray-close-window = Fenster schließen
tray-pause-recording = Aufnahme pausieren
tray-resume-recording = Aufnahme fortsetzen
tray-finish-recording = Beenden Sie die Aufnahme
tray-quit = Beenden Sie Vmux

composer-attach-files = Dateien anhängen (/upload)
composer-remove-attachment = Anhang entfernen

layout-back = Zurück
layout-forward = Vorwärts
layout-reload = Neu laden
layout-bookmark-page = Setzen Sie ein Lesezeichen für diese Seite
layout-remove-bookmark = Lesezeichen entfernen
layout-pin-page = Pinne diese Seite
layout-unpin-page = Diese Seite lösen
layout-manage-extensions = Erweiterungen verwalten
layout-new-stack = Neuer Stapel
layout-close-tab = Tab schließen
layout-bookmark = Lesezeichen
layout-pin = Pin
layout-new-tab = Neuer Tab
layout-team = Team

command-switch-space = Leerzeichen wechseln…
command-search-ask = Suchen oder fragen Sie ...
command-new-tab-placeholder = Suchen oder geben Sie einen URL ein oder wählen Sie „Terminal“ aus.
command-placeholder = Geben Sie URL, Suchregisterkarten oder > für Befehle ein …
command-composer-placeholder = Geben Sie / für Befehle oder @ für Medien ein
command-send = Senden (Enter)
command-terminal = Terminal
command-open-terminal = Im Terminal öffnen
command-stack = Stapel
command-tabs = { $count ->
    [one] 1 Registerkarte
   *[other] { $count } Registerkarten
}
command-prompt = Prompt
command-new-tab = Neuer Tab
command-search = Suchen
command-open-value = Öffnen Sie „{ $value }“
command-search-value = Suchen Sie nach „{ $value }“

schema-appearance = Aussehen
schema-general = Allgemein
schema-layout = Layout
schema-layout-detail = Fenster, Fenster, Seitenleiste und Fokusring.
schema-agent = Agent
schema-agent-detail = Agentenverhalten und Toolberechtigungen.
schema-shortcuts = Verknüpfungen
schema-shortcuts-detail = Schreibgeschützte Ansicht. Bearbeiten Sie settings.ron direkt, um Bindungen zu ändern.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Modus
schema-mode-detail = Farbschema für Webseiten. Das Gerät folgt Ihrem System.
schema-device = Gerät
schema-light = Licht
schema-dark = Dunkel
schema-language = Sprache
schema-language-detail = Verwenden Sie system, en-US, ja oder ein beliebiges BCP 47-Tag mit einem passenden ~/.vmux/locales/<tag>.ftl-Katalog.
schema-auto-update = Automatische Aktualisierung
schema-auto-update-detail = Suchen Sie beim Start und stündlich nach Updates und installieren Sie diese.
schema-startup-url = Start URL
schema-startup-url-detail = „Leer“ öffnet die Eingabeaufforderung der Befehlsleiste.
schema-search-engine = Suchmaschine
schema-search-engine-detail = Wird für Websuchen über Start und die Befehlsleiste verwendet.
schema-window = Fenster
schema-pane = Bereich
schema-side-sheet = Seitenblech
schema-focus-ring = Fokusring
schema-run-placement = Überschreiben der Laufplatzierung zulassen
schema-run-placement-detail = Lassen Sie Agenten den Ausführungsbereichsmodus, die Richtung und den Anker auswählen.
schema-leader = Anführer
schema-leader-detail = Präfixtaste für Akkordkürzel.
schema-chord-timeout = Akkord-Timeout
schema-chord-timeout-detail = Millisekunden, bevor ein Akkordpräfix abläuft.
schema-bindings = Bindungen
schema-confirm-close = Schließen bestätigen
schema-confirm-close-detail = Eingabeaufforderung vor dem Schließen eines Terminals mit einem laufenden Prozess.
schema-default-theme = Standardthema
schema-default-theme-detail = Name des aktiven Themes aus der Themesliste.
