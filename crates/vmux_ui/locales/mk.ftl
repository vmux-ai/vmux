locale-name = македонски
common-open = Отвори
common-close = Затвори
common-install = Инсталирај
common-uninstall = Деинсталирај
common-update = Ажурирај
common-retry = Обиди се повторно
common-refresh = Освежи
common-remove = Отстрани
common-enable = Овозможи
common-disable = Оневозможи
common-new = Ново
common-active = активно
common-running = работи
common-done = готово
common-failed = Неуспешно
common-installed = Инсталирано
common-items = { $count ->
    [one] { $count } ставка
   *[other] { $count } ставки
}
start-title = Почеток
start-tagline = Едно упатство. Сè е завршено.

agents-title = Агенти
agents-search = Пребарај ACP и CLI агенти…
agents-empty = Нема соодветни агенти
agents-empty-detail = Обиди се со име, извршна околина или ACP/CLI.
agents-install-failed = Инсталацијата не успеа
agents-updating = Се ажурира…
agents-retrying = Се обидува повторно…
agents-preparing = Се подготвува…

extensions-title = Екстензии
extensions-search = Пребарај инсталирани или во Chrome Web Store…
extensions-relaunch = Стартувај повторно за примена
extensions-empty = Нема инсталирани екстензии
extensions-no-match = Нема соодветни екстензии
extensions-empty-detail = Пребарај во Chrome Web Store погоре и притисни Enter.
extensions-no-match-detail = Обиди се со друго име или ID на екстензија.
extensions-on = Вклучено
extensions-off = Исклучено
extensions-enable-confirm = Да се овозможи { $name }?
extensions-enable-permissions = Овозможи { $name } и дозволи:

lsp-title = Јазични сервери
lsp-search = Пребарај јазични сервери, линтери, форматери…
lsp-loading = Се вчитува каталогот…
lsp-empty = Нема соодветни јазични сервери
lsp-empty-detail = Обиди се со друг јазик, линтер или форматер.
lsp-needs = бара { $tool }
lsp-status-available = Достапно
lsp-status-on-path = На PATH
lsp-status-installing = Се инсталира…
lsp-status-installed = Инсталирано
lsp-status-outdated = Достапно ажурирање
lsp-status-running = Работи
lsp-status-failed = Неуспешно

spaces-title = Простори
spaces-new-placeholder = Име на нов простор
spaces-empty = Нема простори
spaces-default-name = Простор { $number }
spaces-tabs = { $count ->
    [one] 1 јазиче
   *[other] { $count } јазичиња
}
spaces-delete = Избриши простор

team-title = Тим
team-just-you = Само ти си во овој простор
team-agents = { $count ->
    [one] Ти и 1 агент
   *[other] Ти и { $count } агенти
}
team-empty = Тука сè уште нема никој
team-you = Ти
team-agent = Агент

services-title = Позадински услуги
services-processes = { $count ->
    [one] 1 процес
   *[other] { $count } процеси
}
services-kill-all = Прекини ги сите
services-not-running = Услугата не работи
services-start-with = Стартувај со:
services-empty = Нема активни процеси
services-filter = Филтрирај процеси…
services-no-match = Нема соодветни процеси
services-connected = Поврзано
services-disconnected = Исклучено
services-attached = прикачено
services-kill = Прекини
services-memory = Меморија
services-size = Големина
services-shell = Школка

error-title = Грешка

history-search = Пребарај историја
history-clear-all = Исчисти сè
history-clear-confirm = Да се исчисти целата историја?
history-clear-warning = Ова не може да се врати.
history-cancel = Откажи
history-today = Денес
history-yesterday = Вчера
history-days-ago = Пред { $count } дена
history-day-offset = Ден -{ $count }

