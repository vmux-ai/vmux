common-open = باز کردن
common-close = بستن
common-install = نصب
common-uninstall = حذف نصب
common-update = به‌روزرسانی
common-retry = تلاش مجدد
common-refresh = تازه‌سازی
common-remove = حذف
common-enable = فعال‌سازی
common-disable = غیرفعال‌سازی
common-new = جدید
common-active = فعال
common-running = در حال اجرا
common-done = انجام شد
common-failed = ناموفق
common-installed = نصب شده
common-items = { $count ->
    [one] { $count } مورد
   *[other] { $count } مورد
}
start-title = شروع
start-tagline = یک دستور. هر چیزی، انجام شده.

agents-title = عاملان
agents-search = جستجوی عاملان ACP و CLI…
agents-empty = هیچ عاملی یافت نشد
agents-empty-detail = نام، محیط اجرا، یا ACP/CLI را امتحان کنید.
agents-install-failed = نصب ناموفق بود
agents-updating = در حال به‌روزرسانی…
agents-retrying = در حال تلاش مجدد…
agents-preparing = در حال آماده‌سازی…

extensions-title = افزونه‌ها
extensions-search = جستجوی افزونه‌های نصب‌شده یا فروشگاه Chrome…
extensions-relaunch = برای اعمال، راه‌اندازی مجدد کنید
extensions-empty = هیچ افزونه‌ای نصب نشده
extensions-no-match = هیچ افزونه‌ای یافت نشد
extensions-empty-detail = فروشگاه Chrome Web Store را در بالا جستجو کنید و Return را فشار دهید.
extensions-no-match-detail = نام یا شناسه افزونه دیگری را امتحان کنید.
extensions-on = روشن
extensions-off = خاموش
extensions-enable-confirm = { $name } فعال شود؟
extensions-enable-permissions = { $name } را فعال کنید و اجازه دهید:

lsp-title = سرورهای زبانی
lsp-search = جستجوی سرورهای زبانی، لینترها، قالب‌بندها…
lsp-loading = در حال بارگذاری فهرست…
lsp-empty = هیچ سرور زبانی یافت نشد
lsp-empty-detail = زبان، لینتر، یا قالب‌بند دیگری را امتحان کنید.
lsp-needs = نیاز به { $tool }
lsp-status-available = در دسترس
lsp-status-on-path = در PATH
lsp-status-installing = در حال نصب…
lsp-status-installed = نصب شده
lsp-status-outdated = به‌روزرسانی موجود است
lsp-status-running = در حال اجرا
lsp-status-failed = ناموفق

spaces-title = فضاها
spaces-new-placeholder = نام فضای جدید
spaces-empty = هیچ فضایی وجود ندارد
spaces-default-name = فضا { $number }
spaces-tabs = { $count ->
    [one] 1 برگه
   *[other] { $count } برگه
}
spaces-delete = حذف فضا

team-title = تیم
team-just-you = فقط شما در این فضا
team-agents = { $count ->
    [one] شما و 1 عامل
   *[other] شما و { $count } عامل
}
team-empty = هنوز کسی اینجا نیست
team-you = شما
team-agent = عامل

services-title = سرویس‌های پس‌زمینه
services-processes = { $count ->
    [one] 1 فرآیند
   *[other] { $count } فرآیند
}
services-kill-all = پایان همه
services-not-running = سرویس در حال اجرا نیست
services-start-with = شروع با:
services-empty = هیچ فرآیند فعالی وجود ندارد
services-filter = فیلتر فرآیندها…
services-no-match = هیچ فرآیندی یافت نشد
services-connected = متصل
services-disconnected = قطع شده
services-attached = وابسته
services-kill = خاتمه
services-memory = حافظه
services-size = اندازه
services-shell = پوسته

error-title = خطا

history-search = جستجوی تاریخچه
history-clear-all = پاک کردن همه
history-clear-confirm = همه تاریخچه پاک شود؟
history-clear-warning = این عمل قابل بازگشت نیست.
history-cancel = لغو
history-today = امروز
history-yesterday = دیروز
history-days-ago = { $count } روز پیش
history-day-offset = روز -{ $count }

settings-title = تنظیمات
settings-loading = در حال بارگذاری تنظیمات…
settings-stored = ذخیره‌شده در ~/.vmux/settings.ron
settings-other = سایر
settings-software-update = به‌روزرسانی نرم‌افزار
settings-check-updates = بررسی به‌روزرسانی‌ها
settings-check-updates-hint = در هنگام راه‌اندازی و هر ساعت یک‌بار (در صورت فعال‌بودن به‌روزرسانی خودکار) بررسی می‌شود.
settings-update-unavailable = در دسترس نیست
settings-update-unavailable-hint = به‌روزرسان در این نسخه موجود نیست.
settings-update-checking = در حال بررسی…
settings-update-checking-hint = در حال بررسی به‌روزرسانی‌ها…
settings-update-check-again = بررسی مجدد
settings-update-current = Vmux به‌روز است.
settings-update-downloading = در حال دانلود…
settings-update-downloading-hint = در حال دانلود Vmux { $version }…
settings-update-installing = در حال نصب…
settings-update-installing-hint = در حال نصب Vmux { $version }…
settings-update-ready = به‌روزرسانی آماده است
settings-update-ready-hint = Vmux { $version } آماده است. برای اعمال تغییرات راه‌اندازی مجدد کنید.
settings-update-try-again = تلاش مجدد
settings-update-failed = بررسی به‌روزرسانی‌ها ناموفق بود.
settings-item = مورد
settings-item-number = مورد { $number }
settings-press-key = یک کلید فشار دهید…
settings-saved = ذخیره شد
settings-record-key = برای ضبط ترکیب کلید جدید کلیک کنید

