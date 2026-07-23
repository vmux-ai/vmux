locale-name = български
common-open = Отвори
common-close = Затвори
common-install = Инсталирай
common-uninstall = Деинсталирай
common-update = Обнови
common-retry = Опитай пак
common-refresh = Опресни
common-remove = Премахни
common-enable = Включи
common-disable = Изключи
common-new = Нов
common-active = активно
common-running = работи
common-done = готово
common-failed = Неуспешно
common-installed = Инсталирано
common-items = { $count ->
    [one] { $count } елемент
   *[other] { $count } елемента
}

tools-title = Инструменти
tools-search = Търсене на пакети, агенти, MCP, езикови инструменти и конфигурационни файлове…
tools-open = Отваряне на инструментите
tools-fold = Свиване на инструментите
tools-unfold = Разгъване на инструментите
tools-scanning = Сканиране на локалните инструменти…
tools-no-installed = Няма инсталирани инструменти
tools-empty = Няма съвпадащи инструменти
tools-empty-detail = Инсталирайте пакет или добавете пакет с конфигурационни файлове в стил Stow.
tools-apply = Прилагане
tools-homebrew = Homebrew
tools-homebrew-sync = Инсталираните формули и приложения се синхронизират автоматично.
tools-open-brewfile = Отваряне на Brewfile
tools-managed = управляван
tools-provider-homebrew-formulae = Формули на Homebrew
tools-provider-homebrew-casks = Приложения на Homebrew
tools-provider-npm = Пакети на npm
tools-provider-acp-agents = Агенти на ACP
tools-provider-language-tools = Езикови инструменти
tools-provider-mcp-servers = Сървъри на MCP
tools-provider-dotfiles = Конфигурационни файлове
tools-status-available = Налично
tools-status-missing = Липсва
tools-status-conflict = Конфликт
tools-forget = Забравяне
tools-manage = Управление
tools-link = Свързване
tools-unlink = Прекъсване на връзката
tools-import = Импортиране
tools-update-count = { $count ->
    [one] 1 актуализация
   *[other] { $count } актуализации
}
tools-conflict-count = { $count ->
    [one] 1 конфликт
   *[other] { $count } конфликта
}
tools-result-applied = Инструментите са приложени
tools-result-imported = Инструментите са импортирани
tools-result-installed = { $name } е инсталиран
tools-result-updated = { $name } е актуализиран
tools-result-uninstalled = { $name } е деинсталиран
tools-result-forgotten = { $name } е забравен
tools-result-managed = { $name } вече се управлява
tools-result-linked = { $name } е свързан
tools-result-unlinked = Връзката с { $name } е прекъсната
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Синхронизирайте настройки, инструменти, dotfiles и знания с Git.
vault-sync = Синхр
vault-create = Създавайте
vault-connect = Свържете се
vault-private = Частно хранилище
vault-public-warning = Публичните хранилища разкриват вашето знание и конфигурация.
vault-choose-repository = Изберете хранилище...
vault-empty = празен
vault-clean = В крак с времето
vault-not-connected = Не е свързан
vault-change-count = Промени: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Старт
start-tagline = Един prompt. Всичко — готово.

agents-title = Агенти
agents-search = Търсене в ACP и CLI агенти…
agents-empty = Няма съвпадащи агенти
agents-empty-detail = Опитайте с име, среда за изпълнение или ACP/CLI.
agents-install-failed = Инсталирането е неуспешно
agents-updating = Обновяване…
agents-retrying = Нов опит…
agents-preparing = Подготовка…

extensions-title = Разширения
extensions-search = Търсене в инсталираните или Chrome Web Store…
extensions-relaunch = Рестартирайте, за да се приложи
extensions-empty = Няма инсталирани разширения
extensions-no-match = Няма съвпадащи разширения
extensions-empty-detail = Потърсете в Chrome Web Store по-горе и натиснете Enter.
extensions-no-match-detail = Опитайте с друго име или ID на разширение.
extensions-on = Вкл.
extensions-off = Изкл.
extensions-enable-confirm = Да се включи ли { $name }?
extensions-enable-permissions = Включване на { $name } и разрешаване на:

