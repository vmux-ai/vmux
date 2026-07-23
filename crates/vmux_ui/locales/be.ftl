locale-name = беларуская
common-open = Адкрыць
common-close = Закрыць
common-install = Усталяваць
common-uninstall = Выдаліць
common-update = Абнавіць
common-retry = Паўтарыць
common-refresh = Абнавіць
common-remove = Прыбраць
common-enable = Уключыць
common-disable = Адключыць
common-new = Новы
common-active = актыўна
common-running = выконваецца
common-done = гатова
common-failed = Збой
common-installed = Усталявана
common-items = { $count ->
    [one] { $count } элемент
   *[other] { $count } элементаў
}

tools-title = Інструменты
tools-search = Пошук пакетаў, агентаў, MCP, моўных інструментаў і файлаў канфігурацыі…
tools-open = Адкрыць інструменты
tools-fold = Згарнуць інструменты
tools-unfold = Разгарнуць інструменты
tools-scanning = Сканаванне лакальных інструментаў…
tools-no-installed = Няма ўсталяваных інструментаў
tools-empty = Няма адпаведных інструментаў
tools-empty-detail = Усталюйце пакет або дадайце пакет файлаў канфігурацыі ў стылі Stow.
tools-apply = Ужыць
tools-homebrew = Homebrew
tools-homebrew-sync = Усталяваныя формулы і праграмы сінхранізуюцца аўтаматычна.
tools-open-brewfile = Адкрыць Brewfile
tools-managed = кіруецца
tools-provider-homebrew-formulae = Формулы Homebrew
tools-provider-homebrew-casks = Праграмы Homebrew
tools-provider-npm = Пакеты npm
tools-provider-acp-agents = Агенты ACP
tools-provider-language-tools = Моўныя інструменты
tools-provider-mcp-servers = Серверы MCP
tools-provider-dotfiles = Файлы канфігурацыі
tools-status-available = Даступна
tools-status-missing = Адсутнічае
tools-status-conflict = Канфлікт
tools-forget = Забыць
tools-manage = Кіраваць
tools-link = Звязаць
tools-unlink = Адвязаць
tools-import = Імпартаваць
tools-update-count = { $count ->
    [one] 1 абнаўленне
   *[other] { $count } абнаўленняў
}
tools-conflict-count = { $count ->
    [one] 1 канфлікт
   *[other] { $count } канфліктаў
}
tools-result-applied = Інструменты ўжыты
tools-result-imported = Інструменты імпартаваны
tools-result-installed = { $name } усталяваны
tools-result-updated = { $name } абноўлены
tools-result-uninstalled = { $name } выдалены
tools-result-forgotten = { $name } забыты
tools-result-managed = { $name } цяпер кіруецца
tools-result-linked = { $name } звязаны
tools-result-unlinked = { $name } адвязаны
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Сінхранізуйце налады, інструменты, dot-файлы і веды з Git.
vault-sync = Сінхранізацыя
vault-create = Ствараць
vault-connect = Злучыцца
vault-private = Прыватнае сховішча
vault-public-warning = Публічныя сховішчы раскрываюць вашы веды і канфігурацыю.
vault-choose-repository = Выберыце сховішча…
vault-empty = пусты
vault-clean = Актуальна
vault-not-connected = Не падключана
vault-change-count = Змены: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Пачатак
start-tagline = Адзін промпт — і ўсё гатова.

agents-title = Агенты
agents-search = Пошук ACP- і CLI-агентаў…
agents-empty = Падыходных агентаў няма
agents-empty-detail = Паспрабуйце назву, асяроддзе выканання або ACP/CLI.
agents-install-failed = Не ўдалося ўсталяваць
agents-updating = Абнаўленне…
agents-retrying = Паўтор…
agents-preparing = Падрыхтоўка…

