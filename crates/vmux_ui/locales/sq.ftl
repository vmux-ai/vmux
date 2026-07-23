locale-name = shqip
common-open = Hap
common-close = Mbyll
common-install = Instalo
common-uninstall = Çinstalo
common-update = Përditëso
common-retry = Provo përsëri
common-refresh = Rifresko
common-remove = Hiq
common-enable = Aktivizo
common-disable = Çaktivizo
common-new = I ri
common-active = aktiv
common-running = në ekzekutim
common-done = përfunduar
common-failed = Dështoi
common-installed = Instaluar
common-items = { $count ->
    [one] { $count } artikull
   *[other] { $count } artikuj
}

tools-title = Mjete
tools-search = Kërko paketa, agjentë, MCP, mjete gjuhësore dhe skedarë konfigurimi…
tools-open = Hap Mjetet
tools-fold = Palos mjetet
tools-unfold = Shpalos mjetet
tools-scanning = Po skanohen mjetet vendore…
tools-no-installed = Nuk ka mjete të instaluara
tools-empty = Nuk ka mjete që përputhen
tools-empty-detail = Instaloni një paketë ose shtoni një paketë skedarësh konfigurimi në stilin Stow.
tools-apply = Zbato
tools-homebrew = Homebrew
tools-homebrew-sync = Formulat dhe aplikacionet e instaluara sinkronizohen automatikisht.
tools-open-brewfile = Hap Brewfile
tools-managed = i menaxhuar
tools-provider-homebrew-formulae = Formula Homebrew
tools-provider-homebrew-casks = Aplikacione Homebrew
tools-provider-npm = Paketa npm
tools-provider-acp-agents = Agjentë ACP
tools-provider-language-tools = Mjete gjuhësore
tools-provider-mcp-servers = Serverë MCP
tools-provider-dotfiles = Skedarë konfigurimi
tools-status-available = I disponueshëm
tools-status-missing = Mungon
tools-status-conflict = Konflikt
tools-forget = Harro
tools-manage = Menaxho
tools-link = Lidh
tools-unlink = Shkëput
tools-import = Importo
tools-update-count = { $count ->
    [one] 1 përditësim
   *[other] { $count } përditësime
}
tools-conflict-count = { $count ->
    [one] 1 konflikt
   *[other] { $count } konflikte
}
tools-result-applied = Mjetet u zbatuan
tools-result-imported = Mjetet u importuan
tools-result-installed = { $name } u instalua
tools-result-updated = { $name } u përditësua
tools-result-uninstalled = { $name } u çinstalua
tools-result-forgotten = { $name } u harrua
tools-result-managed = { $name } tani menaxhohet
tools-result-linked = { $name } u lidh
tools-result-unlinked = { $name } u shkëput
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Sinkronizoni cilësimet, veglat, skedarët me pika dhe njohuritë me Git.
vault-sync = Sinkronizimi
vault-create = Krijo
vault-connect = Lidheni
vault-private = Depo private
vault-public-warning = Depot publike ekspozojnë njohuritë dhe konfigurimin tuaj.
vault-choose-repository = Zgjidhni një depo…
vault-empty = bosh
vault-clean = Të përditësuar
vault-not-connected = Nuk është i lidhur
vault-change-count = Ndryshimet: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Fillo
start-tagline = Një prompt. Çdo gjë, e kryer.

agents-title = Agjentët
agents-search = Kërko agjentë ACP dhe CLI…
agents-empty = Nuk ka agjentë që përputhen
agents-empty-detail = Provo me emër, runtime ose ACP/CLI.
agents-install-failed = Instalimi dështoi
agents-updating = Po përditësohet…
agents-retrying = Po provohet përsëri…
agents-preparing = Po përgatitet…

