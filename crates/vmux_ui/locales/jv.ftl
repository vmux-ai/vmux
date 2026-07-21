locale-name = Basa Jawa
common-open = Bukak
common-close = Tutup
common-install = Pasang
common-uninstall = Copot
common-update = Anyari
common-retry = Coba maneh
common-refresh = Segeraké
common-remove = Busak
common-enable = Aktifaké
common-disable = Patèni
common-new = Anyar
common-active = aktif
common-running = mlaku
common-done = rampung
common-failed = Gagal
common-installed = Wis dipasang
common-items = { $count ->
    [one] { $count } item
   *[other] { $count } item
}
start-title = Miwiti
start-tagline = Siji prompt. Apa waé, rampung.

agents-title = Agen
agents-search = Golèk agen ACP lan CLI…
agents-empty = Ora ana agen sing cocog
agents-empty-detail = Coba nganggo jeneng, runtime, utawa ACP/CLI.
agents-install-failed = Pamasangan gagal
agents-updating = Lagi dianyari…
agents-retrying = Lagi dicoba maneh…
agents-preparing = Lagi disiapaké…

extensions-title = Ekstensi
extensions-search = Golèk sing wis dipasang utawa ing Chrome Web Store…
extensions-relaunch = Bukak ulang kanggo ngetrapaké
extensions-empty = Durung ana ekstensi sing dipasang
extensions-no-match = Ora ana ekstensi sing cocog
extensions-empty-detail = Golèk ing Chrome Web Store ndhuwur, banjur pencèt Return.
extensions-no-match-detail = Coba jeneng utawa ID ekstensi liya.
extensions-on = Urip
extensions-off = Mati
extensions-enable-confirm = Aktifaké { $name }?
extensions-enable-permissions = Aktifaké { $name } lan ijini:

lsp-title = Server Basa
lsp-search = Golèk server basa, linter, formatter…
lsp-loading = Lagi ngemot katalog…
lsp-empty = Ora ana server basa sing cocog
lsp-empty-detail = Coba basa, linter, utawa formatter liya.
lsp-needs = butuh { $tool }
lsp-status-available = Kasedhiya
lsp-status-on-path = Ana ing PATH
lsp-status-installing = Lagi dipasang…
lsp-status-installed = Wis dipasang
lsp-status-outdated = Ana anyaran
lsp-status-running = Mlaku
lsp-status-failed = Gagal

