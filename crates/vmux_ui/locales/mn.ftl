locale-name = монгол
common-open = Нээх
common-close = Хаах
common-install = Суулгах
common-uninstall = Устгах
common-update = Шинэчлэх
common-retry = Дахин оролдох
common-refresh = Сэргээх
common-remove = Хасах
common-enable = Идэвхжүүлэх
common-disable = Идэвхгүй болгох
common-new = Шинэ
common-active = идэвхтэй
common-running = ажиллаж байна
common-done = дууссан
common-failed = Амжилтгүй
common-installed = Суулгасан
common-items = { $count ->
    [one] { $count } зүйл
   *[other] { $count } зүйл
}

tools-title = Хэрэгслүүд
tools-search = Багц, агент, MCP, хэлний хэрэгсэл болон тохиргооны файл хайх…
tools-open = Хэрэгслүүдийг нээх
tools-fold = Хэрэгслүүдийг хураах
tools-unfold = Хэрэгслүүдийг дэлгэх
tools-scanning = Дотоод хэрэгслүүдийг шалгаж байна…
tools-no-installed = Суулгасан хэрэгсэл алга
tools-empty = Тохирох хэрэгсэл алга
tools-empty-detail = Багц суулгах эсвэл Stow загварын тохиргооны файлын багц нэмнэ үү.
tools-apply = Хэрэглэх
tools-homebrew = Homebrew
tools-homebrew-sync = Суулгасан томьёо болон аппууд автоматаар синк хийнэ.
tools-open-brewfile = Brewfile нээх
tools-managed = удирдлагатай
tools-provider-homebrew-formulae = Homebrew томьёонууд
tools-provider-homebrew-casks = Homebrew аппууд
tools-provider-npm = npm багцууд
tools-provider-acp-agents = ACP агентууд
tools-provider-language-tools = Хэлний хэрэгслүүд
tools-provider-mcp-servers = MCP серверүүд
tools-provider-dotfiles = Тохиргооны файлууд
tools-status-available = Боломжтой
tools-status-missing = Алга
tools-status-conflict = Зөрчил
tools-forget = Мартах
tools-manage = Удирдах
tools-link = Холбох
tools-unlink = Холбоосыг салгах
tools-import = Импортлох
tools-update-count = { $count ->
    [one] 1 шинэчлэл
   *[other] { $count } шинэчлэл
}
tools-conflict-count = { $count ->
    [one] 1 зөрчил
   *[other] { $count } зөрчил
}
tools-result-applied = Хэрэгслүүдийг хэрэглэв
tools-result-imported = Хэрэгслүүдийг импортлов
tools-result-installed = { $name } суулгагдлаа
tools-result-updated = { $name } шинэчлэгдлээ
tools-result-uninstalled = { $name } устгагдлаа
tools-result-forgotten = { $name } мартагдлаа
tools-result-managed = { $name } одоо удирдлагатай
tools-result-linked = { $name } холбогдлоо
tools-result-unlinked = { $name }-ийн холбоос салгагдлаа

start-title = Эхлэх
start-tagline = Нэг prompt. Юу ч байсан, хийчихнэ.

agents-title = Агентууд
agents-search = ACP болон CLI агент хайх…
agents-empty = Тохирох агент алга
agents-empty-detail = Нэр, runtime, эсвэл ACP/CLI-ээр хайгаад үзнэ үү.
agents-install-failed = Суулгаж чадсангүй
agents-updating = Шинэчилж байна…
agents-retrying = Дахин оролдож байна…
agents-preparing = Бэлдэж байна…

extensions-title = Өргөтгөлүүд
extensions-search = Суулгасан өргөтгөл эсвэл Chrome Web Store-оос хайх…
extensions-relaunch = Хэрэгжүүлэхийн тулд дахин нээх
extensions-empty = Суулгасан өргөтгөл алга
extensions-no-match = Тохирох өргөтгөл алга
extensions-empty-detail = Дээрхээс Chrome Web Store-оос хайгаад Return дарна уу.
extensions-no-match-detail = Өөр нэр эсвэл өргөтгөлийн ID оруулж үзнэ үү.
extensions-on = Асаалттай
extensions-off = Унтраалттай
extensions-enable-confirm = { $name }-г идэвхжүүлэх үү?
extensions-enable-permissions = { $name }-г идэвхжүүлээд дараахыг зөвшөөрөх:

lsp-title = Хэлний серверүүд
lsp-search = Хэлний сервер, линтер, форматлагч хайх…
lsp-loading = Каталог ачаалж байна…
lsp-empty = Тохирох хэлний сервер алга
lsp-empty-detail = Өөр хэл, линтер эсвэл форматлагч хайгаад үзнэ үү.
lsp-needs = { $tool } шаардлагатай
lsp-status-available = Боломжтой
lsp-status-on-path = PATH-д байна
lsp-status-installing = Суулгаж байна…
lsp-status-installed = Суулгасан
lsp-status-outdated = Шинэчлэлт байна
lsp-status-running = Ажиллаж байна
lsp-status-failed = Амжилтгүй

spaces-title = Орчнууд
spaces-new-placeholder = Шинэ орчны нэр
spaces-empty = Орчин алга
spaces-default-name = Орчин { $number }
spaces-tabs = { $count ->
    [one] 1 таб
   *[other] { $count } таб
}
spaces-delete = Орчин устгах

team-title = Баг
team-just-you = Энэ орчинд зөвхөн та байна
team-agents = { $count ->
    [one] Та болон 1 агент
   *[other] Та болон { $count } агент
}
team-empty = Одоогоор энд хэн ч алга
team-you = Та
team-agent = Агент

services-title = Арын үйлчилгээ
services-processes = { $count ->
    [one] 1 процесс
   *[other] { $count } процесс
}
services-kill-all = Бүгдийг хүчээр зогсоох
services-not-running = Үйлчилгээ ажиллахгүй байна
services-start-with = Дараахаар эхлүүлэх:
services-empty = Идэвхтэй процесс алга
services-filter = Процесс шүүх…
services-no-match = Тохирох процесс алга
services-connected = Холбогдсон
services-disconnected = Холбогдоогүй
services-attached = хавсарсан
services-kill = Хүчээр зогсоох
services-memory = Санах ой
services-size = Хэмжээ
services-shell = Shell

error-title = Алдаа

history-search = Түүхээс хайх
history-clear-all = Бүгдийг арилгах
history-clear-confirm = Бүх түүхийг арилгах уу?
history-clear-warning = Үүнийг буцаах боломжгүй.
history-cancel = Цуцлах
history-today = Өнөөдөр
history-yesterday = Өчигдөр
history-days-ago = { $count } хоногийн өмнө
history-day-offset = Өдөр -{ $count }

settings-title = Тохиргоо
settings-loading = Тохиргоо ачаалж байна…
settings-stored = ~/.vmux/settings.ron-д хадгална
settings-other = Бусад
settings-software-update = Програмын шинэчлэлт
settings-check-updates = Шинэчлэлт шалгах
settings-check-updates-hint = Авто шинэчлэлт идэвхтэй үед эхлэх бүрд болон цаг тутам автоматаар шалгана.
settings-update-unavailable = Боломжгүй
settings-update-unavailable-hint = Энэ бүтээлд шинэчлэгч багтаагүй.
settings-update-checking = Шалгаж байна…
settings-update-checking-hint = Шинэчлэлт шалгаж байна…
settings-update-check-again = Дахин шалгах
settings-update-current = Vmux хамгийн сүүлийн хувилбартай байна.
settings-update-downloading = Татаж байна…
settings-update-downloading-hint = Vmux { $version } татаж байна…
settings-update-installing = Суулгаж байна…
settings-update-installing-hint = Vmux { $version } суулгаж байна…
settings-update-ready = Шинэчлэлт бэлэн
settings-update-ready-hint = Vmux { $version } бэлэн боллоо. Хэрэгжүүлэхийн тулд дахин эхлүүлнэ үү.
settings-update-try-again = Дахин оролдох
settings-update-failed = Шинэчлэлт шалгаж чадсангүй.
settings-item = Зүйл
settings-item-number = Зүйл { $number }
settings-press-key = Товч дарна уу…
settings-saved = Хадгалсан
settings-record-key = Шинэ товчны хослол бичихийн тулд дарна уу

