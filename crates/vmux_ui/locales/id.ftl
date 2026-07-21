common-open = Buka
common-close = Tutup
common-install = Instal
common-uninstall = Copot pemasangan
common-update = Pembaruan
common-retry = Coba lagi
common-refresh = Segarkan
common-remove = Hapus
common-enable = Aktifkan
common-disable = Nonaktifkan
common-new = Baru
common-active = aktif
common-running = berlari
common-done = selesai
common-failed = Gagal
common-installed = Dipasang
common-items = { $count ->
    [one] { $count } barang
   *[other] { $count } item
}
start-title = Mulai
start-tagline = Satu perintah. Apapun, selesai.

agents-title = Agen
agents-search = Cari agen ACP dan CLI…
agents-empty = Tidak ada agen yang cocok
agents-empty-detail = Coba nama, waktu proses, atau ACP/CLI.
agents-install-failed = Penginstalan gagal
agents-updating = Memperbarui…
agents-retrying = Mencoba lagi…
agents-preparing = Mempersiapkan…

extensions-title = Ekstensi
extensions-search = Pencarian terpasang atau Chrome Web Store…
extensions-relaunch = Luncurkan kembali untuk melamar
extensions-empty = Tidak ada ekstensi yang dipasang
extensions-no-match = Tidak ada ekstensi yang cocok
extensions-empty-detail = Cari Chrome Web Store di atas dan tekan Return.
extensions-no-match-detail = Coba nama atau ID ekstensi lain.
extensions-on = Aktif
extensions-off = Mati
extensions-enable-confirm = Aktifkan { $name }?
extensions-enable-permissions = Aktifkan { $name } dan izinkan:

lsp-title = Server Bahasa
lsp-search = Cari server bahasa, linter, pemformat…
lsp-loading = Memuat katalog…
lsp-empty = Tidak ada server bahasa yang cocok
lsp-empty-detail = Coba bahasa, linter, atau formatter lain.
lsp-needs = kebutuhan { $tool }
lsp-status-available = Tersedia
lsp-status-on-path = Pada PATH
lsp-status-installing = Memasang…
lsp-status-installed = Dipasang
lsp-status-outdated = Pembaruan tersedia
lsp-status-running = Berlari
lsp-status-failed = Gagal

