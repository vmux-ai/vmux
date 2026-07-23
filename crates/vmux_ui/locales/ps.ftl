locale-name = پښتو
common-open = پرانیزئ
common-close = وتړئ
common-install = ولګوئ
common-uninstall = لرې یې کړئ
common-update = تازه کړئ
common-retry = بیا هڅه وکړئ
common-refresh = تازه کړئ
common-remove = لرې کړئ
common-enable = فعال کړئ
common-disable = ناچالان کړئ
common-new = نوی
common-active = فعال
common-running = روان
common-done = بشپړ
common-failed = ناکام شو
common-installed = لګېدلی
common-items = { $count ->
    [one] { $count } توکی
   *[other] { $count } توکي
}

tools-title = وسایل
tools-search = د کڅوړو، استازو، MCP، ژبې وسایلو او سازونې فایلونو لټون…
tools-open = وسایل پرانیستل
tools-fold = وسایل راټولول
tools-unfold = وسایل غځول
tools-scanning = ځايي وسایل کتل کېږي…
tools-no-installed = هېڅ وسیله نه ده لګول شوې
tools-empty = هېڅ سمون خوړونکې وسیله نشته
tools-empty-detail = یوه کڅوړه ولګوئ یا د Stow په بڼه د سازونې فایلونو کڅوړه ورزیاته کړئ.
tools-apply = پلي کول
tools-homebrew = Homebrew
tools-homebrew-sync = لګول شوې فورمولې او کاریالونه په اتومات ډول همغږي کېږي.
tools-open-brewfile = Brewfile پرانیستل
tools-managed = اداره شوی
tools-provider-homebrew-formulae = د Homebrew فورمولې
tools-provider-homebrew-casks = د Homebrew کاریالونه
tools-provider-npm = د npm کڅوړې
tools-provider-acp-agents = د ACP استازي
tools-provider-language-tools = د ژبې وسایل
tools-provider-mcp-servers = د MCP سرورونه
tools-provider-dotfiles = د سازونې فایلونه
tools-status-available = شته
tools-status-missing = ورک
tools-status-conflict = ټکر
tools-forget = هېرول
tools-manage = اداره کول
tools-link = تړل
tools-unlink = تړاو لرې کول
tools-import = واردول
tools-update-count = { $count ->
    [one] ۱ اوسمهالونه
   *[other] { $count } اوسمهالونه
}
tools-conflict-count = { $count ->
    [one] ۱ ټکر
   *[other] { $count } ټکرونه
}
tools-result-applied = وسایل پلي شول
tools-result-imported = وسایل وارد شول
tools-result-installed = { $name } ولګول شو
tools-result-updated = { $name } اوسمهال شو
tools-result-uninstalled = { $name } لرې شو
tools-result-forgotten = { $name } هېر شو
tools-result-managed = { $name } اوس اداره کېږي
tools-result-linked = { $name } وتړل شو
tools-result-unlinked = د { $name } تړاو لرې شو
vault-title = Vault
vault-open = { common-open } Vault
vault-description = د ګیټ سره تنظیمات ، اوزار ، ډاټ فایلونه او پوهه همغږي کړئ.
vault-sync = همغږي
vault-create = جوړ کړئ
vault-connect = نښلول
vault-private = شخصي ذخیره
vault-public-warning = عامه ذخیره ستاسو پوهه او ترتیب افشا کوي.
vault-choose-repository = یو ذخیره غوره کړئ ...
vault-empty = خالي
vault-clean = تر دې نیټې
vault-not-connected = تړلی نه دی
vault-change-count = بدلونونه: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = پیل
start-tagline = یوه لارښوونه. هر کار، بشپړ.

agents-title = اجنټونه
agents-search = ACP او CLI اجنټونه ولټوئ…
agents-empty = برابر اجنټونه نشته
agents-empty-detail = نوم، رن‌ټایم، یا ACP/CLI وازمویئ.
agents-install-failed = لګول ناکام شول
agents-updating = تازه کېږي…
agents-retrying = بیا هڅه کېږي…
agents-preparing = چمتو کېږي…

