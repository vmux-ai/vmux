locale-name = slovenčina
common-open = Otvoriť
common-close = Zavrieť
common-install = Nainštalovať
common-uninstall = Odinštalovať
common-update = Aktualizovať
common-retry = Skúsiť znova
common-refresh = Obnoviť
common-remove = Odstrániť
common-enable = Zapnúť
common-disable = Vypnúť
common-new = Nové
common-active = aktívne
common-running = spustené
common-done = hotovo
common-failed = Zlyhalo
common-installed = Nainštalované
common-items = { $count ->
    [one] { $count } položka
   *[other] { $count } položiek
}

tools-title = Nástroje
tools-search = Hľadať balíky, agentov, MCP, jazykové nástroje a konfiguračné súbory…
tools-open = Otvoriť nástroje
tools-fold = Zbaliť nástroje
tools-unfold = Rozbaliť nástroje
tools-scanning = Prehľadávajú sa miestne nástroje…
tools-no-installed = Nie sú nainštalované žiadne nástroje
tools-empty = Žiadne zodpovedajúce nástroje
tools-empty-detail = Nainštalujte balík alebo pridajte balík konfiguračných súborov v štýle Stow.
tools-apply = Použiť
tools-homebrew = Homebrew
tools-homebrew-sync = Nainštalované formule a aplikácie sa synchronizujú automaticky.
tools-open-brewfile = Otvoriť Brewfile
tools-managed = spravované
tools-provider-homebrew-formulae = Formule Homebrew
tools-provider-homebrew-casks = Aplikácie Homebrew
tools-provider-npm = Balíky npm
tools-provider-acp-agents = Agenti ACP
tools-provider-language-tools = Jazykové nástroje
tools-provider-mcp-servers = Servery MCP
tools-provider-dotfiles = Konfiguračné súbory
tools-status-available = Dostupné
tools-status-missing = Chýba
tools-status-conflict = Konflikt
tools-forget = Zabudnúť
tools-manage = Spravovať
tools-link = Prepojiť
tools-unlink = Odpojiť
tools-import = Importovať
tools-update-count = { $count ->
    [one] 1 aktualizácia
   *[other] { $count } aktualizácií
}
tools-conflict-count = { $count ->
    [one] 1 konflikt
   *[other] { $count } konfliktov
}
tools-result-applied = Nástroje boli použité
tools-result-imported = Nástroje boli importované
tools-result-installed = { $name } bol nainštalovaný
tools-result-updated = { $name } bol aktualizovaný
tools-result-uninstalled = { $name } bol odinštalovaný
tools-result-forgotten = { $name } bol zabudnutý
tools-result-managed = { $name } je teraz spravovaný
tools-result-linked = { $name } bol prepojený
tools-result-unlinked = { $name } bol odpojený

start-title = Štart
start-tagline = Jeden prompt. Hotové čokoľvek.

agents-title = Agenti
agents-search = Hľadať agentov ACP a CLI…
agents-empty = Žiadni zodpovedajúci agenti
agents-empty-detail = Skúste názov, runtime alebo ACP/CLI.
agents-install-failed = Inštalácia zlyhala
agents-updating = Aktualizuje sa…
agents-retrying = Skúša sa znova…
agents-preparing = Pripravuje sa…

extensions-title = Rozšírenia
extensions-search = Hľadať nainštalované alebo v Chrome Web Store…
extensions-relaunch = Reštartujte na použitie zmien
extensions-empty = Nie sú nainštalované žiadne rozšírenia
extensions-no-match = Žiadne zodpovedajúce rozšírenia
extensions-empty-detail = Vyhľadajte vyššie v Chrome Web Store a stlačte Enter.
extensions-no-match-detail = Skúste iný názov alebo ID rozšírenia.
extensions-on = Zapnuté
extensions-off = Vypnuté
extensions-enable-confirm = Zapnúť { $name }?
extensions-enable-permissions = Zapnúť { $name } a povoliť:

lsp-title = Jazykové servery
lsp-search = Hľadať jazykové servery, lintery, formátovače…
lsp-loading = Načítava sa katalóg…
lsp-empty = Žiadne zodpovedajúce jazykové servery
lsp-empty-detail = Skúste iný jazyk, linter alebo formátovač.
lsp-needs = vyžaduje { $tool }
lsp-status-available = Dostupné
lsp-status-on-path = V PATH
lsp-status-installing = Inštaluje sa…
lsp-status-installed = Nainštalované
lsp-status-outdated = Dostupná aktualizácia
lsp-status-running = Spustené
lsp-status-failed = Zlyhalo

