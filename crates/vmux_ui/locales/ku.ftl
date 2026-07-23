locale-name = kurdî (kurmancî)
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

tools-title = ئامرازەکان
tools-search = گەڕان بۆ پاکێجەکان، بریکارەکان، MCP، ئامرازەکانی زمان و فایلەکانی ڕێکخستن…
tools-open = کردنەوەی ئامرازەکان
tools-fold = کۆکردنەوەی ئامرازەکان
tools-unfold = بڵاوکردنەوەی ئامرازەکان
tools-scanning = پشکنینی ئامرازە ناوخۆییەکان…
tools-no-installed = هیچ ئامرازێک دامەزراو نییە
tools-empty = هیچ ئامرازێکی گونجاو نییە
tools-empty-detail = پاکێجێک دابمەزرێنە یان پاکێجێکی فایلەکانی ڕێکخستن بە شێوازی Stow زیاد بکە.
tools-apply = جێبەجێکردن
tools-homebrew = Homebrew
tools-homebrew-sync = فۆرمولاکان و بەرنامە دامەزراوەکان خۆکارانە هاوکات دەکرێن.
tools-open-brewfile = کردنەوەی Brewfile
tools-managed = بەڕێوەبراو
tools-provider-homebrew-formulae = فۆرمولاکانی Homebrew
tools-provider-homebrew-casks = بەرنامەکانی Homebrew
tools-provider-npm = پاکێجەکانی npm
tools-provider-acp-agents = بریکارەکانی ACP
tools-provider-language-tools = ئامرازەکانی زمان
tools-provider-mcp-servers = ڕاژەکارەکانی MCP
tools-provider-dotfiles = فایلەکانی ڕێکخستن
tools-status-available = بەردەستە
tools-status-missing = ونە
tools-status-conflict = ناکۆکی
tools-forget = لەبیرکردن
tools-manage = بەڕێوەبردن
tools-link = بەستنەوە
tools-unlink = پچڕاندنی بەستەر
tools-import = هاوردەکردن
tools-update-count = { $count ->
    [one] 1 نوێکردنەوە
   *[other] { $count } نوێکردنەوە
}
tools-conflict-count = { $count ->
    [one] 1 ناکۆکی
   *[other] { $count } ناکۆکی
}
tools-result-applied = ئامرازەکان جێبەجێ کران
tools-result-imported = ئامرازەکان هاوردە کران
tools-result-installed = { $name } دامەزرا
tools-result-updated = { $name } نوێ کرایەوە
tools-result-uninstalled = { $name } لابرا
tools-result-forgotten = { $name } لەبیر کرا
tools-result-managed = { $name } ئێستا بەڕێوە دەبرێت
tools-result-linked = { $name } بەسترایەوە
tools-result-unlinked = بەستەری { $name } پچڕێنرا

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

settings-empty = (vala)
settings-none = (tune)

schema-system = Sîstem
schema-editor = Sererastker
schema-recording = Tomarkirin
schema-radius = Tîrêj
schema-padding = Navberê hundir
schema-gap = Navber
schema-width = Firehî
schema-color = Reng
schema-red = Sor
schema-green = Kesk
schema-blue = Şîn
schema-follow-files = Pelan bişopîne
schema-tidy-files = Pelan rêkûpêk bike
schema-tidy-files-max = Astana rêkûpêkirina pelan
schema-tidy-files-auto = Pelan bixweber rêkûpêk bike
schema-app-providers = Pêşkêşkerên sepanan
schema-provider = Pêşkêşker
schema-kind = Cure
schema-models = Model
schema-acp = Agentên ACP
schema-id = ID
schema-name = Nav
schema-command = Ferman
schema-arguments = Argûman
schema-environment = Jîngeh
schema-working-directory = Peldanka xebatê
schema-shell = Qalik
schema-font-family = Malbata fontê
schema-startup-directory = Peldanka destpêkê
schema-themes = Tema
schema-color-scheme = Plana rengan
schema-font-size = Mezinahiya fontê
schema-line-height = Bilindiya rêzikê
schema-cursor-style = Şêwaza nîşankerê
schema-cursor-blink = Çirpîna nîşankerê
schema-custom-themes = Temayên taybet
schema-foreground = Pêşreng
schema-background = Paşreng
schema-cursor = Nîşanker
schema-ansi-colors = Rengên ANSI
schema-keymap = Nexşeya kilîdan
schema-explorer = Gerok
schema-visible = Xuya
schema-language-servers = Serverên ziman
schema-servers = Server
schema-language-id = ID ya ziman
schema-root-markers = Nîşankerên rehê
schema-output-directory = Peldanka deranê

