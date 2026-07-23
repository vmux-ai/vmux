locale-name = svenska
common-open = Öppna
common-close = Stäng
common-install = Installera
common-uninstall = Avinstallera
common-update = Uppdatera
common-retry = Försök igen
common-refresh = Uppdatera
common-remove = Ta bort
common-enable = Aktivera
common-disable = Inaktivera
common-new = Ny
common-active = aktiv
common-running = körs
common-done = klart
common-failed = Misslyckades
common-installed = Installerad
common-items = { $count ->
    [one] { $count } objekt
   *[other] { $count } objekt
}

tools-title = Verktyg
tools-search = Sök efter paket, agenter, MCP, språkverktyg och konfigurationsfiler…
tools-open = Öppna Verktyg
tools-fold = Fäll ihop verktyg
tools-unfold = Fäll ut verktyg
tools-scanning = Söker igenom lokala verktyg…
tools-no-installed = Inga verktyg installerade
tools-empty = Inga matchande verktyg
tools-empty-detail = Installera ett paket eller lägg till ett konfigurationsfilspaket i Stow-stil.
tools-apply = Tillämpa
tools-homebrew = Homebrew
tools-homebrew-sync = Installerade formler och program synkroniseras automatiskt.
tools-open-brewfile = Öppna Brewfile
tools-managed = hanterad
tools-provider-homebrew-formulae = Homebrew-formler
tools-provider-homebrew-casks = Homebrew-program
tools-provider-npm = npm-paket
tools-provider-acp-agents = ACP-agenter
tools-provider-language-tools = Språkverktyg
tools-provider-mcp-servers = MCP-servrar
tools-provider-dotfiles = Konfigurationsfiler
tools-status-available = Tillgänglig
tools-status-missing = Saknas
tools-status-conflict = Konflikt
tools-forget = Glöm
tools-manage = Hantera
tools-link = Länka
tools-unlink = Ta bort länk
tools-import = Importera
tools-update-count = { $count ->
    [one] 1 uppdatering
   *[other] { $count } uppdateringar
}
tools-conflict-count = { $count ->
    [one] 1 konflikt
   *[other] { $count } konflikter
}
tools-result-applied = Verktyg tillämpade
tools-result-imported = Verktyg importerade
tools-result-installed = { $name } installerad
tools-result-updated = { $name } uppdaterad
tools-result-uninstalled = { $name } avinstallerad
tools-result-forgotten = { $name } glömd
tools-result-managed = { $name } hanteras nu
tools-result-linked = { $name } länkad
tools-result-unlinked = Länken till { $name } borttagen
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Synkronisera inställningar, verktyg, dotfiler och Knowledge med Git.
vault-sync = Synkronisera
vault-create = Skapa
vault-connect = Ansluta
vault-private = Privat förvar
vault-public-warning = Offentliga arkiv exponerar din kunskap och konfiguration.
vault-choose-repository = Välj ett arkiv...
vault-empty = tömma
vault-clean = Upp till datum
vault-not-connected = Ej ansluten
vault-change-count = Ändringar: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Start
start-tagline = En prompt. Allt klart.

agents-title = Agenter
agents-search = Sök ACP- och CLI-agenter…
agents-empty = Inga matchande agenter
agents-empty-detail = Prova ett namn, en runtime eller ACP/CLI.
agents-install-failed = Installationen misslyckades
agents-updating = Uppdaterar…
agents-retrying = Försöker igen…
agents-preparing = Förbereder…

extensions-title = Tillägg
extensions-search = Sök installerade tillägg eller Chrome Web Store…
extensions-relaunch = Starta om för att tillämpa
extensions-empty = Inga tillägg installerade
extensions-no-match = Inga matchande tillägg
extensions-empty-detail = Sök i Chrome Web Store ovan och tryck på Retur.
extensions-no-match-detail = Prova ett annat namn eller tilläggs-ID.
extensions-on = På
extensions-off = Av
extensions-enable-confirm = Aktivera { $name }?
extensions-enable-permissions = Aktivera { $name } och tillåt:

lsp-title = Språkservrar
lsp-search = Sök språkservrar, linters, formaterare…
lsp-loading = Läser in katalog…
lsp-empty = Inga matchande språkservrar
lsp-empty-detail = Prova ett annat språk, en linter eller formaterare.
lsp-needs = kräver { $tool }
lsp-status-available = Tillgänglig
lsp-status-on-path = På PATH
lsp-status-installing = Installerar…
lsp-status-installed = Installerad
lsp-status-outdated = Uppdatering finns
lsp-status-running = Körs
lsp-status-failed = Misslyckades

spaces-title = Ytor
spaces-new-placeholder = Namn på ny yta
spaces-empty = Inga ytor
spaces-default-name = Yta { $number }
spaces-tabs = { $count ->
    [one] 1 flik
   *[other] { $count } flikar
}
spaces-delete = Radera yta

team-title = Team
team-just-you = Bara du i den här ytan
team-agents = { $count ->
    [one] Du och 1 agent
   *[other] Du och { $count } agenter
}
team-empty = Ingen här än
team-you = Du
team-agent = Agent

services-title = Bakgrundstjänster
services-processes = { $count ->
    [one] 1 process
   *[other] { $count } processer
}
services-kill-all = Tvångsavsluta alla
services-not-running = Tjänsten körs inte
services-start-with = Starta med:
services-empty = Inga aktiva processer
services-filter = Filtrera processer…
services-no-match = Inga matchande processer
services-connected = Ansluten
services-disconnected = Frånkopplad
services-attached = ansluten
services-kill = Tvångsavsluta
services-memory = Minne
services-size = Storlek
services-shell = Skal

error-title = Fel

history-search = Sök i historik
history-clear-all = Rensa allt
history-clear-confirm = Rensa all historik?
history-clear-warning = Detta går inte att ångra.
history-cancel = Avbryt
history-today = Idag
history-yesterday = Igår
history-days-ago = { $count } dagar sedan
history-day-offset = Dag -{ $count }

settings-title = Inställningar
settings-loading = Läser in inställningar…
settings-stored = Sparas i ~/.vmux/settings.ron
settings-other = Övrigt
settings-software-update = Programuppdatering
settings-check-updates = Sök efter uppdateringar
settings-check-updates-hint = Söker automatiskt vid start och varje timme när automatisk uppdatering är aktiverad.
settings-update-unavailable = Inte tillgänglig
settings-update-unavailable-hint = Uppdateraren ingår inte i den här versionen.
settings-update-checking = Söker…
settings-update-checking-hint = Söker efter uppdateringar…
settings-update-check-again = Sök igen
settings-update-current = Vmux är uppdaterat.
settings-update-downloading = Hämtar…
settings-update-downloading-hint = Hämtar Vmux { $version }…
settings-update-installing = Installerar…
settings-update-installing-hint = Installerar Vmux { $version }…
settings-update-ready = Uppdatering klar
settings-update-ready-hint = Vmux { $version } är klar. Starta om för att tillämpa den.
settings-update-try-again = Försök igen
settings-update-failed = Kunde inte söka efter uppdateringar.
settings-item = Objekt
settings-item-number = Objekt { $number }
settings-press-key = Tryck på en tangent…
settings-saved = Sparat
settings-record-key = Klicka för att spela in en ny tangentkombination

tray-open-window = Öppna fönster
tray-close-window = Stäng fönster
tray-pause-recording = Pausa inspelning
tray-resume-recording = Fortsätt inspelning
tray-finish-recording = Avsluta inspelning
tray-quit = Avsluta Vmux

composer-attach-files = Bifoga filer (/upload)
composer-remove-attachment = Ta bort bilaga

