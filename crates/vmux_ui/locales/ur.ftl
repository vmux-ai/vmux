common-open = کھولیں
common-close = بند کریں
common-install = انسٹال کریں
common-uninstall = ان انسٹال کریں
common-update = اپ ڈیٹ کریں
common-retry = دوبارہ کوشش کریں
common-refresh = تازہ کریں
common-remove = ہٹائیں
common-enable = فعال کریں
common-disable = غیر فعال کریں
common-new = نیا
common-active = فعال
common-running = چل رہا ہے
common-done = مکمل
common-failed = ناکام
common-installed = انسٹال شدہ
common-items = { $count ->
    [one] { $count } آئٹم
   *[other] { $count } آئٹمز
}
start-title = شروع
start-tagline = ایک پرامپٹ۔ کچھ بھی، مکمل۔

agents-title = ایجنٹس
agents-search = ACP اور CLI ایجنٹس تلاش کریں…
agents-empty = کوئی مماثل ایجنٹ نہیں
agents-empty-detail = کوئی نام، رن ٹائم، یا ACP/CLI آزمائیں۔
agents-install-failed = انسٹال ناکام ہوا
agents-updating = اپ ڈیٹ ہو رہا ہے…
agents-retrying = دوبارہ کوشش ہو رہی ہے…
agents-preparing = تیاری ہو رہی ہے…

extensions-title = ایکسٹینشنز
extensions-search = انسٹال شدہ یا Chrome Web Store تلاش کریں…
extensions-relaunch = لاگو کرنے کے لیے دوبارہ لانچ کریں
extensions-empty = کوئی ایکسٹینشن انسٹال نہیں
extensions-no-match = کوئی مماثل ایکسٹینشن نہیں
extensions-empty-detail = اوپر Chrome Web Store تلاش کریں اور Return دبائیں۔
extensions-no-match-detail = کوئی اور نام یا ایکسٹینشن ID آزمائیں۔
extensions-on = چالو
extensions-off = بند
extensions-enable-confirm = { $name } فعال کریں؟
extensions-enable-permissions = { $name } فعال کریں اور اجازت دیں:

lsp-title = لینگویج سرورز
lsp-search = لینگویج سرورز، لنٹرز، فارمیٹرز تلاش کریں…
lsp-loading = کیٹلاگ لوڈ ہو رہا ہے…
lsp-empty = کوئی مماثل لینگویج سرور نہیں
lsp-empty-detail = کوئی اور زبان، لنٹر، یا فارمیٹر آزمائیں۔
lsp-needs = { $tool } درکار ہے
lsp-status-available = دستیاب
lsp-status-on-path = PATH پر
lsp-status-installing = انسٹال ہو رہا ہے…
lsp-status-installed = انسٹال شدہ
lsp-status-outdated = اپ ڈیٹ دستیاب ہے
lsp-status-running = چل رہا ہے
lsp-status-failed = ناکام

spaces-title = اسپیسز
spaces-new-placeholder = نئی اسپیس کا نام
spaces-empty = کوئی اسپیس نہیں
spaces-default-name = اسپیس { $number }
spaces-tabs = { $count ->
    [one] 1 ٹیب
   *[other] { $count } ٹیبز
}
spaces-delete = اسپیس حذف کریں

team-title = ٹیم
team-just-you = اس اسپیس میں صرف آپ
team-agents = { $count ->
    [one] آپ اور 1 ایجنٹ
   *[other] آپ اور { $count } ایجنٹس
}
team-empty = ابھی یہاں کوئی نہیں
team-you = آپ
team-agent = ایجنٹ

services-title = بیک گراؤنڈ سروسز
services-processes = { $count ->
    [one] 1 پروسیس
   *[other] { $count } پروسیسز
}
services-kill-all = سب بند کریں
services-not-running = سروس نہیں چل رہی
services-start-with = اس سے شروع کریں:
services-empty = کوئی فعال پروسیس نہیں
services-filter = پروسیسز فلٹر کریں…
services-no-match = کوئی مماثل پروسیس نہیں
services-connected = جڑا ہوا
services-disconnected = منقطع
services-attached = منسلک
services-kill = بند کریں
services-memory = میموری
services-size = سائز
services-shell = Shell

error-title = خرابی

history-search = تاریخ تلاش کریں
history-clear-all = سب صاف کریں
history-clear-confirm = تمام تاریخ صاف کریں؟
history-clear-warning = یہ واپس نہیں ہو سکتا۔
history-cancel = منسوخ کریں
history-today = آج
history-yesterday = کل
history-days-ago = { $count } دن پہلے
history-day-offset = دن -{ $count }

settings-title = ترتیبات
settings-loading = ترتیبات لوڈ ہو رہی ہیں…
settings-stored = ~/.vmux/settings.ron میں محفوظ
settings-other = دیگر
settings-software-update = سافٹ ویئر اپ ڈیٹ
settings-check-updates = اپ ڈیٹس چیک کریں
settings-check-updates-hint = لانچ پر اور ہر گھنٹے خودکار چیک کرتا ہے جب خودکار اپ ڈیٹ فعال ہو۔
settings-update-unavailable = دستیاب نہیں
settings-update-unavailable-hint = اپ ڈیٹر اس بلڈ میں شامل نہیں۔
settings-update-checking = چیک ہو رہا ہے…
settings-update-checking-hint = اپ ڈیٹس چیک ہو رہی ہیں…
settings-update-check-again = دوبارہ چیک کریں
settings-update-current = Vmux تازہ ترین ہے۔
settings-update-downloading = ڈاؤن لوڈ ہو رہا ہے…
settings-update-downloading-hint = Vmux { $version } ڈاؤن لوڈ ہو رہا ہے…
settings-update-installing = انسٹال ہو رہا ہے…
settings-update-installing-hint = Vmux { $version } انسٹال ہو رہا ہے…
settings-update-ready = اپ ڈیٹ تیار ہے
settings-update-ready-hint = Vmux { $version } تیار ہے۔ لاگو کرنے کے لیے دوبارہ شروع کریں۔
settings-update-try-again = دوبارہ کوشش کریں
settings-update-failed = اپ ڈیٹس چیک کرنے میں ناکامی۔
settings-item = آئٹم
settings-item-number = آئٹم { $number }
settings-press-key = ایک کلید دبائیں…
settings-saved = محفوظ
settings-record-key = نئی کلید کمبو ریکارڈ کرنے کے لیے کلک کریں