extensions-title = Shtesat
extensions-search = Kërko te të instaluarat ose në Chrome Web Store…
extensions-relaunch = Rihape për ta zbatuar
extensions-empty = Nuk ka shtesa të instaluara
extensions-no-match = Nuk ka shtesa që përputhen
extensions-empty-detail = Kërko më sipër në Chrome Web Store dhe shtyp Return.
extensions-no-match-detail = Provo një emër tjetër ose ID shtese.
extensions-on = Aktiv
extensions-off = Joaktiv
extensions-enable-confirm = Të aktivizohet { $name }?
extensions-enable-permissions = Aktivizo { $name } dhe lejo:

lsp-title = Serverët e gjuhëve
lsp-search = Kërko serverë gjuhësh, linterë, formatues…
lsp-loading = Po ngarkohet katalogu…
lsp-empty = Nuk ka serverë gjuhësh që përputhen
lsp-empty-detail = Provo një gjuhë, linter ose formatues tjetër.
lsp-needs = kërkon { $tool }
lsp-status-available = I disponueshëm
lsp-status-on-path = Në PATH
lsp-status-installing = Po instalohet…
lsp-status-installed = Instaluar
lsp-status-outdated = Ka përditësim
lsp-status-running = Në ekzekutim
lsp-status-failed = Dështoi

spaces-title = Hapësirat
spaces-new-placeholder = Emri i hapësirës së re
spaces-empty = Nuk ka hapësira
spaces-default-name = Hapësira { $number }
spaces-tabs = { $count ->
    [one] 1 skedë
   *[other] { $count } skeda
}
spaces-delete = Fshi hapësirën

team-title = Ekipi
team-just-you = Vetëm ti në këtë hapësirë
team-agents = { $count ->
    [one] Ti dhe 1 agjent
   *[other] Ti dhe { $count } agjentë
}
team-empty = Ende s’ka askush këtu
team-you = Ti
team-agent = Agjent

services-title = Shërbimet në sfond
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procese
}
services-kill-all = Ndërprit të gjitha me forcë
services-not-running = Shërbimi nuk po ekzekutohet
services-start-with = Nise me:
services-empty = Nuk ka procese aktive
services-filter = Filtro proceset…
services-no-match = Nuk ka procese që përputhen
services-connected = Lidhur
services-disconnected = Shkëputur
services-attached = bashkëngjitur
services-kill = Ndërprit me forcë
services-memory = Memoria
services-size = Madhësia
services-shell = Shell

error-title = Gabim

history-search = Kërko në historik
history-clear-all = Pastro të gjitha
history-clear-confirm = Të pastrohet i gjithë historiku?
history-clear-warning = Kjo nuk mund të zhbëhet.
history-cancel = Anulo
history-today = Sot
history-yesterday = Dje
history-days-ago = Para { $count } ditësh
history-day-offset = Dita -{ $count }

settings-title = Cilësimet
settings-loading = Po ngarkohen cilësimet…
settings-stored = Ruhet në ~/.vmux/settings.ron
settings-other = Të tjera
settings-software-update = Përditësimi i softuerit
settings-check-updates = Kontrollo për përditësime
settings-check-updates-hint = Kontrollon automatikisht në nisje dhe çdo orë kur përditësimi automatik është aktiv.
settings-update-unavailable = I padisponueshëm
settings-update-unavailable-hint = Përditësuesi nuk është përfshirë në këtë ndërtim.
settings-update-checking = Po kontrollohet…
settings-update-checking-hint = Po kontrollohet për përditësime…
settings-update-check-again = Kontrollo përsëri
settings-update-current = Vmux është i përditësuar.
settings-update-downloading = Po shkarkohet…
settings-update-downloading-hint = Po shkarkohet Vmux { $version }…
settings-update-installing = Po instalohet…
settings-update-installing-hint = Po instalohet Vmux { $version }…
settings-update-ready = Përditësimi është gati
settings-update-ready-hint = Vmux { $version } është gati. Rinise për ta zbatuar.
settings-update-try-again = Provo përsëri
settings-update-failed = Nuk u kontrollua dot për përditësime.
settings-item = Artikull
settings-item-number = Artikulli { $number }
settings-press-key = Shtyp një tast…
settings-saved = U ruajt
settings-record-key = Kliko për të regjistruar një kombinim të ri tastesh