tray-open-window = Цонх нээх
tray-close-window = Цонх хаах
tray-pause-recording = Бичлэг түр зогсоох
tray-resume-recording = Бичлэг үргэлжлүүлэх
tray-finish-recording = Бичлэг дуусгах
tray-quit = Vmux-ээс гарах

composer-attach-files = Файл хавсаргах (/upload)
composer-remove-attachment = Хавсралт хасах

layout-back = Буцах
layout-forward = Урагш
layout-reload = Дахин ачаалах
layout-bookmark-page = Энэ хуудсыг хавчуурга болгох
layout-remove-bookmark = Хавчуургыг хасах
layout-pin-page = Энэ хуудсыг бэхлэх
layout-unpin-page = Энэ хуудсыг салгах
layout-manage-extensions = Өргөтгөл удирдах
layout-new-stack = Шинэ давхарга
layout-close-tab = Таб хаах
layout-bookmark = Хавчуурга
layout-pin = Бэхлэх
layout-new-tab = Шинэ таб
layout-team = Баг

command-switch-space = Орчин солих…
command-search-ask = Хайх эсвэл асуух…
command-new-tab-placeholder = Хайх, URL бичих эсвэл Terminal сонгох…
command-placeholder = URL бичих, таб хайх, эсвэл командын тулд > бичих…
command-composer-placeholder = Командын тулд /, медиа оруулах бол @ бичнэ үү
command-send = Илгээх (Enter)
command-terminal = Терминал
command-open-terminal = Терминалд нээх
command-stack = Давхарга
command-tabs = { $count ->
    [one] 1 таб
   *[other] { $count } таб
}
command-prompt = Prompt
command-new-tab = Шинэ таб
command-search = Хайх
command-open-value = “{ $value }”-г нээх
command-search-value = “{ $value }”-г хайх

schema-appearance = Харагдах байдал
schema-general = Ерөнхий
schema-layout = Байрлал
schema-layout-detail = Цонх, пане, хажуу самбар, фокусын хүрээ.
schema-agent = Агент
schema-agent-detail = Агентын ажиллагаа болон хэрэгслийн зөвшөөрөл.
schema-shortcuts = Товчлолууд
schema-shortcuts-detail = Зөвхөн харах боломжтой. Холболтыг өөрчлөхийн тулд settings.ron-г шууд засна уу.
schema-terminal = Терминал
schema-browser = Хөтөч
schema-mode = Горим
schema-mode-detail = Вэб хуудсын өнгөний схем. Device нь таны системийг дагана.
schema-device = Device
schema-light = Цайвар
schema-dark = Бараан
schema-language = Хэл
schema-language-detail = Системийн хэл, en-US, ja, эсвэл тохирох ~/.vmux/locales/<tag>.ftl каталогтой дурын BCP 47 таг ашиглана.
schema-auto-update = Авто шинэчлэлт
schema-auto-update-detail = Эхлэх үед болон цаг тутам шинэчлэлт шалгаж суулгана.
schema-startup-url = Эхлэх URL
schema-startup-url-detail = Хоосон бол командын мөрийн prompt-ыг нээнэ.
schema-search-engine = Хайлтын систем
schema-search-engine-detail = Эхлэл болон командын мөрөөс хийх вэб хайлтад ашиглана.
schema-window = Цонх
schema-pane = Пане
schema-side-sheet = Хажуу хуудас
schema-focus-ring = Фокусын хүрээ
schema-run-placement = Ажиллуулах байрлалыг өөрчлөхийг зөвшөөрөх
schema-run-placement-detail = Агентуудад ажиллуулах пане горим, чиглэл, зангууг сонгох боломж олгоно.
schema-leader = Удирдах товч
schema-leader-detail = Chord товчлолын өмнөх товч.
schema-chord-timeout = Chord хугацаа
schema-chord-timeout-detail = Chord угтварын хугацаа дуусах хүртэлх миллисекунд.
schema-bindings = Холболтууд
schema-confirm-close = Хаахыг баталгаажуулах
schema-confirm-close-detail = Ажиллаж буй процесстой терминал хаахын өмнө асууна.
schema-default-theme = Өгөгдмөл загвар
schema-default-theme-detail = Загварын жагсаалтаас идэвхтэй загварын нэр.

