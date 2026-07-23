locale-name = русский
common-open = Открыть
common-close = Закрыть
common-install = Установить
common-uninstall = Удалить
common-update = Обновить
common-retry = Повторить
common-refresh = Обновить
common-remove = Убрать
common-enable = Включить
common-disable = Отключить
common-new = Создать
common-active = активно
common-running = выполняется
common-done = готово
common-failed = Сбой
common-installed = Установлено
common-items = { $count ->
    [one] { $count } элемент
   *[other] { $count } элементов
}

tools-title = Инструменты
tools-search = Поиск пакетов, агентов, MCP, языковых инструментов и файлов конфигурации…
tools-open = Открыть инструменты
tools-fold = Свернуть инструменты
tools-unfold = Развернуть инструменты
tools-scanning = Сканирование локальных инструментов…
tools-no-installed = Нет установленных инструментов
tools-empty = Подходящих инструментов нет
tools-empty-detail = Установите пакет или добавьте пакет файлов конфигурации в стиле Stow.
tools-apply = Применить
tools-homebrew = Homebrew
tools-homebrew-sync = Установленные формулы и приложения синхронизируются автоматически.
tools-open-brewfile = Открыть Brewfile
tools-managed = управляется
tools-provider-homebrew-formulae = Формулы Homebrew
tools-provider-homebrew-casks = Приложения Homebrew
tools-provider-npm = Пакеты npm
tools-provider-acp-agents = Агенты ACP
tools-provider-language-tools = Языковые инструменты
tools-provider-mcp-servers = Серверы MCP
tools-provider-dotfiles = Файлы конфигурации
tools-status-available = Доступно
tools-status-missing = Отсутствует
tools-status-conflict = Конфликт
tools-forget = Забыть
tools-manage = Управлять
tools-link = Связать
tools-unlink = Отвязать
tools-import = Импортировать
tools-update-count = { $count ->
    [one] 1 обновление
   *[other] { $count } обновлений
}
tools-conflict-count = { $count ->
    [one] 1 конфликт
   *[other] { $count } конфликтов
}
tools-result-applied = Инструменты применены
tools-result-imported = Инструменты импортированы
tools-result-installed = { $name } установлен
tools-result-updated = { $name } обновлён
tools-result-uninstalled = { $name } удалён
tools-result-forgotten = { $name } забыт
tools-result-managed = { $name } теперь управляется
tools-result-linked = { $name } связан
tools-result-unlinked = { $name } отвязан

start-title = Старт
start-tagline = Один промпт — и дело сделано.

agents-title = Агенты
agents-search = Поиск агентов ACP и CLI…
agents-empty = Подходящих агентов нет
agents-empty-detail = Попробуйте имя, среду выполнения или ACP/CLI.
agents-install-failed = Не удалось установить
agents-updating = Обновление…
agents-retrying = Повторная попытка…
agents-preparing = Подготовка…

extensions-title = Расширения
extensions-search = Поиск среди установленных и в Chrome Web Store…
extensions-relaunch = Перезапустите, чтобы применить
extensions-empty = Расширения не установлены
extensions-no-match = Подходящих расширений нет
extensions-empty-detail = Найдите расширение в Chrome Web Store выше и нажмите Return.
extensions-no-match-detail = Попробуйте другое имя или ID расширения.
extensions-on = Вкл.
extensions-off = Выкл.
extensions-enable-confirm = Включить { $name }?
extensions-enable-permissions = Включить { $name } и разрешить:

lsp-title = Языковые серверы
lsp-search = Поиск языковых серверов, линтеров, форматтеров…
lsp-loading = Загрузка каталога…
lsp-empty = Подходящих языковых серверов нет
lsp-empty-detail = Попробуйте другой язык, линтер или форматтер.
lsp-needs = требуется { $tool }
lsp-status-available = Доступно
lsp-status-on-path = В PATH
lsp-status-installing = Установка…
lsp-status-installed = Установлено
lsp-status-outdated = Доступно обновление
lsp-status-running = Запущено
lsp-status-failed = Сбой

