locale-name = Türkçe
common-open = Aç
common-close = Kapat
common-install = Yükle
common-uninstall = Kaldır
common-update = Güncelle
common-retry = Yeniden dene
common-refresh = Yenile
common-remove = Sil
common-enable = Etkinleştir
common-disable = Devre dışı bırak
common-new = Yeni
common-active = etkin
common-running = çalışıyor
common-done = tamamlandı
common-failed = Başarısız
common-installed = Yüklendi
common-items = { $count ->
    [one] { $count } öğe
   *[other] { $count } öğe
}

tools-title = Araçlar
tools-search = Paketleri, aracıları, MCP'yi, dil araçlarını ve yapılandırma dosyalarını ara…
tools-open = Araçları aç
tools-fold = Araçları daralt
tools-unfold = Araçları genişlet
tools-scanning = Yerel araçlar taranıyor…
tools-no-installed = Yüklü araç yok
tools-empty = Eşleşen araç yok
tools-empty-detail = Bir paket yükleyin veya Stow tarzı bir yapılandırma dosyaları paketi ekleyin.
tools-apply = Uygula
tools-homebrew = Homebrew
tools-homebrew-sync = Yüklü formüller ve uygulamalar otomatik olarak eşitlenir.
tools-open-brewfile = Brewfile'ı aç
tools-managed = yönetiliyor
tools-provider-homebrew-formulae = Homebrew formülleri
tools-provider-homebrew-casks = Homebrew uygulamaları
tools-provider-npm = npm paketleri
tools-provider-acp-agents = ACP aracıları
tools-provider-language-tools = Dil araçları
tools-provider-mcp-servers = MCP sunucuları
tools-provider-dotfiles = Yapılandırma dosyaları
tools-status-available = Kullanılabilir
tools-status-missing = Eksik
tools-status-conflict = Çakışma
tools-forget = Unut
tools-manage = Yönet
tools-link = Bağla
tools-unlink = Bağlantıyı kaldır
tools-import = İçe aktar
tools-update-count = { $count ->
    [one] 1 güncelleme
   *[other] { $count } güncelleme
}
tools-conflict-count = { $count ->
    [one] 1 çakışma
   *[other] { $count } çakışma
}
tools-result-applied = Araçlar uygulandı
tools-result-imported = Araçlar içe aktarıldı
tools-result-installed = { $name } yüklendi
tools-result-updated = { $name } güncellendi
tools-result-uninstalled = { $name } kaldırıldı
tools-result-forgotten = { $name } unutuldu
tools-result-managed = { $name } artık yönetiliyor
tools-result-linked = { $name } bağlandı
tools-result-unlinked = { $name } bağlantısı kaldırıldı

start-title = Başlat
start-tagline = Tek prompt. Her şey hazır.

agents-title = Ajanlar
agents-search = ACP ve CLI ajanlarında ara…
agents-empty = Eşleşen ajan yok
agents-empty-detail = Ad, çalışma zamanı ya da ACP/CLI deneyin.
agents-install-failed = Yükleme başarısız
agents-updating = Güncelleniyor…
agents-retrying = Yeniden deneniyor…
agents-preparing = Hazırlanıyor…

extensions-title = Uzantılar
extensions-search = Yüklü uzantılarda veya Chrome Web Store’da ara…
extensions-relaunch = Uygulamak için yeniden başlat
extensions-empty = Yüklü uzantı yok
extensions-no-match = Eşleşen uzantı yok
extensions-empty-detail = Yukarıdan Chrome Web Store’da arama yapıp Return tuşuna basın.
extensions-no-match-detail = Başka bir ad veya uzantı kimliği deneyin.
extensions-on = Açık
extensions-off = Kapalı
extensions-enable-confirm = { $name } etkinleştirilsin mi?
extensions-enable-permissions = { $name } etkinleştirilsin ve şunlara izin verilsin:

lsp-title = Dil Sunucuları
lsp-search = Dil sunucuları, linter’lar, biçimlendiricilerde ara…
lsp-loading = Katalog yükleniyor…
lsp-empty = Eşleşen dil sunucusu yok
lsp-empty-detail = Başka bir dil, linter veya biçimlendirici deneyin.
lsp-needs = { $tool } gerekiyor
lsp-status-available = Kullanılabilir
lsp-status-on-path = PATH’te
lsp-status-installing = Yükleniyor…
lsp-status-installed = Yüklendi
lsp-status-outdated = Güncelleme var
lsp-status-running = Çalışıyor
lsp-status-failed = Başarısız

