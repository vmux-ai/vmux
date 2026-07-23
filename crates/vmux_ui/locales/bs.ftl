locale-name = bosanski
common-open = Otvori
common-close = Zatvori
common-install = Instaliraj
common-uninstall = Deinstaliraj
common-update = Ažuriraj
common-retry = Pokušaj ponovo
common-refresh = Osvježi
common-remove = Ukloni
common-enable = Omogući
common-disable = Onemogući
common-new = Novo
common-active = aktivno
common-running = pokrenuto
common-done = gotovo
common-failed = Neuspjelo
common-installed = Instalirano
common-items = { $count ->
    [one] { $count } stavka
   *[other] { $count } stavki
}

tools-title = Alati
tools-search = Pretraži pakete, agente, MCP, jezičke alate i konfiguracijske datoteke…
tools-open = Otvori alate
tools-fold = Sažmi alate
tools-unfold = Proširi alate
tools-scanning = Skeniranje lokalnih alata…
tools-no-installed = Nema instaliranih alata
tools-empty = Nema odgovarajućih alata
tools-empty-detail = Instalirajte paket ili dodajte paket konfiguracijskih datoteka u stilu Stow.
tools-apply = Primijeni
tools-homebrew = Homebrew
tools-homebrew-sync = Instalirane formule i aplikacije automatski se sinhronizuju.
tools-open-brewfile = Otvori Brewfile
tools-managed = upravljano
tools-provider-homebrew-formulae = Homebrew formule
tools-provider-homebrew-casks = Homebrew aplikacije
tools-provider-npm = npm paketi
tools-provider-acp-agents = ACP agenti
tools-provider-language-tools = Jezički alati
tools-provider-mcp-servers = MCP serveri
tools-provider-dotfiles = Konfiguracijske datoteke
tools-status-available = Dostupno
tools-status-missing = Nedostaje
tools-status-conflict = Sukob
tools-forget = Zaboravi
tools-manage = Upravljaj
tools-link = Poveži
tools-unlink = Prekini vezu
tools-import = Uvezi
tools-update-count = { $count ->
    [one] 1 ažuriranje
   *[other] { $count } ažuriranja
}
tools-conflict-count = { $count ->
    [one] 1 sukob
   *[other] { $count } sukoba
}
tools-result-applied = Alati su primijenjeni
tools-result-imported = Alati su uvezeni
tools-result-installed = { $name } je instaliran
tools-result-updated = { $name } je ažuriran
tools-result-uninstalled = { $name } je deinstaliran
tools-result-forgotten = { $name } je zaboravljen
tools-result-managed = { $name } je sada pod upravljanjem
tools-result-linked = { $name } je povezan
tools-result-unlinked = Veza s { $name } je prekinuta

start-title = Start
start-tagline = Jedna uputa. Sve završeno.

agents-title = Agenti
agents-search = Pretraži ACP i CLI agente…
agents-empty = Nema odgovarajućih agenata
agents-empty-detail = Pokušaj s nazivom, okruženjem ili ACP/CLI.
agents-install-failed = Instalacija nije uspjela
agents-updating = Ažuriranje…
agents-retrying = Ponovni pokušaj…
agents-preparing = Priprema…

extensions-title = Proširenja
extensions-search = Pretraži instalirano ili Chrome Web Store…
extensions-relaunch = Ponovo pokreni za primjenu
extensions-empty = Nema instaliranih proširenja
extensions-no-match = Nema odgovarajućih proširenja
extensions-empty-detail = Pretraži Chrome Web Store iznad i pritisni Enter.
extensions-no-match-detail = Pokušaj s drugim nazivom ili ID-jem proširenja.
extensions-on = Uključeno
extensions-off = Isključeno
extensions-enable-confirm = Omogućiti { $name }?
extensions-enable-permissions = Omogući { $name } i dozvoli:

lsp-title = Jezički serveri
lsp-search = Pretraži jezičke servere, lintere, formatere…
lsp-loading = Učitavanje kataloga…
lsp-empty = Nema odgovarajućih jezičkih servera
lsp-empty-detail = Pokušaj s drugim jezikom, linterom ili formaterom.
lsp-needs = traži { $tool }
lsp-status-available = Dostupno
lsp-status-on-path = Na PATH
lsp-status-installing = Instaliranje…
lsp-status-installed = Instalirano
lsp-status-outdated = Dostupno ažuriranje
lsp-status-running = Pokrenuto
lsp-status-failed = Neuspjelo

spaces-title = Prostori
spaces-new-placeholder = Naziv novog prostora
spaces-empty = Nema prostora
spaces-default-name = Prostor { $number }
spaces-tabs = { $count ->
    [one] 1 kartica
   *[other] { $count } kartica
}
spaces-delete = Izbriši prostor

team-title = Tim
team-just-you = Samo ti u ovom prostoru
team-agents = { $count ->
    [one] Ti i 1 agent
   *[other] Ti i { $count } agenata
}
team-empty = Ovdje još nema nikoga
team-you = Ti
team-agent = Agent

services-title = Pozadinski servisi
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procesa
}
services-kill-all = Prekini sve
services-not-running = Servis nije pokrenut
services-start-with = Pokreni pomoću:
services-empty = Nema aktivnih procesa
services-filter = Filtriraj procese…
services-no-match = Nema odgovarajućih procesa
services-connected = Povezano
services-disconnected = Prekinuta veza
services-attached = prikačeno
services-kill = Prekini
services-memory = Memorija
services-size = Veličina
services-shell = Shell

error-title = Greška

history-search = Pretraži historiju
history-clear-all = Obriši sve
history-clear-confirm = Obrisati cijelu historiju?
history-clear-warning = Ovo se ne može poništiti.
history-cancel = Otkaži
history-today = Danas
history-yesterday = Jučer
history-days-ago = prije { $count } dana
history-day-offset = Dan -{ $count }

settings-title = Postavke
settings-loading = Učitavanje postavki…
settings-stored = Spremljeno u ~/.vmux/settings.ron
settings-other = Ostalo
settings-software-update = Ažuriranje softvera
settings-check-updates = Provjeri ažuriranja
settings-check-updates-hint = Provjerava automatski pri pokretanju i svakog sata kada je uključeno automatsko ažuriranje.
settings-update-unavailable = Nedostupno
settings-update-unavailable-hint = Ažuriranje nije uključeno u ovu verziju.
settings-update-checking = Provjera…
settings-update-checking-hint = Provjera ažuriranja…
settings-update-check-again = Provjeri ponovo
settings-update-current = Vmux je ažuriran.
settings-update-downloading = Preuzimanje…
settings-update-downloading-hint = Preuzimanje Vmux { $version }…
settings-update-installing = Instaliranje…
settings-update-installing-hint = Instaliranje Vmux { $version }…
settings-update-ready = Ažuriranje spremno
settings-update-ready-hint = Vmux { $version } je spreman. Ponovo pokreni za primjenu.
settings-update-try-again = Pokušaj ponovo
settings-update-failed = Nije moguće provjeriti ažuriranja.
settings-item = Stavka
settings-item-number = Stavka { $number }
settings-press-key = Pritisni tipku…
settings-saved = Spremljeno
settings-record-key = Klikni za snimanje nove kombinacije tipki

tray-open-window = Otvori prozor
tray-close-window = Zatvori prozor
tray-pause-recording = Pauziraj snimanje
tray-resume-recording = Nastavi snimanje
tray-finish-recording = Završi snimanje
tray-quit = Zatvori Vmux

composer-attach-files = Priloži datoteke (/upload)
composer-remove-attachment = Ukloni prilog

layout-back = Nazad
layout-forward = Naprijed
layout-reload = Ponovo učitaj
layout-bookmark-page = Dodaj ovu stranicu u oznake
layout-remove-bookmark = Ukloni oznaku
layout-pin-page = Prikači ovu stranicu
layout-unpin-page = Otkvači ovu stranicu
layout-manage-extensions = Upravljaj proširenjima
layout-new-stack = Novi sloj
layout-close-tab = Zatvori karticu
layout-bookmark = Oznaka
layout-pin = Prikači
layout-new-tab = Nova kartica
layout-team = Tim

