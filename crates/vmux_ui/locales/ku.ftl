common-open = کردنەوە
common-close = داخستن
common-install = دامەزراندن
common-uninstall = سڕینەوە
common-update = نوێکردنەوە
common-retry = هەوڵدانەوە
common-refresh = نوێکردنەوە
common-remove = لابردن
common-enable = چالاککردن
common-disable = ناچالاککردن
common-new = نوێ
common-active = چالاک
common-running = لەکارە
common-done = تەواو
common-failed = سەرکەوتوو نەبوو
common-installed = دامەزراوە
common-items = { $count ->
    [one] { $count } دانە
   *[other] { $count } دانە
}
start-title = دەستپێک
start-tagline = یەک پرۆمپت. هەر کارێک، تەواو.

agents-title = ئەیجەنتەکان
agents-search = گەڕان بۆ ئەیجەنتەکانی ACP و CLI…
agents-empty = هیچ ئەیجەنتێکی هاوتا نییە
agents-empty-detail = ناو، ژینگەی جێبەجێکردن، یان ACP/CLI تاقی بکەرەوە.
agents-install-failed = دامەزراندن سەرکەوتوو نەبوو
agents-updating = نوێ دەکرێتەوە…
agents-retrying = دووبارە هەوڵ دەدرێت…
agents-preparing = ئامادە دەکرێت…

extensions-title = پێوەکراوەکان
extensions-search = لە دامەزراوەکان یان Chrome Web Store بگەڕێ…
extensions-relaunch = بۆ جێبەجێکردنی گۆڕانەکە دووبارە بیکەرەوە
extensions-empty = هیچ پێوەکراوێک دامەزرێنراو نییە
extensions-no-match = هیچ پێوەکراوێکی هاوتا نییە
extensions-empty-detail = لە سەرەوە لە Chrome Web Store بگەڕێ و Return دابگرە.
extensions-no-match-detail = ناوێکی تر یان ID ـی پێوەکراوەکە تاقی بکەرەوە.
extensions-on = کارا
extensions-off = ناکارا
extensions-enable-confirm = { $name } چالاک بکرێت؟
extensions-enable-permissions = { $name } چالاک بکە و ڕێ بدە بە:

lsp-title = ڕاژەکارەکانی زمان
lsp-search = لە ڕاژەکارەکانی زمان، لێنتەر و فۆرماتکەرەکان بگەڕێ…
lsp-loading = کەتەلۆگ بار دەکرێت…
lsp-empty = هیچ ڕاژەکارێکی زمانی هاوتا نییە
lsp-empty-detail = زمان، لێنتەر یان فۆرماتکەرێکی تر تاقی بکەرەوە.
lsp-needs = پێویستی بە { $tool } هەیە
lsp-status-available = بەردەستە
lsp-status-on-path = لە PATH ـە
lsp-status-installing = دادەمەزرێت…
lsp-status-installed = دامەزراوە
lsp-status-outdated = نوێکردنەوە بەردەستە
lsp-status-running = لەکارە
lsp-status-failed = سەرکەوتوو نەبوو

spaces-title = شوێنەکان
spaces-new-placeholder = ناوی شوێنی نوێ
spaces-empty = هیچ شوێنێک نییە
spaces-default-name = شوێن { $number }
spaces-tabs = { $count ->
    [one] 1 تاب
   *[other] { $count } تاب
}
spaces-delete = سڕینەوەی شوێن

team-title = تیم
team-just-you = تەنها تۆ لەم شوێنەیت
team-agents = { $count ->
    [one] تۆ و 1 ئەیجەنت
   *[other] تۆ و { $count } ئەیجەنت
}
team-empty = هێشتا کەس لێرە نییە
team-you = تۆ
team-agent = ئەیجەنت