settings-empty = (хоосон)
settings-none = (байхгүй)

schema-system = Систем
schema-editor = Засварлагч
schema-recording = Бичлэг
schema-radius = Радиус
schema-padding = Дотор зай
schema-gap = Зай
schema-width = Өргөн
schema-color = Өнгө
schema-red = Улаан
schema-green = Ногоон
schema-blue = Цэнхэр
schema-follow-files = Файлуудыг дагах
schema-tidy-files = Файлуудыг цэгцлэх
schema-tidy-files-max = Файл цэгцлэх босго
schema-tidy-files-auto = Файлуудыг автоматаар цэгцлэх
schema-app-providers = Апп нийлүүлэгчид
schema-provider = Нийлүүлэгч
schema-kind = Төрөл
schema-models = Загварууд
schema-acp = ACP агентууд
schema-id = ID
schema-name = Нэр
schema-command = Команд
schema-arguments = Аргументууд
schema-environment = Орчны хувьсагчид
schema-working-directory = Ажлын хавтас
schema-shell = Shell
schema-font-family = Фонтын бүл
schema-startup-directory = Эхлэх хавтас
schema-themes = Загварууд
schema-color-scheme = Өнгөний горим
schema-font-size = Фонтын хэмжээ
schema-line-height = Мөрийн өндөр
schema-cursor-style = Курсорын хэв
schema-cursor-blink = Курсор анивчих
schema-custom-themes = Захиалгат загварууд
schema-foreground = Урд өнгө
schema-background = Дэвсгэр
schema-cursor = Курсор
schema-ansi-colors = ANSI өнгөнүүд
schema-keymap = Товчны зураглал
schema-explorer = Хөтөч
schema-visible = Харагдах
schema-language-servers = Хэлний серверүүд
schema-servers = Серверүүд
schema-language-id = Хэлний ID
schema-root-markers = Үндсэн хавтасны тэмдэглэгээнүүд
schema-output-directory = Гаралтын хавтас

menu-scene = Үзэгдэл
menu-layout = Байршил
menu-terminal = Терминал
menu-browser = Хөтөч
menu-service = Үйлчилгээ
menu-bookmark = Хавчуурга
menu-edit = Засах

layout-knowledge = Мэдлэг
layout-open-knowledge = Мэдлэгийг нээх
layout-open-welcome-knowledge = Мэдлэгт тавтай морилыг нээх
layout-open-path = { $path } нээх
layout-fold-knowledge = Мэдлэгийг хураах
layout-unfold-knowledge = Мэдлэгийг дэлгэх
layout-bookmarks = Хавчуургууд
layout-new-folder = Шинэ хавтас
layout-add-to-bookmarks = Хавчуургад нэмэх
layout-move-to-bookmarks = Хавчуурга руу зөөх
layout-stack-number = Стек { $number }
layout-fold-stack = Стекийг хураах
layout-unfold-stack = Стекийг дэлгэх
layout-close-stack = Стекийг хаах
layout-bookmark-in = { $folder } дотор хавчуулах

common-cancel = Цуцлах
common-delete = Устгах
common-save = Хадгалах
common-rename = Нэр өөрчлөх
common-expand = Дэлгэх
common-collapse = Хураах
common-loading = Ачаалж байна…
common-error = Алдаа
common-output = Гаралт
common-pending = Хүлээгдэж байна
common-current = одоогийн
common-stop = Зогсоох
services-command = Vmux үйлчилгээ
services-uptime-seconds = { $seconds } сек
services-uptime-minutes = { $minutes } мин { $seconds } сек
services-uptime-hours = { $hours } цаг { $minutes } мин
services-uptime-days = { $days } өдөр { $hours } цаг

