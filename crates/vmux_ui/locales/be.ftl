common-open = Адкрыць
common-close = Закрыць
common-install = Усталяваць
common-uninstall = Выдаліць
common-update = Абнавіць
common-retry = Паўтарыць
common-refresh = Абнавіць
common-remove = Прыбраць
common-enable = Уключыць
common-disable = Адключыць
common-new = Новы
common-active = актыўна
common-running = выконваецца
common-done = гатова
common-failed = Збой
common-installed = Усталявана
common-items = { $count ->
    [one] { $count } элемент
   *[other] { $count } элементаў
}
start-title = Пачатак
start-tagline = Адзін промпт — і ўсё гатова.

agents-title = Агенты
agents-search = Пошук ACP- і CLI-агентаў…
agents-empty = Падыходных агентаў няма
agents-empty-detail = Паспрабуйце назву, асяроддзе выканання або ACP/CLI.
agents-install-failed = Не ўдалося ўсталяваць
agents-updating = Абнаўленне…
agents-retrying = Паўтор…
agents-preparing = Падрыхтоўка…

extensions-title = Пашырэнні
extensions-search = Пошук сярод усталяваных або ў Chrome Web Store…
extensions-relaunch = Перазапусціць, каб прымяніць
extensions-empty = Пашырэнні не ўсталяваны
extensions-no-match = Падыходных пашырэнняў няма
extensions-empty-detail = Знайдзіце пашырэнне ў Chrome Web Store вышэй і націсніце Enter.
extensions-no-match-detail = Паспрабуйце іншую назву або ID пашырэння.
extensions-on = Укл.
extensions-off = Выкл.
extensions-enable-confirm = Уключыць { $name }?
extensions-enable-permissions = Уключыць { $name } і дазволіць:

lsp-title = Моўныя серверы
lsp-search = Пошук моўных сервераў, лінтараў, фарматараў…
lsp-loading = Загрузка каталога…
lsp-empty = Падыходных моўных сервераў няма
lsp-empty-detail = Паспрабуйце іншую мову, лінтар або фарматар.
lsp-needs = патрэбны { $tool }
lsp-status-available = Даступны
lsp-status-on-path = У PATH
lsp-status-installing = Усталяванне…
lsp-status-installed = Усталявана
lsp-status-outdated = Ёсць абнаўленне
lsp-status-running = Выконваецца
lsp-status-failed = Збой

spaces-title = Прасторы
spaces-new-placeholder = Назва новай прасторы
spaces-empty = Прастор няма
spaces-default-name = Прастора { $number }
spaces-tabs = { $count ->
    [one] 1 укладка
   *[other] { $count } укладак
}
spaces-delete = Выдаліць прастору

team-title = Каманда
team-just-you = У гэтай прасторы толькі вы
team-agents = { $count ->
    [one] Вы і 1 агент
   *[other] Вы і { $count } агентаў
}
team-empty = Тут пакуль нікога няма
team-you = Вы
team-agent = Агент

services-title = Фонавыя службы
services-processes = { $count ->
    [one] 1 працэс
   *[other] { $count } працэсаў
}
services-kill-all = Завяршыць усе прымусова
services-not-running = Служба не запушчана
services-start-with = Запускаць з:
services-empty = Актыўных працэсаў няма
services-filter = Фільтр працэсаў…
services-no-match = Падыходных працэсаў няма
services-connected = Падключана
services-disconnected = Адключана
services-attached = далучана
services-kill = Завяршыць прымусова
services-memory = Памяць
services-size = Памер
services-shell = Абалонка

error-title = Памылка

history-search = Пошук у гісторыі
history-clear-all = Ачысціць усё
history-clear-confirm = Ачысціць усю гісторыю?
history-clear-warning = Гэта дзеянне нельга скасаваць.
history-cancel = Скасаваць
history-today = Сёння
history-yesterday = Учора
history-days-ago = { $count } дзён таму
history-day-offset = Дзень -{ $count }

settings-title = Налады
settings-loading = Загрузка налад…
settings-stored = Захоўваецца ў ~/.vmux/settings.ron
settings-other = Іншае
settings-software-update = Абнаўленне праграмы
settings-check-updates = Праверыць абнаўленні
settings-check-updates-hint = Правяраецца аўтаматычна пры запуску і штогадзіну, калі ўключана аўтаабнаўленне.
settings-update-unavailable = Недаступна
settings-update-unavailable-hint = У гэтай зборцы няма модуля абнаўлення.
settings-update-checking = Праверка…
settings-update-checking-hint = Праверка абнаўленняў…
settings-update-check-again = Праверыць яшчэ раз
settings-update-current = Vmux абноўлены да апошняй версіі.
settings-update-downloading = Спампоўванне…
settings-update-downloading-hint = Спампоўванне Vmux { $version }…
settings-update-installing = Усталяванне…
settings-update-installing-hint = Усталяванне Vmux { $version }…
settings-update-ready = Абнаўленне гатова
settings-update-ready-hint = Vmux { $version } гатовы. Перазапусціце, каб прымяніць абнаўленне.
settings-update-try-again = Паспрабаваць яшчэ раз
settings-update-failed = Не ўдалося праверыць абнаўленні.
settings-item = Элемент
settings-item-number = Элемент { $number }
settings-press-key = Націсніце клавішу…
settings-saved = Захавана
settings-record-key = Націсніце, каб запісаць новую камбінацыю клавіш