menu-scene = Dîmen
menu-layout = Rêzkirin
menu-terminal = Termînal
menu-browser = Gerok
menu-service = Xizmet
menu-bookmark = Nîşank
menu-edit = Sererastkirin

layout-knowledge = Zanîn
layout-open-knowledge = Zanînê veke
layout-open-welcome-knowledge = Bi xêr hatî Zanînê veke
layout-open-path = { $path } veke
layout-fold-knowledge = Zanînê bitewîne
layout-unfold-knowledge = Zanînê vekin
layout-bookmarks = Nîşank
layout-new-folder = Peldanka nû
layout-add-to-bookmarks = Li nîşankan zêde bike
layout-move-to-bookmarks = Veguhezîne nîşankan
layout-stack-number = Stak { $number }
layout-fold-stack = Stakê bitewîne
layout-unfold-stack = Stakê vekin
layout-close-stack = Stakê bigire
layout-bookmark-in = Di { $folder } de nîşan bike

common-cancel = پاشگەزبوونەوە
common-delete = سڕینەوە
common-save = پاشەکەوت
common-rename = ناوگۆڕین
common-expand = فراوانکردن
common-collapse = داخستنەوە
common-loading = بارکردن…
common-error = هەڵە
common-output = دەرچوو
common-pending = چاوەڕوان
common-current = ئێستا
common-stop = وەستاندن
services-command = خزمەتگوزاری Vmux
services-uptime-seconds = { $seconds }چرکە
services-uptime-minutes = { $minutes }خ { $seconds }چرکە
services-uptime-hours = { $hours }ک { $minutes }خ
services-uptime-days = { $days }ڕ { $hours }ک

error-page-failed-load = بارکردنی پەڕەکە سەرکەوتوو نەبوو
error-page-not-found = پەڕە نەدۆزرایەوە
error-unknown-host = خانەخوێی نەناسراوی ئەپی Vmux: { $host }

history-title = مێژوو

