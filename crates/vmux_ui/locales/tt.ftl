common-open = Ачу
common-close = Ябу
common-install = Урнаштыру
common-uninstall = Бетерү
common-update = Яңарту
common-retry = Кабатлау
common-refresh = Яңарту
common-remove = Алып кую
common-enable = Кушу
common-disable = Сүндерү
common-new = Яңа
common-active = актив
common-running = эшли
common-done = әзер
common-failed = Уңышсыз
common-installed = Урнаштырылган
common-items = { $count ->
    [one] { $count } элемент
   *[other] { $count } элемент
}
start-title = Башлау
start-tagline = Бер prompt. Теләсә нәрсә — әзер.

agents-title = Агентлар
agents-search = ACP һәм CLI агентларын эзләү…
agents-empty = Туры килгән агентлар юк
agents-empty-detail = Исем, башкару мохите яки ACP/CLI буенча эзләгез.
agents-install-failed = Урнаштырып булмады
agents-updating = Яңартыла…
agents-retrying = Кабатлана…
agents-preparing = Әзерләнә…

extensions-title = Өстәмәләр
extensions-search = Урнаштырылганнардан яки Chrome Web Store’дан эзләү…
extensions-relaunch = Куллану өчен яңадан җибәрегез
extensions-empty = Урнаштырылган өстәмәләр юк
extensions-no-match = Туры килгән өстәмәләр юк
extensions-empty-detail = Өстәге Chrome Web Store эзләвенә языгыз һәм Return басыгыз.
extensions-no-match-detail = Башка исем яки өстәмә ID’сын сынагыз.
extensions-on = Кабызылган
extensions-off = Сүндерелгән
extensions-enable-confirm = { $name } өстәмәсен кушаргамы?
extensions-enable-permissions = { $name } өстәмәсен кушу һәм рөхсәт итү:

lsp-title = Тел серверлары
lsp-search = Тел серверларын, линтерларны, форматлагычларны эзләү…
lsp-loading = Каталог йөкләнә…
lsp-empty = Туры килгән тел серверлары юк
lsp-empty-detail = Башка тел, линтер яки форматлагычны сынагыз.
lsp-needs = { $tool } кирәк
lsp-status-available = Бар
lsp-status-on-path = PATH’та
lsp-status-installing = Урнаштырыла…
lsp-status-installed = Урнаштырылган
lsp-status-outdated = Яңарту бар
lsp-status-running = Эшли
lsp-status-failed = Уңышсыз

spaces-title = Мохитләр
spaces-new-placeholder = Яңа мохит исеме
spaces-empty = Мохитләр юк
spaces-default-name = Мохит { $number }
spaces-tabs = { $count ->
    [one] 1 кыстыргыч
   *[other] { $count } кыстыргыч
}
spaces-delete = Мохитне бетерү

team-title = Команда
team-just-you = Бу мохиттә әлегә сез генә
team-agents = { $count ->
    [one] Сез һәм 1 агент
   *[other] Сез һәм { $count } агент
}
team-empty = Монда әлегә беркем юк
team-you = Сез
team-agent = Агент

services-title = Фон хезмәтләре
services-processes = { $count ->
    [one] 1 процесс
   *[other] { $count } процесс
}
services-kill-all = Барысын мәҗбүри туктату
services-not-running = Хезмәт эшләми
services-start-with = Болай башлау:
services-empty = Актив процесслар юк
services-filter = Процессларны фильтрлау…
services-no-match = Туры килгән процесслар юк
services-connected = Тоташкан
services-disconnected = Тоташмаган
services-attached = бәйләнгән
services-kill = Мәҗбүри туктату
services-memory = Хәтер
services-size = Үлчәм
services-shell = Shell

error-title = Хата

history-search = Тарихтан эзләү
history-clear-all = Барысын чистарту
history-clear-confirm = Бөтен тарихны чистартыргамы?
history-clear-warning = Моны кире кайтарып булмый.
history-cancel = Баш тарту
history-today = Бүген
history-yesterday = Кичә
history-days-ago = { $count } көн элек
history-day-offset = Көн -{ $count }

settings-title = Көйләүләр
settings-loading = Көйләүләр йөкләнә…
settings-stored = ~/.vmux/settings.ron эчендә саклана
settings-other = Башка
settings-software-update = Программаны яңарту
settings-check-updates = Яңартуларны тикшерү
settings-check-updates-hint = Автояңарту кабызылган булса, эшли башлаганда һәм һәр сәгать саен автоматик тикшерә.
settings-update-unavailable = Мөмкин түгел
settings-update-unavailable-hint = Бу җыелмада яңарткыч юк.
settings-update-checking = Тикшерелә…
settings-update-checking-hint = Яңартулар тикшерелә…
settings-update-check-again = Кабат тикшерү
settings-update-current = Vmux актуаль.
settings-update-downloading = Йөкләнә…
settings-update-downloading-hint = Vmux { $version } йөкләнә…
settings-update-installing = Урнаштырыла…
settings-update-installing-hint = Vmux { $version } урнаштырыла…
settings-update-ready = Яңарту әзер
settings-update-ready-hint = Vmux { $version } әзер. Куллану өчен яңадан җибәрегез.
settings-update-try-again = Кабат сынау
settings-update-failed = Яңартуларны тикшереп булмады.
settings-item = Элемент
settings-item-number = Элемент { $number }
settings-press-key = Клавишага басыгыз…
settings-saved = Сакланды
settings-record-key = Яңа клавиша комбинациясен язу өчен басыгыз