spaces-title = Priestory
spaces-new-placeholder = Názov nového priestoru
spaces-empty = Žiadne priestory
spaces-default-name = Priestor { $number }
spaces-tabs = { $count ->
    [one] 1 karta
   *[other] { $count } kariet
}
spaces-delete = Odstrániť priestor

team-title = Tím
team-just-you = V tomto priestore ste len vy
team-agents = { $count ->
    [one] Vy a 1 agent
   *[other] Vy a { $count } agentov
}
team-empty = Zatiaľ tu nikto nie je
team-you = Vy
team-agent = Agent

services-title = Služby na pozadí
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procesov
}
services-kill-all = Vynútiť ukončenie všetkých
services-not-running = Služba nie je spustená
services-start-with = Spustiť pomocou:
services-empty = Žiadne aktívne procesy
services-filter = Filtrovať procesy…
services-no-match = Žiadne zodpovedajúce procesy
services-connected = Pripojené
services-disconnected = Odpojené
services-attached = pripojené
services-kill = Vynútiť ukončenie
services-memory = Pamäť
services-size = Veľkosť
services-shell = Shell

error-title = Chyba

history-search = Hľadať v histórii
history-clear-all = Vymazať všetko
history-clear-confirm = Vymazať celú históriu?
history-clear-warning = Túto akciu nemožno vrátiť späť.
history-cancel = Zrušiť
history-today = Dnes
history-yesterday = Včera
history-days-ago = pred { $count } dňami
history-day-offset = Deň -{ $count }

settings-title = Nastavenia
settings-loading = Načítavajú sa nastavenia…
settings-stored = Uložené v ~/.vmux/settings.ron
settings-other = Ostatné
settings-software-update = Aktualizácia softvéru
settings-check-updates = Vyhľadať aktualizácie
settings-check-updates-hint = Pri zapnutej automatickej aktualizácii sa kontrolujú pri spustení a každú hodinu.
settings-update-unavailable = Nedostupné
settings-update-unavailable-hint = Aktualizátor nie je súčasťou tohto zostavenia.
settings-update-checking = Kontroluje sa…
settings-update-checking-hint = Kontrolujú sa aktualizácie…
settings-update-check-again = Skontrolovať znova
settings-update-current = Vmux je aktuálny.
settings-update-downloading = Sťahuje sa…
settings-update-downloading-hint = Sťahuje sa Vmux { $version }…
settings-update-installing = Inštaluje sa…
settings-update-installing-hint = Inštaluje sa Vmux { $version }…
settings-update-ready = Aktualizácia je pripravená
settings-update-ready-hint = Vmux { $version } je pripravený. Reštartujte aplikáciu na použitie aktualizácie.
settings-update-try-again = Skúsiť znova
settings-update-failed = Nepodarilo sa vyhľadať aktualizácie.
settings-item = Položka
settings-item-number = Položka { $number }
settings-press-key = Stlačte kláves…
settings-saved = Uložené
settings-record-key = Kliknutím nahrajte novú klávesovú skratku

tray-open-window = Otvoriť okno
tray-close-window = Zavrieť okno
tray-pause-recording = Pozastaviť nahrávanie
tray-resume-recording = Pokračovať v nahrávaní
tray-finish-recording = Dokončiť nahrávanie
tray-quit = Ukončiť Vmux

composer-attach-files = Priložiť súbory (/upload)
composer-remove-attachment = Odstrániť prílohu

layout-back = Späť
layout-forward = Dopredu
layout-reload = Obnoviť
layout-bookmark-page = Pridať stránku medzi záložky
layout-remove-bookmark = Odstrániť záložku
layout-pin-page = Pripnúť stránku
layout-unpin-page = Odopnúť stránku
layout-manage-extensions = Spravovať rozšírenia
layout-new-stack = Nová vrstva
layout-close-tab = Zavrieť kartu
layout-bookmark = Záložka
layout-pin = Pripnúť
layout-new-tab = Nová karta
layout-team = Tím

