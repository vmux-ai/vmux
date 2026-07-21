common-open = مفتوح
common-close = إغلاق
common-install = تثبيت
common-uninstall = إلغاء التثبيت
common-update = تحديث
common-retry = أعد المحاولة
common-refresh = تحديث
common-remove = إزالة
common-enable = تمكين
common-disable = تعطيل
common-new = جديد
common-active = نشط
common-running = تشغيل
common-done = تم
common-failed = فشل
common-installed = تم التثبيت
common-items = { $count ->
    [one] { $count } عنصر
   *[other] { $count } العناصر
}
start-title = ابدأ
start-tagline = موجه واحد. أي شيء، تم.

agents-title = الوكلاء
agents-search = ابحث عن وكلاء ACP وCLI...
agents-empty = لا يوجد وكلاء مطابقين
agents-empty-detail = جرّب اسمًا أو وقت تشغيل أو ACP/CLI.
agents-install-failed = فشل التثبيت
agents-updating = جارٍ التحديث…
agents-retrying = جارٍ إعادة المحاولة...
agents-preparing = جارٍ التحضير…

extensions-title = ملحقات
extensions-search = تم تثبيت البحث أو Chrome Web Store...
extensions-relaunch = إعادة التشغيل للتطبيق
extensions-empty = لم يتم تثبيت أي ملحقات
extensions-no-match = لا توجد ملحقات مطابقة
extensions-empty-detail = ابحث عن Chrome Web Store أعلاه ثم اضغط على Return.
extensions-no-match-detail = جرّب اسمًا آخر أو معرف ملحق آخر.
extensions-on = على
extensions-off = إيقاف
extensions-enable-confirm = هل تريد تمكين { $name }؟
extensions-enable-permissions = تمكين { $name } والسماح بما يلي:

lsp-title = خوادم اللغة
lsp-search = بحث في خوادم اللغة، واللينترات، والمنسقات...
lsp-loading = جارٍ تحميل الكتالوج…
lsp-empty = لا توجد خوادم لغة مطابقة
lsp-empty-detail = جرب لغة أخرى، أو Linter، أو Formater.
lsp-needs = يحتاج { $tool }
lsp-status-available = متاح
lsp-status-on-path = على PATH
lsp-status-installing = جارٍ التثبيت…
lsp-status-installed = تم التثبيت
lsp-status-outdated = التحديث متاح
lsp-status-running = الجري
lsp-status-failed = فشل

spaces-title = المساحات
spaces-new-placeholder = اسم الفضاء الجديد
spaces-empty = لا مسافات
spaces-default-name = مساحة { $number }
spaces-tabs = { $count ->
    [one] 1 علامة تبويب
   *[other] { $count } علامات التبويب
}
spaces-delete = احذف المساحة

team-title = فريق
team-just-you = أنت فقط في هذا الفضاء
team-agents = { $count ->
    [one] أنت ووكيل واحد
   *[other] أنت ووكلاء { $count }
}
team-empty = لا أحد هنا بعد
team-you = أنت
team-agent = وكيل

services-title = خدمات الخلفية
services-processes = { $count ->
    [one] 1 عملية
   *[other] عمليات { $count }
}
services-kill-all = اقتل الكل
services-not-running = الخدمة ليست قيد التشغيل
services-start-with = ابدأ بـ:
services-empty = لا توجد عمليات نشطة
services-filter = عمليات التصفية…
services-no-match = لا توجد عمليات مطابقة
services-connected = متصل
services-disconnected = غير متصل
services-attached = مرفق
services-kill = اقتل
services-memory = الذاكرة
services-size = الحجم
services-shell = شل

error-title = خطأ

history-search = سجل البحث
history-clear-all = مسح الكل
history-clear-confirm = هل تريد محو السجل بالكامل؟
history-clear-warning = لا يمكن التراجع عن هذا.
history-cancel = إلغاء
history-today = اليوم
history-yesterday = أمس
history-days-ago = { $count } قبل أيام
history-day-offset = يوم -{ $count }

settings-title = الإعدادات
settings-loading = جارٍ تحميل الإعدادات…
settings-stored = مخزنة في ~/.vmux/settings.ron
settings-other = أخرى
settings-software-update = تحديث البرنامج
settings-check-updates = التحقق من وجود تحديثات
settings-check-updates-hint = يتم التحقق تلقائيًا عند التشغيل وكل ساعة يتم فيها تمكين التحديث التلقائي.
settings-update-unavailable = غير متاح
settings-update-unavailable-hint = لم يتم تضمين المحدث في هذا الإصدار.
settings-update-checking = جارٍ التحقق…
settings-update-checking-hint = جارٍ التحقق من وجود تحديثات...
settings-update-check-again = تحقق مرة أخرى
settings-update-current = Vmux محدث.
settings-update-downloading = جارٍ التنزيل…
settings-update-downloading-hint = جارٍ التنزيل Vmux { $version }...
settings-update-installing = جارٍ التثبيت…
settings-update-installing-hint = جارٍ تثبيت Vmux { $version }...
settings-update-ready = التحديث جاهز
settings-update-ready-hint = Vmux { $version } جاهز. أعد التشغيل لتطبيقه.
settings-update-try-again = حاول مرة أخرى
settings-update-failed = غير قادر على التحقق من وجود تحديثات.
settings-item = البند
settings-item-number = العنصر { $number }
settings-press-key = اضغط على مفتاح…
settings-saved = تم الحفظ
settings-record-key = انقر لتسجيل مجموعة مفاتيح جديدة

