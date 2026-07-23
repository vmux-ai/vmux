locale-name = فارسی
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

tools-title = ابزارها
tools-search = جست‌وجوی بسته‌ها، عامل‌ها، MCP، ابزارهای زبان و پرونده‌های پیکربندی…
tools-open = باز کردن ابزارها
tools-fold = جمع کردن ابزارها
tools-unfold = باز کردن ابزارها
tools-scanning = در حال بررسی ابزارهای محلی…
tools-no-installed = هیچ ابزاری نصب نشده است
tools-empty = هیچ ابزار منطبقی وجود ندارد
tools-empty-detail = یک بسته نصب کنید یا بسته‌ای از پرونده‌های پیکربندی به سبک Stow بیفزایید.
tools-apply = اعمال
tools-homebrew = Homebrew
tools-homebrew-sync = فرمول‌ها و برنامه‌های نصب‌شده به‌طور خودکار همگام می‌شوند.
tools-open-brewfile = باز کردن Brewfile
tools-managed = مدیریت‌شده
tools-provider-homebrew-formulae = فرمول‌های Homebrew
tools-provider-homebrew-casks = برنامه‌های Homebrew
tools-provider-npm = بسته‌های npm
tools-provider-acp-agents = عامل‌های ACP
tools-provider-language-tools = ابزارهای زبان
tools-provider-mcp-servers = سرورهای MCP
tools-provider-dotfiles = پرونده‌های پیکربندی
tools-status-available = در دسترس
tools-status-missing = موجود نیست
tools-status-conflict = تداخل
tools-forget = فراموش کردن
tools-manage = مدیریت
tools-link = پیوند دادن
tools-unlink = برداشتن پیوند
tools-import = وارد کردن
tools-update-count = { $count ->
    [one] ۱ به‌روزرسانی
   *[other] { $count } به‌روزرسانی
}
tools-conflict-count = { $count ->
    [one] ۱ تداخل
   *[other] { $count } تداخل
}
tools-result-applied = ابزارها اعمال شدند
tools-result-imported = ابزارها وارد شدند
tools-result-installed = { $name } نصب شد
tools-result-updated = { $name } به‌روزرسانی شد
tools-result-uninstalled = { $name } حذف شد
tools-result-forgotten = { $name } فراموش شد
tools-result-managed = { $name } اکنون مدیریت می‌شود
tools-result-linked = { $name } پیوند داده شد
tools-result-unlinked = پیوند { $name } برداشته شد
vault-title = Vault
vault-open = { common-open } Vault
vault-description = تنظیمات، ابزارها، dotfiles و Knowledge را با Git همگام کنید.
vault-sync = همگام سازی
vault-create = ایجاد کنید
vault-connect = اتصال
vault-private = مخزن خصوصی
vault-public-warning = مخازن عمومی دانش و پیکربندی شما را آشکار می کنند.
vault-choose-repository = انتخاب یک مخزن…
vault-empty = خالی
vault-clean = به روز
vault-not-connected = متصل نیست
vault-change-count = تغییرات: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

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

settings-empty = (خالی)
settings-none = (هیچ‌کدام)

schema-system = سیستم
schema-editor = ویرایشگر
schema-recording = ضبط
schema-radius = شعاع
schema-padding = فاصلهٔ داخلی
schema-gap = فاصله
schema-width = عرض
schema-color = رنگ
schema-red = قرمز
schema-green = سبز
schema-blue = آبی
schema-follow-files = دنبال‌کردن فایل‌ها
schema-tidy-files = مرتب‌سازی فایل‌ها
schema-tidy-files-max = آستانهٔ مرتب‌سازی فایل‌ها
schema-tidy-files-auto = مرتب‌سازی خودکار فایل‌ها
schema-app-providers = ارائه‌دهندگان برنامه
schema-provider = ارائه‌دهنده
schema-kind = نوع
schema-models = مدل‌ها
schema-acp = عامل‌های ACP
schema-id = ID
schema-name = نام
schema-command = فرمان
schema-arguments = آرگومان‌ها
schema-environment = محیط
schema-working-directory = پوشهٔ کاری
schema-shell = پوسته
schema-font-family = خانوادهٔ قلم
schema-startup-directory = پوشهٔ شروع
schema-themes = پوسته‌ها
schema-color-scheme = طرح رنگ
schema-font-size = اندازهٔ قلم
schema-line-height = ارتفاع خط
schema-cursor-style = سبک نشانگر
schema-cursor-blink = چشمک‌زدن نشانگر
schema-custom-themes = پوسته‌های سفارشی
schema-foreground = پیش‌زمینه
schema-background = پس‌زمینه
schema-cursor = نشانگر
schema-ansi-colors = رنگ‌های ANSI
schema-keymap = نگاشت کلیدها
schema-explorer = کاوشگر
schema-visible = نمایان
schema-language-servers = سرورهای زبان
schema-servers = سرورها
schema-language-id = ID زبان
schema-root-markers = نشانگرهای ریشه
schema-output-directory = پوشهٔ خروجی