extensions-title = Пашырэнні
extensions-search = Пошук сярод усталяваных або ў Chrome Web Store…
extensions-relaunch = Перазапусціць, каб прымяніць
extensions-empty = Пашырэнні не ўсталяваны
extensions-no-match = Падыходных пашырэнняў няма
extensions-empty-detail = Знайдзіце пашырэнне ў Chrome Web Store вышэй і націсніце Enter.
extensions-no-match-detail = Паспрабуйце іншую назву або ID пашырэння.
extensions-on = Укл.
extensions-off = Выкл.
extensions-enable-confirm = Уключыць { $name }?
extensions-enable-permissions = Уключыць { $name } і дазволіць:

lsp-title = Моўныя серверы
lsp-search = Пошук моўных сервераў, лінтараў, фарматараў…
lsp-loading = Загрузка каталога…
lsp-empty = Падыходных моўных сервераў няма
lsp-empty-detail = Паспрабуйце іншую мову, лінтар або фарматар.
lsp-needs = патрэбны { $tool }
lsp-status-available = Даступны
lsp-status-on-path = У PATH
lsp-status-installing = Усталяванне…
lsp-status-installed = Усталявана
lsp-status-outdated = Ёсць абнаўленне
lsp-status-running = Выконваецца
lsp-status-failed = Збой

spaces-title = Прасторы
spaces-new-placeholder = Назва новай прасторы
spaces-empty = Прастор няма
spaces-default-name = Прастора { $number }
spaces-tabs = { $count ->
    [one] 1 укладка
   *[other] { $count } укладак
}
spaces-delete = Выдаліць прастору

team-title = Каманда
team-just-you = У гэтай прасторы толькі вы
team-agents = { $count ->
    [one] Вы і 1 агент
   *[other] Вы і { $count } агентаў
}
team-empty = Тут пакуль нікога няма
team-you = Вы
team-agent = Агент

services-title = Фонавыя службы
services-processes = { $count ->
    [one] 1 працэс
   *[other] { $count } працэсаў
}
services-kill-all = Завяршыць усе прымусова
services-not-running = Служба не запушчана
services-start-with = Запускаць з:
services-empty = Актыўных працэсаў няма
services-filter = Фільтр працэсаў…
services-no-match = Падыходных працэсаў няма
services-connected = Падключана
services-disconnected = Адключана
services-attached = далучана
services-kill = Завяршыць прымусова
services-memory = Памяць
services-size = Памер
services-shell = Абалонка

error-title = Памылка

history-search = Пошук у гісторыі
history-clear-all = Ачысціць усё
history-clear-confirm = Ачысціць усю гісторыю?
history-clear-warning = Гэта дзеянне нельга скасаваць.
history-cancel = Скасаваць
history-today = Сёння
history-yesterday = Учора
history-days-ago = { $count } дзён таму
history-day-offset = Дзень -{ $count }

settings-title = Налады
settings-loading = Загрузка налад…
settings-stored = Захоўваецца ў ~/.vmux/settings.ron
settings-other = Іншае
settings-software-update = Абнаўленне праграмы
settings-check-updates = Праверыць абнаўленні
settings-check-updates-hint = Правяраецца аўтаматычна пры запуску і штогадзіну, калі ўключана аўтаабнаўленне.
settings-update-unavailable = Недаступна
settings-update-unavailable-hint = У гэтай зборцы няма модуля абнаўлення.
settings-update-checking = Праверка…
settings-update-checking-hint = Праверка абнаўленняў…
settings-update-check-again = Праверыць яшчэ раз
settings-update-current = Vmux абноўлены да апошняй версіі.
settings-update-downloading = Спампоўванне…
settings-update-downloading-hint = Спампоўванне Vmux { $version }…
settings-update-installing = Усталяванне…
settings-update-installing-hint = Усталяванне Vmux { $version }…
settings-update-ready = Абнаўленне гатова
settings-update-ready-hint = Vmux { $version } гатовы. Перазапусціце, каб прымяніць абнаўленне.
settings-update-try-again = Паспрабаваць яшчэ раз
settings-update-failed = Не ўдалося праверыць абнаўленні.
settings-item = Элемент
settings-item-number = Элемент { $number }
settings-press-key = Націсніце клавішу…
settings-saved = Захавана
settings-record-key = Націсніце, каб запісаць новую камбінацыю клавіш

