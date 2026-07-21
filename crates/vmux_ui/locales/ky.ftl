locale-name = кыргызча
common-open = Ачуу
common-close = Жабуу
common-install = Орнотуу
common-uninstall = Өчүрүү
common-update = Жаңыртуу
common-retry = Кайра аракет кылуу
common-refresh = Жаңылоо
common-remove = Алып салуу
common-enable = Иштетүү
common-disable = Өчүрүү
common-new = Жаңы
common-active = активдүү
common-running = иштеп жатат
common-done = бүттү
common-failed = Ишке ашкан жок
common-installed = Орнотулду
common-items = { $count ->
    [one] { $count } нерсе
   *[other] { $count } нерсе
}
start-title = Баштоо
start-tagline = Бир промпт. Баары даяр.

agents-title = Агенттер
agents-search = ACP жана CLI агенттерин издөө…
agents-empty = Дал келген агент жок
agents-empty-detail = Атын, аткаруу чөйрөсүн же ACP/CLI киргизип көрүңүз.
agents-install-failed = Орнотулбай калды
agents-updating = Жаңыртылууда…
agents-retrying = Кайра аракет кылынууда…
agents-preparing = Даярдалууда…

extensions-title = Кеңейтүүлөр
extensions-search = Орнотулгандардан же Chrome Web Store'дон издөө…
extensions-relaunch = Колдонуу үчүн кайра иштетиңиз
extensions-empty = Орнотулган кеңейтүү жок
extensions-no-match = Дал келген кеңейтүү жок
extensions-empty-detail = Жогорудан Chrome Web Store'дон издеп, Return басыңыз.
extensions-no-match-detail = Башка ат же кеңейтүү ID киргизип көрүңүз.
extensions-on = Күйүк
extensions-off = Өчүк
extensions-enable-confirm = { $name } иштетилсинби?
extensions-enable-permissions = { $name } иштетилип, төмөнкүлөргө уруксат берилсин:

lsp-title = Тил серверлери
lsp-search = Тил серверлерин, линтерлерди, форматтоочуларды издөө…
lsp-loading = Каталог жүктөлүүдө…
lsp-empty = Дал келген тил сервери жок
lsp-empty-detail = Башка тил, линтер же форматтоочу киргизип көрүңүз.
lsp-needs = { $tool } керек
lsp-status-available = Жеткиликтүү
lsp-status-on-path = PATH ичинде
lsp-status-installing = Орнотулууда…
lsp-status-installed = Орнотулду
lsp-status-outdated = Жаңыртуу бар
lsp-status-running = Иштеп жатат
lsp-status-failed = Ишке ашкан жок

spaces-title = Мейкиндиктер
spaces-new-placeholder = Жаңы мейкиндиктин аты
spaces-empty = Мейкиндик жок
spaces-default-name = Мейкиндик { $number }
spaces-tabs = { $count ->
    [one] 1 өтмөк
   *[other] { $count } өтмөк
}
spaces-delete = Мейкиндикти өчүрүү

team-title = Команда
team-just-you = Бул мейкиндикте сиз гана барсыз
team-agents = { $count ->
    [one] Сиз жана 1 агент
   *[other] Сиз жана { $count } агент
}
team-empty = Бул жерде азырынча эч ким жок
team-you = Сиз
team-agent = Агент

services-title = Фондук кызматтар
services-processes = { $count ->
    [one] 1 процесс
   *[other] { $count } процесс
}
services-kill-all = Баарын мажбурлап токтотуу
services-not-running = Кызмат иштеп жаткан жок
services-start-with = Муну менен баштоо:
services-empty = Активдүү процесс жок
services-filter = Процесстерди чыпкалоо…
services-no-match = Дал келген процесс жок
services-connected = Туташты
services-disconnected = Ажыратылды
services-attached = тиркелген
services-kill = Мажбурлап токтотуу
services-memory = Эс тутум
services-size = Өлчөм
services-shell = Shell

error-title = Ката

history-search = Тарыхтан издөө
history-clear-all = Баарын тазалоо
history-clear-confirm = Тарыхтын баары тазалансынбы?
history-clear-warning = Муну артка кайтаруу мүмкүн эмес.
history-cancel = Жокко чыгаруу
history-today = Бүгүн
history-yesterday = Кечээ
history-days-ago = { $count } күн мурун
history-day-offset = Күн -{ $count }

settings-title = Жөндөөлөр
settings-loading = Жөндөөлөр жүктөлүүдө…
settings-stored = ~/.vmux/settings.ron ичинде сакталат
settings-other = Башка
settings-software-update = Программаны жаңыртуу
settings-check-updates = Жаңыртууларды текшерүү
settings-check-updates-hint = Автожаңыртуу иштетилсе, ишке киргенде жана ар бир саат сайын автоматтык текшерет.
settings-update-unavailable = Жеткиликсиз
settings-update-unavailable-hint = Бул түзүлүшкө жаңырткыч киргизилген эмес.
settings-update-checking = Текшерилүүдө…
settings-update-checking-hint = Жаңыртуулар текшерилүүдө…
settings-update-check-again = Кайра текшерүү
settings-update-current = Vmux эң акыркы версияда.
settings-update-downloading = Жүктөлүүдө…
settings-update-downloading-hint = Vmux { $version } жүктөлүүдө…
settings-update-installing = Орнотулууда…
settings-update-installing-hint = Vmux { $version } орнотулууда…
settings-update-ready = Жаңыртуу даяр
settings-update-ready-hint = Vmux { $version } даяр. Колдонуу үчүн кайра иштетиңиз.
settings-update-try-again = Кайра аракет кылуу
settings-update-failed = Жаңыртууларды текшерүү мүмкүн болбоду.
settings-item = Нерсе
settings-item-number = Нерсе { $number }
settings-press-key = Баскычты басыңыз…
settings-saved = Сакталды
settings-record-key = Жаңы баскыч айкалышын жаздыруу үчүн басыңыз

