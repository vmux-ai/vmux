locale-name = azərbaycan
common-open = Aç
common-close = Bağla
common-install = Quraşdır
common-uninstall = Sil
common-update = Yenilə
common-retry = Təkrar cəhd et
common-refresh = Təzələ
common-remove = Sil
common-enable = Aktivləşdir
common-disable = Deaktivləşdir
common-new = Yeni
common-active = aktiv
common-running = işləyir
common-done = hazırdır
common-failed = Alınmadı
common-installed = Quraşdırılıb
common-items = { $count ->
    [one] { $count } element
   *[other] { $count } element
}

tools-title = Alətlər
tools-search = Paketlər, agentlər, MCP, dil alətləri və konfiqurasiya fayllarında axtar…
tools-open = Alətləri aç
tools-fold = Alətləri yığ
tools-unfold = Alətləri genişləndir
tools-scanning = Yerli alətlər yoxlanılır…
tools-no-installed = Quraşdırılmış alət yoxdur
tools-empty = Uyğun alət yoxdur
tools-empty-detail = Paket quraşdırın və ya Stow üslublu konfiqurasiya faylları paketi əlavə edin.
tools-apply = Tətbiq et
tools-homebrew = Homebrew
tools-homebrew-sync = Quraşdırılmış formulalar və tətbiq paketləri avtomatik sinxronlaşdırılır.
tools-open-brewfile = Brewfile-ı aç
tools-managed = idarə olunan
tools-provider-homebrew-formulae = Homebrew formulaları
tools-provider-homebrew-casks = Homebrew tətbiq paketləri
tools-provider-npm = npm paketləri
tools-provider-acp-agents = ACP agentləri
tools-provider-language-tools = Dil alətləri
tools-provider-mcp-servers = MCP serverləri
tools-provider-dotfiles = Konfiqurasiya faylları
tools-status-available = Mövcuddur
tools-status-missing = Çatışmır
tools-status-conflict = Ziddiyyət
tools-forget = Unut
tools-manage = İdarə et
tools-link = Əlaqələndir
tools-unlink = Əlaqəni kəs
tools-import = İdxal et
tools-update-count = { $count ->
    [one] 1 yeniləmə
   *[other] { $count } yeniləmə
}
tools-conflict-count = { $count ->
    [one] 1 ziddiyyət
   *[other] { $count } ziddiyyət
}
tools-result-applied = Alətlər tətbiq edildi
tools-result-imported = Alətlər idxal edildi
tools-result-installed = { $name } quraşdırıldı
tools-result-updated = { $name } yeniləndi
tools-result-uninstalled = { $name } silindi
tools-result-forgotten = { $name } unuduldu
tools-result-managed = { $name } artıq idarə olunur
tools-result-linked = { $name } əlaqələndirildi
tools-result-unlinked = { $name } əlaqədən çıxarıldı
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Parametrləri, alətləri, nöqtə fayllarını və Biliyi Git ilə sinxronlaşdırın.
vault-sync = Sinxronizasiya
vault-create = Yaradın
vault-connect = Qoşun
vault-private = Şəxsi depo
vault-public-warning = İctimai depolar Bilik və konfiqurasiyanızı ifşa edir.
vault-choose-repository = Repozitor seçin...
vault-empty = boş
vault-clean = Bu günə qədər
vault-not-connected = Qoşulmayıb
vault-change-count = Dəyişikliklər: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Başlanğıc
start-tagline = Bir prompt. Hər şey hazır.

agents-title = Agentlər
agents-search = ACP və CLI agentlərini axtar…
agents-empty = Uyğun agent yoxdur
agents-empty-detail = Ad, icra mühiti və ya ACP/CLI sınayın.
agents-install-failed = Quraşdırma alınmadı
agents-updating = Yenilənir…
agents-retrying = Təkrar cəhd edilir…
agents-preparing = Hazırlanır…

