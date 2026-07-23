locale-name = українська
common-open = Відкрити
common-close = Закрити
common-install = Інсталювати
common-uninstall = Видалити
common-update = Оновити
common-retry = Повторити
common-refresh = Оновити
common-remove = Прибрати
common-enable = Увімкнути
common-disable = Вимкнути
common-new = Створити
common-active = активний
common-running = виконується
common-done = готово
common-failed = Помилка
common-installed = Інстальовано
common-items = { $count ->
    [one] { $count } елемент
   *[other] { $count } елементів
}

tools-title = Інструменти
tools-search = Пошук пакетів, агентів, MCP, мовних інструментів і файлів конфігурації…
tools-open = Відкрити інструменти
tools-fold = Згорнути інструменти
tools-unfold = Розгорнути інструменти
tools-scanning = Сканування локальних інструментів…
tools-no-installed = Немає встановлених інструментів
tools-empty = Відповідних інструментів немає
tools-empty-detail = Установіть пакет або додайте пакет файлів конфігурації у стилі Stow.
tools-apply = Застосувати
tools-homebrew = Homebrew
tools-homebrew-sync = Установлені формули й застосунки синхронізуються автоматично.
tools-open-brewfile = Відкрити Brewfile
tools-managed = керується
tools-provider-homebrew-formulae = Формули Homebrew
tools-provider-homebrew-casks = Застосунки Homebrew
tools-provider-npm = Пакети npm
tools-provider-acp-agents = Агенти ACP
tools-provider-language-tools = Мовні інструменти
tools-provider-mcp-servers = Сервери MCP
tools-provider-dotfiles = Файли конфігурації
tools-status-available = Доступно
tools-status-missing = Відсутнє
tools-status-conflict = Конфлікт
tools-forget = Забути
tools-manage = Керувати
tools-link = Зв’язати
tools-unlink = Відв’язати
tools-import = Імпортувати
tools-update-count = { $count ->
    [one] 1 оновлення
   *[other] { $count } оновлень
}
tools-conflict-count = { $count ->
    [one] 1 конфлікт
   *[other] { $count } конфліктів
}
tools-result-applied = Інструменти застосовано
tools-result-imported = Інструменти імпортовано
tools-result-installed = { $name } установлено
tools-result-updated = { $name } оновлено
tools-result-uninstalled = { $name } видалено
tools-result-forgotten = { $name } забуто
tools-result-managed = { $name } тепер керується
tools-result-linked = { $name } зв’язано
tools-result-unlinked = { $name } відв’язано

start-title = Початок
start-tagline = Один prompt — і все готово.

agents-title = Агенти
agents-search = Пошук агентів ACP і CLI…
agents-empty = Відповідних агентів немає
agents-empty-detail = Спробуйте назву, середовище виконання або ACP/CLI.
agents-install-failed = Не вдалося інсталювати
agents-updating = Оновлення…
agents-retrying = Повторна спроба…
agents-preparing = Підготовка…

extensions-title = Розширення
extensions-search = Пошук серед інстальованих або в Chrome Web Store…
extensions-relaunch = Перезапустіть, щоб застосувати
extensions-empty = Розширення не інстальовано
extensions-no-match = Відповідних розширень немає
extensions-empty-detail = Знайдіть розширення в Chrome Web Store вище й натисніть Enter.
extensions-no-match-detail = Спробуйте іншу назву або ID розширення.
extensions-on = Увімкнено
extensions-off = Вимкнено
extensions-enable-confirm = Увімкнути { $name }?
extensions-enable-permissions = Увімкнути { $name } і дозволити:

lsp-title = Мовні сервери
lsp-search = Пошук мовних серверів, лінтерів, форматерів…
lsp-loading = Завантаження каталогу…
lsp-empty = Відповідних мовних серверів немає
lsp-empty-detail = Спробуйте іншу мову, лінтер або форматер.
lsp-needs = потрібен { $tool }
lsp-status-available = Доступно
lsp-status-on-path = У PATH
lsp-status-installing = Інсталяція…
lsp-status-installed = Інстальовано
lsp-status-outdated = Доступне оновлення
lsp-status-running = Запущено
lsp-status-failed = Помилка

