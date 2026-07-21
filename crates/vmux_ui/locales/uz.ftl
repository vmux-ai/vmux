common-open = Ochish
common-close = Yopish
common-install = O‘rnatish
common-uninstall = O‘chirish
common-update = Yangilash
common-retry = Qayta urinish
common-refresh = Yangilash
common-remove = Olib tashlash
common-enable = Yoqish
common-disable = O‘chirish
common-new = Yangi
common-active = faol
common-running = ishlayapti
common-done = tayyor
common-failed = Xato
common-installed = O‘rnatilgan
common-items = { $count ->
    [one] { $count } ta element
   *[other] { $count } ta element
}
start-title = Boshlash
start-tagline = Bitta prompt. Hammasi tayyor.

agents-title = Agentlar
agents-search = ACP va CLI agentlarini qidirish…
agents-empty = Mos agent topilmadi
agents-empty-detail = Nomi, runtime yoki ACP/CLI bo‘yicha urinib ko‘ring.
agents-install-failed = O‘rnatilmadi
agents-updating = Yangilanmoqda…
agents-retrying = Qayta urinilmoqda…
agents-preparing = Tayyorlanmoqda…

extensions-title = Kengaytmalar
extensions-search = O‘rnatilganlar yoki Chrome Web Store’dan qidirish…
extensions-relaunch = Qo‘llash uchun qayta ishga tushiring
extensions-empty = Kengaytma o‘rnatilmagan
extensions-no-match = Mos kengaytma topilmadi
extensions-empty-detail = Yuqoridagi Chrome Web Store’dan qidiring va Enter bosing.
extensions-no-match-detail = Boshqa nom yoki kengaytma ID sini sinab ko‘ring.
extensions-on = Yoqilgan
extensions-off = O‘chirilgan
extensions-enable-confirm = { $name } yoqilsinmi?
extensions-enable-permissions = { $name } yoqilsin va quyidagilarga ruxsat berilsin:

lsp-title = Til serverlari
lsp-search = Til serverlari, linterlar, formatterlarni qidirish…
lsp-loading = Katalog yuklanmoqda…
lsp-empty = Mos til serveri topilmadi
lsp-empty-detail = Boshqa til, linter yoki formatterni sinab ko‘ring.
lsp-needs = { $tool } kerak
lsp-status-available = Mavjud
lsp-status-on-path = PATH’da bor
lsp-status-installing = O‘rnatilmoqda…
lsp-status-installed = O‘rnatilgan
lsp-status-outdated = Yangilanish mavjud
lsp-status-running = Ishlayapti
lsp-status-failed = Xato

spaces-title = Ish maydonlari
spaces-new-placeholder = Yangi ish maydoni nomi
spaces-empty = Ish maydonlari yo‘q
spaces-default-name = Ish maydoni { $number }
spaces-tabs = { $count ->
    [one] 1 ta tab
   *[other] { $count } ta tab
}
spaces-delete = Ish maydonini o‘chirish

team-title = Jamoa
team-just-you = Bu ish maydonida faqat siz
team-agents = { $count ->
    [one] Siz va 1 ta agent
   *[other] Siz va { $count } ta agent
}
team-empty = Bu yerda hali hech kim yo‘q
team-you = Siz
team-agent = Agent

services-title = Fon xizmatlari
services-processes = { $count ->
    [one] 1 ta jarayon
   *[other] { $count } ta jarayon
}
services-kill-all = Hammasini majburan to‘xtatish
services-not-running = Xizmat ishlamayapti
services-start-with = Boshlash:
services-empty = Faol jarayonlar yo‘q
services-filter = Jarayonlarni filtrlash…
services-no-match = Mos jarayon topilmadi
services-connected = Ulangan
services-disconnected = Uzilgan
services-attached = biriktirilgan
services-kill = Majburan to‘xtatish
services-memory = Xotira
services-size = Hajm
services-shell = Shell

error-title = Xato

history-search = Tarixdan qidirish
history-clear-all = Hammasini tozalash
history-clear-confirm = Butun tarix tozalansinmi?
history-clear-warning = Buni ortga qaytarib bo‘lmaydi.
history-cancel = Bekor qilish
history-today = Bugun
history-yesterday = Kecha
history-days-ago = { $count } kun oldin
history-day-offset = Kun -{ $count }

settings-title = Sozlamalar
settings-loading = Sozlamalar yuklanmoqda…
settings-stored = ~/.vmux/settings.ron faylida saqlanadi
settings-other = Boshqa
settings-software-update = Dastur yangilanishi
settings-check-updates = Yangilanishlarni tekshirish
settings-check-updates-hint = Avtoyangilash yoqilgan bo‘lsa, ishga tushganda va har soatda avtomatik tekshiradi.
settings-update-unavailable = Mavjud emas
settings-update-unavailable-hint = Yangilagich bu build tarkibiga kiritilmagan.
settings-update-checking = Tekshirilmoqda…
settings-update-checking-hint = Yangilanishlar tekshirilmoqda…
settings-update-check-again = Qayta tekshirish
settings-update-current = Vmux yangilangan.
settings-update-downloading = Yuklab olinmoqda…
settings-update-downloading-hint = Vmux { $version } yuklab olinmoqda…
settings-update-installing = O‘rnatilmoqda…
settings-update-installing-hint = Vmux { $version } o‘rnatilmoqda…
settings-update-ready = Yangilanish tayyor
settings-update-ready-hint = Vmux { $version } tayyor. Qo‘llash uchun qayta ishga tushiring.
settings-update-try-again = Qayta urinish
settings-update-failed = Yangilanishlarni tekshirib bo‘lmadi.
settings-item = Element
settings-item-number = Element { $number }
settings-press-key = Tugmani bosing…
settings-saved = Saqlandi
settings-record-key = Yangi tugmalar kombinatsiyasini yozish uchun bosing

