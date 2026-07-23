locale-name = slovenščina
common-open = Odpri
common-close = Zapri
common-install = Namesti
common-uninstall = Odstrani
common-update = Posodobi
common-retry = Poskusi znova
common-refresh = Osveži
common-remove = Odstrani
common-enable = Omogoči
common-disable = Onemogoči
common-new = Novo
common-active = aktivno
common-running = se izvaja
common-done = končano
common-failed = Spodletelo
common-installed = Nameščeno
common-items = { $count ->
    [one] { $count } element
   *[other] { $count } elementov
}

tools-title = Orodja
tools-search = Iskanje paketov, agentov, MCP, jezikovnih orodij in nastavitvenih datotek…
tools-open = Odpri orodja
tools-fold = Strni orodja
tools-unfold = Razširi orodja
tools-scanning = Pregledovanje lokalnih orodij…
tools-no-installed = Ni nameščenih orodij
tools-empty = Ni ustreznih orodij
tools-empty-detail = Namestite paket ali dodajte paket nastavitvenih datotek v slogu Stow.
tools-apply = Uporabi
tools-homebrew = Homebrew
tools-homebrew-sync = Nameščene formule in aplikacije se samodejno sinhronizirajo.
tools-open-brewfile = Odpri Brewfile
tools-managed = upravljano
tools-provider-homebrew-formulae = Formule Homebrew
tools-provider-homebrew-casks = Aplikacije Homebrew
tools-provider-npm = Paketi npm
tools-provider-acp-agents = Agenti ACP
tools-provider-language-tools = Jezikovna orodja
tools-provider-mcp-servers = Strežniki MCP
tools-provider-dotfiles = Nastavitvene datoteke
tools-status-available = Na voljo
tools-status-missing = Manjka
tools-status-conflict = Spor
tools-forget = Pozabi
tools-manage = Upravljaj
tools-link = Poveži
tools-unlink = Prekini povezavo
tools-import = Uvozi
tools-update-count = { $count ->
    [one] 1 posodobitev
   *[other] { $count } posodobitev
}
tools-conflict-count = { $count ->
    [one] 1 spor
   *[other] { $count } sporov
}
tools-result-applied = Orodja uporabljena
tools-result-imported = Orodja uvožena
tools-result-installed = { $name } nameščen
tools-result-updated = { $name } posodobljen
tools-result-uninstalled = { $name } odstranjen
tools-result-forgotten = { $name } pozabljen
tools-result-managed = { $name } je zdaj upravljan
tools-result-linked = { $name } povezan
tools-result-unlinked = Povezava z { $name } prekinjena
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Sinhronizirajte nastavitve, orodja, datoteke dot in znanje z Gitom.
vault-sync = Sinhronizacija
vault-create = Ustvari
vault-connect = Povežite se
vault-private = Zasebno skladišče
vault-public-warning = Javna skladišča razkrivajo vaše znanje in konfiguracijo.
vault-choose-repository = Izberite skladišče ...
vault-empty = prazno
vault-clean = Ažurno
vault-not-connected = Ni povezano
vault-change-count = Spremembe: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Začetek
start-tagline = En prompt. Vse opravljeno.

agents-title = Agenti
agents-search = Išči agente ACP in CLI …
agents-empty = Ni ujemajočih se agentov
agents-empty-detail = Poskusite z imenom, izvajalnim okoljem ali ACP/CLI.
agents-install-failed = Namestitev ni uspela
agents-updating = Posodabljanje …
agents-retrying = Ponovni poskus …
agents-preparing = Priprava …

extensions-title = Razširitve
extensions-search = Išči med nameščenimi ali v Chrome Web Store …
extensions-relaunch = Znova zaženi za uveljavitev
extensions-empty = Ni nameščenih razširitev
extensions-no-match = Ni ujemajočih se razširitev
extensions-empty-detail = Zgoraj poiščite v Chrome Web Store in pritisnite Enter.
extensions-no-match-detail = Poskusite z drugim imenom ali ID-jem razširitve.
extensions-on = Vklopljeno
extensions-off = Izklopljeno
extensions-enable-confirm = Omogočim { $name }?
extensions-enable-permissions = Omogoči { $name } in dovoli:

lsp-title = Jezikovni strežniki
lsp-search = Išči jezikovne strežnike, linterje, oblikovalnike …
lsp-loading = Nalaganje kataloga …
lsp-empty = Ni ujemajočih se jezikovnih strežnikov
lsp-empty-detail = Poskusite z drugim jezikom, linterjem ali oblikovalnikom.
lsp-needs = potrebuje { $tool }
lsp-status-available = Na voljo
lsp-status-on-path = Na PATH
lsp-status-installing = Nameščanje …
lsp-status-installed = Nameščeno
lsp-status-outdated = Na voljo je posodobitev
lsp-status-running = Se izvaja
lsp-status-failed = Spodletelo

spaces-title = Prostori
spaces-new-placeholder = Ime novega prostora
spaces-empty = Ni prostorov
spaces-default-name = Prostor { $number }
spaces-tabs = { $count ->
    [one] 1 zavihek
   *[other] { $count } zavihkov
}
spaces-delete = Izbriši prostor

team-title = Ekipa
team-just-you = V tem prostoru ste samo vi
team-agents = { $count ->
    [one] Vi in 1 agent
   *[other] Vi in { $count } agentov
}
team-empty = Tukaj še ni nikogar
team-you = Vi
team-agent = Agent

services-title = Storitve v ozadju
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procesov
}
services-kill-all = Prisili končanje vseh
services-not-running = Storitev se ne izvaja
services-start-with = Zaženi z:
services-empty = Ni aktivnih procesov
services-filter = Filtriraj procese …
services-no-match = Ni ujemajočih se procesov
services-connected = Povezano
services-disconnected = Prekinjena povezava
services-attached = pripeto
services-kill = Prisili končanje
services-memory = Pomnilnik
services-size = Velikost
services-shell = Lupina

error-title = Napaka

history-search = Išči po zgodovini
history-clear-all = Počisti vse
history-clear-confirm = Počistim vso zgodovino?
history-clear-warning = Tega ni mogoče razveljaviti.
history-cancel = Prekliči
history-today = Danes
history-yesterday = Včeraj
history-days-ago = pred { $count } dnevi
history-day-offset = Dan -{ $count }

settings-title = Nastavitve
settings-loading = Nalaganje nastavitev …
settings-stored = Shranjeno v ~/.vmux/settings.ron
settings-other = Drugo
settings-software-update = Posodobitev programske opreme
settings-check-updates = Preveri posodobitve
settings-check-updates-hint = Ob omogočenih samodejnih posodobitvah preveri ob zagonu in vsako uro.
settings-update-unavailable = Ni na voljo
settings-update-unavailable-hint = Posodabljalnik ni vključen v to gradnjo.
settings-update-checking = Preverjanje …
settings-update-checking-hint = Preverjanje posodobitev …
settings-update-check-again = Preveri znova
settings-update-current = Vmux je posodobljen.
settings-update-downloading = Prenašanje …
settings-update-downloading-hint = Prenašanje Vmux { $version } …
settings-update-installing = Nameščanje …
settings-update-installing-hint = Nameščanje Vmux { $version } …
settings-update-ready = Posodobitev je pripravljena
settings-update-ready-hint = Vmux { $version } je pripravljen. Znova zaženite za uveljavitev.
settings-update-try-again = Poskusi znova
settings-update-failed = Posodobitev ni bilo mogoče preveriti.
settings-item = Element
settings-item-number = Element { $number }
settings-press-key = Pritisnite tipko …
settings-saved = Shranjeno
settings-record-key = Kliknite za snemanje nove kombinacije tipk

tray-open-window = Odpri okno
tray-close-window = Zapri okno
tray-pause-recording = Začasno ustavi snemanje
tray-resume-recording = Nadaljuj snemanje
tray-finish-recording = Končaj snemanje
tray-quit = Zapri Vmux

composer-attach-files = Priloži datoteke (/upload)
composer-remove-attachment = Odstrani prilogo

layout-back = Nazaj
layout-forward = Naprej
layout-reload = Znova naloži
layout-bookmark-page = Dodaj stran med zaznamke
layout-remove-bookmark = Odstrani zaznamek
layout-pin-page = Pripni to stran
layout-unpin-page = Odpni to stran
layout-manage-extensions = Upravljaj razširitve
layout-new-stack = Nov sklad
layout-close-tab = Zapri zavihek
layout-bookmark = Zaznamek
layout-pin = Pripni
layout-new-tab = Nov zavihek
layout-team = Ekipa

