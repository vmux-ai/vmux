common-open = Ашық
common-close = Жабу
common-install = Орнату
common-uninstall = Жою
common-update = Жаңарту
common-retry = Қайталап көріңіз
common-refresh = Жаңарту
common-remove = Жою
common-enable = Қосу
common-disable = Өшіру
common-new = Жаңа
common-active = белсенді
common-running = жүгіру
common-done = орындалды
common-failed = Сәтсіз
common-installed = Орнатылған
common-items = { $count ->
    [one] { $count } элемент
   *[other] { $count } элементтер
}
start-title = Бастау
start-tagline = Бір шақыру. Кез келген нәрсе жасалды.

agents-title = Агенттер
agents-search = ACP және CLI агенттерін іздеу…
agents-empty = Сәйкес агенттер жоқ
agents-empty-detail = Атты, орындалу уақытын немесе ACP/CLI қолданып көріңіз.
agents-install-failed = Орнату сәтсіз аяқталды
agents-updating = Жаңарту...
agents-retrying = Қайталануда…
agents-preparing = Дайындалуда…

extensions-title = Кеңейтімдер
extensions-search = Орнатылған іздеу немесе Chrome Web Store…
extensions-relaunch = Өтініш беру үшін қайта іске қосыңыз
extensions-empty = Ешқандай кеңейтімдер орнатылмаған
extensions-no-match = Сәйкес кеңейтімдер жоқ
extensions-empty-detail = Жоғарыдағы Chrome Web Store іздеп, Return түймесін басыңыз.
extensions-no-match-detail = Басқа атауды немесе кеңейтім идентификаторын қолданып көріңіз.
extensions-on = Қосулы
extensions-off = Өшірулі
extensions-enable-confirm = { $name } қосу керек пе?
extensions-enable-permissions = { $name } қосыңыз және рұқсат етіңіз:

lsp-title = Тіл серверлері
lsp-search = Тіл серверлерін, линтерлерін, пішімдеушілерін іздеу…
lsp-loading = Каталог жүктелуде…
lsp-empty = Сәйкес тіл серверлері жоқ
lsp-empty-detail = Басқа тілді, линтерді немесе пішімдеуді қолданып көріңіз.
lsp-needs = қажет { $tool }
lsp-status-available = Қол жетімді
lsp-status-on-path = PATH қосулы
lsp-status-installing = Орнатылуда…
lsp-status-installed = Орнатылған
lsp-status-outdated = Жаңарту қолжетімді
lsp-status-running = Жүгіру
lsp-status-failed = Сәтсіз

spaces-title = Кеңістіктер
spaces-new-placeholder = Жаңа ғарыш атауы
spaces-empty = Бос орындар жоқ
spaces-default-name = Кеңістік { $number }
spaces-tabs = { $count ->
    [one] 1 қойынды
   *[other] { $count } қойындылары
}
spaces-delete = Бос орынды жою

team-title = Команда
team-just-you = Бұл кеңістікте тек сен
team-agents = { $count ->
    [one] Сіз және 1 агент
   *[other] Сіз және { $count } агенттер
}
team-empty = Мұнда әлі ешкім жоқ
team-you = Сіз
team-agent = Агент

services-title = Фондық қызметтер
services-processes = { $count ->
    [one] 1 процесс
   *[other] { $count } процестер
}
services-kill-all = Барлығын өлтір
services-not-running = Қызмет жұмыс істемейді
services-start-with = Мынадан бастаңыз:
services-empty = Белсенді процестер жоқ
services-filter = Процестерді сүзу…
services-no-match = Сәйкес процестер жоқ
services-connected = Қосылды
services-disconnected = Ажыратылған
services-attached = тіркелген
services-kill = Өлтір
services-memory = Жад
services-size = Өлшем
services-shell = Shell

error-title = Қате

history-search = Іздеу тарихы
history-clear-all = Барлығын өшіру
history-clear-confirm = Барлық тарихты өшіру керек пе?
history-clear-warning = Бұл әрекетті қайтару мүмкін емес.
history-cancel = Болдырмау
history-today = Бүгін
history-yesterday = Кеше
history-days-ago = { $count } күн бұрын
history-day-offset = Күн -{ $count }

settings-title = Параметрлер
settings-loading = Параметрлер жүктелуде…
settings-stored = ~/.vmux/settings.ron ішінде сақталған
settings-other = Басқа
settings-software-update = Бағдарламалық құралды жаңарту
settings-check-updates = Жаңартуларды тексеріңіз
settings-check-updates-hint = Іске қосылғанда және Автоматты жаңарту қосылғанда сағат сайын автоматты түрде тексереді.
settings-update-unavailable = Қолжетімсіз
settings-update-unavailable-hint = Жаңартқыш бұл құрылымға қосылмаған.
settings-update-checking = Тексерілуде…
settings-update-checking-hint = Жаңартулар тексерілуде…
settings-update-check-again = Қайта тексеру
settings-update-current = Vmux жаңартылған.
settings-update-downloading = Жүктеп алынуда…
settings-update-downloading-hint = Жүктеп алынуда Vmux { $version }…
settings-update-installing = Орнатылуда…
settings-update-installing-hint = Vmux { $version } орнатылуда…
settings-update-ready = Жаңарту дайын
settings-update-ready-hint = Vmux { $version } дайын. Оны қолдану үшін қайта іске қосыңыз.
settings-update-try-again = Қайталап көріңіз
settings-update-failed = Жаңартуларды тексеру мүмкін емес.
settings-item = Элемент
settings-item-number = Элемент { $number }
settings-press-key = Пернені басыңыз…
settings-saved = Сақталды
settings-record-key = Жаңа пернелер тіркесімін жазу үшін басыңыз