command-new-app-chat = چاتی نوێی { $provider }/{ $model } (ئەپ)
command-interactive-mode-user = دیمەن > دۆخی کارلێکی > بەکارهێنەر
command-interactive-mode-player = دیمەن > دۆخی کارلێکی > یاریزان
command-minimize-window = ڕێکخستن > پەنجەرە > بچووککردنەوە
command-toggle-layout = ڕێکخستن > ڕێکخستن > گۆڕینی ڕێکخستن
command-close-tab = ڕێکخستن > تاب > داخستنی تاب
command-new-task = ڕێکخستن > تاب > ئەرکی نوێ…
command-next-tab = ڕێکخستن > تاب > تابی دواتر
command-prev-tab = ڕێکخستن > تاب > تابی پێشوو
command-rename-tab = ڕێکخستن > تاب > ناوگۆڕینی تاب
command-tab-select-1 = ڕێکخستن > تاب > هەڵبژاردنی تابی ١
command-tab-select-2 = ڕێکخستن > تاب > هەڵبژاردنی تابی ٢
command-tab-select-3 = ڕێکخستن > تاب > هەڵبژاردنی تابی ٣
command-tab-select-4 = ڕێکخستن > تاب > هەڵبژاردنی تابی ٤
command-tab-select-5 = ڕێکخستن > تاب > هەڵبژاردنی تابی ٥
command-tab-select-6 = ڕێکخستن > تاب > هەڵبژاردنی تابی ٦
command-tab-select-7 = ڕێکخستن > تاب > هەڵبژاردنی تابی ٧
command-tab-select-8 = ڕێکخستن > تاب > هەڵبژاردنی تابی ٨
command-tab-select-last = ڕێکخستن > تاب > هەڵبژاردنی تابی کۆتایی
command-close-pane = ڕێکخستن > پانێڵ > داخستنی پانێڵ
command-select-pane-left = ڕێکخستن > پانێڵ > هەڵبژاردنی پانێڵی چەپ
command-select-pane-right = ڕێکخستن > پانێڵ > هەڵبژاردنی پانێڵی ڕاست
command-select-pane-up = ڕێکخستن > پانێڵ > هەڵبژاردنی پانێڵی سەرەوە
command-select-pane-down = ڕێکخستن > پانێڵ > هەڵبژاردنی پانێڵی خوارەوە
command-swap-pane-prev = ڕێکخستن > پانێڵ > گۆڕینەوەی پانێڵ لەگەڵ پێشوو
command-swap-pane-next = ڕێکخستن > پانێڵ > گۆڕینەوەی پانێڵ لەگەڵ دواتر
command-equalize-pane-size = ڕێکخستن > پانێڵ > یەکسانکردنی قەبارەی پانێڵەکان
command-resize-pane-left = ڕێکخستن > پانێڵ > گۆڕینی قەبارەی پانێڵ بۆ چەپ
command-resize-pane-right = ڕێکخستن > پانێڵ > گۆڕینی قەبارەی پانێڵ بۆ ڕاست
command-resize-pane-up = ڕێکخستن > پانێڵ > گۆڕینی قەبارەی پانێڵ بۆ سەرەوە
command-resize-pane-down = ڕێکخستن > پانێڵ > گۆڕینی قەبارەی پانێڵ بۆ خوارەوە
command-stack-close = ڕێکخستن > ستاک > داخستنی ستاک
command-stack-next = ڕێکخستن > ستاک > ستاکی دواتر
command-stack-previous = ڕێکخستن > ستاک > ستاکی پێشوو
command-stack-reopen = ڕێکخستن > ستاک > کردنەوەی پەڕەی داخراو
command-stack-swap-prev = ڕێکخستن > ستاک > گواستنەوەی ستاک بۆ چەپ
command-stack-swap-next = ڕێکخستن > ستاک > گواستنەوەی ستاک بۆ ڕاست
command-space-open = ڕێکخستن > بۆشایی > بۆشاییەکان
command-terminal-close = تێرمیناڵ > داخستنی تێرمیناڵ
command-terminal-next = تێرمیناڵ > تێرمیناڵی دواتر
command-terminal-prev = تێرمیناڵ > تێرمیناڵی پێشوو
command-terminal-clear = تێرمیناڵ > پاککردنەوەی تێرمیناڵ
command-browser-prev-page = وێبگەڕ > گەڕان > دواوە
command-browser-next-page = وێبگەڕ > گەڕان > پێشەوە
command-browser-reload = وێبگەڕ > گەڕان > نوێکردنەوە
command-browser-hard-reload = وێبگەڕ > گەڕان > نوێکردنەوەی تەواو
command-open-in-place = وێبگەڕ > کردنەوە > لێرە بیکەرەوە
command-open-in-new-stack = وێبگەڕ > کردنەوە > لە ستاکی نوێ بیکەرەوە
command-open-in-pane-top = وێبگەڕ > کردنەوە > لە پانێڵی سەرەوە بیکەرەوە
command-open-in-pane-right = وێبگەڕ > کردنەوە > لە پانێڵی ڕاست بیکەرەوە
command-open-in-pane-bottom = وێبگەڕ > کردنەوە > لە پانێڵی خوارەوە بیکەرەوە
command-open-in-pane-left = وێبگەڕ > کردنەوە > لە پانێڵی چەپ بیکەرەوە
command-open-in-new-tab = وێبگەڕ > کردنەوە > لە تابی نوێ بیکەرەوە
command-open-in-new-space = وێبگەڕ > کردنەوە > لە بۆشایی نوێ بیکەرەوە
command-browser-zoom-in = وێبگەڕ > بینین > نزیککردنەوە
command-browser-zoom-out = وێبگەڕ > بینین > دوورکردنەوە
command-browser-zoom-reset = وێبگەڕ > بینین > قەبارەی ڕاستەقینە
command-browser-dev-tools = وێبگەڕ > بینین > ئامرازەکانی گەشەپێدەر
command-browser-open-command-bar = وێبگەڕ > شریت > شریتی فرمان
command-browser-open-page-in-command-bar = وێبگەڕ > شریت > دەستکاریکردنی پەڕە
command-browser-open-path-bar = وێبگەڕ > شریت > ڕێدۆزی ڕێڕەو
command-browser-open-commands = وێبگەڕ > شریت > فرمانەکان
command-browser-open-history = وێبگەڕ > شریت > مێژوو
command-service-open = خزمەتگوزاری > کردنەوەی چاودێری خزمەتگوزاری
command-bookmark-toggle-active = نیشانە > نیشانەکردنی پەڕە
command-bookmark-pin-active = نیشانە > جێگیرکردنی پەڕە

