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

tools-title = Piranti
tools-search = Telusuri paket, agen, MCP, piranti basa lan berkas konfigurasi…
tools-open = Bukak Piranti
tools-fold = Ciutaké piranti
tools-unfold = Jembarake piranti
tools-scanning = Mindhai piranti lokal…
tools-no-installed = Ora ana piranti sing dipasang
tools-empty = Ora ana piranti sing cocog
tools-empty-detail = Pasang paket utawa tambahaké paket berkas konfigurasi gaya Stow.
tools-apply = Terapaké
tools-homebrew = Homebrew
tools-homebrew-sync = Formula lan aplikasi sing dipasang diselarasaké kanthi otomatis.
tools-open-brewfile = Bukak Brewfile
tools-managed = dikelola
tools-provider-homebrew-formulae = Formula Homebrew
tools-provider-homebrew-casks = Aplikasi Homebrew
tools-provider-npm = Paket npm
tools-provider-acp-agents = Agen ACP
tools-provider-language-tools = Piranti basa
tools-provider-mcp-servers = Server MCP
tools-provider-dotfiles = Berkas konfigurasi
tools-status-available = Kasedhiya
tools-status-missing = Ora ana
tools-status-conflict = Konflik
tools-forget = Lalèkaké
tools-manage = Kelola
tools-link = Sambungaké
tools-unlink = Pedhot sambungan
tools-import = Impor
tools-update-count = { $count ->
    [one] 1 nganyari
   *[other] { $count } nganyari
}
tools-conflict-count = { $count ->
    [one] 1 konflik
   *[other] { $count } konflik
}
tools-result-applied = Piranti wis diterapaké
tools-result-imported = Piranti wis diimpor
tools-result-installed = { $name } wis dipasang
tools-result-updated = { $name } wis dianyari
tools-result-uninstalled = { $name } wis dibusak
tools-result-forgotten = { $name } wis dilalèkaké
tools-result-managed = { $name } saiki dikelola
tools-result-linked = { $name } wis disambungaké
tools-result-unlinked = { $name } wis dipedhot
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Nyelarasake setelan, alat, dotfiles, lan Kawruh karo Git.
vault-sync = Sinkronisasi
vault-create = Nggawe
vault-connect = Nyambung
vault-private = Repositori pribadi
vault-public-warning = Repositori umum mbukak Kawruh lan konfigurasi sampeyan.
vault-choose-repository = Pilih repositori…
vault-empty = kosong
vault-clean = Nganti saiki
vault-not-connected = Ora nyambung
vault-change-count = Owah-owahan: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

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

settings-empty = (kosong)
settings-none = (ora ana)

schema-system = Sistem
schema-editor = Panyunting
schema-recording = Rekaman
schema-radius = Radius
schema-padding = Padding
schema-gap = Jarak
schema-width = Ambane
schema-color = Werna
schema-red = Abang
schema-green = Ijo
schema-blue = Biru
schema-follow-files = Tututi berkas
schema-tidy-files = Rapikake berkas
schema-tidy-files-max = Wates rapikake berkas
schema-tidy-files-auto = Rapikake berkas otomatis
schema-app-providers = Panyedhiya aplikasi
schema-provider = Panyedhiya
schema-kind = Jinis
schema-models = Model
schema-acp = Agen ACP
schema-id = ID
schema-name = Jeneng
schema-command = Prentah
schema-arguments = Argumen
schema-environment = Lingkungan
schema-working-directory = Direktori kerja
schema-shell = Shell
schema-font-family = Kulawarga font
schema-startup-directory = Direktori wiwitan
schema-themes = Tema
schema-color-scheme = Skema werna
schema-font-size = Ukuran font
schema-line-height = Dhuwur baris
schema-cursor-style = Gaya kursor
schema-cursor-blink = Kedhip kursor
schema-custom-themes = Tema kustom
schema-foreground = Latar ngarep
schema-background = Latar mburi
schema-cursor = Kursor
schema-ansi-colors = Werna ANSI
schema-keymap = Peta tombol
schema-explorer = Panjlajah
schema-visible = Katon
schema-language-servers = Server basa
schema-servers = Server
schema-language-id = ID basa
schema-root-markers = Tandha root
schema-output-directory = Direktori output