spaces-title = Простори
spaces-new-placeholder = Назва нового простору
spaces-empty = Просторів немає
spaces-default-name = Простір { $number }
spaces-tabs = { $count ->
    [one] 1 вкладка
   *[other] { $count } вкладок
}
spaces-delete = Видалити простір

team-title = Команда
team-just-you = У цьому просторі лише ви
team-agents = { $count ->
    [one] Ви й 1 агент
   *[other] Ви й { $count } агентів
}
team-empty = Тут поки нікого немає
team-you = Ви
team-agent = Агент

services-title = Фонові служби
services-processes = { $count ->
    [one] 1 процес
   *[other] { $count } процесів
}
services-kill-all = Примусово завершити всі
services-not-running = Службу не запущено
services-start-with = Запустити з:
services-empty = Активних процесів немає
services-filter = Фільтр процесів…
services-no-match = Відповідних процесів немає
services-connected = Під’єднано
services-disconnected = Від’єднано
services-attached = приєднано
services-kill = Примусово завершити
services-memory = Пам’ять
services-size = Розмір
services-shell = Оболонка

error-title = Помилка

history-search = Пошук в історії
history-clear-all = Очистити все
history-clear-confirm = Очистити всю історію?
history-clear-warning = Цю дію не можна скасувати.
history-cancel = Скасувати
history-today = Сьогодні
history-yesterday = Учора
history-days-ago = { $count } дн. тому
history-day-offset = День -{ $count }

settings-title = Налаштування
settings-loading = Завантаження налаштувань…
settings-stored = Зберігається в ~/.vmux/settings.ron
settings-other = Інше
settings-software-update = Оновлення ПЗ
settings-check-updates = Перевірити оновлення
settings-check-updates-hint = Перевіряється автоматично під час запуску та щогодини, якщо ввімкнено автооновлення.
settings-update-unavailable = Недоступно
settings-update-unavailable-hint = У цій збірці немає модуля оновлення.
settings-update-checking = Перевірка…
settings-update-checking-hint = Перевірка наявності оновлень…
settings-update-check-again = Перевірити ще раз
settings-update-current = Vmux оновлено до актуальної версії.
settings-update-downloading = Завантаження…
settings-update-downloading-hint = Завантаження Vmux { $version }…
settings-update-installing = Інсталяція…
settings-update-installing-hint = Інсталяція Vmux { $version }…
settings-update-ready = Оновлення готове
settings-update-ready-hint = Vmux { $version } готовий. Перезапустіть, щоб застосувати оновлення.
settings-update-try-again = Спробувати ще раз
settings-update-failed = Не вдалося перевірити оновлення.
settings-item = Елемент
settings-item-number = Елемент { $number }
settings-press-key = Натисніть клавішу…
settings-saved = Збережено
settings-record-key = Натисніть, щоб записати нову комбінацію клавіш

tray-open-window = Відкрити вікно
tray-close-window = Закрити вікно
tray-pause-recording = Призупинити запис
tray-resume-recording = Відновити запис
tray-finish-recording = Завершити запис
tray-quit = Вийти з Vmux

composer-attach-files = Прикріпити файли (/upload)
composer-remove-attachment = Прибрати вкладення

layout-back = Назад
layout-forward = Уперед
layout-reload = Перезавантажити
layout-bookmark-page = Додати сторінку в закладки
layout-remove-bookmark = Видалити закладку
layout-pin-page = Закріпити сторінку
layout-unpin-page = Відкріпити сторінку
layout-manage-extensions = Керувати розширеннями
layout-new-stack = Новий стек
layout-close-tab = Закрити вкладку
layout-bookmark = Закладка
layout-pin = Закріпити
layout-new-tab = Нова вкладка
layout-team = Команда

