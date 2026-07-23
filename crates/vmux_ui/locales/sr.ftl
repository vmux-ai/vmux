locale-name = српски
common-open = Отвори
common-close = Затвори
common-install = Инсталирај
common-uninstall = Деинсталирај
common-update = Ажурирај
common-retry = Покушај поново
common-refresh = Освежи
common-remove = Уклони
common-enable = Омогући
common-disable = Онемогући
common-new = Ново
common-active = активно
common-running = покренуто
common-done = готово
common-failed = Није успело
common-installed = Инсталирано
common-items = { $count ->
    [one] { $count } ставка
   *[other] { $count } ставки
}

tools-title = Алатке
tools-search = Претражи пакете, агенте, MCP, језичке алатке и конфигурационе датотеке…
tools-open = Отвори алатке
tools-fold = Скупи алатке
tools-unfold = Прошири алатке
tools-scanning = Скенирање локалних алатки…
tools-no-installed = Нема инсталираних алатки
tools-empty = Нема одговарајућих алатки
tools-empty-detail = Инсталирајте пакет или додајте пакет конфигурационих датотека у стилу Stow.
tools-apply = Примени
tools-homebrew = Homebrew
tools-homebrew-sync = Инсталиране формуле и апликације аутоматски се синхронизују.
tools-open-brewfile = Отвори Brewfile
tools-managed = управљано
tools-provider-homebrew-formulae = Homebrew формуле
tools-provider-homebrew-casks = Homebrew апликације
tools-provider-npm = npm пакети
tools-provider-acp-agents = ACP агенти
tools-provider-language-tools = Језичке алатке
tools-provider-mcp-servers = MCP сервери
tools-provider-dotfiles = Конфигурационе датотеке
tools-status-available = Доступно
tools-status-missing = Недостаје
tools-status-conflict = Сукоб
tools-forget = Заборави
tools-manage = Управљај
tools-link = Повежи
tools-unlink = Прекини везу
tools-import = Увези
tools-update-count = { $count ->
    [one] 1 ажурирање
   *[other] { $count } ажурирања
}
tools-conflict-count = { $count ->
    [one] 1 сукоб
   *[other] { $count } сукоба
}
tools-result-applied = Алатке су примењене
tools-result-imported = Алатке су увезене
tools-result-installed = { $name } је инсталиран
tools-result-updated = { $name } је ажуриран
tools-result-uninstalled = { $name } је деинсталиран
tools-result-forgotten = { $name } је заборављен
tools-result-managed = { $name } је сада под управљањем
tools-result-linked = { $name } је повезан
tools-result-unlinked = Веза са { $name } је прекинута

start-title = Почетак
start-tagline = Један prompt. Све завршено.

agents-title = Агенти
agents-search = Претражите ACP и CLI агенте…
agents-empty = Нема одговарајућих агената
agents-empty-detail = Покушајте са називом, runtime-ом или ACP/CLI.
agents-install-failed = Инсталација није успела
agents-updating = Ажурирање…
agents-retrying = Поновни покушај…
agents-preparing = Припрема…

extensions-title = Проширења
extensions-search = Претражите инсталирана проширења или Chrome Web Store…
extensions-relaunch = Поново покрените за примену
extensions-empty = Нема инсталираних проширења
extensions-no-match = Нема одговарајућих проширења
extensions-empty-detail = Претражите Chrome Web Store изнад и притисните Enter.
extensions-no-match-detail = Покушајте са другим називом или ID-јем проширења.
extensions-on = Укључено
extensions-off = Искључено
extensions-enable-confirm = Омогућити { $name }?
extensions-enable-permissions = Омогућите { $name } и дозволите:

lsp-title = Језички сервери
lsp-search = Претражите језичке сервере, линтере, форматере…
lsp-loading = Учитавање каталога…
lsp-empty = Нема одговарајућих језичких сервера
lsp-empty-detail = Покушајте са другим језиком, линтером или форматером.
lsp-needs = захтева { $tool }
lsp-status-available = Доступно
lsp-status-on-path = На PATH-у
lsp-status-installing = Инсталирање…
lsp-status-installed = Инсталирано
lsp-status-outdated = Доступно ажурирање
lsp-status-running = Покренуто
lsp-status-failed = Није успело

spaces-title = Простори
spaces-new-placeholder = Назив новог простора
spaces-empty = Нема простора
spaces-default-name = Простор { $number }
spaces-tabs = { $count ->
    [one] 1 картица
   *[other] { $count } картица
}
spaces-delete = Обриши простор

team-title = Тим
team-just-you = Само ви у овом простору
team-agents = { $count ->
    [one] Ви и 1 агент
   *[other] Ви и { $count } агената
}
team-empty = Овде још нема никога
team-you = Ви
team-agent = Агент

services-title = Позадински сервиси
services-processes = { $count ->
    [one] 1 процес
   *[other] { $count } процеса
}
services-kill-all = Прекини све
services-not-running = Сервис није покренут
services-start-with = Покрени са:
services-empty = Нема активних процеса
services-filter = Филтрирај процесе…
services-no-match = Нема одговарајућих процеса
services-connected = Повезано
services-disconnected = Прекинута веза
services-attached = повезано
services-kill = Прекини
services-memory = Меморија
services-size = Величина
services-shell = Shell

error-title = Грешка

history-search = Претражи историју
history-clear-all = Обриши све
history-clear-confirm = Обрисати целу историју?
history-clear-warning = Ово није могуће опозвати.
history-cancel = Откажи
history-today = Данас
history-yesterday = Јуче
history-days-ago = пре { $count } дана
history-day-offset = Дан -{ $count }

settings-title = Подешавања
settings-loading = Учитавање подешавања…
settings-stored = Чува се у ~/.vmux/settings.ron
settings-other = Остало
settings-software-update = Ажурирање софтвера
settings-check-updates = Провери ажурирања
settings-check-updates-hint = Проверава се аутоматски при покретању и сваког сата када је аутоматско ажурирање омогућено.
settings-update-unavailable = Недоступно
settings-update-unavailable-hint = Ажурирач није укључен у ово издање.
settings-update-checking = Провера…
settings-update-checking-hint = Провера ажурирања…
settings-update-check-again = Провери поново
settings-update-current = Vmux је ажуран.
settings-update-downloading = Преузимање…
settings-update-downloading-hint = Преузима се Vmux { $version }…
settings-update-installing = Инсталирање…
settings-update-installing-hint = Инсталира се Vmux { $version }…
settings-update-ready = Ажурирање је спремно
settings-update-ready-hint = Vmux { $version } је спреман. Поново покрените да бисте га применили.
settings-update-try-again = Покушај поново
settings-update-failed = Није могуће проверити ажурирања.
settings-item = Ставка
settings-item-number = Ставка { $number }
settings-press-key = Притисните тастер…
settings-saved = Сачувано
settings-record-key = Кликните да снимите нову комбинацију тастера

tray-open-window = Отвори прозор
tray-close-window = Затвори прозор
tray-pause-recording = Паузирај снимање
tray-resume-recording = Настави снимање
tray-finish-recording = Заврши снимање
tray-quit = Затвори Vmux

composer-attach-files = Приложи датотеке (/upload)
composer-remove-attachment = Уклони прилог

layout-back = Назад
layout-forward = Напред
layout-reload = Поново учитај
layout-bookmark-page = Додај страницу у обележиваче
layout-remove-bookmark = Уклони обележивач
layout-pin-page = Закачи ову страницу
layout-unpin-page = Откачи ову страницу
layout-manage-extensions = Управљај проширењима
layout-new-stack = Нови стек
layout-close-tab = Затвори картицу
layout-bookmark = Обележивач
layout-pin = Закачи
layout-new-tab = Нова картица
layout-team = Тим