extensions-title = غځونې
extensions-search = لګېدلې یا د Chrome Web Store غځونې ولټوئ…
extensions-relaunch = د پلي کېدو لپاره بیا یې چالان کړئ
extensions-empty = هېڅ غځونه نه ده لګېدلې
extensions-no-match = برابره غځونه نشته
extensions-empty-detail = پورته په Chrome Web Store کې ولټوئ او Return ووهئ.
extensions-no-match-detail = بل نوم یا د غځونې ID وازمویئ.
extensions-on = چالان
extensions-off = بند
extensions-enable-confirm = { $name } فعال شي؟
extensions-enable-permissions = { $name } فعال کړئ او اجازه ورکړئ:

lsp-title = د ژبو سرورونه
lsp-search = د ژبو سرورونه، لنټرونه، فارمټرونه ولټوئ…
lsp-loading = کتلاګ پورته کېږي…
lsp-empty = برابر د ژبې سرور نشته
lsp-empty-detail = بله ژبه، لنټر، یا فارمټر وازمویئ.
lsp-needs = { $tool } ته اړتیا لري
lsp-status-available = شته
lsp-status-on-path = په PATH کې
lsp-status-installing = لګېږي…
lsp-status-installed = لګېدلی
lsp-status-outdated = تازه نسخه شته
lsp-status-running = روان
lsp-status-failed = ناکام شو

spaces-title = ځایونه
spaces-new-placeholder = د نوي ځای نوم
spaces-empty = ځایونه نشته
spaces-default-name = ځای { $number }
spaces-tabs = { $count ->
    [one] 1 ټب
   *[other] { $count } ټبونه
}
spaces-delete = ځای ړنګ کړئ

team-title = ټیم
team-just-you = په دې ځای کې یوازې تاسې یاست
team-agents = { $count ->
    [one] تاسې او 1 اجنټ
   *[other] تاسې او { $count } اجنټونه
}
team-empty = تر اوسه څوک نشته
team-you = تاسې
team-agent = اجنټ

services-title = شالیدي خدمتونه
services-processes = { $count ->
    [one] 1 پروسه
   *[other] { $count } پروسې
}
services-kill-all = ټول په زور بند کړئ
services-not-running = خدمت نه چلېږي
services-start-with = په دې یې پیل کړئ:
services-empty = فعالې پروسې نشته
services-filter = پروسې چاڼ کړئ…
services-no-match = برابره پروسه نشته
services-connected = نښلول شوی
services-disconnected = پرې شوی
services-attached = نښتی
services-kill = په زور بند کړئ
services-memory = حافظه
services-size = کچه
services-shell = شېل

error-title = تېروتنه

history-search = مخینه ولټوئ
history-clear-all = ټول پاک کړئ
history-clear-confirm = ټوله مخینه پاکه شي؟
history-clear-warning = دا بېرته نه راګرځي.
history-cancel = لغوه کړئ
history-today = نن
history-yesterday = پرون
history-days-ago = { $count } ورځې مخکې
history-day-offset = ورځ -{ $count }

settings-title = امستنې
settings-loading = امستنې پورته کېږي…
settings-stored = په ~/.vmux/settings.ron کې خوندي دي
settings-other = نور
settings-software-update = د سافټویر تازه‌کول
settings-check-updates = د تازه نسخو کتل
settings-check-updates-hint = د Auto-update په فعالېدو سره د چالانېدو پر مهال او هر ساعت په اوتومات ډول ګوري.
settings-update-unavailable = نشته
settings-update-unavailable-hint = په دې جوړونه کې تازه‌کوونکی نه دی شامل.
settings-update-checking = کتل کېږي…
settings-update-checking-hint = تازه نسخې کتل کېږي…
settings-update-check-again = بیا یې وګورئ
settings-update-current = Vmux تازه دی.
settings-update-downloading = ښکته کېږي…
settings-update-downloading-hint = Vmux { $version } ښکته کېږي…
settings-update-installing = لګېږي…
settings-update-installing-hint = Vmux { $version } لګېږي…
settings-update-ready = تازه نسخه چمتو ده
settings-update-ready-hint = Vmux { $version } چمتو دی. د پلي کېدو لپاره یې بیا چالان کړئ.
settings-update-try-again = بیا هڅه وکړئ
settings-update-failed = تازه نسخې ونه کتل شوې.
settings-item = توکی
settings-item-number = توکی { $number }
settings-press-key = یوه کیلي ووهئ…
settings-saved = خوندي شو
settings-record-key = د نوي کیلي ترکیب ثبتولو لپاره کلیک وکړئ