spaces-title = Пространства
spaces-new-placeholder = Имя нового пространства
spaces-empty = Пространств нет
spaces-default-name = Пространство { $number }
spaces-tabs = { $count ->
    [one] 1 вкладка
   *[other] { $count } вкладок
}
spaces-delete = Удалить пространство

team-title = Команда
team-just-you = В этом пространстве только вы
team-agents = { $count ->
    [one] Вы и 1 агент
   *[other] Вы и { $count } агентов
}
team-empty = Здесь пока никого нет
team-you = Вы
team-agent = Агент

services-title = Фоновые службы
services-processes = { $count ->
    [one] 1 процесс
   *[other] { $count } процессов
}
services-kill-all = Завершить все принудительно
services-not-running = Служба не запущена
services-start-with = Запускать через:
services-empty = Активных процессов нет
services-filter = Фильтр процессов…
services-no-match = Подходящих процессов нет
services-connected = Подключено
services-disconnected = Отключено
services-attached = подключено
services-kill = Завершить принудительно
services-memory = Память
services-size = Размер
services-shell = Оболочка

error-title = Ошибка

history-search = Поиск в истории
history-clear-all = Очистить всё
history-clear-confirm = Очистить всю историю?
history-clear-warning = Это действие нельзя отменить.
history-cancel = Отмена
history-today = Сегодня
history-yesterday = Вчера
history-days-ago = { $count } дн. назад
history-day-offset = День -{ $count }

settings-title = Настройки
settings-loading = Загрузка настроек…
settings-stored = Хранится в ~/.vmux/settings.ron
settings-other = Другое
settings-software-update = Обновление ПО
settings-check-updates = Проверить обновления
settings-check-updates-hint = Проверка выполняется при запуске и каждый час, если включено автообновление.
settings-update-unavailable = Недоступно
settings-update-unavailable-hint = В этой сборке нет модуля обновления.
settings-update-checking = Проверка…
settings-update-checking-hint = Проверяем наличие обновлений…
settings-update-check-again = Проверить ещё раз
settings-update-current = У вас последняя версия Vmux.
settings-update-downloading = Загрузка…
settings-update-downloading-hint = Загружаем Vmux { $version }…
settings-update-installing = Установка…
settings-update-installing-hint = Устанавливаем Vmux { $version }…
settings-update-ready = Обновление готово
settings-update-ready-hint = Vmux { $version } готов. Перезапустите приложение, чтобы применить обновление.
settings-update-try-again = Попробовать ещё раз
settings-update-failed = Не удалось проверить обновления.
settings-item = Элемент
settings-item-number = Элемент { $number }
settings-press-key = Нажмите клавишу…
settings-saved = Сохранено
settings-record-key = Нажмите, чтобы записать новое сочетание клавиш

tray-open-window = Открыть окно
tray-close-window = Закрыть окно
tray-pause-recording = Приостановить запись
tray-resume-recording = Продолжить запись
tray-finish-recording = Завершить запись
tray-quit = Выйти из Vmux

composer-attach-files = Прикрепить файлы (/upload)
composer-remove-attachment = Удалить вложение

layout-back = Назад
layout-forward = Вперёд
layout-reload = Перезагрузить
layout-bookmark-page = Добавить страницу в закладки
layout-remove-bookmark = Удалить закладку
layout-pin-page = Закрепить страницу
layout-unpin-page = Открепить страницу
layout-manage-extensions = Управление расширениями
layout-new-stack = Новый слой
layout-close-tab = Закрыть вкладку
layout-bookmark = Закладка
layout-pin = Закрепить
layout-new-tab = Новая вкладка
layout-team = Команда