command-switch-space = Promijeni prostor…
command-search-ask = Pretraži ili pitaj…
command-new-tab-placeholder = Pretraži ili unesi URL, ili odaberi Terminal…
command-placeholder = Unesi URL, pretraži kartice ili > za naredbe…
command-composer-placeholder = Unesi / za naredbe ili @ za medije
command-send = Pošalji (Enter)
command-terminal = Terminal
command-open-terminal = Otvori u Terminalu
command-stack = Sloj
command-tabs = { $count ->
    [one] 1 kartica
   *[other] { $count } kartica
}
command-prompt = Uputa
command-new-tab = Nova kartica
command-search = Pretraži
command-open-value = Otvori “{ $value }”
command-search-value = Pretraži “{ $value }”

schema-appearance = Izgled
schema-general = Opšte
schema-layout = Raspored
schema-layout-detail = Prozor, okna, bočna traka i fokusni okvir.
schema-agent = Agent
schema-agent-detail = Ponašanje agenta i dozvole za alate.
schema-shortcuts = Prečice
schema-shortcuts-detail = Samo za pregled. Za promjenu prečica direktno uredi settings.ron.
schema-terminal = Terminal
schema-browser = Preglednik
schema-mode = Način
schema-mode-detail = Shema boja za web stranice. Uređaj prati sistem.
schema-device = Uređaj
schema-light = Svijetlo
schema-dark = Tamno
schema-language = Jezik
schema-language-detail = Koristi sistem, en-US, ja ili bilo koju BCP 47 oznaku s odgovarajućim ~/.vmux/locales/<tag>.ftl katalogom.
schema-auto-update = Automatsko ažuriranje
schema-auto-update-detail = Provjeri i instaliraj ažuriranja pri pokretanju i svakog sata.
schema-startup-url = Početni URL
schema-startup-url-detail = Ako je prazno, otvara se upit komandne trake.
schema-search-engine = Pretraživač
schema-search-engine-detail = Koristi se za web pretrage sa Starta i iz komandne trake.
schema-window = Prozor
schema-pane = Okno
schema-side-sheet = Bočni panel
schema-focus-ring = Fokusni okvir
schema-run-placement = Dozvoli promjenu smještaja pokretanja
schema-run-placement-detail = Dozvoli agentima da biraju način okna za pokretanje, smjer i sidro.
schema-leader = Vodeća tipka
schema-leader-detail = Prefiksna tipka za akordne prečice.
schema-chord-timeout = Istek akorda
schema-chord-timeout-detail = Milisekunde prije isteka prefiksa akorda.
schema-bindings = Veze tipki
schema-confirm-close = Potvrdi zatvaranje
schema-confirm-close-detail = Pitaj prije zatvaranja terminala s pokrenutim procesom.
schema-default-theme = Zadana tema
schema-default-theme-detail = Naziv aktivne teme iz liste tema.

settings-empty = (prazno)
settings-none = (ništa)

schema-system = Sistem
schema-editor = Uređivač
schema-recording = Snimanje
schema-radius = Poluprečnik
schema-padding = Unutrašnji razmak
schema-gap = Razmak
schema-width = Širina
schema-color = Boja
schema-red = Crvena
schema-green = Zelena
schema-blue = Plava
schema-follow-files = Prati datoteke
schema-tidy-files = Sređivanje datoteka
schema-tidy-files-max = Prag za sređivanje datoteka
schema-tidy-files-auto = Automatski sređuj datoteke
schema-app-providers = Pružatelji aplikacija
schema-provider = Pružatelj
schema-kind = Vrsta
schema-models = Modeli
schema-acp = ACP agenti
schema-id = ID
schema-name = Naziv
schema-command = Naredba
schema-arguments = Argumenti
schema-environment = Okruženje
schema-working-directory = Radni direktorij
schema-shell = Ljuska
schema-font-family = Porodica fontova
schema-startup-directory = Početni direktorij
schema-themes = Teme
schema-color-scheme = Shema boja
schema-font-size = Veličina fonta
schema-line-height = Visina reda
schema-cursor-style = Stil kursora
schema-cursor-blink = Treperenje kursora
schema-custom-themes = Prilagođene teme
schema-foreground = Prednji plan
schema-background = Pozadina
schema-cursor = Kursor
schema-ansi-colors = ANSI boje
schema-keymap = Mapa tipki
schema-explorer = Istraživač
schema-visible = Vidljivo
schema-language-servers = Jezički serveri
schema-servers = Serveri
schema-language-id = ID jezika
schema-root-markers = Oznake korijena
schema-output-directory = Izlazni direktorij