tray-open-window = کړکۍ پرانیزئ
tray-close-window = کړکۍ وتړئ
tray-pause-recording = ثبتول وځنډوئ
tray-resume-recording = ثبتول بیا پیل کړئ
tray-finish-recording = ثبتول پای ته ورسوئ
tray-quit = Vmux پرېږدئ

composer-attach-files = فایلونه ونښلوئ (/upload)
composer-remove-attachment = نښلون لرې کړئ

layout-back = شاته
layout-forward = مخکې
layout-reload = بیا پورته کړئ
layout-bookmark-page = دا پاڼه نښه کړئ
layout-remove-bookmark = نښه لرې کړئ
layout-pin-page = دا پاڼه پین کړئ
layout-unpin-page = د دې پاڼې پین لرې کړئ
layout-manage-extensions = غځونې اداره کړئ
layout-new-stack = نوی سټک
layout-close-tab = ټب وتړئ
layout-bookmark = نښه
layout-pin = پین
layout-new-tab = نوی ټب
layout-team = ټیم

command-switch-space = ځای بدل کړئ…
command-search-ask = ولټوئ یا وپوښتئ…
command-new-tab-placeholder = ولټوئ یا URL ولیکئ، یا Terminal وټاکئ…
command-placeholder = URL ولیکئ، ټبونه ولټوئ، یا د امرونو لپاره >…
command-composer-placeholder = د امرونو لپاره / یا د مېډیا لپاره @ ولیکئ
command-send = ولېږئ (Enter)
command-terminal = Terminal
command-open-terminal = په Terminal کې پرانیزئ
command-stack = سټک
command-tabs = { $count ->
    [one] 1 ټب
   *[other] { $count } ټبونه
}
command-prompt = لارښوونه
command-new-tab = نوی ټب
command-search = لټون
command-open-value = “{ $value }” پرانیزئ
command-search-value = “{ $value }” ولټوئ

schema-appearance = بڼه
schema-general = عمومي
schema-layout = جوړښت
schema-layout-detail = کړکۍ، پینونه، اړخ‌پټه، او د تمرکز کړۍ.
schema-agent = اجنټ
schema-agent-detail = د اجنټ چلند او د وسیلو اجازې.
schema-shortcuts = لنډلارې
schema-shortcuts-detail = یوازې د لوستلو لید. د تړنو بدلولو لپاره settings.ron نېغ سم کړئ.
schema-terminal = Terminal
schema-browser = براوزر
schema-mode = حالت
schema-mode-detail = د وېب پاڼو د رنګونو طرحه. Device ستاسې د سیستم پیروي کوي.
schema-device = Device
schema-light = روښانه
schema-dark = تیاره
schema-language = ژبه
schema-language-detail = سیستم، en-US، ja، یا هر BCP 47 ټګ وکاروئ چې ورته ~/.vmux/locales/<tag>.ftl کتلاګ ولري.
schema-auto-update = اوتومات تازه‌کول
schema-auto-update-detail = د چالانېدو پر مهال او هر ساعت تازه نسخې وګورئ او ولګوئ.
schema-startup-url = د پیل URL
schema-startup-url-detail = که تش وي، د امرونو پټې لارښوونه پرانيزي.
schema-search-engine = د لټون انجن
schema-search-engine-detail = له پیل او د امرونو له پټې څخه د وېب لټونونو لپاره کارېږي.
schema-window = کړکۍ
schema-pane = پین
schema-side-sheet = اړخ پاڼه
schema-focus-ring = د تمرکز کړۍ
schema-run-placement = د چلولو ځای ټاکلو بدلون ته اجازه ورکړئ
schema-run-placement-detail = اجنټونو ته اجازه ورکړئ چې د چلولو پین حالت، لوری، او لنگر وټاکي.
schema-leader = مشر کیلي
schema-leader-detail = د chord لنډلارو مخکینۍ کیلي.
schema-chord-timeout = د chord وخت پای
schema-chord-timeout-detail = څو میلی‌ثانیې وروسته د chord مخکینی ختمېږي.
schema-bindings = تړنې
schema-confirm-close = د تړلو تایید
schema-confirm-close-detail = د روانې پروسې لرونکی Terminal تر تړلو مخکې وپوښتئ.
schema-default-theme = تلواله تھیم
schema-default-theme-detail = د تھیمونو له لېست څخه د فعال تھیم نوم.