command-switch-space = Перемкнути простір…
command-search-ask = Пошук або запит…
command-new-tab-placeholder = Шукайте, введіть URL або виберіть термінал…
command-placeholder = Введіть URL, шукайте вкладки або > для команд…
command-composer-placeholder = Введіть / для команд або @ для медіа
command-send = Надіслати (Enter)
command-terminal = Термінал
command-open-terminal = Відкрити в терміналі
command-stack = Стек
command-tabs = { $count ->
    [one] 1 вкладка
   *[other] { $count } вкладок
}
command-prompt = Prompt
command-new-tab = Нова вкладка
command-search = Пошук
command-open-value = Відкрити «{ $value }»
command-search-value = Шукати «{ $value }»

schema-appearance = Вигляд
schema-general = Загальні
schema-layout = Макет
schema-layout-detail = Вікно, області, бічна панель і рамка фокуса.
schema-agent = Агент
schema-agent-detail = Поведінка агента й дозволи для інструментів.
schema-shortcuts = Скорочення
schema-shortcuts-detail = Лише для перегляду. Щоб змінити прив’язки, редагуйте settings.ron напряму.
schema-terminal = Термінал
schema-browser = Браузер
schema-mode = Режим
schema-mode-detail = Колірна схема вебсторінок. «Пристрій» використовує системну тему.
schema-device = Пристрій
schema-light = Світла
schema-dark = Темна
schema-language = Мова
schema-language-detail = Використовуйте системну мову, en-US, ja або будь-який тег BCP 47 з відповідним каталогом ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Автооновлення
schema-auto-update-detail = Перевіряти й інсталювати оновлення під час запуску та щогодини.
schema-startup-url = URL запуску
schema-startup-url-detail = Якщо порожньо, відкривається prompt командного рядка.
schema-search-engine = Пошукова система
schema-search-engine-detail = Використовується для вебпошуку з екрана «Початок» і командного рядка.
schema-window = Вікно
schema-pane = Область
schema-side-sheet = Бічна панель
schema-focus-ring = Рамка фокуса
schema-run-placement = Дозволити перевизначення розміщення запуску
schema-run-placement-detail = Дозволити агентам вибирати режим області запуску, напрямок і прив’язку.
schema-leader = Leader
schema-leader-detail = Префіксна клавіша для chord-скорочень.
schema-chord-timeout = Тайм-аут chord
schema-chord-timeout-detail = Мілісекунди до завершення дії chord-префікса.
schema-bindings = Прив’язки
schema-confirm-close = Підтверджувати закриття
schema-confirm-close-detail = Запитувати підтвердження перед закриттям термінала із запущеним процесом.
schema-default-theme = Типова тема
schema-default-theme-detail = Назва активної теми зі списку тем.

settings-empty = (порожньо)
settings-none = (немає)

schema-system = Система
schema-editor = Редактор
schema-recording = Запис
schema-radius = Радіус
schema-padding = Відступ
schema-gap = Проміжок
schema-width = Ширина
schema-color = Колір
schema-red = Червоний
schema-green = Зелений
schema-blue = Синій
schema-follow-files = Стежити за файлами
schema-tidy-files = Упорядкування файлів
schema-tidy-files-max = Поріг упорядкування файлів
schema-tidy-files-auto = Автоматично впорядковувати файли
schema-app-providers = Постачальники застосунків
schema-provider = Постачальник
schema-kind = Тип
schema-models = Моделі
schema-acp = Агенти ACP
schema-id = ID
schema-name = Назва
schema-command = Команда
schema-arguments = Аргументи
schema-environment = Середовище
schema-working-directory = Робочий каталог
schema-shell = Оболонка
schema-font-family = Сімейство шрифтів
schema-startup-directory = Початковий каталог
schema-themes = Теми
schema-color-scheme = Колірна схема
schema-font-size = Розмір шрифту
schema-line-height = Висота рядка
schema-cursor-style = Стиль курсора
schema-cursor-blink = Блимання курсора
schema-custom-themes = Власні теми
schema-foreground = Передній план
schema-background = Тло
schema-cursor = Курсор
schema-ansi-colors = Кольори ANSI
schema-keymap = Розкладка клавіш
schema-explorer = Провідник
schema-visible = Видимий
schema-language-servers = Мовні сервери
schema-servers = Сервери
schema-language-id = ID мови
schema-root-markers = Маркери кореня
schema-output-directory = Каталог виводу

