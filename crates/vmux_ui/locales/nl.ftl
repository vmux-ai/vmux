locale-name = Nederlands
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

tools-title = Hulpmiddelen
tools-search = Zoeken naar pakketten, agenten, MCP, taalhulpmiddelen en configuratiebestanden…
tools-open = Hulpmiddelen openen
tools-fold = Hulpmiddelen invouwen
tools-unfold = Hulpmiddelen uitvouwen
tools-scanning = Lokale hulpmiddelen scannen…
tools-no-installed = Geen hulpmiddelen geïnstalleerd
tools-empty = Geen overeenkomende hulpmiddelen
tools-empty-detail = Installeer een pakket of voeg een configuratiebestandenpakket in Stow-stijl toe.
tools-apply = Toepassen
tools-homebrew = Homebrew
tools-homebrew-sync = Geïnstalleerde formules en toepassingen worden automatisch gesynchroniseerd.
tools-open-brewfile = Brewfile openen
tools-managed = beheerd
tools-provider-homebrew-formulae = Homebrew-formules
tools-provider-homebrew-casks = Homebrew-toepassingen
tools-provider-npm = npm-pakketten
tools-provider-acp-agents = ACP-agenten
tools-provider-language-tools = Taalhulpmiddelen
tools-provider-mcp-servers = MCP-servers
tools-provider-dotfiles = Configuratiebestanden
tools-status-available = Beschikbaar
tools-status-missing = Ontbreekt
tools-status-conflict = Tegenstrijdigheid
tools-forget = Vergeten
tools-manage = Beheren
tools-link = Koppelen
tools-unlink = Ontkoppelen
tools-import = Importeren
tools-update-count = { $count ->
    [one] 1 update
   *[other] { $count } updates
}
tools-conflict-count = { $count ->
    [one] 1 conflict
   *[other] { $count } conflicten
}
tools-result-applied = Hulpmiddelen toegepast
tools-result-imported = Hulpmiddelen geïmporteerd
tools-result-installed = { $name } geïnstalleerd
tools-result-updated = { $name } bijgewerkt
tools-result-uninstalled = { $name } verwijderd
tools-result-forgotten = { $name } vergeten
tools-result-managed = { $name } wordt nu beheerd
tools-result-linked = { $name } gekoppeld
tools-result-unlinked = { $name } ontkoppeld
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Synchroniseer instellingen, tools, dotfiles en Knowledge met Git.
vault-sync = Synchroniseren
vault-create = Creëren
vault-connect = Verbinden
vault-private = Privé-opslagplaats
vault-public-warning = Openbare repository's stellen uw kennis en configuratie bloot.
vault-choose-repository = Kies een opslagplaats…
vault-empty = leeg
vault-clean = Up-to-date
vault-not-connected = Niet verbonden
vault-change-count = Veranderingen: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

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

settings-empty = (leeg)
settings-none = (geen)

schema-system = Systeem
schema-editor = Editor
schema-recording = Opname
schema-radius = Straal
schema-padding = Opvulling
schema-gap = Tussenruimte
schema-width = Breedte
schema-color = Kleur
schema-red = Rood
schema-green = Groen
schema-blue = Blauw
schema-follow-files = Bestanden volgen
schema-tidy-files = Bestanden opruimen
schema-tidy-files-max = Opruimdrempel voor bestanden
schema-tidy-files-auto = Bestanden automatisch opruimen
schema-app-providers = App-aanbieders
schema-provider = Aanbieder
schema-kind = Type
schema-models = Modellen
schema-acp = ACP-agenten
schema-id = ID
schema-name = Naam
schema-command = Opdracht
schema-arguments = Argumenten
schema-environment = Omgevingsvariabelen
schema-working-directory = Werkmap
schema-shell = Shell
schema-font-family = Lettertypefamilie
schema-startup-directory = Startmap
schema-themes = Thema's
schema-color-scheme = Kleurenschema
schema-font-size = Lettergrootte
schema-line-height = Regelhoogte
schema-cursor-style = Cursorstijl
schema-cursor-blink = Knipperende cursor
schema-custom-themes = Aangepaste thema's
schema-foreground = Voorgrond
schema-background = Achtergrond
schema-cursor = Cursor
schema-ansi-colors = ANSI-kleuren
schema-keymap = Toetsindeling
schema-explorer = Verkenner
schema-visible = Zichtbaar
schema-language-servers = Taalservers
schema-servers = Servers
schema-language-id = Taal-ID
schema-root-markers = Projectrootmarkeringen
schema-output-directory = Uitvoermap