command-switch-space = Preklopi prostor …
command-search-ask = Išči ali vprašaj …
command-new-tab-placeholder = Iščite, vnesite URL ali izberite Terminal …
command-placeholder = Vnesite URL, poiščite zavihke ali > za ukaze …
command-composer-placeholder = Vnesite / za ukaze ali @ za predstavnost
command-send = Pošlji (Enter)
command-terminal = Terminal
command-open-terminal = Odpri v Terminalu
command-stack = Sklad
command-tabs = { $count ->
    [one] 1 zavihek
   *[other] { $count } zavihkov
}
command-prompt = Prompt
command-new-tab = Nov zavihek
command-search = Išči
command-open-value = Odpri »{ $value }«
command-search-value = Išči »{ $value }«

schema-appearance = Videz
schema-general = Splošno
schema-layout = Postavitev
schema-layout-detail = Okno, podokna, stranska vrstica in obroč fokusa.
schema-agent = Agent
schema-agent-detail = Vedenje agenta in dovoljenja za orodja.
schema-shortcuts = Bližnjice
schema-shortcuts-detail = Pogled samo za branje. Za spremembo vezav neposredno uredite settings.ron.
schema-terminal = Terminal
schema-browser = Brskalnik
schema-mode = Način
schema-mode-detail = Barvna shema za spletne strani. Naprava sledi sistemu.
schema-device = Naprava
schema-light = Svetlo
schema-dark = Temno
schema-language = Jezik
schema-language-detail = Uporabite sistem, en-US, ja ali katero koli oznako BCP 47 z ustreznim katalogom ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Samodejno posodabljanje
schema-auto-update-detail = Preveri in namesti posodobitve ob zagonu in vsako uro.
schema-startup-url = Začetni URL
schema-startup-url-detail = Prazno odpre prompt ukazne vrstice.
schema-search-engine = Iskalnik
schema-search-engine-detail = Uporablja se za spletna iskanja iz Začetka in ukazne vrstice.
schema-window = Okno
schema-pane = Podokno
schema-side-sheet = Stranski list
schema-focus-ring = Obroč fokusa
schema-run-placement = Dovoli preglasitev postavitve izvajanja
schema-run-placement-detail = Dovoli agentom izbrati način, smer in sidro podokna za izvajanje.
schema-leader = Vodilna tipka
schema-leader-detail = Predponska tipka za akordne bližnjice.
schema-chord-timeout = Časovna omejitev akorda
schema-chord-timeout-detail = Milisekunde, preden predpona akorda poteče.
schema-bindings = Vezave
schema-confirm-close = Potrdi zapiranje
schema-confirm-close-detail = Pred zapiranjem terminala s procesom v teku prikaži poziv.
schema-default-theme = Privzeta tema
schema-default-theme-detail = Ime aktivne teme s seznama tem.

settings-empty = (prazno)
settings-none = (brez)

schema-system = Sistem
schema-editor = Urejevalnik
schema-recording = Snemanje
schema-radius = Polmer
schema-padding = Odmik
schema-gap = Razmik
schema-width = Širina
schema-color = Barva
schema-red = Rdeča
schema-green = Zelena
schema-blue = Modra
schema-follow-files = Sledi datotekam
schema-tidy-files = Pospravi datoteke
schema-tidy-files-max = Prag za pospravljanje datotek
schema-tidy-files-auto = Samodejno pospravi datoteke
schema-app-providers = Ponudniki aplikacij
schema-provider = Ponudnik
schema-kind = Vrsta
schema-models = Modeli
schema-acp = Agenti ACP
schema-id = ID
schema-name = Ime
schema-command = Ukaz
schema-arguments = Argumenti
schema-environment = Okolje
schema-working-directory = Delovni imenik
schema-shell = Lupina
schema-font-family = Družina pisave
schema-startup-directory = Začetni imenik
schema-themes = Teme
schema-color-scheme = Barvna shema
schema-font-size = Velikost pisave
schema-line-height = Višina vrstice
schema-cursor-style = Slog kazalke
schema-cursor-blink = Utripanje kazalke
schema-custom-themes = Teme po meri
schema-foreground = Ospredje
schema-background = Ozadje
schema-cursor = Kazalka
schema-ansi-colors = Barve ANSI
schema-keymap = Razpored tipk
schema-explorer = Raziskovalec
schema-visible = Vidno
schema-language-servers = Jezikovni strežniki
schema-servers = Strežniki
schema-language-id = ID jezika
schema-root-markers = Oznake korena
schema-output-directory = Izhodni imenik