spaces-title = Spasi
spaces-new-placeholder = Nama ruang baru
spaces-empty = Tidak ada spasi
spaces-default-name = Spasi { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
spaces-delete = Hapus spasi

team-title = Tim
team-just-you = Hanya kamu di ruang ini
team-agents = { $count ->
    [one] Anda dan 1 agen
   *[other] Anda dan agen { $count }
}
team-empty = Belum ada seorang pun di sini
team-you = kamu
team-agent = Agen

services-title = Layanan Latar Belakang
services-processes = { $count ->
    [one] 1 proses
   *[other] { $count } proses
}
services-kill-all = Bunuh Semua
services-not-running = Layanan tidak berjalan
services-start-with = Mulailah dengan:
services-empty = Tidak ada proses aktif
services-filter = Memfilter proses…
services-no-match = Tidak ada proses yang cocok
services-connected = Terhubung
services-disconnected = Terputus
services-attached = terlampir
services-kill = Bunuh
services-memory = Memori
services-size = Ukuran
services-shell = cangkang

error-title = Kesalahan

history-search = Riwayat pencarian
history-clear-all = Hapus semuanya
history-clear-confirm = Hapus semua riwayat?
history-clear-warning = Hal ini tidak dapat dibatalkan.
history-cancel = Batalkan
history-today = Hari ini
history-yesterday = Kemarin
history-days-ago = { $count } hari yang lalu
history-day-offset = Hari -{ $count }

settings-title = Pengaturan
settings-loading = Memuat pengaturan…
settings-stored = Disimpan di ~/.vmux/settings.ron
settings-other = Lainnya
settings-software-update = Pembaruan Perangkat Lunak
settings-check-updates = Periksa Pembaruan
settings-check-updates-hint = Memeriksa secara otomatis saat peluncuran dan setiap jam saat Pembaruan otomatis diaktifkan.
settings-update-unavailable = Tidak tersedia
settings-update-unavailable-hint = Updater tidak termasuk dalam build ini.
settings-update-checking = Memeriksa…
settings-update-checking-hint = Memeriksa pembaruan…
settings-update-check-again = Periksa Lagi
settings-update-current = Vmux sudah yang terbaru.
settings-update-downloading = Mengunduh…
settings-update-downloading-hint = Mengunduh Vmux { $version }…
settings-update-installing = Memasang…
settings-update-installing-hint = Memasang Vmux { $version }…
settings-update-ready = Pembaruan Siap
settings-update-ready-hint = Vmux { $version } sudah siap. Mulai ulang untuk menerapkannya.
settings-update-try-again = Coba Lagi
settings-update-failed = Tidak dapat memeriksa pembaruan.
settings-item = Barang
settings-item-number = Barang { $number }
settings-press-key = Tekan sebuah tombol…
settings-saved = Disimpan
settings-record-key = Klik untuk merekam kombo kunci baru

tray-open-window = Buka Jendela
tray-close-window = Tutup Jendela
tray-pause-recording = Jeda Perekaman
tray-resume-recording = Lanjutkan Perekaman
tray-finish-recording = Selesai Merekam
tray-quit = Keluar dari Vmux

composer-attach-files = Lampirkan file (/upload)
composer-remove-attachment = Hapus lampiran

layout-back = Kembali
layout-forward = Maju
layout-reload = Muat ulang
layout-bookmark-page = Tandai halaman ini
layout-remove-bookmark = Hapus penanda
layout-pin-page = Sematkan halaman ini
layout-unpin-page = Lepas sematan halaman ini
layout-manage-extensions = Kelola ekstensi
layout-new-stack = Tumpukan Baru
layout-close-tab = Tutup tab
layout-bookmark = Penanda buku
layout-pin = Sematkan
layout-new-tab = Tab baru
layout-team = Tim

command-switch-space = Ganti ruang…
command-search-ask = Cari atau tanyakan…
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
command-prompt = Cepat
command-new-tab = Tab baru
command-search = Cari
command-open-value = Buka “{ $value }”
command-search-value = Telusuri “{ $value }”

schema-appearance = Penampilan
schema-general = Umum
schema-layout = Tata Letak
schema-layout-detail = Jendela, panel, sidebar, dan cincin fokus.
schema-agent = Agen
schema-agent-detail = Perilaku agen dan izin alat.
schema-shortcuts = Jalan pintas
schema-shortcuts-detail = Tampilan hanya baca. Edit settings.ron secara langsung untuk mengubah pengikatan.
schema-terminal = Terminal
schema-browser = Peramban
schema-mode = Modus
schema-mode-detail = Skema warna untuk halaman web. Perangkat mengikuti sistem Anda.
schema-device = Perangkat
schema-light = Ringan
schema-dark = Gelap
schema-language = Bahasa
schema-language-detail = Gunakan tag sistem, en-US, ja, atau BCP 47 apa pun dengan katalog ~/.vmux/locales/<tag>.ftl yang cocok.
schema-auto-update = Pembaruan otomatis
schema-auto-update-detail = Periksa dan instal pembaruan saat peluncuran dan setiap jam.
schema-startup-url = Memulai URL
schema-startup-url-detail = Kosong membuka prompt bilah perintah.
schema-search-engine = Mesin pencari
schema-search-engine-detail = Digunakan untuk pencarian web dari Mulai dan bilah perintah.
schema-window = Jendela
schema-pane = panel
schema-side-sheet = Lembar samping
schema-focus-ring = Cincin fokus
schema-run-placement = Izinkan penggantian penempatan dijalankan
schema-run-placement-detail = Biarkan agen memilih mode panel lari, arah, dan jangkar.
schema-leader = Pemimpin
schema-leader-detail = Kunci awalan untuk pintasan akord.
schema-chord-timeout = Batas waktu akord
schema-chord-timeout-detail = Milidetik sebelum awalan akor berakhir.
schema-bindings = Binding
schema-confirm-close = Konfirmasikan tutup
schema-confirm-close-detail = Prompt sebelum menutup terminal dengan proses yang sedang berjalan.
schema-default-theme = Tema bawaan
schema-default-theme-detail = Nama tema aktif dari daftar tema.
