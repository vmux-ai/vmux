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