extensions-title = Genişləndirmələr
extensions-search = Quraşdırılanlarda və ya Chrome Web Store-da axtar…
extensions-relaunch = Tətbiq etmək üçün yenidən başladın
extensions-empty = Quraşdırılmış genişləndirmə yoxdur
extensions-no-match = Uyğun genişləndirmə yoxdur
extensions-empty-detail = Yuxarıda Chrome Web Store-da axtarın və Enter basın.
extensions-no-match-detail = Başqa ad və ya genişləndirmə ID-si sınayın.
extensions-on = Açıq
extensions-off = Bağlı
extensions-enable-confirm = { $name } aktivləşdirilsin?
extensions-enable-permissions = { $name } aktivləşdirilsin və bunlara icazə verilsin:

lsp-title = Dil serverləri
lsp-search = Dil serverləri, linterlər, formatlayıcılar axtar…
lsp-loading = Kataloq yüklənir…
lsp-empty = Uyğun dil serveri yoxdur
lsp-empty-detail = Başqa dil, linter və ya formatlayıcı sınayın.
lsp-needs = { $tool } tələb edir
lsp-status-available = Əlçatandır
lsp-status-on-path = PATH-dədir
lsp-status-installing = Quraşdırılır…
lsp-status-installed = Quraşdırılıb
lsp-status-outdated = Yeniləmə var
lsp-status-running = İşləyir
lsp-status-failed = Alınmadı

spaces-title = Məkanlar
spaces-new-placeholder = Yeni məkan adı
spaces-empty = Məkan yoxdur
spaces-default-name = Məkan { $number }
spaces-tabs = { $count ->
    [one] 1 vərəq
   *[other] { $count } vərəq
}
spaces-delete = Məkanı sil

team-title = Komanda
team-just-you = Bu məkanda yalnız siz varsınız
team-agents = { $count ->
    [one] Siz və 1 agent
   *[other] Siz və { $count } agent
}
team-empty = Hələ burada heç kim yoxdur
team-you = Siz
team-agent = Agent

services-title = Fon xidmətləri
services-processes = { $count ->
    [one] 1 proses
   *[other] { $count } proses
}
services-kill-all = Hamısını dayandır
services-not-running = Xidmət işləmir
services-start-with = Bununla başlat:
services-empty = Aktiv proses yoxdur
services-filter = Prosesləri filtrlə…
services-no-match = Uyğun proses yoxdur
services-connected = Qoşulub
services-disconnected = Qoşulmayıb
services-attached = qoşulub
services-kill = Dayandır
services-memory = Yaddaş
services-size = Ölçü
services-shell = Shell

error-title = Xəta

history-search = Tarixçədə axtar
history-clear-all = Hamısını təmizlə
history-clear-confirm = Bütün tarixçə təmizlənsin?
history-clear-warning = Bu əməliyyatı geri qaytarmaq mümkün deyil.
history-cancel = Ləğv et
history-today = Bu gün
history-yesterday = Dünən
history-days-ago = { $count } gün əvvəl
history-day-offset = Gün -{ $count }

settings-title = Parametrlər
settings-loading = Parametrlər yüklənir…
settings-stored = ~/.vmux/settings.ron faylında saxlanılır
settings-other = Digər
settings-software-update = Proqram yeniləməsi
settings-check-updates = Yeniləmələri yoxla
settings-check-updates-hint = Avtomatik yeniləmə aktivdirsə, açılışda və hər saat avtomatik yoxlayır.
settings-update-unavailable = Əlçatan deyil
settings-update-unavailable-hint = Yeniləyici bu quruluşda yoxdur.
settings-update-checking = Yoxlanılır…
settings-update-checking-hint = Yeniləmələr yoxlanılır…
settings-update-check-again = Yenidən yoxla
settings-update-current = Vmux yenidir.
settings-update-downloading = Endirilir…
settings-update-downloading-hint = Vmux { $version } endirilir…
settings-update-installing = Quraşdırılır…
settings-update-installing-hint = Vmux { $version } quraşdırılır…
settings-update-ready = Yeniləmə hazırdır
settings-update-ready-hint = Vmux { $version } hazırdır. Tətbiq etmək üçün yenidən başladın.
settings-update-try-again = Təkrar cəhd et
settings-update-failed = Yeniləmələri yoxlamaq mümkün olmadı.
settings-item = Element
settings-item-number = Element { $number }
settings-press-key = Klaviş basın…
settings-saved = Saxlanıldı
settings-record-key = Yeni klaviş kombinasiyası yazmaq üçün klikləyin

