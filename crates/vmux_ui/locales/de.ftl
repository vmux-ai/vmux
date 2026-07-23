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

tools-title = Werkzeuge
tools-search = Pakete, Agenten, MCP, Sprachwerkzeuge und Konfigurationsdateien durchsuchen…
tools-open = Werkzeuge öffnen
tools-fold = Werkzeuge einklappen
tools-unfold = Werkzeuge ausklappen
tools-scanning = Lokale Werkzeuge werden durchsucht…
tools-no-installed = Keine Werkzeuge installiert
tools-empty = Keine passenden Werkzeuge
tools-empty-detail = Installieren Sie ein Paket oder fügen Sie ein Konfigurationsdateipaket im Stow-Stil hinzu.
tools-apply = Anwenden
tools-homebrew = Homebrew
tools-homebrew-sync = Installierte Formeln und Anwendungen werden automatisch synchronisiert.
tools-open-brewfile = Brewfile öffnen
tools-managed = verwaltet
tools-provider-homebrew-formulae = Homebrew-Formeln
tools-provider-homebrew-casks = Homebrew-Anwendungen
tools-provider-npm = npm-Pakete
tools-provider-acp-agents = ACP-Agenten
tools-provider-language-tools = Sprachwerkzeuge
tools-provider-mcp-servers = MCP-Server
tools-provider-dotfiles = Konfigurationsdateien
tools-status-available = Verfügbar
tools-status-missing = Fehlt
tools-status-conflict = Konflikt
tools-forget = Vergessen
tools-manage = Verwalten
tools-link = Verknüpfen
tools-unlink = Verknüpfung lösen
tools-import = Importieren
tools-update-count = { $count ->
    [one] 1 Aktualisierung
   *[other] { $count } Aktualisierungen
}
tools-conflict-count = { $count ->
    [one] 1 Konflikt
   *[other] { $count } Konflikte
}
tools-result-applied = Werkzeuge angewendet
tools-result-imported = Werkzeuge importiert
tools-result-installed = { $name } installiert
tools-result-updated = { $name } aktualisiert
tools-result-uninstalled = { $name } deinstalliert
tools-result-forgotten = { $name } vergessen
tools-result-managed = { $name } wird jetzt verwaltet
tools-result-linked = { $name } verknüpft
tools-result-unlinked = Verknüpfung mit { $name } gelöst
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Synchronisieren Sie Einstellungen, Tools, Dotfiles und Wissen mit Git.
vault-sync = Synchronisieren
vault-create = Erstellen
vault-connect = Verbinden
vault-private = Privates Repository
vault-public-warning = Öffentliche Repositorys legen Ihr Wissen und Ihre Konfiguration offen.
vault-choose-repository = Wählen Sie ein Repository…
vault-empty = leer
vault-clean = Auf dem neuesten Stand
vault-not-connected = Nicht verbunden
vault-change-count = Änderungen: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

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

settings-empty = (leer)
settings-none = (keine)

schema-system = System
schema-editor = Editor
schema-recording = Aufzeichnung
schema-radius = Radius
schema-padding = Innenabstand
schema-gap = Abstand
schema-width = Breite
schema-color = Farbe
schema-red = Rot
schema-green = Grün
schema-blue = Blau
schema-follow-files = Dateien folgen
schema-tidy-files = Dateien aufräumen
schema-tidy-files-max = Schwellenwert für Datei-Aufräumen
schema-tidy-files-auto = Dateien automatisch aufräumen
schema-app-providers = App-Anbieter
schema-provider = Anbieter
schema-kind = Art
schema-models = Modelle
schema-acp = ACP-Agenten
schema-id = ID
schema-name = Name
schema-command = Befehl
schema-arguments = Argumente
schema-environment = Umgebung
schema-working-directory = Arbeitsverzeichnis
schema-shell = Shell
schema-font-family = Schriftfamilie
schema-startup-directory = Startverzeichnis
schema-themes = Designs
schema-color-scheme = Farbschema
schema-font-size = Schriftgröße
schema-line-height = Zeilenhöhe
schema-cursor-style = Cursorstil
schema-cursor-blink = Cursorblinken
schema-custom-themes = Eigene Designs
schema-foreground = Vordergrund
schema-background = Hintergrund
schema-cursor = Cursor
schema-ansi-colors = ANSI-Farben
schema-keymap = Tastaturbelegung
schema-explorer = Explorer
schema-visible = Sichtbar
schema-language-servers = Sprachserver
schema-servers = Server
schema-language-id = Sprach-ID
schema-root-markers = Root-Markierungen
schema-output-directory = Ausgabeverzeichnis