lsp-title = Езикови сървъри
lsp-search = Търсене на езикови сървъри, линтери, форматери…
lsp-loading = Зареждане на каталога…
lsp-empty = Няма съвпадащи езикови сървъри
lsp-empty-detail = Опитайте с друг език, линтер или форматер.
lsp-needs = изисква { $tool }
lsp-status-available = Налично
lsp-status-on-path = В PATH
lsp-status-installing = Инсталиране…
lsp-status-installed = Инсталирано
lsp-status-outdated = Налично е обновяване
lsp-status-running = Работи
lsp-status-failed = Неуспешно

spaces-title = Пространства
spaces-new-placeholder = Име на ново пространство
spaces-empty = Няма пространства
spaces-default-name = Пространство { $number }
spaces-tabs = { $count ->
    [one] 1 раздел
   *[other] { $count } раздела
}
spaces-delete = Изтрий пространство

team-title = Екип
team-just-you = Само вие сте в това пространство
team-agents = { $count ->
    [one] Вие и 1 агент
   *[other] Вие и { $count } агента
}
team-empty = Тук още няма никого
team-you = Вие
team-agent = Агент

services-title = Фонови услуги
services-processes = { $count ->
    [one] 1 процес
   *[other] { $count } процеса
}
services-kill-all = Прекрати всички
services-not-running = Услугата не работи
services-start-with = Стартиране с:
services-empty = Няма активни процеси
services-filter = Филтриране на процеси…
services-no-match = Няма съвпадащи процеси
services-connected = Свързано
services-disconnected = Прекъснато
services-attached = прикачено
services-kill = Прекрати
services-memory = Памет
services-size = Размер
services-shell = Обвивка

error-title = Грешка

history-search = Търсене в историята
history-clear-all = Изчисти всичко
history-clear-confirm = Да се изчисти ли цялата история?
history-clear-warning = Това действие не може да бъде отменено.
history-cancel = Отказ
history-today = Днес
history-yesterday = Вчера
history-days-ago = преди { $count } дни
history-day-offset = Ден -{ $count }

settings-title = Настройки
settings-loading = Зареждане на настройките…
settings-stored = Записано в ~/.vmux/settings.ron
settings-other = Други
settings-software-update = Обновяване на софтуера
settings-check-updates = Провери за обновявания
settings-check-updates-hint = Проверява автоматично при стартиране и на всеки час, когато автоматичното обновяване е включено.
settings-update-unavailable = Недостъпно
settings-update-unavailable-hint = Модулът за обновяване не е включен в тази компилация.
settings-update-checking = Проверка…
settings-update-checking-hint = Проверка за обновявания…
settings-update-check-again = Провери отново
settings-update-current = Vmux е актуален.
settings-update-downloading = Изтегляне…
settings-update-downloading-hint = Изтегляне на Vmux { $version }…
settings-update-installing = Инсталиране…
settings-update-installing-hint = Инсталиране на Vmux { $version }…
settings-update-ready = Обновяването е готово
settings-update-ready-hint = Vmux { $version } е готов. Рестартирайте, за да се приложи.
settings-update-try-again = Опитай отново
settings-update-failed = Неуспешна проверка за обновявания.
settings-item = Елемент
settings-item-number = Елемент { $number }
settings-press-key = Натиснете клавиш…
settings-saved = Запазено
settings-record-key = Щракнете, за да запишете нова клавишна комбинация

tray-open-window = Отвори прозорец
tray-close-window = Затвори прозорец
tray-pause-recording = Пауза на записването
tray-resume-recording = Продължи записването
tray-finish-recording = Завърши записването
tray-quit = Изход от Vmux

composer-attach-files = Прикачи файлове (/upload)
composer-remove-attachment = Премахни прикачения файл

