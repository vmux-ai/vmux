locale-name = српски
common-open = Отвори
common-close = Затвори
common-install = Инсталирај
common-uninstall = Деинсталирај
common-update = Ажурирај
common-retry = Покушај поново
common-refresh = Освежи
common-remove = Уклони
common-enable = Омогући
common-disable = Онемогући
common-new = Ново
common-active = активно
common-running = покренуто
common-done = готово
common-failed = Није успело
common-installed = Инсталирано
common-items = { $count ->
    [one] { $count } ставка
   *[other] { $count } ставки
}
start-title = Почетак
start-tagline = Један prompt. Све завршено.

agents-title = Агенти
agents-search = Претражите ACP и CLI агенте…
agents-empty = Нема одговарајућих агената
agents-empty-detail = Покушајте са називом, runtime-ом или ACP/CLI.
agents-install-failed = Инсталација није успела
agents-updating = Ажурирање…
agents-retrying = Поновни покушај…
agents-preparing = Припрема…

extensions-title = Проширења
extensions-search = Претражите инсталирана проширења или Chrome Web Store…
extensions-relaunch = Поново покрените за примену
extensions-empty = Нема инсталираних проширења
extensions-no-match = Нема одговарајућих проширења
extensions-empty-detail = Претражите Chrome Web Store изнад и притисните Enter.
extensions-no-match-detail = Покушајте са другим називом или ID-јем проширења.
extensions-on = Укључено
extensions-off = Искључено
extensions-enable-confirm = Омогућити { $name }?
extensions-enable-permissions = Омогућите { $name } и дозволите:

lsp-title = Језички сервери
lsp-search = Претражите језичке сервере, линтере, форматере…
lsp-loading = Учитавање каталога…
lsp-empty = Нема одговарајућих језичких сервера
lsp-empty-detail = Покушајте са другим језиком, линтером или форматером.
lsp-needs = захтева { $tool }
lsp-status-available = Доступно
lsp-status-on-path = На PATH-у
lsp-status-installing = Инсталирање…
lsp-status-installed = Инсталирано
lsp-status-outdated = Доступно ажурирање
lsp-status-running = Покренуто
lsp-status-failed = Није успело

spaces-title = Простори
spaces-new-placeholder = Назив новог простора
spaces-empty = Нема простора
spaces-default-name = Простор { $number }
spaces-tabs = { $count ->
    [one] 1 картица
   *[other] { $count } картица
}
spaces-delete = Обриши простор

team-title = Тим
team-just-you = Само ви у овом простору
team-agents = { $count ->
    [one] Ви и 1 агент
   *[other] Ви и { $count } агената
}
team-empty = Овде још нема никога
team-you = Ви
team-agent = Агент

services-title = Позадински сервиси
services-processes = { $count ->
    [one] 1 процес
   *[other] { $count } процеса
}
services-kill-all = Прекини све
services-not-running = Сервис није покренут
services-start-with = Покрени са:
services-empty = Нема активних процеса
services-filter = Филтрирај процесе…
services-no-match = Нема одговарајућих процеса
services-connected = Повезано
services-disconnected = Прекинута веза
services-attached = повезано
services-kill = Прекини
services-memory = Меморија
services-size = Величина
services-shell = Shell

error-title = Грешка

history-search = Претражи историју
history-clear-all = Обриши све
history-clear-confirm = Обрисати целу историју?
history-clear-warning = Ово није могуће опозвати.
history-cancel = Откажи
history-today = Данас
history-yesterday = Јуче
history-days-ago = пре { $count } дана
history-day-offset = Дан -{ $count }