menu-scene = Szene
menu-layout = Layout
menu-terminal = Terminal
menu-browser = Browser
menu-service = Dienst
menu-bookmark = Lesezeichen
menu-edit = Bearbeiten

layout-knowledge = Wissen
layout-open-knowledge = Wissen öffnen
layout-open-welcome-knowledge = Willkommen im Wissen öffnen
layout-open-path = { $path } öffnen
layout-fold-knowledge = Wissen einklappen
layout-unfold-knowledge = Wissen ausklappen
layout-bookmarks = Lesezeichen
layout-new-folder = Neuer Ordner
layout-add-to-bookmarks = Zu Lesezeichen hinzufügen
layout-move-to-bookmarks = In Lesezeichen verschieben
layout-stack-number = Stack { $number }
layout-fold-stack = Stack einklappen
layout-unfold-stack = Stack ausklappen
layout-close-stack = Stack schließen
layout-bookmark-in = Lesezeichen in { $folder }

common-cancel = Abbrechen
common-delete = Löschen
common-save = Sichern
common-rename = Umbenennen
common-expand = Aufklappen
common-collapse = Zuklappen
common-loading = Lädt…
common-error = Fehler
common-output = Ausgabe
common-pending = Ausstehend
common-current = aktuell
common-stop = Stoppen
services-command = Vmux-Dienst
services-uptime-seconds = { $seconds } s
services-uptime-minutes = { $minutes } min { $seconds } s
services-uptime-hours = { $hours } h { $minutes } min
services-uptime-days = { $days } d { $hours } h

error-page-failed-load = Seite konnte nicht geladen werden
error-page-not-found = Seite nicht gefunden
error-unknown-host = Unbekannter Vmux-App-Host: { $host }

history-title = Verlauf

