common-open = Buka
common-close = Tutup
common-install = Instal
common-uninstall = Hapus instalasi
common-update = Perbarui
common-retry = Coba lagi
common-refresh = Segarkan
common-remove = Hapus
common-enable = Aktifkan
common-disable = Nonaktifkan
common-new = Baru
common-active = aktif
common-running = berjalan
common-done = selesai
common-failed = Gagal
common-installed = Terinstal
common-items = { $count ->
    [one] { $count } item
   *[other] { $count } item
}
start-title = Mulai
start-tagline = Satu prompt. Apa pun beres.

agents-title = Agen
agents-search = Cari agen ACP dan CLI…
agents-empty = Tidak ada agen yang cocok
agents-empty-detail = Coba nama, runtime, atau ACP/CLI.
agents-install-failed = Instalasi gagal
agents-updating = Memperbarui…
agents-retrying = Mencoba lagi…
agents-preparing = Menyiapkan…

extensions-title = Ekstensi
extensions-search = Cari yang terinstal atau di Chrome Web Store…
extensions-relaunch = Buka ulang untuk menerapkan
extensions-empty = Belum ada ekstensi terinstal
extensions-no-match = Tidak ada ekstensi yang cocok
extensions-empty-detail = Cari di Chrome Web Store di atas, lalu tekan Return.
extensions-no-match-detail = Coba nama atau ID ekstensi lain.
extensions-on = Nyala
extensions-off = Mati
extensions-enable-confirm = Aktifkan { $name }?
extensions-enable-permissions = Aktifkan { $name } dan izinkan:

lsp-title = Server Bahasa
lsp-search = Cari server bahasa, linter, formatter…
lsp-loading = Memuat katalog…
lsp-empty = Tidak ada server bahasa yang cocok
lsp-empty-detail = Coba bahasa, linter, atau formatter lain.
lsp-needs = memerlukan { $tool }
lsp-status-available = Tersedia
lsp-status-on-path = Di PATH
lsp-status-installing = Menginstal…
lsp-status-installed = Terinstal
lsp-status-outdated = Pembaruan tersedia
lsp-status-running = Berjalan
lsp-status-failed = Gagal