menu-scene = Scena
menu-layout = Raspored
menu-terminal = Terminal
menu-browser = Preglednik
menu-service = Servis
menu-bookmark = Oznaka
menu-edit = Uredi

layout-knowledge = Znanje
layout-open-knowledge = Otvori Znanje
layout-open-welcome-knowledge = Otvori Dobrodošlicu u Znanje
layout-open-path = Otvori { $path }
layout-fold-knowledge = Sklopi znanje
layout-unfold-knowledge = Rasklopi znanje
layout-bookmarks = Oznake
layout-new-folder = Nova mapa
layout-add-to-bookmarks = Dodaj u oznake
layout-move-to-bookmarks = Premjesti u oznake
layout-stack-number = Sloj { $number }
layout-fold-stack = Sklopi sloj
layout-unfold-stack = Rasklopi sloj
layout-close-stack = Zatvori sloj
layout-bookmark-in = Označi u { $folder }

common-cancel = Otkaži
common-delete = Izbriši
common-save = Spremi
common-rename = Preimenuj
common-expand = Proširi
common-collapse = Sažmi
common-loading = Učitavanje…
common-error = Greška
common-output = Izlaz
common-pending = Na čekanju
common-current = trenutno
common-stop = Zaustavi
services-command = Vmux servis
services-uptime-seconds = { $seconds }s
services-uptime-minutes = { $minutes }m { $seconds }s
services-uptime-hours = { $hours }h { $minutes }m
services-uptime-days = { $days }d { $hours }h

error-page-failed-load = Stranica se nije učitala
error-page-not-found = Stranica nije pronađena
error-unknown-host = Nepoznat host Vmux aplikacije: { $host }

history-title = Historija

