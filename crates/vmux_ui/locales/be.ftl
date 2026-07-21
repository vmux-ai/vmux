common-open = Адкрыты
common-close = Блізка
common-install = Усталяваць
common-uninstall = Выдаліць
common-update = Абнаўленне
common-retry = Паўтарыць
common-refresh = Абнавіць
common-remove = Выдаліць
common-enable = Уключыць
common-disable = Адключыць
common-new = Новы
common-active = актыўны
common-running = бег
common-done = зроблена
common-failed = Не атрымалася
common-installed = Усталяваны
common-items = { $count ->
    [one] { $count } элемент
   *[other] { $count } элементаў
}
start-title = Пачаць
start-tagline = Адна падказка. Усё, зроблена.

agents-title = Агенты
agents-search = Пошук агентаў ACP і CLI…
agents-empty = Няма адпаведных агентаў
agents-empty-detail = Паспрабуйце назву, час выканання або ACP/CLI.
agents-install-failed = Збой усталявання
agents-updating = Ідзе абнаўленне…
agents-retrying = Паўтор…
agents-preparing = Падрыхтоўка...

extensions-title = Пашырэнні
extensions-search = Пошук усталяваны або Chrome Web Store…
extensions-relaunch = Каб прымяніць, перазапусціце
extensions-empty = Пашырэнні не ўсталяваны
extensions-no-match = Няма адпаведных пашырэнняў
extensions-empty-detail = Знайдзіце Chrome Web Store вышэй і націсніце Return.
extensions-no-match-detail = Паспрабуйце іншае імя або ідэнтыфікатар пашырэння.
extensions-on = Укл
extensions-off = Выкл
extensions-enable-confirm = Уключыць { $name }?
extensions-enable-permissions = Уключыць { $name } і дазволіць:

lsp-title = Моўныя серверы
lsp-search = Пошук моўных сервераў, лінтэраў, фарматаў…
lsp-loading = Загрузка каталога…
lsp-empty = Няма адпаведных моўных сервераў
lsp-empty-detail = Паспрабуйце іншую мову, лінтар або фарматавальнік.
lsp-needs = патрабуе { $tool }
lsp-status-available = Даступны
lsp-status-on-path = На PATH
lsp-status-installing = Усталяванне…
lsp-status-installed = Усталяваны
lsp-status-outdated = Абнаўленне даступна
lsp-status-running = Бег
lsp-status-failed = Не атрымалася

spaces-title = Прабелы
spaces-new-placeholder = Новая назва прасторы
spaces-empty = Няма прабелаў
spaces-default-name = Прастора { $number }
spaces-tabs = { $count ->
    [one] 1 таб
   *[other] { $count } укладак
}
spaces-delete = Выдаліць месца

team-title = Каманда
team-just-you = Толькі вы ў гэтай прасторы
team-agents = { $count ->
    [one] Вы і 1 агент
   *[other] Вы і { $count } агенты
}
team-empty = Тут яшчэ нікога
team-you = Вы
team-agent = Агент

services-title = Фонавыя службы
services-processes = { $count ->
    [one] 1 працэс
   *[other] { $count } працэсаў
}
services-kill-all = Забіць усіх
services-not-running = Служба не працуе
services-start-with = Пачаць з:
services-empty = Няма актыўных працэсаў
services-filter = Працэсы фільтрацыі…
services-no-match = Няма адпаведных працэсаў
services-connected = Падключана
services-disconnected = Адключана
services-attached = прыкладаецца
services-kill = Забіць
services-memory = Памяць
services-size = Памер
services-shell = Ракавінка

error-title = Памылка

history-search = Гісторыя пошуку
history-clear-all = Ачысціць усё
history-clear-confirm = Ачысціць усю гісторыю?
history-clear-warning = Гэта нельга адмяніць.
history-cancel = Адмяніць
history-today = сёння
history-yesterday = Учора
history-days-ago = { $count } дзён таму
history-day-offset = Дзень -{ $count }

settings-title = Налады
settings-loading = Загрузка налад...
settings-stored = Захоўваецца ў ~/.vmux/settings.ron
settings-other = Іншае
settings-software-update = Абнаўленне праграмнага забеспячэння
settings-check-updates = Праверце наяўнасць абнаўленняў
settings-check-updates-hint = Правяраецца аўтаматычна пры запуску і кожную гадзіну, калі ўключана аўтаматычнае абнаўленне.
settings-update-unavailable = Недаступны
settings-update-unavailable-hint = Праграма абнаўлення не ўваходзіць у гэтую зборку.
settings-update-checking = Ідзе праверка...
settings-update-checking-hint = Праверка абнаўленняў…
settings-update-check-again = Праверце яшчэ раз
settings-update-current = Vmux абноўлены.
settings-update-downloading = Спампоўка...
settings-update-downloading-hint = Ідзе спампоўка Vmux { $version }…
settings-update-installing = Усталяванне…
settings-update-installing-hint = Усталяванне Vmux { $version }…
settings-update-ready = Абнаўленне гатова
settings-update-ready-hint = Vmux { $version } гатовы. Перазапусціце, каб прымяніць яго.
settings-update-try-again = Паспрабуйце яшчэ раз
settings-update-failed = Немагчыма праверыць наяўнасць абнаўленняў.
settings-item = Пункт
settings-item-number = Элемент { $number }
settings-press-key = Націсніце клавішу...
settings-saved = Захавана
settings-record-key = Націсніце, каб запісаць новую камбінацыю клавіш