menu-scene = Adegan
menu-layout = Tata letak
menu-terminal = Terminal
menu-browser = Pramban
menu-service = Layanan
menu-bookmark = Tetenger
menu-edit = Sunting

layout-knowledge = Kawruh
layout-open-knowledge = Bukak Kawruh
layout-open-welcome-knowledge = Bukak Sugeng rawuh ing Kawruh
layout-open-path = Bukak { $path }
layout-fold-knowledge = Ciutake kawruh
layout-unfold-knowledge = Jembarake kawruh
layout-bookmarks = Tetenger
layout-new-folder = Folder Anyar
layout-add-to-bookmarks = Tambah menyang Tetenger
layout-move-to-bookmarks = Pindhah menyang Tetenger
layout-stack-number = Tumpukan { $number }
layout-fold-stack = Ciutake tumpukan
layout-unfold-stack = Jembarake tumpukan
layout-close-stack = Tutup tumpukan
layout-bookmark-in = Tetenger ing { $folder }

common-cancel = Batal
common-delete = Busak
common-save = Simpen
common-rename = Ganti jeneng
common-expand = Jembarake
common-collapse = Ciutake
common-loading = Ngemot…
common-error = Kaluputan
common-output = Output
common-pending = Ngenteni
common-current = saiki
common-stop = Mandheg
services-command = Layanan Vmux
services-uptime-seconds = { $seconds }d
services-uptime-minutes = { $minutes }m { $seconds }d
services-uptime-hours = { $hours }j { $minutes }m
services-uptime-days = { $days }dina { $hours }j

error-page-failed-load = Kaca gagal diemot
error-page-not-found = Kaca ora ditemokake
error-unknown-host = Host aplikasi Vmux ora dikenal: { $host }

history-title = Riwayat

