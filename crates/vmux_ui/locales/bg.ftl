common-open = Отворете
common-close = затвори
common-install = Инсталирайте
common-uninstall = Деинсталиране
common-update = Актуализация
common-retry = Опитайте отново
common-refresh = Опресняване
common-remove = Премахнете
common-enable = Активирайте
common-disable = Деактивиране
common-new = Нов
common-active = активен
common-running = бягане
common-done = готово
common-failed = Неуспешно
common-installed = Инсталиран
common-items = { $count ->
    [one] { $count } елемент
   *[other] { $count } елемента
}
start-title = Започнете
start-tagline = Една подкана. Всичко, готово.

agents-title = Агенти
agents-search = Търсете ACP и CLI агенти...
agents-empty = Няма съответстващи агенти
agents-empty-detail = Опитайте име, време за изпълнение или ACP/CLI.
agents-install-failed = Инсталирането е неуспешно
agents-updating = Актуализиране...
agents-retrying = Повторен опит...
agents-preparing = Подготвя се...

extensions-title = Разширения
extensions-search = Търсене инсталирано или Chrome Web Store…
extensions-relaunch = Рестартирайте, за да приложите
extensions-empty = Няма инсталирани разширения
extensions-no-match = Няма съответстващи разширения
extensions-empty-detail = Потърсете Chrome Web Store по-горе и натиснете Return.
extensions-no-match-detail = Опитайте с друго име или идентификатор на разширение.
extensions-on = включено
extensions-off = Изкл
extensions-enable-confirm = Да се активира ли { $name }?
extensions-enable-permissions = Активирайте { $name } и разрешете:

lsp-title = Езикови сървъри
lsp-search = Търсене на езикови сървъри, линтери, формати...
lsp-loading = Каталогът се зарежда...
lsp-empty = Няма съответстващи езикови сървъри
lsp-empty-detail = Опитайте друг език, линтер или форматираща програма.
lsp-needs = се нуждае от { $tool }
lsp-status-available = Наличен
lsp-status-on-path = На PATH
lsp-status-installing = Инсталиране...
lsp-status-installed = Инсталиран
lsp-status-outdated = Налична актуализация
lsp-status-running = Бягане
lsp-status-failed = Неуспешно

spaces-title = Пространства
spaces-new-placeholder = Ново име на пространство
spaces-empty = Без интервали
spaces-default-name = Пространство { $number }
spaces-tabs = { $count ->
    [one] 1 табл
   *[other] { $count } раздела
}
spaces-delete = Изтриване на пространство

team-title = Екип
team-just-you = Само ти в това пространство
team-agents = { $count ->
    [one] Вие и 1 агент
   *[other] Вие и { $count } агенти
}
team-empty = Тук още няма никой
team-you = Вие
team-agent = агент

services-title = Фонови услуги
services-processes = { $count ->
    [one] 1 процес
   *[other] { $count } процеси
}
services-kill-all = Убий всички
services-not-running = Услугата не работи
services-start-with = Започнете с:
services-empty = Няма активни процеси
services-filter = Филтриране на процеси...
services-no-match = Няма съвпадащи процеси
services-connected = Свързан
services-disconnected = Прекъсната връзка
services-attached = приложен
services-kill = Убий
services-memory = памет
services-size = Размер
services-shell = Черупка

error-title = Грешка

history-search = История на търсенето
history-clear-all = Изчисти всички
history-clear-confirm = Изчистване на цялата история?
history-clear-warning = Това не може да бъде отменено.
history-cancel = Отказ
history-today = Днес
history-yesterday = Вчера
history-days-ago = преди { $count } дни
history-day-offset = Ден -{ $count }

settings-title = Настройки
settings-loading = Настройките се зареждат...
settings-stored = Съхранява се в ~/.vmux/settings.ron
settings-other = други
settings-software-update = Актуализация на софтуера
settings-check-updates = Проверете за актуализации
settings-check-updates-hint = Проверява автоматично при стартиране и на всеки час, когато е активирана автоматичната актуализация.
settings-update-unavailable = Недостъпен
settings-update-unavailable-hint = Актуализаторът не е включен в тази компилация.
settings-update-checking = Проверка...
settings-update-checking-hint = Проверка за актуализации...
settings-update-check-again = Проверете отново
settings-update-current = Vmux е актуален.
settings-update-downloading = Изтегля се...
settings-update-downloading-hint = Изтегля се Vmux { $version }…
settings-update-installing = Инсталиране...
settings-update-installing-hint = Инсталиране на Vmux { $version }…
settings-update-ready = Актуализацията е готова
settings-update-ready-hint = Vmux { $version } е готов. Рестартирайте, за да го приложите.
settings-update-try-again = Опитайте отново
settings-update-failed = Не може да се провери за актуализации.
settings-item = Артикул
settings-item-number = Артикул { $number }
settings-press-key = Натиснете клавиш...
settings-saved = Запазено
settings-record-key = Кликнете, за да запишете нова клавишна комбинация