command-switch-space = Перейти в пространство…
command-search-ask = Найти или спросить…
command-new-tab-placeholder = Введите запрос или URL либо выберите Терминал…
command-placeholder = Введите URL, найдите вкладку или > для команд…
command-composer-placeholder = Введите / для команд или @ для медиа
command-send = Отправить (Enter)
command-terminal = Терминал
command-open-terminal = Открыть в Терминале
command-stack = Слой
command-tabs = { $count ->
    [one] 1 вкладка
   *[other] { $count } вкладок
}
command-prompt = Промпт
command-new-tab = Новая вкладка
command-search = Поиск
command-open-value = Открыть «{ $value }»
command-search-value = Найти «{ $value }»

schema-appearance = Оформление
schema-general = Основные
schema-layout = Компоновка
schema-layout-detail = Окно, области, боковая панель и контур фокуса.
schema-agent = Агент
schema-agent-detail = Поведение агента и разрешения для инструментов.
schema-shortcuts = Сочетания клавиш
schema-shortcuts-detail = Только просмотр. Чтобы изменить привязки, отредактируйте settings.ron напрямую.
schema-terminal = Терминал
schema-browser = Браузер
schema-mode = Режим
schema-mode-detail = Цветовая схема для веб-страниц. «Устройство» использует системную тему.
schema-device = Устройство
schema-light = Светлая
schema-dark = Тёмная
schema-language = Язык
schema-language-detail = Используйте системный язык, en-US, ja или любой тег BCP 47 с соответствующим каталогом ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Автообновление
schema-auto-update-detail = Проверять и устанавливать обновления при запуске и каждый час.
schema-startup-url = URL при запуске
schema-startup-url-detail = Если пусто, открывается промпт командной строки.
schema-search-engine = Поисковая система
schema-search-engine-detail = Используется для веб-поиска со Старта и из командной строки.
schema-window = Окно
schema-pane = Область
schema-side-sheet = Боковая панель
schema-focus-ring = Контур фокуса
schema-run-placement = Разрешить переопределять размещение запусков
schema-run-placement-detail = Позволяет агентам выбирать режим области запуска, направление и привязку.
schema-leader = Лидер-клавиша
schema-leader-detail = Префиксная клавиша для аккордных сочетаний.
schema-chord-timeout = Тайм-аут аккорда
schema-chord-timeout-detail = Сколько миллисекунд действует префикс аккорда.
schema-bindings = Привязки
schema-confirm-close = Подтверждать закрытие
schema-confirm-close-detail = Спрашивать перед закрытием терминала с запущенным процессом.
schema-default-theme = Тема по умолчанию
schema-default-theme-detail = Имя активной темы из списка тем.

settings-empty = (пусто)
settings-none = (нет)

schema-system = Система
schema-editor = Редактор
schema-recording = Запись
schema-radius = Радиус
schema-padding = Отступ
schema-gap = Зазор
schema-width = Ширина
schema-color = Цвет
schema-red = Красный
schema-green = Зелёный
schema-blue = Синий
schema-follow-files = Следовать за файлами
schema-tidy-files = Убирать файлы
schema-tidy-files-max = Порог уборки файлов
schema-tidy-files-auto = Убирать файлы автоматически
schema-app-providers = Провайдеры приложений
schema-provider = Провайдер
schema-kind = Тип
schema-models = Модели
schema-acp = Агенты ACP
schema-id = ID
schema-name = Имя
schema-command = Команда
schema-arguments = Аргументы
schema-environment = Окружение
schema-working-directory = Рабочий каталог
schema-shell = Оболочка
schema-font-family = Семейство шрифтов
schema-startup-directory = Начальный каталог
schema-themes = Темы
schema-color-scheme = Цветовая схема
schema-font-size = Размер шрифта
schema-line-height = Высота строки
schema-cursor-style = Стиль курсора
schema-cursor-blink = Мигание курсора
schema-custom-themes = Пользовательские темы
schema-foreground = Передний план
schema-background = Фон
schema-cursor = Курсор
schema-ansi-colors = Цвета ANSI
schema-keymap = Раскладка клавиш
schema-explorer = Проводник
schema-visible = Видимый
schema-language-servers = Языковые серверы
schema-servers = Серверы
schema-language-id = ID языка
schema-root-markers = Маркеры корня
schema-output-directory = Каталог вывода