tray-open-window = Pəncərəni aç
tray-close-window = Pəncərəni bağla
tray-pause-recording = Yazını dayandır
tray-resume-recording = Yazını davam etdir
tray-finish-recording = Yazını bitir
tray-quit = Vmux-dan çıx

composer-attach-files = Faylları əlavə et (/upload)
composer-remove-attachment = Əlavəni sil

layout-back = Geri
layout-forward = İrəli
layout-reload = Yenidən yüklə
layout-bookmark-page = Bu səhifəni əlfəcin et
layout-remove-bookmark = Əlfəcini sil
layout-pin-page = Bu səhifəni sabitlə
layout-unpin-page = Bu səhifəni ayır
layout-manage-extensions = Genişləndirmələri idarə et
layout-new-stack = Yeni qat
layout-close-tab = Vərəqi bağla
layout-bookmark = Əlfəcin
layout-pin = Sabitlə
layout-new-tab = Yeni vərəq
layout-team = Komanda

command-switch-space = Məkanı dəyiş…
command-search-ask = Axtar və ya soruş…
command-new-tab-placeholder = Axtarış və ya URL yazın, ya da Terminal seçin…
command-placeholder = URL yazın, vərəqlərdə axtarın və ya əmrlər üçün > yazın…
command-composer-placeholder = Əmrlər üçün /, media üçün @ yazın
command-send = Göndər (Enter)
command-terminal = Terminal
command-open-terminal = Terminalda aç
command-stack = Qat
command-tabs = { $count ->
    [one] 1 vərəq
   *[other] { $count } vərəq
}
command-prompt = Prompt
command-new-tab = Yeni vərəq
command-search = Axtar
command-open-value = “{ $value }” aç
command-search-value = “{ $value }” axtar

schema-appearance = Görünüş
schema-general = Ümumi
schema-layout = Düzən
schema-layout-detail = Pəncərə, panellər, yan panel və fokus çərçivəsi.
schema-agent = Agent
schema-agent-detail = Agent davranışı və alət icazələri.
schema-shortcuts = Qısayollar
schema-shortcuts-detail = Yalnız oxuma görünüşü. Bağlamaları dəyişmək üçün settings.ron faylını birbaşa redaktə edin.
schema-terminal = Terminal
schema-browser = Brauzer
schema-mode = Rejim
schema-mode-detail = Veb səhifələr üçün rəng sxemi. Cihaz seçimi sisteminizi izləyir.
schema-device = Cihaz
schema-light = Açıq
schema-dark = Tünd
schema-language = Dil
schema-language-detail = Sistem dilindən, en-US, ja və ya uyğun ~/.vmux/locales/<tag>.ftl kataloqu olan istənilən BCP 47 teqindən istifadə edin.
schema-auto-update = Avtomatik yeniləmə
schema-auto-update-detail = Açılışda və hər saat yeniləmələri yoxla və quraşdır.
schema-startup-url = Başlanğıc URL-i
schema-startup-url-detail = Boş olduqda əmr paneli promptu açılır.
schema-search-engine = Axtarış sistemi
schema-search-engine-detail = Başlanğıc və əmr panelindən veb axtarışları üçün istifadə olunur.
schema-window = Pəncərə
schema-pane = Panel
schema-side-sheet = Yan vərəq
schema-focus-ring = Fokus çərçivəsi
schema-run-placement = İcra yerləşimini dəyişməyə icazə ver
schema-run-placement-detail = Agentlərə icra paneli rejimini, istiqaməti və lövbəri seçməyə icazə ver.
schema-leader = Lider
schema-leader-detail = Chord qısayolları üçün prefiks klavişi.
schema-chord-timeout = Chord vaxt limiti
schema-chord-timeout-detail = Chord prefiksi bitənə qədər millisanilər.
schema-bindings = Bağlamalar
schema-confirm-close = Bağlamanı təsdiqlə
schema-confirm-close-detail = İşləyən prosesi olan terminal bağlanmazdan əvvəl soruş.
schema-default-theme = Varsayılan tema
schema-default-theme-detail = Temalar siyahısından aktiv temanın adı.