tray-open-window = Hap dritaren
tray-close-window = Mbyll dritaren
tray-pause-recording = Ndalo përkohësisht regjistrimin
tray-resume-recording = Vazhdo regjistrimin
tray-finish-recording = Përfundo regjistrimin
tray-quit = Dil nga Vmux

composer-attach-files = Bashkëngjit skedarë (/upload)
composer-remove-attachment = Hiq bashkëngjitjen

layout-back = Prapa
layout-forward = Përpara
layout-reload = Ringarko
layout-bookmark-page = Shto këtë faqe te faqeshënuesit
layout-remove-bookmark = Hiq faqeshënuesin
layout-pin-page = Gozhdo këtë faqe
layout-unpin-page = Çgozhdo këtë faqe
layout-manage-extensions = Menaxho shtesat
layout-new-stack = Shtresë e re
layout-close-tab = Mbyll skedën
layout-bookmark = Faqeshënues
layout-pin = Gozhdo
layout-new-tab = Skedë e re
layout-team = Ekipi

command-switch-space = Ndërro hapësirën…
command-search-ask = Kërko ose pyet…
command-new-tab-placeholder = Kërko ose shkruaj një URL, ose zgjidh Terminalin…
command-placeholder = Shkruaj një URL, kërko skeda ose > për komanda…
command-composer-placeholder = Shkruaj / për komanda ose @ për media
command-send = Dërgo (Enter)
command-terminal = Terminal
command-open-terminal = Hap në Terminal
command-stack = Shtresë
command-tabs = { $count ->
    [one] 1 skedë
   *[other] { $count } skeda
}
command-prompt = Prompt
command-new-tab = Skedë e re
command-search = Kërko
command-open-value = Hap “{ $value }”
command-search-value = Kërko “{ $value }”

schema-appearance = Pamja
schema-general = Të përgjithshme
schema-layout = Paraqitja
schema-layout-detail = Dritarja, panelet, shiriti anësor dhe unaza e fokusit.
schema-agent = Agjenti
schema-agent-detail = Sjellja e agjentit dhe lejet e mjeteve.
schema-shortcuts = Shkurtoret
schema-shortcuts-detail = Vetëm për lexim. Për të ndryshuar lidhjet, redakto drejtpërdrejt settings.ron.
schema-terminal = Terminal
schema-browser = Shfletuesi
schema-mode = Modaliteti
schema-mode-detail = Skema e ngjyrave për faqet web. Pajisja ndjek sistemin tënd.
schema-device = Pajisja
schema-light = E çelët
schema-dark = E errët
schema-language = Gjuha
schema-language-detail = Përdor sistemin, en-US, ja ose çdo etiketë BCP 47 me një katalog përkatës ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Përditësim automatik
schema-auto-update-detail = Kontrollo dhe instalo përditësime në nisje dhe çdo orë.
schema-startup-url = URL e nisjes
schema-startup-url-detail = Nëse lihet bosh, hap promptin e shiritit të komandave.
schema-search-engine = Motori i kërkimit
schema-search-engine-detail = Përdoret për kërkime në web nga Fillo dhe shiriti i komandave.
schema-window = Dritarja
schema-pane = Paneli
schema-side-sheet = Fleta anësore
schema-focus-ring = Unaza e fokusit
schema-run-placement = Lejo mbishkrimin e vendosjes së ekzekutimit
schema-run-placement-detail = Lejo agjentët të zgjedhin modalitetin, drejtimin dhe ankorën e panelit të ekzekutimit.
schema-leader = Leader
schema-leader-detail = Tasti prefiks për shkurtoret chord.
schema-chord-timeout = Afati i chord
schema-chord-timeout-detail = Milisekondat para se të skadojë prefiksi i një chord.
schema-bindings = Lidhjet
schema-confirm-close = Konfirmo mbylljen
schema-confirm-close-detail = Pyet para se të mbyllet një terminal me proces në ekzekutim.
schema-default-theme = Tema e parazgjedhur
schema-default-theme-detail = Emri i temës aktive nga lista e temave.