layout-back = Tillbaka
layout-forward = Framåt
layout-reload = Läs om
layout-bookmark-page = Bokmärk den här sidan
layout-remove-bookmark = Ta bort bokmärke
layout-pin-page = Fäst den här sidan
layout-unpin-page = Lossa den här sidan
layout-manage-extensions = Hantera tillägg
layout-new-stack = Ny stack
layout-close-tab = Stäng flik
layout-bookmark = Bokmärke
layout-pin = Fäst
layout-new-tab = Ny flik
layout-team = Team

command-switch-space = Byt yta…
command-search-ask = Sök eller fråga…
command-new-tab-placeholder = Sök eller ange en URL, eller välj Terminal…
command-placeholder = Ange en URL, sök flikar eller skriv > för kommandon…
command-composer-placeholder = Skriv / för kommandon eller @ för media
command-send = Skicka (Retur)
command-terminal = Terminal
command-open-terminal = Öppna i Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] 1 flik
   *[other] { $count } flikar
}
command-prompt = Prompt
command-new-tab = Ny flik
command-search = Sök
command-open-value = Öppna ”{ $value }”
command-search-value = Sök efter ”{ $value }”

schema-appearance = Utseende
schema-general = Allmänt
schema-layout = Layout
schema-layout-detail = Fönster, paneler, sidofält och fokusring.
schema-agent = Agent
schema-agent-detail = Agentens beteende och behörigheter för verktyg.
schema-shortcuts = Kortkommandon
schema-shortcuts-detail = Skrivskyddad vy. Redigera settings.ron direkt för att ändra bindningar.
schema-terminal = Terminal
schema-browser = Webbläsare
schema-mode = Läge
schema-mode-detail = Färgschema för webbsidor. Enhet följer systemet.
schema-device = Enhet
schema-light = Ljust
schema-dark = Mörkt
schema-language = Språk
schema-language-detail = Använd systemet, en-US, ja eller valfri BCP 47-tagg med en matchande ~/.vmux/locales/<tag>.ftl-katalog.
schema-auto-update = Automatisk uppdatering
schema-auto-update-detail = Sök efter och installera uppdateringar vid start och varje timme.
schema-startup-url = Start-URL
schema-startup-url-detail = Tomt öppnar kommandoradens prompt.
schema-search-engine = Sökmotor
schema-search-engine-detail = Används för webbsökningar från Start och kommandoraden.
schema-window = Fönster
schema-pane = Panel
schema-side-sheet = Sidopanel
schema-focus-ring = Fokusring
schema-run-placement = Tillåt åsidosättning av körplacering
schema-run-placement-detail = Låt agenter välja panelläge, riktning och ankare för körning.
schema-leader = Leader
schema-leader-detail = Prefixtangent för chord-kortkommandon.
schema-chord-timeout = Tidsgräns för chord
schema-chord-timeout-detail = Millisekunder innan ett chord-prefix slutar gälla.
schema-bindings = Bindningar
schema-confirm-close = Bekräfta stängning
schema-confirm-close-detail = Fråga innan en terminal med en körande process stängs.
schema-default-theme = Standardtema
schema-default-theme-detail = Namnet på det aktiva temat i temalistan.

settings-empty = (tom)
settings-none = (ingen)

schema-system = System
schema-editor = Redigerare
schema-recording = Inspelning
schema-radius = Radie
schema-padding = Utfyllnad
schema-gap = Mellanrum
schema-width = Bredd
schema-color = Färg
schema-red = Röd
schema-green = Grön
schema-blue = Blå
schema-follow-files = Följ filer
schema-tidy-files = Rensa filer
schema-tidy-files-max = Tröskel för filrensning
schema-tidy-files-auto = Rensa filer automatiskt
schema-app-providers = App-leverantörer
schema-provider = Leverantör
schema-kind = Typ
schema-models = Modeller
schema-acp = ACP-agenter
schema-id = ID
schema-name = Namn
schema-command = Kommando
schema-arguments = Argument
schema-environment = Miljövariabler
schema-working-directory = Arbetskatalog
schema-shell = Skal
schema-font-family = Typsnittsfamilj
schema-startup-directory = Startkatalog
schema-themes = Teman
schema-color-scheme = Färgschema
schema-font-size = Teckenstorlek
schema-line-height = Radhöjd
schema-cursor-style = Markörstil
schema-cursor-blink = Blinkande markör
schema-custom-themes = Anpassade teman
schema-foreground = Förgrund
schema-background = Bakgrund
schema-cursor = Markör
schema-ansi-colors = ANSI-färger
schema-keymap = Tangentmappning
schema-explorer = Utforskare
schema-visible = Synlig
schema-language-servers = Språkservrar
schema-servers = Servrar
schema-language-id = Språk-ID
schema-root-markers = Rotmarkörer
schema-output-directory = Utdatakatalog