settings-empty = (تش)
settings-none = (هېڅ)

schema-system = سیسټم
schema-editor = سموونکی
schema-recording = ثبتول
schema-radius = راډیوس
schema-padding = دنننی واټن
schema-gap = واټن
schema-width = پلنوالی
schema-color = رنګ
schema-red = سور
schema-green = شین
schema-blue = آبي
schema-follow-files = دوتنې تعقیبول
schema-tidy-files = دوتنې منظمول
schema-tidy-files-max = د دوتنو منظمولو حد
schema-tidy-files-auto = دوتنې په اتومات ډول منظمول
schema-app-providers = د اپ برابروونکي
schema-provider = برابروونکی
schema-kind = ډول
schema-models = ماډلونه
schema-acp = ACP اجنټان
schema-id = ID
schema-name = نوم
schema-command = امر
schema-arguments = ارګومانونه
schema-environment = چاپېریال
schema-working-directory = کاري پوښۍ
schema-shell = شېل
schema-font-family = د لیکبڼې کورنۍ
schema-startup-directory = د پیل پوښۍ
schema-themes = بڼې
schema-color-scheme = د رنګ سکیم
schema-font-size = د لیکبڼې کچه
schema-line-height = د کرښې لوړوالی
schema-cursor-style = د کرسر بڼه
schema-cursor-blink = د کرسر رپېدل
schema-custom-themes = ځانګړې بڼې
schema-foreground = مخکینی رنګ
schema-background = شالید
schema-cursor = کرسر
schema-ansi-colors = ANSI رنګونه
schema-keymap = د کیلي نقشه
schema-explorer = سپړونکی
schema-visible = ښکاره
schema-language-servers = د ژبو سرورونه
schema-servers = سرورونه
schema-language-id = د ژبې ID
schema-root-markers = د ریښې نښې
schema-output-directory = د وتۍ پوښۍ

menu-scene = صحنه
menu-layout = ترتیب
menu-terminal = ټرمینل
menu-browser = لټونګر
menu-service = خدمت
menu-bookmark = نښه
menu-edit = سمون

layout-knowledge = پوهه
layout-open-knowledge = پوهه پرانیزه
layout-open-welcome-knowledge = د پوهې ښه راغلاست پرانیزه
layout-open-path = { $path } پرانیزه
layout-fold-knowledge = پوهه راټوله کړه
layout-unfold-knowledge = پوهه وغځوه
layout-bookmarks = نښې
layout-new-folder = نوې پوښۍ
layout-add-to-bookmarks = نښو ته زیات کړه
layout-move-to-bookmarks = نښو ته ولېږدوه
layout-stack-number = سټک { $number }
layout-fold-stack = سټک راټول کړه
layout-unfold-stack = سټک وغځوه
layout-close-stack = سټک وتړه
layout-bookmark-in = په { $folder } کې نښه کړه

common-cancel = لغوه
common-delete = ړنګول
common-save = ساتل
common-rename = نوم بدلول
common-expand = غځول
common-collapse = راټولول
common-loading = پورته کېږي…
common-error = تېروتنه
common-output = وتۍ
common-pending = په تمه
common-current = اوسنی
common-stop = تمول
services-command = د Vmux خدمت
services-uptime-seconds = { $seconds }ث
services-uptime-minutes = { $minutes }د { $seconds }ث
services-uptime-hours = { $hours }س { $minutes }د
services-uptime-days = { $days }ورځ { $hours }س