settings-empty = (boş)
settings-none = (yoxdur)

schema-system = Sistem
schema-editor = Redaktor
schema-recording = Yazılış
schema-radius = Radius
schema-padding = İç boşluq
schema-gap = Ara
schema-width = En
schema-color = Rəng
schema-red = Qırmızı
schema-green = Yaşıl
schema-blue = Mavi
schema-follow-files = Faylları izlə
schema-tidy-files = Faylları səliqəyə sal
schema-tidy-files-max = Fayl səliqələmə həddi
schema-tidy-files-auto = Faylları avtomatik səliqəyə sal
schema-app-providers = Tətbiq provayderləri
schema-provider = Provayder
schema-kind = Növ
schema-models = Modellər
schema-acp = ACP agentləri
schema-id = ID
schema-name = Ad
schema-command = Əmr
schema-arguments = Arqumentlər
schema-environment = Mühit
schema-working-directory = İş qovluğu
schema-shell = Qabıq
schema-font-family = Şrift ailəsi
schema-startup-directory = Başlanğıc qovluğu
schema-themes = Mövzular
schema-color-scheme = Rəng sxemi
schema-font-size = Şrift ölçüsü
schema-line-height = Sətir hündürlüyü
schema-cursor-style = Kursor üslubu
schema-cursor-blink = Kursorun yanıb-sönməsi
schema-custom-themes = Fərdi mövzular
schema-foreground = Ön plan
schema-background = Arxa fon
schema-cursor = Kursor
schema-ansi-colors = ANSI rəngləri
schema-keymap = Klaviş xəritəsi
schema-explorer = Bələdçi
schema-visible = Görünür
schema-language-servers = Dil serverləri
schema-servers = Serverlər
schema-language-id = Dil ID
schema-root-markers = Kök markerləri
schema-output-directory = Çıxış qovluğu

menu-scene = Səhnə
menu-layout = Düzən
menu-terminal = Terminal
menu-browser = Brauzer
menu-service = Xidmət
menu-bookmark = Əlfəcin
menu-edit = Redaktə

layout-knowledge = Bilik
layout-open-knowledge = Biliyi aç
layout-open-welcome-knowledge = Biliyə xoş gəlmisiniz səhifəsini aç
layout-open-path = { $path } aç
layout-fold-knowledge = Biliyi yığ
layout-unfold-knowledge = Biliyi aç
layout-bookmarks = Əlfəcinlər
layout-new-folder = Yeni qovluq
layout-add-to-bookmarks = Əlfəcinlərə əlavə et
layout-move-to-bookmarks = Əlfəcinlərə köçür
layout-stack-number = Qat { $number }
layout-fold-stack = Qatı yığ
layout-unfold-stack = Qatı aç
layout-close-stack = Qatı bağla
layout-bookmark-in = { $folder } içində əlfəcinlə

common-cancel = Ləğv et
common-delete = Sil
common-save = Yadda saxla
common-rename = Adını dəyiş
common-expand = Genişləndir
common-collapse = Yığ
common-loading = Yüklənir…
common-error = Xəta
common-output = Çıxış
common-pending = Gözləyir
common-current = cari
common-stop = Dayandır
services-command = Vmux xidməti
services-uptime-seconds = { $seconds } san
services-uptime-minutes = { $minutes } dəq { $seconds } san
services-uptime-hours = { $hours } saat { $minutes } dəq
services-uptime-days = { $days } gün { $hours } saat

