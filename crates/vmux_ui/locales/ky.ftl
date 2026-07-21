common-open = Open
common-close = Жабуу
common-install = Орнотуу
common-uninstall = Чыгаруу
common-update = Жаңыртуу
common-retry = Кайталап көрүңүз
common-refresh = Жаңылоо
common-remove = Алып салуу
common-enable = Иштетүү
common-disable = Өчүрүү
common-new = Жаңы
common-active = активдүү
common-running = чуркоо
common-done = аткарылды
common-failed = Ийгиликсиз
common-installed = Орнотулган
common-items = { $count ->
    [one] { $count } нерсе
   *[other] { $count } нерселер
}
start-title = Баштоо
start-tagline = Бир сурот. Баары болду.

agents-title = Агенттер
agents-search = ACP жана CLI агенттерин издөө…
agents-empty = Дал келген агенттер жок
agents-empty-detail = Атын, иштөө убактысын же ACP/CLI колдонуп көрүңүз.
agents-install-failed = Орнотуу ишке ашкан жок
agents-updating = Жаңыртылууда…
agents-retrying = Кайталанууда…
agents-preparing = Даярдалууда…

extensions-title = Кеңейтүүлөр
extensions-search = Орнотулган издөө же Chrome Web Store…
extensions-relaunch = Колдонуу үчүн кайра иштетиңиз
extensions-empty = Эч кандай кеңейтүүлөр орнотулган
extensions-no-match = Дал келген кеңейтүүлөр жок
extensions-empty-detail = Жогорудагы Chrome Web Store издеңиз жана Return басыңыз.
extensions-no-match-detail = Башка ат же кеңейтүү идентификаторун колдонуп көрүңүз.
extensions-on = Күйүк
extensions-off = Өчүк
extensions-enable-confirm = { $name } иштетилсинби?
extensions-enable-permissions = { $name } иштетүү жана уруксат:

lsp-title = Тил серверлери
lsp-search = Тил серверлерин, линтерлерин, форматтоочуларын издөө…
lsp-loading = Каталог жүктөлүүдө…
lsp-empty = Дал келген тил серверлери жок
lsp-empty-detail = Башка тилди, линтерди же форматтоочуну байкап көрүңүз.
lsp-needs = { $tool } керек
lsp-status-available = жеткиликтүү
lsp-status-on-path = PATH боюнча
lsp-status-installing = Орнотулууда…
lsp-status-installed = Орнотулган
lsp-status-outdated = Жаңыртуу жеткиликтүү
lsp-status-running = чуркоо
lsp-status-failed = Ийгиликсиз

spaces-title = Spaces
spaces-new-placeholder = Жаңы космос аты
spaces-empty = Боштук жок
spaces-default-name = Боштук { $number }
spaces-tabs = { $count ->
    [one] 1 өтмөк
   *[other] { $count } өтмөктөр
}
spaces-delete = Бош орун жок кылуу

team-title = Команда
team-just-you = Бул мейкиндикте сиз гана
team-agents = { $count ->
    [one] Сиз жана 1 агент
   *[other] Сиз жана { $count } агенттери
}
team-empty = Бул жерде азырынча эч ким жок
team-you = сен
team-agent = Агент

services-title = Фондук кызматтар
services-processes = { $count ->
    [one] 1 процесс
   *[other] { $count } процесстери
}
services-kill-all = Баарын өлтүр
services-not-running = Кызмат иштебей жатат
services-start-with = Баштоо:
services-empty = Активдүү процесстер жок
services-filter = Процесстерди чыпкалоо…
services-no-match = Дал келген процесстер жок
services-connected = Туташкан
services-disconnected = Ажыратылды
services-attached = тиркелген
services-kill = Өлтүрүү
services-memory = Эс
services-size = Өлчөмү
services-shell = Shell

error-title = Ката

history-search = Издөө таржымалы
history-clear-all = Баарын тазалоо
history-clear-confirm = Таржымалдын баары тазалансынбы?
history-clear-warning = Муну артка кайтаруу мүмкүн эмес.
history-cancel = Жокко чыгаруу
history-today = Бүгүн
history-yesterday = Кечээ
history-days-ago = { $count } күн мурун
history-day-offset = Күн -{ $count }

settings-title = Орнотуулар
settings-loading = Жөндөөлөр жүктөлүүдө…
settings-stored = ~/.vmux/settings.ron ичинде сакталган
settings-other = Башка
settings-software-update = Программалык камсыздоону жаңыртуу
settings-check-updates = Жаңыртууларды текшериңиз
settings-check-updates-hint = Ишке киргизилгенде жана Автоматтык жаңыртуу иштетилгенде саат сайын автоматтык түрдө текшерет.
settings-update-unavailable = Жеткиликсиз
settings-update-unavailable-hint = Жаңырткыч бул түзүүгө камтылган эмес.
settings-update-checking = Текшерилүүдө…
settings-update-checking-hint = Жаңыртуулар текшерилүүдө…
settings-update-check-again = Кайра текшерүү
settings-update-current = Vmux жаңыртылган.
settings-update-downloading = Жүктөлүүдө…
settings-update-downloading-hint = Жүктөлүп алынууда Vmux { $version }…
settings-update-installing = Орнотулууда…
settings-update-installing-hint = Vmux { $version } орнотулууда…
settings-update-ready = Жаңыртуу даяр
settings-update-ready-hint = Vmux { $version } даяр. Аны колдонуу үчүн кайра баштаңыз.
settings-update-try-again = Кайра аракет кылыңыз
settings-update-failed = Жаңыртууларды текшерүү мүмкүн эмес.
settings-item = пункт
settings-item-number = { $number } нерсе
settings-press-key = Баскычты басыңыз…
settings-saved = Сакталган
settings-record-key = Жаңы баскычтар айкалышын жаздыруу үчүн чыкылдатыңыз