command-new-app-chat = Novi chat { $provider }/{ $model } (aplikacija)
command-interactive-mode-user = Scena > Interaktivni način > Korisnik
command-interactive-mode-player = Scena > Interaktivni način > Player
command-minimize-window = Raspored > Prozor > Minimiziraj
command-toggle-layout = Raspored > Raspored > Prebaci raspored
command-close-tab = Raspored > Kartica > Zatvori karticu
command-new-task = Raspored > Kartica > Novi zadatak…
command-next-tab = Raspored > Kartica > Sljedeća kartica
command-prev-tab = Raspored > Kartica > Prethodna kartica
command-rename-tab = Raspored > Kartica > Preimenuj karticu
command-tab-select-1 = Raspored > Kartica > Odaberi karticu 1
command-tab-select-2 = Raspored > Kartica > Odaberi karticu 2
command-tab-select-3 = Raspored > Kartica > Odaberi karticu 3
command-tab-select-4 = Raspored > Kartica > Odaberi karticu 4
command-tab-select-5 = Raspored > Kartica > Odaberi karticu 5
command-tab-select-6 = Raspored > Kartica > Odaberi karticu 6
command-tab-select-7 = Raspored > Kartica > Odaberi karticu 7
command-tab-select-8 = Raspored > Kartica > Odaberi karticu 8
command-tab-select-last = Raspored > Kartica > Odaberi zadnju karticu
command-close-pane = Raspored > Okno > Zatvori okno
command-select-pane-left = Raspored > Okno > Odaberi lijevo okno
command-select-pane-right = Raspored > Okno > Odaberi desno okno
command-select-pane-up = Raspored > Okno > Odaberi gornje okno
command-select-pane-down = Raspored > Okno > Odaberi donje okno
command-swap-pane-prev = Raspored > Okno > Zamijeni s prethodnim oknom
command-swap-pane-next = Raspored > Okno > Zamijeni sa sljedećim oknom
command-equalize-pane-size = Raspored > Okno > Izjednači veličinu okana
command-resize-pane-left = Raspored > Okno > Promijeni veličinu okna ulijevo
command-resize-pane-right = Raspored > Okno > Promijeni veličinu okna udesno
command-resize-pane-up = Raspored > Okno > Promijeni veličinu okna prema gore
command-resize-pane-down = Raspored > Okno > Promijeni veličinu okna prema dolje
command-stack-close = Raspored > Stek > Zatvori stek
command-stack-next = Raspored > Stek > Sljedeći stek
command-stack-previous = Raspored > Stek > Prethodni stek
command-stack-reopen = Raspored > Stek > Ponovo otvori zatvorenu stranicu
command-stack-swap-prev = Raspored > Stek > Pomjeri stek ulijevo
command-stack-swap-next = Raspored > Stek > Pomjeri stek udesno
command-space-open = Raspored > Prostor > Prostori
command-terminal-close = Terminal > Zatvori terminal
command-terminal-next = Terminal > Sljedeći terminal
command-terminal-prev = Terminal > Prethodni terminal
command-terminal-clear = Terminal > Očisti terminal
command-browser-prev-page = Preglednik > Navigacija > Nazad
command-browser-next-page = Preglednik > Navigacija > Naprijed
command-browser-reload = Preglednik > Navigacija > Ponovo učitaj
command-browser-hard-reload = Preglednik > Navigacija > Potpuno ponovo učitaj
command-open-in-place = Preglednik > Otvori > Otvori ovdje
command-open-in-new-stack = Preglednik > Otvori > Otvori u novom steku
command-open-in-pane-top = Preglednik > Otvori > Otvori u oknu iznad
command-open-in-pane-right = Preglednik > Otvori > Otvori u desnom oknu
command-open-in-pane-bottom = Preglednik > Otvori > Otvori u oknu ispod
command-open-in-pane-left = Preglednik > Otvori > Otvori u lijevom oknu
command-open-in-new-tab = Preglednik > Otvori > Otvori u novoj kartici
command-open-in-new-space = Preglednik > Otvori > Otvori u novom prostoru
command-browser-zoom-in = Preglednik > Prikaz > Uvećaj
command-browser-zoom-out = Preglednik > Prikaz > Umanji
command-browser-zoom-reset = Preglednik > Prikaz > Stvarna veličina
command-browser-dev-tools = Preglednik > Prikaz > Alati za programere
command-browser-open-command-bar = Preglednik > Traka > Komandna traka
command-browser-open-page-in-command-bar = Preglednik > Traka > Uredi stranicu
command-browser-open-path-bar = Preglednik > Traka > Navigator putanje
command-browser-open-commands = Preglednik > Traka > Komande
command-browser-open-history = Preglednik > Traka > Historija
command-service-open = Servis > Otvori nadzor servisa
command-bookmark-toggle-active = Oznaka > Označi stranicu
command-bookmark-pin-active = Oznaka > Prikvači stranicu

layout-tab = Kartica
layout-no-stacks = Nema stekova
layout-loading = Učitavanje…
layout-no-markdown-files = Nema Markdown datoteka
layout-empty-folder = Prazan folder
layout-worktree = radno stablo
layout-folder-name = Naziv foldera
layout-no-pins-bookmarks = Nema prikvačenih stranica ni oznaka
layout-move-to = Premjesti u { $folder }
layout-bookmark-current-page = Označi trenutnu stranicu
layout-rename-folder = Preimenuj folder
layout-remove-folder = Ukloni folder
layout-update-downloading = Preuzimanje ažuriranja
layout-update-installing = Instaliranje ažuriranja…
layout-update-ready = Dostupna je nova verzija
layout-restart-update = Ponovo pokreni za ažuriranje