menu-scene = Scen
menu-layout = Layout
menu-terminal = Terminal
menu-browser = Webbläsare
menu-service = Tjänster
menu-bookmark = Bokmärke
menu-edit = Redigera

layout-knowledge = Kunskap
layout-open-knowledge = Öppna Kunskap
layout-open-welcome-knowledge = Öppna Välkommen till Kunskap
layout-open-path = Öppna { $path }
layout-fold-knowledge = Fäll ihop kunskap
layout-unfold-knowledge = Fäll ut kunskap
layout-bookmarks = Bokmärken
layout-new-folder = Ny mapp
layout-add-to-bookmarks = Lägg till i Bokmärken
layout-move-to-bookmarks = Flytta till Bokmärken
layout-stack-number = Stapel { $number }
layout-fold-stack = Fäll ihop stapel
layout-unfold-stack = Fäll ut stapel
layout-close-stack = Stäng stapel
layout-bookmark-in = Bokmärk i { $folder }

common-cancel = Avbryt
common-delete = Radera
common-save = Spara
common-rename = Byt namn
common-expand = Expandera
common-collapse = Fäll ihop
common-loading = Läser in…
common-error = Fel
common-output = Utdata
common-pending = Väntar
common-current = aktuell
common-stop = Stoppa
services-command = Vmux-tjänst
services-uptime-seconds = { $seconds } s
services-uptime-minutes = { $minutes } min { $seconds } s
services-uptime-hours = { $hours } tim { $minutes } min
services-uptime-days = { $days } d { $hours } tim

error-page-failed-load = Sidan kunde inte läsas in
error-page-not-found = Sidan hittades inte
error-unknown-host = Okänd Vmux-appvärd: { $host }

history-title = Historik