menu-scene = Scena
menu-layout = Postavitev
menu-terminal = Terminal
menu-browser = Brskalnik
menu-service = Storitev
menu-bookmark = Zaznamek
menu-edit = Uredi

layout-knowledge = Znanje
layout-open-knowledge = Odpri znanje
layout-open-welcome-knowledge = Odpri Dobrodošli v znanju
layout-open-path = Odpri { $path }
layout-fold-knowledge = Strni znanje
layout-unfold-knowledge = Razširi znanje
layout-bookmarks = Zaznamki
layout-new-folder = Nova mapa
layout-add-to-bookmarks = Dodaj med zaznamke
layout-move-to-bookmarks = Premakni med zaznamke
layout-stack-number = Sklad { $number }
layout-fold-stack = Strni sklad
layout-unfold-stack = Razširi sklad
layout-close-stack = Zapri sklad
layout-bookmark-in = Zaznamek v { $folder }

common-cancel = Prekliči
common-delete = Izbriši
common-save = Shrani
common-rename = Preimenuj
common-expand = Razširi
common-collapse = Strni
common-loading = Nalaganje …
common-error = Napaka
common-output = Izhod
common-pending = V teku
common-current = trenutno
common-stop = Ustavi
services-command = Storitev Vmux
services-uptime-seconds = { $seconds } s
services-uptime-minutes = { $minutes } min { $seconds } s
services-uptime-hours = { $hours } h { $minutes } min
services-uptime-days = { $days } d { $hours } h

error-page-failed-load = Strani ni bilo mogoče naložiti
error-page-not-found = Strani ni bilo mogoče najti
error-unknown-host = Neznan gostitelj aplikacije Vmux: { $host }

history-title = Zgodovina