menu-scene = Scène
menu-layout = Lay-out
menu-terminal = Terminal
menu-browser = Browser
menu-service = Dienst
menu-bookmark = Bladwijzer
menu-edit = Bewerk

layout-knowledge = Kennis
layout-open-knowledge = Kennis openen
layout-open-welcome-knowledge = Welkom bij Kennis openen
layout-open-path = { $path } openen
layout-fold-knowledge = Kennis inklappen
layout-unfold-knowledge = Kennis uitklappen
layout-bookmarks = Bladwijzers
layout-new-folder = Nieuwe map
layout-add-to-bookmarks = Toevoegen aan bladwijzers
layout-move-to-bookmarks = Verplaatsen naar bladwijzers
layout-stack-number = Stapel { $number }
layout-fold-stack = Stapel inklappen
layout-unfold-stack = Stapel uitklappen
layout-close-stack = Stapel sluiten
layout-bookmark-in = Bladwijzer in { $folder }

common-cancel = Annuleer
common-delete = Verwijder
common-save = Bewaar
common-rename = Hernoem
common-expand = Vouw uit
common-collapse = Vouw samen
common-loading = Laden…
common-error = Fout
common-output = Uitvoer
common-pending = In behandeling
common-current = huidig
common-stop = Stop
services-command = Vmux-service
services-uptime-seconds = { $seconds }s
services-uptime-minutes = { $minutes }m { $seconds }s
services-uptime-hours = { $hours }u { $minutes }m
services-uptime-days = { $days }d { $hours }u

error-page-failed-load = Pagina kon niet worden geladen
error-page-not-found = Pagina niet gevonden
error-unknown-host = Onbekende Vmux-apphost: { $host }

history-title = Geschiedenis