error-page-failed-load = پاڼه پورته نه شوه
error-page-not-found = پاڼه ونه موندل شوه
error-unknown-host = ناپېژندل شوی د Vmux اپ کوربه: { $host }

history-title = مخینه

command-new-app-chat = نوې { $provider }/{ $model } خبرې اترې (اپ)
command-interactive-mode-user = صحنه > تعاملي حالت > کارن
command-interactive-mode-player = صحنه > تعاملي حالت > لوبغاړی
command-minimize-window = ترتیب > کړکۍ > کوچنۍ کول
command-toggle-layout = ترتیب > ترتیب > ترتیب بدلول
command-close-tab = ترتیب > ټب > ټب تړل
command-new-task = ترتیب > ټب > نوې دنده…
command-next-tab = ترتیب > ټب > بل ټب
command-prev-tab = ترتیب > ټب > مخکینی ټب
command-rename-tab = ترتیب > ټب > د ټب نوم بدلول
command-tab-select-1 = ترتیب > ټب > ټب ۱ ټاکل
command-tab-select-2 = ترتیب > ټب > ټب ۲ ټاکل
command-tab-select-3 = ترتیب > ټب > ټب ۳ ټاکل
command-tab-select-4 = ترتیب > ټب > ټب ۴ ټاکل
command-tab-select-5 = ترتیب > ټب > ټب ۵ ټاکل
command-tab-select-6 = ترتیب > ټب > ټب ۶ ټاکل
command-tab-select-7 = ترتیب > ټب > ټب ۷ ټاکل
command-tab-select-8 = ترتیب > ټب > ټب ۸ ټاکل
command-tab-select-last = ترتیب > ټب > وروستی ټب ټاکل
command-close-pane = ترتیب > چوکاټ > چوکاټ تړل
command-select-pane-left = ترتیب > چوکاټ > کیڼ چوکاټ ټاکل
command-select-pane-right = ترتیب > چوکاټ > ښی چوکاټ ټاکل
command-select-pane-up = ترتیب > چوکاټ > پورته چوکاټ ټاکل
command-select-pane-down = ترتیب > چوکاټ > لاندې چوکاټ ټاکل
command-swap-pane-prev = ترتیب > چوکاټ > له مخکیني چوکاټ سره بدلول
command-swap-pane-next = ترتیب > چوکاټ > له بل چوکاټ سره بدلول
command-equalize-pane-size = ترتیب > چوکاټ > د چوکاټونو اندازه برابرول
command-resize-pane-left = ترتیب > چوکاټ > چوکاټ کیڼ ته بیااندازه کول
command-resize-pane-right = ترتیب > چوکاټ > چوکاټ ښي ته بیااندازه کول
command-resize-pane-up = ترتیب > چوکاټ > چوکاټ پورته بیااندازه کول
command-resize-pane-down = ترتیب > چوکاټ > چوکاټ لاندې بیااندازه کول
command-stack-close = ترتیب > سټک > سټک تړل
command-stack-next = ترتیب > سټک > بل سټک
command-stack-previous = ترتیب > سټک > مخکینی سټک
command-stack-reopen = ترتیب > سټک > تړل شوې پاڼه بیا پرانیستل
command-stack-swap-prev = ترتیب > سټک > سټک کیڼ ته وړل
command-stack-swap-next = ترتیب > سټک > سټک ښي ته وړل
command-space-open = ترتیب > ځای > ځایونه
command-terminal-close = ترمینل > ترمینل تړل
command-terminal-next = ترمینل > بل ترمینل
command-terminal-prev = ترمینل > مخکینی ترمینل
command-terminal-clear = ترمینل > ترمینل پاکول
command-browser-prev-page = براوزر > ګرځېدل > شاته
command-browser-next-page = براوزر > ګرځېدل > مخکې
command-browser-reload = براوزر > ګرځېدل > بیا پورته کول
command-browser-hard-reload = براوزر > ګرځېدل > بشپړ بیاپورته کول
command-open-in-place = براوزر > پرانیستل > همدلته پرانیستل
command-open-in-new-stack = براوزر > پرانیستل > په نوي سټک کې پرانیستل
command-open-in-pane-top = براوزر > پرانیستل > په پورته چوکاټ کې پرانیستل
command-open-in-pane-right = براوزر > پرانیستل > په ښي چوکاټ کې پرانیستل
command-open-in-pane-bottom = براوزر > پرانیستل > په لاندې چوکاټ کې پرانیستل
command-open-in-pane-left = براوزر > پرانیستل > په کیڼ چوکاټ کې پرانیستل
command-open-in-new-tab = براوزر > پرانیستل > په نوي ټب کې پرانیستل
command-open-in-new-space = براوزر > پرانیستل > په نوي ځای کې پرانیستل
command-browser-zoom-in = براوزر > لید > غټول
command-browser-zoom-out = براوزر > لید > کوچنی کول
command-browser-zoom-reset = براوزر > لید > اصلي اندازه
command-browser-dev-tools = براوزر > لید > د پراختیاګر وسیلې
command-browser-open-command-bar = براوزر > پټۍ > د قوماندې پټۍ
command-browser-open-page-in-command-bar = براوزر > پټۍ > پاڼه سمول
command-browser-open-path-bar = براوزر > پټۍ > د مسیر ګرځوونکی
command-browser-open-commands = براوزر > پټۍ > قوماندې
command-browser-open-history = براوزر > پټۍ > مخینه
command-service-open = خدمت > د خدمت څارونکی پرانیستل
command-bookmark-toggle-active = نښه > پاڼه نښه کول
command-bookmark-pin-active = نښه > پاڼه سنجاقول