command-new-app-chat = Ny { $provider }/{ $model }-chatt (app)
command-interactive-mode-user = Scen > Interaktivt läge > Användare
command-interactive-mode-player = Scen > Interaktivt läge > Spelare
command-minimize-window = Layout > Fönster > Minimera
command-toggle-layout = Layout > Layout > Växla layout
command-close-tab = Layout > Flik > Stäng flik
command-new-task = Layout > Flik > Ny uppgift…
command-next-tab = Layout > Flik > Nästa flik
command-prev-tab = Layout > Flik > Föregående flik
command-rename-tab = Layout > Flik > Byt namn på flik
command-tab-select-1 = Layout > Flik > Välj flik 1
command-tab-select-2 = Layout > Flik > Välj flik 2
command-tab-select-3 = Layout > Flik > Välj flik 3
command-tab-select-4 = Layout > Flik > Välj flik 4
command-tab-select-5 = Layout > Flik > Välj flik 5
command-tab-select-6 = Layout > Flik > Välj flik 6
command-tab-select-7 = Layout > Flik > Välj flik 7
command-tab-select-8 = Layout > Flik > Välj flik 8
command-tab-select-last = Layout > Flik > Välj sista fliken
command-close-pane = Layout > Panel > Stäng panel
command-select-pane-left = Layout > Panel > Välj vänster panel
command-select-pane-right = Layout > Panel > Välj höger panel
command-select-pane-up = Layout > Panel > Välj panel ovanför
command-select-pane-down = Layout > Panel > Välj panel nedanför
command-swap-pane-prev = Layout > Panel > Byt panel med föregående
command-swap-pane-next = Layout > Panel > Byt panel med nästa
command-equalize-pane-size = Layout > Panel > Gör paneler lika stora
command-resize-pane-left = Layout > Panel > Ändra panelstorlek åt vänster
command-resize-pane-right = Layout > Panel > Ändra panelstorlek åt höger
command-resize-pane-up = Layout > Panel > Ändra panelstorlek uppåt
command-resize-pane-down = Layout > Panel > Ändra panelstorlek nedåt
command-stack-close = Layout > Stack > Stäng stack
command-stack-next = Layout > Stack > Nästa stack
command-stack-previous = Layout > Stack > Föregående stack
command-stack-reopen = Layout > Stack > Öppna stängd sida igen
command-stack-swap-prev = Layout > Stack > Flytta stack åt vänster
command-stack-swap-next = Layout > Stack > Flytta stack åt höger
command-space-open = Layout > Space > Spaces
command-terminal-close = Terminal > Stäng terminal
command-terminal-next = Terminal > Nästa terminal
command-terminal-prev = Terminal > Föregående terminal
command-terminal-clear = Terminal > Rensa terminal
command-browser-prev-page = Webbläsare > Navigering > Tillbaka
command-browser-next-page = Webbläsare > Navigering > Framåt
command-browser-reload = Webbläsare > Navigering > Läs in igen
command-browser-hard-reload = Webbläsare > Navigering > Läs in helt på nytt
command-open-in-place = Webbläsare > Öppna > Öppna här
command-open-in-new-stack = Webbläsare > Öppna > Öppna i ny stack
command-open-in-pane-top = Webbläsare > Öppna > Öppna i panel ovanför
command-open-in-pane-right = Webbläsare > Öppna > Öppna i panel till höger
command-open-in-pane-bottom = Webbläsare > Öppna > Öppna i panel nedanför
command-open-in-pane-left = Webbläsare > Öppna > Öppna i panel till vänster
command-open-in-new-tab = Webbläsare > Öppna > Öppna i ny flik
command-open-in-new-space = Webbläsare > Öppna > Öppna i ny Space
command-browser-zoom-in = Webbläsare > Visa > Zooma in
command-browser-zoom-out = Webbläsare > Visa > Zooma ut
command-browser-zoom-reset = Webbläsare > Visa > Verklig storlek
command-browser-dev-tools = Webbläsare > Visa > Utvecklarverktyg
command-browser-open-command-bar = Webbläsare > Fält > Kommandofält
command-browser-open-page-in-command-bar = Webbläsare > Fält > Redigera sida
command-browser-open-path-bar = Webbläsare > Fält > Sökvägsnavigator
command-browser-open-commands = Webbläsare > Fält > Kommandon
command-browser-open-history = Webbläsare > Fält > Historik
command-service-open = Tjänst > Öppna tjänstövervakaren
command-bookmark-toggle-active = Bokmärke > Bokmärk sida
command-bookmark-pin-active = Bokmärke > Fäst sida

layout-tab = Flik
layout-no-stacks = Inga stackar
layout-loading = Läser in…
layout-no-markdown-files = Inga Markdown-filer
layout-empty-folder = Tom mapp
layout-worktree = worktree
layout-folder-name = Mappnamn
layout-no-pins-bookmarks = Inga fästa sidor eller bokmärken
layout-move-to = Flytta till { $folder }
layout-bookmark-current-page = Bokmärk aktuell sida
layout-rename-folder = Byt namn på mapp
layout-remove-folder = Ta bort mapp
layout-update-downloading = Hämtar uppdatering
layout-update-installing = Installerar uppdatering…
layout-update-ready = Ny version tillgänglig
layout-restart-update = Starta om för att uppdatera