spaces-title = Alanlar
spaces-new-placeholder = Yeni alan adı
spaces-empty = Alan yok
spaces-default-name = Alan { $number }
spaces-tabs = { $count ->
    [one] 1 sekme
   *[other] { $count } sekme
}
spaces-delete = Alanı sil

team-title = Ekip
team-just-you = Bu alanda yalnızca siz varsınız
team-agents = { $count ->
    [one] Siz ve 1 ajan
   *[other] Siz ve { $count } ajan
}
team-empty = Burada henüz kimse yok
team-you = Siz
team-agent = Ajan

services-title = Arka Plan Servisleri
services-processes = { $count ->
    [one] 1 işlem
   *[other] { $count } işlem
}
services-kill-all = Tümünü Sonlandır
services-not-running = Servis çalışmıyor
services-start-with = Şununla başlat:
services-empty = Etkin işlem yok
services-filter = İşlemleri filtrele…
services-no-match = Eşleşen işlem yok
services-connected = Bağlı
services-disconnected = Bağlantı kesildi
services-attached = bağlı
services-kill = Sonlandır
services-memory = Bellek
services-size = Boyut
services-shell = Kabuk

error-title = Hata

history-search = Geçmişte ara
history-clear-all = Tümünü temizle
history-clear-confirm = Tüm geçmiş temizlensin mi?
history-clear-warning = Bu işlem geri alınamaz.
history-cancel = İptal
history-today = Bugün
history-yesterday = Dün
history-days-ago = { $count } gün önce
history-day-offset = Gün -{ $count }

settings-title = Ayarlar
settings-loading = Ayarlar yükleniyor…
settings-stored = ~/.vmux/settings.ron içinde saklanır
settings-other = Diğer
settings-software-update = Yazılım Güncelleme
settings-check-updates = Güncellemeleri Denetle
settings-check-updates-hint = Otomatik güncelleme açıkken başlangıçta ve her saat otomatik denetler.
settings-update-unavailable = Kullanılamıyor
settings-update-unavailable-hint = Güncelleyici bu derlemeye dahil değil.
settings-update-checking = Denetleniyor…
settings-update-checking-hint = Güncellemeler denetleniyor…
settings-update-check-again = Yeniden Denetle
settings-update-current = Vmux güncel.
settings-update-downloading = İndiriliyor…
settings-update-downloading-hint = Vmux { $version } indiriliyor…
settings-update-installing = Yükleniyor…
settings-update-installing-hint = Vmux { $version } yükleniyor…
settings-update-ready = Güncelleme Hazır
settings-update-ready-hint = Vmux { $version } hazır. Uygulamak için yeniden başlatın.
settings-update-try-again = Tekrar Dene
settings-update-failed = Güncellemeler denetlenemedi.
settings-item = Öğe
settings-item-number = Öğe { $number }
settings-press-key = Bir tuşa basın…
settings-saved = Kaydedildi
settings-record-key = Yeni bir tuş kombinasyonu kaydetmek için tıklayın

tray-open-window = Pencereyi Aç
tray-close-window = Pencereyi Kapat
tray-pause-recording = Kaydı Duraklat
tray-resume-recording = Kaydı Sürdür
tray-finish-recording = Kaydı Bitir
tray-quit = Vmux’tan Çık

composer-attach-files = Dosya ekle (/upload)
composer-remove-attachment = Eki kaldır

layout-back = Geri
layout-forward = İleri
layout-reload = Yeniden yükle
layout-bookmark-page = Bu sayfayı yer imlerine ekle
layout-remove-bookmark = Yer imini kaldır
layout-pin-page = Bu sayfayı sabitle
layout-unpin-page = Bu sayfanın sabitlemesini kaldır
layout-manage-extensions = Uzantıları yönet
layout-new-stack = Yeni yığın
layout-close-tab = Sekmeyi kapat
layout-bookmark = Yer imi
layout-pin = Sabitle
layout-new-tab = Yeni sekme
layout-team = Ekip