menu-scene = صحنه
menu-layout = چیدمان
menu-terminal = پایانه
menu-browser = مرورگر
menu-service = سرویس
menu-bookmark = نشانک
menu-edit = ویرایش

layout-knowledge = دانش
layout-open-knowledge = بازکردن دانش
layout-open-welcome-knowledge = بازکردن خوشامدگویی دانش
layout-open-path = بازکردن { $path }
layout-fold-knowledge = جمع‌کردن دانش
layout-unfold-knowledge = بازکردن دانش
layout-bookmarks = نشانک‌ها
layout-new-folder = پوشهٔ جدید
layout-add-to-bookmarks = افزودن به نشانک‌ها
layout-move-to-bookmarks = انتقال به نشانک‌ها
layout-stack-number = پشتهٔ { $number }
layout-fold-stack = جمع‌کردن پشته
layout-unfold-stack = بازکردن پشته
layout-close-stack = بستن پشته
layout-bookmark-in = نشانک‌گذاری در { $folder }

common-cancel = لغو
common-delete = حذف
common-save = ذخیره
common-rename = تغییر نام
common-expand = باز کردن
common-collapse = بستن
common-loading = در حال بارگذاری…
common-error = خطا
common-output = خروجی
common-pending = در انتظار
common-current = فعلی
common-stop = توقف
services-command = سرویس Vmux
services-uptime-seconds = { $seconds }ث
services-uptime-minutes = { $minutes }د { $seconds }ث
services-uptime-hours = { $hours }س { $minutes }د
services-uptime-days = { $days }ر { $hours }س

error-page-failed-load = بارگذاری صفحه ناموفق بود
error-page-not-found = صفحه پیدا نشد
error-unknown-host = میزبان برنامهٔ Vmux ناشناخته است: { $host }

history-title = تاریخچه