command-switch-space = Prepnúť priestor…
command-search-ask = Hľadať alebo sa opýtať…
command-new-tab-placeholder = Hľadajte, zadajte URL alebo vyberte Terminál…
command-placeholder = Zadajte URL, hľadajte karty alebo použite > na príkazy…
command-composer-placeholder = Zadajte / pre príkazy alebo @ pre médiá
command-send = Odoslať (Enter)
command-terminal = Terminál
command-open-terminal = Otvoriť v Termináli
command-stack = Vrstva
command-tabs = { $count ->
    [one] 1 karta
   *[other] { $count } kariet
}
command-prompt = Prompt
command-new-tab = Nová karta
command-search = Hľadať
command-open-value = Otvoriť „{ $value }“
command-search-value = Hľadať „{ $value }“

schema-appearance = Vzhľad
schema-general = Všeobecné
schema-layout = Rozloženie
schema-layout-detail = Okno, panely, bočný panel a zvýraznenie fokusu.
schema-agent = Agent
schema-agent-detail = Správanie agenta a povolenia nástrojov.
schema-shortcuts = Skratky
schema-shortcuts-detail = Iba na čítanie. Väzby zmeníte priamo v settings.ron.
schema-terminal = Terminál
schema-browser = Prehliadač
schema-mode = Režim
schema-mode-detail = Farebná schéma webových stránok. Zariadenie nasleduje systém.
schema-device = Zariadenie
schema-light = Svetlý
schema-dark = Tmavý
schema-language = Jazyk
schema-language-detail = Použite systém, en-US, ja alebo ľubovoľný tag BCP 47 so zodpovedajúcim katalógom ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Automatické aktualizácie
schema-auto-update-detail = Vyhľadávať a inštalovať aktualizácie pri spustení a každú hodinu.
schema-startup-url = Úvodná URL
schema-startup-url-detail = Ak je prázdna, otvorí sa výzva príkazového panela.
schema-search-engine = Vyhľadávač
schema-search-engine-detail = Používa sa na webové vyhľadávanie zo Štartu a príkazového panela.
schema-window = Okno
schema-pane = Panel
schema-side-sheet = Bočný panel
schema-focus-ring = Zvýraznenie fokusu
schema-run-placement = Povoliť prepísanie umiestnenia spustenia
schema-run-placement-detail = Umožniť agentom zvoliť režim panelu spustenia, smer a ukotvenie.
schema-leader = Leader
schema-leader-detail = Prefixová klávesa pre chord skratky.
schema-chord-timeout = Časový limit chordu
schema-chord-timeout-detail = Počet milisekúnd, po ktorých prefix chordu vyprší.
schema-bindings = Väzby
schema-confirm-close = Potvrdzovať zatvorenie
schema-confirm-close-detail = Pred zatvorením terminálu so spusteným procesom zobraziť výzvu.
schema-default-theme = Predvolená téma
schema-default-theme-detail = Názov aktívnej témy zo zoznamu tém.

settings-empty = (prázdne)
settings-none = (žiadne)

schema-system = Systém
schema-editor = Editor
schema-recording = Nahrávanie
schema-radius = Polomer
schema-padding = Vnútorný okraj
schema-gap = Medzera
schema-width = Šírka
schema-color = Farba
schema-red = Červená
schema-green = Zelená
schema-blue = Modrá
schema-follow-files = Sledovať súbory
schema-tidy-files = Upratovať súbory
schema-tidy-files-max = Limit upratovania súborov
schema-tidy-files-auto = Upratovať súbory automaticky
schema-app-providers = Poskytovatelia aplikácií
schema-provider = Poskytovateľ
schema-kind = Druh
schema-models = Modely
schema-acp = Agenti ACP
schema-id = ID
schema-name = Názov
schema-command = Príkaz
schema-arguments = Argumenty
schema-environment = Prostredie
schema-working-directory = Pracovný adresár
schema-shell = Shell
schema-font-family = Rodina písma
schema-startup-directory = Úvodný adresár
schema-themes = Motívy
schema-color-scheme = Farebná schéma
schema-font-size = Veľkosť písma
schema-line-height = Výška riadka
schema-cursor-style = Štýl kurzora
schema-cursor-blink = Blikanie kurzora
schema-custom-themes = Vlastné motívy
schema-foreground = Popredie
schema-background = Pozadie
schema-cursor = Kurzor
schema-ansi-colors = Farby ANSI
schema-keymap = Mapa klávesov
schema-explorer = Prieskumník
schema-visible = Viditeľné
schema-language-servers = Jazykové servery
schema-servers = Servery
schema-language-id = ID jazyka
schema-root-markers = Značky koreňa
schema-output-directory = Výstupný adresár