tray-open-window = Терезени ачуу
tray-close-window = Терезени жабуу
tray-pause-recording = Жаздырууну тындыруу
tray-resume-recording = Жаздырууну улантуу
tray-finish-recording = Жаздырууну бүтүрүү
tray-quit = Vmux'тан чыгуу

composer-attach-files = Файлдарды тиркөө (/upload)
composer-remove-attachment = Тиркемени алып салуу

layout-back = Артка
layout-forward = Алга
layout-reload = Кайра жүктөө
layout-bookmark-page = Бул баракты кыстармага кошуу
layout-remove-bookmark = Кыстарманы алып салуу
layout-pin-page = Бул баракты кадап коюу
layout-unpin-page = Бул баракты кадоодон чыгаруу
layout-manage-extensions = Кеңейтүүлөрдү башкаруу
layout-new-stack = Жаңы катмар
layout-close-tab = Өтмөктү жабуу
layout-bookmark = Кыстарма
layout-pin = Кадоо
layout-new-tab = Жаңы өтмөк
layout-team = Команда

command-switch-space = Мейкиндикти алмаштыруу…
command-search-ask = Издөө же суроо берүү…
command-new-tab-placeholder = Издеңиз же URL териңиз, же Терминалды тандаңыз…
command-placeholder = URL териңиз, өтмөктөрдөн издеңиз же буйруктар үчүн > киргизиңиз…
command-composer-placeholder = Буйруктар үчүн / же медиа үчүн @ териңиз
command-send = Жөнөтүү (Enter)
command-terminal = Терминал
command-open-terminal = Терминалда ачуу
command-stack = Катмар
command-tabs = { $count ->
    [one] 1 өтмөк
   *[other] { $count } өтмөк
}
command-prompt = Промпт
command-new-tab = Жаңы өтмөк
command-search = Издөө
command-open-value = “{ $value }” ачуу
command-search-value = “{ $value }” издөө

schema-appearance = Көрүнүш
schema-general = Жалпы
schema-layout = Жайгашуу
schema-layout-detail = Терезе, панелдер, каптал тилке жана фокус алкагы.
schema-agent = Агент
schema-agent-detail = Агенттин жүрүм-туруму жана курал уруксаттары.
schema-shortcuts = Кыска жолдор
schema-shortcuts-detail = Окуу үчүн гана көрүнүш. Байламдарды өзгөртүү үчүн settings.ron файлын түз түзөтүңүз.
schema-terminal = Терминал
schema-browser = Браузер
schema-mode = Режим
schema-mode-detail = Веб-барактар үчүн түс схемасы. Түзмөк тутумуңузду ээрчийт.
schema-device = Түзмөк
schema-light = Ачык
schema-dark = Караңгы
schema-language = Тил
schema-language-detail = Тутумду, en-US, ja же дал келген ~/.vmux/locales/<tag>.ftl каталогу бар каалаган BCP 47 тегин колдонуңуз.
schema-auto-update = Автожаңыртуу
schema-auto-update-detail = Ишке киргенде жана ар бир саат сайын жаңыртууларды текшерип, орнотот.
schema-startup-url = Баштапкы URL
schema-startup-url-detail = Бош болсо, буйрук тилкесиндеги промпт ачылат.
schema-search-engine = Издөө кыймылдаткычы
schema-search-engine-detail = Баштоодон жана буйрук тилкесинен веб-издөөлөр үчүн колдонулат.
schema-window = Терезе
schema-pane = Панель
schema-side-sheet = Каптал барак
schema-focus-ring = Фокус алкагы
schema-run-placement = Иштетүү жайгашуусун өзгөртүүгө уруксат берүү
schema-run-placement-detail = Агенттерге иштетүү панелинин режимин, багытын жана анкерин тандоого уруксат бериңиз.
schema-leader = Лидер
schema-leader-detail = Аккорд кыска жолдору үчүн префикс баскыч.
schema-chord-timeout = Аккорд күтүү мөөнөтү
schema-chord-timeout-detail = Аккорд префикси жараксыз болгонго чейинки миллисекунддар.
schema-bindings = Байламдар
schema-confirm-close = Жабууну ырастоо
schema-confirm-close-detail = Иштеп жаткан процесси бар терминалды жабуудан мурун сурайт.
schema-default-theme = Баштапкы тема
schema-default-theme-detail = Темалар тизмесиндеги активдүү теманын аты.