tray-open-window = Адкрыць акно
tray-close-window = Закрыць акно
tray-pause-recording = Прыпыніць запіс
tray-resume-recording = Працягнуць запіс
tray-finish-recording = Завяршыць запіс
tray-quit = Выйсці з Vmux

composer-attach-files = Далучыць файлы (/upload)
composer-remove-attachment = Прыбраць укладанне

layout-back = Назад
layout-forward = Наперад
layout-reload = Перазагрузіць
layout-bookmark-page = Дадаць старонку ў закладкі
layout-remove-bookmark = Прыбраць закладку
layout-pin-page = Замацаваць старонку
layout-unpin-page = Адмацаваць старонку
layout-manage-extensions = Кіраваць пашырэннямі
layout-new-stack = Новы стэк
layout-close-tab = Закрыць укладку
layout-bookmark = Закладка
layout-pin = Замацаваць
layout-new-tab = Новая ўкладка
layout-team = Каманда

command-switch-space = Пераключыць прастору…
command-search-ask = Шукаць або спытаць…
command-new-tab-placeholder = Шукайце, увядзіце URL або выберыце Тэрмінал…
command-placeholder = Увядзіце URL, шукайце ўкладкі або > для каманд…
command-composer-placeholder = Увядзіце / для каманд або @ для медыя
command-send = Адправіць (Enter)
command-terminal = Тэрмінал
command-open-terminal = Адкрыць у тэрмінале
command-stack = Стэк
command-tabs = { $count ->
    [one] 1 укладка
   *[other] { $count } укладак
}
command-prompt = Промпт
command-new-tab = Новая ўкладка
command-search = Пошук
command-open-value = Адкрыць «{ $value }»
command-search-value = Шукаць «{ $value }»

schema-appearance = Выгляд
schema-general = Агульныя
schema-layout = Макет
schema-layout-detail = Акно, панэлі, бакавая панэль і контур фокуса.
schema-agent = Агент
schema-agent-detail = Паводзіны агента і дазволы на інструменты.
schema-shortcuts = Спалучэнні клавіш
schema-shortcuts-detail = Толькі прагляд. Каб змяніць прывязкі, рэдагуйце settings.ron непасрэдна.
schema-terminal = Тэрмінал
schema-browser = Браўзер
schema-mode = Рэжым
schema-mode-detail = Каляровая схема для вэб-старонак. «Прылада» выкарыстоўвае сістэмныя налады.
schema-device = Прылада
schema-light = Светлая
schema-dark = Цёмная
schema-language = Мова
schema-language-detail = Выкарыстоўвайце сістэмную, en-US, ja або любы тэг BCP 47 з адпаведным каталогам ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Аўтаабнаўленне
schema-auto-update-detail = Правяраць і ўсталёўваць абнаўленні пры запуску і штогадзіну.
schema-startup-url = Стартавы URL
schema-startup-url-detail = Калі пуста, адкрываецца промпт каманднай панэлі.
schema-search-engine = Пашукавік
schema-search-engine-detail = Выкарыстоўваецца для вэб-пошуку з Пачатку і каманднай панэлі.
schema-window = Акно
schema-pane = Панэль
schema-side-sheet = Бакавая панэль
schema-focus-ring = Контур фокуса
schema-run-placement = Дазволіць перавызначаць месца запуску
schema-run-placement-detail = Дазволіць агентам выбіраць рэжым панэлі запуску, кірунак і прывязку.
schema-leader = Лідар
schema-leader-detail = Прэфіксная клавіша для акордных спалучэнняў.
schema-chord-timeout = Тайм-аўт акорда
schema-chord-timeout-detail = Колькі мілісекунд дзейнічае прэфікс акорда.
schema-bindings = Прывязкі
schema-confirm-close = Пацвярджаць закрыццё
schema-confirm-close-detail = Пытаць перад закрыццём тэрмінала, у якім працуе працэс.
schema-default-theme = Прадвызначаная тэма
schema-default-theme-detail = Назва актыўнай тэмы са спіса тэм.