tray-open-window = Open Window
tray-close-window = Терезени жабуу
tray-pause-recording = Жаздырууну тындыруу
tray-resume-recording = Жазууну улантуу
tray-finish-recording = Жаздырууну бүтүрүү
tray-quit = Vmux чыгуу

composer-attach-files = Файлдарды тиркөө (/upload)
composer-remove-attachment = Тиркемени алып салуу

layout-back = Артка
layout-forward = Алга
layout-reload = Кайра жүктөө
layout-bookmark-page = Бул баракты белгилеңиз
layout-remove-bookmark = Кыстарманы алып салуу
layout-pin-page = Бул баракты кадап коюңуз
layout-unpin-page = Бул баракты бошотуу
layout-manage-extensions = Кеңейтүүлөрдү башкаруу
layout-new-stack = Жаңы стек
layout-close-tab = Өтмөктү жабуу
layout-bookmark = Bookmark
layout-pin = Pin
layout-new-tab = Жаңы өтмөк
layout-team = Команда

command-switch-space = Орун алмаштыруу…
command-search-ask = Издөө же суроо…
command-new-tab-placeholder = URL издеңиз же териңиз, же Терминалды тандаңыз…
command-placeholder = URL, издөө өтмөктөрүн териңиз же буйруктар үчүн >…
command-composer-placeholder = Буйруктар үчүн / же медиа үчүн @ териңиз
command-send = Жөнөтүү (Enter)
command-terminal = Терминал
command-open-terminal = Терминалда ачуу
command-stack = Стек
command-tabs = { $count ->
    [one] 1 өтмөк
   *[other] { $count } өтмөктөр
}
command-prompt = Prompt
command-new-tab = Жаңы өтмөк
command-search = Издөө
command-open-value = “{ $value }” ачуу
command-search-value = "{ $value }" издөө

schema-appearance = Көрүнүш
schema-general = Генерал
schema-layout = Макет
schema-layout-detail = Терезе, панелдер, каптал тилкеси жана фокус шакеги.
schema-agent = Агент
schema-agent-detail = Агенттин жүрүм-туруму жана курал уруксаттары.
schema-shortcuts = Жарлыктар
schema-shortcuts-detail = Окуу үчүн гана көрүнүш. Байланыштарды өзгөртүү үчүн settings.ron түз түзөтүңүз.
schema-terminal = Терминал
schema-browser = Браузер
schema-mode = Mode
schema-mode-detail = Веб баракчалар үчүн түс схемасы. Түзмөк тутумуңузду ээрчийт.
schema-device = Түзмөк
schema-light = Жарык
schema-dark = Караңгы
schema-language = Тил
schema-language-detail = Системаны, en-US, ja, же ~/.vmux/locales/<tag>.ftl каталогу менен каалаган BCP 47 тэгди колдонуңуз.
schema-auto-update = Автоматтык жаңыртуу
schema-auto-update-detail = Жаңыртууларды ишке киргизүүдө жана саат сайын текшерип, орнотуңуз.
schema-startup-url = Ишке киргизүү URL
schema-startup-url-detail = Бош команда тилкесин ачат.
schema-search-engine = Издөө системасы
schema-search-engine-detail = Баштоо жана буйрук тилкесинде желе издөө үчүн колдонулат.
schema-window = Терезе
schema-pane = Пане
schema-side-sheet = Каптал барак
schema-focus-ring = Фокус шакеги
schema-run-placement = Ишке жайгаштырууну жокко чыгарууга уруксат берүү
schema-run-placement-detail = Агенттерге иштетүү панелинин режимин, багытын жана анкерди тандоосуна уруксат бериңиз.
schema-leader = Лидер
schema-leader-detail = Аккорд жарлыктары үчүн префикс баскычы.
schema-chord-timeout = Аккорд таймауту
schema-chord-timeout-detail = Аккорд префиксинин мөөнөтү бүтөрүнө миллисекунд.
schema-bindings = Байланыштар
schema-confirm-close = Жабууну ырастоо
schema-confirm-close-detail = Иштеп жаткан процесс менен терминалды жабуудан мурун суроо.
schema-default-theme = Демейки тема
schema-default-theme-detail = Темалар тизмесинен активдүү теманын аталышы.
