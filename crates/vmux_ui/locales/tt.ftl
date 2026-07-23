locale-name = татарча
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

tools-title = Кораллар
tools-search = Пакетлар, агентлар, MCP, тел кораллары һәм көйләү файлларын эзләү…
tools-open = Коралларны ачу
tools-fold = Коралларны җыю
tools-unfold = Коралларны җәю
tools-scanning = Җирле кораллар тикшерелә…
tools-no-installed = Урнаштырылган кораллар юк
tools-empty = Туры килгән кораллар юк
tools-empty-detail = Пакет урнаштырыгыз яки Stow рәвешендәге көйләү файллары пакетын өстәгез.
tools-apply = Куллану
tools-homebrew = Homebrew
tools-homebrew-sync = Урнаштырылган формулалар һәм кушымталар автоматик рәвештә синхронлаша.
tools-open-brewfile = Brewfile ачу
tools-managed = идарә ителә
tools-provider-homebrew-formulae = Homebrew формулалары
tools-provider-homebrew-casks = Homebrew кушымталары
tools-provider-npm = npm пакетлары
tools-provider-acp-agents = ACP агентлары
tools-provider-language-tools = Тел кораллары
tools-provider-mcp-servers = MCP серверлары
tools-provider-dotfiles = Көйләү файллары
tools-status-available = Кулланырга мөмкин
tools-status-missing = Юк
tools-status-conflict = Каршылык
tools-forget = Онытырга
tools-manage = Идарә итәргә
tools-link = Бәйләргә
tools-unlink = Бәйләнешне өзәргә
tools-import = Импортларга
tools-update-count = { $count ->
    [one] 1 яңарту
   *[other] { $count } яңарту
}
tools-conflict-count = { $count ->
    [one] 1 каршылык
   *[other] { $count } каршылык
}
tools-result-applied = Кораллар кулланылды
tools-result-imported = Кораллар импортланды
tools-result-installed = { $name } урнаштырылды
tools-result-updated = { $name } яңартылды
tools-result-uninstalled = { $name } бетерелде
tools-result-forgotten = { $name } онытылды
tools-result-managed = { $name } хәзер идарә ителә
tools-result-linked = { $name } бәйләнде
tools-result-unlinked = { $name } бәйләнеше өзелде
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Git белән көйләүләрне, коралларны, нокталарны һәм Белемне синхронлагыз.
vault-sync = Синхронизация
vault-create = Ярат
vault-connect = Бәйләнегез
vault-private = Шәхси саклагыч
vault-public-warning = Иҗтимагый складлар сезнең Белемегезне һәм конфигурациягезне фаш итәләр.
vault-choose-repository = Резервуарны сайлагыз ...
vault-empty = буш
vault-clean = Бүгенге көнгә кадәр
vault-not-connected = Бәйләнмәгән
vault-change-count = Esзгәрешләр: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

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

settings-empty = (буш)
settings-none = (юк)

schema-system = Система
schema-editor = Мөхәррир
schema-recording = Яздыру
schema-radius = Радиус
schema-padding = Эчке чигенеш
schema-gap = Ара
schema-width = Киңлек
schema-color = Төс
schema-red = Кызыл
schema-green = Яшел
schema-blue = Зәңгәр
schema-follow-files = Файлларны күзәтү
schema-tidy-files = Файлларны җыештыру
schema-tidy-files-max = Файл җыештыру чиге
schema-tidy-files-auto = Файлларны автоматик җыештыру
schema-app-providers = Кушымта провайдерлары
schema-provider = Провайдер
schema-kind = Төр
schema-models = Модельләр
schema-acp = ACP агентлары
schema-id = ID
schema-name = Исем
schema-command = Команда
schema-arguments = Аргументлар
schema-environment = Мохит
schema-working-directory = Эш каталогы
schema-shell = Shell
schema-font-family = Шрифт гаиләсе
schema-startup-directory = Башлангыч каталог
schema-themes = Темалар
schema-color-scheme = Төс схемасы
schema-font-size = Шрифт зурлыгы
schema-line-height = Юл биеклеге
schema-cursor-style = Курсор стиле
schema-cursor-blink = Курсор җемелдәве
schema-custom-themes = Үз темалары
schema-foreground = Алгы план
schema-background = Фон
schema-cursor = Курсор
schema-ansi-colors = ANSI төсләре
schema-keymap = Клавишалар картасы
schema-explorer = Күзәткеч
schema-visible = Күренә
schema-language-servers = Тел серверлары
schema-servers = Серверлар
schema-language-id = Тел ID
schema-root-markers = Тамыр маркерлары
schema-output-directory = Чыгыш каталогы