layout-tab = تاب
layout-no-stacks = هیچ ستاکێک نییە
layout-loading = بارکردن…
layout-no-markdown-files = هیچ پەڕگەی Markdown نییە
layout-empty-folder = بوخچەی بەتاڵ
layout-worktree = worktree
layout-folder-name = ناوی بوخچە
layout-no-pins-bookmarks = هیچ جێگیرکراو یان نیشانەیەک نییە
layout-move-to = گواستنەوە بۆ { $folder }
layout-bookmark-current-page = نیشانەکردنی پەڕەی ئێستا
layout-rename-folder = ناوگۆڕینی بوخچە
layout-remove-folder = لابردنی بوخچە
layout-update-downloading = داگرتنی نوێکاری
layout-update-installing = دامەزراندنی نوێکاری…
layout-update-ready = وەشانی نوێ بەردەستە
layout-restart-update = دووبارەکردنەوە بۆ نوێکردنەوە

agent-preparing = ئیجێنت ئامادە دەکرێت…
agent-send-all-queued = ناردنی هەموو داواکارییە چاوەڕوانەکان ئێستا (Esc)
agent-send = ناردن (Enter)
agent-ready = کاتێک ئامادەیت.
agent-loading-older = بارکردنی پەیامە کۆنترەکان…
agent-load-older = بارکردنی پەیامە کۆنترەکان
agent-continued-from = بەردەوامە لە { $source }
agent-older-context-omitted = کۆنتێکستی کۆنتر لابراوە
agent-interrupted = پچڕاوە
agent-allow-tool = ڕێگە بە { $tool } بدەیت؟
agent-deny = ڕەتکردنەوە
agent-allow-always = هەمیشە ڕێگەبدە
agent-allow = ڕێگەدان
agent-loading-sessions = بارکردنی دانیشتنەکان…
agent-no-resumable-sessions = هیچ دانیشتنێکی بەردەوامبوون نەدۆزرایەوە
agent-no-matching-sessions = هیچ دانیشتنێکی هاوتا نییە
agent-no-matching-models = هیچ مۆدێلێکی هاوتا نییە
agent-choice-help = ↑/↓ یان Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = بوخچەی کۆگای کۆد هەڵبژێرە
agent-choose-repository-detail = کۆگای Git ناوخۆیی هەڵبژێرە کە ئیجێنت بەکاری بهێنێت.
agent-choosing = هەڵبژاردن…
agent-choose-folder = بوخچە هەڵبژێرە
agent-queued = لە ڕیزدایە
agent-attached = هاوپێچکراو:
agent-cancel-queued = هەڵوەشاندنەوەی داواکاریی ڕیزکراو
agent-resume-queued = بەردەوامکردنی داواکارییە ڕیزکراوەکان
agent-clear-queue = پاککردنەوەی ڕیز
agent-send-all-now = هەمووی ئێستا بنێرە
agent-choose-option = هەڵبژاردەیەک لە سەرەوە هەڵبژێرە
agent-loading-media = بارکردنی میدیا…
agent-no-matching-media = هیچ میدیایەکی هاوتا نییە
agent-prompt-context = کۆنتێکستی داواکاری
agent-details = وردەکاری
agent-path = ڕێڕەو
agent-tool = ئامراز
agent-server = سێرڤەر
agent-bytes = { $count } بایت
agent-worked-for = کاری کرد بۆ { $duration }
agent-worked-for-steps = { $count ->
    [one] کاری کرد بۆ { $duration } · ١ هەنگاو
   *[other] کاری کرد بۆ { $duration } · { $count } هەنگاو
}
agent-tool-guardian-review = پێداچوونەوەی پارێزەر
agent-tool-read-files = پەڕگەکانی خوێندەوە
agent-tool-viewed-image = وێنەی بینی
agent-tool-used-browser = وێبگەڕی بەکارهێنا
agent-tool-searched-files = لە پەڕگەکان گەڕا
agent-tool-ran-commands = فرمانی جێبەجێکرد
agent-thinking = بیرکردنەوە
agent-subagent = ژێر-ئیجێنت
agent-prompt = داواکاری
agent-thread = زنجیرە
agent-parent = دایک
agent-children = لقەکان
agent-call = بانگکردن
agent-raw-event = ڕووداوی خاو
agent-plan = پلان
agent-tasks = { $count ->
    [one] ١ ئەرک
   *[other] { $count } ئەرک
}
agent-edited = دەستکاریکرا
agent-reconnecting = پەیوەندییەوە { $attempt }/{ $total }
agent-status-running = کاردەکات
agent-status-done = تەواو
agent-status-failed = سەرکەوتوو نەبوو
agent-status-pending = چاوەڕوان
agent-slash-attach-files = هاوپێچکردنی پەڕگەکان
agent-slash-resume-session = بەردەوامکردنی دانیشتنێکی پێشوو
agent-slash-select-model = هەڵبژاردنی مۆدێل
agent-slash-continue-cli = بەردەوامبوونی ئەم دانیشتنە لە CLI
agent-session-just-now = هەر ئێستا
agent-session-minutes-ago = { $count }خ پێش ئێستا
agent-session-hours-ago = { $count }ک پێش ئێستا
agent-session-days-ago = { $count }ڕ پێش ئێستا
agent-working-working = کارکردن
agent-working-thinking = بیرکردنەوە
agent-working-pondering = تێڕامان
agent-working-noodling = خەریکە دەگەڕێ
agent-working-percolating = پێدەگەیشت
agent-working-conjuring = دروستدەکات
agent-working-cooking = لێنان
agent-working-brewing = ئامادەکردن
agent-working-musing = ڕامان
agent-working-ruminating = بیرلێکردنەوە
agent-working-scheming = پلان دانان
agent-working-synthesizing = تێکەڵکردن
agent-working-tinkering = دەستکاری ورد
agent-working-churning = خەریکی کارە
agent-working-vibing = لە وێڤە
agent-working-simmering = هێواش هێواش پێدەگەیشت
agent-working-crafting = داڕشتن
agent-working-divining = هەستپێکردن
agent-working-mulling = وردبوونەوە
agent-working-spelunking = قووڵگەڕان