error-page-failed-load = Səhifə yüklənmədi
error-page-not-found = Səhifə tapılmadı
error-unknown-host = Naməlum Vmux tətbiq hostu: { $host }

history-title = Tarixçə

command-new-app-chat = Yeni { $provider }/{ $model } söhbəti (Tətbiq)
command-interactive-mode-user = Səhnə > İnteraktiv rejim > İstifadəçi
command-interactive-mode-player = Səhnə > İnteraktiv rejim > Oynadıcı
command-minimize-window = Düzən > Pəncərə > Kiçilt
command-toggle-layout = Düzən > Düzən > Düzəni dəyiş
command-close-tab = Düzən > Vərəq > Vərəqi bağla
command-new-task = Düzən > Vərəq > Yeni tapşırıq…
command-next-tab = Düzən > Vərəq > Növbəti vərəq
command-prev-tab = Düzən > Vərəq > Əvvəlki vərəq
command-rename-tab = Düzən > Vərəq > Vərəqin adını dəyiş
command-tab-select-1 = Düzən > Vərəq > 1-ci vərəqi seç
command-tab-select-2 = Düzən > Vərəq > 2-ci vərəqi seç
command-tab-select-3 = Düzən > Vərəq > 3-cü vərəqi seç
command-tab-select-4 = Düzən > Vərəq > 4-cü vərəqi seç
command-tab-select-5 = Düzən > Vərəq > 5-ci vərəqi seç
command-tab-select-6 = Düzən > Vərəq > 6-cı vərəqi seç
command-tab-select-7 = Düzən > Vərəq > 7-ci vərəqi seç
command-tab-select-8 = Düzən > Vərəq > 8-ci vərəqi seç
command-tab-select-last = Düzən > Vərəq > Son vərəqi seç
command-close-pane = Düzən > Panel > Paneli bağla
command-select-pane-left = Düzən > Panel > Soldakı paneli seç
command-select-pane-right = Düzən > Panel > Sağdakı paneli seç
command-select-pane-up = Düzən > Panel > Yuxarıdakı paneli seç
command-select-pane-down = Düzən > Panel > Aşağıdakı paneli seç
command-swap-pane-prev = Düzən > Panel > Paneli əvvəlki ilə dəyiş
command-swap-pane-next = Düzən > Panel > Paneli növbəti ilə dəyiş
command-equalize-pane-size = Düzən > Panel > Panel ölçülərini bərabərləşdir
command-resize-pane-left = Düzən > Panel > Paneli sola ölçüləndir
command-resize-pane-right = Düzən > Panel > Paneli sağa ölçüləndir
command-resize-pane-up = Düzən > Panel > Paneli yuxarı ölçüləndir
command-resize-pane-down = Düzən > Panel > Paneli aşağı ölçüləndir
command-stack-close = Düzən > Yığın > Yığını bağla
command-stack-next = Düzən > Yığın > Növbəti yığın
command-stack-previous = Düzən > Yığın > Əvvəlki yığın
command-stack-reopen = Düzən > Yığın > Bağlanmış səhifəni yenidən aç
command-stack-swap-prev = Düzən > Yığın > Yığını sola köçür
command-stack-swap-next = Düzən > Yığın > Yığını sağa köçür
command-space-open = Düzən > Məkan > Məkanlar
command-terminal-close = Terminal > Terminalı bağla
command-terminal-next = Terminal > Növbəti terminal
command-terminal-prev = Terminal > Əvvəlki terminal
command-terminal-clear = Terminal > Terminalı təmizlə
command-browser-prev-page = Brauzer > Naviqasiya > Geri
command-browser-next-page = Brauzer > Naviqasiya > İrəli
command-browser-reload = Brauzer > Naviqasiya > Yenilə
command-browser-hard-reload = Brauzer > Naviqasiya > Tam yenilə
command-open-in-place = Brauzer > Aç > Burada aç
command-open-in-new-stack = Brauzer > Aç > Yeni yığında aç
command-open-in-pane-top = Brauzer > Aç > Yuxarıdakı paneldə aç
command-open-in-pane-right = Brauzer > Aç > Sağ paneldə aç
command-open-in-pane-bottom = Brauzer > Aç > Aşağıdakı paneldə aç
command-open-in-pane-left = Brauzer > Aç > Sol paneldə aç
command-open-in-new-tab = Brauzer > Aç > Yeni vərəqdə aç
command-open-in-new-space = Brauzer > Aç > Yeni məkanda aç
command-browser-zoom-in = Brauzer > Görünüş > Yaxınlaşdır
command-browser-zoom-out = Brauzer > Görünüş > Uzaqlaşdır
command-browser-zoom-reset = Brauzer > Görünüş > Həqiqi ölçü
command-browser-dev-tools = Brauzer > Görünüş > Tərtibatçı alətləri
command-browser-open-command-bar = Brauzer > Zolaq > Əmr zolağı
command-browser-open-page-in-command-bar = Brauzer > Zolaq > Səhifəni redaktə et
command-browser-open-path-bar = Brauzer > Zolaq > Yol naviqatoru
command-browser-open-commands = Brauzer > Zolaq > Əmrlər
command-browser-open-history = Brauzer > Zolaq > Tarixçə
command-service-open = Xidmət > Xidmət monitorunu aç
command-bookmark-toggle-active = Əlfəcin > Səhifəni əlfəcinlə
command-bookmark-pin-active = Əlfəcin > Səhifəni sabitlə