tray-open-window = ونڈو کھولیں
tray-close-window = ونڈو بند کریں
tray-pause-recording = ریکارڈنگ روکیں
tray-resume-recording = ریکارڈنگ جاری رکھیں
tray-finish-recording = ریکارڈنگ ختم کریں
tray-quit = Vmux چھوڑیں

composer-attach-files = فائلیں منسلک کریں (/upload)
composer-remove-attachment = منسلک فائل ہٹائیں

layout-back = پیچھے
layout-forward = آگے
layout-reload = دوبارہ لوڈ کریں
layout-bookmark-page = یہ صفحہ بک مارک کریں
layout-remove-bookmark = بک مارک ہٹائیں
layout-pin-page = یہ صفحہ پن کریں
layout-unpin-page = یہ صفحہ ان پن کریں
layout-manage-extensions = ایکسٹینشنز منظم کریں
layout-new-stack = نیا اسٹیک
layout-close-tab = ٹیب بند کریں
layout-bookmark = بک مارک
layout-pin = پن
layout-new-tab = نیا ٹیب
layout-team = ٹیم

command-switch-space = اسپیس تبدیل کریں…
command-search-ask = تلاش کریں یا پوچھیں…
command-new-tab-placeholder = تلاش کریں یا URL ٹائپ کریں، یا Terminal منتخب کریں…
command-placeholder = URL ٹائپ کریں، ٹیبز تلاش کریں، یا > کمانڈز کے لیے…
command-composer-placeholder = کمانڈز کے لیے / یا میڈیا کے لیے @ ٹائپ کریں
command-send = بھیجیں (Enter)
command-terminal = ٹرمینل
command-open-terminal = ٹرمینل میں کھولیں
command-stack = اسٹیک
command-tabs = { $count ->
    [one] 1 ٹیب
   *[other] { $count } ٹیبز
}
command-prompt = پرامپٹ
command-new-tab = نیا ٹیب
command-search = تلاش
command-open-value = "{ $value }" کھولیں
command-search-value = "{ $value }" تلاش کریں

schema-appearance = ظاہری شکل
schema-general = عمومی
schema-layout = لے آؤٹ
schema-layout-detail = ونڈو، پینز، سائڈبار، اور فوکس رنگ۔
schema-agent = ایجنٹ
schema-agent-detail = ایجنٹ کا رویہ اور ٹول کی اجازتیں۔
schema-shortcuts = شارٹ کٹس
schema-shortcuts-detail = صرف پڑھنے کا منظر۔ بائنڈنگز تبدیل کرنے کے لیے settings.ron براہ راست ترمیم کریں۔
schema-terminal = ٹرمینل
schema-browser = براؤزر
schema-mode = موڈ
schema-mode-detail = ویب صفحات کے لیے رنگ اسکیم۔ ڈیوائس آپ کے سسٹم کی پیروی کرتی ہے۔
schema-device = ڈیوائس
schema-light = ہلکا
schema-dark = گہرا
schema-language = زبان
schema-language-detail = سسٹم، en-US، ja، یا کوئی بھی BCP 47 ٹیگ استعمال کریں جس کے ساتھ مماثل ~/.vmux/locales/<tag>.ftl کیٹلاگ ہو۔
schema-auto-update = خودکار اپ ڈیٹ
schema-auto-update-detail = لانچ پر اور ہر گھنٹے اپ ڈیٹس چیک اور انسٹال کریں۔
schema-startup-url = اسٹارٹ اپ URL
schema-startup-url-detail = خالی کمانڈ بار پرامپٹ کھولتا ہے۔
schema-search-engine = سرچ انجن
schema-search-engine-detail = Start اور کمانڈ بار سے ویب سرچ کے لیے استعمال ہوتا ہے۔
schema-window = ونڈو
schema-pane = پین
schema-side-sheet = سائڈ شیٹ
schema-focus-ring = فوکس رنگ
schema-run-placement = رن پلیسمنٹ اوورائڈ کی اجازت دیں
schema-run-placement-detail = ایجنٹس کو رن پین موڈ، سمت، اور اینکر منتخب کرنے دیں۔
schema-leader = لیڈر
schema-leader-detail = کورڈ شارٹ کٹس کے لیے پریفکس کلید۔
schema-chord-timeout = کورڈ ٹائم آؤٹ
schema-chord-timeout-detail = کورڈ پریفکس ختم ہونے سے پہلے ملی سیکنڈز۔
schema-bindings = بائنڈنگز
schema-confirm-close = بند کرنے کی تصدیق کریں
schema-confirm-close-detail = چلتے پروسیس کے ساتھ ٹرمینل بند کرنے سے پہلے پوچھیں۔
schema-default-theme = ڈیفالٹ تھیم
schema-default-theme-detail = تھیمز کی فہرست سے فعال تھیم کا نام۔