settings-empty = (пуста)
settings-none = (няма)

schema-system = Сістэма
schema-editor = Рэдактар
schema-recording = Запіс
schema-radius = Радыус
schema-padding = Водступ
schema-gap = Прамежак
schema-width = Шырыня
schema-color = Колер
schema-red = Чырвоны
schema-green = Зялёны
schema-blue = Сіні
schema-follow-files = Сачыць за файламі
schema-tidy-files = Прыбіраць файлы
schema-tidy-files-max = Парог прыборкі файлаў
schema-tidy-files-auto = Прыбіраць файлы аўтаматычна
schema-app-providers = Правайдары праграм
schema-provider = Правайдар
schema-kind = Тып
schema-models = Мадэлі
schema-acp = Агенты ACP
schema-id = ID
schema-name = Назва
schema-command = Каманда
schema-arguments = Аргументы
schema-environment = Асяроддзе
schema-working-directory = Працоўны каталог
schema-shell = Абалонка
schema-font-family = Сямейства шрыфтоў
schema-startup-directory = Пачатковы каталог
schema-themes = Тэмы
schema-color-scheme = Колеравая схема
schema-font-size = Памер шрыфту
schema-line-height = Вышыня радка
schema-cursor-style = Стыль курсора
schema-cursor-blink = Мірганне курсора
schema-custom-themes = Карыстальніцкія тэмы
schema-foreground = Пярэдні план
schema-background = Фон
schema-cursor = Курсор
schema-ansi-colors = Колеры ANSI
schema-keymap = Раскладка клавіш
schema-explorer = Аглядальнік
schema-visible = Бачны
schema-language-servers = Моўныя серверы
schema-servers = Серверы
schema-language-id = ID мовы
schema-root-markers = Маркеры кораня
schema-output-directory = Каталог вываду

menu-scene = Сцэна
menu-layout = Макет
menu-terminal = Тэрмінал
menu-browser = Браўзер
menu-service = Сэрвіс
menu-bookmark = Закладка
menu-edit = Рэдагаванне

layout-knowledge = Веды
layout-open-knowledge = Адкрыць Веды
layout-open-welcome-knowledge = Адкрыць «Вітаем у Ведах»
layout-open-path = Адкрыць { $path }
layout-fold-knowledge = Згарнуць Веды
layout-unfold-knowledge = Разгарнуць Веды
layout-bookmarks = Закладкі
layout-new-folder = Новая папка
layout-add-to-bookmarks = Дадаць у закладкі
layout-move-to-bookmarks = Перамясціць у закладкі
layout-stack-number = Стэк { $number }
layout-fold-stack = Згарнуць стэк
layout-unfold-stack = Разгарнуць стэк
layout-close-stack = Закрыць стэк
layout-bookmark-in = Закладка ў { $folder }

common-cancel = Скасаваць
common-delete = Выдаліць
common-save = Захаваць
common-rename = Перайменаваць
common-expand = Разгарнуць
common-collapse = Згарнуць
common-loading = Загрузка…
common-error = Памылка
common-output = Вывад
common-pending = Чакае
common-current = бягучы
common-stop = Спыніць
services-command = Сэрвіс Vmux
services-uptime-seconds = { $seconds } с
services-uptime-minutes = { $minutes } хв { $seconds } с
services-uptime-hours = { $hours } г { $minutes } хв
services-uptime-days = { $days } д { $hours } г