layout-tab = Vərəq
layout-no-stacks = Yığın yoxdur
layout-loading = Yüklənir…
layout-no-markdown-files = Markdown faylı yoxdur
layout-empty-folder = Boş qovluq
layout-worktree = iş ağacı
layout-folder-name = Qovluq adı
layout-no-pins-bookmarks = Sabitlənmiş səhifə və ya əlfəcin yoxdur
layout-move-to = { $folder } qovluğuna köçür
layout-bookmark-current-page = Cari səhifəni əlfəcinlə
layout-rename-folder = Qovluğun adını dəyiş
layout-remove-folder = Qovluğu sil
layout-update-downloading = Yeniləmə endirilir
layout-update-installing = Yeniləmə quraşdırılır…
layout-update-ready = Yeni versiya əlçatandır
layout-restart-update = Yeniləmək üçün yenidən başladın

agent-preparing = Agent hazırlanır…
agent-send-all-queued = Növbədəki bütün promptları indi göndər (Esc)
agent-send = Göndər (Enter)
agent-ready = Hazır olanda yazın.
agent-loading-older = Köhnə mesajlar yüklənir…
agent-load-older = Köhnə mesajları yüklə
agent-continued-from = { $source } mənbəyindən davam etdirildi
agent-older-context-omitted = köhnə kontekst buraxılıb
agent-interrupted = yarımçıq kəsildi
agent-allow-tool = { $tool } alətinə icazə verilsin?
agent-deny = Rədd et
agent-allow-always = Həmişə icazə ver
agent-allow = İcazə ver
agent-loading-sessions = Sessiyalar yüklənir…
agent-no-resumable-sessions = Davam etdirilə bilən sessiya tapılmadı
agent-no-matching-sessions = Uyğun sessiya yoxdur
agent-no-matching-models = Uyğun model yoxdur
agent-choice-help = ↑/↓ və ya Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Repozitoriya qovluğunu seç
agent-choose-repository-detail = Agentin istifadə edəcəyi lokal Git repozitoriyasını seçin.
agent-choosing = Seçilir…
agent-choose-folder = Qovluq seç
agent-queued = növbədə
agent-attached = Əlavə edilib:
agent-cancel-queued = Növbədəki promptu ləğv et
agent-resume-queued = Növbədəki promptları davam etdir
agent-clear-queue = Növbəni təmizlə
agent-send-all-now = hamısını indi göndər
agent-choose-option = Yuxarıdan seçim edin
agent-loading-media = Media yüklənir…
agent-no-matching-media = Uyğun media yoxdur
agent-prompt-context = Prompt konteksti
agent-details = Təfərrüatlar
agent-path = Yol
agent-tool = Alət
agent-server = Server
agent-bytes = { $count } bayt
agent-worked-for = { $duration } işlədi
agent-worked-for-steps = { $count ->
    [one] { $duration } işlədi · 1 addım
   *[other] { $duration } işlədi · { $count } addım
}
agent-tool-guardian-review = Mühafizəçi baxışı
agent-tool-read-files = Faylları oxudu
agent-tool-viewed-image = Şəkilə baxdı
agent-tool-used-browser = Brauzerdən istifadə etdi
agent-tool-searched-files = Fayllarda axtardı
agent-tool-ran-commands = Əmrləri işlətdi
agent-thinking = Düşünür
agent-subagent = Alt agent
agent-prompt = Prompt
agent-thread = Mövzu
agent-parent = Üst
agent-children = Altlar
agent-call = Çağırış
agent-raw-event = Xam hadisə
agent-plan = Plan
agent-tasks = { $count ->
    [one] 1 tapşırıq
   *[other] { $count } tapşırıq
}
agent-edited = Redaktə edildi
agent-reconnecting = Yenidən qoşulur { $attempt }/{ $total }
agent-status-running = İşləyir
agent-status-done = Hazırdır
agent-status-failed = Uğursuz oldu
agent-status-pending = Gözləyir
agent-slash-attach-files = Fayllar əlavə et
agent-slash-resume-session = Keçmiş sessiyanı davam etdir
agent-slash-select-model = Model seç
agent-slash-continue-cli = Bu sessiyanı CLI-də davam etdir
agent-session-just-now = indicə
agent-session-minutes-ago = { $count } dəq əvvəl
agent-session-hours-ago = { $count } saat əvvəl
agent-session-days-ago = { $count } gün əvvəl
agent-working-working = İşləyir
agent-working-thinking = Düşünür
agent-working-pondering = Fikirləşir
agent-working-noodling = İdeyalar yoxlayır
agent-working-percolating = Yetişdirir
agent-working-conjuring = Ortaya çıxarır
agent-working-cooking = Hazırlayır
agent-working-brewing = Dəmləyir
agent-working-musing = Düşüncələrə dalır
agent-working-ruminating = Götür-qoy edir
agent-working-scheming = Plan qurur
agent-working-synthesizing = Sintez edir
agent-working-tinkering = Qurdalayır
agent-working-churning = İşləyib-hazırlayır
agent-working-vibing = Ritmə düşür
agent-working-simmering = Bişirir
agent-working-crafting = Formalaşdırır
agent-working-divining = Axtarıb tapır
agent-working-mulling = Götür-qoy edir
agent-working-spelunking = Dərinliklərə enir

