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