settings-empty = (bosh)
settings-none = (asnjë)

schema-system = Sistemi
schema-editor = Redaktori
schema-recording = Regjistrimi
schema-radius = Rrezja
schema-padding = Mbushja
schema-gap = Hapësira
schema-width = Gjerësia
schema-color = Ngjyra
schema-red = E kuqe
schema-green = E gjelbër
schema-blue = Blu
schema-follow-files = Ndiq skedarët
schema-tidy-files = Sistemo skedarët
schema-tidy-files-max = Pragu i sistemimit të skedarëve
schema-tidy-files-auto = Sistemo skedarët automatikisht
schema-app-providers = Ofrues aplikacionesh
schema-provider = Ofruesi
schema-kind = Lloji
schema-models = Modelet
schema-acp = Agjentë ACP
schema-id = ID
schema-name = Emri
schema-command = Komanda
schema-arguments = Argumente
schema-environment = Mjedisi
schema-working-directory = Drejtoria e punës
schema-shell = Shell
schema-font-family = Familja e shkronjave
schema-startup-directory = Drejtoria e nisjes
schema-themes = Temat
schema-color-scheme = Skema e ngjyrave
schema-font-size = Madhësia e shkronjave
schema-line-height = Lartësia e rreshtit
schema-cursor-style = Stili i kursorit
schema-cursor-blink = Pulsimi i kursorit
schema-custom-themes = Tema të personalizuara
schema-foreground = Plani i parë
schema-background = Sfondi
schema-cursor = Kursori
schema-ansi-colors = Ngjyra ANSI
schema-keymap = Harta e tasteve
schema-explorer = Eksploruesi
schema-visible = I dukshëm
schema-language-servers = Serverë gjuhësh
schema-servers = Serverë
schema-language-id = ID e gjuhës
schema-root-markers = Shënues rrënje
schema-output-directory = Drejtoria e daljes

menu-scene = Skena
menu-layout = Paraqitja
menu-terminal = Terminali
menu-browser = Shfletuesi
menu-service = Shërbimi
menu-bookmark = Faqeshënuesi
menu-edit = Redaktimi

layout-knowledge = Njohuri
layout-open-knowledge = Hap Njohuritë
layout-open-welcome-knowledge = Hap Mirë se vini te Njohuritë
layout-open-path = Hap { $path }
layout-fold-knowledge = Palos njohuritë
layout-unfold-knowledge = Shpalos njohuritë
layout-bookmarks = Faqeshënues
layout-new-folder = Dosje e re
layout-add-to-bookmarks = Shto te Faqeshënuesit
layout-move-to-bookmarks = Zhvendos te Faqeshënuesit
layout-stack-number = Shtresa { $number }
layout-fold-stack = Palos shtresën
layout-unfold-stack = Shpalos shtresën
layout-close-stack = Mbyll shtresën
layout-bookmark-in = Faqeshënues te { $folder }

common-cancel = Anulo
common-delete = Fshi
common-save = Ruaj
common-rename = Riemërto
common-expand = Zgjero
common-collapse = Palos
common-loading = Po ngarkohet…
common-error = Gabim
common-output = Dalje
common-pending = Në pritje
common-current = aktual
common-stop = Ndalo
services-command = Shërbim Vmux
services-uptime-seconds = { $seconds }s
services-uptime-minutes = { $minutes }m { $seconds }s
services-uptime-hours = { $hours }h { $minutes }m
services-uptime-days = { $days }d { $hours }h