editor-toggle-explorer = Bələdçini aç/bağla (Cmd+B)
editor-unsaved = yadda saxlanmayıb
editor-rendered-markdown = Canlı redaktə ilə göstərilmiş Markdown
editor-note = Qeyd
editor-source-editor = Mənbə redaktoru
editor-editor = Redaktor
editor-git-diff = Git fərqi
editor-diff = Fərq
editor-tidy = Səliqəyə sal
editor-always = Həmişə
editor-unchanged-previews = { $count ->
    [one] ✦ 1 dəyişməmiş önbaxış
   *[other] ✦ { $count } dəyişməmiş önbaxış
}
editor-open-externally = Xarici tətbiqdə aç
editor-changed-line = Dəyişmiş sətir
editor-go-to-definition = Tərifə keç
editor-find-references = İstinadları tap
editor-references = { $count ->
    [one] 1 istinad
   *[other] { $count } istinad
}
editor-lsp-starting = { $server } başladılır…
editor-lsp-not-installed = { $server } — quraşdırılmayıb
editor-explorer = Bələdçi
editor-open-editors = Açıq redaktorlar
editor-outline = Struktur
editor-new-file = Yeni fayl
editor-new-folder = Yeni qovluq
editor-delete-confirm = “{ $name }” silinsin? Bunu geri qaytarmaq mümkün deyil.
editor-created-folder = { $name } qovluğu yaradıldı
editor-created-file = { $name } faylı yaradıldı
editor-renamed-to = Adı { $name } olaraq dəyişdirildi
editor-deleted = { $name } silindi
editor-failed-decode-image = Şəkli dekodlamaq alınmadı
editor-preview-large-image = şəkil (önbaxış üçün çox böyükdür)
editor-preview-binary = binar
editor-preview-file = fayl

