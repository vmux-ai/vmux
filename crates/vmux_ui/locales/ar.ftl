locale-name = العربية
common-open = فتح
common-close = إغلاق
common-install = تثبيت
common-uninstall = إلغاء التثبيت
common-update = تحديث
common-retry = إعادة المحاولة
common-refresh = تحديث
common-remove = إزالة
common-enable = تفعيل
common-disable = تعطيل
common-new = جديد
common-active = نشط
common-running = قيد التشغيل
common-done = تم
common-failed = فشل
common-installed = مثبّت
common-items = { $count ->
    [one] عنصر واحد
   *[other] { $count } عناصر
}
start-title = البدء
start-tagline = موجّه واحد. وأنجز أي شيء.

agents-title = الوكلاء
agents-search = ابحث في وكلاء ACP وCLI…
agents-empty = لا توجد وكلاء مطابقة
agents-empty-detail = جرّب اسمًا أو بيئة تشغيل أو ACP/CLI.
agents-install-failed = فشل التثبيت
agents-updating = جارٍ التحديث…
agents-retrying = جارٍ إعادة المحاولة…
agents-preparing = جارٍ التحضير…

extensions-title = الإضافات
extensions-search = ابحث في المثبّتة أو في Chrome Web Store…
extensions-relaunch = أعد التشغيل للتطبيق
extensions-empty = لا توجد إضافات مثبّتة
extensions-no-match = لا توجد إضافات مطابقة
extensions-empty-detail = ابحث في Chrome Web Store أعلاه واضغط رجوع.
extensions-no-match-detail = جرّب اسمًا آخر أو معرّف إضافة آخر.
extensions-on = مفعّل
extensions-off = معطّل
extensions-enable-confirm = هل تريد تفعيل { $name }؟
extensions-enable-permissions = فعّل { $name } واسمح بـ:

lsp-title = خوادم اللغات
lsp-search = ابحث عن خوادم لغات أو مدقّقات أو منسّقات…
lsp-loading = جارٍ تحميل الفهرس…
lsp-empty = لا توجد خوادم لغات مطابقة
lsp-empty-detail = جرّب لغة أو مدقّقًا أو منسّقًا آخر.
lsp-needs = يتطلب { $tool }
lsp-status-available = متاح
lsp-status-on-path = على PATH
lsp-status-installing = جارٍ التثبيت…
lsp-status-installed = مثبّت
lsp-status-outdated = يتوفر تحديث
lsp-status-running = قيد التشغيل
lsp-status-failed = فشل

spaces-title = مساحات العمل
spaces-new-placeholder = اسم مساحة العمل الجديدة
spaces-empty = لا توجد مساحات عمل
spaces-default-name = مساحة العمل { $number }
spaces-tabs = { $count ->
    [one] تبويب واحد
   *[other] { $count } تبويبات
}
spaces-delete = حذف مساحة العمل

team-title = الفريق
team-just-you = أنت وحدك في مساحة العمل هذه
team-agents = { $count ->
    [one] أنت ووكيل واحد
   *[other] أنت و{ $count } وكلاء
}
team-empty = لا أحد هنا بعد
team-you = أنت
team-agent = وكيل

services-title = خدمات الخلفية
services-processes = { $count ->
    [one] عملية واحدة
   *[other] { $count } عمليات
}
services-kill-all = إنهاء الكل قسرًا
services-not-running = الخدمة لا تعمل
services-start-with = البدء باستخدام:
services-empty = لا توجد عمليات نشطة
services-filter = تصفية العمليات…
services-no-match = لا توجد عمليات مطابقة
services-connected = متصل
services-disconnected = غير متصل
services-attached = مرفق
services-kill = إنهاء قسرًا
services-memory = الذاكرة
services-size = الحجم
services-shell = الصَدفة

error-title = خطأ

history-search = البحث في السجل
history-clear-all = مسح الكل
history-clear-confirm = هل تريد مسح السجل كله؟
history-clear-warning = لا يمكن التراجع عن هذا الإجراء.
history-cancel = إلغاء
history-today = اليوم
history-yesterday = أمس
history-days-ago = منذ { $count } أيام
history-day-offset = اليوم -{ $count }

settings-title = الإعدادات
settings-loading = جارٍ تحميل الإعدادات…
settings-stored = محفوظة في ~/.vmux/settings.ron
settings-other = أخرى
settings-software-update = تحديث البرنامج
settings-check-updates = التحقق من التحديثات
settings-check-updates-hint = يتم التحقق تلقائيًا عند التشغيل وكل ساعة عند تفعيل التحديث التلقائي.
settings-update-unavailable = غير متاح
settings-update-unavailable-hint = أداة التحديث غير مضمنة في هذا الإصدار.
settings-update-checking = جارٍ التحقق…
settings-update-checking-hint = جارٍ التحقق من التحديثات…
settings-update-check-again = التحقق مجددًا
settings-update-current = Vmux محدّث.
settings-update-downloading = جارٍ التنزيل…
settings-update-downloading-hint = جارٍ تنزيل Vmux { $version }…
settings-update-installing = جارٍ التثبيت…
settings-update-installing-hint = جارٍ تثبيت Vmux { $version }…
settings-update-ready = التحديث جاهز
settings-update-ready-hint = Vmux { $version } جاهز. أعد التشغيل لتطبيقه.
settings-update-try-again = حاول مجددًا
settings-update-failed = تعذّر التحقق من التحديثات.
settings-item = عنصر
settings-item-number = العنصر { $number }
settings-press-key = اضغط مفتاحًا…
settings-saved = تم الحفظ
settings-record-key = انقر لتسجيل تركيبة مفاتيح جديدة

