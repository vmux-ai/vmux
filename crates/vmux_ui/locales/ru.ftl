common-open = Открыть
common-close = Закрыть
common-install = Установить
common-uninstall = Удалить
common-update = Обновить
common-retry = Повторить
common-refresh = Обновить
common-remove = Удалить
common-enable = Включить
common-disable = Отключить
common-new = Новый
common-active = активен
common-running = выполняется
common-done = готово
common-failed = Ошибка
common-installed = Установлено
common-items = { $count ->
    [one] { $count } элемент
   *[other] { $count } элементов
}
start-title = Начало
start-tagline = Один запрос. Что угодно — готово.

agents-title = Агенты
agents-search = Поиск агентов ACP и CLI…
agents-empty = Нет подходящих агентов
agents-empty-detail = Попробуйте имя, среду выполнения или ACP/CLI.
agents-install-failed = Ошибка установки
agents-updating = Обновление…
agents-retrying = Повтор…
agents-preparing = Подготовка…

extensions-title = Расширения
extensions-search = Поиск среди установленных или в Chrome Web Store…
extensions-relaunch = Перезапустить для применения
extensions-empty = Расширения не установлены
extensions-no-match = Нет подходящих расширений
extensions-empty-detail = Найдите расширение в Chrome Web Store выше и нажмите Return.
extensions-no-match-detail = Попробуйте другое название или ID расширения.
extensions-on = Вкл
extensions-off = Выкл
extensions-enable-confirm = Включить { $name }?
extensions-enable-permissions = Включить { $name } и разрешить:

lsp-title = Языковые серверы
lsp-search = Поиск языковых серверов, линтеров, форматтеров…
lsp-loading = Загрузка каталога…
lsp-empty = Нет подходящих языковых серверов
lsp-empty-detail = Попробуйте другой язык, линтер или форматтер.
lsp-needs = требуется { $tool }
lsp-status-available = Доступен
lsp-status-on-path = В PATH
lsp-status-installing = Установка…
lsp-status-installed = Установлен
lsp-status-outdated = Доступно обновление
lsp-status-running = Выполняется
lsp-status-failed = Ошибка

spaces-title = Пространства
spaces-new-placeholder = Название нового пространства
spaces-empty = Нет пространств
spaces-default-name = Пространство { $number }
spaces-tabs = { $count ->
    [one] 1 вкладка
   *[other] { $count } вкладок
}
spaces-delete = Удалить пространство

team-title = Команда
team-just-you = Только вы в этом пространстве
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
services-kill-all = Завершить все
services-not-running = Служба не запущена
services-start-with = Запустить с:
services-empty = Нет активных процессов
services-filter = Фильтр процессов…
services-no-match = Нет подходящих процессов
services-connected = Подключено
services-disconnected = Отключено
services-attached = подключён
services-kill = Завершить
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
history-days-ago = { $count } дней назад
history-day-offset = День -{ $count }

settings-title = Настройки
settings-loading = Загрузка настроек…
settings-stored = Хранится в ~/.vmux/settings.ron
settings-other = Прочее
settings-software-update = Обновление программы
settings-check-updates = Проверить обновления
settings-check-updates-hint = Проверяется автоматически при запуске и каждый час, если включено автообновление.
settings-update-unavailable = Недоступно
settings-update-unavailable-hint = Средство обновления не включено в эту сборку.
settings-update-checking = Проверка…
settings-update-checking-hint = Проверка обновлений…
settings-update-check-again = Проверить снова
settings-update-current = Vmux актуален.
settings-update-downloading = Загрузка…
settings-update-downloading-hint = Загрузка Vmux { $version }…
settings-update-installing = Установка…
settings-update-installing-hint = Установка Vmux { $version }…
settings-update-ready = Обновление готово
settings-update-ready-hint = Vmux { $version } готов. Перезапустите для применения.
settings-update-try-again = Повторить попытку
settings-update-failed = Не удалось проверить обновления.
settings-item = Элемент
settings-item-number = Элемент { $number }
settings-press-key = Нажмите клавишу…
settings-saved = Сохранено
settings-record-key = Нажмите для записи новой комбинации клавиш

tray-open-window = Открыть окно
tray-close-window = Закрыть окно
tray-pause-recording = Приостановить запись
tray-resume-recording = Возобновить запись
tray-finish-recording = Завершить запись
tray-quit = Выйти из Vmux

composer-attach-files = Прикрепить файлы (/upload)
composer-remove-attachment = Удалить вложение

layout-back = Назад
layout-forward = Вперёд
layout-reload = Перезагрузить
layout-bookmark-page = Добавить в закладки
layout-remove-bookmark = Удалить закладку
layout-pin-page = Закрепить страницу
layout-unpin-page = Открепить страницу
layout-manage-extensions = Управление расширениями
layout-new-stack = Новый стек
layout-close-tab = Закрыть вкладку
layout-bookmark = Закладка
layout-pin = Закрепить
layout-new-tab = Новая вкладка
layout-team = Команда

command-switch-space = Переключить пространство…
command-search-ask = Найти или спросить…
command-new-tab-placeholder = Поиск или введите URL, или выберите Терминал…
command-placeholder = Введите URL, ищите вкладки или > для команд…
command-composer-placeholder = Введите / для команд или @ для медиа
command-send = Отправить (Enter)
command-terminal = Терминал
command-open-terminal = Открыть в терминале
command-stack = Стек
command-tabs = { $count ->
    [one] 1 вкладка
   *[other] { $count } вкладок
}
command-prompt = Запрос
command-new-tab = Новая вкладка
command-search = Поиск
command-open-value = Открыть «{ $value }»
command-search-value = Найти «{ $value }»

schema-appearance = Внешний вид
schema-general = Общие
schema-layout = Расположение
schema-layout-detail = Окно, панели, боковая панель и кольцо фокуса.
schema-agent = Агент
schema-agent-detail = Поведение агента и разрешения инструментов.
schema-shortcuts = Сочетания клавиш
schema-shortcuts-detail = Только для просмотра. Для изменения привязок редактируйте settings.ron напрямую.
schema-terminal = Терминал
schema-browser = Браузер
schema-mode = Режим
schema-mode-detail = Цветовая схема для веб-страниц. Устройство следует настройкам системы.
schema-device = Устройство
schema-light = Светлая
schema-dark = Тёмная
schema-language = Язык
schema-language-detail = Используйте system, en-US, ja или любой тег BCP 47 с соответствующим каталогом ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Автообновление
schema-auto-update-detail = Проверять и устанавливать обновления при запуске и каждый час.
schema-startup-url = URL при запуске
schema-startup-url-detail = Пустое значение открывает строку ввода команд.
schema-search-engine = Поисковая система
schema-search-engine-detail = Используется для поиска в интернете из «Начало» и командной строки.
schema-window = Окно
schema-pane = Панель
schema-side-sheet = Боковой лист
schema-focus-ring = Кольцо фокуса
schema-run-placement = Разрешить переопределение размещения запуска
schema-run-placement-detail = Позволить агентам выбирать режим панели запуска, направление и якорь.
schema-leader = Лидер
schema-leader-detail = Префиксная клавиша для аккордных сочетаний.
schema-chord-timeout = Таймаут аккорда
schema-chord-timeout-detail = Миллисекунды до истечения времени ожидания префикса аккорда.
schema-bindings = Привязки
schema-confirm-close = Подтверждение закрытия
schema-confirm-close-detail = Запрашивать подтверждение перед закрытием терминала с запущенным процессом.
schema-default-theme = Тема по умолчанию
schema-default-theme-detail = Название активной темы из списка тем.