layout-tab = ټب
layout-no-stacks = سټکونه نشته
layout-loading = پورته کېږي…
layout-no-markdown-files = Markdown فایلونه نشته
layout-empty-folder = تش فولډر
layout-worktree = کاري ونه
layout-folder-name = د فولډر نوم
layout-no-pins-bookmarks = سنجاقونه یا نښې نشته
layout-move-to = { $folder } ته وړل
layout-bookmark-current-page = اوسنۍ پاڼه نښه کول
layout-rename-folder = د فولډر نوم بدلول
layout-remove-folder = فولډر لرې کول
layout-update-downloading = اوسمهالنه ښکته کېږي
layout-update-installing = اوسمهالنه لګېږي…
layout-update-ready = نوې نسخه شته
layout-restart-update = د اوسمهالنې لپاره بیاپیل کړئ

agent-preparing = اجنټ چمتو کېږي…
agent-send-all-queued = ټول کتار شوي پرامپټونه اوس ولېږئ (Esc)
agent-send = لېږل (Enter)
agent-ready = چې کله تاسو چمتو یاست.
agent-loading-older = زاړه پیغامونه پورته کېږي…
agent-load-older = زاړه پیغامونه پورته کول
agent-continued-from = له { $source } څخه دوام ورکړل شو
agent-older-context-omitted = زوړ متن پرېښودل شو
agent-interrupted = پرې شوی
agent-allow-tool = { $tool } ته اجازه ورکړئ؟
agent-deny = ردول
agent-allow-always = تل اجازه
agent-allow = اجازه
agent-loading-sessions = ناستې پورته کېږي…
agent-no-resumable-sessions = د بیاپیل وړ ناستې ونه موندل شوې
agent-no-matching-sessions = برابرې ناستې نشته
agent-no-matching-models = برابر موډلونه نشته
agent-choice-help = ↑/↓ یا Ctrl+N/Ctrl+P · ۱–۹ · Enter
agent-choose-repository = د رېپو فولډر وټاکئ
agent-choose-repository-detail = هغه ځایي Git رېپو وټاکئ چې اجنټ یې وکاروي.
agent-choosing = ټاکل کېږي…
agent-choose-folder = فولډر وټاکئ
agent-queued = په کتار کې
agent-attached = نښلول شوي:
agent-cancel-queued = کتار شوی پرامپټ لغوه کول
agent-resume-queued = کتار شوي پرامپټونه بیاپیلول
agent-clear-queue = کتار پاکول
agent-send-all-now = ټول اوس لېږل
agent-choose-option = پورته یو انتخاب وټاکئ
agent-loading-media = رسنۍ پورته کېږي…
agent-no-matching-media = برابره رسنۍ نشته
agent-prompt-context = د پرامپټ متن
agent-details = جزئیات
agent-path = مسیر
agent-tool = وسیله
agent-server = سرور
agent-bytes = { $count } بایټونه
agent-worked-for = { $duration } یې کار وکړ
agent-worked-for-steps = { $count ->
    [one] { $duration } یې کار وکړ · ۱ ګام
   *[other] { $duration } یې کار وکړ · { $count } ګامونه
}
agent-tool-guardian-review = د ساتونکي بیاکتنه
agent-tool-read-files = فایلونه ولوستل
agent-tool-viewed-image = انځور وکتل شو
agent-tool-used-browser = براوزر وکارول شو
agent-tool-searched-files = فایلونه ولټول شول
agent-tool-ran-commands = قوماندې وچلېدې
agent-thinking = فکر کوي
agent-subagent = فرعي اجنټ
agent-prompt = پرامپټ
agent-thread = تار
agent-parent = مورنی
agent-children = بچي
agent-call = زنګ
agent-raw-event = خامه پېښه
agent-plan = پلان
agent-tasks = { $count ->
    [one] ۱ دنده
   *[other] { $count } دندې
}
agent-edited = سمول شوی
agent-reconnecting = بیا نښلېږي { $attempt }/{ $total }
agent-status-running = روان
agent-status-done = بشپړ
agent-status-failed = ناکام
agent-status-pending = په تمه
agent-slash-attach-files = فایلونه نښلول
agent-slash-resume-session = پخوانۍ ناسته بیاپیلول
agent-slash-select-model = موډل ټاکل
agent-slash-continue-cli = دا ناسته په CLI کې دوامول
agent-session-just-now = همدا اوس
agent-session-minutes-ago = { $count }د مخکې
agent-session-hours-ago = { $count }س مخکې
agent-session-days-ago = { $count }ورځ مخکې
agent-working-working = کار کوي
agent-working-thinking = فکر کوي
agent-working-pondering = غور کوي
agent-working-noodling = سوچ کوي
agent-working-percolating = پخېږي
agent-working-conjuring = جوړوي
agent-working-cooking = پخوي
agent-working-brewing = چمتو کوي
agent-working-musing = خیال کوي
agent-working-ruminating = ژور فکر کوي
agent-working-scheming = طرحه جوړوي
agent-working-synthesizing = ترکیبوي
agent-working-tinkering = لاس وهنه کوي
agent-working-churning = تاووي
agent-working-vibing = په جریان کې دی
agent-working-simmering = ورو پخېږي
agent-working-crafting = جوړوي
agent-working-divining = موندنه کوي
agent-working-mulling = غور پرې کوي
agent-working-spelunking = ژوره پلټنه کوي