tray-open-window = Терезені ашу
tray-close-window = Терезені жабу
tray-pause-recording = Жазуды кідірту
tray-resume-recording = Жазуды жалғастыру
tray-finish-recording = Жазуды аяқтау
tray-quit = Vmux шығу

composer-attach-files = Файлдарды тіркеу (/upload)
composer-remove-attachment = Қосымшаны алып тастаңыз

layout-back = Артқа
layout-forward = Алға
layout-reload = Қайта жүктеңіз
layout-bookmark-page = Осы бетті белгілеңіз
layout-remove-bookmark = Бетбелгіні алып тастаңыз
layout-pin-page = Бұл бетті бекітіңіз
layout-unpin-page = Бұл бетті босату
layout-manage-extensions = Кеңейтімдерді басқару
layout-new-stack = Жаңа стек
layout-close-tab = Қойындыны жабу
layout-bookmark = Бетбелгі
layout-pin = Pin
layout-new-tab = Жаңа қойынды
layout-team = Команда

command-switch-space = Кеңістікті ауыстыру…
command-search-ask = Іздеу немесе сұрау…
command-new-tab-placeholder = URL іздеңіз немесе теріңіз немесе Терминалды таңдаңыз…
command-placeholder = Пәрмендер үшін URL, іздеу қойындыларын немесе > теріңіз…
command-composer-placeholder = Пәрмендер үшін / немесе медиа үшін @ теріңіз
command-send = Жіберу (Enter)
command-terminal = Терминал
command-open-terminal = Терминалда ашыңыз
command-stack = Стек
command-tabs = { $count ->
    [one] 1 қойынды
   *[other] { $count } қойындылары
}
command-prompt = Шақыру
command-new-tab = Жаңа қойынды
command-search = Іздеу
command-open-value = “{ $value }” ашу
command-search-value = "{ $value }" іздеу

schema-appearance = Сыртқы түрі
schema-general = Жалпы
schema-layout = Орналасу
schema-layout-detail = Терезе, тақталар, бүйірлік тақта және фокус сақинасы.
schema-agent = Агент
schema-agent-detail = Агент әрекеті және құрал рұқсаттары.
schema-shortcuts = Таңбашалар
schema-shortcuts-detail = Тек оқуға арналған көрініс. Байланыстыруларды өзгерту үшін settings.ron тікелей өңдеңіз.
schema-terminal = Терминал
schema-browser = Браузер
schema-mode = Режим
schema-mode-detail = Веб-беттерге арналған түс схемасы. Құрылғы жүйеңізді бақылайды.
schema-device = Құрылғы
schema-light = Жарық
schema-dark = Қараңғы
schema-language = Тіл
schema-language-detail = Жүйені, en-US, ja немесе сәйкес ~/.vmux/locales/<tag>.ftl каталогы бар кез келген BCP 47 тегін пайдаланыңыз.
schema-auto-update = Автоматты жаңарту
schema-auto-update-detail = Жаңартуларды іске қосу кезінде және әр сағат сайын тексеріңіз және орнатыңыз.
schema-startup-url = Іске қосу URL
schema-startup-url-detail = Бос пәрмен жолының шақыруын ашады.
schema-search-engine = Іздеу жүйесі
schema-search-engine-detail = «Бастау» және пәрмендер тақтасынан веб-іздеу үшін пайдаланылады.
schema-window = Терезе
schema-pane = Панель
schema-side-sheet = Бүйірлік парақ
schema-focus-ring = Фокус сақинасы
schema-run-placement = Орналастыруды қайта анықтауға рұқсат беріңіз
schema-run-placement-detail = Агенттерге іске қосу тақтасы режимін, бағытты және якорьді таңдауға рұқсат етіңіз.
schema-leader = Көшбасшы
schema-leader-detail = Аккорд таңбашалары үшін префикс пернесі.
schema-chord-timeout = Аккорд күту уақыты
schema-chord-timeout-detail = Аккорд префиксінің мерзімі аяқталғанға дейін миллисекундтар.
schema-bindings = Байланыстар
schema-confirm-close = Жабуды растаңыз
schema-confirm-close-detail = Жұмыс істеп тұрған процессі бар терминалды жабу алдында сұрау.
schema-default-theme = Әдепкі тақырып
schema-default-theme-detail = Тақырыптар тізімінен белсенді тақырыптың атауы.