error-page-failed-load = Faqja nuk u ngarkua
error-page-not-found = Faqja nuk u gjet
error-unknown-host = Host i panjohur aplikacioni Vmux: { $host }

history-title = Historiku

command-new-app-chat = Bisedë e re me { $provider }/{ $model } (Aplikacion)
command-interactive-mode-user = Skena > Modaliteti interaktiv > Përdorues
command-interactive-mode-player = Skena > Modaliteti interaktiv > Luajtës
command-minimize-window = Paraqitja > Dritarja > Minimizoj
command-toggle-layout = Paraqitja > Paraqitja > Ndërro paraqitjen
command-close-tab = Paraqitja > Skeda > Mbyll skedën
command-new-task = Paraqitja > Skeda > Detyrë e re…
command-next-tab = Paraqitja > Skeda > Skeda tjetër
command-prev-tab = Paraqitja > Skeda > Skeda e mëparshme
command-rename-tab = Paraqitja > Skeda > Riemërto skedën
command-tab-select-1 = Paraqitja > Skeda > Zgjidh skedën 1
command-tab-select-2 = Paraqitja > Skeda > Zgjidh skedën 2
command-tab-select-3 = Paraqitja > Skeda > Zgjidh skedën 3
command-tab-select-4 = Paraqitja > Skeda > Zgjidh skedën 4
command-tab-select-5 = Paraqitja > Skeda > Zgjidh skedën 5
command-tab-select-6 = Paraqitja > Skeda > Zgjidh skedën 6
command-tab-select-7 = Paraqitja > Skeda > Zgjidh skedën 7
command-tab-select-8 = Paraqitja > Skeda > Zgjidh skedën 8
command-tab-select-last = Paraqitja > Skeda > Zgjidh skedën e fundit
command-close-pane = Paraqitja > Paneli > Mbyll panelin
command-select-pane-left = Paraqitja > Paneli > Zgjidh panelin majtas
command-select-pane-right = Paraqitja > Paneli > Zgjidh panelin djathtas
command-select-pane-up = Paraqitja > Paneli > Zgjidh panelin sipër
command-select-pane-down = Paraqitja > Paneli > Zgjidh panelin poshtë
command-swap-pane-prev = Paraqitja > Paneli > Ndërro me panelin e mëparshëm
command-swap-pane-next = Paraqitja > Paneli > Ndërro me panelin tjetër
command-equalize-pane-size = Paraqitja > Paneli > Barazo madhësinë e paneleve
command-resize-pane-left = Paraqitja > Paneli > Ndrysho madhësinë majtas
command-resize-pane-right = Paraqitja > Paneli > Ndrysho madhësinë djathtas
command-resize-pane-up = Paraqitja > Paneli > Ndrysho madhësinë sipër
command-resize-pane-down = Paraqitja > Paneli > Ndrysho madhësinë poshtë
command-stack-close = Paraqitja > Stiva > Mbyll stivën
command-stack-next = Paraqitja > Stiva > Stiva tjetër
command-stack-previous = Paraqitja > Stiva > Stiva e mëparshme
command-stack-reopen = Paraqitja > Stiva > Rihap faqen e mbyllur
command-stack-swap-prev = Paraqitja > Stiva > Zhvendos stivën majtas
command-stack-swap-next = Paraqitja > Stiva > Zhvendos stivën djathtas
command-space-open = Paraqitja > Hapësira > Hapësirat
command-terminal-close = Terminali > Mbyll terminalin
command-terminal-next = Terminali > Terminali tjetër
command-terminal-prev = Terminali > Terminali i mëparshëm
command-terminal-clear = Terminali > Pastro terminalin
command-browser-prev-page = Shfletuesi > Navigimi > Prapa
command-browser-next-page = Shfletuesi > Navigimi > Përpara
command-browser-reload = Shfletuesi > Navigimi > Ringarko
command-browser-hard-reload = Shfletuesi > Navigimi > Ringarkim i plotë
command-open-in-place = Shfletuesi > Hap > Hap këtu
command-open-in-new-stack = Shfletuesi > Hap > Hap në stivë të re
command-open-in-pane-top = Shfletuesi > Hap > Hap në panel sipër
command-open-in-pane-right = Shfletuesi > Hap > Hap në panel djathtas
command-open-in-pane-bottom = Shfletuesi > Hap > Hap në panel poshtë
command-open-in-pane-left = Shfletuesi > Hap > Hap në panel majtas
command-open-in-new-tab = Shfletuesi > Hap > Hap në skedë të re
command-open-in-new-space = Shfletuesi > Hap > Hap në hapësirë të re
command-browser-zoom-in = Shfletuesi > Pamja > Zmadho
command-browser-zoom-out = Shfletuesi > Pamja > Zvogëlo
command-browser-zoom-reset = Shfletuesi > Pamja > Madhësia reale
command-browser-dev-tools = Shfletuesi > Pamja > Veglat e zhvilluesit
command-browser-open-command-bar = Shfletuesi > Shiriti > Shiriti i komandave
command-browser-open-page-in-command-bar = Shfletuesi > Shiriti > Redakto faqen
command-browser-open-path-bar = Shfletuesi > Shiriti > Naviguesi i shtegut
command-browser-open-commands = Shfletuesi > Shiriti > Komandat
command-browser-open-history = Shfletuesi > Shiriti > Historiku
command-service-open = Shërbimi > Hap monitorin e shërbimeve
command-bookmark-toggle-active = Faqeshënuesi > Shëno faqen
command-bookmark-pin-active = Faqeshënuesi > Fikso faqen