menu-scene = Scéna
menu-layout = Rozloženie
menu-terminal = Terminál
menu-browser = Prehliadač
menu-service = Služba
menu-bookmark = Záložka
menu-edit = Upraviť

layout-knowledge = Znalosti
layout-open-knowledge = Otvoriť Znalosti
layout-open-welcome-knowledge = Otvoriť uvítanie v Znalostiach
layout-open-path = Otvoriť { $path }
layout-fold-knowledge = Zbaliť Znalosti
layout-unfold-knowledge = Rozbaliť Znalosti
layout-bookmarks = Záložky
layout-new-folder = Nový priečinok
layout-add-to-bookmarks = Pridať do záložiek
layout-move-to-bookmarks = Presunúť do záložiek
layout-stack-number = Vrstva { $number }
layout-fold-stack = Zbaliť vrstvu
layout-unfold-stack = Rozbaliť vrstvu
layout-close-stack = Zavrieť vrstvu
layout-bookmark-in = Záložka v priečinku { $folder }

common-cancel = Zrušiť
common-delete = Odstrániť
common-save = Uložiť
common-rename = Premenovať
common-expand = Rozbaliť
common-collapse = Zbaliť
common-loading = Načítava sa…
common-error = Chyba
common-output = Výstup
common-pending = Čaká
common-current = aktuálne
common-stop = Zastaviť
services-command = Služba Vmux
services-uptime-seconds = { $seconds } s
services-uptime-minutes = { $minutes } min { $seconds } s
services-uptime-hours = { $hours } h { $minutes } min
services-uptime-days = { $days } d { $hours } h

error-page-failed-load = Stránku sa nepodarilo načítať
error-page-not-found = Stránka sa nenašla
error-unknown-host = Neznámy hostiteľ aplikácie Vmux: { $host }

history-title = História

