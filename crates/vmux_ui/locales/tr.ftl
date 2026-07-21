common-open = Aç
common-close = Kapat
common-install = Yükle
common-uninstall = Kaldır
common-update = Güncelle
common-retry = Yeniden Dene
common-refresh = Yenile
common-remove = Kaldır
common-enable = Etkinleştir
common-disable = Devre Dışı Bırak
common-new = Yeni
common-active = etkin
common-running = çalışıyor
common-done = tamamlandı
common-failed = Başarısız
common-installed = Yüklendi
common-items = { $count ->
    [one] { $count } öge
   *[other] { $count } öge
}
start-title = Başlangıç
start-tagline = Tek komut. Her şey, tamam.

agents-title = Ajanlar
agents-search = ACP ve CLI ajanlarını ara…
agents-empty = Eşleşen ajan yok
agents-empty-detail = Bir ad, çalışma zamanı veya ACP/CLI deneyin.
agents-install-failed = Yükleme başarısız
agents-updating = Güncelleniyor…
agents-retrying = Yeniden deneniyor…
agents-preparing = Hazırlanıyor…

extensions-title = Uzantılar
extensions-search = Yüklü veya Chrome Web Mağazası'nda ara…
extensions-relaunch = Uygulamak için yeniden başlat
extensions-empty = Yüklü uzantı yok
extensions-no-match = Eşleşen uzantı yok
extensions-empty-detail = Yukarıdan Chrome Web Mağazası'nı arayın ve Return tuşuna basın.
extensions-no-match-detail = Başka bir ad veya uzantı kimliği deneyin.
extensions-on = Açık
extensions-off = Kapalı
extensions-enable-confirm = { $name } etkinleştirilsin mi?
extensions-enable-permissions = { $name } etkinleştir ve izin ver:

lsp-title = Dil Sunucuları
lsp-search = Dil sunucuları, linter'lar, formatlayıcılar ara…
lsp-loading = Katalog yükleniyor…
lsp-empty = Eşleşen dil sunucusu yok
lsp-empty-detail = Başka bir dil, linter veya formatlayıcı deneyin.
lsp-needs = { $tool } gerekli
lsp-status-available = Mevcut
lsp-status-on-path = PATH'te
lsp-status-installing = Yükleniyor…
lsp-status-installed = Yüklendi
lsp-status-outdated = Güncelleme mevcut
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
team-empty = Henüz kimse yok
team-you = Siz
team-agent = Ajan

services-title = Arka Plan Hizmetleri
services-processes = { $count ->
    [one] 1 süreç
   *[other] { $count } süreç
}
services-kill-all = Tümünü Sonlandır
services-not-running = Hizmet çalışmıyor
services-start-with = Şununla başlat:
services-empty = Etkin süreç yok
services-filter = Süreçleri filtrele…
services-no-match = Eşleşen süreç yok
services-connected = Bağlı
services-disconnected = Bağlantı Kesildi
services-attached = ekli
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
settings-stored = ~/.vmux/settings.ron dosyasında saklandı
settings-other = Diğer
settings-software-update = Yazılım Güncellemesi
settings-check-updates = Güncellemeleri Kontrol Et
settings-check-updates-hint = Otomatik güncelleme etkinken başlangıçta ve her saatte bir otomatik olarak kontrol eder.
settings-update-unavailable = Kullanılamıyor
settings-update-unavailable-hint = Güncelleyici bu yapıya dahil değil.
settings-update-checking = Kontrol ediliyor…
settings-update-checking-hint = Güncellemeler kontrol ediliyor…
settings-update-check-again = Tekrar Kontrol Et
settings-update-current = Vmux güncel.
settings-update-downloading = İndiriliyor…
settings-update-downloading-hint = Vmux { $version } indiriliyor…
settings-update-installing = Yükleniyor…
settings-update-installing-hint = Vmux { $version } yükleniyor…
settings-update-ready = Güncelleme Hazır
settings-update-ready-hint = Vmux { $version } hazır. Uygulamak için yeniden başlatın.
settings-update-try-again = Tekrar Dene
settings-update-failed = Güncellemeler kontrol edilemiyor.
settings-item = Öge
settings-item-number = Öge { $number }
settings-press-key = Bir tuşa basın…
settings-saved = Kaydedildi
settings-record-key = Yeni tuş kombinasyonu kaydetmek için tıklayın