command-new-app-chat = گفت‌وگوی جدید { $provider }/{ $model } (برنامه)
command-interactive-mode-user = Scene > حالت تعاملی > کاربر
command-interactive-mode-player = Scene > حالت تعاملی > پخش‌کننده
command-minimize-window = Layout > پنجره > کوچک‌سازی
command-toggle-layout = Layout > چیدمان > تغییر چیدمان
command-close-tab = Layout > زبانه > بستن زبانه
command-new-task = Layout > زبانه > کار جدید…
command-next-tab = Layout > زبانه > زبانهٔ بعدی
command-prev-tab = Layout > زبانه > زبانهٔ قبلی
command-rename-tab = Layout > زبانه > تغییر نام زبانه
command-tab-select-1 = Layout > زبانه > انتخاب زبانهٔ ۱
command-tab-select-2 = Layout > زبانه > انتخاب زبانهٔ ۲
command-tab-select-3 = Layout > زبانه > انتخاب زبانهٔ ۳
command-tab-select-4 = Layout > زبانه > انتخاب زبانهٔ ۴
command-tab-select-5 = Layout > زبانه > انتخاب زبانهٔ ۵
command-tab-select-6 = Layout > زبانه > انتخاب زبانهٔ ۶
command-tab-select-7 = Layout > زبانه > انتخاب زبانهٔ ۷
command-tab-select-8 = Layout > زبانه > انتخاب زبانهٔ ۸
command-tab-select-last = Layout > زبانه > انتخاب آخرین زبانه
command-close-pane = Layout > Pane > بستن Pane
command-select-pane-left = Layout > Pane > انتخاب Pane چپ
command-select-pane-right = Layout > Pane > انتخاب Pane راست
command-select-pane-up = Layout > Pane > انتخاب Pane بالا
command-select-pane-down = Layout > Pane > انتخاب Pane پایین
command-swap-pane-prev = Layout > Pane > جابه‌جایی با Pane قبلی
command-swap-pane-next = Layout > Pane > جابه‌جایی با Pane بعدی
command-equalize-pane-size = Layout > Pane > هم‌اندازه کردن Paneها
command-resize-pane-left = Layout > Pane > تغییر اندازه به چپ
command-resize-pane-right = Layout > Pane > تغییر اندازه به راست
command-resize-pane-up = Layout > Pane > تغییر اندازه به بالا
command-resize-pane-down = Layout > Pane > تغییر اندازه به پایین
command-stack-close = Layout > Stack > بستن Stack
command-stack-next = Layout > Stack > Stack بعدی
command-stack-previous = Layout > Stack > Stack قبلی
command-stack-reopen = Layout > Stack > بازگشایی صفحهٔ بسته‌شده
command-stack-swap-prev = Layout > Stack > انتقال Stack به چپ
command-stack-swap-next = Layout > Stack > انتقال Stack به راست
command-space-open = Layout > Space > Spaceها
command-terminal-close = Terminal > بستن Terminal
command-terminal-next = Terminal > Terminal بعدی
command-terminal-prev = Terminal > Terminal قبلی
command-terminal-clear = Terminal > پاک کردن Terminal
command-browser-prev-page = Browser > پیمایش > برگشت
command-browser-next-page = Browser > پیمایش > جلو
command-browser-reload = Browser > پیمایش > بارگذاری مجدد
command-browser-hard-reload = Browser > پیمایش > بارگذاری مجدد کامل
command-open-in-place = Browser > باز کردن > باز کردن همین‌جا
command-open-in-new-stack = Browser > باز کردن > باز کردن در Stack جدید
command-open-in-pane-top = Browser > باز کردن > باز کردن در Pane بالا
command-open-in-pane-right = Browser > باز کردن > باز کردن در Pane راست
command-open-in-pane-bottom = Browser > باز کردن > باز کردن در Pane پایین
command-open-in-pane-left = Browser > باز کردن > باز کردن در Pane چپ
command-open-in-new-tab = Browser > باز کردن > باز کردن در زبانهٔ جدید
command-open-in-new-space = Browser > باز کردن > باز کردن در Space جدید
command-browser-zoom-in = Browser > نما > بزرگ‌نمایی
command-browser-zoom-out = Browser > نما > کوچک‌نمایی
command-browser-zoom-reset = Browser > نما > اندازهٔ واقعی
command-browser-dev-tools = Browser > نما > ابزارهای توسعه‌دهنده
command-browser-open-command-bar = Browser > نوار > نوار فرمان
command-browser-open-page-in-command-bar = Browser > نوار > ویرایش صفحه
command-browser-open-path-bar = Browser > نوار > پیمایشگر مسیر
command-browser-open-commands = Browser > نوار > فرمان‌ها
command-browser-open-history = Browser > نوار > تاریخچه
command-service-open = Service > باز کردن پایشگر سرویس
command-bookmark-toggle-active = Bookmark > نشانک‌گذاری صفحه
command-bookmark-pin-active = Bookmark > سنجاق کردن صفحه

layout-tab = زبانه
layout-no-stacks = Stackی وجود ندارد
layout-loading = در حال بارگذاری…
layout-no-markdown-files = فایل Markdownی وجود ندارد
layout-empty-folder = پوشه خالی است
layout-worktree = worktree
layout-folder-name = نام پوشه
layout-no-pins-bookmarks = سنجاق یا نشانکی وجود ندارد
layout-move-to = انتقال به { $folder }
layout-bookmark-current-page = نشانک‌گذاری صفحهٔ فعلی
layout-rename-folder = تغییر نام پوشه
layout-remove-folder = حذف پوشه
layout-update-downloading = در حال دانلود به‌روزرسانی
layout-update-installing = در حال نصب به‌روزرسانی…
layout-update-ready = نسخهٔ جدید آماده است
layout-restart-update = برای به‌روزرسانی راه‌اندازی مجدد کنید