command-new-app-chat = Nový chat { $provider }/{ $model } (aplikácia)
command-interactive-mode-user = Scéna > Interaktívny režim > Používateľ
command-interactive-mode-player = Scéna > Interaktívny režim > Prehrávač
command-minimize-window = Rozloženie > Okno > Minimalizovať
command-toggle-layout = Rozloženie > Rozloženie > Prepnúť rozloženie
command-close-tab = Rozloženie > Karta > Zavrieť kartu
command-new-task = Rozloženie > Karta > Nová úloha…
command-next-tab = Rozloženie > Karta > Ďalšia karta
command-prev-tab = Rozloženie > Karta > Predchádzajúca karta
command-rename-tab = Rozloženie > Karta > Premenovať kartu
command-tab-select-1 = Rozloženie > Karta > Vybrať kartu 1
command-tab-select-2 = Rozloženie > Karta > Vybrať kartu 2
command-tab-select-3 = Rozloženie > Karta > Vybrať kartu 3
command-tab-select-4 = Rozloženie > Karta > Vybrať kartu 4
command-tab-select-5 = Rozloženie > Karta > Vybrať kartu 5
command-tab-select-6 = Rozloženie > Karta > Vybrať kartu 6
command-tab-select-7 = Rozloženie > Karta > Vybrať kartu 7
command-tab-select-8 = Rozloženie > Karta > Vybrať kartu 8
command-tab-select-last = Rozloženie > Karta > Vybrať poslednú kartu
command-close-pane = Rozloženie > Panel > Zavrieť panel
command-select-pane-left = Rozloženie > Panel > Vybrať panel vľavo
command-select-pane-right = Rozloženie > Panel > Vybrať panel vpravo
command-select-pane-up = Rozloženie > Panel > Vybrať panel hore
command-select-pane-down = Rozloženie > Panel > Vybrať panel dole
command-swap-pane-prev = Rozloženie > Panel > Vymeniť s predchádzajúcim panelom
command-swap-pane-next = Rozloženie > Panel > Vymeniť s ďalším panelom
command-equalize-pane-size = Rozloženie > Panel > Zjednotiť veľkosť panelov
command-resize-pane-left = Rozloženie > Panel > Zmeniť veľkosť doľava
command-resize-pane-right = Rozloženie > Panel > Zmeniť veľkosť doprava
command-resize-pane-up = Rozloženie > Panel > Zmeniť veľkosť nahor
command-resize-pane-down = Rozloženie > Panel > Zmeniť veľkosť nadol
command-stack-close = Rozloženie > Vrstva > Zavrieť vrstvu
command-stack-next = Rozloženie > Vrstva > Ďalšia vrstva
command-stack-previous = Rozloženie > Vrstva > Predchádzajúca vrstva
command-stack-reopen = Rozloženie > Vrstva > Znova otvoriť zavretú stránku
command-stack-swap-prev = Rozloženie > Vrstva > Presunúť vrstvu doľava
command-stack-swap-next = Rozloženie > Vrstva > Presunúť vrstvu doprava
command-space-open = Rozloženie > Priestor > Priestory
command-terminal-close = Terminál > Zavrieť terminál
command-terminal-next = Terminál > Ďalší terminál
command-terminal-prev = Terminál > Predchádzajúci terminál
command-terminal-clear = Terminál > Vymazať terminál
command-browser-prev-page = Prehliadač > Navigácia > Späť
command-browser-next-page = Prehliadač > Navigácia > Dopredu
command-browser-reload = Prehliadač > Navigácia > Znovu načítať
command-browser-hard-reload = Prehliadač > Navigácia > Úplne znovu načítať
command-open-in-place = Prehliadač > Otvoriť > Otvoriť tu
command-open-in-new-stack = Prehliadač > Otvoriť > Otvoriť v novej vrstve
command-open-in-pane-top = Prehliadač > Otvoriť > Otvoriť v paneli hore
command-open-in-pane-right = Prehliadač > Otvoriť > Otvoriť v paneli vpravo
command-open-in-pane-bottom = Prehliadač > Otvoriť > Otvoriť v paneli dole
command-open-in-pane-left = Prehliadač > Otvoriť > Otvoriť v paneli vľavo
command-open-in-new-tab = Prehliadač > Otvoriť > Otvoriť na novej karte
command-open-in-new-space = Prehliadač > Otvoriť > Otvoriť v novom priestore
command-browser-zoom-in = Prehliadač > Zobrazenie > Priblížiť
command-browser-zoom-out = Prehliadač > Zobrazenie > Oddialiť
command-browser-zoom-reset = Prehliadač > Zobrazenie > Skutočná veľkosť
command-browser-dev-tools = Prehliadač > Zobrazenie > Nástroje pre vývojárov
command-browser-open-command-bar = Prehliadač > Lišta > Panel príkazov
command-browser-open-page-in-command-bar = Prehliadač > Lišta > Upraviť stránku
command-browser-open-path-bar = Prehliadač > Lišta > Navigátor cesty
command-browser-open-commands = Prehliadač > Lišta > Príkazy
command-browser-open-history = Prehliadač > Lišta > História
command-service-open = Služba > Otvoriť monitor služieb
command-bookmark-toggle-active = Záložka > Pridať stránku do záložiek
command-bookmark-pin-active = Záložka > Pripnúť stránku

layout-tab = Karta
layout-no-stacks = Žiadne vrstvy
layout-loading = Načítava sa…
layout-no-markdown-files = Žiadne súbory Markdown
layout-empty-folder = Prázdny priečinok
layout-worktree = pracovný strom
layout-folder-name = Názov priečinka
layout-no-pins-bookmarks = Žiadne pripnutia ani záložky
layout-move-to = Presunúť do { $folder }
layout-bookmark-current-page = Pridať aktuálnu stránku do záložiek
layout-rename-folder = Premenovať priečinok
layout-remove-folder = Odstrániť priečinok
layout-update-downloading = Sťahuje sa aktualizácia
layout-update-installing = Inštaluje sa aktualizácia…
layout-update-ready = Je k dispozícii nová verzia
layout-restart-update = Reštartovať a aktualizovať