command-switch-space = Alan değiştir…
command-search-ask = Ara veya sor…
command-new-tab-placeholder = Ara, URL yaz veya Terminal’i seç…
command-placeholder = URL yazın, sekmelerde arayın veya komutlar için > yazın…
command-composer-placeholder = Komutlar için /, medya için @ yazın
command-send = Gönder (Enter)
command-terminal = Terminal
command-open-terminal = Terminal’de aç
command-stack = Yığın
command-tabs = { $count ->
    [one] 1 sekme
   *[other] { $count } sekme
}
command-prompt = Prompt
command-new-tab = Yeni sekme
command-search = Ara
command-open-value = “{ $value }” aç
command-search-value = “{ $value }” ara

schema-appearance = Görünüm
schema-general = Genel
schema-layout = Yerleşim
schema-layout-detail = Pencere, bölmeler, kenar çubuğu ve odak halkası.
schema-agent = Ajan
schema-agent-detail = Ajan davranışı ve araç izinleri.
schema-shortcuts = Kısayollar
schema-shortcuts-detail = Salt okunur görünüm. Kısayolları değiştirmek için settings.ron dosyasını doğrudan düzenleyin.
schema-terminal = Terminal
schema-browser = Tarayıcı
schema-mode = Mod
schema-mode-detail = Web sayfaları için renk düzeni. Aygıt, sistem ayarınızı izler.
schema-device = Aygıt
schema-light = Açık
schema-dark = Koyu
schema-language = Dil
schema-language-detail = Sistem dilini, en-US, ja ya da eşleşen bir ~/.vmux/locales/<tag>.ftl kataloğu olan herhangi bir BCP 47 etiketini kullanın.
schema-auto-update = Otomatik güncelleme
schema-auto-update-detail = Başlangıçta ve her saat güncellemeleri denetleyip yükle.
schema-startup-url = Başlangıç URL’si
schema-startup-url-detail = Boş bırakılırsa komut çubuğu prompt’u açılır.
schema-search-engine = Arama motoru
schema-search-engine-detail = Başlat ekranından ve komut çubuğundan yapılan web aramalarında kullanılır.
schema-window = Pencere
schema-pane = Bölme
schema-side-sheet = Yan panel
schema-focus-ring = Odak halkası
schema-run-placement = Çalıştırma yerleşimini geçersiz kılmaya izin ver
schema-run-placement-detail = Ajanların çalıştırma bölmesi modunu, yönünü ve çapasını seçmesine izin ver.
schema-leader = Leader
schema-leader-detail = Chord kısayolları için önek tuşu.
schema-chord-timeout = Chord zaman aşımı
schema-chord-timeout-detail = Chord önekinin süresi dolmadan önce geçecek milisaniye.
schema-bindings = Bağlantılar
schema-confirm-close = Kapatmayı onayla
schema-confirm-close-detail = Çalışan işlemi olan bir terminali kapatmadan önce sor.
schema-default-theme = Varsayılan tema
schema-default-theme-detail = Tema listesindeki etkin temanın adı.

settings-empty = (boş)
settings-none = (yok)

schema-system = Sistem
schema-editor = Düzenleyici
schema-recording = Kayıt
schema-radius = Yarıçap
schema-padding = Dolgu
schema-gap = Boşluk
schema-width = Genişlik
schema-color = Renk
schema-red = Kırmızı
schema-green = Yeşil
schema-blue = Mavi
schema-follow-files = Dosyaları izle
schema-tidy-files = Dosyaları toparla
schema-tidy-files-max = Dosya toparlama eşiği
schema-tidy-files-auto = Dosyaları otomatik toparla
schema-app-providers = Uygulama sağlayıcıları
schema-provider = Sağlayıcı
schema-kind = Tür
schema-models = Modeller
schema-acp = ACP ajanları
schema-id = ID
schema-name = Ad
schema-command = Komut
schema-arguments = Argümanlar
schema-environment = Ortam
schema-working-directory = Çalışma dizini
schema-shell = Kabuk
schema-font-family = Yazı tipi ailesi
schema-startup-directory = Başlangıç dizini
schema-themes = Temalar
schema-color-scheme = Renk şeması
schema-font-size = Yazı tipi boyutu
schema-line-height = Satır yüksekliği
schema-cursor-style = İmleç stili
schema-cursor-blink = İmleç yanıp sönmesi
schema-custom-themes = Özel temalar
schema-foreground = Ön plan
schema-background = Arka plan
schema-cursor = İmleç
schema-ansi-colors = ANSI renkleri
schema-keymap = Tuş eşlemi
schema-explorer = Gezgin
schema-visible = Görünür
schema-language-servers = Dil sunucuları
schema-servers = Sunucular
schema-language-id = Dil ID
schema-root-markers = Kök işaretçileri
schema-output-directory = Çıktı dizini