command-new-app-chat = Nov klepet { $provider }/{ $model } (aplikacija)
command-interactive-mode-user = Scena > Interaktivni način > Uporabnik
command-interactive-mode-player = Scena > Interaktivni način > Predvajalnik
command-minimize-window = Postavitev > Okno > Minimiziraj
command-toggle-layout = Postavitev > Postavitev > Preklopi postavitev
command-close-tab = Postavitev > Zavihek > Zapri zavihek
command-new-task = Postavitev > Zavihek > Novo opravilo …
command-next-tab = Postavitev > Zavihek > Naslednji zavihek
command-prev-tab = Postavitev > Zavihek > Prejšnji zavihek
command-rename-tab = Postavitev > Zavihek > Preimenuj zavihek
command-tab-select-1 = Postavitev > Zavihek > Izberi zavihek 1
command-tab-select-2 = Postavitev > Zavihek > Izberi zavihek 2
command-tab-select-3 = Postavitev > Zavihek > Izberi zavihek 3
command-tab-select-4 = Postavitev > Zavihek > Izberi zavihek 4
command-tab-select-5 = Postavitev > Zavihek > Izberi zavihek 5
command-tab-select-6 = Postavitev > Zavihek > Izberi zavihek 6
command-tab-select-7 = Postavitev > Zavihek > Izberi zavihek 7
command-tab-select-8 = Postavitev > Zavihek > Izberi zavihek 8
command-tab-select-last = Postavitev > Zavihek > Izberi zadnji zavihek
command-close-pane = Postavitev > Podokno > Zapri podokno
command-select-pane-left = Postavitev > Podokno > Izberi levo podokno
command-select-pane-right = Postavitev > Podokno > Izberi desno podokno
command-select-pane-up = Postavitev > Podokno > Izberi zgornje podokno
command-select-pane-down = Postavitev > Podokno > Izberi spodnje podokno
command-swap-pane-prev = Postavitev > Podokno > Zamenjaj s prejšnjim podoknom
command-swap-pane-next = Postavitev > Podokno > Zamenjaj z naslednjim podoknom
command-equalize-pane-size = Postavitev > Podokno > Izenači velikost podoken
command-resize-pane-left = Postavitev > Podokno > Spremeni velikost podokna levo
command-resize-pane-right = Postavitev > Podokno > Spremeni velikost podokna desno
command-resize-pane-up = Postavitev > Podokno > Spremeni velikost podokna gor
command-resize-pane-down = Postavitev > Podokno > Spremeni velikost podokna dol
command-stack-close = Postavitev > Sklad > Zapri sklad
command-stack-next = Postavitev > Sklad > Naslednji sklad
command-stack-previous = Postavitev > Sklad > Prejšnji sklad
command-stack-reopen = Postavitev > Sklad > Znova odpri zaprto stran
command-stack-swap-prev = Postavitev > Sklad > Premakni sklad levo
command-stack-swap-next = Postavitev > Sklad > Premakni sklad desno
command-space-open = Postavitev > Prostor > Prostori
command-terminal-close = Terminal > Zapri terminal
command-terminal-next = Terminal > Naslednji terminal
command-terminal-prev = Terminal > Prejšnji terminal
command-terminal-clear = Terminal > Počisti terminal
command-browser-prev-page = Brskalnik > Navigacija > Nazaj
command-browser-next-page = Brskalnik > Navigacija > Naprej
command-browser-reload = Brskalnik > Navigacija > Ponovno naloži
command-browser-hard-reload = Brskalnik > Navigacija > Prisilno ponovno naloži
command-open-in-place = Brskalnik > Odpri > Odpri tukaj
command-open-in-new-stack = Brskalnik > Odpri > Odpri v novem skladu
command-open-in-pane-top = Brskalnik > Odpri > Odpri v zgornjem podoknu
command-open-in-pane-right = Brskalnik > Odpri > Odpri v desnem podoknu
command-open-in-pane-bottom = Brskalnik > Odpri > Odpri v spodnjem podoknu
command-open-in-pane-left = Brskalnik > Odpri > Odpri v levem podoknu
command-open-in-new-tab = Brskalnik > Odpri > Odpri v novem zavihku
command-open-in-new-space = Brskalnik > Odpri > Odpri v novem prostoru
command-browser-zoom-in = Brskalnik > Pogled > Povečaj
command-browser-zoom-out = Brskalnik > Pogled > Pomanjšaj
command-browser-zoom-reset = Brskalnik > Pogled > Dejanska velikost
command-browser-dev-tools = Brskalnik > Pogled > Orodja za razvijalce
command-browser-open-command-bar = Brskalnik > Vrstica > Ukazna vrstica
command-browser-open-page-in-command-bar = Brskalnik > Vrstica > Uredi stran
command-browser-open-path-bar = Brskalnik > Vrstica > Navigator poti
command-browser-open-commands = Brskalnik > Vrstica > Ukazi
command-browser-open-history = Brskalnik > Vrstica > Zgodovina
command-service-open = Storitev > Odpri nadzornik storitev
command-bookmark-toggle-active = Zaznamek > Dodaj stran med zaznamke
command-bookmark-pin-active = Zaznamek > Pripni stran

layout-tab = Zavihek
layout-no-stacks = Ni skladov
layout-loading = Nalaganje …
layout-no-markdown-files = Ni datotek Markdown
layout-empty-folder = Prazna mapa
layout-worktree = delovno drevo
layout-folder-name = Ime mape
layout-no-pins-bookmarks = Ni pripetih strani ali zaznamkov
layout-move-to = Premakni v { $folder }
layout-bookmark-current-page = Dodaj trenutno stran med zaznamke
layout-rename-folder = Preimenuj mapo
layout-remove-folder = Odstrani mapo
layout-update-downloading = Prenašanje posodobitve
layout-update-installing = Nameščanje posodobitve …
layout-update-ready = Na voljo je nova različica
layout-restart-update = Znova zaženi za posodobitev