command-switch-space = Промени простор…
command-search-ask = Претражите или питајте…
command-new-tab-placeholder = Претражите, унесите URL или изаберите Terminal…
command-placeholder = Унесите URL, претражите картице или > за команде…
command-composer-placeholder = Унесите / за команде или @ за медије
command-send = Пошаљи (Enter)
command-terminal = Терминал
command-open-terminal = Отвори у терминалу
command-stack = Стек
command-tabs = { $count ->
    [one] 1 картица
   *[other] { $count } картица
}
command-prompt = Prompt
command-new-tab = Нова картица
command-search = Претрага
command-open-value = Отвори „{ $value }”
command-search-value = Претражи „{ $value }”

schema-appearance = Изглед
schema-general = Опште
schema-layout = Распоред
schema-layout-detail = Прозор, панели, бочна трака и оквир фокуса.
schema-agent = Агент
schema-agent-detail = Понашање агента и дозволе за алате.
schema-shortcuts = Пречице
schema-shortcuts-detail = Само за читање. Измените settings.ron директно да промените пречице.
schema-terminal = Терминал
schema-browser = Прегледач
schema-mode = Режим
schema-mode-detail = Шема боја за веб-странице. Уређај прати систем.
schema-device = Уређај
schema-light = Светла
schema-dark = Тамна
schema-language = Језик
schema-language-detail = Користите системски, en-US, ja или било коју BCP 47 ознаку са одговарајућим каталогом ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Аутоматско ажурирање
schema-auto-update-detail = Проверавај и инсталирај ажурирања при покретању и сваког сата.
schema-startup-url = Почетни URL
schema-startup-url-detail = Ако је празно, отвара се prompt командне траке.
schema-search-engine = Претраживач
schema-search-engine-detail = Користи се за веб-претраге са почетне странице и из командне траке.
schema-window = Прозор
schema-pane = Панел
schema-side-sheet = Бочни лист
schema-focus-ring = Оквир фокуса
schema-run-placement = Дозволи замену положаја покретања
schema-run-placement-detail = Дозволите агентима да изаберу режим, смер и сидро панела за покретање.
schema-leader = Лидер
schema-leader-detail = Префикс тастер за chord пречице.
schema-chord-timeout = Истек chord-а
schema-chord-timeout-detail = Милисекунде пре него што префикс chord-а истекне.
schema-bindings = Везе
schema-confirm-close = Потврди затварање
schema-confirm-close-detail = Питај пре затварања терминала са покренутим процесом.
schema-default-theme = Подразумевана тема
schema-default-theme-detail = Назив активне теме са листе тема.

settings-empty = (prazno)
settings-none = (ništa)

schema-system = Sistem
schema-editor = Uređivač
schema-recording = Snimanje
schema-radius = Poluprečnik
schema-padding = Unutrašnja margina
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
schema-app-providers = Dobavljači aplikacija
schema-provider = Dobavljač
schema-kind = Vrsta
schema-models = Modeli
schema-acp = ACP agenti
schema-id = ID
schema-name = Naziv
schema-command = Komanda
schema-arguments = Argumenti
schema-environment = Okruženje
schema-working-directory = Radni direktorijum
schema-shell = Shell
schema-font-family = Porodica fonta
schema-startup-directory = Početni direktorijum
schema-themes = Teme
schema-color-scheme = Šema boja
schema-font-size = Veličina fonta
schema-line-height = Visina reda
schema-cursor-style = Stil kursora
schema-cursor-blink = Treperenje kursora
schema-custom-themes = Prilagođene teme
schema-foreground = Prednji plan
schema-background = Pozadina
schema-cursor = Kursor
schema-ansi-colors = ANSI boje
schema-keymap = Mapa tastera
schema-explorer = Istraživač
schema-visible = Vidljivo
schema-language-servers = Jezički serveri
schema-servers = Serveri
schema-language-id = ID jezika
schema-root-markers = Oznake korena
schema-output-directory = Izlazni direktorijum