spaces-title = Ruang
spaces-new-placeholder = Nama ruang baru
spaces-empty = Belum ada ruang
spaces-default-name = Ruang { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
spaces-delete = Hapus ruang

team-title = Tim
team-just-you = Hanya Anda di ruang ini
team-agents = { $count ->
    [one] Anda dan 1 agen
   *[other] Anda dan { $count } agen
}
team-empty = Belum ada siapa pun di sini
team-you = Anda
team-agent = Agen

services-title = Layanan Latar Belakang
services-processes = { $count ->
    [one] 1 proses
   *[other] { $count } proses
}
services-kill-all = Hentikan Semua Paksa
services-not-running = Layanan tidak berjalan
services-start-with = Mulai dengan:
services-empty = Tidak ada proses aktif
services-filter = Filter proses…
services-no-match = Tidak ada proses yang cocok
services-connected = Terhubung
services-disconnected = Terputus
services-attached = terlampir
services-kill = Hentikan paksa
services-memory = Memori
services-size = Ukuran
services-shell = Shell

error-title = Kesalahan

history-search = Cari riwayat
history-clear-all = Hapus semua
history-clear-confirm = Hapus semua riwayat?
history-clear-warning = Tindakan ini tidak dapat dibatalkan.
history-cancel = Batal
history-today = Hari ini
history-yesterday = Kemarin
history-days-ago = { $count } hari lalu
history-day-offset = Hari -{ $count }

settings-title = Pengaturan
settings-loading = Memuat pengaturan…
settings-stored = Disimpan di ~/.vmux/settings.ron
settings-other = Lainnya
settings-software-update = Pembaruan Perangkat Lunak
settings-check-updates = Periksa Pembaruan
settings-check-updates-hint = Diperiksa otomatis saat diluncurkan dan setiap jam jika Pembaruan otomatis aktif.
settings-update-unavailable = Tidak tersedia
settings-update-unavailable-hint = Pembaru tidak disertakan dalam build ini.
settings-update-checking = Memeriksa…
settings-update-checking-hint = Memeriksa pembaruan…
settings-update-check-again = Periksa Lagi
settings-update-current = Vmux sudah versi terbaru.
settings-update-downloading = Mengunduh…
settings-update-downloading-hint = Mengunduh Vmux { $version }…
settings-update-installing = Menginstal…
settings-update-installing-hint = Menginstal Vmux { $version }…
settings-update-ready = Pembaruan Siap
settings-update-ready-hint = Vmux { $version } siap. Mulai ulang untuk menerapkannya.
settings-update-try-again = Coba Lagi
settings-update-failed = Tidak dapat memeriksa pembaruan.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Tekan tombol…
settings-saved = Tersimpan
settings-record-key = Klik untuk merekam kombinasi tombol baru

tray-open-window = Buka Jendela
tray-close-window = Tutup Jendela
tray-pause-recording = Jeda Perekaman
tray-resume-recording = Lanjutkan Perekaman
tray-finish-recording = Selesaikan Perekaman
tray-quit = Keluar dari Vmux

composer-attach-files = Lampirkan file (/upload)
composer-remove-attachment = Hapus lampiran

layout-back = Kembali
layout-forward = Maju
layout-reload = Muat ulang
layout-bookmark-page = Tandai halaman ini
layout-remove-bookmark = Hapus markah
layout-pin-page = Sematkan halaman ini
layout-unpin-page = Lepas sematan halaman ini
layout-manage-extensions = Kelola ekstensi
layout-new-stack = Tumpukan baru
layout-close-tab = Tutup tab
layout-bookmark = Markah
layout-pin = Sematkan
layout-new-tab = Tab baru
layout-team = Tim

command-switch-space = Beralih ruang…
command-search-ask = Cari atau tanya…
command-new-tab-placeholder = Cari atau ketik URL, atau pilih Terminal…
command-placeholder = Ketik URL, cari tab, atau > untuk perintah…
command-composer-placeholder = Ketik / untuk perintah atau @ untuk media
command-send = Kirim (Enter)
command-terminal = Terminal
command-open-terminal = Buka di Terminal
command-stack = Tumpukan
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
command-prompt = Prompt
command-new-tab = Tab baru
command-search = Cari
command-open-value = Buka “{ $value }”
command-search-value = Cari “{ $value }”

schema-appearance = Tampilan
schema-general = Umum
schema-layout = Tata letak
schema-layout-detail = Jendela, panel, bilah sisi, dan cincin fokus.
schema-agent = Agen
schema-agent-detail = Perilaku agen dan izin alat.
schema-shortcuts = Pintasan
schema-shortcuts-detail = Tampilan hanya-baca. Edit settings.ron langsung untuk mengubah binding.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Mode
schema-mode-detail = Skema warna untuk halaman web. Perangkat mengikuti sistem Anda.
schema-device = Perangkat
schema-light = Terang
schema-dark = Gelap
schema-language = Bahasa
schema-language-detail = Gunakan sistem, en-US, ja, atau tag BCP 47 apa pun dengan katalog ~/.vmux/locales/<tag>.ftl yang sesuai.
schema-auto-update = Pembaruan otomatis
schema-auto-update-detail = Periksa dan instal pembaruan saat diluncurkan dan setiap jam.
schema-startup-url = URL awal
schema-startup-url-detail = Kosongkan untuk membuka prompt bilah perintah.
schema-search-engine = Mesin telusur
schema-search-engine-detail = Digunakan untuk pencarian web dari Mulai dan bilah perintah.
schema-window = Jendela
schema-pane = Panel
schema-side-sheet = Lembar samping
schema-focus-ring = Cincin fokus
schema-run-placement = Izinkan penggantian penempatan run
schema-run-placement-detail = Izinkan agen memilih mode panel run, arah, dan jangkar.
schema-leader = Leader
schema-leader-detail = Tombol awalan untuk pintasan chord.
schema-chord-timeout = Waktu habis chord
schema-chord-timeout-detail = Milidetik sebelum awalan chord kedaluwarsa.
schema-bindings = Binding
schema-confirm-close = Konfirmasi saat menutup
schema-confirm-close-detail = Minta konfirmasi sebelum menutup terminal dengan proses yang sedang berjalan.
schema-default-theme = Tema default
schema-default-theme-detail = Nama tema aktif dari daftar tema.