agent-preparing = Pripravljanje agenta …
agent-send-all-queued = Pošlji vse čakajoče pozive zdaj (Esc)
agent-send = Pošlji (Enter)
agent-ready = Pripravljen, ko ste vi.
agent-loading-older = Nalaganje starejših sporočil …
agent-load-older = Naloži starejša sporočila
agent-continued-from = Nadaljevano iz { $source }
agent-older-context-omitted = starejši kontekst izpuščen
agent-interrupted = prekinjeno
agent-allow-tool = Dovolite { $tool }?
agent-deny = Zavrni
agent-allow-always = Vedno dovoli
agent-allow = Dovoli
agent-loading-sessions = Nalaganje sej …
agent-no-resumable-sessions = Ni nadaljevalnih sej
agent-no-matching-sessions = Ni ujemajočih se sej
agent-no-matching-models = Ni ujemajočih se modelov
agent-choice-help = ↑/↓ ali Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Izberite mapo repozitorija
agent-choose-repository-detail = Izberite lokalni repozitorij Git, ki ga naj agent uporabi.
agent-choosing = Izbiranje …
agent-choose-folder = Izberite mapo
agent-queued = v čakalni vrsti
agent-attached = Priloženo:
agent-cancel-queued = Prekliči čakajoči poziv
agent-resume-queued = Nadaljuj čakajoče pozive
agent-clear-queue = Počisti čakalno vrsto
agent-send-all-now = pošlji vse zdaj
agent-choose-option = Izberite možnost zgoraj
agent-loading-media = Nalaganje predstavnosti …
agent-no-matching-media = Ni ujemajoče se predstavnosti
agent-prompt-context = Kontekst poziva
agent-details = Podrobnosti
agent-path = Pot
agent-tool = Orodje
agent-server = Strežnik
agent-bytes = { $count } bajtov
agent-worked-for = Delal { $duration }
agent-worked-for-steps = { $count ->
    [one] Delal { $duration } · 1 korak
   *[other] Delal { $duration } · { $count } korakov
}
agent-tool-guardian-review = Pregled varuha
agent-tool-read-files = Prebral datoteke
agent-tool-viewed-image = Ogledal si je sliko
agent-tool-used-browser = Uporabil brskalnik
agent-tool-searched-files = Iskal po datotekah
agent-tool-ran-commands = Zagnal ukaze
agent-thinking = Razmišlja
agent-subagent = Podagent
agent-prompt = Poziv
agent-thread = Nit
agent-parent = Nadrejeno
agent-children = Podrejeno
agent-call = Klic
agent-raw-event = Surovi dogodek
agent-plan = Načrt
agent-tasks = { $count ->
    [one] 1 opravilo
   *[other] { $count } opravil
}
agent-edited = Urejeno
agent-reconnecting = Ponovno povezovanje { $attempt }/{ $total }
agent-status-running = Se izvaja
agent-status-done = Končano
agent-status-failed = Spodletelo
agent-status-pending = V teku
agent-slash-attach-files = Priloži datoteke
agent-slash-resume-session = Nadaljuj preteklo sejo
agent-slash-select-model = Izberi model
agent-slash-continue-cli = Nadaljuj to sejo v CLI
agent-session-just-now = pravkar
agent-session-minutes-ago = pred { $count } min
agent-session-hours-ago = pred { $count } h
agent-session-days-ago = pred { $count } d
agent-working-working = Dela
agent-working-thinking = Razmišlja
agent-working-pondering = Premišljuje
agent-working-noodling = Tuhta
agent-working-percolating = Zori
agent-working-conjuring = Čara
agent-working-cooking = Kuha
agent-working-brewing = Vari
agent-working-musing = Razglablja
agent-working-ruminating = Premleva
agent-working-scheming = Snova
agent-working-synthesizing = Sintetizira
agent-working-tinkering = Popravlja
agent-working-churning = Melje
agent-working-vibing = Je v elementu
agent-working-simmering = Počasi kuha
agent-working-crafting = Sestavlja
agent-working-divining = Vedežuje
agent-working-mulling = Tuhta
agent-working-spelunking = Brska po globinah