menu-scene = Сәхнә
menu-layout = Урнашу
menu-terminal = Терминал
menu-browser = Браузер
menu-service = Хезмәт
menu-bookmark = Кыстыргыч
menu-edit = Үзгәртү

layout-knowledge = Белем
layout-open-knowledge = Белемне ачу
layout-open-welcome-knowledge = «Белемгә рәхим итегез»не ачу
layout-open-path = { $path } ачу
layout-fold-knowledge = Белемне төрү
layout-unfold-knowledge = Белемне җәю
layout-bookmarks = Кыстыргычлар
layout-new-folder = Яңа папка
layout-add-to-bookmarks = Кыстыргычларга өстәү
layout-move-to-bookmarks = Кыстыргычларга күчерү
layout-stack-number = Стек { $number }
layout-fold-stack = Стекны төрү
layout-unfold-stack = Стекны җәю
layout-close-stack = Стекны ябу
layout-bookmark-in = { $folder } эченә кыстыргычлау

common-cancel = Баш тарту
common-delete = Бетерү
common-save = Саклау
common-rename = Исемен үзгәртү
common-expand = Җәю
common-collapse = Җыю
common-loading = Йөкләнә…
common-error = Хата
common-output = Чыгыш
common-pending = Көтелә
common-current = агымдагы
common-stop = Туктату
services-command = Vmux хезмәте
services-uptime-seconds = { $seconds }с
services-uptime-minutes = { $minutes }мин { $seconds }с
services-uptime-hours = { $hours }сәг { $minutes }мин
services-uptime-days = { $days }к { $hours }сәг

error-page-failed-load = Бит йөкләнмәде
error-page-not-found = Бит табылмады
error-unknown-host = Билгесез Vmux кушымта хосты: { $host }

history-title = Тарих

