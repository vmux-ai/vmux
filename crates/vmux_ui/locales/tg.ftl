common-open = Кушодан
common-close = Бастан
common-install = Насб кардан
common-uninstall = Насб бардоштан
common-update = Навсозӣ
common-retry = Такрор
common-refresh = Навсозӣ
common-remove = Хориҷ кардан
common-enable = Фаъол кардан
common-disable = Ғайрифаъол кардан
common-new = Нав
common-active = фаъол
common-running = кор мекунад
common-done = тамом
common-failed = Иҷро нашуд
common-installed = Насб шуд
common-items = { $count ->
    [one] { $count } банд
   *[other] { $count } банд
}
start-title = Оғоз
start-tagline = Як дастур. Ҳар чиз, тамом.

agents-title = Агентҳо
agents-search = Ҷустуҷӯи агентҳои ACP ва CLI…
agents-empty = Агентҳои мувофиқ ёфт нашуд
agents-empty-detail = Ном, муҳити иҷро ё ACP/CLI-ро санҷед.
agents-install-failed = Насб нашуд
agents-updating = Навсозӣ…
agents-retrying = Такрор…
agents-preparing = Омодасозӣ…

extensions-title = Иловагиҳо
extensions-search = Ҷустуҷӯ дар насбшудаҳо ё Chrome Web Store…
extensions-relaunch = Барои татбиқ аз нав оғоз кунед
extensions-empty = Иловагиҳо насб нашудаанд
extensions-no-match = Иловагиҳои мувофиқ ёфт нашуд
extensions-empty-detail = Дар Chrome Web Store боло ҷустуҷӯ кунед ва Return-ро пахш кунед.
extensions-no-match-detail = Номи дигар ё ID иловагиро санҷед.
extensions-on = Фаъол
extensions-off = Ғайрифаъол
extensions-enable-confirm = { $name }-ро фаъол кунед?
extensions-enable-permissions = { $name }-ро фаъол кунед ва иҷозат диҳед:

lsp-title = Серверҳои забон
lsp-search = Ҷустуҷӯи серверҳои забон, линтерҳо, форматкунандаҳо…
lsp-loading = Каталог бор мешавад…
lsp-empty = Серверҳои забони мувофиқ ёфт нашуд
lsp-empty-detail = Забон, линтер ё форматкунандаи дигарро санҷед.
lsp-needs = { $tool } лозим аст
lsp-status-available = Дастрас
lsp-status-on-path = Дар PATH
lsp-status-installing = Насб мешавад…
lsp-status-installed = Насб шуд
lsp-status-outdated = Навсозӣ мавҷуд аст
lsp-status-running = Кор мекунад
lsp-status-failed = Иҷро нашуд

spaces-title = Фазоҳо
spaces-new-placeholder = Номи фазои нав
spaces-empty = Фазоҳо вуҷуд надоранд
spaces-default-name = Фазо { $number }
spaces-tabs = { $count ->
    [one] 1 варақа
   *[other] { $count } варақаҳо
}
spaces-delete = Фазоро ҳазф кунед

team-title = Гурӯҳ
team-just-you = Танҳо шумо дар ин фазо
team-agents = { $count ->
    [one] Шумо ва 1 агент
   *[other] Шумо ва { $count } агентҳо
}
team-empty = Ҳанӯз касе инҷо нест
team-you = Шумо
team-agent = Агент

services-title = Хидматҳои фонӣ
services-processes = { $count ->
    [one] 1 раванд
   *[other] { $count } равандҳо
}
services-kill-all = Ҳамаро қатъ кунед
services-not-running = Хидмат кор намекунад
services-start-with = Оғоз бо:
services-empty = Равандҳои фаъол нестанд
services-filter = Равандҳоро филтр кунед…
services-no-match = Равандҳои мувофиқ ёфт нашуд
services-connected = Пайваст
services-disconnected = Қатъ шуд
services-attached = замима шуд
services-kill = Қатъ
services-memory = Хотира
services-size = Андоза
services-shell = Shell

error-title = Хато

history-search = Ҷустуҷӯи таърих
history-clear-all = Ҳамаро тоза кунед
history-clear-confirm = Тамоми таърихро тоза кунед?
history-clear-warning = Ин амалро баргардонидан мумкин нест.
history-cancel = Бекор кунед
history-today = Имрӯз
history-yesterday = Дирӯз
history-days-ago = { $count } рӯз пеш
history-day-offset = Рӯз -{ $count }

settings-title = Танзимот
settings-loading = Танзимот бор мешавад…
settings-stored = Дар ~/.vmux/settings.ron нигоҳ дошта мешавад
settings-other = Дигар
settings-software-update = Навсозии нармафзор
settings-check-updates = Санҷиши навсозиҳо
settings-check-updates-hint = Ҳангоми оғоз ва ҳар соат дар сурати фаъол будани навсозии автоматӣ худкор санҷида мешавад.
settings-update-unavailable = Дастнорас
settings-update-unavailable-hint = Навсозкунанда дар ин сохт мавҷуд нест.
settings-update-checking = Санҷиш…
settings-update-checking-hint = Дар ҷустуҷӯи навсозиҳо…
settings-update-check-again = Боз санҷед
settings-update-current = Vmux навтарин аст.
settings-update-downloading = Зеркашӣ…
settings-update-downloading-hint = Vmux { $version } зеркашӣ мешавад…
settings-update-installing = Насб мешавад…
settings-update-installing-hint = Vmux { $version } насб мешавад…
settings-update-ready = Навсозӣ омодааст
settings-update-ready-hint = Vmux { $version } омодааст. Барои татбиқ аз нав оғоз кунед.
settings-update-try-again = Боз кӯшиш кунед
settings-update-failed = Санҷиши навсозиҳо имконпазир нест.
settings-item = Банд
settings-item-number = Банд { $number }
settings-press-key = Тугмаро пахш кунед…
settings-saved = Захира шуд
settings-record-key = Барои сабти комбои нави тугмаҳо клик кунед