menu-scene = Scena
menu-layout = Raspored
menu-terminal = Terminal
menu-browser = Pregledač
menu-service = Servis
menu-bookmark = Obeleživač
menu-edit = Uređivanje

layout-knowledge = Znanje
layout-open-knowledge = Otvori Znanje
layout-open-welcome-knowledge = Otvori Dobro došli u Znanje
layout-open-path = Otvori { $path }
layout-fold-knowledge = Sklopi znanje
layout-unfold-knowledge = Rasklopi znanje
layout-bookmarks = Obeleživači
layout-new-folder = Nova fascikla
layout-add-to-bookmarks = Dodaj u obeleživače
layout-move-to-bookmarks = Premesti u obeleživače
layout-stack-number = Stek { $number }
layout-fold-stack = Sklopi stek
layout-unfold-stack = Rasklopi stek
layout-close-stack = Zatvori stek
layout-bookmark-in = Obeleži u { $folder }

common-cancel = Otkaži
common-delete = Obriši
common-save = Sačuvaj
common-rename = Preimenuj
common-expand = Proširi
common-collapse = Skupi
common-loading = Učitavanje…
common-error = Greška
common-output = Izlaz
common-pending = Na čekanju
common-current = trenutno
common-stop = Zaustavi
services-command = Vmux servis
services-uptime-seconds = { $seconds } s
services-uptime-minutes = { $minutes } min { $seconds } s
services-uptime-hours = { $hours } č { $minutes } min
services-uptime-days = { $days } d { $hours } č

error-page-failed-load = Učitavanje stranice nije uspelo
error-page-not-found = Stranica nije pronađena
error-unknown-host = Nepoznat host Vmux aplikacije: { $host }

history-title = Istorija