command-new-app-chat = Яңа { $provider }/{ $model } чаты (кушымта)
command-interactive-mode-user = Сцена > Интерактив режим > Кулланучы
command-interactive-mode-player = Сцена > Интерактив режим > Уенчы
command-minimize-window = Макет > Тәрәзә > Кечерәйтү
command-toggle-layout = Макет > Макет > Макетны күчерү
command-close-tab = Макет > Өстәмә бит > Өстәмә битне ябу
command-new-task = Макет > Өстәмә бит > Яңа бирем…
command-next-tab = Макет > Өстәмә бит > Киләсе өстәмә бит
command-prev-tab = Макет > Өстәмә бит > Алдагы өстәмә бит
command-rename-tab = Макет > Өстәмә бит > Өстәмә битнең исемен үзгәртү
command-tab-select-1 = Макет > Өстәмә бит > 1 нче өстәмә битне сайлау
command-tab-select-2 = Макет > Өстәмә бит > 2 нче өстәмә битне сайлау
command-tab-select-3 = Макет > Өстәмә бит > 3 нче өстәмә битне сайлау
command-tab-select-4 = Макет > Өстәмә бит > 4 нче өстәмә битне сайлау
command-tab-select-5 = Макет > Өстәмә бит > 5 нче өстәмә битне сайлау
command-tab-select-6 = Макет > Өстәмә бит > 6 нчы өстәмә битне сайлау
command-tab-select-7 = Макет > Өстәмә бит > 7 нче өстәмә битне сайлау
command-tab-select-8 = Макет > Өстәмә бит > 8 нче өстәмә битне сайлау
command-tab-select-last = Макет > Өстәмә бит > Соңгы өстәмә битне сайлау
command-close-pane = Макет > Панель > Панельне ябу
command-select-pane-left = Макет > Панель > Сул панельне сайлау
command-select-pane-right = Макет > Панель > Уң панельне сайлау
command-select-pane-up = Макет > Панель > Өске панельне сайлау
command-select-pane-down = Макет > Панель > Аскы панельне сайлау
command-swap-pane-prev = Макет > Панель > Панельне алдагысы белән алыштыру
command-swap-pane-next = Макет > Панель > Панельне киләсе белән алыштыру
command-equalize-pane-size = Макет > Панель > Панель үлчәмнәрен тигезләү
command-resize-pane-left = Макет > Панель > Панельне сулга үзгәртү
command-resize-pane-right = Макет > Панель > Панельне уңга үзгәртү
command-resize-pane-up = Макет > Панель > Панельне өскә үзгәртү
command-resize-pane-down = Макет > Панель > Панельне аска үзгәртү
command-stack-close = Макет > Стек > Стекны ябу
command-stack-next = Макет > Стек > Киләсе стек
command-stack-previous = Макет > Стек > Алдагы стек
command-stack-reopen = Макет > Стек > Ябылган битне яңадан ачу
command-stack-swap-prev = Макет > Стек > Стекны сулга күчерү
command-stack-swap-next = Макет > Стек > Стекны уңга күчерү
command-space-open = Макет > Аралык > Аралыклар
command-terminal-close = Терминал > Терминалны ябу
command-terminal-next = Терминал > Киләсе терминал
command-terminal-prev = Терминал > Алдагы терминал
command-terminal-clear = Терминал > Терминалны чистарту
command-browser-prev-page = Браузер > Навигация > Артка
command-browser-next-page = Браузер > Навигация > Алга
command-browser-reload = Браузер > Навигация > Яңарту
command-browser-hard-reload = Браузер > Навигация > Көчләп яңарту
command-open-in-place = Браузер > Ачу > Монда ачу
command-open-in-new-stack = Браузер > Ачу > Яңа стекта ачу
command-open-in-pane-top = Браузер > Ачу > Өстәге панельдә ачу
command-open-in-pane-right = Браузер > Ачу > Уң панельдә ачу
command-open-in-pane-bottom = Браузер > Ачу > Астагы панельдә ачу
command-open-in-pane-left = Браузер > Ачу > Сул панельдә ачу
command-open-in-new-tab = Браузер > Ачу > Яңа өстәмә биттә ачу
command-open-in-new-space = Браузер > Ачу > Яңа аралыкта ачу
command-browser-zoom-in = Браузер > Күренеш > Зурайту
command-browser-zoom-out = Браузер > Күренеш > Кечерәйтү
command-browser-zoom-reset = Браузер > Күренеш > Чын үлчәм
command-browser-dev-tools = Браузер > Күренеш > Төзүче кораллары
command-browser-open-command-bar = Браузер > Такта > Боерыклар тактасы
command-browser-open-page-in-command-bar = Браузер > Такта > Битне төзәтү
command-browser-open-path-bar = Браузер > Такта > Юл навигаторы
command-browser-open-commands = Браузер > Такта > Боерыклар
command-browser-open-history = Браузер > Такта > Тарих
command-service-open = Хезмәт > Хезмәт күзәткечен ачу
command-bookmark-toggle-active = Кыстыргыч > Битне кыстыргычка өстәү
command-bookmark-pin-active = Кыстыргыч > Битне беркетү

layout-tab = Өстәмә бит
layout-no-stacks = Стеклар юк
layout-loading = Йөкләнә…
layout-no-markdown-files = Markdown файллары юк
layout-empty-folder = Буш папка
layout-worktree = эш агачы
layout-folder-name = Папка исеме
layout-no-pins-bookmarks = Беркетелгән битләр яки кыстыргычлар юк
layout-move-to = { $folder } папкасына күчерү
layout-bookmark-current-page = Агымдагы битне кыстыргычка өстәү
layout-rename-folder = Папка исемен үзгәртү
layout-remove-folder = Папканы бетерү
layout-update-downloading = Яңарту йөкләнә
layout-update-installing = Яңарту урнаштырыла…
layout-update-ready = Яңа версия бар
layout-restart-update = Яңарту өчен яңадан җибәрү