editor-toggle-explorer = سپړونکی ښکاره/پټول (Cmd+B)
editor-unsaved = نه دی ساتل شوی
editor-rendered-markdown = رېنډر شوی Markdown له ژوندۍ سمونې سره
editor-note = یادښت
editor-source-editor = د سرچینې اېډېټر
editor-editor = اېډېټر
editor-git-diff = د Git توپیر
editor-diff = توپیر
editor-tidy = پاکول
editor-always = تل
editor-unchanged-previews = { $count ->
    [one] ✦ ۱ نه‌بدل شوی مخکتنه
   *[other] ✦ { $count } نه‌بدل شوې مخکتنې
}
editor-open-externally = په بهرني اپ کې پرانیستل
editor-changed-line = بدله شوې کرښه
editor-go-to-definition = تعریف ته تلل
editor-find-references = مراجع موندل
editor-references = { $count ->
    [one] ۱ مرجع
   *[other] { $count } مراجع
}
editor-lsp-starting = { $server } پیلېږي…
editor-lsp-not-installed = { $server } — نه دی لګول شوی
editor-explorer = سپړونکی
editor-open-editors = پرانیستي اېډېټرونه
editor-outline = خاکه
editor-new-file = نوی فایل
editor-new-folder = نوی فولډر
editor-delete-confirm = “{ $name }” ړنګ کړئ؟ دا کار بېرته نه شي کېدای.
editor-created-folder = فولډر { $name } جوړ شو
editor-created-file = فایل { $name } جوړ شو
editor-renamed-to = نوم یې { $name } ته بدل شو
editor-deleted = { $name } ړنګ شو
editor-failed-decode-image = انځور لوستلای نه شو
editor-preview-large-image = انځور (د مخکتنې لپاره ډېر لوی)
editor-preview-binary = باینري
editor-preview-file = فایل