command-new-app-chat = Novo { $provider }/{ $model } ćaskanje (aplikacija)
command-interactive-mode-user = Scena > Interaktivni režim > Korisnik
command-interactive-mode-player = Scena > Interaktivni režim > Igrač
command-minimize-window = Raspored > Prozor > Minimizuj
command-toggle-layout = Raspored > Raspored > Prebaci raspored
command-close-tab = Raspored > Kartica > Zatvori karticu
command-new-task = Raspored > Kartica > Novi zadatak…
command-next-tab = Raspored > Kartica > Sledeća kartica
command-prev-tab = Raspored > Kartica > Prethodna kartica
command-rename-tab = Raspored > Kartica > Preimenuj karticu
command-tab-select-1 = Raspored > Kartica > Izaberi karticu 1
command-tab-select-2 = Raspored > Kartica > Izaberi karticu 2
command-tab-select-3 = Raspored > Kartica > Izaberi karticu 3
command-tab-select-4 = Raspored > Kartica > Izaberi karticu 4
command-tab-select-5 = Raspored > Kartica > Izaberi karticu 5
command-tab-select-6 = Raspored > Kartica > Izaberi karticu 6
command-tab-select-7 = Raspored > Kartica > Izaberi karticu 7
command-tab-select-8 = Raspored > Kartica > Izaberi karticu 8
command-tab-select-last = Raspored > Kartica > Izaberi poslednju karticu
command-close-pane = Raspored > Okno > Zatvori okno
command-select-pane-left = Raspored > Okno > Izaberi okno levo
command-select-pane-right = Raspored > Okno > Izaberi okno desno
command-select-pane-up = Raspored > Okno > Izaberi okno gore
command-select-pane-down = Raspored > Okno > Izaberi okno dole
command-swap-pane-prev = Raspored > Okno > Zameni s prethodnim oknom
command-swap-pane-next = Raspored > Okno > Zameni sa sledećim oknom
command-equalize-pane-size = Raspored > Okno > Izjednači veličinu okana
command-resize-pane-left = Raspored > Okno > Promeni veličinu okna ulevo
command-resize-pane-right = Raspored > Okno > Promeni veličinu okna udesno
command-resize-pane-up = Raspored > Okno > Promeni veličinu okna nagore
command-resize-pane-down = Raspored > Okno > Promeni veličinu okna nadole
command-stack-close = Raspored > Stek > Zatvori stek
command-stack-next = Raspored > Stek > Sledeći stek
command-stack-previous = Raspored > Stek > Prethodni stek
command-stack-reopen = Raspored > Stek > Ponovo otvori zatvorenu stranicu
command-stack-swap-prev = Raspored > Stek > Pomeri stek ulevo
command-stack-swap-next = Raspored > Stek > Pomeri stek udesno
command-space-open = Raspored > Prostor > Prostori
command-terminal-close = Terminal > Zatvori terminal
command-terminal-next = Terminal > Sledeći terminal
command-terminal-prev = Terminal > Prethodni terminal
command-terminal-clear = Terminal > Očisti terminal
command-browser-prev-page = Pregledač > Navigacija > Nazad
command-browser-next-page = Pregledač > Navigacija > Napred
command-browser-reload = Pregledač > Navigacija > Ponovo učitaj
command-browser-hard-reload = Pregledač > Navigacija > Prinudno ponovo učitaj
command-open-in-place = Pregledač > Otvori > Otvori ovde
command-open-in-new-stack = Pregledač > Otvori > Otvori u novom steku
command-open-in-pane-top = Pregledač > Otvori > Otvori u oknu iznad
command-open-in-pane-right = Pregledač > Otvori > Otvori u oknu desno
command-open-in-pane-bottom = Pregledač > Otvori > Otvori u oknu ispod
command-open-in-pane-left = Pregledač > Otvori > Otvori u oknu levo
command-open-in-new-tab = Pregledač > Otvori > Otvori u novoj kartici
command-open-in-new-space = Pregledač > Otvori > Otvori u novom prostoru
command-browser-zoom-in = Pregledač > Prikaz > Uvećaj
command-browser-zoom-out = Pregledač > Prikaz > Umanji
command-browser-zoom-reset = Pregledač > Prikaz > Stvarna veličina
command-browser-dev-tools = Pregledač > Prikaz > Alatke za programere
command-browser-open-command-bar = Pregledač > Traka > Komandna traka
command-browser-open-page-in-command-bar = Pregledač > Traka > Uredi stranicu
command-browser-open-path-bar = Pregledač > Traka > Navigator putanje
command-browser-open-commands = Pregledač > Traka > Komande
command-browser-open-history = Pregledač > Traka > Istorija
command-service-open = Servis > Otvori nadzor servisa
command-bookmark-toggle-active = Obeleživač > Obeleži stranicu
command-bookmark-pin-active = Obeleživač > Prikači stranicu

layout-tab = Kartica
layout-no-stacks = Nema stekova
layout-loading = Učitavanje…
layout-no-markdown-files = Nema Markdown datoteka
layout-empty-folder = Prazna fascikla
layout-worktree = radno stablo
layout-folder-name = Naziv fascikle
layout-no-pins-bookmarks = Nema prikačenih ni obeleživača
layout-move-to = Premesti u { $folder }
layout-bookmark-current-page = Obeleži trenutnu stranicu
layout-rename-folder = Preimenuj fasciklu
layout-remove-folder = Ukloni fasciklu
layout-update-downloading = Preuzimanje ažuriranja
layout-update-installing = Instaliranje ažuriranja…
layout-update-ready = Dostupna je nova verzija
layout-restart-update = Ponovo pokreni za ažuriranje

