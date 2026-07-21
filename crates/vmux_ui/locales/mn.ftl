common-open = Нээлттэй
common-close = Хаах
common-install = Суулгах
common-uninstall = Устгах
common-update = Шинэчлэх
common-retry = Дахин оролдоно уу
common-refresh = Сэргээх
common-remove = Устгах
common-enable = Идэвхжүүлэх
common-disable = Идэвхгүй болгох
common-new = Шинэ
common-active = идэвхтэй
common-running = гүйж байна
common-done = хийсэн
common-failed = Амжилтгүй
common-installed = Суулгасан
common-items = { $count ->
    [one] { $count } зүйл
   *[other] { $count } зүйл
}
start-title = Эхлэх
start-tagline = Нэг сануулга. Юу ч болсон.

agents-title = Агентууд
agents-search = ACP болон CLI агентуудыг хайх...
agents-empty = Тохирох агент байхгүй
agents-empty-detail = Нэр, ажиллах цаг, эсвэл ACP/CLI гэж оролдоно уу.
agents-install-failed = Суулгаж чадсангүй
agents-updating = Шинэчилж байна...
agents-retrying = Дахин оролдож байна...
agents-preparing = Бэлтгэж байна...

extensions-title = Өргөтгөлүүд
extensions-search = Суулгасан эсвэл Chrome Web Store... хайх
extensions-relaunch = Өргөдөл гаргахын тулд дахин ажиллуулна уу
extensions-empty = Ямар ч өргөтгөл суулгаагүй байна
extensions-no-match = Тохирох өргөтгөл байхгүй байна
extensions-empty-detail = Дээрх Chrome Web Store хайгаад Return дарна уу.
extensions-no-match-detail = Өөр нэр эсвэл өргөтгөлийн ID-г оролдож үзнэ үү.
extensions-on = Асаалттай
extensions-off = Унтраах
extensions-enable-confirm = { $name }-г идэвхжүүлэх үү?
extensions-enable-permissions = { $name }-г идэвхжүүлж, зөвшөөрнө үү:

lsp-title = Хэлний серверүүд
lsp-search = Хэлний сервер, линтер, форматлагч хайх...
lsp-loading = Каталогийг ачаалж байна...
lsp-empty = Тохирох хэлний сервер алга
lsp-empty-detail = Өөр хэл, линтер эсвэл форматлагчаар оролдоно уу.
lsp-needs = { $tool } хэрэгтэй
lsp-status-available = Боломжтой
lsp-status-on-path = PATH дээр
lsp-status-installing = Суулгаж байна…
lsp-status-installed = Суулгасан
lsp-status-outdated = Шинэчлэлт боломжтой
lsp-status-running = Гүйж байна
lsp-status-failed = Амжилтгүй

spaces-title = Орон зай
spaces-new-placeholder = Орон зайн шинэ нэр
spaces-empty = Хоосон зай алга
spaces-default-name = Зай { $number }
spaces-tabs = { $count ->
    [one] 1 таб
   *[other] { $count } таб
}
spaces-delete = Зай устгах

team-title = Баг
team-just-you = Энэ орон зайд зөвхөн чи
team-agents = { $count ->
    [one] Та болон 1 төлөөлөгч
   *[other] Та болон { $count } агентууд
}
team-empty = Одоохондоо энд хэн ч алга
team-you = Та
team-agent = Агент

services-title = Арын дэвсгэр үйлчилгээ
services-processes = { $count ->
    [one] 1 процесс
   *[other] { $count } процессууд
}
services-kill-all = Бүгдийг нь ал
services-not-running = Үйлчилгээ ажиллахгүй байна
services-start-with = Эхлэх:
services-empty = Идэвхтэй процесс байхгүй
services-filter = Процессуудыг шүүх...
services-no-match = Тохирох процесс байхгүй
services-connected = Холбогдсон
services-disconnected = Салгасан
services-attached = хавсаргасан
services-kill = Алах
services-memory = Санах ой
services-size = Хэмжээ
services-shell = Бүрхүүл

error-title = Алдаа

history-search = Хайлтын түүх
history-clear-all = Бүгдийг арилгах
history-clear-confirm = Бүх түүхийг арилгах уу?
history-clear-warning = Үүнийг буцаах боломжгүй.
history-cancel = Цуцлах
history-today = Өнөөдөр
history-yesterday = Өчигдөр
history-days-ago = { $count } хоногийн өмнө
history-day-offset = Өдөр -{ $count }

settings-title = Тохиргоо
settings-loading = Тохиргоог ачаалж байна...
settings-stored = ~/.vmux/settings.ron-д хадгалагдсан
settings-other = Бусад
settings-software-update = Програм хангамжийн шинэчлэл
settings-check-updates = Шинэчлэлтүүдийг шалгана уу
settings-check-updates-hint = Эхлэх үед автоматаар болон автомат шинэчлэлт идэвхжсэн үед цаг тутамд шалгадаг.
settings-update-unavailable = Боломжгүй
settings-update-unavailable-hint = Шинэчлэгч нь энэ бүтцэд ороогүй болно.
settings-update-checking = Шалгаж байна...
settings-update-checking-hint = Шинэчлэлтүүдийг шалгаж байна...
settings-update-check-again = Дахин шалгана уу
settings-update-current = Vmux шинэчлэгдсэн.
settings-update-downloading = Татаж авч байна…
settings-update-downloading-hint = Vmux { $version } татаж авч байна…
settings-update-installing = Суулгаж байна…
settings-update-installing-hint = Vmux { $version } суулгаж байна…
settings-update-ready = Шинэчлэлт бэлэн боллоо
settings-update-ready-hint = Vmux { $version } бэлэн боллоо. Хэрэглэхийн тулд дахин эхлүүлнэ үү.
settings-update-try-again = Дахин оролдоно уу
settings-update-failed = Шинэчлэлтүүдийг шалгах боломжгүй байна.
settings-item = Зүйл
settings-item-number = Зүйл { $number }
settings-press-key = Товч дарна уу...
settings-saved = Хадгалсан
settings-record-key = Шинэ товчлуурын хослол бичихийн тулд товшино уу