error-page-failed-load = Не ўдалося загрузіць старонку
error-page-not-found = Старонка не знойдзена
error-unknown-host = Невядомы хост праграмы Vmux: { $host }

history-title = Гісторыя

command-new-app-chat = Новы чат { $provider }/{ $model } (Праграма)
command-interactive-mode-user = Сцэна > Інтэрактыўны рэжым > Карыстальнік
command-interactive-mode-player = Сцэна > Інтэрактыўны рэжым > Прайгравальнік
command-minimize-window = Макет > Акно > Згарнуць
command-toggle-layout = Макет > Макет > Пераключыць макет
command-close-tab = Макет > Укладка > Закрыць укладку
command-new-task = Макет > Укладка > Новая задача…
command-next-tab = Макет > Укладка > Наступная ўкладка
command-prev-tab = Макет > Укладка > Папярэдняя ўкладка
command-rename-tab = Макет > Укладка > Перайменаваць укладку
command-tab-select-1 = Макет > Укладка > Выбраць укладку 1
command-tab-select-2 = Макет > Укладка > Выбраць укладку 2
command-tab-select-3 = Макет > Укладка > Выбраць укладку 3
command-tab-select-4 = Макет > Укладка > Выбраць укладку 4
command-tab-select-5 = Макет > Укладка > Выбраць укладку 5
command-tab-select-6 = Макет > Укладка > Выбраць укладку 6
command-tab-select-7 = Макет > Укладка > Выбраць укладку 7
command-tab-select-8 = Макет > Укладка > Выбраць укладку 8
command-tab-select-last = Макет > Укладка > Выбраць апошнюю ўкладку
command-close-pane = Макет > Панэль > Закрыць панэль
command-select-pane-left = Макет > Панэль > Выбраць панэль злева
command-select-pane-right = Макет > Панэль > Выбраць панэль справа
command-select-pane-up = Макет > Панэль > Выбраць панэль вышэй
command-select-pane-down = Макет > Панэль > Выбраць панэль ніжэй
command-swap-pane-prev = Макет > Панэль > Памяняць з папярэдняй панэллю
command-swap-pane-next = Макет > Панэль > Памяняць з наступнай панэллю
command-equalize-pane-size = Макет > Панэль > Выраўнаваць памер панэляў
command-resize-pane-left = Макет > Панэль > Змяніць памер панэлі ўлева
command-resize-pane-right = Макет > Панэль > Змяніць памер панэлі ўправа
command-resize-pane-up = Макет > Панэль > Змяніць памер панэлі ўверх
command-resize-pane-down = Макет > Панэль > Змяніць памер панэлі ўніз
command-stack-close = Макет > Стос > Закрыць стос
command-stack-next = Макет > Стос > Наступны стос
command-stack-previous = Макет > Стос > Папярэдні стос
command-stack-reopen = Макет > Стос > Зноў адкрыць закрытую старонку
command-stack-swap-prev = Макет > Стос > Перамясціць стос улева
command-stack-swap-next = Макет > Стос > Перамясціць стос управа
command-space-open = Макет > Прастора > Прасторы
command-terminal-close = Тэрмінал > Закрыць тэрмінал
command-terminal-next = Тэрмінал > Наступны тэрмінал
command-terminal-prev = Тэрмінал > Папярэдні тэрмінал
command-terminal-clear = Тэрмінал > Ачысціць тэрмінал
command-browser-prev-page = Браўзер > Навігацыя > Назад
command-browser-next-page = Браўзер > Навігацыя > Наперад
command-browser-reload = Браўзер > Навігацыя > Перазагрузіць
command-browser-hard-reload = Браўзер > Навігацыя > Жорстка перазагрузіць
command-open-in-place = Браўзер > Адкрыць > Адкрыць тут
command-open-in-new-stack = Браўзер > Адкрыць > Адкрыць у новым стосе
command-open-in-pane-top = Браўзер > Адкрыць > Адкрыць у панэлі вышэй
command-open-in-pane-right = Браўзер > Адкрыць > Адкрыць у панэлі справа
command-open-in-pane-bottom = Браўзер > Адкрыць > Адкрыць у панэлі ніжэй
command-open-in-pane-left = Браўзер > Адкрыць > Адкрыць у панэлі злева
command-open-in-new-tab = Браўзер > Адкрыць > Адкрыць у новай укладцы
command-open-in-new-space = Браўзер > Адкрыць > Адкрыць у новай прасторы
command-browser-zoom-in = Браўзер > Выгляд > Павялічыць
command-browser-zoom-out = Браўзер > Выгляд > Паменшыць
command-browser-zoom-reset = Браўзер > Выгляд > Сапраўдны памер
command-browser-dev-tools = Браўзер > Выгляд > Інструменты распрацоўшчыка
command-browser-open-command-bar = Браўзер > Панэль > Панэль каманд
command-browser-open-page-in-command-bar = Браўзер > Панэль > Рэдагаваць старонку
command-browser-open-path-bar = Браўзер > Панэль > Навігатар шляхоў
command-browser-open-commands = Браўзер > Панэль > Каманды
command-browser-open-history = Браўзер > Панэль > Гісторыя
command-service-open = Сэрвіс > Адкрыць манітор сэрвісаў
command-bookmark-toggle-active = Закладка > Дадаць старонку ў закладкі
command-bookmark-pin-active = Закладка > Замацаваць старонку