command-new-app-chat = Chat { $provider }/{ $model } anyar (Aplikasi)
command-interactive-mode-user = Adegan > Mode Interaktif > Panganggo
command-interactive-mode-player = Adegan > Mode Interaktif > Pamuter
command-minimize-window = Tata Letak > Jendhela > Ciutake
command-toggle-layout = Tata Letak > Tata Letak > Ganti Tata Letak
command-close-tab = Tata Letak > Tab > Tutup Tab
command-new-task = Tata Letak > Tab > Tugas Anyar…
command-next-tab = Tata Letak > Tab > Tab Sabanjure
command-prev-tab = Tata Letak > Tab > Tab Sadurunge
command-rename-tab = Tata Letak > Tab > Ganti Jeneng Tab
command-tab-select-1 = Tata Letak > Tab > Pilih Tab 1
command-tab-select-2 = Tata Letak > Tab > Pilih Tab 2
command-tab-select-3 = Tata Letak > Tab > Pilih Tab 3
command-tab-select-4 = Tata Letak > Tab > Pilih Tab 4
command-tab-select-5 = Tata Letak > Tab > Pilih Tab 5
command-tab-select-6 = Tata Letak > Tab > Pilih Tab 6
command-tab-select-7 = Tata Letak > Tab > Pilih Tab 7
command-tab-select-8 = Tata Letak > Tab > Pilih Tab 8
command-tab-select-last = Tata Letak > Tab > Pilih Tab Pungkasan
command-close-pane = Tata Letak > Pane > Tutup Pane
command-select-pane-left = Tata Letak > Pane > Pilih Pane Kiwa
command-select-pane-right = Tata Letak > Pane > Pilih Pane Tengen
command-select-pane-up = Tata Letak > Pane > Pilih Pane Ndhuwur
command-select-pane-down = Tata Letak > Pane > Pilih Pane Ngisor
command-swap-pane-prev = Tata Letak > Pane > Ijol Pane Sadurunge
command-swap-pane-next = Tata Letak > Pane > Ijol Pane Sabanjure
command-equalize-pane-size = Tata Letak > Pane > Padakake Ukuran Pane
command-resize-pane-left = Tata Letak > Pane > Owahi Ukuran Pane Ngiwa
command-resize-pane-right = Tata Letak > Pane > Owahi Ukuran Pane Nengen
command-resize-pane-up = Tata Letak > Pane > Owahi Ukuran Pane Munggah
command-resize-pane-down = Tata Letak > Pane > Owahi Ukuran Pane Mudhun
command-stack-close = Tata Letak > Stack > Tutup Stack
command-stack-next = Tata Letak > Stack > Stack Sabanjure
command-stack-previous = Tata Letak > Stack > Stack Sadurunge
command-stack-reopen = Tata Letak > Stack > Bukak Maneh Kaca sing Ditutup
command-stack-swap-prev = Tata Letak > Stack > Pindhah Stack Ngikiwa
command-stack-swap-next = Tata Letak > Stack > Pindhah Stack Nengen
command-space-open = Tata Letak > Space > Spaces
command-terminal-close = Terminal > Tutup Terminal
command-terminal-next = Terminal > Terminal Sabanjure
command-terminal-prev = Terminal > Terminal Sadurunge
command-terminal-clear = Terminal > Resiki Terminal
command-browser-prev-page = Browser > Navigasi > Bali
command-browser-next-page = Browser > Navigasi > Maju
command-browser-reload = Browser > Navigasi > Muat Ulang
command-browser-hard-reload = Browser > Navigasi > Muat Ulang Paksa
command-open-in-place = Browser > Bukak > Bukak Ing Kene
command-open-in-new-stack = Browser > Bukak > Bukak ing Stack Anyar
command-open-in-pane-top = Browser > Bukak > Bukak ing Pane Ndhuwur
command-open-in-pane-right = Browser > Bukak > Bukak ing Pane Tengen
command-open-in-pane-bottom = Browser > Bukak > Bukak ing Pane Ngisor
command-open-in-pane-left = Browser > Bukak > Bukak ing Pane Kiwa
command-open-in-new-tab = Browser > Bukak > Bukak ing Tab Anyar
command-open-in-new-space = Browser > Bukak > Bukak ing Space Anyar
command-browser-zoom-in = Browser > Tampilan > Gedhekake
command-browser-zoom-out = Browser > Tampilan > Cilikake
command-browser-zoom-reset = Browser > Tampilan > Ukuran Asli
command-browser-dev-tools = Browser > Tampilan > Piranti Pangembang
command-browser-open-command-bar = Browser > Bar > Bar Prentah
command-browser-open-page-in-command-bar = Browser > Bar > Owahi Kaca
command-browser-open-path-bar = Browser > Bar > Navigator Path
command-browser-open-commands = Browser > Bar > Prentah
command-browser-open-history = Browser > Bar > Riwayat
command-service-open = Layanan > Bukak Monitor Layanan
command-bookmark-toggle-active = Tetenger > Tandhani Kaca
command-bookmark-pin-active = Tetenger > Semat Kaca

layout-tab = Tab
layout-no-stacks = Ora ana stack
layout-loading = Ngemot…
layout-no-markdown-files = Ora ana berkas Markdown
layout-empty-folder = Folder kosong
layout-worktree = worktree
layout-folder-name = Jeneng folder
layout-no-pins-bookmarks = Ora ana pin utawa tetenger
layout-move-to = Pindhah menyang { $folder }
layout-bookmark-current-page = Tandhani Kaca Saiki
layout-rename-folder = Ganti Jeneng Folder
layout-remove-folder = Busak Folder
layout-update-downloading = Ngundhuh nganyari
layout-update-installing = Masang nganyari…
layout-update-ready = Versi anyar kasedhiya
layout-restart-update = Wiwiti maneh kanggo nganyari