command-new-app-chat = Nieuwe { $provider }/{ $model }-chat (App)
command-interactive-mode-user = Scène > Interactieve modus > Gebruiker
command-interactive-mode-player = Scène > Interactieve modus > Speler
command-minimize-window = Indeling > Venster > Minimaliseer
command-toggle-layout = Indeling > Indeling > Wissel indeling
command-close-tab = Indeling > Tabblad > Sluit tabblad
command-new-task = Indeling > Tabblad > Nieuwe taak…
command-next-tab = Indeling > Tabblad > Volgend tabblad
command-prev-tab = Indeling > Tabblad > Vorig tabblad
command-rename-tab = Indeling > Tabblad > Hernoem tabblad
command-tab-select-1 = Indeling > Tabblad > Selecteer tabblad 1
command-tab-select-2 = Indeling > Tabblad > Selecteer tabblad 2
command-tab-select-3 = Indeling > Tabblad > Selecteer tabblad 3
command-tab-select-4 = Indeling > Tabblad > Selecteer tabblad 4
command-tab-select-5 = Indeling > Tabblad > Selecteer tabblad 5
command-tab-select-6 = Indeling > Tabblad > Selecteer tabblad 6
command-tab-select-7 = Indeling > Tabblad > Selecteer tabblad 7
command-tab-select-8 = Indeling > Tabblad > Selecteer tabblad 8
command-tab-select-last = Indeling > Tabblad > Selecteer laatste tabblad
command-close-pane = Indeling > Deelvenster > Sluit deelvenster
command-select-pane-left = Indeling > Deelvenster > Selecteer linker deelvenster
command-select-pane-right = Indeling > Deelvenster > Selecteer rechter deelvenster
command-select-pane-up = Indeling > Deelvenster > Selecteer deelvenster boven
command-select-pane-down = Indeling > Deelvenster > Selecteer deelvenster onder
command-swap-pane-prev = Indeling > Deelvenster > Verwissel met vorig deelvenster
command-swap-pane-next = Indeling > Deelvenster > Verwissel met volgend deelvenster
command-equalize-pane-size = Indeling > Deelvenster > Maak deelvensters even groot
command-resize-pane-left = Indeling > Deelvenster > Verklein naar links
command-resize-pane-right = Indeling > Deelvenster > Vergroot naar rechts
command-resize-pane-up = Indeling > Deelvenster > Verklein naar boven
command-resize-pane-down = Indeling > Deelvenster > Vergroot naar beneden
command-stack-close = Indeling > Stapel > Sluit stapel
command-stack-next = Indeling > Stapel > Volgende stapel
command-stack-previous = Indeling > Stapel > Vorige stapel
command-stack-reopen = Indeling > Stapel > Open gesloten pagina opnieuw
command-stack-swap-prev = Indeling > Stapel > Verplaats stapel naar links
command-stack-swap-next = Indeling > Stapel > Verplaats stapel naar rechts
command-space-open = Indeling > Ruimte > Ruimtes
command-terminal-close = Terminal > Sluit terminal
command-terminal-next = Terminal > Volgende terminal
command-terminal-prev = Terminal > Vorige terminal
command-terminal-clear = Terminal > Wis terminal
command-browser-prev-page = Browser > Navigatie > Terug
command-browser-next-page = Browser > Navigatie > Vooruit
command-browser-reload = Browser > Navigatie > Herlaad
command-browser-hard-reload = Browser > Navigatie > Herlaad volledig
command-open-in-place = Browser > Openen > Open hier
command-open-in-new-stack = Browser > Openen > Open in nieuwe stapel
command-open-in-pane-top = Browser > Openen > Open in deelvenster erboven
command-open-in-pane-right = Browser > Openen > Open in rechter deelvenster
command-open-in-pane-bottom = Browser > Openen > Open in deelvenster eronder
command-open-in-pane-left = Browser > Openen > Open in linker deelvenster
command-open-in-new-tab = Browser > Openen > Open in nieuw tabblad
command-open-in-new-space = Browser > Openen > Open in nieuwe ruimte
command-browser-zoom-in = Browser > Weergave > Zoom in
command-browser-zoom-out = Browser > Weergave > Zoom uit
command-browser-zoom-reset = Browser > Weergave > Werkelijke grootte
command-browser-dev-tools = Browser > Weergave > Ontwikkelaarstools
command-browser-open-command-bar = Browser > Balk > Commandobalk
command-browser-open-page-in-command-bar = Browser > Balk > Bewerk pagina
command-browser-open-path-bar = Browser > Balk > Padnavigator
command-browser-open-commands = Browser > Balk > Commando’s
command-browser-open-history = Browser > Balk > Geschiedenis
command-service-open = Service > Open servicemonitor
command-bookmark-toggle-active = Bladwijzer > Voeg pagina toe als bladwijzer
command-bookmark-pin-active = Bladwijzer > Pin pagina vast

layout-tab = Tabblad
layout-no-stacks = Geen stapels
layout-loading = Laden…
layout-no-markdown-files = Geen Markdown-bestanden
layout-empty-folder = Lege map
layout-worktree = worktree
layout-folder-name = Mapnaam
layout-no-pins-bookmarks = Geen pins of bladwijzers
layout-move-to = Verplaats naar { $folder }
layout-bookmark-current-page = Voeg huidige pagina toe als bladwijzer
layout-rename-folder = Hernoem map
layout-remove-folder = Verwijder map
layout-update-downloading = Update downloaden
layout-update-installing = Update installeren…
layout-update-ready = Nieuwe versie beschikbaar
layout-restart-update = Herstart om bij te werken

