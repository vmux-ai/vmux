common-open = Отвори
common-close = Затвори
common-install = Инсталирајте
common-uninstall = Деинсталирајте
common-update = Ажурирање
common-retry = Обидете се повторно
common-refresh = Освежи
common-remove = Отстрани
common-enable = Овозможи
common-disable = Оневозможи
common-new = Ново
common-active = активни
common-running = трчање
common-done = направено
common-failed = Неуспешно
common-installed = Инсталиран
common-items = { $count ->
    [one] { $count } ставка
   *[other] { $count } ставки
}
start-title = Започнете
start-tagline = Еден потсетник. Било, готово.

agents-title = Агенти
agents-search = Пребарајте ги агентите ACP и CLI…
agents-empty = Нема соодветни агенти
agents-empty-detail = Обидете се со име, време на траење или ACP/CLI.
agents-install-failed = Инсталирањето не успеа
agents-updating = Се ажурира…
agents-retrying = Се обидува повторно…
agents-preparing = Се подготвува…

extensions-title = Екстензии
extensions-search = Пребарување инсталирано или Chrome Web Store…
extensions-relaunch = Рестартирајте за да аплицирате
extensions-empty = Не се инсталирани екстензии
extensions-no-match = Нема соодветни наставки
extensions-empty-detail = Пребарајте го Chrome Web Store погоре и притиснете Return.
extensions-no-match-detail = Обидете се со друго име или ID на екстензија.
extensions-on = Вклучено
extensions-off = Исклучено
extensions-enable-confirm = Овозможи { $name }?
extensions-enable-permissions = Овозможете { $name } и дозволете:

lsp-title = Јазични сервери
lsp-search = Пребарајте јазични сервери, линтери, форматери…
lsp-loading = Се вчитува каталогот…
lsp-empty = Нема соодветни јазични сервери
lsp-empty-detail = Обидете се со друг јазик, линтер или форматер.
lsp-needs = има потреба од { $tool }
lsp-status-available = Достапно
lsp-status-on-path = На PATH
lsp-status-installing = Се инсталира…
lsp-status-installed = Инсталиран
lsp-status-outdated = Достапно е ажурирање
lsp-status-running = Трчање
lsp-status-failed = Неуспешно

spaces-title = Простори
spaces-new-placeholder = Ново име на просторот
spaces-empty = Нема празни места
spaces-default-name = Простор { $number }
spaces-tabs = { $count ->
    [one] 1 таб
   *[other] { $count } јазичиња
}
spaces-delete = Избришете простор

team-title = Тим
team-just-you = Само вие во овој простор
team-agents = { $count ->
    [one] Вие и 1 агент
   *[other] Вие и { $count } агенти
}
team-empty = Сè уште нема никој овде
team-you = Вие
team-agent = Агент

services-title = Услуги во позадина
services-processes = { $count ->
    [one] 1 процес
   *[other] { $count } процеси
}
services-kill-all = Убиј ги сите
services-not-running = Услугата не работи
services-start-with = Започнете со:
services-empty = Нема активни процеси
services-filter = Филтрирајте ги процесите…
services-no-match = Нема процеси за совпаѓање
services-connected = Поврзан
services-disconnected = Исклучено
services-attached = прикачен
services-kill = Убиј
services-memory = Меморија
services-size = Големина
services-shell = Школка

error-title = Грешка

history-search = Историја на пребарување
history-clear-all = Исчистете ги сите
history-clear-confirm = Да се исчисти целата историја?
history-clear-warning = Ова не може да се врати.
history-cancel = Откажи
history-today = Денес
history-yesterday = Вчера
history-days-ago = пред { $count } дена
history-day-offset = Ден -{ $count }