git-status-clean = təmiz
git-status-modified = dəyişdirilib
git-status-staged = indekslənib
git-status-staged-modified = indekslənib*
git-status-untracked = izlənmir
git-status-deleted = silinib
git-status-conflict = münaqişə
git-accept-all = ✓ hamısını qəbul et
git-unstage = İndeksdən çıxar
git-confirm-deny-all = Hamısını rədd etməyi təsdiqlə
git-deny-all = ✗ hamısını rədd et
git-commit-message = kommit mesajı
git-commit = Kommit et ({ $count })
git-push = ↑ Göndər
git-loading-diff = Fərq yüklənir…
git-no-changes = Göstəriləcək dəyişiklik yoxdur
git-accept = ✓ qəbul et
git-deny = ✗ rədd et
git-show-unchanged-lines = { $count } dəyişməmiş sətri göstər

terminal-loading = Yüklənir…
terminal-runs-when-ready = hazır olanda işə düşür · Ctrl+C təmizləyir · Esc ötürür
terminal-booting = başladılır
terminal-type-command = əmr yazın · hazır olanda işə düşür · Esc ötürür

setup-tagline-claude = Anthropic-in kodlaşdırma agenti, Vmux-da
setup-tagline-codex = OpenAI-ın kodlaşdırma agenti, Vmux-da
setup-tagline-vibe = Mistral-ın kodlaşdırma agenti, Vmux-da
setup-install-title = { $name } CLI quraşdır
setup-homebrew-required = { $command } quraşdırmaq üçün Homebrew lazımdır və hələ qurulmayıb. Vmux əvvəlcə Homebrew, sonra { $name } quraşdıracaq.
setup-terminal-instructions = Terminalda başlamaq üçün Return basın, istənəndə Mac parolunuzu daxil edin.
setup-command-missing = Lokal { $command } əmri hələ quraşdırılmadığı üçün Vmux bu səhifəni açdı. Onu əldə etmək üçün aşağıdakı əmri işə salın.
setup-install-failed = Quraşdırma tamamlanmadı. Təfərrüatlar üçün terminalı yoxlayın, sonra yenidən cəhd edin.
setup-installing = Quraşdırılır…
setup-install-homebrew = Homebrew + { $name } quraşdır
setup-run-install = Quraşdırma əmrini işə sal
setup-auto-reload = Vmux bunu terminalda işə salır və { $command } hazır olanda yenidən yükləyir.

debug-title = Sazlama
debug-auto-update = Avtomatik yenilə
debug-simulate-update = Yeniləmə əlçatanlığını simulyasiya et
debug-simulate-download = Endirməni simulyasiya et
debug-clear-update = Yeniləməni təmizlə
debug-trigger-restart = Yenidən başlatmanı işə sal

command-manage-spaces = Məkanları idarə et…
command-pane-stack-location = panel { $pane } / qat { $stack }
command-space-pane-stack-location = { $space } / panel { $pane } / qat { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = İnteraktiv rejim
command-group-window = Pəncərə
command-group-tab = Vərəq
command-group-pane = Panel
command-group-stack = Qat
command-group-space = Məkan
command-group-navigation = Naviqasiya
command-group-open = Aç
command-group-view = Görünüş
command-group-bar = Panel

menu-close-vmux = Vmux-u bağla

agents-terminal-coding-agent = Terminal əsaslı kodlaşdırma agenti