agent-preparing = Priprema agenta…
agent-send-all-queued = Pošalji sve promptove na čekanju sada (Esc)
agent-send = Pošalji (Enter)
agent-ready = Spreman kad i vi.
agent-loading-older = Učitavanje starijih poruka…
agent-load-older = Učitaj starije poruke
agent-continued-from = Nastavljeno iz { $source }
agent-older-context-omitted = stariji kontekst izostavljen
agent-interrupted = prekinuto
agent-allow-tool = Dozvoliti { $tool }?
agent-deny = Odbij
agent-allow-always = Uvek dozvoli
agent-allow = Dozvoli
agent-loading-sessions = Učitavanje sesija…
agent-no-resumable-sessions = Nisu pronađene sesije koje se mogu nastaviti
agent-no-matching-sessions = Nema odgovarajućih sesija
agent-no-matching-models = Nema odgovarajućih modela
agent-choice-help = ↑/↓ ili Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Izaberite fasciklu repozitorijuma
agent-choose-repository-detail = Izaberite lokalni Git repozitorijum koji agent treba da koristi.
agent-choosing = Biranje…
agent-choose-folder = Izaberite fasciklu
agent-queued = u redu
agent-attached = Priloženo:
agent-cancel-queued = Otkaži prompt na čekanju
agent-resume-queued = Nastavi promptove na čekanju
agent-clear-queue = Očisti red
agent-send-all-now = pošalji sve sada
agent-choose-option = Izaberite opciju iznad
agent-loading-media = Učitavanje medija…
agent-no-matching-media = Nema odgovarajućih medija
agent-prompt-context = Kontekst prompta
agent-details = Detalji
agent-path = Putanja
agent-tool = Alatka
agent-server = Server
agent-bytes = { $count } bajtova
agent-worked-for = Radio { $duration }
agent-worked-for-steps = { $count ->
    [one] Radio { $duration } · 1 korak
   *[other] Radio { $duration } · { $count } koraka
}
agent-tool-guardian-review = Guardian pregled
agent-tool-read-files = Pročitane datoteke
agent-tool-viewed-image = Pregledana slika
agent-tool-used-browser = Korišćen pregledač
agent-tool-searched-files = Pretražene datoteke
agent-tool-ran-commands = Pokrenute komande
agent-thinking = Razmišlja
agent-subagent = Podagent
agent-prompt = Prompt
agent-thread = Nit
agent-parent = Nadređeno
agent-children = Podređeno
agent-call = Poziv
agent-raw-event = Sirovi događaj
agent-plan = Plan
agent-tasks = { $count ->
    [one] 1 zadatak
   *[other] { $count } zadataka
}
agent-edited = Izmenjeno
agent-reconnecting = Ponovno povezivanje { $attempt }/{ $total }
agent-status-running = Pokrenuto
agent-status-done = Gotovo
agent-status-failed = Nije uspelo
agent-status-pending = Na čekanju
agent-slash-attach-files = Priloži datoteke
agent-slash-resume-session = Nastavi prethodnu sesiju
agent-slash-select-model = Izaberi model
agent-slash-continue-cli = Nastavi ovu sesiju u CLI-ju
agent-session-just-now = upravo sada
agent-session-minutes-ago = pre { $count } min
agent-session-hours-ago = pre { $count } č
agent-session-days-ago = pre { $count } d
agent-working-working = Radi
agent-working-thinking = Razmišlja
agent-working-pondering = Promišlja
agent-working-noodling = Razrađuje
agent-working-percolating = Krčka
agent-working-conjuring = Smišlja
agent-working-cooking = Kuva
agent-working-brewing = Sprema
agent-working-musing = Razmatra
agent-working-ruminating = Premišlja
agent-working-scheming = Planira
agent-working-synthesizing = Sintetizuje
agent-working-tinkering = Petlja
agent-working-churning = Obrađuje
agent-working-vibing = U ritmu je
agent-working-simmering = Lagano krčka
agent-working-crafting = Sklapa
agent-working-divining = Proniče
agent-working-mulling = Mozga
agent-working-spelunking = Kopa

