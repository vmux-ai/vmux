common-open = باز کردن
common-close = بستن
common-install = نصب
common-uninstall = حذف نصب
common-update = به‌روزرسانی
common-retry = تلاش دوباره
common-refresh = بازخوانی
common-remove = حذف
common-enable = فعال کردن
common-disable = غیرفعال کردن
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
start-tagline = یک پرامپت. هر کاری، انجام می‌شود.

agents-title = عامل‌ها
agents-search = جست‌وجوی عامل‌های ACP و CLI…
agents-empty = عامل مطابقی پیدا نشد
agents-empty-detail = نام، محیط اجرا یا ACP/CLI را امتحان کنید.
agents-install-failed = نصب ناموفق بود
agents-updating = در حال به‌روزرسانی…
agents-retrying = در حال تلاش دوباره…
agents-preparing = در حال آماده‌سازی…

extensions-title = افزونه‌ها
extensions-search = جست‌وجو در افزونه‌های نصب‌شده یا Chrome Web Store…
extensions-relaunch = برای اعمال، دوباره راه‌اندازی کنید
extensions-empty = افزونه‌ای نصب نشده است
extensions-no-match = افزونه مطابقی پیدا نشد
extensions-empty-detail = در بالا Chrome Web Store را جست‌وجو کنید و Enter را بزنید.
extensions-no-match-detail = نام یا شناسهٔ افزونهٔ دیگری را امتحان کنید.
extensions-on = روشن
extensions-off = خاموش
extensions-enable-confirm = { $name } فعال شود؟
extensions-enable-permissions = { $name } را فعال کنید و اجازه دهید:

lsp-title = سرورهای زبان
lsp-search = جست‌وجوی سرورهای زبان، لینترها، قالب‌بندها…
lsp-loading = در حال بارگذاری کاتالوگ…
lsp-empty = سرور زبان مطابقی پیدا نشد
lsp-empty-detail = زبان، لینتر یا قالب‌بند دیگری را امتحان کنید.
lsp-needs = به { $tool } نیاز دارد
lsp-status-available = موجود
lsp-status-on-path = روی PATH
lsp-status-installing = در حال نصب…
lsp-status-installed = نصب شده
lsp-status-outdated = به‌روزرسانی موجود است
lsp-status-running = در حال اجرا
lsp-status-failed = ناموفق

spaces-title = فضاها
spaces-new-placeholder = نام فضای جدید
spaces-empty = فضایی وجود ندارد
spaces-default-name = فضای { $number }
spaces-tabs = { $count ->
    [one] 1 زبانه
   *[other] { $count } زبانه
}
spaces-delete = حذف فضا

team-title = تیم
team-just-you = فقط شما در این فضا هستید
team-agents = { $count ->
    [one] شما و 1 عامل
   *[other] شما و { $count } عامل
}
team-empty = هنوز کسی اینجا نیست
team-you = شما
team-agent = عامل

services-title = سرویس‌های پس‌زمینه
services-processes = { $count ->
    [one] 1 فرایند
   *[other] { $count } فرایند
}
services-kill-all = پایان اجباری همه
services-not-running = سرویس در حال اجرا نیست
services-start-with = شروع با:
services-empty = فرایند فعالی وجود ندارد
services-filter = فیلتر کردن فرایندها…
services-no-match = فرایند مطابقی پیدا نشد
services-connected = متصل
services-disconnected = قطع‌شده
services-attached = پیوست‌شده
services-kill = پایان اجباری
services-memory = حافظه
services-size = اندازه
services-shell = شل

error-title = خطا

history-search = جست‌وجو در تاریخچه
history-clear-all = پاک کردن همه
history-clear-confirm = همهٔ تاریخچه پاک شود؟
history-clear-warning = این کار قابل بازگشت نیست.
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
settings-check-updates-hint = در صورت فعال بودن به‌روزرسانی خودکار، هنگام اجرا و هر ساعت به‌طور خودکار بررسی می‌شود.
settings-update-unavailable = در دسترس نیست
settings-update-unavailable-hint = به‌روزرسان در این بیلد موجود نیست.
settings-update-checking = در حال بررسی…
settings-update-checking-hint = در حال بررسی به‌روزرسانی‌ها…
settings-update-check-again = بررسی دوباره
settings-update-current = Vmux به‌روز است.
settings-update-downloading = در حال دانلود…
settings-update-downloading-hint = در حال دانلود Vmux { $version }…
settings-update-installing = در حال نصب…
settings-update-installing-hint = در حال نصب Vmux { $version }…
settings-update-ready = به‌روزرسانی آماده است
settings-update-ready-hint = Vmux { $version } آماده است. برای اعمال، برنامه را دوباره راه‌اندازی کنید.
settings-update-try-again = تلاش دوباره
settings-update-failed = بررسی به‌روزرسانی‌ها ممکن نیست.
settings-item = مورد
settings-item-number = مورد { $number }
settings-press-key = کلیدی را فشار دهید…
settings-saved = ذخیره شد
settings-record-key = برای ثبت میان‌بر جدید کلیک کنید