layout-back = Назад
layout-forward = Напред
layout-reload = Презареди
layout-bookmark-page = Добави страницата в отметки
layout-remove-bookmark = Премахни отметката
layout-pin-page = Закачи тази страница
layout-unpin-page = Откачи тази страница
layout-manage-extensions = Управление на разширенията
layout-new-stack = Нов стек
layout-close-tab = Затвори раздела
layout-bookmark = Отметка
layout-pin = Закачи
layout-new-tab = Нов раздел
layout-team = Екип

command-switch-space = Превключване на пространство…
command-search-ask = Търсене или въпрос…
command-new-tab-placeholder = Търсете, въведете URL или изберете Терминал…
command-placeholder = Въведете URL, търсете в разделите или > за команди…
command-composer-placeholder = Въведете / за команди или @ за медия
command-send = Изпрати (Enter)
command-terminal = Терминал
command-open-terminal = Отвори в Терминал
command-stack = Стек
command-tabs = { $count ->
    [one] 1 раздел
   *[other] { $count } раздела
}
command-prompt = Prompt
command-new-tab = Нов раздел
command-search = Търсене
command-open-value = Отвори „{ $value }“
command-search-value = Търси „{ $value }“

schema-appearance = Облик
schema-general = Общи
schema-layout = Оформление
schema-layout-detail = Прозорец, панели, странична лента и контур на фокуса.
schema-agent = Агент
schema-agent-detail = Поведение на агента и разрешения за инструменти.
schema-shortcuts = Клавишни комбинации
schema-shortcuts-detail = Само за преглед. Редактирайте settings.ron директно, за да промените комбинациите.
schema-terminal = Терминал
schema-browser = Браузър
schema-mode = Режим
schema-mode-detail = Цветова схема за уеб страници. „Устройство“ следва системните настройки.
schema-device = Устройство
schema-light = Светла
schema-dark = Тъмна
schema-language = Език
schema-language-detail = Използвайте системния език, en-US, ja или произволен BCP 47 таг със съответстващ каталог ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Автоматично обновяване
schema-auto-update-detail = Проверява и инсталира обновявания при стартиране и на всеки час.
schema-startup-url = Начален URL
schema-startup-url-detail = Ако е празно, се отваря полето за команди.
schema-search-engine = Търсачка
schema-search-engine-detail = Използва се за уеб търсения от „Старт“ и полето за команди.
schema-window = Прозорец
schema-pane = Панел
schema-side-sheet = Страничен лист
schema-focus-ring = Контур на фокуса
schema-run-placement = Разреши промяна на мястото за изпълнение
schema-run-placement-detail = Позволява на агентите да избират режим, посока и котва на панела за изпълнение.
schema-leader = Водещ клавиш
schema-leader-detail = Префиксен клавиш за chord комбинации.
schema-chord-timeout = Време за chord
schema-chord-timeout-detail = Милисекунди преди префиксът на chord комбинацията да изтече.
schema-bindings = Комбинации
schema-confirm-close = Потвърждение при затваряне
schema-confirm-close-detail = Пита преди затваряне на терминал с работещ процес.
schema-default-theme = Тема по подразбиране
schema-default-theme-detail = Име на активната тема от списъка с теми.

settings-empty = (празно)
settings-none = (няма)

schema-system = Система
schema-editor = Редактор
schema-recording = Записване
schema-radius = Радиус
schema-padding = Отстъп
schema-gap = Разстояние
schema-width = Ширина
schema-color = Цвят
schema-red = Червено
schema-green = Зелено
schema-blue = Синьо
schema-follow-files = Следване на файлове
schema-tidy-files = Подреждане на файлове
schema-tidy-files-max = Праг за подреждане на файлове
schema-tidy-files-auto = Автоматично подреждане на файлове
schema-app-providers = Доставчици на приложения
schema-provider = Доставчик
schema-kind = Вид
schema-models = Модели
schema-acp = ACP агенти
schema-id = ID
schema-name = Име
schema-command = Команда
schema-arguments = Аргументи
schema-environment = Среда
schema-working-directory = Работна директория
schema-shell = Обвивка
schema-font-family = Шрифт
schema-startup-directory = Начална директория
schema-themes = Теми
schema-color-scheme = Цветова схема
schema-font-size = Размер на шрифта
schema-line-height = Междуредие
schema-cursor-style = Стил на курсора
schema-cursor-blink = Мигане на курсора
schema-custom-themes = Персонализирани теми
schema-foreground = Преден план
schema-background = Фон
schema-cursor = Курсор
schema-ansi-colors = ANSI цветове
schema-keymap = Клавишни комбинации
schema-explorer = Навигатор
schema-visible = Видимо
schema-language-servers = Езикови сървъри
schema-servers = Сървъри
schema-language-id = ID на език
schema-root-markers = Маркери за корен
schema-output-directory = Изходна директория

