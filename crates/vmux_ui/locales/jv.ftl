common-open = Mbukak
common-close = Nutup
common-install = Instal
common-uninstall = Busak instal
common-update = Nganyari
common-retry = Coba maneh
common-refresh = Refresh
common-remove = Mbusak
common-enable = Aktifake
common-disable = Pateni
common-new = Anyar
common-active = aktif
common-running = mlaku
common-done = rampung
common-failed = Gagal
common-installed = Dipasang
common-items = { $count ->
    [one] { $count } item
   *[other] { $count } item
}
start-title = Miwiti
start-tagline = Siji pituduh. Apa wae, rampung.

agents-title = Agen
agents-search = Telusuri ACP lan CLI agen…
agents-empty = Ora ana agen sing cocog
agents-empty-detail = Coba jeneng, runtime, utawa ACP/CLI.
agents-install-failed = Instal gagal
agents-updating = Nganyari…
agents-retrying = Nyoba maneh…
agents-preparing = Nyiapake…

extensions-title = Ekstensi
extensions-search = Telusuri diinstal utawa Chrome Web Store…
extensions-relaunch = Bukak maneh kanggo nglamar
extensions-empty = Ora ana ekstensi sing diinstal
extensions-no-match = Ora ana ekstensi sing cocog
extensions-empty-detail = Telusuri Chrome Web Store ing ndhuwur banjur pencet Return.
extensions-no-match-detail = Coba jeneng utawa ID ekstensi liyane.
extensions-on = On
extensions-off = Mati
extensions-enable-confirm = Aktifake { $name }?
extensions-enable-permissions = Aktifake { $name } lan ngidini:

lsp-title = Server Basa
lsp-search = Telusuri server basa, linters, formatter…
lsp-loading = Memuat katalog…
lsp-empty = Ora ana server basa sing cocog
lsp-empty-detail = Coba basa liyane, linter, utawa formatter.
lsp-needs = butuh { $tool }
lsp-status-available = kasedhiya
lsp-status-on-path = Ing PATH
lsp-status-installing = Nginstal…
lsp-status-installed = Dipasang
lsp-status-outdated = Nganyari kasedhiya
lsp-status-running = mlaku
lsp-status-failed = Gagal