menu-scene = Sahne
menu-layout = Yerleşim
menu-terminal = Terminal
menu-browser = Tarayıcı
menu-service = Servis
menu-bookmark = Yer İmi
menu-edit = Düzen

layout-knowledge = Bilgi
layout-open-knowledge = Bilgiyi Aç
layout-open-welcome-knowledge = Bilgiye Hoş Geldiniz’i Aç
layout-open-path = { $path } Yolunu Aç
layout-fold-knowledge = Bilgiyi daralt
layout-unfold-knowledge = Bilgiyi genişlet
layout-bookmarks = Yer imleri
layout-new-folder = Yeni Klasör
layout-add-to-bookmarks = Yer İmlerine Ekle
layout-move-to-bookmarks = Yer İmlerine Taşı
layout-stack-number = Yığın { $number }
layout-fold-stack = Yığını daralt
layout-unfold-stack = Yığını genişlet
layout-close-stack = Yığını kapat
layout-bookmark-in = { $folder } içine yer imi ekle

common-cancel = İptal
common-delete = Sil
common-save = Kaydet
common-rename = Yeniden adlandır
common-expand = Genişlet
common-collapse = Daralt
common-loading = Yükleniyor…
common-error = Hata
common-output = Çıktı
common-pending = Beklemede
common-current = geçerli
common-stop = Durdur
services-command = Vmux servisi
services-uptime-seconds = { $seconds } sn
services-uptime-minutes = { $minutes } dk { $seconds } sn
services-uptime-hours = { $hours } sa { $minutes } dk
services-uptime-days = { $days } gn { $hours } sa

error-page-failed-load = Sayfa yüklenemedi
error-page-not-found = Sayfa bulunamadı
error-unknown-host = Bilinmeyen Vmux uygulama ana makinesi: { $host }

history-title = Geçmiş