menu-scene = Сцена
menu-layout = Оформление
menu-terminal = Терминал
menu-browser = Браузър
menu-service = Услуга
menu-bookmark = Отметка
menu-edit = Редактиране

layout-knowledge = Знания
layout-open-knowledge = Отвори Знания
layout-open-welcome-knowledge = Отвори „Добре дошли в Знания“
layout-open-path = Отвори { $path }
layout-fold-knowledge = Свий знанията
layout-unfold-knowledge = Разгъни знанията
layout-bookmarks = Отметки
layout-new-folder = Нова папка
layout-add-to-bookmarks = Добави към отметките
layout-move-to-bookmarks = Премести в отметките
layout-stack-number = Стек { $number }
layout-fold-stack = Свий стека
layout-unfold-stack = Разгъни стека
layout-close-stack = Затвори стека
layout-bookmark-in = Отметка в { $folder }

common-cancel = Отказ
common-delete = Изтрий
common-save = Запази
common-rename = Преименувай
common-expand = Разгъни
common-collapse = Свий
common-loading = Зареждане…
common-error = Грешка
common-output = Изход
common-pending = Изчаква
common-current = текущ
common-stop = Спри
services-command = Услуга на Vmux
services-uptime-seconds = { $seconds } сек
services-uptime-minutes = { $minutes } мин { $seconds } сек
services-uptime-hours = { $hours } ч { $minutes } мин
services-uptime-days = { $days } д { $hours } ч

error-page-failed-load = Страницата не се зареди
error-page-not-found = Страницата не е намерена
error-unknown-host = Неизвестен хост на приложение Vmux: { $host }

history-title = История

