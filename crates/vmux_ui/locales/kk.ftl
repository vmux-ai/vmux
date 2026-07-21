common-open = Ашу
common-close = Жабу
common-install = Орнату
common-uninstall = Жою
common-update = Жаңарту
common-retry = Қайталау
common-refresh = Жаңарту
common-remove = Өшіру
common-enable = Қосу
common-disable = Өшіру
common-new = Жаңа
common-active = белсенді
common-running = орындалуда
common-done = дайын
common-failed = Сәтсіз
common-installed = Орнатылған
common-items = { $count ->
    [one] { $count } элемент
   *[other] { $count } элемент
}
start-title = Бастау
start-tagline = Бір prompt. Кез келген іс — дайын.

agents-title = Агенттер
agents-search = ACP және CLI агенттерін іздеу…
agents-empty = Сәйкес агенттер жоқ
agents-empty-detail = Атауын, орындалу ортасын немесе ACP/CLI мәнін көріңіз.
agents-install-failed = Орнату сәтсіз аяқталды
agents-updating = Жаңартылуда…
agents-retrying = Қайталануда…
agents-preparing = Дайындалуда…

extensions-title = Кеңейтімдер
extensions-search = Орнатылғандардан немесе Chrome Web Store ішінен іздеу…
extensions-relaunch = Қолдану үшін қайта іске қосыңыз
extensions-empty = Кеңейтімдер орнатылмаған
extensions-no-match = Сәйкес кеңейтімдер жоқ
extensions-empty-detail = Жоғарыдан Chrome Web Store ішінен іздеп, Return пернесін басыңыз.
extensions-no-match-detail = Басқа атауды немесе кеңейтім ID-ін көріңіз.
extensions-on = Қосулы
extensions-off = Өшірулі
extensions-enable-confirm = { $name } қосылсын ба?
extensions-enable-permissions = { $name } қосылып, мыналарға рұқсат берілсін:

lsp-title = Тіл серверлері
lsp-search = Тіл серверлерін, линтерлерді, форматтағыштарды іздеу…
lsp-loading = Каталог жүктелуде…
lsp-empty = Сәйкес тіл серверлері жоқ
lsp-empty-detail = Басқа тілді, линтерді немесе форматтағышты көріңіз.
lsp-needs = { $tool } қажет
lsp-status-available = Қолжетімді
lsp-status-on-path = PATH ішінде
lsp-status-installing = Орнатылуда…
lsp-status-installed = Орнатылған
lsp-status-outdated = Жаңарту бар
lsp-status-running = Орындалуда
lsp-status-failed = Сәтсіз

spaces-title = Кеңістіктер
spaces-new-placeholder = Жаңа кеңістік атауы
spaces-empty = Кеңістіктер жоқ
spaces-default-name = Кеңістік { $number }
spaces-tabs = { $count ->
    [one] 1 қойынды
   *[other] { $count } қойынды
}
spaces-delete = Кеңістікті өшіру

team-title = Топ
team-just-you = Бұл кеңістікте тек сіз барсыз
team-agents = { $count ->
    [one] Сіз және 1 агент
   *[other] Сіз және { $count } агент
}
team-empty = Мұнда әзірге ешкім жоқ
team-you = Сіз
team-agent = Агент

services-title = Фондық қызметтер
services-processes = { $count ->
    [one] 1 процесс
   *[other] { $count } процесс
}
services-kill-all = Барлығын мәжбүрлеп тоқтату
services-not-running = Қызмет іске қосылмаған
services-start-with = Мынамен іске қосу:
services-empty = Белсенді процестер жоқ
services-filter = Процестерді сүзу…
services-no-match = Сәйкес процестер жоқ
services-connected = Қосылған
services-disconnected = Ажыратылған
services-attached = тіркелген
services-kill = Мәжбүрлеп тоқтату
services-memory = Жад
services-size = Өлшем
services-shell = Shell

error-title = Қате

history-search = Тарихтан іздеу
history-clear-all = Барлығын тазалау
history-clear-confirm = Бүкіл тарих тазалансын ба?
history-clear-warning = Бұл әрекетті қайтару мүмкін емес.
history-cancel = Бас тарту
history-today = Бүгін
history-yesterday = Кеше
history-days-ago = { $count } күн бұрын
history-day-offset = Күн -{ $count }