tray-open-window = Нээлттэй цонх
tray-close-window = Цонхыг хаах
tray-pause-recording = Бичлэгийг түр зогсоох
tray-resume-recording = Бичлэгийг үргэлжлүүлэх
tray-finish-recording = Бичлэг дуусгах
tray-quit = Vmux гарах

composer-attach-files = Файл хавсаргах (/upload)
composer-remove-attachment = Хавсралтыг устгана уу

layout-back = Буцах
layout-forward = Урагшаа
layout-reload = Дахин ачаалах
layout-bookmark-page = Энэ хуудсыг тэмдэглэ
layout-remove-bookmark = Хавчуурга арилгах
layout-pin-page = Энэ хуудсыг бэхлэх
layout-unpin-page = Энэ хуудсыг буулгана уу
layout-manage-extensions = Өргөтгөлүүдийг удирдах
layout-new-stack = Шинэ стек
layout-close-tab = Табыг хаах
layout-bookmark = Хавчуурга
layout-pin = Pin
layout-new-tab = Шинэ таб
layout-team = Баг

command-switch-space = Зай солих...
command-search-ask = Хайх эсвэл асуух...
command-new-tab-placeholder = URL хайх буюу бичих, эсвэл Терминал…-г сонгоно уу.
command-placeholder = Командын хувьд URL, хайлтын таб, эсвэл > гэж бичнэ үү...
command-composer-placeholder = Командын хувьд /, медиагийн хувьд @ гэж бичнэ үү
command-send = Илгээх (Enter)
command-terminal = Терминал
command-open-terminal = Терминал дээр нээнэ үү
command-stack = Стек
command-tabs = { $count ->
    [one] 1 таб
   *[other] { $count } таб
}
command-prompt = Шуурхай
command-new-tab = Шинэ таб
command-search = Хайх
command-open-value = "{ $value }"-г нээх
command-search-value = "{ $value }" хайх

schema-appearance = Гадаад төрх
schema-general = Генерал
schema-layout = Зохион байгуулалт
schema-layout-detail = Цонх, цонх, хажуугийн самбар, фокусын цагираг.
schema-agent = Агент
schema-agent-detail = Агентын зан төлөв ба хэрэгслийн зөвшөөрөл.
schema-shortcuts = Товчлолууд
schema-shortcuts-detail = Зөвхөн унших боломжтой. Холболтыг өөрчлөхийн тулд settings.ron-г шууд засна уу.
schema-terminal = Терминал
schema-browser = Хөтөч
schema-mode = Горим
schema-mode-detail = Вэб хуудасны өнгөний схем. Төхөөрөмж таны системийг дагаж байна.
schema-device = Төхөөрөмж
schema-light = Гэрэл
schema-dark = Харанхуй
schema-language = Хэл
schema-language-detail = Систем, en-US, ja, эсвэл тохирох ~/.vmux/locales/<tag>.ftl каталог бүхий дурын BCP 47 шошго ашиглана уу.
schema-auto-update = Автоматаар шинэчлэх
schema-auto-update-detail = Шинэчлэлтүүдийг эхлүүлэх үед болон цаг тутамд шалгаж суулгаарай.
schema-startup-url = Эхлүүлэх URL
schema-startup-url-detail = Хоосон нь тушаалын мөрийг нээнэ.
schema-search-engine = Хайлтын систем
schema-search-engine-detail = Эхлэл болон тушаалын мөрөөс вэб хайлт хийхэд ашигладаг.
schema-window = Цонх
schema-pane = Пане
schema-side-sheet = Хажуугийн хуудас
schema-focus-ring = Фокусын бөгж
schema-run-placement = Ажиллуулах байршлыг хүчингүй болгохыг зөвшөөрөх
schema-run-placement-detail = Агентуудад ажиллуулах самбарын горим, чиглэл, зангууг сонгохыг зөвшөөрнө үү.
schema-leader = Удирдагч
schema-leader-detail = Хөвчний товчлолын угтвар товч.
schema-chord-timeout = Хөвчний завсарлага
schema-chord-timeout-detail = Хөвчний угтвар дуусахаас өмнө миллисекунд.
schema-bindings = Холболт
schema-confirm-close = Хаахыг баталгаажуулна уу
schema-confirm-close-detail = Ажиллаж байгаа процессоор терминалыг хаахаас өмнө сануулга.
schema-default-theme = Өгөгдмөл загвар
schema-default-theme-detail = Сэдвүүдийн жагсаалтаас идэвхтэй сэдвийн нэр.