tray-open-window = Адкрытае акно
tray-close-window = Зачыніць акно
tray-pause-recording = Прыпыніць запіс
tray-resume-recording = Аднавіць запіс
tray-finish-recording = Скончыць запіс
tray-quit = Выйсці з Vmux

composer-attach-files = Далучыць файлы (/upload)
composer-remove-attachment = Выдаліць укладанне

layout-back = Назад
layout-forward = Наперад
layout-reload = Перазагрузіць
layout-bookmark-page = Дадаць гэтую старонку ў закладкі
layout-remove-bookmark = Выдаліць закладку
layout-pin-page = Замацаваць гэту старонку
layout-unpin-page = Адмацаваць гэту старонку
layout-manage-extensions = Кіраванне пашырэннямі
layout-new-stack = Новы стэк
layout-close-tab = Закрыць укладку
layout-bookmark = Закладка
layout-pin = Pin
layout-new-tab = Новая ўкладка
layout-team = Каманда

command-switch-space = Пераключыць месца…
command-search-ask = Шукайце або пытайцеся…
command-new-tab-placeholder = Знайдзіце або ўвядзіце URL або абярыце Тэрмінал…
command-placeholder = Увядзіце URL, шукайце ўкладкі або > для каманд…
command-composer-placeholder = Увядзіце / для каманд або @ для мультымедыя
command-send = Адправіць (Enter)
command-terminal = Тэрмінал
command-open-terminal = Адкрыйце ў тэрмінале
command-stack = Стэк
command-tabs = { $count ->
    [one] 1 таб
   *[other] { $count } укладак
}
command-prompt = Падкажыце
command-new-tab = Новая ўкладка
command-search = Пошук
command-open-value = Адкрыць «{ $value }»
command-search-value = Пошук «{ $value }»

schema-appearance = Знешні выгляд
schema-general = Генерал
schema-layout = Макет
schema-layout-detail = Акно, панэлі, бакавая панэль і кольца факусіроўкі.
schema-agent = Агент
schema-agent-detail = Паводзіны агента і дазволы інструмента.
schema-shortcuts = Ярлыкі
schema-shortcuts-detail = Прагляд толькі для чытання. Адрэдагуйце settings.ron непасрэдна, каб змяніць прывязкі.
schema-terminal = Тэрмінал
schema-browser = Браўзэр
schema-mode = Рэжым
schema-mode-detail = Каляровая схема для вэб-старонак. Прылада сочыць за вашай сістэмай.
schema-device = прылада
schema-light = Святло
schema-dark = Цёмны
schema-language = мова
schema-language-detail = Выкарыстоўвайце system, en-US, ja або любы тэг BCP 47 з адпаведным каталогам ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Аўтаматычнае абнаўленне
schema-auto-update-detail = Правярайце і ўсталёўвайце абнаўленні пры запуску і кожную гадзіну.
schema-startup-url = Запуск URL
schema-startup-url-detail = Пусты адкрывае камандны радок.
schema-search-engine = Пошукавая сістэма
schema-search-engine-detail = Выкарыстоўваецца для вэб-пошуку з «Пуску» і панэлі каманд.
schema-window = Акно
schema-pane = Панэль
schema-side-sheet = Бакавы ліст
schema-focus-ring = Кольца факусіроўкі
schema-run-placement = Дазволіць перавызначэнне размяшчэння запуску
schema-run-placement-detail = Дазвольце агентам выбіраць рэжым панэлі запуску, кірунак і прывязку.
schema-leader = Лідэр
schema-leader-detail = Клавіша-прэфікс для спалучэнняў акордаў.
schema-chord-timeout = Тайм-аўт акорда
schema-chord-timeout-detail = Мілісекунды да заканчэння тэрміну дзеяння прэфікса акорда.
schema-bindings = Пераплёты
schema-confirm-close = Пацвердзіце закрыццё
schema-confirm-close-detail = Запрасіць перад закрыццём тэрмінала з запушчаным працэсам.
schema-default-theme = Тэма па змаўчанні
schema-default-theme-detail = Назва актыўнай тэмы са спісу тэм.