agent-preparing = Агент әзерләнә…
agent-send-all-queued = Чираттагы барлык сорауларны хәзер җибәрү (Esc)
agent-send = Җибәрү (Enter)
agent-ready = Әзер булгач яза аласыз.
agent-loading-older = Искерәк хәбәрләр йөкләнә…
agent-load-older = Искерәк хәбәрләрне йөкләү
agent-continued-from = { $source } чыганагыннан дәвам итте
agent-older-context-omitted = искерәк контекст төшереп калдырылды
agent-interrupted = өзелде
agent-allow-tool = { $tool } рөхсәт ителсенме?
agent-deny = Кире кагу
agent-allow-always = Һәрвакыт рөхсәт итү
agent-allow = Рөхсәт итү
agent-loading-sessions = Сессияләр йөкләнә…
agent-no-resumable-sessions = Дәвам итәрлек сессияләр табылмады
agent-no-matching-sessions = Туры килгән сессияләр юк
agent-no-matching-models = Туры килгән модельләр юк
agent-choice-help = ↑/↓ яки Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Репозиторий папкасын сайлау
agent-choose-repository-detail = Агент кулланырга тиешле җирле Git репозиторийны сайлагыз.
agent-choosing = Сайлана…
agent-choose-folder = Папка сайлау
agent-queued = чиратта
agent-attached = Беркетелгән:
agent-cancel-queued = Чираттагы сораудан баш тарту
agent-resume-queued = Чираттагы сорауларны дәвам итү
agent-clear-queue = Чиратны чистарту
agent-send-all-now = барысын хәзер җибәрү
agent-choose-option = Өстәге вариантны сайлагыз
agent-loading-media = Медиа йөкләнә…
agent-no-matching-media = Туры килгән медиа юк
agent-prompt-context = Сорау контексты
agent-details = Тулырак
agent-path = Юл
agent-tool = Корал
agent-server = Сервер
agent-bytes = { $count } байт
agent-worked-for = { $duration } эшләде
agent-worked-for-steps = { $count ->
    [one] { $duration } эшләде · 1 адым
   *[other] { $duration } эшләде · { $count } адым
}
agent-tool-guardian-review = Сакчы тикшерүе
agent-tool-read-files = Файлларны укыды
agent-tool-viewed-image = Рәсемне карады
agent-tool-used-browser = Браузер кулланды
agent-tool-searched-files = Файллардан эзләде
agent-tool-ran-commands = Боерыклар башкарды
agent-thinking = Уйлана
agent-subagent = Субагент
agent-prompt = Сорау
agent-thread = Җеп
agent-parent = Ата
agent-children = Балалар
agent-call = Чакыру
agent-raw-event = Чимал вакыйга
agent-plan = План
agent-tasks = { $count ->
    [one] 1 бирем
   *[other] { $count } бирем
}
agent-edited = Үзгәртелде
agent-reconnecting = Яңадан тоташа { $attempt }/{ $total }
agent-status-running = Эшли
agent-status-done = Әзер
agent-status-failed = Уңышсыз
agent-status-pending = Көтелә
agent-slash-attach-files = Файллар беркетү
agent-slash-resume-session = Элекке сессияне дәвам итү
agent-slash-select-model = Модель сайлау
agent-slash-continue-cli = Бу сессияне CLI эчендә дәвам итү
agent-session-just-now = әле генә
agent-session-minutes-ago = { $count }мин элек
agent-session-hours-ago = { $count }сәг элек
agent-session-days-ago = { $count }к элек
agent-working-working = Эшли
agent-working-thinking = Уйлана
agent-working-pondering = Уй йөртә
agent-working-noodling = Баш вата
agent-working-percolating = Өлгертә
agent-working-conjuring = Тудыра
agent-working-cooking = Пешерә
agent-working-brewing = Кайната
agent-working-musing = Уйланып тора
agent-working-ruminating = Уйлап-үлчи
agent-working-scheming = План кора
agent-working-synthesizing = Җыя
agent-working-tinkering = Чокына
agent-working-churning = Эшкәртә
agent-working-vibing = Дулкында
agent-working-simmering = Талгын гына кайный
agent-working-crafting = Эшләп чыгара
agent-working-divining = Төпченә
agent-working-mulling = Уйлап карый
agent-working-spelunking = Тирән казына