command-new-app-chat = Нов чат с { $provider }/{ $model } (приложение)
command-interactive-mode-user = Сцена > Интерактивен режим > Потребител
command-interactive-mode-player = Сцена > Интерактивен режим > Изпълнител
command-minimize-window = Оформление > Прозорец > Минимизирай
command-toggle-layout = Оформление > Оформление > Превключи оформлението
command-close-tab = Оформление > Раздел > Затвори раздела
command-new-task = Оформление > Раздел > Нова задача…
command-next-tab = Оформление > Раздел > Следващ раздел
command-prev-tab = Оформление > Раздел > Предишен раздел
command-rename-tab = Оформление > Раздел > Преименувай раздела
command-tab-select-1 = Оформление > Раздел > Избери раздел 1
command-tab-select-2 = Оформление > Раздел > Избери раздел 2
command-tab-select-3 = Оформление > Раздел > Избери раздел 3
command-tab-select-4 = Оформление > Раздел > Избери раздел 4
command-tab-select-5 = Оформление > Раздел > Избери раздел 5
command-tab-select-6 = Оформление > Раздел > Избери раздел 6
command-tab-select-7 = Оформление > Раздел > Избери раздел 7
command-tab-select-8 = Оформление > Раздел > Избери раздел 8
command-tab-select-last = Оформление > Раздел > Избери последния раздел
command-close-pane = Оформление > Панел > Затвори панела
command-select-pane-left = Оформление > Панел > Избери левия панел
command-select-pane-right = Оформление > Панел > Избери десния панел
command-select-pane-up = Оформление > Панел > Избери горния панел
command-select-pane-down = Оформление > Панел > Избери долния панел
command-swap-pane-prev = Оформление > Панел > Размени с предишния панел
command-swap-pane-next = Оформление > Панел > Размени със следващия панел
command-equalize-pane-size = Оформление > Панел > Изравни размера на панелите
command-resize-pane-left = Оформление > Панел > Преоразмери панела наляво
command-resize-pane-right = Оформление > Панел > Преоразмери панела надясно
command-resize-pane-up = Оформление > Панел > Преоразмери панела нагоре
command-resize-pane-down = Оформление > Панел > Преоразмери панела надолу
command-stack-close = Оформление > Стек > Затвори стека
command-stack-next = Оформление > Стек > Следващ стек
command-stack-previous = Оформление > Стек > Предишен стек
command-stack-reopen = Оформление > Стек > Отвори отново затворена страница
command-stack-swap-prev = Оформление > Стек > Премести стека наляво
command-stack-swap-next = Оформление > Стек > Премести стека надясно
command-space-open = Оформление > Пространство > Пространства
command-terminal-close = Терминал > Затвори терминала
command-terminal-next = Терминал > Следващ терминал
command-terminal-prev = Терминал > Предишен терминал
command-terminal-clear = Терминал > Изчисти терминала
command-browser-prev-page = Браузър > Навигация > Назад
command-browser-next-page = Браузър > Навигация > Напред
command-browser-reload = Браузър > Навигация > Презареди
command-browser-hard-reload = Браузър > Навигация > Принудително презареждане
command-open-in-place = Браузър > Отвори > Отвори тук
command-open-in-new-stack = Браузър > Отвори > Отвори в нов стек
command-open-in-pane-top = Браузър > Отвори > Отвори в горен панел
command-open-in-pane-right = Браузър > Отвори > Отвори в десен панел
command-open-in-pane-bottom = Браузър > Отвори > Отвори в долен панел
command-open-in-pane-left = Браузър > Отвори > Отвори в ляв панел
command-open-in-new-tab = Браузър > Отвори > Отвори в нов раздел
command-open-in-new-space = Браузър > Отвори > Отвори в ново пространство
command-browser-zoom-in = Браузър > Изглед > Увеличи
command-browser-zoom-out = Браузър > Изглед > Намали
command-browser-zoom-reset = Браузър > Изглед > Действителен размер
command-browser-dev-tools = Браузър > Изглед > Инструменти за разработчици
command-browser-open-command-bar = Браузър > Лента > Командна лента
command-browser-open-page-in-command-bar = Браузър > Лента > Редактирай страницата
command-browser-open-path-bar = Браузър > Лента > Навигатор по път
command-browser-open-commands = Браузър > Лента > Команди
command-browser-open-history = Браузър > Лента > История
command-service-open = Услуга > Отвори монитора на услуги
command-bookmark-toggle-active = Отметка > Добави страница към отметките
command-bookmark-pin-active = Отметка > Закачи страница

layout-tab = Раздел
layout-no-stacks = Няма стекове
layout-loading = Зареждане…
layout-no-markdown-files = Няма Markdown файлове
layout-empty-folder = Празна папка
layout-worktree = работно дърво
layout-folder-name = Име на папка
layout-no-pins-bookmarks = Няма закачени страници или отметки
layout-move-to = Премести в { $folder }
layout-bookmark-current-page = Добави текущата страница към отметките
layout-rename-folder = Преименувай папката
layout-remove-folder = Премахни папката
layout-update-downloading = Изтегляне на актуализация
layout-update-installing = Инсталиране на актуализация…
layout-update-ready = Налична е нова версия
layout-restart-update = Рестартирай за актуализиране