error-page-failed-load = Хуудсыг ачаалж чадсангүй
error-page-not-found = Хуудас олдсонгүй
error-unknown-host = Үл мэдэгдэх Vmux апп хост: { $host }

history-title = Түүх

command-new-app-chat = Шинэ { $provider }/{ $model } чат (Апп)
command-interactive-mode-user = Scene > Интерактив горим > Хэрэглэгч
command-interactive-mode-player = Scene > Интерактив горим > Тоглуулагч
command-minimize-window = Layout > Цонх > Жижигрүүлэх
command-toggle-layout = Layout > Layout > Байршил солих
command-close-tab = Layout > Таб > Табы хаах
command-new-task = Layout > Таб > Шинэ даалгавар…
command-next-tab = Layout > Таб > Дараагийн таб
command-prev-tab = Layout > Таб > Өмнөх таб
command-rename-tab = Layout > Таб > Табы нэрлэх
command-tab-select-1 = Layout > Таб > 1-р табыг сонгох
command-tab-select-2 = Layout > Таб > 2-р табыг сонгох
command-tab-select-3 = Layout > Таб > 3-р табыг сонгох
command-tab-select-4 = Layout > Таб > 4-р табыг сонгох
command-tab-select-5 = Layout > Таб > 5-р табыг сонгох
command-tab-select-6 = Layout > Таб > 6-р табыг сонгох
command-tab-select-7 = Layout > Таб > 7-р табыг сонгох
command-tab-select-8 = Layout > Таб > 8-р табыг сонгох
command-tab-select-last = Layout > Таб > Сүүлийн табыг сонгох
command-close-pane = Layout > Pane > Pane хаах
command-select-pane-left = Layout > Pane > Зүүн Pane-г сонгох
command-select-pane-right = Layout > Pane > Баруун Pane-г сонгох
command-select-pane-up = Layout > Pane > Дээд Pane-г сонгох
command-select-pane-down = Layout > Pane > Доод Pane-г сонгох
command-swap-pane-prev = Layout > Pane > Pane-г өмнөхтэй солих
command-swap-pane-next = Layout > Pane > Pane-г дараагийнтай солих
command-equalize-pane-size = Layout > Pane > Pane-уудын хэмжээг тэнцүүлэх
command-resize-pane-left = Layout > Pane > Pane-г зүүн тийш хэмжээг өөрчлөх
command-resize-pane-right = Layout > Pane > Pane-г баруун тийш хэмжээг өөрчлөх
command-resize-pane-up = Layout > Pane > Pane-г дээш хэмжээг өөрчлөх
command-resize-pane-down = Layout > Pane > Pane-г доош хэмжээг өөрчлөх
command-stack-close = Layout > Stack > Stack хаах
command-stack-next = Layout > Stack > Дараагийн Stack
command-stack-previous = Layout > Stack > Өмнөх Stack
command-stack-reopen = Layout > Stack > Хаасан хуудсыг дахин нээх
command-stack-swap-prev = Layout > Stack > Stack-г зүүн тийш зөөх
command-stack-swap-next = Layout > Stack > Stack-г баруун тийш зөөх
command-space-open = Layout > Space > Space-ууд
command-terminal-close = Terminal > Терминалыг хаах
command-terminal-next = Terminal > Дараагийн терминал
command-terminal-prev = Terminal > Өмнөх терминал
command-terminal-clear = Terminal > Терминалыг цэвэрлэх
command-browser-prev-page = Browser > Навигаци > Буцах
command-browser-next-page = Browser > Навигаци > Урагшлах
command-browser-reload = Browser > Навигаци > Дахин ачаалах
command-browser-hard-reload = Browser > Навигаци > Бүрэн дахин ачаалах
command-open-in-place = Browser > Нээх > Энд нээх
command-open-in-new-stack = Browser > Нээх > Шинэ Stack-д нээх
command-open-in-pane-top = Browser > Нээх > Дээд Pane-д нээх
command-open-in-pane-right = Browser > Нээх > Баруун Pane-д нээх
command-open-in-pane-bottom = Browser > Нээх > Доод Pane-д нээх
command-open-in-pane-left = Browser > Нээх > Зүүн Pane-д нээх
command-open-in-new-tab = Browser > Нээх > Шинэ табад нээх
command-open-in-new-space = Browser > Нээх > Шинэ Space-д нээх
command-browser-zoom-in = Browser > Харагдац > Томруулах
command-browser-zoom-out = Browser > Харагдац > Жижигрүүлэх
command-browser-zoom-reset = Browser > Харагдац > Бодит хэмжээ
command-browser-dev-tools = Browser > Харагдац > Хөгжүүлэгчийн хэрэгсэл
command-browser-open-command-bar = Browser > Мөр > Командын мөр
command-browser-open-page-in-command-bar = Browser > Мөр > Хуудас засах
command-browser-open-path-bar = Browser > Мөр > Зам чиглүүлэгч
command-browser-open-commands = Browser > Мөр > Командууд
command-browser-open-history = Browser > Мөр > Түүх
command-service-open = Service > Үйлчилгээний хяналтыг нээх
command-bookmark-toggle-active = Bookmark > Хуудсыг хавчуургалах
command-bookmark-pin-active = Bookmark > Хуудсыг тогтоох