agent-preparing = Agent voorbereiden…
agent-send-all-queued = Verstuur alle prompts in de wachtrij nu (Esc)
agent-send = Verstuur (Enter)
agent-ready = Klaar wanneer jij dat bent.
agent-loading-older = Oudere berichten laden…
agent-load-older = Laad oudere berichten
agent-continued-from = Vervolg van { $source }
agent-older-context-omitted = oudere context weggelaten
agent-interrupted = onderbroken
agent-allow-tool = { $tool } toestaan?
agent-deny = Weiger
agent-allow-always = Altijd toestaan
agent-allow = Sta toe
agent-loading-sessions = Sessies laden…
agent-no-resumable-sessions = Geen hervatbare sessies gevonden
agent-no-matching-sessions = Geen overeenkomende sessies
agent-no-matching-models = Geen overeenkomende modellen
agent-choice-help = ↑/↓ of Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Kies repositorymap
agent-choose-repository-detail = Selecteer de lokale Git-repository die de agent moet gebruiken.
agent-choosing = Kiezen…
agent-choose-folder = Kies map
agent-queued = in wachtrij
agent-attached = Bijgevoegd:
agent-cancel-queued = Annuleer prompt in wachtrij
agent-resume-queued = Hervat prompts in wachtrij
agent-clear-queue = Wis wachtrij
agent-send-all-now = alles nu versturen
agent-choose-option = Kies hierboven een optie
agent-loading-media = Media laden…
agent-no-matching-media = Geen overeenkomende media
agent-prompt-context = Promptcontext
agent-details = Details
agent-path = Pad
agent-tool = Tool
agent-server = Server
agent-bytes = { $count } bytes
agent-worked-for = { $duration } gewerkt
agent-worked-for-steps = { $count ->
    [one] { $duration } gewerkt · 1 stap
   *[other] { $duration } gewerkt · { $count } stappen
}
agent-tool-guardian-review = Guardian-controle
agent-tool-read-files = Bestanden gelezen
agent-tool-viewed-image = Afbeelding bekeken
agent-tool-used-browser = Browser gebruikt
agent-tool-searched-files = Bestanden doorzocht
agent-tool-ran-commands = Commando’s uitgevoerd
agent-thinking = Denken
agent-subagent = Subagent
agent-prompt = Prompt
agent-thread = Thread
agent-parent = Bovenliggend
agent-children = Onderliggend
agent-call = Aanroep
agent-raw-event = Ruwe gebeurtenis
agent-plan = Plan
agent-tasks = { $count ->
    [one] 1 taak
   *[other] { $count } taken
}
agent-edited = Bewerkt
agent-reconnecting = Opnieuw verbinden { $attempt }/{ $total }
agent-status-running = Actief
agent-status-done = Klaar
agent-status-failed = Mislukt
agent-status-pending = In behandeling
agent-slash-attach-files = Bestanden bijvoegen
agent-slash-resume-session = Hervat een eerdere sessie
agent-slash-select-model = Selecteer model
agent-slash-continue-cli = Ga verder met deze sessie in de CLI
agent-session-just-now = zojuist
agent-session-minutes-ago = { $count }m geleden
agent-session-hours-ago = { $count }u geleden
agent-session-days-ago = { $count }d geleden
agent-working-working = Werken
agent-working-thinking = Denken
agent-working-pondering = Nadenken
agent-working-noodling = Pruttelen
agent-working-percolating = Doorsudderen
agent-working-conjuring = Toveren
agent-working-cooking = Koken
agent-working-brewing = Brouwen
agent-working-musing = Peinzen
agent-working-ruminating = Overdenken
agent-working-scheming = Plannen smeden
agent-working-synthesizing = Synthetiseren
agent-working-tinkering = Knutselen
agent-working-churning = Draaien
agent-working-vibing = Vibing
agent-working-simmering = Sudderen
agent-working-crafting = Maken
agent-working-divining = Peilen
agent-working-mulling = Overwegen
agent-working-spelunking = Diep graven