tray-open-window = باز کردن پنجره
tray-close-window = بستن پنجره
tray-pause-recording = مکث ضبط
tray-resume-recording = ادامهٔ ضبط
tray-finish-recording = پایان ضبط
tray-quit = خروج از Vmux

composer-attach-files = پیوست کردن فایل‌ها (/upload)
composer-remove-attachment = حذف پیوست

layout-back = بازگشت
layout-forward = جلو
layout-reload = بازخوانی
layout-bookmark-page = نشانک‌گذاری این صفحه
layout-remove-bookmark = حذف نشانک
layout-pin-page = سنجاق کردن این صفحه
layout-unpin-page = برداشتن سنجاق این صفحه
layout-manage-extensions = مدیریت افزونه‌ها
layout-new-stack = استک جدید
layout-close-tab = بستن زبانه
layout-bookmark = نشانک
layout-pin = سنجاق
layout-new-tab = زبانهٔ جدید
layout-team = تیم

command-switch-space = تغییر فضا…
command-search-ask = جست‌وجو یا پرسش…
command-new-tab-placeholder = جست‌وجو کنید، URL وارد کنید یا Terminal را انتخاب کنید…
command-placeholder = URL وارد کنید، زبانه‌ها را جست‌وجو کنید، یا برای فرمان‌ها > بزنید…
command-composer-placeholder = برای فرمان‌ها / یا برای رسانه @ تایپ کنید
command-send = ارسال (Enter)
command-terminal = ترمینال
command-open-terminal = باز کردن در ترمینال
command-stack = استک
command-tabs = { $count ->
    [one] 1 زبانه
   *[other] { $count } زبانه
}
command-prompt = پرامپت
command-new-tab = زبانهٔ جدید
command-search = جست‌وجو
command-open-value = باز کردن «{ $value }»
command-search-value = جست‌وجوی «{ $value }»

schema-appearance = ظاهر
schema-general = عمومی
schema-layout = چیدمان
schema-layout-detail = پنجره، پنل‌ها، نوار کناری و حلقهٔ فوکوس.
schema-agent = عامل
schema-agent-detail = رفتار عامل و مجوزهای ابزار.
schema-shortcuts = میان‌برها
schema-shortcuts-detail = نمای فقط‌خواندنی. برای تغییر کلیدها، settings.ron را مستقیم ویرایش کنید.
schema-terminal = ترمینال
schema-browser = مرورگر
schema-mode = حالت
schema-mode-detail = طرح رنگ صفحه‌های وب. Device از سیستم شما پیروی می‌کند.
schema-device = Device
schema-light = روشن
schema-dark = تیره
schema-language = زبان
schema-language-detail = از زبان سیستم، en-US، ja یا هر برچسب BCP 47 با کاتالوگ مطابق در ~/.vmux/locales/<tag>.ftl استفاده کنید.
schema-auto-update = به‌روزرسانی خودکار
schema-auto-update-detail = هنگام اجرا و هر ساعت، به‌روزرسانی‌ها را بررسی و نصب می‌کند.
schema-startup-url = URL آغازین
schema-startup-url-detail = اگر خالی باشد، پرامپت نوار فرمان باز می‌شود.
schema-search-engine = موتور جست‌وجو
schema-search-engine-detail = برای جست‌وجوهای وب از شروع و نوار فرمان استفاده می‌شود.
schema-window = پنجره
schema-pane = پنل
schema-side-sheet = برگهٔ کناری
schema-focus-ring = حلقهٔ فوکوس
schema-run-placement = اجازهٔ بازنویسی محل اجرا
schema-run-placement-detail = به عامل‌ها اجازه دهید حالت پنل اجرا، جهت و لنگر را انتخاب کنند.
schema-leader = لیدر
schema-leader-detail = کلید پیشوند برای میان‌برهای دنباله‌ای.
schema-chord-timeout = مهلت دنباله
schema-chord-timeout-detail = میلی‌ثانیه تا منقضی شدن پیشوند دنباله.
schema-bindings = کلیدها
schema-confirm-close = تأیید بستن
schema-confirm-close-detail = پیش از بستن ترمینالی که فرایند در حال اجرا دارد، سؤال شود.
schema-default-theme = تم پیش‌فرض
schema-default-theme-detail = نام تم فعال از فهرست تم‌ها.