agent-preparing = در حال آماده‌سازی عامل…
agent-send-all-queued = ارسال همهٔ درخواست‌های صف‌شده اکنون (Esc)
agent-send = ارسال (Enter)
agent-ready = هر وقت آماده‌اید.
agent-loading-older = در حال بارگذاری پیام‌های قدیمی‌تر…
agent-load-older = بارگذاری پیام‌های قدیمی‌تر
agent-continued-from = ادامه‌یافته از { $source }
agent-older-context-omitted = زمینهٔ قدیمی‌تر حذف شده است
agent-interrupted = متوقف شد
agent-allow-tool = به { $tool } اجازه داده شود؟
agent-deny = رد
agent-allow-always = همیشه اجازه بده
agent-allow = اجازه بده
agent-loading-sessions = در حال بارگذاری نشست‌ها…
agent-no-resumable-sessions = نشست قابل ادامه‌ای پیدا نشد
agent-no-matching-sessions = نشست منطبقی وجود ندارد
agent-no-matching-models = مدل منطبقی وجود ندارد
agent-choice-help = ↑/↓ یا Ctrl+N/Ctrl+P · ۱–۹ · Enter
agent-choose-repository = انتخاب پوشهٔ مخزن
agent-choose-repository-detail = مخزن Git محلی‌ای را که عامل باید استفاده کند انتخاب کنید.
agent-choosing = در حال انتخاب…
agent-choose-folder = انتخاب پوشه
agent-queued = در صف
agent-attached = پیوست‌شده:
agent-cancel-queued = لغو درخواست صف‌شده
agent-resume-queued = ادامهٔ درخواست‌های صف‌شده
agent-clear-queue = پاک کردن صف
agent-send-all-now = ارسال همه اکنون
agent-choose-option = یکی از گزینه‌های بالا را انتخاب کنید
agent-loading-media = در حال بارگذاری رسانه…
agent-no-matching-media = رسانهٔ منطبقی وجود ندارد
agent-prompt-context = زمینهٔ درخواست
agent-details = جزئیات
agent-path = مسیر
agent-tool = ابزار
agent-server = سرور
agent-bytes = { $count } بایت
agent-worked-for = به‌مدت { $duration } کار کرد
agent-worked-for-steps = { $count ->
    [one] به‌مدت { $duration } کار کرد · ۱ گام
   *[other] به‌مدت { $duration } کار کرد · { $count } گام
}
agent-tool-guardian-review = بازبینی نگهبان
agent-tool-read-files = فایل‌ها را خواند
agent-tool-viewed-image = تصویر را دید
agent-tool-used-browser = از مرورگر استفاده کرد
agent-tool-searched-files = فایل‌ها را جست‌وجو کرد
agent-tool-ran-commands = فرمان‌ها را اجرا کرد
agent-thinking = در حال فکر کردن
agent-subagent = زیرعامل
agent-prompt = درخواست
agent-thread = رشته
agent-parent = والد
agent-children = فرزندان
agent-call = فراخوانی
agent-raw-event = رویداد خام
agent-plan = برنامه
agent-tasks = { $count ->
    [one] ۱ کار
   *[other] { $count } کار
}
agent-edited = ویرایش شد
agent-reconnecting = در حال اتصال دوباره { $attempt }/{ $total }
agent-status-running = در حال اجرا
agent-status-done = انجام شد
agent-status-failed = ناموفق
agent-status-pending = در انتظار
agent-slash-attach-files = پیوست کردن فایل‌ها
agent-slash-resume-session = ادامهٔ نشست قبلی
agent-slash-select-model = انتخاب مدل
agent-slash-continue-cli = ادامهٔ این نشست در CLI
agent-session-just-now = همین حالا
agent-session-minutes-ago = { $count }د پیش
agent-session-hours-ago = { $count }س پیش
agent-session-days-ago = { $count }ر پیش
agent-working-working = در حال کار
agent-working-thinking = در حال فکر کردن
agent-working-pondering = در حال تأمل
agent-working-noodling = در حال ور رفتن
agent-working-percolating = در حال جا افتادن
agent-working-conjuring = در حال احضار
agent-working-cooking = در حال پخت‌وپز
agent-working-brewing = در حال دم کردن
agent-working-musing = در حال خیال‌پردازی
agent-working-ruminating = در حال سبک‌سنگین کردن
agent-working-scheming = در حال نقشه کشیدن
agent-working-synthesizing = در حال ترکیب
agent-working-tinkering = در حال دست‌کاری
agent-working-churning = در حال پردازش
agent-working-vibing = در حال گرفتن حال‌وهوا
agent-working-simmering = در حال قل زدن
agent-working-crafting = در حال ساختن
agent-working-divining = در حال کشف
agent-working-mulling = در حال بررسی
agent-working-spelunking = در حال کاوش