menu-scene = Сцена
menu-layout = Макет
menu-terminal = Термінал
menu-browser = Браузер
menu-service = Сервіс
menu-bookmark = Закладка
menu-edit = Зміни

layout-knowledge = Знання
layout-open-knowledge = Відкрити Знання
layout-open-welcome-knowledge = Відкрити привітання у Знаннях
layout-open-path = Відкрити { $path }
layout-fold-knowledge = Згорнути знання
layout-unfold-knowledge = Розгорнути знання
layout-bookmarks = Закладки
layout-new-folder = Нова папка
layout-add-to-bookmarks = Додати до закладок
layout-move-to-bookmarks = Перемістити до закладок
layout-stack-number = Стек { $number }
layout-fold-stack = Згорнути стек
layout-unfold-stack = Розгорнути стек
layout-close-stack = Закрити стек
layout-bookmark-in = Додати закладку в { $folder }

common-cancel = Скасувати
common-delete = Видалити
common-save = Зберегти
common-rename = Перейменувати
common-expand = Розгорнути
common-collapse = Згорнути
common-loading = Завантаження…
common-error = Помилка
common-output = Вивід
common-pending = Очікує
common-current = поточний
common-stop = Зупинити
services-command = Сервіс Vmux
services-uptime-seconds = { $seconds } с
services-uptime-minutes = { $minutes } хв { $seconds } с
services-uptime-hours = { $hours } год { $minutes } хв
services-uptime-days = { $days } дн { $hours } год

error-page-failed-load = Не вдалося завантажити сторінку
error-page-not-found = Сторінку не знайдено
error-unknown-host = Невідомий хост застосунку Vmux: { $host }

history-title = Історія