menu-scene = Сцена
menu-layout = Макет
menu-terminal = Терминал
menu-browser = Браузер
menu-service = Сервис
menu-bookmark = Закладка
menu-edit = Правка

layout-knowledge = Знания
layout-open-knowledge = Открыть Знания
layout-open-welcome-knowledge = Открыть «Добро пожаловать» в Знаниях
layout-open-path = Открыть { $path }
layout-fold-knowledge = Свернуть Знания
layout-unfold-knowledge = Развернуть Знания
layout-bookmarks = Закладки
layout-new-folder = Новая папка
layout-add-to-bookmarks = Добавить в закладки
layout-move-to-bookmarks = Переместить в закладки
layout-stack-number = Стек { $number }
layout-fold-stack = Свернуть стек
layout-unfold-stack = Развернуть стек
layout-close-stack = Закрыть стек
layout-bookmark-in = Закладка в { $folder }

common-cancel = Отмена
common-delete = Удалить
common-save = Сохранить
common-rename = Переименовать
common-expand = Развернуть
common-collapse = Свернуть
common-loading = Загрузка…
common-error = Ошибка
common-output = Вывод
common-pending = Ожидает
common-current = текущий
common-stop = Остановить
services-command = Сервис Vmux
services-uptime-seconds = { $seconds } с
services-uptime-minutes = { $minutes } мин { $seconds } с
services-uptime-hours = { $hours } ч { $minutes } мин
services-uptime-days = { $days } дн { $hours } ч

error-page-failed-load = Не удалось загрузить страницу
error-page-not-found = Страница не найдена
error-unknown-host = Неизвестный хост приложения Vmux: { $host }

history-title = История