command-new-app-chat = Yeni { $provider }/{ $model } sohbeti (Uygulama)
command-interactive-mode-user = Sahne > Etkileşimli Mod > Kullanıcı
command-interactive-mode-player = Sahne > Etkileşimli Mod > Oyuncu
command-minimize-window = Yerleşim > Pencere > Simge durumuna küçült
command-toggle-layout = Yerleşim > Yerleşim > Yerleşimi değiştir
command-close-tab = Yerleşim > Sekme > Sekmeyi kapat
command-new-task = Yerleşim > Sekme > Yeni görev…
command-next-tab = Yerleşim > Sekme > Sonraki sekme
command-prev-tab = Yerleşim > Sekme > Önceki sekme
command-rename-tab = Yerleşim > Sekme > Sekmeyi yeniden adlandır
command-tab-select-1 = Yerleşim > Sekme > Sekme 1’i seç
command-tab-select-2 = Yerleşim > Sekme > Sekme 2’yi seç
command-tab-select-3 = Yerleşim > Sekme > Sekme 3’ü seç
command-tab-select-4 = Yerleşim > Sekme > Sekme 4’ü seç
command-tab-select-5 = Yerleşim > Sekme > Sekme 5’i seç
command-tab-select-6 = Yerleşim > Sekme > Sekme 6’yı seç
command-tab-select-7 = Yerleşim > Sekme > Sekme 7’yi seç
command-tab-select-8 = Yerleşim > Sekme > Sekme 8’i seç
command-tab-select-last = Yerleşim > Sekme > Son sekmeyi seç
command-close-pane = Yerleşim > Bölme > Bölmeyi kapat
command-select-pane-left = Yerleşim > Bölme > Soldaki bölmeyi seç
command-select-pane-right = Yerleşim > Bölme > Sağdaki bölmeyi seç
command-select-pane-up = Yerleşim > Bölme > Üstteki bölmeyi seç
command-select-pane-down = Yerleşim > Bölme > Alttaki bölmeyi seç
command-swap-pane-prev = Yerleşim > Bölme > Bölmeyi öncekiyle değiştir
command-swap-pane-next = Yerleşim > Bölme > Bölmeyi sonrakiyle değiştir
command-equalize-pane-size = Yerleşim > Bölme > Bölme boyutlarını eşitle
command-resize-pane-left = Yerleşim > Bölme > Bölmeyi sola doğru boyutlandır
command-resize-pane-right = Yerleşim > Bölme > Bölmeyi sağa doğru boyutlandır
command-resize-pane-up = Yerleşim > Bölme > Bölmeyi yukarı doğru boyutlandır
command-resize-pane-down = Yerleşim > Bölme > Bölmeyi aşağı doğru boyutlandır
command-stack-close = Yerleşim > Yığın > Yığını kapat
command-stack-next = Yerleşim > Yığın > Sonraki yığın
command-stack-previous = Yerleşim > Yığın > Önceki yığın
command-stack-reopen = Yerleşim > Yığın > Kapatılan sayfayı yeniden aç
command-stack-swap-prev = Yerleşim > Yığın > Yığını sola taşı
command-stack-swap-next = Yerleşim > Yığın > Yığını sağa taşı
command-space-open = Yerleşim > Alan > Alanlar
command-terminal-close = Terminal > Terminali kapat
command-terminal-next = Terminal > Sonraki terminal
command-terminal-prev = Terminal > Önceki terminal
command-terminal-clear = Terminal > Terminali temizle
command-browser-prev-page = Tarayıcı > Gezinme > Geri
command-browser-next-page = Tarayıcı > Gezinme > İleri
command-browser-reload = Tarayıcı > Gezinme > Yeniden yükle
command-browser-hard-reload = Tarayıcı > Gezinme > Tam yeniden yükle
command-open-in-place = Tarayıcı > Aç > Burada aç
command-open-in-new-stack = Tarayıcı > Aç > Yeni yığında aç
command-open-in-pane-top = Tarayıcı > Aç > Üstteki bölmede aç
command-open-in-pane-right = Tarayıcı > Aç > Sağdaki bölmede aç
command-open-in-pane-bottom = Tarayıcı > Aç > Alttaki bölmede aç
command-open-in-pane-left = Tarayıcı > Aç > Soldaki bölmede aç
command-open-in-new-tab = Tarayıcı > Aç > Yeni sekmede aç
command-open-in-new-space = Tarayıcı > Aç > Yeni alanda aç
command-browser-zoom-in = Tarayıcı > Görünüm > Yakınlaştır
command-browser-zoom-out = Tarayıcı > Görünüm > Uzaklaştır
command-browser-zoom-reset = Tarayıcı > Görünüm > Gerçek boyut
command-browser-dev-tools = Tarayıcı > Görünüm > Geliştirici araçları
command-browser-open-command-bar = Tarayıcı > Çubuk > Komut çubuğu
command-browser-open-page-in-command-bar = Tarayıcı > Çubuk > Sayfayı düzenle
command-browser-open-path-bar = Tarayıcı > Çubuk > Yol gezgini
command-browser-open-commands = Tarayıcı > Çubuk > Komutlar
command-browser-open-history = Tarayıcı > Çubuk > Geçmiş
command-service-open = Servis > Servis izleyicisini aç
command-bookmark-toggle-active = Yer imi > Sayfayı yer imlerine ekle
command-bookmark-pin-active = Yer imi > Sayfayı sabitle

layout-tab = Sekme
layout-no-stacks = Yığın yok
layout-loading = Yükleniyor…
layout-no-markdown-files = Markdown dosyası yok
layout-empty-folder = Boş klasör
layout-worktree = çalışma ağacı
layout-folder-name = Klasör adı
layout-no-pins-bookmarks = Sabitlenen veya yer imi yok
layout-move-to = { $folder } klasörüne taşı
layout-bookmark-current-page = Geçerli sayfayı yer imlerine ekle
layout-rename-folder = Klasörü yeniden adlandır
layout-remove-folder = Klasörü kaldır
layout-update-downloading = Güncelleme indiriliyor
layout-update-installing = Güncelleme yükleniyor…
layout-update-ready = Yeni sürüm mevcut
layout-restart-update = Güncellemek için yeniden başlat