layout-tab = Skedë
layout-no-stacks = Nuk ka stiva
layout-loading = Po ngarkohet…
layout-no-markdown-files = Nuk ka skedarë Markdown
layout-empty-folder = Dosje bosh
layout-worktree = worktree
layout-folder-name = Emri i dosjes
layout-no-pins-bookmarks = Nuk ka të fiksuara ose faqeshënues
layout-move-to = Zhvendos te { $folder }
layout-bookmark-current-page = Shëno faqen aktuale
layout-rename-folder = Riemërto dosjen
layout-remove-folder = Hiq dosjen
layout-update-downloading = Po shkarkohet përditësimi
layout-update-installing = Po instalohet përditësimi…
layout-update-ready = Ka version të ri
layout-restart-update = Rinis për ta përditësuar

agent-preparing = Po përgatitet agjenti…
agent-send-all-queued = Dërgo tani të gjitha kërkesat në radhë (Esc)
agent-send = Dërgo (Enter)
agent-ready = Gati kur të jeni.
agent-loading-older = Po ngarkohen mesazhet më të vjetra…
agent-load-older = Ngarko mesazhe më të vjetra
agent-continued-from = Vazhduar nga { $source }
agent-older-context-omitted = konteksti më i vjetër u la jashtë
agent-interrupted = u ndërpre
agent-allow-tool = Të lejohet { $tool }?
agent-deny = Refuzo
agent-allow-always = Lejo gjithmonë
agent-allow = Lejo
agent-loading-sessions = Po ngarkohen sesionet…
agent-no-resumable-sessions = Nuk u gjetën sesione për vazhdim
agent-no-matching-sessions = Nuk ka sesione që përputhen
agent-no-matching-models = Nuk ka modele që përputhen
agent-choice-help = ↑/↓ ose Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Zgjidh dosjen e repos
agent-choose-repository-detail = Zgjidh repon lokale Git që duhet të përdorë agjenti.
agent-choosing = Po zgjidhet…
agent-choose-folder = Zgjidh dosje
agent-queued = në radhë
agent-attached = Bashkëngjitur:
agent-cancel-queued = Anulo kërkesën në radhë
agent-resume-queued = Vazhdo kërkesat në radhë
agent-clear-queue = Pastro radhën
agent-send-all-now = dërgo të gjitha tani
agent-choose-option = Zgjidh një opsion më sipër
agent-loading-media = Po ngarkohet media…
agent-no-matching-media = Nuk ka media që përputhet
agent-prompt-context = Konteksti i kërkesës
agent-details = Detaje
agent-path = Shtegu
agent-tool = Vegla
agent-server = Serveri
agent-bytes = { $count } bajte
agent-worked-for = Punoi për { $duration }
agent-worked-for-steps = { $count ->
    [one] Punoi për { $duration } · 1 hap
   *[other] Punoi për { $duration } · { $count } hapa
}
agent-tool-guardian-review = Rishikim Guardian
agent-tool-read-files = Lexoi skedarë
agent-tool-viewed-image = Pa imazh
agent-tool-used-browser = Përdori shfletuesin
agent-tool-searched-files = Kërkoi në skedarë
agent-tool-ran-commands = Ekzekutoi komanda
agent-thinking = Po mendon
agent-subagent = Nënagjent
agent-prompt = Kërkesë
agent-thread = Fill
agent-parent = Prindi
agent-children = Fëmijët
agent-call = Thirrje
agent-raw-event = Ngjarje bruto
agent-plan = Plan
agent-tasks = { $count ->
    [one] 1 detyrë
   *[other] { $count } detyra
}
agent-edited = Redaktuar
agent-reconnecting = Po rilidhet { $attempt }/{ $total }
agent-status-running = Në ekzekutim
agent-status-done = Përfunduar
agent-status-failed = Dështoi
agent-status-pending = Në pritje
agent-slash-attach-files = Bashkëngjit skedarë
agent-slash-resume-session = Vazhdo një sesion të mëparshëm
agent-slash-select-model = Zgjidh modelin
agent-slash-continue-cli = Vazhdo këtë sesion në CLI
agent-session-just-now = sapo
agent-session-minutes-ago = { $count }m më parë
agent-session-hours-ago = { $count }h më parë
agent-session-days-ago = { $count }d më parë
agent-working-working = Po punon
agent-working-thinking = Po mendon
agent-working-pondering = Po e shqyrton
agent-working-noodling = Po e përtyp
agent-working-percolating = Po përpunohet
agent-working-conjuring = Po sajon
agent-working-cooking = Po gatuan
agent-working-brewing = Po zihet
agent-working-musing = Po mediton
agent-working-ruminating = Po e ripërtyp
agent-working-scheming = Po planifikon
agent-working-synthesizing = Po sintetizon
agent-working-tinkering = Po eksperimenton
agent-working-churning = Po përpunon
agent-working-vibing = Po kap ritmin
agent-working-simmering = Po zien ngadalë
agent-working-crafting = Po ndërton
agent-working-divining = Po zbulon
agent-working-mulling = Po e mendon
agent-working-spelunking = Po gërmon