layout-tab = Таб
layout-no-stacks = Stack алга
layout-loading = Ачаалж байна…
layout-no-markdown-files = Markdown файл алга
layout-empty-folder = Хоосон хавтас
layout-worktree = worktree
layout-folder-name = Хавтасны нэр
layout-no-pins-bookmarks = Тогтоосон хуудас эсвэл хавчуурга алга
layout-move-to = { $folder } руу зөөх
layout-bookmark-current-page = Одоогийн хуудсыг хавчуургалах
layout-rename-folder = Хавтсыг нэрлэх
layout-remove-folder = Хавтсыг устгах
layout-update-downloading = Шинэчлэл татаж байна
layout-update-installing = Шинэчлэл суулгаж байна…
layout-update-ready = Шинэ хувилбар бэлэн
layout-restart-update = Шинэчлэхийн тулд дахин эхлүүлэх

agent-preparing = Агент бэлдэж байна…
agent-send-all-queued = Дараалалд буй бүх хүсэлтийг одоо илгээх (Esc)
agent-send = Илгээх (Enter)
agent-ready = Та бэлэн үед бэлэн.
agent-loading-older = Хуучин зурвасуудыг ачаалж байна…
agent-load-older = Хуучин зурвасуудыг ачаалах
agent-continued-from = { $source }-оос үргэлжлүүлсэн
agent-older-context-omitted = хуучин контекстыг орхисон
agent-interrupted = тасалдсан
agent-allow-tool = { $tool }-ийг зөвшөөрөх үү?
agent-deny = Татгалзах
agent-allow-always = Үргэлж зөвшөөрөх
agent-allow = Зөвшөөрөх
agent-loading-sessions = Сешнүүдийг ачаалж байна…
agent-no-resumable-sessions = Үргэлжлүүлэх боломжтой сешн олдсонгүй
agent-no-matching-sessions = Тохирох сешн алга
agent-no-matching-models = Тохирох модель алга
agent-choice-help = ↑/↓ эсвэл Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Repository хавтас сонгох
agent-choose-repository-detail = Агент ашиглах локал Git repository-г сонгоно уу.
agent-choosing = Сонгож байна…
agent-choose-folder = Хавтас сонгох
agent-queued = дараалалд
agent-attached = Хавсаргасан:
agent-cancel-queued = Дараалал дахь хүсэлтийг цуцлах
agent-resume-queued = Дараалал дахь хүсэлтүүдийг үргэлжлүүлэх
agent-clear-queue = Дарааллыг цэвэрлэх
agent-send-all-now = бүгдийг одоо илгээх
agent-choose-option = Дээрээс сонголт хийнэ үү
agent-loading-media = Медиа ачаалж байна…
agent-no-matching-media = Тохирох медиа алга
agent-prompt-context = Хүсэлтийн контекст
agent-details = Дэлгэрэнгүй
agent-path = Зам
agent-tool = Хэрэгсэл
agent-server = Сервер
agent-bytes = { $count } байт
agent-worked-for = { $duration } ажиллав
agent-worked-for-steps = { $count ->
    [one] { $duration } ажиллав · 1 алхам
   *[other] { $duration } ажиллав · { $count } алхам
}
agent-tool-guardian-review = Guardian хяналт
agent-tool-read-files = Файлууд уншсан
agent-tool-viewed-image = Зураг үзсэн
agent-tool-used-browser = Хөтөч ашигласан
agent-tool-searched-files = Файлуудаас хайсан
agent-tool-ran-commands = Командууд ажиллуулсан
agent-thinking = Бодож байна
agent-subagent = Дэд агент
agent-prompt = Хүсэлт
agent-thread = Хэлхээ
agent-parent = Эцэг
agent-children = Хүүхдүүд
agent-call = Дуудлага
agent-raw-event = Түүхий эвент
agent-plan = Төлөвлөгөө
agent-tasks = { $count ->
    [one] 1 даалгавар
   *[other] { $count } даалгавар
}
agent-edited = Зассан
agent-reconnecting = Дахин холбогдож байна { $attempt }/{ $total }
agent-status-running = Ажиллаж байна
agent-status-done = Дууссан
agent-status-failed = Амжилтгүй
agent-status-pending = Хүлээгдэж байна
agent-slash-attach-files = Файл хавсаргах
agent-slash-resume-session = Өмнөх сешнийг үргэлжлүүлэх
agent-slash-select-model = Модель сонгох
agent-slash-continue-cli = Энэ сешнийг CLI-д үргэлжлүүлэх
agent-session-just-now = дөнгөж сая
agent-session-minutes-ago = { $count } мин өмнө
agent-session-hours-ago = { $count } цаг өмнө
agent-session-days-ago = { $count } өдөр өмнө
agent-working-working = Ажиллаж байна
agent-working-thinking = Бодож байна
agent-working-pondering = Эргэцүүлж байна
agent-working-noodling = Тунгааж байна
agent-working-percolating = Боловсруулж байна
agent-working-conjuring = Ургуулан бодож байна
agent-working-cooking = Найруулж байна
agent-working-brewing = Исгэж байна
agent-working-musing = Бясалгаж байна
agent-working-ruminating = Эргэцүүлэн бодож байна
agent-working-scheming = Төлөвлөж байна
agent-working-synthesizing = Нэгтгэж байна
agent-working-tinkering = Оролдож байна
agent-working-churning = Боловсруулж байна
agent-working-vibing = Хэмнэлд орж байна
agent-working-simmering = Зөөлөн буцалгаж байна
agent-working-crafting = Урлаж байна
agent-working-divining = Таамаглаж байна
agent-working-mulling = Тунгааж байна
agent-working-spelunking = Гүн ухаж байна