spaces-title = Ruang
spaces-new-placeholder = Jeneng ruang anyar
spaces-empty = Durung ana ruang
spaces-default-name = Ruang { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
spaces-delete = Busak ruang

team-title = Tim
team-just-you = Mung sampeyan ing ruang iki
team-agents = { $count ->
    [one] Sampeyan lan 1 agen
   *[other] Sampeyan lan { $count } agen
}
team-empty = Durung ana sapa-sapa ing kéné
team-you = Sampeyan
team-agent = Agen

services-title = Layanan Latar
services-processes = { $count ->
    [one] 1 prosès
   *[other] { $count } prosès
}
services-kill-all = Patèni Kabeh
services-not-running = Layanan ora mlaku
services-start-with = Miwiti nganggo:
services-empty = Ora ana prosès aktif
services-filter = Saring prosès…
services-no-match = Ora ana prosès sing cocog
services-connected = Kasambung
services-disconnected = Pedhot
services-attached = kagandhèng
services-kill = Patèni
services-memory = Memori
services-size = Ukuran
services-shell = Shell

error-title = Kasalahan

history-search = Golèk riwayat
history-clear-all = Busak kabeh
history-clear-confirm = Busak kabeh riwayat?
history-clear-warning = Iki ora bisa dibalèkaké.
history-cancel = Batal
history-today = Dina iki
history-yesterday = Wingi
history-days-ago = { $count } dina kepungkur
history-day-offset = Dina -{ $count }

settings-title = Setèlan
settings-loading = Lagi ngemot setèlan…
settings-stored = Kasimpen ing ~/.vmux/settings.ron
settings-other = Liyane
settings-software-update = Anyaran Piranti Lunak
settings-check-updates = Priksa Anyaran
settings-check-updates-hint = Mriksa otomatis nalika dibukak lan saben jam yen Auto-update diaktifaké.
settings-update-unavailable = Ora kasedhiya
settings-update-unavailable-hint = Panganyar ora kalebu ing build iki.
settings-update-checking = Lagi mriksa…
settings-update-checking-hint = Lagi mriksa anyaran…
settings-update-check-again = Priksa Maneh
settings-update-current = Vmux wis paling anyar.
settings-update-downloading = Lagi ngundhuh…
settings-update-downloading-hint = Lagi ngundhuh Vmux { $version }…
settings-update-installing = Lagi masang…
settings-update-installing-hint = Lagi masang Vmux { $version }…
settings-update-ready = Anyaran Siap
settings-update-ready-hint = Vmux { $version } wis siap. Wiwiti ulang kanggo ngetrapaké.
settings-update-try-again = Coba Maneh
settings-update-failed = Ora bisa mriksa anyaran.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Pencèt tombol…
settings-saved = Kasimpen
settings-record-key = Klik kanggo ngrekam kombinasi tombol anyar

tray-open-window = Bukak Jendhela
tray-close-window = Tutup Jendhela
tray-pause-recording = Ngaso Ngrekam
tray-resume-recording = Terusaké Ngrekam
tray-finish-recording = Rampungaké Ngrekam
tray-quit = Metu saka Vmux

composer-attach-files = Lampiraké berkas (/upload)
composer-remove-attachment = Busak lampiran

layout-back = Mbalik
layout-forward = Maju
layout-reload = Muat ulang
layout-bookmark-page = Tetenger kaca iki
layout-remove-bookmark = Busak tetenger
layout-pin-page = Semataké kaca iki
layout-unpin-page = Copot sematan kaca iki
layout-manage-extensions = Atur ekstensi
layout-new-stack = Stack anyar
layout-close-tab = Tutup tab
layout-bookmark = Tetenger
layout-pin = Semataké
layout-new-tab = Tab anyar
layout-team = Tim

command-switch-space = Ganti ruang…
command-search-ask = Golèk utawa takon…
command-new-tab-placeholder = Golèk utawa ketik URL, utawa pilih Terminal…
command-placeholder = Ketik URL, golèk tab, utawa > kanggo prentah…
command-composer-placeholder = Ketik / kanggo prentah utawa @ kanggo media
command-send = Kirim (Enter)
command-terminal = Terminal
command-open-terminal = Bukak ing Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
command-prompt = Prompt
command-new-tab = Tab anyar
command-search = Golèk
command-open-value = Bukak “{ $value }”
command-search-value = Golèk “{ $value }”

schema-appearance = Tampilan
schema-general = Umum
schema-layout = Tata letak
schema-layout-detail = Jendhela, panel, sidebar, lan cincin fokus.
schema-agent = Agen
schema-agent-detail = Tumindaké agen lan ijin piranti.
schema-shortcuts = Trabasan
schema-shortcuts-detail = Tampilan mung-waca. Owahi settings.ron langsung kanggo ngganti binding.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Mode
schema-mode-detail = Skema werna kanggo kaca web. Piranti ngetutaké sistem sampeyan.
schema-device = Piranti
schema-light = Padhang
schema-dark = Peteng
schema-language = Basa
schema-language-detail = Gunakaké sistem, en-US, ja, utawa tag BCP 47 apa waé sing nduwé katalog ~/.vmux/locales/<tag>.ftl sing cocog.
schema-auto-update = Auto-update
schema-auto-update-detail = Priksa lan pasang anyaran nalika dibukak lan saben jam.
schema-startup-url = URL wiwitan
schema-startup-url-detail = Yen kosong, bakal mbukak prompt bar prentah.
schema-search-engine = Mesin golèk
schema-search-engine-detail = Digunakaké kanggo golèkan web saka Miwiti lan bar prentah.
schema-window = Jendhela
schema-pane = Panel
schema-side-sheet = Lembar sisih
schema-focus-ring = Cincin fokus
schema-run-placement = Idini agen ngganti panggonan mlaku
schema-run-placement-detail = Idini agen milih mode panel mlaku, arah, lan jangkar.
schema-leader = Leader
schema-leader-detail = Tombol prefiks kanggo trabasan chord.
schema-chord-timeout = Wektu entèk chord
schema-chord-timeout-detail = Milidetik sadurungé prefiks chord kadaluwarsa.
schema-bindings = Binding
schema-confirm-close = Konfirmasi nutup
schema-confirm-close-detail = Takon dhisik sadurungé nutup terminal sing isih ana prosès mlaku.
schema-default-theme = Tema gawan
schema-default-theme-detail = Jeneng tema aktif saka dhaptar tema.