tray-open-window = Oynani ochish
tray-close-window = Oynani yopish
tray-pause-recording = Yozishni pauza qilish
tray-resume-recording = Yozishni davom ettirish
tray-finish-recording = Yozishni tugatish
tray-quit = Vmux’dan chiqish

composer-attach-files = Fayllarni biriktirish (/upload)
composer-remove-attachment = Biriktirmani olib tashlash

layout-back = Orqaga
layout-forward = Oldinga
layout-reload = Qayta yuklash
layout-bookmark-page = Bu sahifani xatcho‘pga qo‘shish
layout-remove-bookmark = Xatcho‘pni olib tashlash
layout-pin-page = Bu sahifani mahkamlash
layout-unpin-page = Bu sahifani mahkamlashdan chiqarish
layout-manage-extensions = Kengaytmalarni boshqarish
layout-new-stack = Yangi qatlam
layout-close-tab = Tabni yopish
layout-bookmark = Xatcho‘p
layout-pin = Mahkamlash
layout-new-tab = Yangi tab
layout-team = Jamoa

command-switch-space = Ish maydonini almashtirish…
command-search-ask = Qidirish yoki so‘rash…
command-new-tab-placeholder = Qidiring yoki URL kiriting, yoki Terminalni tanlang…
command-placeholder = URL kiriting, tablarni qidiring yoki buyruqlar uchun > bosing…
command-composer-placeholder = Buyruqlar uchun / yoki media uchun @ kiriting
command-send = Yuborish (Enter)
command-terminal = Terminal
command-open-terminal = Terminalda ochish
command-stack = Qatlam
command-tabs = { $count ->
    [one] 1 ta tab
   *[other] { $count } ta tab
}
command-prompt = Prompt
command-new-tab = Yangi tab
command-search = Qidirish
command-open-value = “{ $value }”ni ochish
command-search-value = “{ $value }”ni qidirish

schema-appearance = Ko‘rinish
schema-general = Umumiy
schema-layout = Joylashuv
schema-layout-detail = Oyna, panellar, yon panel va fokus halqasi.
schema-agent = Agent
schema-agent-detail = Agent xatti-harakati va asboblar ruxsatlari.
schema-shortcuts = Qisqa tugmalar
schema-shortcuts-detail = Faqat ko‘rish uchun. Bog‘lamalarni o‘zgartirish uchun settings.ron faylini bevosita tahrirlang.
schema-terminal = Terminal
schema-browser = Brauzer
schema-mode = Rejim
schema-mode-detail = Veb-sahifalar rang sxemasi. Qurilma tizimingizga ergashadi.
schema-device = Qurilma
schema-light = Yorug‘
schema-dark = Qorong‘i
schema-language = Til
schema-language-detail = Tizim tili, en-US, ja yoki mos ~/.vmux/locales/<tag>.ftl katalogiga ega istalgan BCP 47 tegidan foydalaning.
schema-auto-update = Avtoyangilash
schema-auto-update-detail = Ishga tushganda va har soatda yangilanishlarni tekshirish va o‘rnatish.
schema-startup-url = Boshlang‘ich URL
schema-startup-url-detail = Bo‘sh bo‘lsa, buyruqlar paneli prompti ochiladi.
schema-search-engine = Qidiruv tizimi
schema-search-engine-detail = Start va buyruqlar panelidan veb-qidiruvlar uchun ishlatiladi.
schema-window = Oyna
schema-pane = Panel
schema-side-sheet = Yon varaq
schema-focus-ring = Fokus halqasi
schema-run-placement = Ishga tushirish joylashuvini almashtirishga ruxsat berish
schema-run-placement-detail = Agentlarga ishga tushirish paneli rejimi, yo‘nalishi va langarini tanlashga ruxsat berish.
schema-leader = Lider
schema-leader-detail = Chord qisqa tugmalari uchun prefiks tugma.
schema-chord-timeout = Chord vaqti tugashi
schema-chord-timeout-detail = Chord prefiksi tugashigacha bo‘lgan millisekundlar.
schema-bindings = Bog‘lamalar
schema-confirm-close = Yopishni tasdiqlash
schema-confirm-close-detail = Ishlayotgan jarayoni bor terminalni yopishdan oldin so‘rash.
schema-default-theme = Standart mavzu
schema-default-theme-detail = Mavzular ro‘yxatidagi faol mavzu nomi.