editor-toggle-explorer = Explorer-г сэлгэх (Cmd+B)
editor-unsaved = хадгалаагүй
editor-rendered-markdown = Шууд засвартай Markdown дүрслэл
editor-note = Тэмдэглэл
editor-source-editor = Эх кодын редактор
editor-editor = Редактор
editor-git-diff = Git diff
editor-diff = Diff
editor-tidy = Цэгцлэх
editor-always = Үргэлж
editor-unchanged-previews = { $count ->
    [one] ✦ 1 өөрчлөгдөөгүй урьдчилсан харагдац
   *[other] ✦ { $count } өөрчлөгдөөгүй урьдчилсан харагдац
}
editor-open-externally = Гадна аппад нээх
editor-changed-line = Өөрчлөгдсөн мөр
editor-go-to-definition = Тодорхойлолт руу очих
editor-find-references = Ашиглалтуудыг олох
editor-references = { $count ->
    [one] 1 ашиглалт
   *[other] { $count } ашиглалт
}
editor-lsp-starting = { $server } эхэлж байна…
editor-lsp-not-installed = { $server } — суулгаагүй
editor-explorer = Explorer
editor-open-editors = Нээлттэй редакторууд
editor-outline = Бүтэц
editor-new-file = Шинэ файл
editor-new-folder = Шинэ хавтас
editor-delete-confirm = “{ $name }”-г устгах уу? Үүнийг буцаах боломжгүй.
editor-created-folder = { $name } хавтас үүсгэлээ
editor-created-file = { $name } файл үүсгэлээ
editor-renamed-to = { $name } болгож нэрлэв
editor-deleted = { $name } устгалаа
editor-failed-decode-image = Зургийг тайлж чадсангүй
editor-preview-large-image = зураг (урьдчилан харахад хэт том)
editor-preview-binary = бинар
editor-preview-file = файл