agent-preparing = Förbereder agent…
agent-send-all-queued = Skicka alla köade prompter nu (Esc)
agent-send = Skicka (Enter)
agent-ready = Redo när du är det.
agent-loading-older = Läser in äldre meddelanden…
agent-load-older = Läs in äldre meddelanden
agent-continued-from = Fortsatt från { $source }
agent-older-context-omitted = äldre kontext utelämnad
agent-interrupted = avbruten
agent-allow-tool = Tillåt { $tool }?
agent-deny = Neka
agent-allow-always = Tillåt alltid
agent-allow = Tillåt
agent-loading-sessions = Läser in sessioner…
agent-no-resumable-sessions = Inga återupptagbara sessioner hittades
agent-no-matching-sessions = Inga matchande sessioner
agent-no-matching-models = Inga matchande modeller
agent-choice-help = ↑/↓ eller Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Välj repository-mapp
agent-choose-repository-detail = Välj det lokala Git-repository som agenten ska använda.
agent-choosing = Väljer…
agent-choose-folder = Välj mapp
agent-queued = köad
agent-attached = Bifogat:
agent-cancel-queued = Avbryt köad prompt
agent-resume-queued = Återuppta köade prompter
agent-clear-queue = Rensa kö
agent-send-all-now = skicka alla nu
agent-choose-option = Välj ett alternativ ovan
agent-loading-media = Läser in media…
agent-no-matching-media = Inga matchande media
agent-prompt-context = Promptkontext
agent-details = Detaljer
agent-path = Sökväg
agent-tool = Verktyg
agent-server = Server
agent-bytes = { $count } byte
agent-worked-for = Arbetade i { $duration }
agent-worked-for-steps = { $count ->
    [one] Arbetade i { $duration } · 1 steg
   *[other] Arbetade i { $duration } · { $count } steg
}
agent-tool-guardian-review = Guardian-granskning
agent-tool-read-files = Läste filer
agent-tool-viewed-image = Visade bild
agent-tool-used-browser = Använde webbläsare
agent-tool-searched-files = Sökte i filer
agent-tool-ran-commands = Körda kommandon
agent-thinking = Tänker
agent-subagent = Underagent
agent-prompt = Prompt
agent-thread = Tråd
agent-parent = Överordnad
agent-children = Underordnade
agent-call = Anrop
agent-raw-event = Rå händelse
agent-plan = Plan
agent-tasks = { $count ->
    [one] 1 uppgift
   *[other] { $count } uppgifter
}
agent-edited = Redigerade
agent-reconnecting = Återansluter { $attempt }/{ $total }
agent-status-running = Körs
agent-status-done = Klart
agent-status-failed = Misslyckades
agent-status-pending = Väntar
agent-slash-attach-files = Bifoga filer
agent-slash-resume-session = Återuppta en tidigare session
agent-slash-select-model = Välj modell
agent-slash-continue-cli = Fortsätt den här sessionen i CLI
agent-session-just-now = nyss
agent-session-minutes-ago = för { $count } min sedan
agent-session-hours-ago = för { $count } tim sedan
agent-session-days-ago = för { $count } d sedan
agent-working-working = Arbetar
agent-working-thinking = Tänker
agent-working-pondering = Funderar
agent-working-noodling = Klurar
agent-working-percolating = Bearbetar
agent-working-conjuring = Trollar fram
agent-working-cooking = Kokar ihop
agent-working-brewing = Brygger
agent-working-musing = Reflekterar
agent-working-ruminating = Grubblar
agent-working-scheming = Planerar
agent-working-synthesizing = Syntetiserar
agent-working-tinkering = Pular
agent-working-churning = Jobbar på
agent-working-vibing = Vibbar
agent-working-simmering = Sjuder
agent-working-crafting = Skapar
agent-working-divining = Letar svar
agent-working-mulling = Överväger
agent-working-spelunking = Djupdyker

