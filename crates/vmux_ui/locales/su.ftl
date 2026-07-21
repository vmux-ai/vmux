common-open = Buka
common-close = Tutup
common-install = Pasang
common-uninstall = Cabut
common-update = Apdet
common-retry = Cobaan Deui
common-refresh = Segarkeun
common-remove = Hapus
common-enable = Aktipkeun
common-disable = Nonaktipkeun
common-new = Anyar
common-active = aktip
common-running = jalan
common-done = réngsé
common-failed = Gagal
common-installed = Kapasang
common-items = { $count ->
    [one] { $count } item
   *[other] { $count } item
}
start-title = Mimiti
start-tagline = Hiji paréntah. Naon waé, réngsé.

agents-title = Agén
agents-search = Teang agén ACP sareng CLI…
agents-empty = Teu aya agén nu cocog
agents-empty-detail = Coba ngaran, runtime, atawa ACP/CLI.
agents-install-failed = Pamasangan gagal
agents-updating = Ngapdet…
agents-retrying = Nyobaan deui…
agents-preparing = Nyiapkeun…

extensions-title = Éksténsi
extensions-search = Teang anu kapasang atawa Chrome Web Store…
extensions-relaunch = Jalankeun deui pikeun nerapkeun
extensions-empty = Teu aya éksténsi anu kapasang
extensions-no-match = Teu aya éksténsi nu cocog
extensions-empty-detail = Teang di Chrome Web Store di luhur teras pencét Return.
extensions-no-match-detail = Coba ngaran atawa ID éksténsi séjén.
extensions-on = Hirup
extensions-off = Pareup
extensions-enable-confirm = Aktipkeun { $name }?
extensions-enable-permissions = Aktipkeun { $name } sareng idinkeun:

lsp-title = Server Basa
lsp-search = Teang server basa, linter, formatter…
lsp-loading = Ngamuat katalog…
lsp-empty = Teu aya server basa nu cocog
lsp-empty-detail = Coba basa, linter, atawa formatter séjén.
lsp-needs = butuh { $tool }
lsp-status-available = Sadia
lsp-status-on-path = Dina PATH
lsp-status-installing = Masang…
lsp-status-installed = Kapasang
lsp-status-outdated = Apdet sadia
lsp-status-running = Jalan
lsp-status-failed = Gagal