git-status-clean = цэвэр
git-status-modified = өөрчлөгдсөн
git-status-staged = stage хийсэн
git-status-staged-modified = stage хийсэн*
git-status-untracked = хянагдаагүй
git-status-deleted = устгасан
git-status-conflict = зөрчил
git-accept-all = ✓ бүгдийг зөвшөөрөх
git-unstage = Stage-ээс гаргах
git-confirm-deny-all = Бүгдийг татгалзахыг батлах
git-deny-all = ✗ бүгдийг татгалзах
git-commit-message = commit зурвас
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Diff ачаалж байна…
git-no-changes = Харуулах өөрчлөлт алга
git-accept = ✓ зөвшөөрөх
git-deny = ✗ татгалзах
git-show-unchanged-lines = Өөрчлөгдөөгүй { $count } мөрийг харуулах

terminal-loading = Ачаалж байна…
terminal-runs-when-ready = бэлэн болмогц ажиллана · Ctrl+C цэвэрлэнэ · Esc алгасна
terminal-booting = эхэлж байна
terminal-type-command = команд бичнэ үү · бэлэн болмогц ажиллана · Esc алгасна

setup-tagline-claude = Anthropic-ийн код бичих агент, Vmux дотор
setup-tagline-codex = OpenAI-ийн код бичих агент, Vmux дотор
setup-tagline-vibe = Mistral-ийн код бичих агент, Vmux дотор
setup-install-title = { $name } CLI суулгах
setup-homebrew-required = { $command } суулгахад Homebrew шаардлагатай бөгөөд хараахан тохируулаагүй байна. Vmux эхлээд Homebrew, дараа нь { $name } суулгана.
setup-terminal-instructions = Терминал дээр Return дарж эхлүүлээд, асуухад Mac нууц үгээ оруулна уу.
setup-command-missing = Локал { $command } команд хараахан суулгаагүй тул Vmux энэ хуудсыг нээлээ. Үүнийг авахын тулд доорх командыг ажиллуулна уу.
setup-install-failed = Суулгалт дууссангүй. Дэлгэрэнгүйг терминалаас шалгаад дахин оролдоно уу.
setup-installing = Суулгаж байна…
setup-install-homebrew = Homebrew + { $name } суулгах
setup-run-install = Суулгах командыг ажиллуулах
setup-auto-reload = Vmux үүнийг терминалд ажиллуулж, { $command } бэлэн болохоор дахин ачаална.

debug-title = Дибаг
debug-auto-update = Автомат шинэчлэлт
debug-simulate-update = Шинэчлэл бэлэнг дуурайх
debug-simulate-download = Таталтыг дуурайх
debug-clear-update = Шинэчлэл цэвэрлэх
debug-trigger-restart = Дахин эхлүүлэхийг өдөөх

command-manage-spaces = Спэйсүүдийг удирдах…
command-pane-stack-location = самбар { $pane } / стек { $stack }
command-space-pane-stack-location = { $space } / самбар { $pane } / стек { $stack }
command-terminal-path = Терминал ({ $path })
command-group-interactive-mode = Интерактив горим
command-group-window = Цонх
command-group-tab = Таб
command-group-pane = Самбар
command-group-stack = Стек
command-group-space = Спэйс
command-group-navigation = Навигац
command-group-open = Нээх
command-group-view = Харагдац
command-group-bar = Мөр

menu-close-vmux = Vmux-ийг хаах

agents-terminal-coding-agent = Терминалд суурилсан код бичих агент