tray-open-window = Адкрыць акно
tray-close-window = Закрыць акно
tray-pause-recording = Прыпыніць запіс
tray-resume-recording = Працягнуць запіс
tray-finish-recording = Завяршыць запіс
tray-quit = Выйсці з Vmux

composer-attach-files = Далучыць файлы (/upload)
composer-remove-attachment = Прыбраць укладанне

layout-back = Назад
layout-forward = Наперад
layout-reload = Перазагрузіць
layout-bookmark-page = Дадаць старонку ў закладкі
layout-remove-bookmark = Прыбраць закладку
layout-pin-page = Замацаваць старонку
layout-unpin-page = Адмацаваць старонку
layout-manage-extensions = Кіраваць пашырэннямі
layout-new-stack = Новы стэк
layout-close-tab = Закрыць укладку
layout-bookmark = Закладка
layout-pin = Замацаваць
layout-new-tab = Новая ўкладка
layout-team = Каманда

command-switch-space = Пераключыць прастору…
command-search-ask = Шукаць або спытаць…
command-new-tab-placeholder = Шукайце, увядзіце URL або выберыце Тэрмінал…
command-placeholder = Увядзіце URL, шукайце ўкладкі або > для каманд…
command-composer-placeholder = Увядзіце / для каманд або @ для медыя
command-send = Адправіць (Enter)
command-terminal = Тэрмінал
command-open-terminal = Адкрыць у тэрмінале
command-stack = Стэк
command-tabs = { $count ->
    [one] 1 укладка
   *[other] { $count } укладак
}
command-prompt = Промпт
command-new-tab = Новая ўкладка
command-search = Пошук
command-open-value = Адкрыць «{ $value }»
command-search-value = Шукаць «{ $value }»

schema-appearance = Выгляд
schema-general = Агульныя
schema-layout = Макет
schema-layout-detail = Акно, панэлі, бакавая панэль і контур фокуса.
schema-agent = Агент
schema-agent-detail = Паводзіны агента і дазволы на інструменты.
schema-shortcuts = Спалучэнні клавіш
schema-shortcuts-detail = Толькі прагляд. Каб змяніць прывязкі, рэдагуйце settings.ron непасрэдна.
schema-terminal = Тэрмінал
schema-browser = Браўзер
schema-mode = Рэжым
schema-mode-detail = Каляровая схема для вэб-старонак. «Прылада» выкарыстоўвае сістэмныя налады.
schema-device = Прылада
schema-light = Светлая
schema-dark = Цёмная
schema-language = Мова
schema-language-detail = Выкарыстоўвайце сістэмную, en-US, ja або любы тэг BCP 47 з адпаведным каталогам ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Аўтаабнаўленне
schema-auto-update-detail = Правяраць і ўсталёўваць абнаўленні пры запуску і штогадзіну.
schema-startup-url = Стартавы URL
schema-startup-url-detail = Калі пуста, адкрываецца промпт каманднай панэлі.
schema-search-engine = Пашукавік
schema-search-engine-detail = Выкарыстоўваецца для вэб-пошуку з Пачатку і каманднай панэлі.
schema-window = Акно
schema-pane = Панэль
schema-side-sheet = Бакавая панэль
schema-focus-ring = Контур фокуса
schema-run-placement = Дазволіць перавызначаць месца запуску
schema-run-placement-detail = Дазволіць агентам выбіраць рэжым панэлі запуску, кірунак і прывязку.
schema-leader = Лідар
schema-leader-detail = Прэфіксная клавіша для акордных спалучэнняў.
schema-chord-timeout = Тайм-аўт акорда
schema-chord-timeout-detail = Колькі мілісекунд дзейнічае прэфікс акорда.
schema-bindings = Прывязкі
schema-confirm-close = Пацвярджаць закрыццё
schema-confirm-close-detail = Пытаць перад закрыццём тэрмінала, у якім працуе працэс.
schema-default-theme = Прадвызначаная тэма
schema-default-theme-detail = Назва актыўнай тэмы са спіса тэм.