editor-toggle-explorer = گۆڕینی دۆخی گەڕەکەر (Cmd+B)
editor-unsaved = پاشەکەوتنەکراو
editor-rendered-markdown = Markdown پیشاندراو لەگەڵ دەستکاریی زیندوو
editor-note = تێبینی
editor-source-editor = دەستکاریکەری سەرچاوە
editor-editor = دەستکاریکەر
editor-git-diff = جیاوازی Git
editor-diff = جیاوازی
editor-tidy = ڕێککردن
editor-always = هەمیشە
editor-unchanged-previews = { $count ->
    [one] ✦ ١ پێشبینینی نەگۆڕاو
   *[other] ✦ { $count } پێشبینینی نەگۆڕاو
}
editor-open-externally = لە دەرەوە بیکەرەوە
editor-changed-line = دێڕی گۆڕاو
editor-go-to-definition = بڕۆ بۆ پێناسە
editor-find-references = دۆزینەوەی ئاماژەکان
editor-references = { $count ->
    [one] ١ ئاماژە
   *[other] { $count } ئاماژە
}
editor-lsp-starting = { $server } دەستپێدەکات…
editor-lsp-not-installed = { $server } — دانەمەزراوە
editor-explorer = گەڕەکەر
editor-open-editors = دەستکاریکەرە کراوەکان
editor-outline = پوختە
editor-new-file = پەڕگەی نوێ
editor-new-folder = بوخچەی نوێ
editor-delete-confirm = “{ $name }” بسڕدرێتەوە؟ ئەمە ناگەڕێتەوە.
editor-created-folder = بوخچەی { $name } دروستکرا
editor-created-file = پەڕگەی { $name } دروستکرا
editor-renamed-to = ناوی گۆڕدرا بۆ { $name }
editor-deleted = { $name } سڕایەوە
editor-failed-decode-image = کۆدکردنەوەی وێنە سەرکەوتوو نەبوو
editor-preview-large-image = وێنە (زۆر گەورەیە بۆ پێشبینین)
editor-preview-binary = دووانەیی
editor-preview-file = پەڕگە