layout-tab = Укладка
layout-no-stacks = Няма стосаў
layout-loading = Загрузка…
layout-no-markdown-files = Няма файлаў Markdown
layout-empty-folder = Пустая папка
layout-worktree = рабочае дрэва
layout-folder-name = Назва папкі
layout-no-pins-bookmarks = Няма замацаваных старонак або закладак
layout-move-to = Перамясціць у { $folder }
layout-bookmark-current-page = Дадаць бягучую старонку ў закладкі
layout-rename-folder = Перайменаваць папку
layout-remove-folder = Выдаліць папку
layout-update-downloading = Спампоўванне абнаўлення
layout-update-installing = Усталяванне абнаўлення…
layout-update-ready = Даступная новая версія
layout-restart-update = Перазапусціць для абнаўлення

agent-preparing = Падрыхтоўка агента…
agent-send-all-queued = Адправіць усе запыты з чаргі цяпер (Esc)
agent-send = Адправіць (Enter)
agent-ready = Гатовы, калі вы гатовыя.
agent-loading-older = Загрузка старэйшых паведамленняў…
agent-load-older = Загрузіць старэйшыя паведамленні
agent-continued-from = Працяг з { $source }
agent-older-context-omitted = старэйшы кантэкст прапушчаны
agent-interrupted = перапынена
agent-allow-tool = Дазволіць { $tool }?
agent-deny = Адхіліць
agent-allow-always = Заўсёды дазваляць
agent-allow = Дазволіць
agent-loading-sessions = Загрузка сеансаў…
agent-no-resumable-sessions = Сеансаў для аднаўлення не знойдзена
agent-no-matching-sessions = Няма адпаведных сеансаў
agent-no-matching-models = Няма адпаведных мадэляў
agent-choice-help = ↑/↓ або Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Выберыце папку рэпазіторыя
agent-choose-repository-detail = Выберыце лакальны Git-рэпазіторый, які павінен выкарыстоўваць агент.
agent-choosing = Выбар…
agent-choose-folder = Выбраць папку
agent-queued = у чарзе
agent-attached = Далучана:
agent-cancel-queued = Скасаваць запыт у чарзе
agent-resume-queued = Аднавіць запыты ў чарзе
agent-clear-queue = Ачысціць чаргу
agent-send-all-now = адправіць усё цяпер
agent-choose-option = Выберыце варыянт вышэй
agent-loading-media = Загрузка медыя…
agent-no-matching-media = Няма адпаведных медыя
agent-prompt-context = Кантэкст запыту
agent-details = Падрабязнасці
agent-path = Шлях
agent-tool = Інструмент
agent-server = Сервер
agent-bytes = { $count } байт
agent-worked-for = Працаваў { $duration }
agent-worked-for-steps = { $count ->
    [one] Працаваў { $duration } · 1 крок
   *[other] Працаваў { $duration } · { $count } крокаў
}
agent-tool-guardian-review = Праверка Guardian
agent-tool-read-files = Прачытаў файлы
agent-tool-viewed-image = Прагледзеў выяву
agent-tool-used-browser = Выкарыстаў браўзер
agent-tool-searched-files = Шукаў у файлах
agent-tool-ran-commands = Выканаў каманды
agent-thinking = Думае
agent-subagent = Падагент
agent-prompt = Запыт
agent-thread = Ланцужок
agent-parent = Бацькоўскі
agent-children = Даччыныя
agent-call = Выклік
agent-raw-event = Сырая падзея
agent-plan = План
agent-tasks = { $count ->
    [one] 1 задача
   *[other] { $count } задач
}
agent-edited = Зменена
agent-reconnecting = Паўторнае падключэнне { $attempt }/{ $total }
agent-status-running = Выконваецца
agent-status-done = Гатова
agent-status-failed = Збой
agent-status-pending = Чакае
agent-slash-attach-files = Далучыць файлы
agent-slash-resume-session = Аднавіць мінулы сеанс
agent-slash-select-model = Выбраць мадэль
agent-slash-continue-cli = Працягнуць гэты сеанс у CLI
agent-session-just-now = толькі што
agent-session-minutes-ago = { $count } хв таму
agent-session-hours-ago = { $count } г таму
agent-session-days-ago = { $count } д таму
agent-working-working = Працуе
agent-working-thinking = Думае
agent-working-pondering = Разважае
agent-working-noodling = Абдумвае
agent-working-percolating = Выспявае
agent-working-conjuring = Чаруе
agent-working-cooking = Гатуе
agent-working-brewing = Заварвае
agent-working-musing = Разважае
agent-working-ruminating = Перажоўвае думкі
agent-working-scheming = Будуе план
agent-working-synthesizing = Сінтэзуе
agent-working-tinkering = Майструе
agent-working-churning = Апрацоўвае
agent-working-vibing = Ловіць настрой
agent-working-simmering = Томіць
agent-working-crafting = Стварае
agent-working-divining = Варажыць
agent-working-mulling = Абдумвае
agent-working-spelunking = Капаецца ў глыбінях