command-new-app-chat = Neuer Chat mit { $provider }/{ $model } (App)
command-interactive-mode-user = Szene > Interaktiver Modus > Benutzer
command-interactive-mode-player = Szene > Interaktiver Modus > Player
command-minimize-window = Layout > Fenster > Minimieren
command-toggle-layout = Layout > Layout > Layout umschalten
command-close-tab = Layout > Tab > Tab schließen
command-new-task = Layout > Tab > Neue Aufgabe…
command-next-tab = Layout > Tab > Nächster Tab
command-prev-tab = Layout > Tab > Vorheriger Tab
command-rename-tab = Layout > Tab > Tab umbenennen
command-tab-select-1 = Layout > Tab > Tab 1 auswählen
command-tab-select-2 = Layout > Tab > Tab 2 auswählen
command-tab-select-3 = Layout > Tab > Tab 3 auswählen
command-tab-select-4 = Layout > Tab > Tab 4 auswählen
command-tab-select-5 = Layout > Tab > Tab 5 auswählen
command-tab-select-6 = Layout > Tab > Tab 6 auswählen
command-tab-select-7 = Layout > Tab > Tab 7 auswählen
command-tab-select-8 = Layout > Tab > Tab 8 auswählen
command-tab-select-last = Layout > Tab > Letzten Tab auswählen
command-close-pane = Layout > Bereich > Bereich schließen
command-select-pane-left = Layout > Bereich > Linken Bereich auswählen
command-select-pane-right = Layout > Bereich > Rechten Bereich auswählen
command-select-pane-up = Layout > Bereich > Oberen Bereich auswählen
command-select-pane-down = Layout > Bereich > Unteren Bereich auswählen
command-swap-pane-prev = Layout > Bereich > Bereich mit vorherigem tauschen
command-swap-pane-next = Layout > Bereich > Bereich mit nächstem tauschen
command-equalize-pane-size = Layout > Bereich > Bereichsgrößen angleichen
command-resize-pane-left = Layout > Bereich > Bereich nach links vergrößern
command-resize-pane-right = Layout > Bereich > Bereich nach rechts vergrößern
command-resize-pane-up = Layout > Bereich > Bereich nach oben vergrößern
command-resize-pane-down = Layout > Bereich > Bereich nach unten vergrößern
command-stack-close = Layout > Stack > Stack schließen
command-stack-next = Layout > Stack > Nächster Stack
command-stack-previous = Layout > Stack > Vorheriger Stack
command-stack-reopen = Layout > Stack > Geschlossene Seite erneut öffnen
command-stack-swap-prev = Layout > Stack > Stack nach links bewegen
command-stack-swap-next = Layout > Stack > Stack nach rechts bewegen
command-space-open = Layout > Space > Spaces
command-terminal-close = Terminal > Terminal schließen
command-terminal-next = Terminal > Nächstes Terminal
command-terminal-prev = Terminal > Vorheriges Terminal
command-terminal-clear = Terminal > Terminal leeren
command-browser-prev-page = Browser > Navigation > Zurück
command-browser-next-page = Browser > Navigation > Vorwärts
command-browser-reload = Browser > Navigation > Neu laden
command-browser-hard-reload = Browser > Navigation > Ohne Cache neu laden
command-open-in-place = Browser > Öffnen > Hier öffnen
command-open-in-new-stack = Browser > Öffnen > In neuem Stack öffnen
command-open-in-pane-top = Browser > Öffnen > Im Bereich darüber öffnen
command-open-in-pane-right = Browser > Öffnen > Im rechten Bereich öffnen
command-open-in-pane-bottom = Browser > Öffnen > Im Bereich darunter öffnen
command-open-in-pane-left = Browser > Öffnen > Im linken Bereich öffnen
command-open-in-new-tab = Browser > Öffnen > In neuem Tab öffnen
command-open-in-new-space = Browser > Öffnen > In neuem Space öffnen
command-browser-zoom-in = Browser > Darstellung > Vergrößern
command-browser-zoom-out = Browser > Darstellung > Verkleinern
command-browser-zoom-reset = Browser > Darstellung > Originalgröße
command-browser-dev-tools = Browser > Darstellung > Entwicklertools
command-browser-open-command-bar = Browser > Leiste > Befehlsleiste
command-browser-open-page-in-command-bar = Browser > Leiste > Seite bearbeiten
command-browser-open-path-bar = Browser > Leiste > Pfadnavigator
command-browser-open-commands = Browser > Leiste > Befehle
command-browser-open-history = Browser > Leiste > Verlauf
command-service-open = Dienst > Dienstmonitor öffnen
command-bookmark-toggle-active = Lesezeichen > Seite als Lesezeichen sichern
command-bookmark-pin-active = Lesezeichen > Seite anheften

layout-tab = Tab
layout-no-stacks = Keine Stacks
layout-loading = Lädt…
layout-no-markdown-files = Keine Markdown-Dateien
layout-empty-folder = Leerer Ordner
layout-worktree = Worktree
layout-folder-name = Ordnername
layout-no-pins-bookmarks = Keine angehefteten Seiten oder Lesezeichen
layout-move-to = Nach { $folder } bewegen
layout-bookmark-current-page = Aktuelle Seite als Lesezeichen sichern
layout-rename-folder = Ordner umbenennen
layout-remove-folder = Ordner entfernen
layout-update-downloading = Update wird geladen
layout-update-installing = Update wird installiert…
layout-update-ready = Neue Version verfügbar
layout-restart-update = Zum Aktualisieren neu starten