tray-open-window = باز کردن پنجره
tray-close-window = بستن پنجره
tray-pause-recording = توقف ضبط
tray-resume-recording = ادامه ضبط
tray-finish-recording = پایان ضبط
tray-quit = خروج از Vmux

composer-attach-files = پیوست فایل‌ها (/upload)
composer-remove-attachment = حذف پیوست

layout-back = برگشت
layout-forward = جلو
layout-reload = بارگذاری مجدد
layout-bookmark-page = نشانه‌گذاری این صفحه
layout-remove-bookmark = حذف نشانه‌گذاری
layout-pin-page = سنجاق کردن این صفحه
layout-unpin-page = برداشتن سنجاق این صفحه
layout-manage-extensions = مدیریت افزونه‌ها
layout-new-stack = پشته جدید
layout-close-tab = بستن برگه
layout-bookmark = نشانه‌گذاری
layout-pin = سنجاق
layout-new-tab = برگه جدید
layout-team = تیم

command-switch-space = تغییر فضا…
command-search-ask = جستجو یا پرسش…
command-new-tab-placeholder = جستجو یا URL وارد کنید، یا پایانه را انتخاب کنید…
command-placeholder = URL وارد کنید، برگه‌ها را جستجو کنید، یا > برای دستورات…
command-composer-placeholder = / برای دستورات یا @ برای رسانه تایپ کنید
command-send = ارسال (Enter)
command-terminal = پایانه
command-open-terminal = باز کردن در پایانه
command-stack = پشته
command-tabs = { $count ->
    [one] 1 برگه
   *[other] { $count } برگه
}
command-prompt = دستور
command-new-tab = برگه جدید
command-search = جستجو
command-open-value = باز کردن "{ $value }"
command-search-value = جستجوی "{ $value }"

schema-appearance = ظاهر
schema-general = عمومی
schema-layout = چیدمان
schema-layout-detail = پنجره، پانل‌ها، نوار کناری، و حلقه تمرکز.
schema-agent = عامل
schema-agent-detail = رفتار عامل و مجوزهای ابزار.
schema-shortcuts = میانبرها
schema-shortcuts-detail = نمای فقط‌خواندنی. برای تغییر کلیدبندها مستقیماً settings.ron را ویرایش کنید.
schema-terminal = پایانه
schema-browser = مرورگر
schema-mode = حالت
schema-mode-detail = طرح رنگی برای صفحات وب. دستگاه از تنظیمات سیستم شما پیروی می‌کند.
schema-device = دستگاه
schema-light = روشن
schema-dark = تاریک
schema-language = زبان
schema-language-detail = از سیستم، en-US، ja، یا هر برچسب BCP 47 با فهرست ~/.vmux/locales/<tag>.ftl مربوطه استفاده کنید.
schema-auto-update = به‌روزرسانی خودکار
schema-auto-update-detail = در هنگام راه‌اندازی و هر ساعت یک‌بار به‌روزرسانی‌ها را بررسی و نصب کنید.
schema-startup-url = URL راه‌اندازی
schema-startup-url-detail = خالی گذاشتن نوار دستورات را باز می‌کند.
schema-search-engine = موتور جستجو
schema-search-engine-detail = برای جستجوهای وب از Start و نوار دستورات استفاده می‌شود.
schema-window = پنجره
schema-pane = پانل
schema-side-sheet = صفحه کناری
schema-focus-ring = حلقه تمرکز
schema-run-placement = اجازه تغییر محل اجرا
schema-run-placement-detail = به عاملان اجازه دهید حالت، جهت، و لنگر پانل اجرا را انتخاب کنند.
schema-leader = کلید رهبر
schema-leader-detail = کلید پیشوند برای میانبرهای ترکیبی.
schema-chord-timeout = مهلت ترکیب کلید
schema-chord-timeout-detail = میلی‌ثانیه قبل از انقضای پیشوند ترکیب.
schema-bindings = کلیدبندها
schema-confirm-close = تأیید بستن
schema-confirm-close-detail = قبل از بستن پایانه‌ای با فرآیند در حال اجرا تأیید بخواهید.
schema-default-theme = تم پیش‌فرض
schema-default-theme-detail = نام تم فعال از فهرست تم‌ها.