editor-toggle-explorer = Пераключыць Правадыр (Cmd+B)
editor-unsaved = не захавана
editor-rendered-markdown = Адлюстраваны Markdown з жывым рэдагаваннем
editor-note = Нататка
editor-source-editor = Рэдактар кода
editor-editor = Рэдактар
editor-git-diff = Git-розніца
editor-diff = Розніца
editor-tidy = Прыбіраць
editor-always = Заўсёды
editor-unchanged-previews = { $count ->
    [one] ✦ 1 нязменены предпрогляд
   *[other] ✦ { $count } нязмененых предпроглядаў
}
editor-open-externally = Адкрыць знешне
editor-changed-line = Зменены радок
editor-go-to-definition = Перайсці да вызначэння
editor-find-references = Знайсці спасылкі
editor-references = { $count ->
    [one] 1 спасылка
   *[other] { $count } спасылак
}
editor-lsp-starting = { $server } запускаецца…
editor-lsp-not-installed = { $server } — не ўсталяваны
editor-explorer = Правадыр
editor-open-editors = Адкрытыя рэдактары
editor-outline = Структура
editor-new-file = Новы файл
editor-new-folder = Новая папка
editor-delete-confirm = Выдаліць “{ $name }”? Гэта дзеянне нельга адрабіць.
editor-created-folder = Створана папка { $name }
editor-created-file = Створаны файл { $name }
editor-renamed-to = Перайменавана ў { $name }
editor-deleted = Выдалена { $name }
editor-failed-decode-image = Не ўдалося дэкадаваць выяву
editor-preview-large-image = выява (занадта вялікая для предпрогляду)
editor-preview-binary = двайковы файл
editor-preview-file = файл