agent-preparing = Agent wird vorbereitet…
agent-send-all-queued = Alle wartenden Prompts jetzt senden (Esc)
agent-send = Senden (Enter)
agent-ready = Bereit, wenn du es bist.
agent-loading-older = Ältere Nachrichten werden geladen…
agent-load-older = Ältere Nachrichten laden
agent-continued-from = Fortgesetzt von { $source }
agent-older-context-omitted = älterer Kontext ausgelassen
agent-interrupted = unterbrochen
agent-allow-tool = { $tool } erlauben?
agent-deny = Ablehnen
agent-allow-always = Immer erlauben
agent-allow = Erlauben
agent-loading-sessions = Sitzungen werden geladen…
agent-no-resumable-sessions = Keine fortsetzbaren Sitzungen gefunden
agent-no-matching-sessions = Keine passenden Sitzungen
agent-no-matching-models = Keine passenden Modelle
agent-choice-help = ↑/↓ oder Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Repository-Ordner auswählen
agent-choose-repository-detail = Wähle das lokale Git-Repository aus, das der Agent verwenden soll.
agent-choosing = Wird ausgewählt…
agent-choose-folder = Ordner auswählen
agent-queued = in der Warteschlange
agent-attached = Angehängt:
agent-cancel-queued = Wartenden Prompt abbrechen
agent-resume-queued = Wartende Prompts fortsetzen
agent-clear-queue = Warteschlange leeren
agent-send-all-now = alle jetzt senden
agent-choose-option = Wähle oben eine Option
agent-loading-media = Medien werden geladen…
agent-no-matching-media = Keine passenden Medien
agent-prompt-context = Prompt-Kontext
agent-details = Details
agent-path = Pfad
agent-tool = Tool
agent-server = Server
agent-bytes = { $count } Byte
agent-worked-for = { $duration } gearbeitet
agent-worked-for-steps = { $count ->
    [one] { $duration } gearbeitet · 1 Schritt
   *[other] { $duration } gearbeitet · { $count } Schritte
}
agent-tool-guardian-review = Guardian-Prüfung
agent-tool-read-files = Dateien gelesen
agent-tool-viewed-image = Bild angesehen
agent-tool-used-browser = Browser verwendet
agent-tool-searched-files = Dateien durchsucht
agent-tool-ran-commands = Befehle ausgeführt
agent-thinking = Denkt nach
agent-subagent = Subagent
agent-prompt = Prompt
agent-thread = Thread
agent-parent = Übergeordnet
agent-children = Untergeordnet
agent-call = Aufruf
agent-raw-event = Rohereignis
agent-plan = Plan
agent-tasks = { $count ->
    [one] 1 Aufgabe
   *[other] { $count } Aufgaben
}
agent-edited = Bearbeitet
agent-reconnecting = Verbindung wird wiederhergestellt { $attempt }/{ $total }
agent-status-running = Läuft
agent-status-done = Fertig
agent-status-failed = Fehlgeschlagen
agent-status-pending = Ausstehend
agent-slash-attach-files = Dateien anhängen
agent-slash-resume-session = Frühere Sitzung fortsetzen
agent-slash-select-model = Modell auswählen
agent-slash-continue-cli = Diese Sitzung in der CLI fortsetzen
agent-session-just-now = gerade eben
agent-session-minutes-ago = vor { $count } min
agent-session-hours-ago = vor { $count } h
agent-session-days-ago = vor { $count } d
agent-working-working = Arbeitet
agent-working-thinking = Denkt nach
agent-working-pondering = Überlegt
agent-working-noodling = Tüftelt
agent-working-percolating = Brütet
agent-working-conjuring = Zaubert
agent-working-cooking = Kocht
agent-working-brewing = Braut
agent-working-musing = Sinniert
agent-working-ruminating = Grübelt
agent-working-scheming = Plant
agent-working-synthesizing = Synthetisiert
agent-working-tinkering = Bastelt
agent-working-churning = Wühlt
agent-working-vibing = Vibt
agent-working-simmering = Köchelt
agent-working-crafting = Formt
agent-working-divining = Ergründet
agent-working-mulling = Wälzt
agent-working-spelunking = Gräbt sich durch