editor-toggle-explorer = Shfaq/fshih Explorer (Cmd+B)
editor-unsaved = i paruajtur
editor-rendered-markdown = Markdown i shfaqur me redaktim live
editor-note = Shënim
editor-source-editor = Redaktues burimi
editor-editor = Redaktues
editor-git-diff = Diff Git
editor-diff = Diff
editor-tidy = Pastro
editor-always = Gjithmonë
editor-unchanged-previews = { $count ->
    [one] ✦ 1 pamje paraprake e pandryshuar
   *[other] ✦ { $count } pamje paraprake të pandryshuara
}
editor-open-externally = Hap jashtë aplikacionit
editor-changed-line = Rresht i ndryshuar
editor-go-to-definition = Shko te përkufizimi
editor-find-references = Gjej referenca
editor-references = { $count ->
    [one] 1 referencë
   *[other] { $count } referenca
}
editor-lsp-starting = { $server } po niset…
editor-lsp-not-installed = { $server } — nuk është instaluar
editor-explorer = Explorer
editor-open-editors = Redaktuesit e hapur
editor-outline = Përmbledhje
editor-new-file = Skedar i ri
editor-new-folder = Dosje e re
editor-delete-confirm = Të fshihet “{ $name }”? Kjo nuk mund të zhbëhet.
editor-created-folder = U krijua dosja { $name }
editor-created-file = U krijua skedari { $name }
editor-renamed-to = U riemërtua në { $name }
editor-deleted = U fshi { $name }
editor-failed-decode-image = Imazhi nuk u dekodua
editor-preview-large-image = imazh (shumë i madh për pamje paraprake)
editor-preview-binary = binar
editor-preview-file = skedar