services-title = خزمەتگوزارییەکانی پاشبنەما
services-processes = { $count ->
    [one] 1 پرۆسە
   *[other] { $count } پرۆسە
}
services-kill-all = هەموویان بوەستێنە
services-not-running = خزمەتگوزارییەکە لەکار نییە
services-start-with = دەستپێکردن بە:
services-empty = هیچ پرۆسەیەکی چالاک نییە
services-filter = پاڵاوتنی پرۆسەکان…
services-no-match = هیچ پرۆسەیەکی هاوتا نییە
services-connected = پەیوەستە
services-disconnected = پەیوەست نییە
services-attached = پەیوەستکراوە
services-kill = وەستاندنی زۆرەملێ
services-memory = بیرگە
services-size = قەبارە
services-shell = شێڵ

error-title = هەڵە

history-search = گەڕان لە مێژوو
history-clear-all = هەمووی بسڕەوە
history-clear-confirm = هەموو مێژوو بسڕدرێتەوە؟
history-clear-warning = ئەمە ناگەڕێتەوە.
history-cancel = پاشگەزبوونەوە
history-today = ئەمڕۆ
history-yesterday = دوێنێ
history-days-ago = { $count } ڕۆژ لەمەوبەر
history-day-offset = ڕۆژ -{ $count }

settings-title = ڕێکخستنەکان
settings-loading = ڕێکخستنەکان بار دەکرێن…
settings-stored = لە ~/.vmux/settings.ron هەڵگیراوە
settings-other = هی تر
settings-software-update = نوێکردنەوەی نەرمەکاڵا
settings-check-updates = پشکنینی نوێکردنەوەکان
settings-check-updates-hint = کاتێک نوێکردنەوەی خۆکار چالاکە، لە کاتی کردنەوە و هەر کاتژمێرێک خۆکار پشکنین دەکات.
settings-update-unavailable = بەردەست نییە
settings-update-unavailable-hint = نوێکەرەوە لەم دروستکراوەدا نییە.
settings-update-checking = دەپشکنێت…
settings-update-checking-hint = نوێکردنەوەکان دەپشکنرێن…
settings-update-check-again = دووبارە بپشکنە
settings-update-current = Vmux نوێترین وەشانی هەیە.
settings-update-downloading = دادەبەزێنرێت…
settings-update-downloading-hint = Vmux { $version } دادەبەزێنرێت…
settings-update-installing = دادەمەزرێت…
settings-update-installing-hint = Vmux { $version } دادەمەزرێت…
settings-update-ready = نوێکردنەوە ئامادەیە
settings-update-ready-hint = Vmux { $version } ئامادەیە. بۆ جێبەجێکردنەکە دووبارە دەستی پێ بکەرەوە.
settings-update-try-again = دووبارە هەوڵ بدە
settings-update-failed = ناتوانرێت نوێکردنەوەکان بپشکنرێن.
settings-item = دانە
settings-item-number = دانە { $number }
settings-press-key = کلیکێک دابگرە…
settings-saved = پاشەکەوت کرا
settings-record-key = کرتە بکە بۆ تۆمارکردنی کۆمبۆیەکی نوێی کلیک

tray-open-window = پەنجەرە بکەرەوە
tray-close-window = پەنجەرە دابخە
tray-pause-recording = تۆمارکردن ڕابگرە
tray-resume-recording = تۆمارکردن بەردەوام بکە
tray-finish-recording = تۆمارکردن تەواو بکە
tray-quit = دەرچوون لە Vmux

composer-attach-files = پەڕگەکان هاوپێچ بکە (/upload)
composer-remove-attachment = هاوپێچ لاببە

layout-back = گەڕانەوە
layout-forward = پێشەوە
layout-reload = بارکردنەوە
layout-bookmark-page = ئەم پەڕەیە بکە بە دڵخواز
layout-remove-bookmark = دڵخواز لاببە
layout-pin-page = ئەم پەڕەیە جێگیر بکە
layout-unpin-page = جێگیریی پەڕەکە لاببە
layout-manage-extensions = بەڕێوەبردنی پێوەکراوەکان
layout-new-stack = ستاکی نوێ
layout-close-tab = تاب دابخە
layout-bookmark = دڵخواز
layout-pin = جێگیرکردن
layout-new-tab = تابی نوێ
layout-team = تیم