spaces-title = Spasi
spaces-new-placeholder = Jeneng papan anyar
spaces-empty = Ora ana spasi
spaces-default-name = Angkasa { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
spaces-delete = Mbusak spasi

team-title = tim
team-just-you = Mung sampeyan ing papan iki
team-agents = { $count ->
    [one] Sampeyan lan 1 agen
   *[other] Sampeyan lan { $count } agen
}
team-empty = Durung ana wong kene
team-you = Sampeyan
team-agent = Agen

services-title = Layanan latar mburi
services-processes = { $count ->
    [one] 1 proses
   *[other] { $count } pangolahan
}
services-kill-all = Mateni Kabeh
services-not-running = Layanan ora mlaku
services-start-with = Mulai karo:
services-empty = Ora ana proses aktif
services-filter = Proses Filter…
services-no-match = Ora ana pangolahan sing cocog
services-connected = Nyambung
services-disconnected = Pedhot
services-attached = ditempelake
services-kill = mateni
services-memory = Memori
services-size = Ukuran
services-shell = cangkang

error-title = Kesalahan

history-search = Riwayat telusuran
history-clear-all = Mbusak kabeh
history-clear-confirm = Mbusak kabeh riwayat?
history-clear-warning = Iki ora bisa dibatalake.
history-cancel = Batal
history-today = Dina iki
history-yesterday = wingi
history-days-ago = { $count } dina kepungkur
history-day-offset = Dina -{ $count }

settings-title = Setelan
settings-loading = Setelan dimuat…
settings-stored = Disimpen ing ~/.vmux/settings.ron
settings-other = Liyane
settings-software-update = Nganyari piranti lunak
settings-check-updates = Priksa Update
settings-check-updates-hint = Priksa kanthi otomatis nalika diluncurake lan saben jam yen Nganyari otomatis diaktifake.
settings-update-unavailable = Ora kasedhiya
settings-update-unavailable-hint = Updater ora kalebu ing mbangun iki.
settings-update-checking = Priksa…
settings-update-checking-hint = Priksa nganyari…
settings-update-check-again = Priksa maneh
settings-update-current = Vmux paling anyar.
settings-update-downloading = Ngundhuh…
settings-update-downloading-hint = Ngundhuh Vmux { $version }…
settings-update-installing = Nginstal…
settings-update-installing-hint = Nginstal Vmux { $version }…
settings-update-ready = Nganyari Siap
settings-update-ready-hint = Vmux { $version } wis siyap. Wiwiti maneh kanggo ngetrapake.
settings-update-try-again = Coba maneh
settings-update-failed = Ora bisa mriksa nganyari.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Pencet tombol…
settings-saved = Disimpen
settings-record-key = Klik kanggo ngrekam kombinasi tombol anyar

tray-open-window = Mbukak Window
tray-close-window = Tutup Jendela
tray-pause-recording = Ngaso Rekaman
tray-resume-recording = Resume Rekaman
tray-finish-recording = Rampung Rekaman
tray-quit = Mungkasi Vmux

composer-attach-files = Masang file (/upload)
composer-remove-attachment = Copot lampiran

layout-back = Mbalik
layout-forward = Maju
layout-reload = Muat maneh
layout-bookmark-page = Tetenger kaca iki
layout-remove-bookmark = Mbusak tetenger
layout-pin-page = Pin kaca iki
layout-unpin-page = Copot kaca iki
layout-manage-extensions = Ngatur ekstensi
layout-new-stack = Tumpukan Anyar
layout-close-tab = Nutup tab
layout-bookmark = Tetenger
layout-pin = Pin
layout-new-tab = Tab anyar
layout-team = tim

command-switch-space = Ganti spasi…
command-search-ask = Telusuri utawa takon…
command-new-tab-placeholder = Telusuri utawa ketik URL, utawa pilih Terminal…
command-placeholder = Ketik URL, tab telusuran, utawa > kanggo prentah...
command-composer-placeholder = Ketik / kanggo printah utawa @ kanggo media
command-send = Kirim (Enter)
command-terminal = Terminal
command-open-terminal = Bukak ing Terminal
command-stack = tumpukan
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
command-prompt = Prompt
command-new-tab = Tab anyar
command-search = Nggoleki
command-open-value = Bukak "{ $value }"
command-search-value = Telusuri "{ $value }"

schema-appearance = Penampilan
schema-general = Umum
schema-layout = Tata letak
schema-layout-detail = Jendhela, panel, sidebar, lan dering fokus.
schema-agent = Agen
schema-agent-detail = Perilaku agen lan ijin alat.
schema-shortcuts = Trabasan
schema-shortcuts-detail = Tampilan mung diwaca. Sunting settings.ron langsung kanggo ngganti bindings.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Mode
schema-mode-detail = Skema warna kanggo kaca web. Piranti ngetutake sistem sampeyan.
schema-device = piranti
schema-light = cahya
schema-dark = peteng
schema-language = Basa
schema-language-detail = Gunakake sistem, en-US, ja, utawa tag BCP 47 karo katalog ~/.vmux/locales/<tag>.ftl sing cocog.
schema-auto-update = Nganyari otomatis
schema-auto-update-detail = Priksa lan instal nganyari nalika diluncurake lan saben jam.
schema-startup-url = Wiwiti URL
schema-startup-url-detail = Kosong mbukak command bar prompt.
schema-search-engine = Mesin telusur
schema-search-engine-detail = Digunakake kanggo nggoleki web saka Mulai lan garis perintah.
schema-window = jendhela
schema-pane = Pane
schema-side-sheet = Lembar sisih
schema-focus-ring = Ring fokus
schema-run-placement = Allow placement override run
schema-run-placement-detail = Ayo agen milih mode run panel, arah, lan jangkar.
schema-leader = Pimpinan
schema-leader-detail = Tombol awalan kanggo trabasan kord.
schema-chord-timeout = Chord wektu entek
schema-chord-timeout-detail = Milidetik sadurunge prefiks kord kadaluwarsa.
schema-bindings = Bindings
schema-confirm-close = Konfirmasi cedhak
schema-confirm-close-detail = Prompt sadurunge nutup terminal kanthi proses mlaku.
schema-default-theme = Tema standar
schema-default-theme-detail = Jeneng tema aktif saka dhaptar tema.