editor-toggle-explorer = نمایش/پنهان کردن Explorer (Cmd+B)
editor-unsaved = ذخیره‌نشده
editor-rendered-markdown = Markdown رندرشده با ویرایش زنده
editor-note = یادداشت
editor-source-editor = ویرایشگر کد
editor-editor = ویرایشگر
editor-git-diff = تفاوت Git
editor-diff = تفاوت
editor-tidy = مرتب‌سازی
editor-always = همیشه
editor-unchanged-previews = { $count ->
    [one] ✦ ۱ پیش‌نمایش بدون تغییر
   *[other] ✦ { $count } پیش‌نمایش بدون تغییر
}
editor-open-externally = باز کردن در برنامهٔ بیرونی
editor-changed-line = خط تغییرکرده
editor-go-to-definition = رفتن به تعریف
editor-find-references = یافتن ارجاع‌ها
editor-references = { $count ->
    [one] ۱ ارجاع
   *[other] { $count } ارجاع
}
editor-lsp-starting = { $server } در حال راه‌اندازی…
editor-lsp-not-installed = { $server } — نصب نشده
editor-explorer = Explorer
editor-open-editors = ویرایشگرهای باز
editor-outline = نمای کلی
editor-new-file = فایل جدید
editor-new-folder = پوشهٔ جدید
editor-delete-confirm = «{ $name }» حذف شود؟ این کار قابل بازگشت نیست.
editor-created-folder = پوشهٔ { $name } ساخته شد
editor-created-file = فایل { $name } ساخته شد
editor-renamed-to = به { $name } تغییر نام یافت
editor-deleted = { $name } حذف شد
editor-failed-decode-image = رمزگشایی تصویر ناموفق بود
editor-preview-large-image = تصویر (برای پیش‌نمایش خیلی بزرگ است)
editor-preview-binary = دودویی
editor-preview-file = فایل

git-status-clean = پاک
git-status-modified = تغییرکرده
git-status-staged = آمادهٔ کامیت
git-status-staged-modified = آمادهٔ کامیت*
git-status-untracked = ردیابی‌نشده
git-status-deleted = حذف‌شده
git-status-conflict = تعارض
git-accept-all = ✓ پذیرش همه
git-unstage = خارج کردن از آمادهٔ کامیت
git-confirm-deny-all = تأیید رد همه
git-deny-all = ✗ رد همه
git-commit-message = پیام کامیت
git-commit = کامیت ({ $count })
git-push = ↑ Push
git-loading-diff = در حال بارگذاری تفاوت…
git-no-changes = تغییری برای نمایش نیست
git-accept = ✓ پذیرش
git-deny = ✗ رد
git-show-unchanged-lines = نمایش { $count } خط بدون تغییر

terminal-loading = در حال بارگذاری…
terminal-runs-when-ready = پس از آماده شدن اجرا می‌شود · Ctrl+C پاک می‌کند · Esc رد می‌کند
terminal-booting = در حال راه‌اندازی
terminal-type-command = فرمانی وارد کنید · پس از آماده شدن اجرا می‌شود · Esc رد می‌کند

setup-tagline-claude = عامل کدنویسی Anthropic، در Vmux
setup-tagline-codex = عامل کدنویسی OpenAI، در Vmux
setup-tagline-vibe = عامل کدنویسی Mistral، در Vmux
setup-install-title = نصب CLI { $name }
setup-homebrew-required = برای نصب { $command } به Homebrew نیاز است و هنوز راه‌اندازی نشده. Vmux ابتدا Homebrew و سپس { $name } را نصب می‌کند.
setup-terminal-instructions = در ترمینال، برای شروع Return را بزنید و وقتی خواسته شد رمز Mac خود را وارد کنید.
setup-command-missing = Vmux این صفحه را باز کرد چون فرمان محلی { $command } هنوز نصب نشده است. برای دریافت آن فرمان زیر را اجرا کنید.
setup-install-failed = نصب کامل نشد. جزئیات را در ترمینال بررسی کنید و دوباره تلاش کنید.
setup-installing = در حال نصب…
setup-install-homebrew = نصب Homebrew + { $name }
setup-run-install = اجرای فرمان نصب
setup-auto-reload = Vmux آن را در ترمینال اجرا می‌کند و وقتی { $command } آماده شد دوباره بارگذاری می‌شود.

debug-title = اشکال‌زدایی
debug-auto-update = به‌روزرسانی خودکار
debug-simulate-update = شبیه‌سازی وجود به‌روزرسانی
debug-simulate-download = شبیه‌سازی دانلود
debug-clear-update = پاک کردن به‌روزرسانی
debug-trigger-restart = راه‌اندازی مجدد را فعال کن

command-manage-spaces = مدیریت فضاها…
command-pane-stack-location = پنل { $pane } / پشته { $stack }
command-space-pane-stack-location = { $space } / پنل { $pane } / پشته { $stack }
command-terminal-path = ترمینال ({ $path })
command-group-interactive-mode = حالت تعاملی
command-group-window = پنجره
command-group-tab = زبانه
command-group-pane = پنل
command-group-stack = پشته
command-group-space = فضا
command-group-navigation = پیمایش
command-group-open = باز کردن
command-group-view = نما
command-group-bar = نوار

menu-close-vmux = بستن Vmux

agents-terminal-coding-agent = عامل کدنویسی مبتنی بر ترمینال