spaces-title = Rohangan
spaces-new-placeholder = Ngaran rohangan anyar
spaces-empty = Teu aya rohangan
spaces-default-name = Rohangan { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
spaces-delete = Hapus rohangan

team-title = Tim
team-just-you = Ngan anjeun di rohangan ieu
team-agents = { $count ->
    [one] Anjeun sareng 1 agén
   *[other] Anjeun sareng { $count } agén
}
team-empty = Teu aya saha di dieu
team-you = Anjeun
team-agent = Agén

services-title = Layanan Latar
services-processes = { $count ->
    [one] 1 prosés
   *[other] { $count } prosés
}
services-kill-all = Eureunkeun Sadayana
services-not-running = Layanan teu jalan
services-start-with = Mimitian kalayan:
services-empty = Teu aya prosés aktip
services-filter = Saring prosés…
services-no-match = Teu aya prosés nu cocog
services-connected = Nyambung
services-disconnected = Teu Nyambung
services-attached = katempel
services-kill = Eureunkeun
services-memory = Mémori
services-size = Ukuran
services-shell = Shell

error-title = Kasalahan

history-search = Teang riwayat
history-clear-all = Hapus sadayana
history-clear-confirm = Hapus sadaya riwayat?
history-clear-warning = Ieu teu bisa dibalikeun.
history-cancel = Batal
history-today = Kiwari
history-yesterday = Kamari
history-days-ago = { $count } poé kapengker
history-day-offset = Poé -{ $count }

settings-title = Setelan
settings-loading = Ngamuat setelan…
settings-stored = Disimpen di ~/.vmux/settings.ron
settings-other = Séjénna
settings-software-update = Apdet Parangkat Lunak
settings-check-updates = Pariksa Apdet
settings-check-updates-hint = Mariksa otomatis nalika diluncurkeun sareng unggal jam nalika Apdet Otomatis diaktipkeun.
settings-update-unavailable = Teu Sadia
settings-update-unavailable-hint = Updater teu kaasup dina versi ieu.
settings-update-checking = Mariksa…
settings-update-checking-hint = Mariksa apdet…
settings-update-check-again = Pariksa Deui
settings-update-current = Vmux parantos apdet.
settings-update-downloading = Ngaunduh…
settings-update-downloading-hint = Ngaunduh Vmux { $version }…
settings-update-installing = Masang…
settings-update-installing-hint = Masang Vmux { $version }…
settings-update-ready = Apdet Siap
settings-update-ready-hint = Vmux { $version } siap. Restart pikeun nerapkeun.
settings-update-try-again = Cobaan Deui
settings-update-failed = Teu bisa mariksa apdet.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Pencét tombol…
settings-saved = Disimpen
settings-record-key = Klik pikeun ngarékam kombinasi tombol anyar

tray-open-window = Buka Jandéla
tray-close-window = Tutup Jandéla
tray-pause-recording = Jeda Rékaman
tray-resume-recording = Teruskeun Rékaman
tray-finish-recording = Réngsé Rékaman
tray-quit = Kaluar Vmux

composer-attach-files = Lampirkeun file (/upload)
composer-remove-attachment = Hapus lampiran

layout-back = Balik
layout-forward = Maju
layout-reload = Muat Deui
layout-bookmark-page = Bookmark kaca ieu
layout-remove-bookmark = Hapus bookmark
layout-pin-page = Sematkeun kaca ieu
layout-unpin-page = Cabut sematan kaca ieu
layout-manage-extensions = Atur éksténsi
layout-new-stack = Tumpukan Anyar
layout-close-tab = Tutup tab
layout-bookmark = Bookmark
layout-pin = Sematkeun
layout-new-tab = Tab anyar
layout-team = Tim

command-switch-space = Ganti rohangan…
command-search-ask = Teang atawa tanya…
command-new-tab-placeholder = Teang atawa ketik URL, atawa pilih Terminal…
command-placeholder = Ketik URL, teang tab, atawa > pikeun paréntah…
command-composer-placeholder = Ketik / pikeun paréntah atawa @ pikeun média
command-send = Kirim (Enter)
command-terminal = Terminal
command-open-terminal = Buka di Terminal
command-stack = Tumpukan
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
command-prompt = Pituduh
command-new-tab = Tab anyar
command-search = Teang
command-open-value = Buka "{ $value }"
command-search-value = Teang "{ $value }"

schema-appearance = Tampilan
schema-general = Umum
schema-layout = Tata Letak
schema-layout-detail = Jandéla, panel, sidebar, sareng focus ring.
schema-agent = Agén
schema-agent-detail = Paripolah agén sareng idin alat.
schema-shortcuts = Jalan Pintas
schema-shortcuts-detail = Tampilan baca wungkul. Édit settings.ron langsung pikeun ngarobah binding.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Modeu
schema-mode-detail = Skéma warna pikeun kaca wéb. Alat nuturkeun sistem anjeun.
schema-device = Alat
schema-light = Caang
schema-dark = Poék
schema-language = Basa
schema-language-detail = Paké sistem, en-US, ja, atawa tag BCP 47 mana waé kalayan katalog ~/.vmux/locales/<tag>.ftl nu cocog.
schema-auto-update = Apdet Otomatis
schema-auto-update-detail = Mariksa sareng masang apdet nalika diluncurkeun sareng unggal jam.
schema-startup-url = URL Mimiti
schema-startup-url-detail = Kosong muka paréntah bar.
schema-search-engine = Mesin Teang
schema-search-engine-detail = Dipaké pikeun teang wéb ti Mimiti sareng command bar.
schema-window = Jandéla
schema-pane = Panel
schema-side-sheet = Lambaran Sisi
schema-focus-ring = Focus ring
schema-run-placement = Idinkeun ogantian panempatan jalankeun
schema-run-placement-detail = Ngidinan agén milih modeu panel jalankeun, arah, sareng jangkar.
schema-leader = Leader
schema-leader-detail = Tombol awalan pikeun jalan pintas chord.
schema-chord-timeout = Waktos chord
schema-chord-timeout-detail = Millisékén sateuacan awalan chord kadaluwarsa.
schema-bindings = Binding
schema-confirm-close = Konfirmasi tutup
schema-confirm-close-detail = Menta konfirmasi sateuacan nutup terminal kalayan prosés anu jalan.
schema-default-theme = Téma Standar
schema-default-theme-detail = Ngaran téma aktip tina daptar téma.