agent-preparing = Priprema agenta…
agent-send-all-queued = Pošalji sve upite iz reda odmah (Esc)
agent-send = Pošalji (Enter)
agent-ready = Spreman kad i vi.
agent-loading-older = Učitavanje starijih poruka…
agent-load-older = Učitaj starije poruke
agent-continued-from = Nastavljeno iz { $source }
agent-older-context-omitted = stariji kontekst izostavljen
agent-interrupted = prekinuto
agent-allow-tool = Dozvoliti { $tool }?
agent-deny = Odbij
agent-allow-always = Uvijek dozvoli
agent-allow = Dozvoli
agent-loading-sessions = Učitavanje sesija…
agent-no-resumable-sessions = Nema sesija koje se mogu nastaviti
agent-no-matching-sessions = Nema odgovarajućih sesija
agent-no-matching-models = Nema odgovarajućih modela
agent-choice-help = ↑/↓ ili Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Odaberite folder repozitorija
agent-choose-repository-detail = Odaberite lokalni Git repozitorij koji agent treba koristiti.
agent-choosing = Odabir…
agent-choose-folder = Odaberite folder
agent-queued = u redu
agent-attached = Priloženo:
agent-cancel-queued = Otkaži upit u redu
agent-resume-queued = Nastavi upite iz reda
agent-clear-queue = Očisti red
agent-send-all-now = pošalji sve odmah
agent-choose-option = Odaberite opciju iznad
agent-loading-media = Učitavanje medija…
agent-no-matching-media = Nema odgovarajućih medija
agent-prompt-context = Kontekst upita
agent-details = Detalji
agent-path = Putanja
agent-tool = Alat
agent-server = Server
agent-bytes = { $count } bajtova
agent-worked-for = Radio { $duration }
agent-worked-for-steps = { $count ->
    [one] Radio { $duration } · 1 korak
   *[other] Radio { $duration } · { $count } koraka
}
agent-tool-guardian-review = Guardian pregled
agent-tool-read-files = Pročitao datoteke
agent-tool-viewed-image = Pregledao sliku
agent-tool-used-browser = Koristio preglednik
agent-tool-searched-files = Pretražio datoteke
agent-tool-ran-commands = Pokrenuo komande
agent-thinking = Razmišlja
agent-subagent = Podagent
agent-prompt = Upit
agent-thread = Nit
agent-parent = Roditelj
agent-children = Djeca
agent-call = Poziv
agent-raw-event = Sirovi događaj
agent-plan = Plan
agent-tasks = { $count ->
    [one] 1 zadatak
   *[other] { $count } zadataka
}
agent-edited = Uređeno
agent-reconnecting = Ponovno povezivanje { $attempt }/{ $total }
agent-status-running = Izvršava se
agent-status-done = Gotovo
agent-status-failed = Neuspjelo
agent-status-pending = Na čekanju
agent-slash-attach-files = Priloži datoteke
agent-slash-resume-session = Nastavi prethodnu sesiju
agent-slash-select-model = Odaberi model
agent-slash-continue-cli = Nastavi ovu sesiju u CLI-ju
agent-session-just-now = upravo sada
agent-session-minutes-ago = prije { $count } min
agent-session-hours-ago = prije { $count } h
agent-session-days-ago = prije { $count } d
agent-working-working = Radi
agent-working-thinking = Razmišlja
agent-working-pondering = Promišlja
agent-working-noodling = Mozga
agent-working-percolating = Kuha ideje
agent-working-conjuring = Priziva rješenje
agent-working-cooking = Sprema
agent-working-brewing = Zakuhava
agent-working-musing = Razmatra
agent-working-ruminating = Premotava
agent-working-scheming = Smišlja plan
agent-working-synthesizing = Sintetizira
agent-working-tinkering = Petlja
agent-working-churning = Obrađuje
agent-working-vibing = Hvata ritam
agent-working-simmering = Krčka
agent-working-crafting = Sklapa
agent-working-divining = Naslućuje
agent-working-mulling = Premeće
agent-working-spelunking = Kopa po dubini