editor-toggle-explorer = Explorer ein-/ausblenden (Cmd+B)
editor-unsaved = ungesichert
editor-rendered-markdown = Gerendertes Markdown mit Live-Bearbeitung
editor-note = Notiz
editor-source-editor = Quelltext-Editor
editor-editor = Editor
editor-git-diff = Git-Diff
editor-diff = Diff
editor-tidy = Aufräumen
editor-always = Immer
editor-unchanged-previews = { $count ->
    [one] ✦ 1 unveränderte Vorschau
   *[other] ✦ { $count } unveränderte Vorschauen
}
editor-open-externally = Extern öffnen
editor-changed-line = Geänderte Zeile
editor-go-to-definition = Zur Definition
editor-find-references = Referenzen suchen
editor-references = { $count ->
    [one] 1 Referenz
   *[other] { $count } Referenzen
}
editor-lsp-starting = { $server } startet…
editor-lsp-not-installed = { $server } — nicht installiert
editor-explorer = Explorer
editor-open-editors = Geöffnete Editoren
editor-outline = Gliederung
editor-new-file = Neue Datei
editor-new-folder = Neuer Ordner
editor-delete-confirm = „{ $name }“ löschen? Dies kann nicht rückgängig gemacht werden.
editor-created-folder = Ordner { $name } erstellt
editor-created-file = Datei { $name } erstellt
editor-renamed-to = Umbenannt in { $name }
editor-deleted = { $name } gelöscht
editor-failed-decode-image = Bild konnte nicht dekodiert werden
editor-preview-large-image = Bild (zu groß für die Vorschau)
editor-preview-binary = Binärdatei
editor-preview-file = Datei

git-status-clean = unverändert
git-status-modified = geändert
git-status-staged = gestaged
git-status-staged-modified = gestaged*
git-status-untracked = nicht verfolgt
git-status-deleted = gelöscht
git-status-conflict = Konflikt
git-accept-all = ✓ alle übernehmen
git-unstage = Aus Stage entfernen
git-confirm-deny-all = Alle Ablehnungen bestätigen
git-deny-all = ✗ alle ablehnen
git-commit-message = Commit-Nachricht
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Diff wird geladen…
git-no-changes = Keine Änderungen anzuzeigen
git-accept = ✓ übernehmen
git-deny = ✗ ablehnen
git-show-unchanged-lines = { $count } unveränderte Zeilen anzeigen

terminal-loading = Lädt…
terminal-runs-when-ready = startet, wenn bereit · Ctrl+C leert · Esc überspringt
terminal-booting = startet
terminal-type-command = Befehl eingeben · startet, wenn bereit · Esc überspringt

setup-tagline-claude = Anthropic-Coding-Agent, in Vmux
setup-tagline-codex = OpenAI-Coding-Agent, in Vmux
setup-tagline-vibe = Mistral-Coding-Agent, in Vmux
setup-install-title = { $name }-CLI installieren
setup-homebrew-required = Homebrew wird benötigt, um { $command } zu installieren, und ist noch nicht eingerichtet. Vmux installiert zuerst Homebrew und dann { $name }.
setup-terminal-instructions = Drücke im Terminal die Eingabetaste, um zu starten, und gib anschließend dein Mac-Passwort ein, wenn du dazu aufgefordert wirst.
setup-command-missing = Vmux hat diese Seite geöffnet, weil der lokale Befehl { $command } noch nicht installiert ist. Führe den folgenden Befehl aus, um ihn zu installieren.
setup-install-failed = Installation wurde nicht abgeschlossen. Prüfe das Terminal auf Details und versuche es erneut.
setup-installing = Wird installiert…
setup-install-homebrew = Homebrew + { $name } installieren
setup-run-install = Installationsbefehl ausführen
setup-auto-reload = Vmux führt ihn in einem Terminal aus und lädt neu, sobald { $command } bereit ist.

debug-title = Debug
debug-auto-update = Automatisch aktualisieren
debug-simulate-update = Verfügbares Update simulieren
debug-simulate-download = Download simulieren
debug-clear-update = Update zurücksetzen
debug-trigger-restart = Neustart auslösen

command-manage-spaces = Spaces verwalten…
command-pane-stack-location = Bereich { $pane } / Stapel { $stack }
command-space-pane-stack-location = { $space } / Bereich { $pane } / Stapel { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Interaktiver Modus
command-group-window = Fenster
command-group-tab = Tab
command-group-pane = Bereich
command-group-stack = Stapel
command-group-space = Space
command-group-navigation = Navigation
command-group-open = Öffnen
command-group-view = Ansicht
command-group-bar = Leiste

menu-close-vmux = Vmux schließen

agents-terminal-coding-agent = Terminalbasierter Coding-Agent