git-status-clean = پاک
git-status-modified = گۆڕاو
git-status-staged = ئامادەکراو
git-status-staged-modified = ئامادەکراو*
git-status-untracked = شوێنپێنەگیراو
git-status-deleted = سڕاوە
git-status-conflict = ناکۆکی
git-accept-all = ✓ هەموو پەسەند بکە
git-unstage = لە ئامادەکراوەکان لایبەرە
git-confirm-deny-all = پشتڕاستکردنەوەی ڕەتکردنەوەی هەموو
git-deny-all = ✗ هەموو ڕەت بکەوە
git-commit-message = پەیامی commit
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = بارکردنی جیاوازی…
git-no-changes = هیچ گۆڕانێک نییە بۆ پیشاندان
git-accept = ✓ پەسەند
git-deny = ✗ ڕەتکردنەوە
git-show-unchanged-lines = پیشاندانی { $count } دێڕی نەگۆڕاو

terminal-loading = بارکردن…
terminal-runs-when-ready = کاتێک ئامادە بوو جێبەجێ دەبێت · Ctrl+C پاکی دەکاتەوە · Esc تێدەپەڕێنێت
terminal-booting = هەڵکردن
terminal-type-command = فرمانێک بنووسە · کاتێک ئامادە بوو جێبەجێ دەبێت · Esc تێدەپەڕێنێت

setup-tagline-claude = ئیجێنتی کۆدنووسی Anthropic، لە Vmux
setup-tagline-codex = ئیجێنتی کۆدنووسی OpenAI، لە Vmux
setup-tagline-vibe = ئیجێنتی کۆدنووسی Mistral، لە Vmux
setup-install-title = دامەزراندنی CLI ـی { $name }
setup-homebrew-required = بۆ دامەزراندنی { $command } پێویست بە Homebrew ـە و هێشتا ڕێکنەخراوە. Vmux سەرەتا Homebrew دادەمەزرێنێت، پاشان { $name }.
setup-terminal-instructions = لە تێرمیناڵدا، Return دابگرە بۆ دەستپێکردن، پاشان وشەی نهێنیی Mac ـەکەت بنووسە کاتێک داوا دەکرێت.
setup-command-missing = Vmux ئەم پەڕەیەی کردەوە چونکە فرمانی ناوخۆیی { $command } هێشتا دانەمەزراوە. فرمانی خوارەوە جێبەجێ بکە بۆ وەرگرتنی.
setup-install-failed = دامەزراندن تەواو نەبوو. تێرمیناڵ بۆ وردەکاری بپشکنە، پاشان دووبارە هەوڵ بدەوە.
setup-installing = دامەزراندن…
setup-install-homebrew = دامەزراندنی Homebrew + { $name }
setup-run-install = جێبەجێکردنی فرمانی دامەزراندن
setup-auto-reload = Vmux لە تێرمیناڵێکدا جێبەجێی دەکات و کاتێک { $command } ئامادە بوو دووبارە بار دەکاتەوە.

debug-title = هەڵەدۆزی
debug-auto-update = نوێکردنەوەی خودکار
debug-simulate-update = لاساییکردنەوەی بەردەستبوونی نوێکاری
debug-simulate-download = لاساییکردنەوەی داگرتن
debug-clear-update = پاککردنەوەی نوێکاری
debug-trigger-restart = دەستپێکردنی دووبارەکردنەوە

command-manage-spaces = بەڕێوەبردنی فەزاكان…
command-pane-stack-location = پەین { $pane } / ستاك { $stack }
command-space-pane-stack-location = { $space } / پەین { $pane } / ستاك { $stack }
command-terminal-path = تێرمیناڵ ({ $path })
command-group-interactive-mode = دۆخی کارلێکی
command-group-window = پەنجەرە
command-group-tab = تاب
command-group-pane = پەین
command-group-stack = ستاك
command-group-space = فەزا
command-group-navigation = گەڕان
command-group-open = کردنەوە
command-group-view = بینین
command-group-bar = تووڵ

menu-close-vmux = داخستنی Vmux

agents-terminal-coding-agent = ئەیجەنتی کۆدنوسیی بنچینە-تێرمیناڵ