settings-title = Баптаулар
settings-loading = Баптаулар жүктелуде…
settings-stored = ~/.vmux/settings.ron ішінде сақталады
settings-other = Басқа
settings-software-update = Бағдарламаны жаңарту
settings-check-updates = Жаңартуларды тексеру
settings-check-updates-hint = Автожаңарту қосулы болса, іске қосылғанда және әр сағат сайын автоматты түрде тексереді.
settings-update-unavailable = Қолжетімді емес
settings-update-unavailable-hint = Бұл жинақта жаңартқыш жоқ.
settings-update-checking = Тексерілуде…
settings-update-checking-hint = Жаңартулар тексерілуде…
settings-update-check-again = Қайта тексеру
settings-update-current = Vmux жаңартылған.
settings-update-downloading = Жүктеп алынуда…
settings-update-downloading-hint = Vmux { $version } жүктеп алынуда…
settings-update-installing = Орнатылуда…
settings-update-installing-hint = Vmux { $version } орнатылуда…
settings-update-ready = Жаңарту дайын
settings-update-ready-hint = Vmux { $version } дайын. Қолдану үшін қайта іске қосыңыз.
settings-update-try-again = Қайталап көру
settings-update-failed = Жаңартуларды тексеру мүмкін болмады.
settings-item = Элемент
settings-item-number = Элемент { $number }
settings-press-key = Пернені басыңыз…
settings-saved = Сақталды
settings-record-key = Жаңа перне тіркесімін жазу үшін басыңыз

tray-open-window = Терезені ашу
tray-close-window = Терезені жабу
tray-pause-recording = Жазуды кідірту
tray-resume-recording = Жазуды жалғастыру
tray-finish-recording = Жазуды аяқтау
tray-quit = Vmux-тан шығу

composer-attach-files = Файлдарды тіркеу (/upload)
composer-remove-attachment = Тіркемені өшіру

layout-back = Артқа
layout-forward = Алға
layout-reload = Қайта жүктеу
layout-bookmark-page = Бұл бетті бетбелгіге қосу
layout-remove-bookmark = Бетбелгіні өшіру
layout-pin-page = Бұл бетті бекіту
layout-unpin-page = Бұл бетті босату
layout-manage-extensions = Кеңейтімдерді басқару
layout-new-stack = Жаңа стек
layout-close-tab = Қойындыны жабу
layout-bookmark = Бетбелгі
layout-pin = Бекіту
layout-new-tab = Жаңа қойынды
layout-team = Топ

command-switch-space = Кеңістікті ауыстыру…
command-search-ask = Іздеу немесе сұрау…
command-new-tab-placeholder = Іздеңіз, URL енгізіңіз немесе Терминалды таңдаңыз…
command-placeholder = URL енгізіңіз, қойындылардан іздеңіз немесе командалар үшін > теріңіз…
command-composer-placeholder = Командалар үшін /, медиа үшін @ теріңіз
command-send = Жіберу (Enter)
command-terminal = Терминал
command-open-terminal = Терминалда ашу
command-stack = Стек
command-tabs = { $count ->
    [one] 1 қойынды
   *[other] { $count } қойынды
}
command-prompt = Prompt
command-new-tab = Жаңа қойынды
command-search = Іздеу
command-open-value = “{ $value }” ашу
command-search-value = “{ $value }” іздеу

schema-appearance = Көрініс
schema-general = Жалпы
schema-layout = Орналасу
schema-layout-detail = Терезе, панельдер, бүйірлік тақта және фокус жиегі.
schema-agent = Агент
schema-agent-detail = Агент әрекеті және құрал рұқсаттары.
schema-shortcuts = Перне тіркесімдері
schema-shortcuts-detail = Тек оқуға арналған көрініс. Тіркесімдерді өзгерту үшін settings.ron файлын тікелей өңдеңіз.
schema-terminal = Терминал
schema-browser = Браузер
schema-mode = Режим
schema-mode-detail = Веб-беттердің түс схемасы. «Құрылғы» жүйеңізге сай жүреді.
schema-device = Құрылғы
schema-light = Ашық
schema-dark = Қараңғы
schema-language = Тіл
schema-language-detail = Жүйе тілін, en-US, ja немесе сәйкес ~/.vmux/locales/<tag>.ftl каталогы бар кез келген BCP 47 тегін пайдаланыңыз.
schema-auto-update = Автожаңарту
schema-auto-update-detail = Іске қосылғанда және әр сағат сайын жаңартуларды тексеріп, орнату.
schema-startup-url = Іске қосу URL-і
schema-startup-url-detail = Бос болса, команда жолағының prompt өрісі ашылады.
schema-search-engine = Іздеу жүйесі
schema-search-engine-detail = Бастау бетінен және команда жолағынан вебте іздеу үшін қолданылады.
schema-window = Терезе
schema-pane = Панель
schema-side-sheet = Бүйірлік парақ
schema-focus-ring = Фокус жиегі
schema-run-placement = Орындау орнын өзгертуге рұқсат ету
schema-run-placement-detail = Агенттерге орындау панелі режимін, бағытын және бекіту нүктесін таңдауға рұқсат етіңіз.
schema-leader = Бастапқы перне
schema-leader-detail = Аккорд тіркесімдеріне арналған префикс перне.
schema-chord-timeout = Аккорд күту уақыты
schema-chord-timeout-detail = Аккорд префиксі аяқталғанға дейінгі миллисекундтар.
schema-bindings = Тіркесімдер
schema-confirm-close = Жабуды растау
schema-confirm-close-detail = Процесс орындалып тұрған терминалды жабар алдында сұрау.
schema-default-theme = Әдепкі тақырып
schema-default-theme-detail = Тақырыптар тізіміндегі белсенді тақырып атауы.