agent-preparing = Nyiyapake agen…
agent-send-all-queued = Kirim kabeh prompt antrean saiki (Esc)
agent-send = Kirim (Enter)
agent-ready = Siap kapan wae.
agent-loading-older = Ngemot pesen lawas…
agent-load-older = Muat pesen lawas
agent-continued-from = Diterusake saka { $source }
agent-older-context-omitted = konteks lawas ora ditampilake
agent-interrupted = keselani
agent-allow-tool = Idini { $tool }?
agent-deny = Tolak
agent-allow-always = Tansah idini
agent-allow = Idini
agent-loading-sessions = Ngemot sesi…
agent-no-resumable-sessions = Ora ana sesi sing bisa diterusake
agent-no-matching-sessions = Ora ana sesi sing cocog
agent-no-matching-models = Ora ana model sing cocog
agent-choice-help = ↑/↓ utawa Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Pilih folder repositori
agent-choose-repository-detail = Pilih repositori Git lokal sing kudu dienggo agen.
agent-choosing = Milih…
agent-choose-folder = Pilih folder
agent-queued = antre
agent-attached = Dilampirake:
agent-cancel-queued = Batalake prompt antrean
agent-resume-queued = Terusake prompt antrean
agent-clear-queue = Resiki antrean
agent-send-all-now = kirim kabeh saiki
agent-choose-option = Pilih opsi ing ndhuwur
agent-loading-media = Ngemot media…
agent-no-matching-media = Ora ana media sing cocog
agent-prompt-context = Konteks prompt
agent-details = Rincian
agent-path = Path
agent-tool = Piranti
agent-server = Server
agent-bytes = { $count } byte
agent-worked-for = Nyambut gawe { $duration }
agent-worked-for-steps = { $count ->
    [one] Nyambut gawe { $duration } · 1 langkah
   *[other] Nyambut gawe { $duration } · { $count } langkah
}
agent-tool-guardian-review = Tinjauan Guardian
agent-tool-read-files = Maca berkas
agent-tool-viewed-image = Ndeleng gambar
agent-tool-used-browser = Nganggo browser
agent-tool-searched-files = Nggoleki berkas
agent-tool-ran-commands = Nglakokake prentah
agent-thinking = Mikir
agent-subagent = Subagen
agent-prompt = Prompt
agent-thread = Utas
agent-parent = Induk
agent-children = Anak
agent-call = Panggilan
agent-raw-event = Acara mentah
agent-plan = Rencana
agent-tasks = { $count ->
    [one] 1 tugas
   *[other] { $count } tugas
}
agent-edited = Diowahi
agent-reconnecting = Nyambung maneh { $attempt }/{ $total }
agent-status-running = Mlaku
agent-status-done = Rampung
agent-status-failed = Gagal
agent-status-pending = Ngenteni
agent-slash-attach-files = Lampirake berkas
agent-slash-resume-session = Terusake sesi kepungkur
agent-slash-select-model = Pilih model
agent-slash-continue-cli = Terusake sesi iki ing CLI
agent-session-just-now = mentas wae
agent-session-minutes-ago = { $count }m kepungkur
agent-session-hours-ago = { $count }j kepungkur
agent-session-days-ago = { $count }dina kepungkur
agent-working-working = Nyambut gawe
agent-working-thinking = Mikir
agent-working-pondering = Nimbang-nimbang
agent-working-noodling = Nggagas
agent-working-percolating = Nglungguhi gagasan
agent-working-conjuring = Ngracik
agent-working-cooking = Ngolah
agent-working-brewing = Ngracik
agent-working-musing = Nglamun mikir
agent-working-ruminating = Mikir jero
agent-working-scheming = Ngrancang
agent-working-synthesizing = Ngringkes lan nyawijèkaké
agent-working-tinkering = Nguji-nguji
agent-working-churning = Ngolah
agent-working-vibing = Nyetel rasa
agent-working-simmering = Ngendhem gagasan
agent-working-crafting = Nggawé
agent-working-divining = Nglacak jawaban
agent-working-mulling = Nimbang
agent-working-spelunking = Nlusuri jero