command-new-app-chat = Новий чат { $provider }/{ $model } (Застосунок)
command-interactive-mode-user = Сцена > Інтерактивний режим > Користувач
command-interactive-mode-player = Сцена > Інтерактивний режим > Гравець
command-minimize-window = Макет > Вікно > Згорнути
command-toggle-layout = Макет > Макет > Перемкнути макет
command-close-tab = Макет > Вкладка > Закрити вкладку
command-new-task = Макет > Вкладка > Нове завдання…
command-next-tab = Макет > Вкладка > Наступна вкладка
command-prev-tab = Макет > Вкладка > Попередня вкладка
command-rename-tab = Макет > Вкладка > Перейменувати вкладку
command-tab-select-1 = Макет > Вкладка > Вибрати вкладку 1
command-tab-select-2 = Макет > Вкладка > Вибрати вкладку 2
command-tab-select-3 = Макет > Вкладка > Вибрати вкладку 3
command-tab-select-4 = Макет > Вкладка > Вибрати вкладку 4
command-tab-select-5 = Макет > Вкладка > Вибрати вкладку 5
command-tab-select-6 = Макет > Вкладка > Вибрати вкладку 6
command-tab-select-7 = Макет > Вкладка > Вибрати вкладку 7
command-tab-select-8 = Макет > Вкладка > Вибрати вкладку 8
command-tab-select-last = Макет > Вкладка > Вибрати останню вкладку
command-close-pane = Макет > Панель > Закрити панель
command-select-pane-left = Макет > Панель > Вибрати панель ліворуч
command-select-pane-right = Макет > Панель > Вибрати панель праворуч
command-select-pane-up = Макет > Панель > Вибрати панель угорі
command-select-pane-down = Макет > Панель > Вибрати панель унизу
command-swap-pane-prev = Макет > Панель > Поміняти з попередньою панеллю
command-swap-pane-next = Макет > Панель > Поміняти з наступною панеллю
command-equalize-pane-size = Макет > Панель > Вирівняти розмір панелей
command-resize-pane-left = Макет > Панель > Змінити розмір панелі ліворуч
command-resize-pane-right = Макет > Панель > Змінити розмір панелі праворуч
command-resize-pane-up = Макет > Панель > Змінити розмір панелі вгору
command-resize-pane-down = Макет > Панель > Змінити розмір панелі вниз
command-stack-close = Макет > Стек > Закрити стек
command-stack-next = Макет > Стек > Наступний стек
command-stack-previous = Макет > Стек > Попередній стек
command-stack-reopen = Макет > Стек > Відкрити закриту сторінку
command-stack-swap-prev = Макет > Стек > Перемістити стек ліворуч
command-stack-swap-next = Макет > Стек > Перемістити стек праворуч
command-space-open = Макет > Простір > Простори
command-terminal-close = Термінал > Закрити термінал
command-terminal-next = Термінал > Наступний термінал
command-terminal-prev = Термінал > Попередній термінал
command-terminal-clear = Термінал > Очистити термінал
command-browser-prev-page = Браузер > Навігація > Назад
command-browser-next-page = Браузер > Навігація > Вперед
command-browser-reload = Браузер > Навігація > Перезавантажити
command-browser-hard-reload = Браузер > Навігація > Повне перезавантаження
command-open-in-place = Браузер > Відкрити > Відкрити тут
command-open-in-new-stack = Браузер > Відкрити > Відкрити в новому стеку
command-open-in-pane-top = Браузер > Відкрити > Відкрити в панелі вище
command-open-in-pane-right = Браузер > Відкрити > Відкрити в панелі праворуч
command-open-in-pane-bottom = Браузер > Відкрити > Відкрити в панелі нижче
command-open-in-pane-left = Браузер > Відкрити > Відкрити в панелі ліворуч
command-open-in-new-tab = Браузер > Відкрити > Відкрити в новій вкладці
command-open-in-new-space = Браузер > Відкрити > Відкрити в новому просторі
command-browser-zoom-in = Браузер > Перегляд > Збільшити
command-browser-zoom-out = Браузер > Перегляд > Зменшити
command-browser-zoom-reset = Браузер > Перегляд > Фактичний розмір
command-browser-dev-tools = Браузер > Перегляд > Інструменти розробника
command-browser-open-command-bar = Браузер > Панель > Командний рядок
command-browser-open-page-in-command-bar = Браузер > Панель > Редагувати сторінку
command-browser-open-path-bar = Браузер > Панель > Навігатор шляху
command-browser-open-commands = Браузер > Панель > Команди
command-browser-open-history = Браузер > Панель > Історія
command-service-open = Сервіс > Відкрити монітор сервісів
command-bookmark-toggle-active = Закладка > Додати сторінку в закладки
command-bookmark-pin-active = Закладка > Закріпити сторінку

layout-tab = Вкладка
layout-no-stacks = Немає стеків
layout-loading = Завантаження…
layout-no-markdown-files = Немає файлів Markdown
layout-empty-folder = Порожня папка
layout-worktree = робоче дерево
layout-folder-name = Назва папки
layout-no-pins-bookmarks = Немає закріплень або закладок
layout-move-to = Перемістити до { $folder }
layout-bookmark-current-page = Додати поточну сторінку в закладки
layout-rename-folder = Перейменувати папку
layout-remove-folder = Видалити папку
layout-update-downloading = Завантаження оновлення
layout-update-installing = Встановлення оновлення…
layout-update-ready = Доступна нова версія
layout-restart-update = Перезапустити для оновлення