tray-open-window = فتح النافذة
tray-close-window = إغلاق النافذة
tray-pause-recording = إيقاف التسجيل مؤقتًا
tray-resume-recording = استئناف التسجيل
tray-finish-recording = إنهاء التسجيل
tray-quit = إنهاء Vmux

composer-attach-files = إرفاق ملفات (/upload)
composer-remove-attachment = إزالة المرفق

layout-back = رجوع
layout-forward = إلى الأمام
layout-reload = إعادة التحميل
layout-bookmark-page = إضافة هذه الصفحة إلى المفضلة
layout-remove-bookmark = إزالة من المفضلة
layout-pin-page = تثبيت هذه الصفحة
layout-unpin-page = إلغاء تثبيت هذه الصفحة
layout-manage-extensions = إدارة الإضافات
layout-new-stack = طبقة جديدة
layout-close-tab = إغلاق التبويب
layout-bookmark = إضافة إلى المفضلة
layout-pin = تثبيت
layout-new-tab = تبويب جديد
layout-team = الفريق

command-switch-space = تبديل مساحة العمل…
command-search-ask = ابحث أو اسأل…
command-new-tab-placeholder = ابحث أو اكتب عنوان URL، أو اختر الطرفية…
command-placeholder = اكتب عنوان URL أو ابحث في التبويبات، أو > للأوامر…
command-composer-placeholder = اكتب / للأوامر أو @ للوسائط
command-send = إرسال (Enter)
command-terminal = الطرفية
command-open-terminal = فتح في الطرفية
command-stack = طبقة
command-tabs = { $count ->
    [one] تبويب واحد
   *[other] { $count } تبويبات
}
command-prompt = موجّه
command-new-tab = تبويب جديد
command-search = بحث
command-open-value = فتح “{ $value }”
command-search-value = البحث عن “{ $value }”

schema-appearance = المظهر
schema-general = عام
schema-layout = التخطيط
schema-layout-detail = النافذة، الأجزاء، الشريط الجانبي، وإطار التركيز.
schema-agent = الوكيل
schema-agent-detail = سلوك الوكيل وأذونات الأدوات.
schema-shortcuts = الاختصارات
schema-shortcuts-detail = عرض للقراءة فقط. عدّل settings.ron مباشرة لتغيير الارتباطات.
schema-terminal = الطرفية
schema-browser = المتصفح
schema-mode = الوضع
schema-mode-detail = نظام ألوان صفحات الويب. الجهاز يتبع نظامك.
schema-device = الجهاز
schema-light = فاتح
schema-dark = داكن
schema-language = اللغة
schema-language-detail = استخدم لغة النظام أو en-US أو ja أو أي وسم BCP 47 مع فهرس ~/.vmux/locales/<tag>.ftl مطابق.
schema-auto-update = التحديث التلقائي
schema-auto-update-detail = التحقق من التحديثات وتثبيتها عند التشغيل وكل ساعة.
schema-startup-url = عنوان URL عند البدء
schema-startup-url-detail = اتركه فارغًا لفتح موجّه شريط الأوامر.
schema-search-engine = محرك البحث
schema-search-engine-detail = يُستخدم لعمليات بحث الويب من شاشة البدء وشريط الأوامر.
schema-window = النافذة
schema-pane = جزء
schema-side-sheet = اللوحة الجانبية
schema-focus-ring = إطار التركيز
schema-run-placement = السماح بتجاوز موضع التشغيل
schema-run-placement-detail = السماح للوكلاء باختيار وضع جزء التشغيل واتجاهه ومرساه.
schema-leader = مفتاح بادئ
schema-leader-detail = مفتاح تمهيدي لاختصارات التتابع.
schema-chord-timeout = مهلة التتابع
schema-chord-timeout-detail = عدد الملّي ثوانٍ قبل انتهاء صلاحية بادئة التتابع.
schema-bindings = الارتباطات
schema-confirm-close = تأكيد الإغلاق
schema-confirm-close-detail = طلب التأكيد قبل إغلاق طرفية تتضمن عملية قيد التشغيل.
schema-default-theme = السمة الافتراضية
schema-default-theme-detail = اسم السمة النشطة من قائمة السمات.