tray-open-window = افتح النافذة
tray-close-window = إغلاق النافذة
tray-pause-recording = إيقاف التسجيل مؤقتًا
tray-resume-recording = استئناف التسجيل
tray-finish-recording = إنهاء التسجيل
tray-quit = قم بإنهاء Vmux

composer-attach-files = إرفاق الملفات (/upload)
composer-remove-attachment = إزالة المرفق

layout-back = العودة
layout-forward = إلى الأمام
layout-reload = إعادة تحميل
layout-bookmark-page = قم بوضع إشارة مرجعية على هذه الصفحة
layout-remove-bookmark = إزالة الإشارة المرجعية
layout-pin-page = ثبت هذه الصفحة
layout-unpin-page = قم بإزالة تثبيت هذه الصفحة
layout-manage-extensions = إدارة الملحقات
layout-new-stack = المكدس الجديد
layout-close-tab = إغلاق علامة التبويب
layout-bookmark = إشارة مرجعية
layout-pin = دبوس
layout-new-tab = علامة تبويب جديدة
layout-team = فريق

command-switch-space = تبديل المساحة…
command-search-ask = ابحث أو اسأل...
command-new-tab-placeholder = ابحث أو اكتب URL، أو حدد الوحدة الطرفية...
command-placeholder = اكتب URL، أو علامات تبويب البحث، أو > للأوامر...
command-composer-placeholder = اكتب / للأوامر أو @ للوسائط
command-send = إرسال (Enter)
command-terminal = المحطة
command-open-terminal = فتح في المحطة الطرفية
command-stack = كومة
command-tabs = { $count ->
    [one] 1 علامة تبويب
   *[other] { $count } علامات التبويب
}
command-prompt = موجه
command-new-tab = علامة تبويب جديدة
command-search = بحث
command-open-value = افتح "{ $value }"
command-search-value = بحث عن "{ $value }"

schema-appearance = المظهر
schema-general = عام
schema-layout = التخطيط
schema-layout-detail = النافذة والأجزاء والشريط الجانبي وحلقة التركيز.
schema-agent = وكيل
schema-agent-detail = سلوك الوكيل وأذونات الأداة.
schema-shortcuts = الاختصارات
schema-shortcuts-detail = عرض للقراءة فقط. قم بتحرير settings.ron مباشرة لتغيير الارتباطات.
schema-terminal = المحطة
schema-browser = المتصفح
schema-mode = الوضع
schema-mode-detail = نظام الألوان لصفحات الويب. الجهاز يتبع نظامك.
schema-device = الجهاز
schema-light = ضوء
schema-dark = الظلام
schema-language = اللغة
schema-language-detail = استخدم النظام، en-US، ja، أو أي علامة BCP 47 مع كتالوج ~/.vmux/locales/<tag>.ftl المطابق.
schema-auto-update = التحديث التلقائي
schema-auto-update-detail = التحقق من التحديثات وتثبيتها عند الإطلاق وكل ساعة.
schema-startup-url = بدء التشغيل URL
schema-startup-url-detail = فارغ يفتح موجه شريط الأوامر.
schema-search-engine = محرك البحث
schema-search-engine-detail = يُستخدم لعمليات البحث على الويب من Start (ابدأ) وشريط الأوامر.
schema-window = نافذة
schema-pane = جزء
schema-side-sheet = ورقة جانبية
schema-focus-ring = حلقة التركيز
schema-run-placement = السماح بتجاوز موضع التشغيل
schema-run-placement-detail = اسمح للوكلاء باختيار وضع جزء التشغيل والاتجاه والارتساء.
schema-leader = زعيم
schema-leader-detail = مفتاح البادئة لاختصارات الوتر.
schema-chord-timeout = مهلة الوتر
schema-chord-timeout-detail = مللي ثانية قبل انتهاء صلاحية بادئة الوتر.
schema-bindings = الارتباطات
schema-confirm-close = تأكيد الإغلاق
schema-confirm-close-detail = قم بالمطالبة قبل إغلاق الجهاز الطرفي بعملية قيد التشغيل.
schema-default-theme = الموضوع الافتراضي
schema-default-theme-detail = اسم السمة النشطة من قائمة السمات.