agent-preparing = Ajan hazırlanıyor…
agent-send-all-queued = Kuyruktaki tüm istemleri şimdi gönder (Esc)
agent-send = Gönder (Enter)
agent-ready = Hazır olduğunuzda başlayabiliriz.
agent-loading-older = Eski mesajlar yükleniyor…
agent-load-older = Eski mesajları yükle
agent-continued-from = { $source } üzerinden devam edildi
agent-older-context-omitted = eski bağlam atlandı
agent-interrupted = kesintiye uğradı
agent-allow-tool = { $tool } aracına izin verilsin mi?
agent-deny = Reddet
agent-allow-always = Her zaman izin ver
agent-allow = İzin ver
agent-loading-sessions = Oturumlar yükleniyor…
agent-no-resumable-sessions = Sürdürülebilir oturum bulunamadı
agent-no-matching-sessions = Eşleşen oturum yok
agent-no-matching-models = Eşleşen model yok
agent-choice-help = ↑/↓ veya Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Depo klasörünü seç
agent-choose-repository-detail = Ajanın kullanacağı yerel Git deposunu seçin.
agent-choosing = Seçiliyor…
agent-choose-folder = Klasör seç
agent-queued = kuyrukta
agent-attached = Ekli:
agent-cancel-queued = Kuyruktaki istemi iptal et
agent-resume-queued = Kuyruktaki istemlere devam et
agent-clear-queue = Kuyruğu temizle
agent-send-all-now = tümünü şimdi gönder
agent-choose-option = Yukarıdan bir seçenek seçin
agent-loading-media = Medya yükleniyor…
agent-no-matching-media = Eşleşen medya yok
agent-prompt-context = İstem bağlamı
agent-details = Ayrıntılar
agent-path = Yol
agent-tool = Araç
agent-server = Sunucu
agent-bytes = { $count } bayt
agent-worked-for = { $duration } çalıştı
agent-worked-for-steps = { $count ->
    [one] { $duration } çalıştı · 1 adım
   *[other] { $duration } çalıştı · { $count } adım
}
agent-tool-guardian-review = Guardian incelemesi
agent-tool-read-files = Dosyaları okudu
agent-tool-viewed-image = Görseli görüntüledi
agent-tool-used-browser = Tarayıcı kullandı
agent-tool-searched-files = Dosyalarda arama yaptı
agent-tool-ran-commands = Komut çalıştırdı
agent-thinking = Düşünüyor
agent-subagent = Alt ajan
agent-prompt = İstem
agent-thread = Konu
agent-parent = Üst
agent-children = Altlar
agent-call = Çağrı
agent-raw-event = Ham olay
agent-plan = Plan
agent-tasks = { $count ->
    [one] 1 görev
   *[other] { $count } görev
}
agent-edited = Düzenlendi
agent-reconnecting = Yeniden bağlanıyor { $attempt }/{ $total }
agent-status-running = Çalışıyor
agent-status-done = Tamamlandı
agent-status-failed = Başarısız
agent-status-pending = Beklemede
agent-slash-attach-files = Dosya ekle
agent-slash-resume-session = Önceki bir oturuma devam et
agent-slash-select-model = Model seç
agent-slash-continue-cli = Bu oturuma CLI’da devam et
agent-session-just-now = az önce
agent-session-minutes-ago = { $count } dk önce
agent-session-hours-ago = { $count } sa önce
agent-session-days-ago = { $count } gn önce
agent-working-working = Çalışıyor
agent-working-thinking = Düşünüyor
agent-working-pondering = Kafa yoruyor
agent-working-noodling = Fikir deniyor
agent-working-percolating = Olgunlaştırıyor
agent-working-conjuring = Bir şeyler çıkarıyor
agent-working-cooking = Pişiriyor
agent-working-brewing = Demliyor
agent-working-musing = Düşünüp taşınıyor
agent-working-ruminating = Üzerinde düşünüyor
agent-working-scheming = Plan kuruyor
agent-working-synthesizing = Sentezliyor
agent-working-tinkering = Kurcalıyor
agent-working-churning = İşliyor
agent-working-vibing = Akışta
agent-working-simmering = Kısık ateşte pişiriyor
agent-working-crafting = Şekillendiriyor
agent-working-divining = Sezmeye çalışıyor
agent-working-mulling = Tartıyor
agent-working-spelunking = Derinlere iniyor