agent-preparing = Pripravuje sa agent…
agent-send-all-queued = Odoslať všetky čakajúce prompty teraz (Esc)
agent-send = Odoslať (Enter)
agent-ready = Som pripravený.
agent-loading-older = Načítavajú sa staršie správy…
agent-load-older = Načítať staršie správy
agent-continued-from = Pokračovanie z { $source }
agent-older-context-omitted = starší kontext vynechaný
agent-interrupted = prerušené
agent-allow-tool = Povoliť { $tool }?
agent-deny = Zamietnuť
agent-allow-always = Vždy povoliť
agent-allow = Povoliť
agent-loading-sessions = Načítavajú sa relácie…
agent-no-resumable-sessions = Nenašli sa žiadne obnoviteľné relácie
agent-no-matching-sessions = Žiadne zodpovedajúce relácie
agent-no-matching-models = Žiadne zodpovedajúce modely
agent-choice-help = ↑/↓ alebo Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Vyberte priečinok repozitára
agent-choose-repository-detail = Vyberte lokálny Git repozitár, ktorý má agent použiť.
agent-choosing = Vyberá sa…
agent-choose-folder = Vybrať priečinok
agent-queued = vo fronte
agent-attached = Priložené:
agent-cancel-queued = Zrušiť prompt vo fronte
agent-resume-queued = Pokračovať v promptoch vo fronte
agent-clear-queue = Vymazať front
agent-send-all-now = odoslať všetko teraz
agent-choose-option = Vyberte možnosť vyššie
agent-loading-media = Načítavajú sa médiá…
agent-no-matching-media = Žiadne zodpovedajúce médiá
agent-prompt-context = Kontext promptu
agent-details = Podrobnosti
agent-path = Cesta
agent-tool = Nástroj
agent-server = Server
agent-bytes = { $count } bajtov
agent-worked-for = Pracoval { $duration }
agent-worked-for-steps = { $count ->
    [one] Pracoval { $duration } · 1 krok
   *[other] Pracoval { $duration } · { $count } krokov
}
agent-tool-guardian-review = Kontrola ochrancom
agent-tool-read-files = Prečítané súbory
agent-tool-viewed-image = Zobrazený obrázok
agent-tool-used-browser = Použitý prehliadač
agent-tool-searched-files = Prehľadané súbory
agent-tool-ran-commands = Spustené príkazy
agent-thinking = Premýšľa
agent-subagent = Podagent
agent-prompt = Prompt
agent-thread = Vlákno
agent-parent = Rodič
agent-children = Potomkovia
agent-call = Volanie
agent-raw-event = Surová udalosť
agent-plan = Plán
agent-tasks = { $count ->
    [one] 1 úloha
   *[other] { $count } úloh
}
agent-edited = Upravené
agent-reconnecting = Znovu sa pripája { $attempt }/{ $total }
agent-status-running = Beží
agent-status-done = Hotovo
agent-status-failed = Zlyhalo
agent-status-pending = Čaká
agent-slash-attach-files = Priložiť súbory
agent-slash-resume-session = Pokračovať v minulej relácii
agent-slash-select-model = Vybrať model
agent-slash-continue-cli = Pokračovať v tejto relácii v CLI
agent-session-just-now = práve teraz
agent-session-minutes-ago = pred { $count } min
agent-session-hours-ago = pred { $count } h
agent-session-days-ago = pred { $count } d
agent-working-working = Pracuje
agent-working-thinking = Premýšľa
agent-working-pondering = Uvažuje
agent-working-noodling = Dumá
agent-working-percolating = Dozrieva
agent-working-conjuring = Čaruje
agent-working-cooking = Varí
agent-working-brewing = Lúhuje
agent-working-musing = Rozjíma
agent-working-ruminating = Premieľa
agent-working-scheming = Plánuje
agent-working-synthesizing = Syntetizuje
agent-working-tinkering = Majstruje
agent-working-churning = Spracúva
agent-working-vibing = Ladí sa
agent-working-simmering = Pomaly varí
agent-working-crafting = Tvorí
agent-working-divining = Veští
agent-working-mulling = Zvažuje
agent-working-spelunking = Pátra do hĺbky