tray-open-window = Тәрәзә ачу
tray-close-window = Тәрәзәне ябу
tray-pause-recording = Язуны туктатып тору
tray-resume-recording = Язуны дәвам итү
tray-finish-recording = Язуны тәмамлау
tray-quit = Vmux’тан чыгу

composer-attach-files = Файллар беркетү (/upload)
composer-remove-attachment = Беркетмәне алып кую

layout-back = Артка
layout-forward = Алга
layout-reload = Кабат йөкләү
layout-bookmark-page = Бу битне кыстыргычларга өстәү
layout-remove-bookmark = Кыстыргычны алып кую
layout-pin-page = Бу битне беркетү
layout-unpin-page = Бу битне ычкындыру
layout-manage-extensions = Өстәмәләр белән идарә итү
layout-new-stack = Яңа стек
layout-close-tab = Кыстыргычны ябу
layout-bookmark = Кыстыргычка өстәү
layout-pin = Беркетү
layout-new-tab = Яңа кыстыргыч
layout-team = Команда

command-switch-space = Мохитне алыштыру…
command-search-ask = Эзләү яки сорау…
command-new-tab-placeholder = Эзләгез, URL языгыз яки Терминалны сайлагыз…
command-placeholder = URL языгыз, кыстыргычлардан эзләгез яки командалар өчен > кертегез…
command-composer-placeholder = Командалар өчен / яки медиа өчен @ кертегез
command-send = Җибәрү (Enter)
command-terminal = Терминал
command-open-terminal = Терминалда ачу
command-stack = Стек
command-tabs = { $count ->
    [one] 1 кыстыргыч
   *[other] { $count } кыстыргыч
}
command-prompt = Prompt
command-new-tab = Яңа кыстыргыч
command-search = Эзләү
command-open-value = “{ $value }” ачу
command-search-value = “{ $value }” эзләү

schema-appearance = Күренеш
schema-general = Гомуми
schema-layout = Макет
schema-layout-detail = Тәрәзә, панельләр, ян такта һәм фокус кысасы.
schema-agent = Агент
schema-agent-detail = Агент тәртибе һәм коралларга рөхсәтләр.
schema-shortcuts = Тиз клавишалар
schema-shortcuts-detail = Уку өчен генә. Бәйләүләрне үзгәртү өчен settings.ron файлын турыдан-туры төзәтегез.
schema-terminal = Терминал
schema-browser = Браузер
schema-mode = Режим
schema-mode-detail = Веб-битләр өчен төс схемасы. Җайланма режимы система көйләвен куллана.
schema-device = Җайланма
schema-light = Якты
schema-dark = Караңгы
schema-language = Тел
schema-language-detail = Системаны, en-US, ja яки туры килгән ~/.vmux/locales/<tag>.ftl каталогы булган теләсә кайсы BCP 47 тегын кулланыгыз.
schema-auto-update = Автояңарту
schema-auto-update-detail = Эшли башлаганда һәм һәр сәгать саен яңартуларны тикшерү һәм урнаштыру.
schema-startup-url = Башлангыч URL
schema-startup-url-detail = Буш булса, команда юлы prompt’ын ача.
schema-search-engine = Эзләү системасы
schema-search-engine-detail = Башлау битеннән һәм команда юлыннан веб-эзләү өчен кулланыла.
schema-window = Тәрәзә
schema-pane = Панель
schema-side-sheet = Ян бит
schema-focus-ring = Фокус кысасы
schema-run-placement = Эшләтү урынын үзгәртүне рөхсәт итү
schema-run-placement-detail = Агентларга эшләтү панеле режимын, юнәлешен һәм терәк ноктасын сайларга рөхсәт итү.
schema-leader = Лидер
schema-leader-detail = Chord тиз клавишалары өчен префикс клавиша.
schema-chord-timeout = Chord көтү вакыты
schema-chord-timeout-detail = Chord префиксы гамәлдән чыкканчы миллисекундлар.
schema-bindings = Бәйләүләр
schema-confirm-close = Ябуны раслау
schema-confirm-close-detail = Эшләп торган процессы булган терминалны япканчы сорау.
schema-default-theme = Килешенгән тема
schema-default-theme-detail = Темалар исемлегендәге актив тема исеме.