agent-preparing = Підготовка агента…
agent-send-all-queued = Надіслати всі запити з черги зараз (Esc)
agent-send = Надіслати (Enter)
agent-ready = Готовий, коли будете готові.
agent-loading-older = Завантаження старіших повідомлень…
agent-load-older = Завантажити старіші повідомлення
agent-continued-from = Продовжено з { $source }
agent-older-context-omitted = старіший контекст пропущено
agent-interrupted = перервано
agent-allow-tool = Дозволити { $tool }?
agent-deny = Відхилити
agent-allow-always = Завжди дозволяти
agent-allow = Дозволити
agent-loading-sessions = Завантаження сеансів…
agent-no-resumable-sessions = Немає сеансів для відновлення
agent-no-matching-sessions = Немає відповідних сеансів
agent-no-matching-models = Немає відповідних моделей
agent-choice-help = ↑/↓ або Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Виберіть папку репозиторію
agent-choose-repository-detail = Виберіть локальний Git-репозиторій, який має використовувати агент.
agent-choosing = Вибір…
agent-choose-folder = Вибрати папку
agent-queued = у черзі
agent-attached = Додано:
agent-cancel-queued = Скасувати запит у черзі
agent-resume-queued = Відновити запити в черзі
agent-clear-queue = Очистити чергу
agent-send-all-now = надіслати всі зараз
agent-choose-option = Виберіть варіант вище
agent-loading-media = Завантаження медіа…
agent-no-matching-media = Немає відповідних медіа
agent-prompt-context = Контекст запиту
agent-details = Докладно
agent-path = Шлях
agent-tool = Інструмент
agent-server = Сервер
agent-bytes = { $count } байт
agent-worked-for = Працював { $duration }
agent-worked-for-steps = { $count ->
    [one] Працював { $duration } · 1 крок
   *[other] Працював { $duration } · { $count } кроків
}
agent-tool-guardian-review = Перевірка Guardian
agent-tool-read-files = Читав файли
agent-tool-viewed-image = Переглянув зображення
agent-tool-used-browser = Використав браузер
agent-tool-searched-files = Шукав у файлах
agent-tool-ran-commands = Виконав команди
agent-thinking = Думає
agent-subagent = Субагент
agent-prompt = Запит
agent-thread = Гілка
agent-parent = Батьківський
agent-children = Дочірні
agent-call = Виклик
agent-raw-event = Необроблена подія
agent-plan = План
agent-tasks = { $count ->
    [one] 1 завдання
   *[other] { $count } завдань
}
agent-edited = Змінено
agent-reconnecting = Повторне підключення { $attempt }/{ $total }
agent-status-running = Виконується
agent-status-done = Готово
agent-status-failed = Не вдалося
agent-status-pending = Очікує
agent-slash-attach-files = Додати файли
agent-slash-resume-session = Відновити попередній сеанс
agent-slash-select-model = Вибрати модель
agent-slash-continue-cli = Продовжити цей сеанс у CLI
agent-session-just-now = щойно
agent-session-minutes-ago = { $count } хв тому
agent-session-hours-ago = { $count } год тому
agent-session-days-ago = { $count } дн тому
agent-working-working = Працює
agent-working-thinking = Думає
agent-working-pondering = Розмірковує
agent-working-noodling = Обмірковує
agent-working-percolating = Дозріває
agent-working-conjuring = Чаклує
agent-working-cooking = Готує
agent-working-brewing = Заварює
agent-working-musing = Міркує
agent-working-ruminating = Роздумує
agent-working-scheming = Планує
agent-working-synthesizing = Синтезує
agent-working-tinkering = Майструє
agent-working-churning = Опрацьовує
agent-working-vibing = Ловить вайб
agent-working-simmering = Томиться
agent-working-crafting = Виготовляє
agent-working-divining = Ворожить
agent-working-mulling = Зважує
agent-working-spelunking = Досліджує глибини