git-status-clean = чыста
git-status-modified = зменена
git-status-staged = у індэксе
git-status-staged-modified = у індэксе*
git-status-untracked = не адсочваецца
git-status-deleted = выдалена
git-status-conflict = канфлікт
git-accept-all = ✓ прыняць усё
git-unstage = Прыбраць з індэкса
git-confirm-deny-all = Пацвердзіць адхіленне ўсяго
git-deny-all = ✗ адхіліць усё
git-commit-message = паведамленне каміта
git-commit = Каміт ({ $count })
git-push = ↑ Адправіць
git-loading-diff = Загрузка розніцы…
git-no-changes = Няма змен для паказу
git-accept = ✓ прыняць
git-deny = ✗ адхіліць
git-show-unchanged-lines = Паказаць { $count } нязмененых радкоў

terminal-loading = Загрузка…
terminal-runs-when-ready = запусціцца, калі будзе гатова · Ctrl+C ачышчае · Esc прапускае
terminal-booting = запуск
terminal-type-command = увядзіце каманду · запусціцца, калі будзе гатова · Esc прапускае

setup-tagline-claude = Кодавы агент Anthropic у Vmux
setup-tagline-codex = Кодавы агент OpenAI у Vmux
setup-tagline-vibe = Кодавы агент Mistral у Vmux
setup-install-title = Усталяваць CLI { $name }
setup-homebrew-required = Для ўсталявання { $command } патрэбны Homebrew, але ён яшчэ не наладжаны. Vmux спачатку ўсталюе Homebrew, а потым { $name }.
setup-terminal-instructions = У тэрмінале націсніце Return, каб пачаць, потым увядзіце пароль Mac, калі будзе запыт.
setup-command-missing = Vmux адкрыў гэтую старонку, бо лакальная каманда { $command } яшчэ не ўсталявана. Выканайце каманду ніжэй, каб атрымаць яе.
setup-install-failed = Усталяванне не завершана. Праверце падрабязнасці ў тэрмінале і паўтарыце спробу.
setup-installing = Усталяванне…
setup-install-homebrew = Усталяваць Homebrew + { $name }
setup-run-install = Запусціць каманду ўсталявання
setup-auto-reload = Vmux запускае яе ў тэрмінале і перазагружае старонку, калі { $command } будзе гатова.

debug-title = Адладка
debug-auto-update = Аўтаабнаўленне
debug-simulate-update = Зымітаваць даступнае абнаўленне
debug-simulate-download = Зымітаваць спампоўванне
debug-clear-update = Ачысціць абнаўленне
debug-trigger-restart = Запусціць перазапуск

command-manage-spaces = Кіраваць прасторамі…
command-pane-stack-location = панэль { $pane } / стос { $stack }
command-space-pane-stack-location = { $space } / панэль { $pane } / стос { $stack }
command-terminal-path = Тэрмінал ({ $path })
command-group-interactive-mode = Інтэрактыўны рэжым
command-group-window = Акно
command-group-tab = Укладка
command-group-pane = Панэль
command-group-stack = Стос
command-group-space = Прастора
command-group-navigation = Навігацыя
command-group-open = Адкрыць
command-group-view = Выгляд
command-group-bar = Панэль

menu-close-vmux = Закрыць Vmux

agents-terminal-coding-agent = Тэрмінальны агент для праграмавання