tray-open-window = Тиреза кушодан
tray-close-window = Тиреза бастан
tray-pause-recording = Сабтро таваққуф кунед
tray-resume-recording = Сабтро идома диҳед
tray-finish-recording = Сабтро ба итмом расонед
tray-quit = Vmux-ро бастан

composer-attach-files = Файлҳо замима кунед (/upload)
composer-remove-attachment = Замимаро хориҷ кунед

layout-back = Ақиб
layout-forward = Пеш
layout-reload = Аз нав бор кунед
layout-bookmark-page = Ин саҳифаро хатгузор кунед
layout-remove-bookmark = Хатгузорро хориҷ кунед
layout-pin-page = Ин саҳифаро маҳкам кунед
layout-unpin-page = Ин саҳифаро озод кунед
layout-manage-extensions = Идораи иловагиҳо
layout-new-stack = Стаки нав
layout-close-tab = Варақаро бастан
layout-bookmark = Хатгузор
layout-pin = Маҳкам кардан
layout-new-tab = Варақаи нав
layout-team = Гурӯҳ

command-switch-space = Фазоро иваз кунед…
command-search-ask = Ҷустуҷӯ ё пурсед…
command-new-tab-placeholder = Ҷустуҷӯ кунед ё URL ворид кунед, ё Terminal-ро интихоб кунед…
command-placeholder = URL ворид кунед, варақаҳоро ҷустуҷӯ кунед, ё > барои фармонҳо…
command-composer-placeholder = / барои фармонҳо ё @ барои медиа ворид кунед
command-send = Ирсол (Enter)
command-terminal = Terminal
command-open-terminal = Дар Terminal кушодан
command-stack = Стак
command-tabs = { $count ->
    [one] 1 варақа
   *[other] { $count } варақаҳо
}
command-prompt = Дастур
command-new-tab = Варақаи нав
command-search = Ҷустуҷӯ
command-open-value = "{ $value }"-ро кушодан
command-search-value = "{ $value }"-ро ҷустуҷӯ кунед

schema-appearance = Намуд
schema-general = Умумӣ
schema-layout = Тартиббандӣ
schema-layout-detail = Тиреза, панелҳо, навбар ва доираи фокус.
schema-agent = Агент
schema-agent-detail = Рафтори агент ва иҷозатҳои асбобҳо.
schema-shortcuts = Миёнбурҳо
schema-shortcuts-detail = Намоиши танҳо хонданӣ. Барои тағйири пайвандиҳо settings.ron-ро бевосита таҳрир кунед.
schema-terminal = Terminal
schema-browser = Браузер
schema-mode = Ҳолат
schema-mode-detail = Тарҳи ранги саҳифаҳои веб. Дастгоҳ системаи шуморо пайравӣ мекунад.
schema-device = Дастгоҳ
schema-light = Равшан
schema-dark = Торик
schema-language = Забон
schema-language-detail = Системаро, en-US, ja ё ҳар гуна тасмаи BCP 47 бо каталоги мувофиқи ~/.vmux/locales/<tag>.ftl истифода баред.
schema-auto-update = Навсозии автоматӣ
schema-auto-update-detail = Ҳангоми оғоз ва ҳар соат навсозиҳоро санҷед ва насб кунед.
schema-startup-url = URL-и оғоз
schema-startup-url-detail = Холӣ дастурдиҳандаи навбари фармонро мекушояд.
schema-search-engine = Муҳаррики ҷустуҷӯ
schema-search-engine-detail = Барои ҷустуҷӯи веб аз Оғоз ва навбари фармон истифода мешавад.
schema-window = Тиреза
schema-pane = Панел
schema-side-sheet = Варақаи паҳлӯ
schema-focus-ring = Доираи фокус
schema-run-placement = Иҷозати тағйири ҷойгузории иҷро
schema-run-placement-detail = Ба агентҳо иҷозат диҳед, ки ҳолат, самт ва лангари панели иҷроро интихоб кунанд.
schema-leader = Пешво
schema-leader-detail = Тугмаи пешванд барои миёнбурҳои хорд.
schema-chord-timeout = Вақтбарории хорд
schema-chord-timeout-detail = Миллисония то тамом шудани мӯҳлати пешванди хорд.
schema-bindings = Пайвандиҳо
schema-confirm-close = Тасдиқи бастан
schema-confirm-close-detail = Пеш аз бастани терминал бо раванди кориёбӣ огоҳӣ диҳед.
schema-default-theme = Тарҳи пешфарз
schema-default-theme-detail = Номи тарҳи фаъол аз рӯйхати тарҳҳо.