editor-toggle-explorer = Verkenner tonen/verbergen (Cmd+B)
editor-unsaved = niet bewaard
editor-rendered-markdown = Gerenderde Markdown met live bewerking
editor-note = Notitie
editor-source-editor = Broncode-editor
editor-editor = Editor
editor-git-diff = Git-diff
editor-diff = Diff
editor-tidy = Opruimen
editor-always = Altijd
editor-unchanged-previews = { $count ->
    [one] ✦ 1 ongewijzigde preview
   *[other] ✦ { $count } ongewijzigde previews
}
editor-open-externally = Open extern
editor-changed-line = Gewijzigde regel
editor-go-to-definition = Ga naar definitie
editor-find-references = Zoek verwijzingen
editor-references = { $count ->
    [one] 1 verwijzing
   *[other] { $count } verwijzingen
}
editor-lsp-starting = { $server } starten…
editor-lsp-not-installed = { $server } — niet geïnstalleerd
editor-explorer = Verkenner
editor-open-editors = Open editors
editor-outline = Overzicht
editor-new-file = Nieuw bestand
editor-new-folder = Nieuwe map
editor-delete-confirm = “{ $name }” verwijderen? Dit kan niet ongedaan worden gemaakt.
editor-created-folder = Map { $name } aangemaakt
editor-created-file = Bestand { $name } aangemaakt
editor-renamed-to = Hernoemd naar { $name }
editor-deleted = { $name } verwijderd
editor-failed-decode-image = Afbeelding kon niet worden gedecodeerd
editor-preview-large-image = afbeelding (te groot voor preview)
editor-preview-binary = binair
editor-preview-file = bestand

git-status-clean = schoon
git-status-modified = gewijzigd
git-status-staged = gestaged
git-status-staged-modified = gestaged*
git-status-untracked = niet gevolgd
git-status-deleted = verwijderd
git-status-conflict = conflict
git-accept-all = ✓ alles accepteren
git-unstage = Unstage
git-confirm-deny-all = Alles weigeren bevestigen
git-deny-all = ✗ alles weigeren
git-commit-message = commitbericht
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Diff laden…
git-no-changes = Geen wijzigingen om te tonen
git-accept = ✓ accepteren
git-deny = ✗ weigeren
git-show-unchanged-lines = Toon { $count } ongewijzigde regels

terminal-loading = Laden…
terminal-runs-when-ready = voert uit zodra klaar · Ctrl+C wist · Esc slaat over
terminal-booting = opstarten
terminal-type-command = typ een commando · voert uit zodra klaar · Esc slaat over

setup-tagline-claude = De coding-agent van Anthropic, in Vmux
setup-tagline-codex = De coding-agent van OpenAI, in Vmux
setup-tagline-vibe = De coding-agent van Mistral, in Vmux
setup-install-title = Installeer { $name } CLI
setup-homebrew-required = Homebrew is vereist om { $command } te installeren en is nog niet ingesteld. Vmux installeert eerst Homebrew en daarna { $name }.
setup-terminal-instructions = Druk in de terminal op Return om te starten en voer daarna je Mac-wachtwoord in wanneer daarom wordt gevraagd.
setup-command-missing = Vmux heeft deze pagina geopend omdat het lokale commando { $command } nog niet is geïnstalleerd. Voer het onderstaande commando uit om het te installeren.
setup-install-failed = Installatie is niet voltooid. Controleer de terminal voor details en probeer het opnieuw.
setup-installing = Installeren…
setup-install-homebrew = Installeer Homebrew + { $name }
setup-run-install = Voer installatiecommando uit
setup-auto-reload = Vmux voert dit uit in een terminal en herlaadt wanneer { $command } klaar is.

debug-title = Debug
debug-auto-update = Automatisch bijwerken
debug-simulate-update = Beschikbare update simuleren
debug-simulate-download = Download simuleren
debug-clear-update = Update wissen
debug-trigger-restart = Herstart activeren

command-manage-spaces = Spaces beheren…
command-pane-stack-location = paneel { $pane } / stack { $stack }
command-space-pane-stack-location = { $space } / paneel { $pane } / stack { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Interactieve modus
command-group-window = Venster
command-group-tab = Tabblad
command-group-pane = Paneel
command-group-stack = Stack
command-group-space = Space
command-group-navigation = Navigatie
command-group-open = Openen
command-group-view = Weergave
command-group-bar = Balk

menu-close-vmux = Vmux sluiten

agents-terminal-coding-agent = Terminalgebaseerde codingagent