editor-toggle-explorer = Prikaži/sakrij Explorer (Cmd+B)
editor-unsaved = nespremljeno
editor-rendered-markdown = Renderovani Markdown s uređivanjem uživo
editor-note = Napomena
editor-source-editor = Uređivač izvornog koda
editor-editor = Uređivač
editor-git-diff = Git diff
editor-diff = Diff
editor-tidy = Pospremi
editor-always = Uvijek
editor-unchanged-previews = { $count ->
    [one] ✦ 1 nepromijenjen pregled
   *[other] ✦ { $count } nepromijenjenih pregleda
}
editor-open-externally = Otvori eksterno
editor-changed-line = Izmijenjeni red
editor-go-to-definition = Idi na definiciju
editor-find-references = Pronađi reference
editor-references = { $count ->
    [one] 1 referenca
   *[other] { $count } referenci
}
editor-lsp-starting = { $server } se pokreće…
editor-lsp-not-installed = { $server } — nije instaliran
editor-explorer = Explorer
editor-open-editors = Otvoreni uređivači
editor-outline = Struktura
editor-new-file = Nova datoteka
editor-new-folder = Novi folder
editor-delete-confirm = Izbrisati “{ $name }”? Ovo se ne može poništiti.
editor-created-folder = Kreiran folder { $name }
editor-created-file = Kreirana datoteka { $name }
editor-renamed-to = Preimenovano u { $name }
editor-deleted = Izbrisano { $name }
editor-failed-decode-image = Slika se nije mogla dekodirati
editor-preview-large-image = slika (prevelika za pregled)
editor-preview-binary = binarno
editor-preview-file = datoteka

git-status-clean = čisto
git-status-modified = izmijenjeno
git-status-staged = dodano u stage
git-status-staged-modified = stage*
git-status-untracked = nepraćeno
git-status-deleted = izbrisano
git-status-conflict = konflikt
git-accept-all = ✓ prihvati sve
git-unstage = Ukloni iz stagea
git-confirm-deny-all = Potvrdi odbijanje svega
git-deny-all = ✗ odbij sve
git-commit-message = commit poruka
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Učitavanje diffa…
git-no-changes = Nema promjena za prikaz
git-accept = ✓ prihvati
git-deny = ✗ odbij
git-show-unchanged-lines = Prikaži { $count } nepromijenjenih redova

terminal-loading = Učitavanje…
terminal-runs-when-ready = pokreće se kad bude spremno · Ctrl+C čisti · Esc preskače
terminal-booting = pokretanje
terminal-type-command = unesite komandu · pokreće se kad bude spremno · Esc preskače

setup-tagline-claude = Anthropicov agent za kodiranje, u Vmuxu
setup-tagline-codex = OpenAI-jev agent za kodiranje, u Vmuxu
setup-tagline-vibe = Mistralov agent za kodiranje, u Vmuxu
setup-install-title = Instaliraj { $name } CLI
setup-homebrew-required = Homebrew je potreban za instalaciju { $command } i još nije postavljen. Vmux će prvo instalirati Homebrew, zatim { $name }.
setup-terminal-instructions = U terminalu pritisnite Return za početak, zatim unesite lozinku za Mac kad se zatraži.
setup-command-missing = Vmux je otvorio ovu stranicu jer lokalna komanda { $command } još nije instalirana. Pokrenite komandu ispod da je dobijete.
setup-install-failed = Instalacija nije završena. Provjerite detalje u terminalu, pa pokušajte ponovo.
setup-installing = Instaliranje…
setup-install-homebrew = Instaliraj Homebrew + { $name }
setup-run-install = Pokreni instalacijsku komandu
setup-auto-reload = Vmux je pokreće u terminalu i ponovo učitava kad { $command } bude spreman.

debug-title = Otklanjanje grešaka
debug-auto-update = Automatsko ažuriranje
debug-simulate-update = Simuliraj dostupno ažuriranje
debug-simulate-download = Simuliraj preuzimanje
debug-clear-update = Očisti ažuriranje
debug-trigger-restart = Pokreni ponovno pokretanje

command-manage-spaces = Upravljaj prostorima…
command-pane-stack-location = okno { $pane } / stog { $stack }
command-space-pane-stack-location = { $space } / okno { $pane } / stog { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Interaktivni način
command-group-window = Prozor
command-group-tab = Kartica
command-group-pane = Okno
command-group-stack = Stog
command-group-space = Prostor
command-group-navigation = Navigacija
command-group-open = Otvori
command-group-view = Prikaz
command-group-bar = Traka

menu-close-vmux = Zatvori Vmux

agents-terminal-coding-agent = Agentski programer u terminalu