editor-toggle-explorer = Ganti Explorer (Cmd+B)
editor-unsaved = durung disimpen
editor-rendered-markdown = Markdown dirender nganggo panyuntingan langsung
editor-note = Cathetan
editor-source-editor = Editor sumber
editor-editor = Editor
editor-git-diff = Diff Git
editor-diff = Diff
editor-tidy = Rapèkaké
editor-always = Tansah
editor-unchanged-previews = { $count ->
    [one] ✦ 1 pratinjau ora owah
   *[other] ✦ { $count } pratinjau ora owah
}
editor-open-externally = Bukak nganggo aplikasi njaba
editor-changed-line = Baris sing owah
editor-go-to-definition = Menyang Definisi
editor-find-references = Goleki Referensi
editor-references = { $count ->
    [one] 1 referensi
   *[other] { $count } referensi
}
editor-lsp-starting = { $server } miwiti…
editor-lsp-not-installed = { $server } — durung dipasang
editor-explorer = Explorer
editor-open-editors = Editor sing Kabukak
editor-outline = Ringkesan
editor-new-file = Berkas Anyar
editor-new-folder = Folder Anyar
editor-delete-confirm = Busak “{ $name }”? Iki ora bisa dibalekake.
editor-created-folder = Folder { $name } digawe
editor-created-file = Berkas { $name } digawe
editor-renamed-to = Diganti jeneng dadi { $name }
editor-deleted = { $name } dibusak
editor-failed-decode-image = Gagal maca gambar
editor-preview-large-image = gambar (kegedhen kanggo pratinjau)
editor-preview-binary = binèr
editor-preview-file = berkas

git-status-clean = resik
git-status-modified = diowahi
git-status-staged = staged
git-status-staged-modified = staged*
git-status-untracked = ora dilacak
git-status-deleted = dibusak
git-status-conflict = konflik
git-accept-all = ✓ tampa kabeh
git-unstage = Copot saka stage
git-confirm-deny-all = Konfirmasi tolak kabeh
git-deny-all = ✗ tolak kabeh
git-commit-message = pesen commit
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Ngemot diff…
git-no-changes = Ora ana owahan kanggo ditampilake
git-accept = ✓ tampa
git-deny = ✗ tolak
git-show-unchanged-lines = Tampilake { $count } baris ora owah

terminal-loading = Ngemot…
terminal-runs-when-ready = mlaku yen wis siap · Ctrl+C ngresiki · Esc ngliwati
terminal-booting = miwiti
terminal-type-command = ketik prentah · mlaku yen wis siap · Esc ngliwati

setup-tagline-claude = Agen coding Anthropic, ing Vmux
setup-tagline-codex = Agen coding OpenAI, ing Vmux
setup-tagline-vibe = Agen coding Mistral, ing Vmux
setup-install-title = Pasang CLI { $name }
setup-homebrew-required = Homebrew dibutuhake kanggo masang { $command } lan durung disetel. Vmux bakal masang Homebrew dhisik, banjur { $name }.
setup-terminal-instructions = Ing terminal, penet Return kanggo miwiti, banjur ketik sandhi Mac nalika dijaluk.
setup-command-missing = Vmux mbukak kaca iki amarga prentah lokal { $command } durung dipasang. Lakokake prentah ing ngisor iki kanggo njupuk.
setup-install-failed = Pamasangan durung rampung. Priksa terminal kanggo rincian, banjur coba maneh.
setup-installing = Masang…
setup-install-homebrew = Pasang Homebrew + { $name }
setup-run-install = Lakokake prentah pamasangan
setup-auto-reload = Vmux nglakokake ing terminal lan ngemot ulang nalika { $command } wis siap.

debug-title = Debug
debug-auto-update = Nganyari otomatis
debug-simulate-update = Simulasikake nganyari kasedhiya
debug-simulate-download = Simulasikake undhuhan
debug-clear-update = Resiki nganyari
debug-trigger-restart = Picu wiwit maneh

command-manage-spaces = Atur space…
command-pane-stack-location = pane { $pane } / stack { $stack }
command-space-pane-stack-location = { $space } / pane { $pane } / stack { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Mode Interaktif
command-group-window = Jendhela
command-group-tab = Tab
command-group-pane = Pane
command-group-stack = Stack
command-group-space = Space
command-group-navigation = Navigasi
command-group-open = Bukak
command-group-view = Tampilan
command-group-bar = Bilah

menu-close-vmux = Tutup Vmux

agents-terminal-coding-agent = Agen coding adhedhasar terminal