settings-title = Подешавања
settings-loading = Учитавање подешавања…
settings-stored = Чува се у ~/.vmux/settings.ron
settings-other = Остало
settings-software-update = Ажурирање софтвера
settings-check-updates = Провери ажурирања
settings-check-updates-hint = Проверава се аутоматски при покретању и сваког сата када је аутоматско ажурирање омогућено.
settings-update-unavailable = Недоступно
settings-update-unavailable-hint = Ажурирач није укључен у ово издање.
settings-update-checking = Провера…
settings-update-checking-hint = Провера ажурирања…
settings-update-check-again = Провери поново
settings-update-current = Vmux је ажуран.
settings-update-downloading = Преузимање…
settings-update-downloading-hint = Преузима се Vmux { $version }…
settings-update-installing = Инсталирање…
settings-update-installing-hint = Инсталира се Vmux { $version }…
settings-update-ready = Ажурирање је спремно
settings-update-ready-hint = Vmux { $version } је спреман. Поново покрените да бисте га применили.
settings-update-try-again = Покушај поново
settings-update-failed = Није могуће проверити ажурирања.
settings-item = Ставка
settings-item-number = Ставка { $number }
settings-press-key = Притисните тастер…
settings-saved = Сачувано
settings-record-key = Кликните да снимите нову комбинацију тастера

tray-open-window = Отвори прозор
tray-close-window = Затвори прозор
tray-pause-recording = Паузирај снимање
tray-resume-recording = Настави снимање
tray-finish-recording = Заврши снимање
tray-quit = Затвори Vmux

composer-attach-files = Приложи датотеке (/upload)
composer-remove-attachment = Уклони прилог

layout-back = Назад
layout-forward = Напред
layout-reload = Поново учитај
layout-bookmark-page = Додај страницу у обележиваче
layout-remove-bookmark = Уклони обележивач
layout-pin-page = Закачи ову страницу
layout-unpin-page = Откачи ову страницу
layout-manage-extensions = Управљај проширењима
layout-new-stack = Нови стек
layout-close-tab = Затвори картицу
layout-bookmark = Обележивач
layout-pin = Закачи
layout-new-tab = Нова картица
layout-team = Тим

command-switch-space = Промени простор…
command-search-ask = Претражите или питајте…
command-new-tab-placeholder = Претражите, унесите URL или изаберите Terminal…
command-placeholder = Унесите URL, претражите картице или > за команде…
command-composer-placeholder = Унесите / за команде или @ за медије
command-send = Пошаљи (Enter)
command-terminal = Терминал
command-open-terminal = Отвори у терминалу
command-stack = Стек
command-tabs = { $count ->
    [one] 1 картица
   *[other] { $count } картица
}
command-prompt = Prompt
command-new-tab = Нова картица
command-search = Претрага
command-open-value = Отвори „{ $value }”
command-search-value = Претражи „{ $value }”

schema-appearance = Изглед
schema-general = Опште
schema-layout = Распоред
schema-layout-detail = Прозор, панели, бочна трака и оквир фокуса.
schema-agent = Агент
schema-agent-detail = Понашање агента и дозволе за алате.
schema-shortcuts = Пречице
schema-shortcuts-detail = Само за читање. Измените settings.ron директно да промените пречице.
schema-terminal = Терминал
schema-browser = Прегледач
schema-mode = Режим
schema-mode-detail = Шема боја за веб-странице. Уређај прати систем.
schema-device = Уређај
schema-light = Светла
schema-dark = Тамна
schema-language = Језик
schema-language-detail = Користите системски, en-US, ja или било коју BCP 47 ознаку са одговарајућим каталогом ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Аутоматско ажурирање
schema-auto-update-detail = Проверавај и инсталирај ажурирања при покретању и сваког сата.
schema-startup-url = Почетни URL
schema-startup-url-detail = Ако је празно, отвара се prompt командне траке.
schema-search-engine = Претраживач
schema-search-engine-detail = Користи се за веб-претраге са почетне странице и из командне траке.
schema-window = Прозор
schema-pane = Панел
schema-side-sheet = Бочни лист
schema-focus-ring = Оквир фокуса
schema-run-placement = Дозволи замену положаја покретања
schema-run-placement-detail = Дозволите агентима да изаберу режим, смер и сидро панела за покретање.
schema-leader = Лидер
schema-leader-detail = Префикс тастер за chord пречице.
schema-chord-timeout = Истек chord-а
schema-chord-timeout-detail = Милисекунде пре него што префикс chord-а истекне.
schema-bindings = Везе
schema-confirm-close = Потврди затварање
schema-confirm-close-detail = Питај пре затварања терминала са покренутим процесом.
schema-default-theme = Подразумевана тема
schema-default-theme-detail = Назив активне теме са листе тема.