settings-title = Поставки
settings-loading = Се вчитуваат поставките…
settings-stored = Зачувано во ~/.vmux/settings.ron
settings-other = Друго
settings-software-update = Ажурирање на софтвер
settings-check-updates = Провери за ажурирања
settings-check-updates-hint = Проверува автоматски при стартување и секој час кога е овозможено автоматско ажурирање.
settings-update-unavailable = Недостапно
settings-update-unavailable-hint = Ажурирачот не е вклучен во оваа верзија.
settings-update-checking = Се проверува…
settings-update-checking-hint = Се проверува за ажурирања…
settings-update-check-again = Провери повторно
settings-update-current = Vmux е ажуриран.
settings-update-downloading = Се презема…
settings-update-downloading-hint = Се презема Vmux { $version }…
settings-update-installing = Се инсталира…
settings-update-installing-hint = Се инсталира Vmux { $version }…
settings-update-ready = Ажурирањето е подготвено
settings-update-ready-hint = Vmux { $version } е подготвен. Рестартирај за примена.
settings-update-try-again = Обиди се повторно
settings-update-failed = Не може да се провери за ажурирања.
settings-item = Ставка
settings-item-number = Ставка { $number }
settings-press-key = Притисни копче…
settings-saved = Зачувано
settings-record-key = Кликни за снимање нова комбинација на копчиња

tray-open-window = Отвори прозорец
tray-close-window = Затвори прозорец
tray-pause-recording = Паузирај снимање
tray-resume-recording = Продолжи снимање
tray-finish-recording = Заврши снимање
tray-quit = Излези од Vmux

composer-attach-files = Прикачи датотеки (/upload)
composer-remove-attachment = Отстрани прилог

layout-back = Назад
layout-forward = Напред
layout-reload = Освежи
layout-bookmark-page = Додај ја страницата во обележувачи
layout-remove-bookmark = Отстрани обележувач
layout-pin-page = Закачи ја страницата
layout-unpin-page = Откачи ја страницата
layout-manage-extensions = Управувај со екстензии
layout-new-stack = Нов слој
layout-close-tab = Затвори јазиче
layout-bookmark = Обележувач
layout-pin = Закачи
layout-new-tab = Ново јазиче
layout-team = Тим

command-switch-space = Смени простор…
command-search-ask = Пребарај или прашај…
command-new-tab-placeholder = Пребарај или внеси URL, или избери Терминал…
command-placeholder = Внеси URL, пребарај јазичиња или > за команди…
command-composer-placeholder = Внеси / за команди или @ за медиуми
command-send = Испрати (Enter)
command-terminal = Терминал
command-open-terminal = Отвори во Терминал
command-stack = Слој
command-tabs = { $count ->
    [one] 1 јазиче
   *[other] { $count } јазичиња
}
command-prompt = Упатство
command-new-tab = Ново јазиче
command-search = Пребарај
command-open-value = Отвори „{ $value }“
command-search-value = Пребарај „{ $value }“

schema-appearance = Изглед
schema-general = Општо
schema-layout = Распоред
schema-layout-detail = Прозорец, панели, странична лента и прстен за фокус.
schema-agent = Агент
schema-agent-detail = Однесување на агентот и дозволи за алатки.
schema-shortcuts = Кратенки
schema-shortcuts-detail = Само за преглед. Уреди settings.ron директно за да ги смениш врските.
schema-terminal = Терминал
schema-browser = Прелистувач
schema-mode = Режим
schema-mode-detail = Шема на бои за веб-страници. Уредот го следи системот.
schema-device = Уред
schema-light = Светло
schema-dark = Темно
schema-language = Јазик
schema-language-detail = Користи системски, en-US, ja или која било BCP 47 ознака со соодветен ~/.vmux/locales/<tag>.ftl каталог.
schema-auto-update = Автоматско ажурирање
schema-auto-update-detail = Проверувај и инсталирај ажурирања при стартување и секој час.
schema-startup-url = Почетен URL
schema-startup-url-detail = Празно го отвора барањето во командната лента.
schema-search-engine = Пребарувач
schema-search-engine-detail = Се користи за веб-пребарувања од Почеток и командната лента.
schema-window = Прозорец
schema-pane = Панел
schema-side-sheet = Страничен лист
schema-focus-ring = Прстен за фокус
schema-run-placement = Дозволи агентите да го менуваат местото за извршување
schema-run-placement-detail = Дозволи агентите да изберат режим, насока и сидро на панелот за извршување.
schema-leader = Водечко копче
schema-leader-detail = Префиксно копче за chord кратенки.
schema-chord-timeout = Истек на chord
schema-chord-timeout-detail = Милисекунди пред да истече префиксот за chord.
schema-bindings = Врски
schema-confirm-close = Потврди затворање
schema-confirm-close-detail = Прашај пред затворање терминал со активен процес.
schema-default-theme = Стандардна тема
schema-default-theme-detail = Име на активната тема од списокот со теми.