command-new-app-chat = Новый чат { $provider }/{ $model } (приложение)
command-interactive-mode-user = Сцена > Интерактивный режим > Пользователь
command-interactive-mode-player = Сцена > Интерактивный режим > Игрок
command-minimize-window = Макет > Окно > Свернуть
command-toggle-layout = Макет > Макет > Переключить макет
command-close-tab = Макет > Вкладка > Закрыть вкладку
command-new-task = Макет > Вкладка > Новая задача…
command-next-tab = Макет > Вкладка > Следующая вкладка
command-prev-tab = Макет > Вкладка > Предыдущая вкладка
command-rename-tab = Макет > Вкладка > Переименовать вкладку
command-tab-select-1 = Макет > Вкладка > Выбрать вкладку 1
command-tab-select-2 = Макет > Вкладка > Выбрать вкладку 2
command-tab-select-3 = Макет > Вкладка > Выбрать вкладку 3
command-tab-select-4 = Макет > Вкладка > Выбрать вкладку 4
command-tab-select-5 = Макет > Вкладка > Выбрать вкладку 5
command-tab-select-6 = Макет > Вкладка > Выбрать вкладку 6
command-tab-select-7 = Макет > Вкладка > Выбрать вкладку 7
command-tab-select-8 = Макет > Вкладка > Выбрать вкладку 8
command-tab-select-last = Макет > Вкладка > Выбрать последнюю вкладку
command-close-pane = Макет > Панель > Закрыть панель
command-select-pane-left = Макет > Панель > Выбрать панель слева
command-select-pane-right = Макет > Панель > Выбрать панель справа
command-select-pane-up = Макет > Панель > Выбрать панель выше
command-select-pane-down = Макет > Панель > Выбрать панель ниже
command-swap-pane-prev = Макет > Панель > Поменять с предыдущей панелью
command-swap-pane-next = Макет > Панель > Поменять со следующей панелью
command-equalize-pane-size = Макет > Панель > Выровнять размер панелей
command-resize-pane-left = Макет > Панель > Изменить размер панели влево
command-resize-pane-right = Макет > Панель > Изменить размер панели вправо
command-resize-pane-up = Макет > Панель > Изменить размер панели вверх
command-resize-pane-down = Макет > Панель > Изменить размер панели вниз
command-stack-close = Макет > Стек > Закрыть стек
command-stack-next = Макет > Стек > Следующий стек
command-stack-previous = Макет > Стек > Предыдущий стек
command-stack-reopen = Макет > Стек > Открыть закрытую страницу
command-stack-swap-prev = Макет > Стек > Переместить стек влево
command-stack-swap-next = Макет > Стек > Переместить стек вправо
command-space-open = Макет > Пространство > Пространства
command-terminal-close = Терминал > Закрыть терминал
command-terminal-next = Терминал > Следующий терминал
command-terminal-prev = Терминал > Предыдущий терминал
command-terminal-clear = Терминал > Очистить терминал
command-browser-prev-page = Браузер > Навигация > Назад
command-browser-next-page = Браузер > Навигация > Вперёд
command-browser-reload = Браузер > Навигация > Перезагрузить
command-browser-hard-reload = Браузер > Навигация > Полная перезагрузка
command-open-in-place = Браузер > Открыть > Открыть здесь
command-open-in-new-stack = Браузер > Открыть > Открыть в новом стеке
command-open-in-pane-top = Браузер > Открыть > Открыть в панели выше
command-open-in-pane-right = Браузер > Открыть > Открыть в панели справа
command-open-in-pane-bottom = Браузер > Открыть > Открыть в панели ниже
command-open-in-pane-left = Браузер > Открыть > Открыть в панели слева
command-open-in-new-tab = Браузер > Открыть > Открыть в новой вкладке
command-open-in-new-space = Браузер > Открыть > Открыть в новом пространстве
command-browser-zoom-in = Браузер > Вид > Увеличить
command-browser-zoom-out = Браузер > Вид > Уменьшить
command-browser-zoom-reset = Браузер > Вид > Реальный размер
command-browser-dev-tools = Браузер > Вид > Инструменты разработчика
command-browser-open-command-bar = Браузер > Панель > Командная строка
command-browser-open-page-in-command-bar = Браузер > Панель > Редактировать страницу
command-browser-open-path-bar = Браузер > Панель > Навигатор пути
command-browser-open-commands = Браузер > Панель > Команды
command-browser-open-history = Браузер > Панель > История
command-service-open = Сервис > Открыть монитор сервисов
command-bookmark-toggle-active = Закладка > Добавить страницу в закладки
command-bookmark-pin-active = Закладка > Закрепить страницу

layout-tab = Вкладка
layout-no-stacks = Нет стеков
layout-loading = Загрузка…
layout-no-markdown-files = Нет файлов Markdown
layout-empty-folder = Пустая папка
layout-worktree = рабочее дерево
layout-folder-name = Имя папки
layout-no-pins-bookmarks = Нет закреплений или закладок
layout-move-to = Переместить в { $folder }
layout-bookmark-current-page = Добавить текущую страницу в закладки
layout-rename-folder = Переименовать папку
layout-remove-folder = Удалить папку
layout-update-downloading = Загрузка обновления
layout-update-installing = Установка обновления…
layout-update-ready = Доступна новая версия
layout-restart-update = Перезапустить для обновления