settings-title = Поставки
settings-loading = Се вчитуваат поставките…
settings-stored = Зачувано во ~/.vmux/settings.ron
settings-other = Друго
settings-software-update = Ажурирање на софтвер
settings-check-updates = Проверете дали има ажурирања
settings-check-updates-hint = Се проверува автоматски при стартување и секој час кога е овозможено автоматското ажурирање.
settings-update-unavailable = Недостапно
settings-update-unavailable-hint = Ажурирачот не е вклучен во оваа верзија.
settings-update-checking = Се проверува…
settings-update-checking-hint = Се проверува за ажурирања…
settings-update-check-again = Проверете повторно
settings-update-current = Vmux е ажуриран.
settings-update-downloading = Се презема…
settings-update-downloading-hint = Се презема Vmux { $version }…
settings-update-installing = Се инсталира…
settings-update-installing-hint = Се инсталира Vmux { $version }…
settings-update-ready = Подготвено за ажурирање
settings-update-ready-hint = Vmux { $version } е подготвен. Рестартирајте за да го примените.
settings-update-try-again = Обидете се повторно
settings-update-failed = Не може да се провери дали има ажурирања.
settings-item = Ставка
settings-item-number = Ставка { $number }
settings-press-key = Притиснете копче…
settings-saved = Зачувано
settings-record-key = Кликнете за да снимите нова комбинација на копчиња

tray-open-window = Отворете го прозорецот
tray-close-window = Затвори го прозорецот
tray-pause-recording = Паузирајте го снимањето
tray-resume-recording = Продолжи со снимање
tray-finish-recording = Завршете го снимањето
tray-quit = Напуштете се Vmux

composer-attach-files = Прикачи датотеки (/upload)
composer-remove-attachment = Отстранете го прилогот

layout-back = Назад
layout-forward = Напред
layout-reload = Вчитај повторно
layout-bookmark-page = Обележете ја оваа страница
layout-remove-bookmark = Отстранете го обележувачот
layout-pin-page = Закачете ја оваа страница
layout-unpin-page = Откачете ја оваа страница
layout-manage-extensions = Управувајте со екстензии
layout-new-stack = Нов стек
layout-close-tab = Затвори ја картичката
layout-bookmark = Обележете
layout-pin = Пин
layout-new-tab = Ново јазиче
layout-team = Тим

command-switch-space = Префрли простор…
command-search-ask = Пребарувајте или прашајте…
command-new-tab-placeholder = Пребарајте или напишете URL или изберете Терминал…
command-placeholder = Напишете URL, пребарувајте јазичиња или > за команди…
command-composer-placeholder = Внесете / за команди или @ за медиум
command-send = Испрати (Enter)
command-terminal = Терминал
command-open-terminal = Отвори во терминал
command-stack = Стак
command-tabs = { $count ->
    [one] 1 таб
   *[other] { $count } јазичиња
}
command-prompt = Прашај
command-new-tab = Ново јазиче
command-search = Пребарување
command-open-value = Отворете „{ $value }“
command-search-value = Пребарајте „{ $value }“

schema-appearance = Изглед
schema-general = Општо
schema-layout = Распоред
schema-layout-detail = Прозорец, стакла, странична лента и прстен за фокусирање.
schema-agent = Агент
schema-agent-detail = Однесување на агентот и дозволи за алатки.
schema-shortcuts = Кратенки
schema-shortcuts-detail = Приказ само за читање. Уредете settings.ron директно за да ги промените врските.
schema-terminal = Терминал
schema-browser = Прелистувач
schema-mode = Режим
schema-mode-detail = Шема на бои за веб-страници. Уредот го следи вашиот систем.
schema-device = Уред
schema-light = Светлина
schema-dark = Темно
schema-language = Јазик
schema-language-detail = Користете систем, en-US, ja или која било ознака BCP 47 со соодветен каталог ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Автоматско ажурирање
schema-auto-update-detail = Проверете и инсталирајте ажурирања при стартување и секој час.
schema-startup-url = Стартување URL
schema-startup-url-detail = Празни ја отвора линијата за команди.
schema-search-engine = Пребарувач
schema-search-engine-detail = Се користи за веб-пребарувања од Start и командната лента.
schema-window = Прозорец
schema-pane = Панел
schema-side-sheet = Страничен лист
schema-focus-ring = Фокус прстен
schema-run-placement = Дозволи отфрлање на поставеноста на стартување
schema-run-placement-detail = Дозволете им на агентите да изберат режим на окното за извршување, насока и прицврстување.
schema-leader = Водач
schema-leader-detail = Копче за префикс за кратенки на акорд.
schema-chord-timeout = Истекување на акордите
schema-chord-timeout-detail = Милисекунди пред да истече префиксот на акорд.
schema-bindings = Поврзувања
schema-confirm-close = Потврдете затворање
schema-confirm-close-detail = Прашајте пред да затворите терминал со процес кој работи.
schema-default-theme = Стандардна тема
schema-default-theme-detail = Име на активната тема од списокот со теми.