agent-preparing = Подготовка на агента…
agent-send-all-queued = Изпрати всички чакащи подкани сега (Esc)
agent-send = Изпрати (Enter)
agent-ready = Готов съм, когато сте готови.
agent-loading-older = Зареждане на по-стари съобщения…
agent-load-older = Зареди по-стари съобщения
agent-continued-from = Продължено от { $source }
agent-older-context-omitted = по-старият контекст е пропуснат
agent-interrupted = прекъснато
agent-allow-tool = Да се разреши ли { $tool }?
agent-deny = Откажи
agent-allow-always = Винаги разрешавай
agent-allow = Разреши
agent-loading-sessions = Зареждане на сесии…
agent-no-resumable-sessions = Няма сесии за възобновяване
agent-no-matching-sessions = Няма съвпадащи сесии
agent-no-matching-models = Няма съвпадащи модели
agent-choice-help = ↑/↓ или Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Изберете папка на хранилище
agent-choose-repository-detail = Изберете локалното Git хранилище, което агентът да използва.
agent-choosing = Избиране…
agent-choose-folder = Изберете папка
agent-queued = на опашка
agent-attached = Прикачено:
agent-cancel-queued = Отмени чакащата подкана
agent-resume-queued = Възобнови чакащите подкани
agent-clear-queue = Изчисти опашката
agent-send-all-now = изпрати всички сега
agent-choose-option = Изберете опция по-горе
agent-loading-media = Зареждане на мултимедия…
agent-no-matching-media = Няма съвпадаща мултимедия
agent-prompt-context = Контекст на подканата
agent-details = Подробности
agent-path = Път
agent-tool = Инструмент
agent-server = Сървър
agent-bytes = { $count } байта
agent-worked-for = Работи { $duration }
agent-worked-for-steps = { $count ->
    [one] Работи { $duration } · 1 стъпка
   *[other] Работи { $duration } · { $count } стъпки
}
agent-tool-guardian-review = Преглед от Guardian
agent-tool-read-files = Прочете файлове
agent-tool-viewed-image = Прегледа изображение
agent-tool-used-browser = Използва браузър
agent-tool-searched-files = Търси във файлове
agent-tool-ran-commands = Изпълни команди
agent-thinking = Мисли
agent-subagent = Подагент
agent-prompt = Подкана
agent-thread = Нишка
agent-parent = Родител
agent-children = Деца
agent-call = Повикване
agent-raw-event = Сурово събитие
agent-plan = План
agent-tasks = { $count ->
    [one] 1 задача
   *[other] { $count } задачи
}
agent-edited = Редактирано
agent-reconnecting = Повторно свързване { $attempt }/{ $total }
agent-status-running = Изпълнява се
agent-status-done = Готово
agent-status-failed = Неуспешно
agent-status-pending = Изчаква
agent-slash-attach-files = Прикачи файлове
agent-slash-resume-session = Възобнови предишна сесия
agent-slash-select-model = Избери модел
agent-slash-continue-cli = Продължи тази сесия в CLI
agent-session-just-now = току-що
agent-session-minutes-ago = преди { $count } мин
agent-session-hours-ago = преди { $count } ч
agent-session-days-ago = преди { $count } д
agent-working-working = Работи
agent-working-thinking = Мисли
agent-working-pondering = Обмисля
agent-working-noodling = Разсъждава
agent-working-percolating = Избистря идея
agent-working-conjuring = Измисля
agent-working-cooking = Готви решение
agent-working-brewing = Забърква решение
agent-working-musing = Размишлява
agent-working-ruminating = Предъвква идеи
agent-working-scheming = Крои план
agent-working-synthesizing = Синтезира
agent-working-tinkering = Настройва
agent-working-churning = Обработва
agent-working-vibing = Напипва ритъма
agent-working-simmering = Къкри
agent-working-crafting = Изгражда
agent-working-divining = Търси отговор
agent-working-mulling = Премисля
agent-working-spelunking = Рови надълбоко