agent-preparing = Подготовка агента…
agent-send-all-queued = Отправить все запросы из очереди сейчас (Esc)
agent-send = Отправить (Enter)
agent-ready = Готов, когда будете готовы.
agent-loading-older = Загрузка старых сообщений…
agent-load-older = Загрузить старые сообщения
agent-continued-from = Продолжено из { $source }
agent-older-context-omitted = старый контекст пропущен
agent-interrupted = прервано
agent-allow-tool = Разрешить { $tool }?
agent-deny = Запретить
agent-allow-always = Всегда разрешать
agent-allow = Разрешить
agent-loading-sessions = Загрузка сессий…
agent-no-resumable-sessions = Нет сессий для возобновления
agent-no-matching-sessions = Подходящих сессий нет
agent-no-matching-models = Подходящих моделей нет
agent-choice-help = ↑/↓ или Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Выберите папку репозитория
agent-choose-repository-detail = Выберите локальный Git-репозиторий, который будет использовать агент.
agent-choosing = Выбор…
agent-choose-folder = Выберите папку
agent-queued = в очереди
agent-attached = Прикреплено:
agent-cancel-queued = Отменить запрос в очереди
agent-resume-queued = Возобновить запросы в очереди
agent-clear-queue = Очистить очередь
agent-send-all-now = отправить всё сейчас
agent-choose-option = Выберите вариант выше
agent-loading-media = Загрузка медиа…
agent-no-matching-media = Подходящих медиа нет
agent-prompt-context = Контекст запроса
agent-details = Подробности
agent-path = Путь
agent-tool = Инструмент
agent-server = Сервер
agent-bytes = { $count } байт
agent-worked-for = Работал { $duration }
agent-worked-for-steps = { $count ->
    [one] Работал { $duration } · 1 шаг
   *[other] Работал { $duration } · { $count } шагов
}
agent-tool-guardian-review = Проверка Guardian
agent-tool-read-files = Читал файлы
agent-tool-viewed-image = Просматривал изображение
agent-tool-used-browser = Использовал браузер
agent-tool-searched-files = Искал файлы
agent-tool-ran-commands = Выполнял команды
agent-thinking = Думает
agent-subagent = Субагент
agent-prompt = Запрос
agent-thread = Ветка
agent-parent = Родитель
agent-children = Дочерние
agent-call = Вызов
agent-raw-event = Необработанное событие
agent-plan = План
agent-tasks = { $count ->
    [one] 1 задача
   *[other] { $count } задач
}
agent-edited = Изменено
agent-reconnecting = Переподключение { $attempt }/{ $total }
agent-status-running = Выполняется
agent-status-done = Готово
agent-status-failed = Сбой
agent-status-pending = Ожидает
agent-slash-attach-files = Прикрепить файлы
agent-slash-resume-session = Возобновить прошлую сессию
agent-slash-select-model = Выбрать модель
agent-slash-continue-cli = Продолжить эту сессию в CLI
agent-session-just-now = только что
agent-session-minutes-ago = { $count } мин назад
agent-session-hours-ago = { $count } ч назад
agent-session-days-ago = { $count } дн назад
agent-working-working = Работает
agent-working-thinking = Думает
agent-working-pondering = Размышляет
agent-working-noodling = Прикидывает
agent-working-percolating = Созревает
agent-working-conjuring = Колдует
agent-working-cooking = Готовит
agent-working-brewing = Заваривает
agent-working-musing = Обдумывает
agent-working-ruminating = Раздумывает
agent-working-scheming = Планирует
agent-working-synthesizing = Синтезирует
agent-working-tinkering = Возится
agent-working-churning = Обрабатывает
agent-working-vibing = Ловит вайб
agent-working-simmering = Томится
agent-working-crafting = Создаёт
agent-working-divining = Прозревает
agent-working-mulling = Обмозговывает
agent-working-spelunking = Копается