editor-toggle-explorer = Explorer күрсәтү/яшерү (Cmd+B)
editor-unsaved = сакланмаган
editor-rendered-markdown = Тере төзәтүле Markdown күренеше
editor-note = Искәрмә
editor-source-editor = Чыганак редакторы
editor-editor = Редактор
editor-git-diff = Git аермасы
editor-diff = Аерма
editor-tidy = Җыештыру
editor-always = Һәрвакыт
editor-unchanged-previews = { $count ->
    [one] ✦ 1 үзгәрмәгән карап чыгу
   *[other] ✦ { $count } үзгәрмәгән карап чыгу
}
editor-open-externally = Тышкы кушымтада ачу
editor-changed-line = Үзгәргән юл
editor-go-to-definition = Билгеләмәгә күчү
editor-find-references = Сылтамаларны табу
editor-references = { $count ->
    [one] 1 сылтама
   *[other] { $count } сылтама
}
editor-lsp-starting = { $server } эшли башлый…
editor-lsp-not-installed = { $server } — урнаштырылмаган
editor-explorer = Explorer
editor-open-editors = Ачык редакторлар
editor-outline = Структура
editor-new-file = Яңа файл
editor-new-folder = Яңа папка
editor-delete-confirm = “{ $name }” бетерелсенме? Моны кире кайтарып булмый.
editor-created-folder = { $name } папкасы булдырылды
editor-created-file = { $name } файлы булдырылды
editor-renamed-to = Исеме { $name } итеп үзгәртелде
editor-deleted = { $name } бетерелде
editor-failed-decode-image = Рәсемне декодлау уңышсыз
editor-preview-large-image = рәсем (карап чыгу өчен артык зур)
editor-preview-binary = бинар
editor-preview-file = файл

git-status-clean = чиста
git-status-modified = үзгәртелгән
git-status-staged = әзерләнгән
git-status-staged-modified = әзерләнгән*
git-status-untracked = күзәтелми
git-status-deleted = бетерелгән
git-status-conflict = конфликт
git-accept-all = ✓ барысын кабул итү
git-unstage = Әзерләүдән алу
git-confirm-deny-all = Барысын кире кагуны раслау
git-deny-all = ✗ барысын кире кагу
git-commit-message = commit хәбәре
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Аерма йөкләнә…
git-no-changes = Күрсәтерлек үзгәрешләр юк
git-accept = ✓ кабул итү
git-deny = ✗ кире кагу
git-show-unchanged-lines = { $count } үзгәрмәгән юлны күрсәтү

terminal-loading = Йөкләнә…
terminal-runs-when-ready = әзер булгач эшли · Ctrl+C чистарта · Esc үткәреп җибәрә
terminal-booting = эшләтеп җибәрелә
terminal-type-command = боерык языгыз · әзер булгач эшли · Esc үткәреп җибәрә

setup-tagline-claude = Anthropic кодлау агенты, Vmux эчендә
setup-tagline-codex = OpenAI кодлау агенты, Vmux эчендә
setup-tagline-vibe = Mistral кодлау агенты, Vmux эчендә
setup-install-title = { $name } CLI урнаштыру
setup-homebrew-required = { $command } урнаштыру өчен Homebrew кирәк, ә ул әле көйләнмәгән. Vmux башта Homebrew, аннары { $name } урнаштырачак.
setup-terminal-instructions = Терминалда башлау өчен Return басыгыз, аннары соралгач Mac серсүзегезне кертегез.
setup-command-missing = Vmux бу битне җирле { $command } боерыгы әле урнаштырылмаганга ачты. Аны алу өчен түбәндәге боерыкны эшләтегез.
setup-install-failed = Урнаштыру тәмамланмады. Тулырак мәгълүмат өчен терминалны тикшерегез, аннары кабатлап карагыз.
setup-installing = Урнаштырыла…
setup-install-homebrew = Homebrew + { $name } урнаштыру
setup-run-install = Урнаштыру боерыгын эшләтү
setup-auto-reload = Vmux аны терминалда эшләтә һәм { $command } әзер булгач яңадан йөкли.

debug-title = Көйләү
debug-auto-update = Авто-яңарту
debug-simulate-update = Яңарту барлыгын имитацияләү
debug-simulate-download = Йөкләүне имитацияләү
debug-clear-update = Яңартуны чистарту
debug-trigger-restart = Яңадан җибәрүне башлату

command-manage-spaces = Аралыклар белән идарә итү…
command-pane-stack-location = панель { $pane } / катлам { $stack }
command-space-pane-stack-location = { $space } / панель { $pane } / катлам { $stack }
command-terminal-path = Терминал ({ $path })
command-group-interactive-mode = Интерактив режим
command-group-window = Тәрәзә
command-group-tab = Кыстыргыч
command-group-pane = Панель
command-group-stack = Катлам
command-group-space = Аралык
command-group-navigation = Навигация
command-group-open = Ачу
command-group-view = Күренеш
command-group-bar = Такта

menu-close-vmux = Vmux’ны ябу

agents-terminal-coding-agent = Терминалга нигезләнгән кодлау агенты