git-status-clean = پاک
git-status-modified = بدل شوی
git-status-staged = سټېج شوی
git-status-staged-modified = سټېج شوی*
git-status-untracked = نه‌تعقیب شوی
git-status-deleted = ړنګ شوی
git-status-conflict = ټکر
git-accept-all = ✓ ټول منل
git-unstage = له سټېج څخه ایستل
git-confirm-deny-all = د ټولو ردول تایید کړئ
git-deny-all = ✗ ټول ردول
git-commit-message = د کمېټ پیغام
git-commit = کمېټ ({ $count })
git-push = ↑ Push
git-loading-diff = توپیر پورته کېږي…
git-no-changes = د ښودلو لپاره بدلونونه نشته
git-accept = ✓ منل
git-deny = ✗ ردول
git-show-unchanged-lines = { $count } نه‌بدلې کرښې ښودل

terminal-loading = پورته کېږي…
terminal-runs-when-ready = چې چمتو شي چلېږي · Ctrl+C پاکوي · Esc پرېږدي
terminal-booting = پیلېږي
terminal-type-command = قومانده ولیکئ · چې چمتو شي چلېږي · Esc پرېږدي

setup-tagline-claude = د Anthropic د کوډینګ اجنټ، په Vmux کې
setup-tagline-codex = د OpenAI د کوډینګ اجنټ، په Vmux کې
setup-tagline-vibe = د Mistral د کوډینګ اجنټ، په Vmux کې
setup-install-title = د { $name } CLI لګول
setup-homebrew-required = د { $command } لګولو لپاره Homebrew ته اړتیا ده او لا نه دی چمتو شوی. Vmux به لومړی Homebrew ولګوي، بیا { $name }.
setup-terminal-instructions = په ترمینل کې د پیل لپاره Return کېکاږئ، بیا چې وغوښتل شي د خپل Mac پټنوم دننه کړئ.
setup-command-missing = Vmux دا پاڼه ځکه پرانیسته چې ځایي { $command } قومانده لا نه ده لګول شوې. د ترلاسه کولو لپاره لاندې قومانده وچلوئ.
setup-install-failed = لګول بشپړ نه شول. د جزئیاتو لپاره ترمینل وګورئ، بیا بیا هڅه وکړئ.
setup-installing = لګېږي…
setup-install-homebrew = Homebrew + { $name } لګول
setup-run-install = د لګولو قومانده چلول
setup-auto-reload = Vmux یې په ترمینل کې چلوي او کله چې { $command } چمتو شي، بیا یې پورته کوي.

debug-title = ډیبګ
debug-auto-update = اتومات اوسمهالنه
debug-simulate-update = د شته اوسمهالنې تقلید
debug-simulate-download = د ښکته کولو تقلید
debug-clear-update = اوسمهالنه پاکول
debug-trigger-restart = بیاپیلول راپارول

command-manage-spaces = سپېسونه مدیریت کړئ…
command-pane-stack-location = پین { $pane } / سټک { $stack }
command-space-pane-stack-location = { $space } / پین { $pane } / سټک { $stack }
command-terminal-path = ټرمینل ({ $path })
command-group-interactive-mode = تعاملي حالت
command-group-window = کړکۍ
command-group-tab = ټب
command-group-pane = پین
command-group-stack = سټک
command-group-space = سپېس
command-group-navigation = ګرځښت
command-group-open = پرانیستل
command-group-view = لید
command-group-bar = پټه

menu-close-vmux = Vmux بند کړئ

agents-terminal-coding-agent = د ټرمینل پر بنسټ د کوډ لیکلو اېجنټ