editor-toggle-explorer = Preklopi Raziskovalca (Cmd+B)
editor-unsaved = neshranjeno
editor-rendered-markdown = Izrisan Markdown z urejanjem v živo
editor-note = Opomba
editor-source-editor = Urejevalnik izvorne kode
editor-editor = Urejevalnik
editor-git-diff = Razlika Git
editor-diff = Razlika
editor-tidy = Pospravi
editor-always = Vedno
editor-unchanged-previews = { $count ->
    [one] ✦ 1 nespremenjen predogled
   *[other] ✦ { $count } nespremenjenih predogledov
}
editor-open-externally = Odpri zunanje
editor-changed-line = Spremenjena vrstica
editor-go-to-definition = Pojdi na definicijo
editor-find-references = Poišči sklice
editor-references = { $count ->
    [one] 1 sklic
   *[other] { $count } sklicev
}
editor-lsp-starting = { $server } se zaganja …
editor-lsp-not-installed = { $server } — ni nameščen
editor-explorer = Raziskovalec
editor-open-editors = Odprti urejevalniki
editor-outline = Oris
editor-new-file = Nova datoteka
editor-new-folder = Nova mapa
editor-delete-confirm = Izbrišem »{ $name }«? Tega ni mogoče razveljaviti.
editor-created-folder = Ustvarjena mapa { $name }
editor-created-file = Ustvarjena datoteka { $name }
editor-renamed-to = Preimenovano v { $name }
editor-deleted = Izbrisano { $name }
editor-failed-decode-image = Slike ni bilo mogoče dekodirati
editor-preview-large-image = slika (prevelika za predogled)
editor-preview-binary = binarno
editor-preview-file = datoteka

git-status-clean = čisto
git-status-modified = spremenjeno
git-status-staged = pripravljeno
git-status-staged-modified = pripravljeno*
git-status-untracked = nesledeno
git-status-deleted = izbrisano
git-status-conflict = spor
git-accept-all = ✓ sprejmi vse
git-unstage = Odstrani iz priprave
git-confirm-deny-all = Potrdi zavrnitev vseh
git-deny-all = ✗ zavrni vse
git-commit-message = sporočilo commita
git-commit = Commit ({ $count })
git-push = ↑ Potisni
git-loading-diff = Nalaganje razlike …
git-no-changes = Ni sprememb za prikaz
git-accept = ✓ sprejmi
git-deny = ✗ zavrni
git-show-unchanged-lines = Pokaži { $count } nespremenjenih vrstic

terminal-loading = Nalaganje …
terminal-runs-when-ready = zažene se, ko je pripravljeno · Ctrl+C počisti · Esc preskoči
terminal-booting = zaganjanje
terminal-type-command = vnesite ukaz · zažene se, ko je pripravljeno · Esc preskoči

setup-tagline-claude = Anthropicov kodirni agent v Vmuxu
setup-tagline-codex = OpenAI-jev kodirni agent v Vmuxu
setup-tagline-vibe = Mistralov kodirni agent v Vmuxu
setup-install-title = Namesti CLI { $name }
setup-homebrew-required = Za namestitev { $command } je potreben Homebrew, ki še ni nastavljen. Vmux bo najprej namestil Homebrew, nato { $name }.
setup-terminal-instructions = V terminalu pritisnite Return za začetek, nato ob pozivu vnesite geslo za Mac.
setup-command-missing = Vmux je odprl to stran, ker lokalni ukaz { $command } še ni nameščen. Za namestitev zaženite spodnji ukaz.
setup-install-failed = Namestitev se ni dokončala. Za podrobnosti preverite terminal, nato poskusite znova.
setup-installing = Nameščanje …
setup-install-homebrew = Namesti Homebrew + { $name }
setup-run-install = Zaženi ukaz za namestitev
setup-auto-reload = Vmux ga zažene v terminalu in znova naloži, ko je { $command } pripravljen.

debug-title = Razhroščevanje
debug-auto-update = Samodejno posodabljanje
debug-simulate-update = Simuliraj razpoložljivo posodobitev
debug-simulate-download = Simuliraj prenos
debug-clear-update = Počisti posodobitev
debug-trigger-restart = Sproži ponovni zagon

command-manage-spaces = Upravljanje prostorov …
command-pane-stack-location = podokno { $pane } / sklad { $stack }
command-space-pane-stack-location = { $space } / podokno { $pane } / sklad { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Interaktivni način
command-group-window = Okno
command-group-tab = Zavihek
command-group-pane = Podokno
command-group-stack = Sklad
command-group-space = Prostor
command-group-navigation = Krmarjenje
command-group-open = Odpri
command-group-view = Pogled
command-group-bar = Vrstica

menu-close-vmux = Zapri Vmux

agents-terminal-coding-agent = Kodirni agent v terminalu
