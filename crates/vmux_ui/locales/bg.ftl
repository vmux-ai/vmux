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