tray-open-window = Отворен прозорец
tray-close-window = Затваряне на прозореца
tray-pause-recording = Пауза на записа
tray-resume-recording = Възобновяване на записа
tray-finish-recording = Завършете записа
tray-quit = Излезте от Vmux

composer-attach-files = Прикачване на файлове (/upload)
composer-remove-attachment = Премахване на прикачения файл

layout-back = Назад
layout-forward = Напред
layout-reload = Презареди
layout-bookmark-page = Маркирайте тази страница
layout-remove-bookmark = Премахване на отметка
layout-pin-page = Фиксирайте тази страница
layout-unpin-page = Освободете тази страница
layout-manage-extensions = Управление на разширенията
layout-new-stack = Нов стек
layout-close-tab = Затваряне на раздела
layout-bookmark = Отметка
layout-pin = ПИН
layout-new-tab = Нов раздел
layout-team = Екип

command-switch-space = Превключване на място…
command-search-ask = Потърсете или попитайте...
command-new-tab-placeholder = Потърсете или въведете URL или изберете Терминал…
command-placeholder = Въведете URL, търсете раздели или > за команди…
command-composer-placeholder = Въведете / за команди или @ за медии
command-send = Изпрати (Enter)
command-terminal = Терминал
command-open-terminal = Отворете в терминал
command-stack = Стек
command-tabs = { $count ->
    [one] 1 табл
   *[other] { $count } раздела
}
command-prompt = подкана
command-new-tab = Нов раздел
command-search = Търсене
command-open-value = Отворете „{ $value }“
command-search-value = Търсете „{ $value }“

schema-appearance = Външен вид
schema-general = генерал
schema-layout = Оформление
schema-layout-detail = Прозорец, панели, странична лента и пръстен за фокусиране.
schema-agent = агент
schema-agent-detail = Поведение на агент и разрешения за инструменти.
schema-shortcuts = Преки пътища
schema-shortcuts-detail = Изглед само за четене. Редактирайте settings.ron директно, за да промените обвързванията.
schema-terminal = Терминал
schema-browser = Браузър
schema-mode = Режим
schema-mode-detail = Цветова схема за уеб страници. Устройството следва вашата система.
schema-device = устройство
schema-light = светлина
schema-dark = Тъмно
schema-language = език
schema-language-detail = Използвайте system, en-US, ja или който и да е BCP 47 таг със съответстващ ~/.vmux/locales/<tag>.ftl каталог.
schema-auto-update = Автоматична актуализация
schema-auto-update-detail = Проверявайте и инсталирайте актуализации при стартиране и на всеки час.
schema-startup-url = Стартиране URL
schema-startup-url-detail = Empty отваря прозореца на командната лента.
schema-search-engine = Търсачка
schema-search-engine-detail = Използва се за уеб търсения от Старт и командната лента.
schema-window = прозорец
schema-pane = Прозорец
schema-side-sheet = Страничен лист
schema-focus-ring = Пръстен за фокусиране
schema-run-placement = Разрешаване на отмяна на разположението на изпълнение
schema-run-placement-detail = Позволете на агентите да изберат режим на панел за изпълнение, посока и котва.
schema-leader = лидер
schema-leader-detail = Префиксен клавиш за преки пътища за акорди.
schema-chord-timeout = Изчакване на акорда
schema-chord-timeout-detail = Милисекунди преди префиксът на акорда да изтече.
schema-bindings = Подвързии
schema-confirm-close = Потвърдете затваряне
schema-confirm-close-detail = Подкана преди затваряне на терминал с работещ процес.
schema-default-theme = Тема по подразбиране
schema-default-theme-detail = Име на активната тема от списъка с теми.