editor-toggle-explorer = Показать/скрыть проводник (Cmd+B)
editor-unsaved = не сохранено
editor-rendered-markdown = Отображённый Markdown с живым редактированием
editor-note = Заметка
editor-source-editor = Редактор исходного кода
editor-editor = Редактор
editor-git-diff = Git-различия
editor-diff = Различия
editor-tidy = Автоочистка
editor-always = Всегда
editor-unchanged-previews = { $count ->
    [one] ✦ 1 неизменённое превью
   *[other] ✦ { $count } неизменённых превью
}
editor-open-externally = Открыть во внешнем приложении
editor-changed-line = Изменённая строка
editor-go-to-definition = Перейти к определению
editor-find-references = Найти ссылки
editor-references = { $count ->
    [one] 1 ссылка
   *[other] { $count } ссылок
}
editor-lsp-starting = { $server } запускается…
editor-lsp-not-installed = { $server } — не установлен
editor-explorer = Проводник
editor-open-editors = Открытые редакторы
editor-outline = Структура
editor-new-file = Новый файл
editor-new-folder = Новая папка
editor-delete-confirm = Удалить «{ $name }»? Это действие нельзя отменить.
editor-created-folder = Создана папка { $name }
editor-created-file = Создан файл { $name }
editor-renamed-to = Переименовано в { $name }
editor-deleted = Удалено: { $name }
editor-failed-decode-image = Не удалось декодировать изображение
editor-preview-large-image = изображение (слишком большое для предпросмотра)
editor-preview-binary = двоичный файл
editor-preview-file = файл

git-status-clean = без изменений
git-status-modified = изменено
git-status-staged = в индексе
git-status-staged-modified = в индексе*
git-status-untracked = не отслеживается
git-status-deleted = удалено
git-status-conflict = конфликт
git-accept-all = ✓ принять всё
git-unstage = Убрать из индекса
git-confirm-deny-all = Подтвердить отклонение всего
git-deny-all = ✗ отклонить всё
git-commit-message = сообщение коммита
git-commit = Коммит ({ $count })
git-push = ↑ Отправить
git-loading-diff = Загрузка различий…
git-no-changes = Нет изменений для просмотра
git-accept = ✓ принять
git-deny = ✗ отклонить
git-show-unchanged-lines = Показать неизменённые строки: { $count }

terminal-loading = Загрузка…
terminal-runs-when-ready = запустится, когда будет готово · Ctrl+C очищает · Esc пропускает
terminal-booting = запуск
terminal-type-command = введите команду · запустится, когда будет готово · Esc пропускает

setup-tagline-claude = Агент для кода от Anthropic в Vmux
setup-tagline-codex = Агент для кода от OpenAI в Vmux
setup-tagline-vibe = Агент для кода от Mistral в Vmux
setup-install-title = Установить CLI { $name }
setup-homebrew-required = Для установки { $command } нужен Homebrew, но он ещё не настроен. Vmux сначала установит Homebrew, затем { $name }.
setup-terminal-instructions = В терминале нажмите Return, чтобы начать, затем введите пароль Mac при запросе.
setup-command-missing = Vmux открыл эту страницу, потому что локальная команда { $command } ещё не установлена. Выполните команду ниже, чтобы установить её.
setup-install-failed = Установка не завершилась. Проверьте подробности в терминале и повторите попытку.
setup-installing = Установка…
setup-install-homebrew = Установить Homebrew + { $name }
setup-run-install = Запустить команду установки
setup-auto-reload = Vmux запустит её в терминале и перезагрузится, когда { $command } будет готова.

debug-title = Отладка
debug-auto-update = Автообновление
debug-simulate-update = Имитировать доступное обновление
debug-simulate-download = Имитировать загрузку
debug-clear-update = Очистить обновление
debug-trigger-restart = Запустить перезапуск

command-manage-spaces = Управление пространствами…
command-pane-stack-location = область { $pane } / стопка { $stack }
command-space-pane-stack-location = { $space } / область { $pane } / стопка { $stack }
command-terminal-path = Терминал ({ $path })
command-group-interactive-mode = Интерактивный режим
command-group-window = Окно
command-group-tab = Вкладка
command-group-pane = Область
command-group-stack = Стопка
command-group-space = Пространство
command-group-navigation = Навигация
command-group-open = Открыть
command-group-view = Вид
command-group-bar = Панель

menu-close-vmux = Закрыть Vmux

agents-terminal-coding-agent = Агент для программирования в терминале