git-status-clean = i pastër
git-status-modified = i ndryshuar
git-status-staged = në stage
git-status-staged-modified = në stage*
git-status-untracked = i pagjurmuar
git-status-deleted = i fshirë
git-status-conflict = konflikt
git-accept-all = ✓ prano të gjitha
git-unstage = Hiq nga stage
git-confirm-deny-all = Konfirmo refuzimin e të gjithave
git-deny-all = ✗ refuzo të gjitha
git-commit-message = mesazh commit-i
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Po ngarkohet diff-i…
git-no-changes = Nuk ka ndryshime për t’u shfaqur
git-accept = ✓ prano
git-deny = ✗ refuzo
git-show-unchanged-lines = Shfaq { $count } rreshta të pandryshuar

terminal-loading = Po ngarkohet…
terminal-runs-when-ready = ekzekutohet kur të jetë gati · Ctrl+C pastron · Esc kapërcen
terminal-booting = po niset
terminal-type-command = shkruaj një komandë · ekzekutohet kur të jetë gati · Esc kapërcen

setup-tagline-claude = Agjenti i kodimit i Anthropic, në Vmux
setup-tagline-codex = Agjenti i kodimit i OpenAI, në Vmux
setup-tagline-vibe = Agjenti i kodimit i Mistral, në Vmux
setup-install-title = Instalo CLI-në { $name }
setup-homebrew-required = Homebrew kërkohet për të instaluar { $command } dhe ende nuk është konfiguruar. Vmux do të instalojë fillimisht Homebrew, pastaj { $name }.
setup-terminal-instructions = Në terminal, shtyp Return për të nisur, pastaj fut fjalëkalimin e Mac-ut kur të kërkohet.
setup-command-missing = Vmux e hapi këtë faqe sepse komanda lokale { $command } nuk është instaluar ende. Ekzekuto komandën më poshtë për ta marrë.
setup-install-failed = Instalimi nuk përfundoi. Kontrollo terminalin për detaje, pastaj provo përsëri.
setup-installing = Po instalohet…
setup-install-homebrew = Instalo Homebrew + { $name }
setup-run-install = Ekzekuto komandën e instalimit
setup-auto-reload = Vmux e ekzekuton në terminal dhe ringarkohet kur { $command } të jetë gati.

debug-title = Diagnostikim
debug-auto-update = Përditësim automatik
debug-simulate-update = Simulo përditësim të disponueshëm
debug-simulate-download = Simulo shkarkim
debug-clear-update = Pastro përditësimin
debug-trigger-restart = Shkakto rinisje

command-manage-spaces = Menaxho hapësirat…
command-pane-stack-location = paneli { $pane } / stiva { $stack }
command-space-pane-stack-location = { $space } / paneli { $pane } / stiva { $stack }
command-terminal-path = Terminali ({ $path })
command-group-interactive-mode = Modaliteti ndërveprues
command-group-window = Dritarja
command-group-tab = Skeda
command-group-pane = Paneli
command-group-stack = Stiva
command-group-space = Hapësira
command-group-navigation = Navigimi
command-group-open = Hap
command-group-view = Pamja
command-group-bar = Shiriti

menu-close-vmux = Mbyll Vmux

agents-terminal-coding-agent = Agjent kodimi me bazë terminali