editor-toggle-explorer = Покажи/скрий Explorer (Cmd+B)
editor-unsaved = незапазено
editor-rendered-markdown = Визуализиран Markdown с редактиране на живо
editor-note = Бележка
editor-source-editor = Редактор на код
editor-editor = Редактор
editor-git-diff = Git разлики
editor-diff = Разлики
editor-tidy = Подреждане
editor-always = Винаги
editor-unchanged-previews = { $count ->
    [one] ✦ 1 непроменен преглед
   *[other] ✦ { $count } непроменени прегледа
}
editor-open-externally = Отвори външно
editor-changed-line = Променен ред
editor-go-to-definition = Към дефиницията
editor-find-references = Намери препратки
editor-references = { $count ->
    [one] 1 препратка
   *[other] { $count } препратки
}
editor-lsp-starting = { $server } се стартира…
editor-lsp-not-installed = { $server } — не е инсталиран
editor-explorer = Explorer
editor-open-editors = Отворени редактори
editor-outline = Структура
editor-new-file = Нов файл
editor-new-folder = Нова папка
editor-delete-confirm = Да се изтрие ли „{ $name }“? Това не може да се отмени.
editor-created-folder = Създадена папка { $name }
editor-created-file = Създаден файл { $name }
editor-renamed-to = Преименувано на { $name }
editor-deleted = Изтрито { $name }
editor-failed-decode-image = Неуспешно декодиране на изображението
editor-preview-large-image = изображение (твърде голямо за преглед)
editor-preview-binary = двоичен файл
editor-preview-file = файл

git-status-clean = чисто
git-status-modified = променено
git-status-staged = добавено в индекса
git-status-staged-modified = в индекса*
git-status-untracked = непроследено
git-status-deleted = изтрито
git-status-conflict = конфликт
git-accept-all = ✓ приеми всички
git-unstage = Извади от индекса
git-confirm-deny-all = Потвърди отказ на всички
git-deny-all = ✗ откажи всички
git-commit-message = съобщение за commit
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Зареждане на разликите…
git-no-changes = Няма промени за показване
git-accept = ✓ приеми
git-deny = ✗ откажи
git-show-unchanged-lines = Покажи { $count } непроменени реда

terminal-loading = Зареждане…
terminal-runs-when-ready = изпълнява се при готовност · Ctrl+C изчиства · Esc пропуска
terminal-booting = стартиране
terminal-type-command = въведете команда · изпълнява се при готовност · Esc пропуска

setup-tagline-claude = Агентът за кодиране на Anthropic във Vmux
setup-tagline-codex = Агентът за кодиране на OpenAI във Vmux
setup-tagline-vibe = Агентът за кодиране на Mistral във Vmux
setup-install-title = Инсталиране на CLI за { $name }
setup-homebrew-required = Homebrew е необходим за инсталиране на { $command }, но още не е настроен. Vmux първо ще инсталира Homebrew, после { $name }.
setup-terminal-instructions = В терминала натиснете Return за старт, след което въведете паролата си за Mac, когато бъдете подканени.
setup-command-missing = Vmux отвори тази страница, защото локалната команда { $command } още не е инсталирана. Изпълнете командата по-долу, за да я получите.
setup-install-failed = Инсталацията не завърши. Проверете терминала за подробности и опитайте отново.
setup-installing = Инсталиране…
setup-install-homebrew = Инсталирай Homebrew + { $name }
setup-run-install = Изпълни командата за инсталиране
setup-auto-reload = Vmux я изпълнява в терминал и презарежда, когато { $command } е готова.

debug-title = Отстраняване на грешки
debug-auto-update = Автоматично актуализиране
debug-simulate-update = Симулирай налична актуализация
debug-simulate-download = Симулирай изтегляне
debug-clear-update = Изчисти актуализацията
debug-trigger-restart = Задействай рестарт

command-manage-spaces = Управление на пространствата…
command-pane-stack-location = панел { $pane } / стек { $stack }
command-space-pane-stack-location = { $space } / панел { $pane } / стек { $stack }
command-terminal-path = Терминал ({ $path })
command-group-interactive-mode = Интерактивен режим
command-group-window = Прозорец
command-group-tab = Раздел
command-group-pane = Панел
command-group-stack = Стек
command-group-space = Пространство
command-group-navigation = Навигация
command-group-open = Отваряне
command-group-view = Изглед
command-group-bar = Лента

menu-close-vmux = Затваряне на Vmux

agents-terminal-coding-agent = Терминален кодиращ агент