editor-toggle-explorer = Explorer’ı aç/kapat (Cmd+B)
editor-unsaved = kaydedilmemiş
editor-rendered-markdown = Canlı düzenlemeli işlenmiş Markdown
editor-note = Not
editor-source-editor = Kaynak düzenleyici
editor-editor = Düzenleyici
editor-git-diff = Git farkı
editor-diff = Fark
editor-tidy = Toparla
editor-always = Her zaman
editor-unchanged-previews = { $count ->
    [one] ✦ 1 değişmemiş önizleme
   *[other] ✦ { $count } değişmemiş önizleme
}
editor-open-externally = Harici olarak aç
editor-changed-line = Değişen satır
editor-go-to-definition = Tanıma git
editor-find-references = Başvuruları bul
editor-references = { $count ->
    [one] 1 başvuru
   *[other] { $count } başvuru
}
editor-lsp-starting = { $server } başlatılıyor…
editor-lsp-not-installed = { $server } — yüklü değil
editor-explorer = Explorer
editor-open-editors = Açık düzenleyiciler
editor-outline = Ana hat
editor-new-file = Yeni dosya
editor-new-folder = Yeni klasör
editor-delete-confirm = “{ $name }” silinsin mi? Bu işlem geri alınamaz.
editor-created-folder = { $name } klasörü oluşturuldu
editor-created-file = { $name } dosyası oluşturuldu
editor-renamed-to = { $name } olarak yeniden adlandırıldı
editor-deleted = { $name } silindi
editor-failed-decode-image = Görsel çözümlenemedi
editor-preview-large-image = görsel (önizleme için çok büyük)
editor-preview-binary = ikili
editor-preview-file = dosya

git-status-clean = temiz
git-status-modified = değiştirilmiş
git-status-staged = hazırlanmış
git-status-staged-modified = hazırlanmış*
git-status-untracked = izlenmeyen
git-status-deleted = silinmiş
git-status-conflict = çakışma
git-accept-all = ✓ tümünü kabul et
git-unstage = Hazırlıktan çıkar
git-confirm-deny-all = Tümünü reddetmeyi onayla
git-deny-all = ✗ tümünü reddet
git-commit-message = commit mesajı
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Fark yükleniyor…
git-no-changes = Gösterilecek değişiklik yok
git-accept = ✓ kabul et
git-deny = ✗ reddet
git-show-unchanged-lines = Değişmemiş { $count } satırı göster

terminal-loading = Yükleniyor…
terminal-runs-when-ready = hazır olunca çalışır · Ctrl+C temizler · Esc atlar
terminal-booting = başlatılıyor
terminal-type-command = komut yazın · hazır olunca çalışır · Esc atlar

setup-tagline-claude = Anthropic’in kodlama ajanı, Vmux içinde
setup-tagline-codex = OpenAI’ın kodlama ajanı, Vmux içinde
setup-tagline-vibe = Mistral’in kodlama ajanı, Vmux içinde
setup-install-title = { $name } CLI’ı yükle
setup-homebrew-required = { $command } yüklemek için Homebrew gerekiyor ve henüz ayarlanmamış. Vmux önce Homebrew’i, ardından { $name } yükleyecek.
setup-terminal-instructions = Terminalde başlatmak için Return tuşuna basın, ardından istendiğinde Mac parolanızı girin.
setup-command-missing = Yerel { $command } komutu henüz yüklü olmadığı için Vmux bu sayfayı açtı. Edinmek için aşağıdaki komutu çalıştırın.
setup-install-failed = Yükleme tamamlanmadı. Ayrıntılar için terminali kontrol edin, sonra yeniden deneyin.
setup-installing = Yükleniyor…
setup-install-homebrew = Homebrew + { $name } yükle
setup-run-install = Yükleme komutunu çalıştır
setup-auto-reload = Vmux bunu bir terminalde çalıştırır ve { $command } hazır olduğunda yeniden yükler.

debug-title = Hata ayıklama
debug-auto-update = Otomatik güncelle
debug-simulate-update = Güncelleme varmış gibi simüle et
debug-simulate-download = İndirmeyi simüle et
debug-clear-update = Güncellemeyi temizle
debug-trigger-restart = Yeniden başlatmayı tetikle

command-manage-spaces = Alanları yönet…
command-pane-stack-location = bölme { $pane } / yığın { $stack }
command-space-pane-stack-location = { $space } / bölme { $pane } / yığın { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Etkileşimli Mod
command-group-window = Pencere
command-group-tab = Sekme
command-group-pane = Bölme
command-group-stack = Yığın
command-group-space = Alan
command-group-navigation = Gezinme
command-group-open = Aç
command-group-view = Görünüm
command-group-bar = Çubuk

menu-close-vmux = Vmux’ı Kapat

agents-terminal-coding-agent = Terminal tabanlı kodlama ajanı