editor-toggle-explorer = Prepnúť Prieskumník (Cmd+B)
editor-unsaved = neuložené
editor-rendered-markdown = Vykreslený Markdown so živými úpravami
editor-note = Poznámka
editor-source-editor = Editor zdrojového kódu
editor-editor = Editor
editor-git-diff = Git diff
editor-diff = Diff
editor-tidy = Upratovať
editor-always = Vždy
editor-unchanged-previews = { $count ->
    [one] ✦ 1 nezmenený náhľad
   *[other] ✦ { $count } nezmenených náhľadov
}
editor-open-externally = Otvoriť externe
editor-changed-line = Zmenený riadok
editor-go-to-definition = Prejsť na definíciu
editor-find-references = Nájsť referencie
editor-references = { $count ->
    [one] 1 referencia
   *[other] { $count } referencií
}
editor-lsp-starting = { $server } sa spúšťa…
editor-lsp-not-installed = { $server } — nie je nainštalovaný
editor-explorer = Prieskumník
editor-open-editors = Otvorené editory
editor-outline = Osnova
editor-new-file = Nový súbor
editor-new-folder = Nový priečinok
editor-delete-confirm = Odstrániť „{ $name }“? Tento krok sa nedá vrátiť späť.
editor-created-folder = Priečinok { $name } bol vytvorený
editor-created-file = Súbor { $name } bol vytvorený
editor-renamed-to = Premenované na { $name }
editor-deleted = Odstránené: { $name }
editor-failed-decode-image = Obrázok sa nepodarilo dekódovať
editor-preview-large-image = obrázok (príliš veľký na náhľad)
editor-preview-binary = binárny súbor
editor-preview-file = súbor

git-status-clean = čisté
git-status-modified = zmenené
git-status-staged = pripravené
git-status-staged-modified = pripravené*
git-status-untracked = nesledované
git-status-deleted = odstránené
git-status-conflict = konflikt
git-accept-all = ✓ prijať všetko
git-unstage = Zrušiť prípravu
git-confirm-deny-all = Potvrdiť zamietnutie všetkého
git-deny-all = ✗ zamietnuť všetko
git-commit-message = správa commitu
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Načítava sa diff…
git-no-changes = Žiadne zmeny na zobrazenie
git-accept = ✓ prijať
git-deny = ✗ zamietnuť
git-show-unchanged-lines = Zobraziť { $count } nezmenených riadkov

terminal-loading = Načítava sa…
terminal-runs-when-ready = spustí sa, keď bude pripravené · Ctrl+C vymaže · Esc preskočí
terminal-booting = spúšťa sa
terminal-type-command = zadajte príkaz · spustí sa, keď bude pripravené · Esc preskočí

setup-tagline-claude = Kódovací agent od Anthropic vo Vmux
setup-tagline-codex = Kódovací agent od OpenAI vo Vmux
setup-tagline-vibe = Kódovací agent od Mistral vo Vmux
setup-install-title = Nainštalovať CLI { $name }
setup-homebrew-required = Na inštaláciu { $command } je potrebný Homebrew a ešte nie je nastavený. Vmux najprv nainštaluje Homebrew a potom { $name }.
setup-terminal-instructions = V termináli stlačte Return na spustenie a po výzve zadajte heslo k Macu.
setup-command-missing = Vmux otvoril túto stránku, pretože lokálny príkaz { $command } ešte nie je nainštalovaný. Spustite príkaz nižšie a získajte ho.
setup-install-failed = Inštalácia sa nedokončila. Skontrolujte podrobnosti v termináli a skúste to znova.
setup-installing = Inštaluje sa…
setup-install-homebrew = Nainštalovať Homebrew + { $name }
setup-run-install = Spustiť inštalačný príkaz
setup-auto-reload = Vmux ho spustí v termináli a znovu načíta, keď bude { $command } pripravený.

debug-title = Ladenie
debug-auto-update = Automatická aktualizácia
debug-simulate-update = Simulovať dostupnú aktualizáciu
debug-simulate-download = Simulovať sťahovanie
debug-clear-update = Vymazať aktualizáciu
debug-trigger-restart = Vyvolať reštart

command-manage-spaces = Spravovať priestory…
command-pane-stack-location = panel { $pane } / vrstva { $stack }
command-space-pane-stack-location = { $space } / panel { $pane } / vrstva { $stack }
command-terminal-path = Terminál ({ $path })
command-group-interactive-mode = Interaktívny režim
command-group-window = Okno
command-group-tab = Karta
command-group-pane = Panel
command-group-stack = Vrstva
command-group-space = Priestor
command-group-navigation = Navigácia
command-group-open = Otvoriť
command-group-view = Zobrazenie
command-group-bar = Lišta

menu-close-vmux = Zavrieť Vmux

agents-terminal-coding-agent = Kódovací agent v termináli