editor-toggle-explorer = Visa/dölj Utforskaren (Cmd+B)
editor-unsaved = osparad
editor-rendered-markdown = Renderad Markdown med liveredigering
editor-note = Anteckning
editor-source-editor = Källkodsredigerare
editor-editor = Redigerare
editor-git-diff = Git-diff
editor-diff = Diff
editor-tidy = Städa
editor-always = Alltid
editor-unchanged-previews = { $count ->
    [one] ✦ 1 oförändrad förhandsvisning
   *[other] ✦ { $count } oförändrade förhandsvisningar
}
editor-open-externally = Öppna externt
editor-changed-line = Ändrad rad
editor-go-to-definition = Gå till definition
editor-find-references = Hitta referenser
editor-references = { $count ->
    [one] 1 referens
   *[other] { $count } referenser
}
editor-lsp-starting = { $server } startar…
editor-lsp-not-installed = { $server } — inte installerad
editor-explorer = Utforskaren
editor-open-editors = Öppna redigerare
editor-outline = Översikt
editor-new-file = Ny fil
editor-new-folder = Ny mapp
editor-delete-confirm = Radera ”{ $name }”? Det kan inte ångras.
editor-created-folder = Skapade mappen { $name }
editor-created-file = Skapade filen { $name }
editor-renamed-to = Bytte namn till { $name }
editor-deleted = Raderade { $name }
editor-failed-decode-image = Kunde inte avkoda bild
editor-preview-large-image = bild (för stor för förhandsvisning)
editor-preview-binary = binär
editor-preview-file = fil

git-status-clean = ren
git-status-modified = ändrad
git-status-staged = stagad
git-status-staged-modified = stagad*
git-status-untracked = ospårad
git-status-deleted = raderad
git-status-conflict = konflikt
git-accept-all = ✓ acceptera alla
git-unstage = Avstaga
git-confirm-deny-all = Bekräfta neka alla
git-deny-all = ✗ neka alla
git-commit-message = commit-meddelande
git-commit = Commit ({ $count })
git-push = ↑ Pusha
git-loading-diff = Läser in diff…
git-no-changes = Inga ändringar att visa
git-accept = ✓ acceptera
git-deny = ✗ neka
git-show-unchanged-lines = Visa { $count } oförändrade rader

terminal-loading = Läser in…
terminal-runs-when-ready = körs när den är redo · Ctrl+C rensar · Esc hoppar över
terminal-booting = startar
terminal-type-command = skriv ett kommando · körs när den är redo · Esc hoppar över

setup-tagline-claude = Anthropics kodagent, i Vmux
setup-tagline-codex = OpenAI:s kodagent, i Vmux
setup-tagline-vibe = Mistrals kodagent, i Vmux
setup-install-title = Installera { $name } CLI
setup-homebrew-required = Homebrew krävs för att installera { $command } och är inte konfigurerat än. Vmux installerar Homebrew först och sedan { $name }.
setup-terminal-instructions = Tryck på Retur i terminalen för att starta och ange sedan ditt Mac-lösenord när du uppmanas.
setup-command-missing = Vmux öppnade den här sidan eftersom det lokala kommandot { $command } inte är installerat än. Kör kommandot nedan för att hämta det.
setup-install-failed = Installationen slutfördes inte. Kontrollera terminalen för detaljer och försök igen.
setup-installing = Installerar…
setup-install-homebrew = Installera Homebrew + { $name }
setup-run-install = Kör installationskommando
setup-auto-reload = Vmux kör det i en terminal och läser in igen när { $command } är redo.

debug-title = Felsök
debug-auto-update = Uppdatera automatiskt
debug-simulate-update = Simulera tillgänglig uppdatering
debug-simulate-download = Simulera hämtning
debug-clear-update = Rensa uppdatering
debug-trigger-restart = Utlös omstart

command-manage-spaces = Hantera spaces…
command-pane-stack-location = panel { $pane } / stack { $stack }
command-space-pane-stack-location = { $space } / panel { $pane } / stack { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Interaktivt läge
command-group-window = Fönster
command-group-tab = Flik
command-group-pane = Panel
command-group-stack = Stack
command-group-space = Space
command-group-navigation = Navigering
command-group-open = Öppna
command-group-view = Visa
command-group-bar = Fält

menu-close-vmux = Stäng Vmux

agents-terminal-coding-agent = Terminalbaserad kodningsagent