command-switch-space = گۆڕینی شوێن…
command-search-ask = بگەڕێ یان بپرسە…
command-new-tab-placeholder = بگەڕێ یان URL بنووسە، یان Terminal هەڵبژێرە…
command-placeholder = URL بنووسە، لە تابەکان بگەڕێ، یان > بۆ فرمانەکان…
command-composer-placeholder = / بۆ فرمانەکان یان @ بۆ میدیا بنووسە
command-send = ناردن (Enter)
command-terminal = تێرمیناڵ
command-open-terminal = لە تێرمیناڵ بکەرەوە
command-stack = ستاک
command-tabs = { $count ->
    [one] 1 تاب
   *[other] { $count } تاب
}
command-prompt = پرۆمپت
command-new-tab = تابی نوێ
command-search = گەڕان
command-open-value = “{ $value }” بکەرەوە
command-search-value = گەڕان بۆ “{ $value }”

schema-appearance = دەرکەوتن
schema-general = گشتی
schema-layout = ڕێکخستنی ڕووکار
schema-layout-detail = پەنجەرە، پانەکان، لاپەڕەی لا و بازنەی فۆکەس.
schema-agent = ئەیجەنت
schema-agent-detail = هەڵسوکەوتی ئەیجەنت و مۆڵەتەکانی ئامراز.
schema-shortcuts = کورتەڕێکان
schema-shortcuts-detail = تەنها بۆ خوێندنەوەیە. بۆ گۆڕینی گرێدانەکان settings.ron ڕاستەوخۆ دەستکاری بکە.
schema-terminal = تێرمیناڵ
schema-browser = وێبگەڕ
schema-mode = دۆخ
schema-mode-detail = پلانی ڕەنگ بۆ پەڕەکانی وێب. ئامێر شوێنی سیستەمەکەت دەکەوێت.
schema-device = ئامێر
schema-light = ڕووناک
schema-dark = تاریک
schema-language = زمان
schema-language-detail = سیستەم، en-US، ja، یان هەر تاگێکی BCP 47 بەکاربهێنە کە کەتەلۆگی هاوتای ~/.vmux/locales/<tag>.ftl هەبێت.
schema-auto-update = نوێکردنەوەی خۆکار
schema-auto-update-detail = لە کاتی کردنەوە و هەر کاتژمێرێک نوێکردنەوە بپشکنە و دایمەزرێنە.
schema-startup-url = URL ـی دەستپێک
schema-startup-url-detail = بەتاڵ بێت پرۆمپتی شریتی فرمان دەکاتەوە.
schema-search-engine = بزوێنەری گەڕان
schema-search-engine-detail = بۆ گەڕانی وێب لە دەستپێک و شریتی فرمان بەکاردێت.
schema-window = پەنجەرە
schema-pane = پانە
schema-side-sheet = پەڕەی لا
schema-focus-ring = بازنەی فۆکەس
schema-run-placement = ڕێگەدان بە گۆڕینی شوێنی جێبەجێکردن
schema-run-placement-detail = ڕێ بدە ئەیجەنتەکان دۆخی پانەی جێبەجێکردن، ئاڕاستە و خاڵی جێگیرکردن هەڵبژێرن.
schema-leader = پێشگر
schema-leader-detail = کلیلی پێشگر بۆ کورتەڕێی کۆرد.
schema-chord-timeout = کاتی بەسەرچوونی کۆرد
schema-chord-timeout-detail = چەند میلیچرکە پێش ئەوەی پێشگری کۆرد بەسەر بچێت.
schema-bindings = گرێدانەکان
schema-confirm-close = پشتڕاستکردنەوەی داخستن
schema-confirm-close-detail = پێش داخستنی تێرمیناڵێک کە پرۆسەیەکی لەکار هەیە پرسیار بکە.
schema-default-theme = ڕووکاری بنەڕەت
schema-default-theme-detail = ناوی ڕووکاری چالاک لە لیستی ڕووکارەکان.