editor-toggle-explorer = Показати/сховати Провідник (Cmd+B)
editor-unsaved = не збережено
editor-rendered-markdown = Відтворений Markdown із живим редагуванням
editor-note = Нотатка
editor-source-editor = Редактор коду
editor-editor = Редактор
editor-git-diff = Git-різниця
editor-diff = Різниця
editor-tidy = Прибирання
editor-always = Завжди
editor-unchanged-previews = { $count ->
    [one] ✦ 1 незмінений перегляд
   *[other] ✦ { $count } незмінених переглядів
}
editor-open-externally = Відкрити зовні
editor-changed-line = Змінений рядок
editor-go-to-definition = Перейти до визначення
editor-find-references = Знайти посилання
editor-references = { $count ->
    [one] 1 посилання
   *[other] { $count } посилань
}
editor-lsp-starting = { $server } запускається…
editor-lsp-not-installed = { $server } — не встановлено
editor-explorer = Провідник
editor-open-editors = Відкриті редактори
editor-outline = Структура
editor-new-file = Новий файл
editor-new-folder = Нова папка
editor-delete-confirm = Видалити «{ $name }»? Цю дію не можна скасувати.
editor-created-folder = Створено папку { $name }
editor-created-file = Створено файл { $name }
editor-renamed-to = Перейменовано на { $name }
editor-deleted = Видалено { $name }
editor-failed-decode-image = Не вдалося декодувати зображення
editor-preview-large-image = зображення (завелике для перегляду)
editor-preview-binary = двійковий файл
editor-preview-file = файл

git-status-clean = чисто
git-status-modified = змінено
git-status-staged = індексовано
git-status-staged-modified = індексовано*
git-status-untracked = не відстежується
git-status-deleted = видалено
git-status-conflict = конфлікт
git-accept-all = ✓ прийняти всі
git-unstage = Прибрати з індексу
git-confirm-deny-all = Підтвердити відхилення всіх
git-deny-all = ✗ відхилити всі
git-commit-message = повідомлення коміту
git-commit = Коміт ({ $count })
git-push = ↑ Надіслати
git-loading-diff = Завантаження різниці…
git-no-changes = Немає змін для показу
git-accept = ✓ прийняти
git-deny = ✗ відхилити
git-show-unchanged-lines = Показати { $count } незмінених рядків

terminal-loading = Завантаження…
terminal-runs-when-ready = виконається, коли буде готово · Ctrl+C очищає · Esc пропускає
terminal-booting = запуск
terminal-type-command = введіть команду · виконається, коли буде готово · Esc пропускає

setup-tagline-claude = Агент кодування Anthropic у Vmux
setup-tagline-codex = Агент кодування OpenAI у Vmux
setup-tagline-vibe = Агент кодування Mistral у Vmux
setup-install-title = Встановити CLI { $name }
setup-homebrew-required = Для встановлення { $command } потрібен Homebrew, але його ще не налаштовано. Vmux спершу встановить Homebrew, а потім { $name }.
setup-terminal-instructions = У терміналі натисніть Return, щоб почати, а потім введіть пароль Mac, коли буде запит.
setup-command-missing = Vmux відкрив цю сторінку, бо локальну команду { $command } ще не встановлено. Виконайте команду нижче, щоб її отримати.
setup-install-failed = Встановлення не завершилося. Перевірте подробиці в терміналі й спробуйте ще раз.
setup-installing = Встановлення…
setup-install-homebrew = Встановити Homebrew + { $name }
setup-run-install = Виконати команду встановлення
setup-auto-reload = Vmux виконає її в терміналі й перезавантажиться, коли { $command } буде готова.

debug-title = Налагодження
debug-auto-update = Автооновлення
debug-simulate-update = Імітувати доступне оновлення
debug-simulate-download = Імітувати завантаження
debug-clear-update = Очистити оновлення
debug-trigger-restart = Запустити перезапуск

command-manage-spaces = Керувати просторами…
command-pane-stack-location = панель { $pane } / стек { $stack }
command-space-pane-stack-location = { $space } / панель { $pane } / стек { $stack }
command-terminal-path = Термінал ({ $path })
command-group-interactive-mode = Інтерактивний режим
command-group-window = Вікно
command-group-tab = Вкладка
command-group-pane = Панель
command-group-stack = Стек
command-group-space = Простір
command-group-navigation = Навігація
command-group-open = Відкрити
command-group-view = Перегляд
command-group-bar = Панель

menu-close-vmux = Закрити Vmux

agents-terminal-coding-agent = Агент кодування на базі термінала