tray-open-window = Pencereyi Aç
tray-close-window = Pencereyi Kapat
tray-pause-recording = Kaydı Duraklat
tray-resume-recording = Kaydı Sürdür
tray-finish-recording = Kaydı Bitir
tray-quit = Vmux'tan Çık

composer-attach-files = Dosya ekle (/upload)
composer-remove-attachment = Eki kaldır

layout-back = Geri
layout-forward = İleri
layout-reload = Yenile
layout-bookmark-page = Bu sayfayı yer imlerine ekle
layout-remove-bookmark = Yer imini kaldır
layout-pin-page = Bu sayfayı sabitle
layout-unpin-page = Bu sayfanın sabitlemesini kaldır
layout-manage-extensions = Uzantıları yönet
layout-new-stack = Yeni Yığın
layout-close-tab = Sekmeyi kapat
layout-bookmark = Yer İmi
layout-pin = Sabitle
layout-new-tab = Yeni sekme
layout-team = Ekip

command-switch-space = Alan değiştir…
command-search-ask = Ara veya sor…
command-new-tab-placeholder = Ara veya URL girin ya da Terminal'i seçin…
command-placeholder = URL girin, sekmelerde arayın veya komutlar için >…
command-composer-placeholder = Komutlar için / veya medya için @ yazın
command-send = Gönder (Enter)
command-terminal = Terminal
command-open-terminal = Terminal'de Aç
command-stack = Yığın
command-tabs = { $count ->
    [one] 1 sekme
   *[other] { $count } sekme
}
command-prompt = Komut İstemi
command-new-tab = Yeni sekme
command-search = Ara
command-open-value = "{ $value }" aç
command-search-value = "{ $value }" ara

schema-appearance = Görünüm
schema-general = Genel
schema-layout = Düzen
schema-layout-detail = Pencere, bölmeler, kenar çubuğu ve odak halkası.
schema-agent = Ajan
schema-agent-detail = Ajan davranışı ve araç izinleri.
schema-shortcuts = Kısayollar
schema-shortcuts-detail = Salt okunur görünüm. Bağlantıları değiştirmek için settings.ron'u doğrudan düzenleyin.
schema-terminal = Terminal
schema-browser = Tarayıcı
schema-mode = Mod
schema-mode-detail = Web sayfaları için renk şeması. Cihaz sisteminizi izler.
schema-device = Cihaz
schema-light = Açık
schema-dark = Koyu
schema-language = Dil
schema-language-detail = Sistem, en-US, ja veya eşleşen ~/.vmux/locales/<tag>.ftl kataloğu olan herhangi bir BCP 47 etiketi kullanın.
schema-auto-update = Otomatik Güncelleme
schema-auto-update-detail = Başlangıçta ve her saatte bir güncellemeleri kontrol edin ve yükleyin.
schema-startup-url = Başlangıç URL'si
schema-startup-url-detail = Boş bırakmak komut çubuğunu açar.
schema-search-engine = Arama motoru
schema-search-engine-detail = Başlangıç ve komut çubuğundan web aramaları için kullanılır.
schema-window = Pencere
schema-pane = Bölme
schema-side-sheet = Yan panel
schema-focus-ring = Odak halkası
schema-run-placement = Çalıştırma yerleşimini geçersiz kılmaya izin ver
schema-run-placement-detail = Ajanların çalıştırma bölmesi modunu, yönünü ve çapasını seçmesine izin verin.
schema-leader = Lider tuşu
schema-leader-detail = Akort kısayolları için ön ek tuşu.
schema-chord-timeout = Akort zaman aşımı
schema-chord-timeout-detail = Akort ön ekinin sona ermesinden önceki milisaniye.
schema-bindings = Bağlantılar
schema-confirm-close = Kapatmayı onayla
schema-confirm-close-detail = Çalışan bir süreç içeren terminali kapatmadan önce sor.
schema-default-theme = Varsayılan tema
schema-default-theme-detail = Temalar listesinden etkin temanın adı.