editor-toggle-explorer = Prikaži/sakrij Explorer (Cmd+B)
editor-unsaved = nesačuvano
editor-rendered-markdown = Renderovan Markdown uz uređivanje uživo
editor-note = Napomena
editor-source-editor = Uređivač izvornog koda
editor-editor = Uređivač
editor-git-diff = Git diff
editor-diff = Diff
editor-tidy = Sredi
editor-always = Uvek
editor-unchanged-previews = { $count ->
    [one] ✦ 1 neizmenjen pregled
   *[other] ✦ { $count } neizmenjenih pregleda
}
editor-open-externally = Otvori spolja
editor-changed-line = Izmenjen red
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
editor-new-folder = Nova fascikla
editor-delete-confirm = Obrisati „{ $name }”? Ovo se ne može opozvati.
editor-created-folder = Napravljena fascikla { $name }
editor-created-file = Napravljena datoteka { $name }
editor-renamed-to = Preimenovano u { $name }
editor-deleted = Obrisano { $name }
editor-failed-decode-image = Dekodiranje slike nije uspelo
editor-preview-large-image = slika (prevelika za pregled)
editor-preview-binary = binarna datoteka
editor-preview-file = datoteka

git-status-clean = čisto
git-status-modified = izmenjeno
git-status-staged = pripremljeno
git-status-staged-modified = pripremljeno*
git-status-untracked = nepraćeno
git-status-deleted = obrisano
git-status-conflict = konflikt
git-accept-all = ✓ prihvati sve
git-unstage = Ukloni iz pripreme
git-confirm-deny-all = Potvrdi odbijanje svega
git-deny-all = ✗ odbij sve
git-commit-message = poruka commita
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Učitavanje diff-a…
git-no-changes = Nema izmena za prikaz
git-accept = ✓ prihvati
git-deny = ✗ odbij
git-show-unchanged-lines = Prikaži { $count } neizmenjenih redova

terminal-loading = Učitavanje…
terminal-runs-when-ready = pokreće se kad bude spremno · Ctrl+C čisti · Esc preskače
terminal-booting = pokretanje
terminal-type-command = unesite komandu · pokreće se kad bude spremno · Esc preskače

setup-tagline-claude = Anthropic-ov agent za kodiranje, u Vmux-u
setup-tagline-codex = OpenAI-jev agent za kodiranje, u Vmux-u
setup-tagline-vibe = Mistral-ov agent za kodiranje, u Vmux-u
setup-install-title = Instaliraj { $name } CLI
setup-homebrew-required = Homebrew je potreban za instaliranje { $command }, a još nije podešen. Vmux će prvo instalirati Homebrew, pa zatim { $name }.
setup-terminal-instructions = U terminalu pritisnite Return za početak, a zatim unesite lozinku za Mac kada se zatraži.
setup-command-missing = Vmux je otvorio ovu stranicu jer lokalna komanda { $command } još nije instalirana. Pokrenite komandu ispod da biste je dobili.
setup-install-failed = Instalacija nije završena. Proverite detalje u terminalu, pa pokušajte ponovo.
setup-installing = Instaliranje…
setup-install-homebrew = Instaliraj Homebrew + { $name }
setup-run-install = Pokreni komandu za instalaciju
setup-auto-reload = Vmux je pokreće u terminalu i ponovo učitava kada { $command } bude spremna.

debug-title = Otklanjanje grešaka
debug-auto-update = Automatsko ažuriranje
debug-simulate-update = Simuliraj dostupno ažuriranje
debug-simulate-download = Simuliraj preuzimanje
debug-clear-update = Očisti ažuriranje
debug-trigger-restart = Pokreni ponovno pokretanje

command-manage-spaces = Upravljaj prostorima…
command-pane-stack-location = okno { $pane } / stek { $stack }
command-space-pane-stack-location = { $space } / okno { $pane } / stek { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Interaktivni režim
command-group-window = Prozor
command-group-tab = Kartica
command-group-pane = Okno
command-group-stack = Stek
command-group-space = Prostor
command-group-navigation = Navigacija
command-group-open = Otvaranje
command-group-view = Prikaz
command-group-bar = Traka

menu-close-vmux = Zatvori Vmux

agents-terminal-coding-agent = Agentski programer u terminalu
