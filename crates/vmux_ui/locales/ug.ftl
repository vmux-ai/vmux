locale-name = ئۇيغۇرچە
common-open = ئېچىش
common-close = تاقاش
common-install = ئورنىتىش
common-uninstall = ئۆچۈرۈش
common-update = يېڭىلاش
common-retry = قايتا سىناش
common-refresh = يېڭىلاش
common-remove = چىقىرىۋېتىش
common-enable = قوزغىتىش
common-disable = توختىتىش
common-new = يېڭى
common-active = ئاكتىپ
common-running = ئىجرا بولۇۋاتىدۇ
common-done = تامام
common-failed = مەغلۇپ بولدى
common-installed = ئورنىتىلغان
common-items = { $count ->
    [one] { $count } تۈر
   *[other] { $count } تۈر
}

tools-title = قوراللار
tools-search = بوغچا، ۋاكالەتچى، MCP، تىل قورالى ۋە سەپلىمە ھۆججەتلىرىنى ئىزدەش…
tools-open = قوراللارنى ئېچىش
tools-fold = قوراللارنى يىغىش
tools-unfold = قوراللارنى يېيىش
tools-scanning = يەرلىك قوراللار تەكشۈرۈلۈۋاتىدۇ…
tools-no-installed = قاچىلانغان قورال يوق
tools-empty = ماس كېلىدىغان قورال يوق
tools-empty-detail = بىر بوغچا قاچىلاڭ ياكى Stow ئۇسلۇبىدىكى سەپلىمە ھۆججەتلىرى بوغچىسىنى قوشۇڭ.
tools-apply = قوللىنىش
tools-homebrew = Homebrew
tools-homebrew-sync = قاچىلانغان فورمۇلا ۋە ئەپلەر ئاپتوماتىك ماسقەدەملىنىدۇ.
tools-open-brewfile = Brewfile نى ئېچىش
tools-managed = باشقۇرۇلىدۇ
tools-provider-homebrew-formulae = Homebrew فورمۇلالىرى
tools-provider-homebrew-casks = Homebrew ئەپلىرى
tools-provider-npm = npm بوغچىلىرى
tools-provider-acp-agents = ACP ۋاكالەتچىلىرى
tools-provider-language-tools = تىل قوراللىرى
tools-provider-mcp-servers = MCP مۇلازىمېتىرلىرى
tools-provider-dotfiles = سەپلىمە ھۆججەتلىرى
tools-status-available = بار
tools-status-missing = يوق
tools-status-conflict = توقۇنۇش
tools-forget = ئۇنتۇش
tools-manage = باشقۇرۇش
tools-link = ئۇلاش
tools-unlink = ئۇلىنىشنى ئۈزۈش
tools-import = ئەكىرىش
tools-update-count = { $count ->
    [one] 1 يېڭىلاش
   *[other] { $count } يېڭىلاش
}
tools-conflict-count = { $count ->
    [one] 1 توقۇنۇش
   *[other] { $count } توقۇنۇش
}
tools-result-applied = قوراللار قوللىنىلدى
tools-result-imported = قوراللار ئەكىرىلدى
tools-result-installed = { $name } قاچىلاندى
tools-result-updated = { $name } يېڭىلاندى
tools-result-uninstalled = { $name } ئۆچۈرۈلدى
tools-result-forgotten = { $name } ئۇنتۇلدى
tools-result-managed = { $name } ھازىر باشقۇرۇلىدۇ
tools-result-linked = { $name } ئۇلاندى
tools-result-unlinked = { $name } نىڭ ئۇلىنىشى ئۈزۈلدى

start-title = باشلاش
start-tagline = بىر prompt. ھەممىسى تەييار.

agents-title = ئاگېنتلار
agents-search = ACP ۋە CLI ئاگېنتلىرىنى ئىزدەڭ…
agents-empty = ماس كېلىدىغان ئاگېنت يوق
agents-empty-detail = نام، ئىجرا مۇھىتى ياكى ACP/CLI نى سىناپ بېقىڭ.
agents-install-failed = ئورنىتىش مەغلۇپ بولدى
agents-updating = يېڭىلىنىۋاتىدۇ…
agents-retrying = قايتا سىنىلىۋاتىدۇ…
agents-preparing = تەييارلىنىۋاتىدۇ…

extensions-title = كېڭەيتىلمىلەر
extensions-search = ئورنىتىلغانلاردىن ياكى Chrome Web Store دىن ئىزدەڭ…
extensions-relaunch = قوللىنىش ئۈچۈن قايتا قوزغىتىڭ
extensions-empty = ھېچقانداق كېڭەيتىلمە ئورنىتىلمىغان
extensions-no-match = ماس كېلىدىغان كېڭەيتىلمە يوق
extensions-empty-detail = يۇقىرىدىن Chrome Web Store دىن ئىزدەپ، Return نى بېسىڭ.
extensions-no-match-detail = باشقا نام ياكى كېڭەيتىلمە ID سىنى سىناڭ.
extensions-on = ئېچىلغان
extensions-off = تاقالغان
extensions-enable-confirm = { $name } نى قوزغىتامسىز؟
extensions-enable-permissions = { $name } نى قوزغىتىپ تۆۋەندىكىلەرگە رۇخسەت بېرىش:

lsp-title = تىل مۇلازىمېتىرلىرى
lsp-search = تىل مۇلازىمېتىرلىرى، linter ۋە formatter لارنى ئىزدەڭ…
lsp-loading = مۇندەرىجە يۈكلىنىۋاتىدۇ…
lsp-empty = ماس كېلىدىغان تىل مۇلازىمېتىرى يوق
lsp-empty-detail = باشقا تىل، linter ياكى formatter نى سىناڭ.
lsp-needs = { $tool } كېرەك
lsp-status-available = بار
lsp-status-on-path = PATH تا بار
lsp-status-installing = ئورنىتىلىۋاتىدۇ…
lsp-status-installed = ئورنىتىلغان
lsp-status-outdated = يېڭىلانما بار
lsp-status-running = ئىجرا بولۇۋاتىدۇ
lsp-status-failed = مەغلۇپ بولدى

spaces-title = ماكانلار
spaces-new-placeholder = يېڭى ماكان نامى
spaces-empty = ماكان يوق
spaces-default-name = ماكان { $number }
spaces-tabs = { $count ->
    [one] 1 بەتكۈچ
   *[other] { $count } بەتكۈچ
}
spaces-delete = ماكاننى ئۆچۈرۈش

team-title = گۇرۇپپا
team-just-you = بۇ ماكاندا پەقەت سىزلا بار
team-agents = { $count ->
    [one] سىز ۋە 1 ئاگېنت
   *[other] سىز ۋە { $count } ئاگېنت
}
team-empty = بۇ يەردە تېخى ھېچكىم يوق
team-you = سىز
team-agent = ئاگېنت

services-title = ئارقا سۇپىدىكى مۇلازىمەتلەر
services-processes = { $count ->
    [one] 1 جەريان
   *[other] { $count } جەريان
}
services-kill-all = ھەممىسىنى مەجبۇرىي توختىتىش
services-not-running = مۇلازىمەت ئىجرا بولمايۋاتىدۇ
services-start-with = مۇنداق باشلاش:
services-empty = ئاكتىپ جەريان يوق
services-filter = جەريانلارنى سۈزۈش…
services-no-match = ماس كېلىدىغان جەريان يوق
services-connected = ئۇلانغان
services-disconnected = ئۈزۈلگەن
services-attached = باغلانغان
services-kill = مەجبۇرىي توختىتىش
services-memory = ئىچكى ساقلىغۇچ
services-size = چوڭلۇقى
services-shell = Shell

error-title = خاتالىق

history-search = تارىختىن ئىزدەش
history-clear-all = ھەممىسىنى تازىلاش
history-clear-confirm = پۈتۈن تارىخنى تازىلىسۇنمۇ؟
history-clear-warning = بۇنى قايتۇرۇۋالغىلى بولمايدۇ.
history-cancel = ۋاز كېچىش
history-today = بۈگۈن
history-yesterday = تۈنۈگۈن
history-days-ago = { $count } كۈن بۇرۇن
history-day-offset = كۈن -{ $count }

settings-title = تەڭشەكلەر
settings-loading = تەڭشەكلەر يۈكلىنىۋاتىدۇ…
settings-stored = ~/.vmux/settings.ron دا ساقلىنىدۇ
settings-other = باشقا
settings-software-update = يۇمشاق دېتال يېڭىلاش
settings-check-updates = يېڭىلانما تەكشۈرۈش
settings-check-updates-hint = ئاپتوماتىك يېڭىلاش قوزغىتىلسا، ئېچىلغاندا ۋە ھەر سائەتتە ئۆزلۈكىدىن تەكشۈرىدۇ.
settings-update-unavailable = ئىشلەتكىلى بولمايدۇ
settings-update-unavailable-hint = بۇ build ئىچىدە يېڭىلىغۇچ يوق.
settings-update-checking = تەكشۈرۈلۈۋاتىدۇ…
settings-update-checking-hint = يېڭىلانما تەكشۈرۈلۈۋاتىدۇ…
settings-update-check-again = قايتا تەكشۈرۈش
settings-update-current = Vmux ئەڭ يېڭى نەشرىدە.
settings-update-downloading = چۈشۈرۈلۈۋاتىدۇ…
settings-update-downloading-hint = Vmux { $version } چۈشۈرۈلۈۋاتىدۇ…
settings-update-installing = ئورنىتىلىۋاتىدۇ…
settings-update-installing-hint = Vmux { $version } ئورنىتىلىۋاتىدۇ…
settings-update-ready = يېڭىلانما تەييار
settings-update-ready-hint = Vmux { $version } تەييار. قوللىنىش ئۈچۈن قايتا قوزغىتىڭ.
settings-update-try-again = قايتا سىناش
settings-update-failed = يېڭىلانما تەكشۈرگىلى بولمىدى.
settings-item = تۈر
settings-item-number = تۈر { $number }
settings-press-key = بىر كۇنۇپكا بېسىڭ…
settings-saved = ساقلاندى
settings-record-key = يېڭى كۇنۇپكا بىرىكمىسىنى خاتىرىلەش ئۈچۈن چېكىڭ

tray-open-window = كۆزنەكنى ئېچىش
tray-close-window = كۆزنەكنى تاقاش
tray-pause-recording = خاتىرىلەشنى توختىتىپ تۇرۇش
tray-resume-recording = خاتىرىلەشنى داۋاملاشتۇرۇش
tray-finish-recording = خاتىرىلەشنى ئاخىرلاشتۇرۇش
tray-quit = Vmux تىن چېكىنىش

composer-attach-files = ھۆججەت قوشۇش (/upload)
composer-remove-attachment = قوشۇلمىنى چىقىرىۋېتىش

layout-back = كەينىگە
layout-forward = ئالدىغا
layout-reload = قايتا يۈكلەش
layout-bookmark-page = بۇ بەتنى خەتكۈچلەش
layout-remove-bookmark = خەتكۈچنى چىقىرىۋېتىش
layout-pin-page = بۇ بەتنى قاداش
layout-unpin-page = بۇ بەتنى قاداقتىن ئېلىش
layout-manage-extensions = كېڭەيتىلمىلەرنى باشقۇرۇش
layout-new-stack = يېڭى قاتلام
layout-close-tab = بەتكۈچنى تاقاش
layout-bookmark = خەتكۈچ
layout-pin = قاداش
layout-new-tab = يېڭى بەتكۈچ
layout-team = گۇرۇپپا

command-switch-space = ماكان ئالماشتۇرۇش…
command-search-ask = ئىزدەڭ ياكى سوراڭ…
command-new-tab-placeholder = ئىزدەڭ، URL كىرگۈزۈڭ ياكى Terminal تاللاڭ…
command-placeholder = URL كىرگۈزۈڭ، بەتكۈچ ئىزدەڭ ياكى بۇيرۇقلار ئۈچۈن > كىرگۈزۈڭ…
command-composer-placeholder = بۇيرۇقلار ئۈچۈن /، مېدىيا ئۈچۈن @ كىرگۈزۈڭ
command-send = يوللاش (Enter)
command-terminal = تېرمىنال
command-open-terminal = تېرمىنالدا ئېچىش
command-stack = قاتلام
command-tabs = { $count ->
    [one] 1 بەتكۈچ
   *[other] { $count } بەتكۈچ
}
command-prompt = Prompt
command-new-tab = يېڭى بەتكۈچ
command-search = ئىزدەش
command-open-value = «{ $value }» نى ئېچىش
command-search-value = «{ $value }» نى ئىزدەش

schema-appearance = كۆرۈنۈش
schema-general = ئادەتتىكى
schema-layout = جايلىشىش
schema-layout-detail = كۆزنەك، بۆلەكلەر، يان بالداق ۋە فوكۇس ھالقىسى.
schema-agent = ئاگېنت
schema-agent-detail = ئاگېنتنىڭ ھەرىكىتى ۋە قورال رۇخسەتلىرى.
schema-shortcuts = تېزلەتمىلەر
schema-shortcuts-detail = پەقەت كۆرۈش. باغلانمىلارنى ئۆزگەرتىش ئۈچۈن settings.ron نى بىۋاسىتە تەھرىرلەڭ.
schema-terminal = تېرمىنال
schema-browser = توركۆرگۈ
schema-mode = ھالەت
schema-mode-detail = تور بەتلەرنىڭ رەڭ لايىھىسى. ئۈسكۈنە ھالىتى سىستېمىڭىزغا ئەگىشىدۇ.
schema-device = ئۈسكۈنە
schema-light = يورۇق
schema-dark = قاراڭغۇ
schema-language = تىل
schema-language-detail = سىستېما تىلى، en-US، ja ياكى ماس كېلىدىغان ~/.vmux/locales/<tag>.ftl مۇندەرىجىسى بار خالىغان BCP 47 بەلگىسىنى ئىشلىتىڭ.
schema-auto-update = ئاپتوماتىك يېڭىلاش
schema-auto-update-detail = ئېچىلغاندا ۋە ھەر سائەتتە يېڭىلانما تەكشۈرۈپ ئورنىتىدۇ.
schema-startup-url = قوزغىلىش URL
schema-startup-url-detail = بوش قالسا بۇيرۇق بالداق prompt ى ئېچىلىدۇ.
schema-search-engine = ئىزدەش ماتورى
schema-search-engine-detail = باشلاش بېتى ۋە بۇيرۇق بالداقتىن تور ئىزدەشكە ئىشلىتىلىدۇ.
schema-window = كۆزنەك
schema-pane = بۆلەك
schema-side-sheet = يان تاختا
schema-focus-ring = فوكۇس ھالقىسى
schema-run-placement = ئىجرا جايلىشىشىنى قاپلاشقا رۇخسەت قىلىش
schema-run-placement-detail = ئاگېنتلار ئىجرا بۆلىكى ھالىتى، يۆنىلىشى ۋە تىرەك نۇقتىسىنى تاللىيالىسۇن.
schema-leader = باشلامچى كۇنۇپكا
schema-leader-detail = chord تېزلەتمىلىرىنىڭ ئالدى كۇنۇپكىسى.
schema-chord-timeout = Chord ۋاقىت چېكى
schema-chord-timeout-detail = chord ئالدى كۇنۇپكىسىنىڭ كۈچى يوقىلىشتىن بۇرۇنقى مىللىسېكۇنت.
schema-bindings = باغلانمىلار
schema-confirm-close = تاقاشنى جەزملەش
schema-confirm-close-detail = ئىجرا بولۇۋاتقان جەريانى بار تېرمىنالنى تاقاشتىن بۇرۇن سورايدۇ.
schema-default-theme = كۆڭۈلدىكى تېما
schema-default-theme-detail = تېمىلار تىزىملىكىدىكى ئاكتىپ تېمىنىڭ نامى.

settings-empty = (قۇرۇق)
settings-none = (يوق)

schema-system = سىستېما
schema-editor = تەھرىرلىگۈچ
schema-recording = خاتىرىلەش
schema-radius = رادىئۇس
schema-padding = ئىچكى بوشلۇق
schema-gap = ئارىلىق
schema-width = كەڭلىك
schema-color = رەڭ
schema-red = قىزىل
schema-green = يېشىل
schema-blue = كۆك
schema-follow-files = ھۆججەتلەرگە ئەگىشىش
schema-tidy-files = ھۆججەتلەرنى رەتلەش
schema-tidy-files-max = ھۆججەت رەتلەش چېكى
schema-tidy-files-auto = ھۆججەتلەرنى ئاپتوماتىك رەتلەش
schema-app-providers = ئەپ تەمىنلىگۈچىلەر
schema-provider = تەمىنلىگۈچى
schema-kind = تۈرى
schema-models = مودېللار
schema-acp = ACP ۋاكالەتچىلىرى
schema-id = ID
schema-name = نامى
schema-command = بۇيرۇق
schema-arguments = ئارگۇمېنتلار
schema-environment = مۇھىت
schema-working-directory = خىزمەت مۇندەرىجىسى
schema-shell = Shell
schema-font-family = خەت نۇسخىسى
schema-startup-directory = قوزغىلىش مۇندەرىجىسى
schema-themes = ئۇسلۇبلار
schema-color-scheme = رەڭ لايىھىسى
schema-font-size = خەت چوڭلۇقى
schema-line-height = قۇر ئېگىزلىكى
schema-cursor-style = نۇر بەلگە ئۇسلۇبى
schema-cursor-blink = نۇر بەلگىنىڭ چاقنىشى
schema-custom-themes = خاس ئۇسلۇبلار
schema-foreground = ئالدى رەڭ
schema-background = تەگلىك
schema-cursor = نۇر بەلگە
schema-ansi-colors = ANSI رەڭلىرى
schema-keymap = كۇنۇپكا خەرىتىسى
schema-explorer = كۆزەتكۈچ
schema-visible = كۆرۈنىدۇ
schema-language-servers = تىل مۇلازىمېتىرلىرى
schema-servers = مۇلازىمېتىرلار
schema-language-id = تىل ID
schema-root-markers = يىلتىز بەلگىلىرى
schema-output-directory = چىقىرىش مۇندەرىجىسى

menu-scene = كۆرۈنۈش
menu-layout = ئورۇنلاشتۇرۇش
menu-terminal = تېرمىنال
menu-browser = توركۆرگۈچ
menu-service = مۇلازىمەت
menu-bookmark = خەتكۈچ
menu-edit = تەھرىرلەش

layout-knowledge = بىلىم
layout-open-knowledge = بىلىمنى ئېچىش
layout-open-welcome-knowledge = بىلىمگە خۇش كەلدىڭىزنى ئېچىش
layout-open-path = { $path } نى ئېچىش
layout-fold-knowledge = بىلىمنى قاتلاش
layout-unfold-knowledge = بىلىمنى يېيىش
layout-bookmarks = خەتكۈچلەر
layout-new-folder = يېڭى قىسقۇچ
layout-add-to-bookmarks = خەتكۈچلەرگە قوشۇش
layout-move-to-bookmarks = خەتكۈچلەرگە يۆتكەش
layout-stack-number = قاتلام { $number }
layout-fold-stack = قاتلامنى قاتلاش
layout-unfold-stack = قاتلامنى يېيىش
layout-close-stack = قاتلامنى تاقاش
layout-bookmark-in = { $folder } ئىچىگە خەتكۈچلەش

common-cancel = ۋاز كەچ
common-delete = ئۆچۈر
common-save = ساقلا
common-rename = نام ئۆزگەرت
common-expand = ياپ
common-collapse = يىغ
common-loading = يۈكلىنىۋاتىدۇ…
common-error = خاتالىق
common-output = چىقىرىش
common-pending = كۈتۈۋاتىدۇ
common-current = نۆۋەتتىكى
common-stop = توختات
services-command = Vmux مۇلازىمىتى
services-uptime-seconds = { $seconds } سېكۇنت
services-uptime-minutes = { $minutes } مىنۇت { $seconds } سېكۇنت
services-uptime-hours = { $hours } سائەت { $minutes } مىنۇت
services-uptime-days = { $days } كۈن { $hours } سائەت

error-page-failed-load = بەت يۈكلەنمىدى
error-page-not-found = بەت تېپىلمىدى
error-unknown-host = نامەلۇم Vmux ئەپ مۇلازىمېتىرى: { $host }

history-title = تارىخ

command-new-app-chat = يېڭى { $provider }/{ $model } سۆھبىتى (ئەپ)
command-interactive-mode-user = كۆرۈنۈش > ئۆزئارا ھالەت > ئىشلەتكۈچى
command-interactive-mode-player = كۆرۈنۈش > ئۆزئارا ھالەت > قويغۇچى
command-minimize-window = ئورۇنلاشتۇرۇش > كۆزنەك > كىچىكلەت
command-toggle-layout = ئورۇنلاشتۇرۇش > ئورۇنلاشتۇرۇش > ئورۇنلاشتۇرۇشنى ئالماشتۇر
command-close-tab = ئورۇنلاشتۇرۇش > بەتكۈچ > بەتكۈچنى ياپ
command-new-task = ئورۇنلاشتۇرۇش > بەتكۈچ > يېڭى ۋەزىپە…
command-next-tab = ئورۇنلاشتۇرۇش > بەتكۈچ > كېيىنكى بەتكۈچ
command-prev-tab = ئورۇنلاشتۇرۇش > بەتكۈچ > ئالدىنقى بەتكۈچ
command-rename-tab = ئورۇنلاشتۇرۇش > بەتكۈچ > بەتكۈچ نامىنى ئۆزگەرت
command-tab-select-1 = ئورۇنلاشتۇرۇش > بەتكۈچ > 1-بەتكۈچنى تاللا
command-tab-select-2 = ئورۇنلاشتۇرۇش > بەتكۈچ > 2-بەتكۈچنى تاللا
command-tab-select-3 = ئورۇنلاشتۇرۇش > بەتكۈچ > 3-بەتكۈچنى تاللا
command-tab-select-4 = ئورۇنلاشتۇرۇش > بەتكۈچ > 4-بەتكۈچنى تاللا
command-tab-select-5 = ئورۇنلاشتۇرۇش > بەتكۈچ > 5-بەتكۈچنى تاللا
command-tab-select-6 = ئورۇنلاشتۇرۇش > بەتكۈچ > 6-بەتكۈچنى تاللا
command-tab-select-7 = ئورۇنلاشتۇرۇش > بەتكۈچ > 7-بەتكۈچنى تاللا
command-tab-select-8 = ئورۇنلاشتۇرۇش > بەتكۈچ > 8-بەتكۈچنى تاللا
command-tab-select-last = ئورۇنلاشتۇرۇش > بەتكۈچ > ئاخىرقى بەتكۈچنى تاللا
command-close-pane = ئورۇنلاشتۇرۇش > كۆزنەكچە > كۆزنەكچىنى ياپ
command-select-pane-left = ئورۇنلاشتۇرۇش > كۆزنەكچە > سول كۆزنەكچىنى تاللا
command-select-pane-right = ئورۇنلاشتۇرۇش > كۆزنەكچە > ئوڭ كۆزنەكچىنى تاللا
command-select-pane-up = ئورۇنلاشتۇرۇش > كۆزنەكچە > ئۈستى كۆزنەكچىنى تاللا
command-select-pane-down = ئورۇنلاشتۇرۇش > كۆزنەكچە > ئاستى كۆزنەكچىنى تاللا
command-swap-pane-prev = ئورۇنلاشتۇرۇش > كۆزنەكچە > ئالدىنقى كۆزنەكچە بىلەن ئالماشتۇر
command-swap-pane-next = ئورۇنلاشتۇرۇش > كۆزنەكچە > كېيىنكى كۆزنەكچە بىلەن ئالماشتۇر
command-equalize-pane-size = ئورۇنلاشتۇرۇش > كۆزنەكچە > كۆزنەكچە چوڭلۇقىنى تەڭلە
command-resize-pane-left = ئورۇنلاشتۇرۇش > كۆزنەكچە > كۆزنەكچىنى سولغا چوڭلۇقىنى ئۆزگەرت
command-resize-pane-right = ئورۇنلاشتۇرۇش > كۆزنەكچە > كۆزنەكچىنى ئوڭغا چوڭلۇقىنى ئۆزگەرت
command-resize-pane-up = ئورۇنلاشتۇرۇش > كۆزنەكچە > كۆزنەكچىنى ئۈستىگە چوڭلۇقىنى ئۆزگەرت
command-resize-pane-down = ئورۇنلاشتۇرۇش > كۆزنەكچە > كۆزنەكچىنى ئاستىغا چوڭلۇقىنى ئۆزگەرت
command-stack-close = ئورۇنلاشتۇرۇش > قاتلام > قاتلامنى ياپ
command-stack-next = ئورۇنلاشتۇرۇش > قاتلام > كېيىنكى قاتلام
command-stack-previous = ئورۇنلاشتۇرۇش > قاتلام > ئالدىنقى قاتلام
command-stack-reopen = ئورۇنلاشتۇرۇش > قاتلام > يېپىلغان بەتنى قايتا ئاچ
command-stack-swap-prev = ئورۇنلاشتۇرۇش > قاتلام > قاتلامنى سولغا يۆتكە
command-stack-swap-next = ئورۇنلاشتۇرۇش > قاتلام > قاتلامنى ئوڭغا يۆتكە
command-space-open = ئورۇنلاشتۇرۇش > بوشلۇق > بوشلۇقلار
command-terminal-close = تېرمىنال > تېرمىنالنى ياپ
command-terminal-next = تېرمىنال > كېيىنكى تېرمىنال
command-terminal-prev = تېرمىنال > ئالدىنقى تېرمىنال
command-terminal-clear = تېرمىنال > تېرمىنالنى تازىلا
command-browser-prev-page = توركۆرگۈچ > يولباشلاش > كەينىگە
command-browser-next-page = توركۆرگۈچ > يولباشلاش > ئالدىغا
command-browser-reload = توركۆرگۈچ > يولباشلاش > قايتا يۈكلە
command-browser-hard-reload = توركۆرگۈچ > يولباشلاش > مەجبۇرىي قايتا يۈكلە
command-open-in-place = توركۆرگۈچ > ئاچ > بۇ يەردە ئاچ
command-open-in-new-stack = توركۆرگۈچ > ئاچ > يېڭى قاتلامدا ئاچ
command-open-in-pane-top = توركۆرگۈچ > ئاچ > ئۈستىدىكى كۆزنەكچىدە ئاچ
command-open-in-pane-right = توركۆرگۈچ > ئاچ > ئوڭ كۆزنەكچىدە ئاچ
command-open-in-pane-bottom = توركۆرگۈچ > ئاچ > ئاستىدىكى كۆزنەكچىدە ئاچ
command-open-in-pane-left = توركۆرگۈچ > ئاچ > سول كۆزنەكچىدە ئاچ
command-open-in-new-tab = توركۆرگۈچ > ئاچ > يېڭى بەتكۈچتە ئاچ
command-open-in-new-space = توركۆرگۈچ > ئاچ > يېڭى بوشلۇقتا ئاچ
command-browser-zoom-in = توركۆرگۈچ > كۆرۈنۈش > چوڭايت
command-browser-zoom-out = توركۆرگۈچ > كۆرۈنۈش > كىچىكلەت
command-browser-zoom-reset = توركۆرگۈچ > كۆرۈنۈش > ئەسلى چوڭلۇق
command-browser-dev-tools = توركۆرگۈچ > كۆرۈنۈش > ئاچقۇچى قوراللىرى
command-browser-open-command-bar = توركۆرگۈچ > بالداق > بۇيرۇق بالداق
command-browser-open-page-in-command-bar = توركۆرگۈچ > بالداق > بەتنى تەھرىرلە
command-browser-open-path-bar = توركۆرگۈچ > بالداق > يول يولباشلىغۇچ
command-browser-open-commands = توركۆرگۈچ > بالداق > بۇيرۇقلار
command-browser-open-history = توركۆرگۈچ > بالداق > تارىخ
command-service-open = مۇلازىمەت > مۇلازىمەت نازارەتچىسىنى ئاچ
command-bookmark-toggle-active = خەتكۈش > بەتنى خەتكۈچلە
command-bookmark-pin-active = خەتكۈش > بەتنى مىخلا

layout-tab = بەتكۈچ
layout-no-stacks = قاتلام يوق
layout-loading = يۈكلىنىۋاتىدۇ…
layout-no-markdown-files = Markdown ھۆججەتلىرى يوق
layout-empty-folder = بوش قىسقۇچ
layout-worktree = خىزمەت دەرىخى
layout-folder-name = قىسقۇچ نامى
layout-no-pins-bookmarks = مىخ ياكى خەتكۈش يوق
layout-move-to = { $folder } غا يۆتكە
layout-bookmark-current-page = نۆۋەتتىكى بەتنى خەتكۈچلە
layout-rename-folder = قىسقۇچ نامىنى ئۆزگەرت
layout-remove-folder = قىسقۇچنى ئۆچۈر
layout-update-downloading = يېڭىلانما چۈشۈرۈلۈۋاتىدۇ
layout-update-installing = يېڭىلانما ئورنىتىلىۋاتىدۇ…
layout-update-ready = يېڭى نەشرى بار
layout-restart-update = يېڭىلاش ئۈچۈن قايتا قوزغات

agent-preparing = ۋاكالەتچى تەييارلىنىۋاتىدۇ…
agent-send-all-queued = ئۆچىرەتتىكى ئەسكەرتىشلەرنىڭ ھەممىسىنى ھازىر ئەۋەت (Esc)
agent-send = ئەۋەت (Enter)
agent-ready = تەييار بولسىڭىز باشلايمەن.
agent-loading-older = كونا ئۇچۇرلار يۈكلىنىۋاتىدۇ…
agent-load-older = كونا ئۇچۇرلارنى يۈكلە
agent-continued-from = { $source } دىن داۋاملاشتى
agent-older-context-omitted = كونا مەزمۇن قالدۇرۇلدى
agent-interrupted = ئۈزۈلدى
agent-allow-tool = { $tool } غا رۇخسەت قىلامسىز؟
agent-deny = رەت قىل
agent-allow-always = ھەمىشە رۇخسەت قىل
agent-allow = رۇخسەت قىل
agent-loading-sessions = ئولتۇرۇملار يۈكلىنىۋاتىدۇ…
agent-no-resumable-sessions = داۋاملاشتۇرغىلى بولىدىغان ئولتۇرۇم تېپىلمىدى
agent-no-matching-sessions = ماس ئولتۇرۇم يوق
agent-no-matching-models = ماس مودېل يوق
agent-choice-help = ↑/↓ ياكى Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = ئامبار قىسقۇچىنى تاللا
agent-choose-repository-detail = ۋاكالەتچى ئىشلىتىدىغان يەرلىك Git ئامبىرىنى تاللاڭ.
agent-choosing = تاللىنىۋاتىدۇ…
agent-choose-folder = قىسقۇچ تاللا
agent-queued = ئۆچىرەتتە
agent-attached = قوشۇلغان:
agent-cancel-queued = ئۆچىرەتتىكى ئەسكەرتىشتىن ۋاز كەچ
agent-resume-queued = ئۆچىرەتتىكى ئەسكەرتىشلەرنى داۋاملاشتۇر
agent-clear-queue = ئۆچىرەتنى تازىلا
agent-send-all-now = ھەممىسىنى ھازىر ئەۋەت
agent-choose-option = ئۈستىدىكى بىر تاللاشنى تاللاڭ
agent-loading-media = مېدىيا يۈكلىنىۋاتىدۇ…
agent-no-matching-media = ماس مېدىيا يوق
agent-prompt-context = ئەسكەرتىش مەزمۇنى
agent-details = تەپسىلاتلار
agent-path = يول
agent-tool = قورال
agent-server = مۇلازىمېتىر
agent-bytes = { $count } بايت
agent-worked-for = { $duration } ئىشلىدى
agent-worked-for-steps = { $count ->
    [one] { $duration } ئىشلىدى · 1 قەدەم
   *[other] { $duration } ئىشلىدى · { $count } قەدەم
}
agent-tool-guardian-review = قوغدىغۇچى تەكشۈرۈشى
agent-tool-read-files = ھۆججەتلەرنى ئوقۇدى
agent-tool-viewed-image = سۈرەتنى كۆردى
agent-tool-used-browser = توركۆرگۈچ ئىشلەتتى
agent-tool-searched-files = ھۆججەتلەردىن ئىزدىدى
agent-tool-ran-commands = بۇيرۇقلارنى ئىجرا قىلدى
agent-thinking = ئويلىنىۋاتىدۇ
agent-subagent = تارماق ۋاكالەتچى
agent-prompt = ئەسكەرتىش
agent-thread = تېما
agent-parent = ئاتا
agent-children = بالا
agent-call = چاقىرىق
agent-raw-event = خام ھادىسە
agent-plan = پىلان
agent-tasks = { $count ->
    [one] 1 ۋەزىپە
   *[other] { $count } ۋەزىپە
}
agent-edited = تەھرىرلەندى
agent-reconnecting = قايتا ئۇلىنىۋاتىدۇ { $attempt }/{ $total }
agent-status-running = ئىجرا بولۇۋاتىدۇ
agent-status-done = تامام
agent-status-failed = مەغلۇپ بولدى
agent-status-pending = كۈتۈۋاتىدۇ
agent-slash-attach-files = ھۆججەت قوش
agent-slash-resume-session = بۇرۇنقى ئولتۇرۇمنى داۋاملاشتۇر
agent-slash-select-model = مودېل تاللا
agent-slash-continue-cli = بۇ ئولتۇرۇمنى CLI دا داۋاملاشتۇر
agent-session-just-now = ھازىرلا
agent-session-minutes-ago = { $count } مىنۇت بۇرۇن
agent-session-hours-ago = { $count } سائەت بۇرۇن
agent-session-days-ago = { $count } كۈن بۇرۇن
agent-working-working = ئىشلەۋاتىدۇ
agent-working-thinking = ئويلىنىۋاتىدۇ
agent-working-pondering = چوڭقۇر ئويلىنىۋاتىدۇ
agent-working-noodling = ئىنچىكە ئويلىنىۋاتىدۇ
agent-working-percolating = پىشىپ يېتىلىۋاتىدۇ
agent-working-conjuring = تاپقۇزۇۋاتىدۇ
agent-working-cooking = تەييارلاۋاتىدۇ
agent-working-brewing = دەملەۋاتىدۇ
agent-working-musing = خىيال سۈرۈۋاتىدۇ
agent-working-ruminating = قايتا-قايتا ئويلىنىۋاتىدۇ
agent-working-scheming = لايىھىلەۋاتىدۇ
agent-working-synthesizing = بىرىكتۈرۈۋاتىدۇ
agent-working-tinkering = تەڭشەۋاتىدۇ
agent-working-churning = ئىشلەپ چىقىرىۋاتىدۇ
agent-working-vibing = كەيپىگە كىرىۋاتىدۇ
agent-working-simmering = ئاستا پىشىۋاتىدۇ
agent-working-crafting = ياساۋاتىدۇ
agent-working-divining = پەرەز قىلىۋاتىدۇ
agent-working-mulling = ئويلىشىۋاتىدۇ
agent-working-spelunking = چوڭقۇر قېزىۋاتىدۇ

editor-toggle-explorer = ئىزدىگۈچنى ئالماشتۇر (Cmd+B)
editor-unsaved = ساقلانمىغان
editor-rendered-markdown = نەق تەھرىرلەشلىك كۆرسىتىلگەن Markdown
editor-note = ئىزاھات
editor-source-editor = مەنبە تەھرىرلىگۈچ
editor-editor = تەھرىرلىگۈچ
editor-git-diff = Git پەرقى
editor-diff = پەرق
editor-tidy = رەتلە
editor-always = ھەمىشە
editor-unchanged-previews = { $count ->
    [one] ✦ 1 ئۆزگەرمىگەن ئالدىن كۆرۈش
   *[other] ✦ { $count } ئۆزگەرمىگەن ئالدىن كۆرۈش
}
editor-open-externally = سىرتتا ئاچ
editor-changed-line = ئۆزگەرگەن قۇر
editor-go-to-definition = ئېنىقلىمىغا بار
editor-find-references = نەقىللەرنى تاپ
editor-references = { $count ->
    [one] 1 نەقىل
   *[other] { $count } نەقىل
}
editor-lsp-starting = { $server } قوزغىلىۋاتىدۇ…
editor-lsp-not-installed = { $server } — ئورنىتىلمىغان
editor-explorer = ئىزدىگۈچ
editor-open-editors = ئوچۇق تەھرىرلىگۈچلەر
editor-outline = قۇرۇلما
editor-new-file = يېڭى ھۆججەت
editor-new-folder = يېڭى قىسقۇچ
editor-delete-confirm = «{ $name }» نى ئۆچۈرەمسىز؟ بۇنى ئەسلىگە كەلتۈرگىلى بولمايدۇ.
editor-created-folder = { $name } قىسقۇچى قۇرۇلدى
editor-created-file = { $name } ھۆججىتى قۇرۇلدى
editor-renamed-to = نامى { $name } غا ئۆزگەرتىلدى
editor-deleted = { $name } ئۆچۈرۈلدى
editor-failed-decode-image = سۈرەتنى يېشەلمىدى
editor-preview-large-image = سۈرەت (ئالدىن كۆرۈشكە بەك چوڭ)
editor-preview-binary = ئىككىلىك
editor-preview-file = ھۆججەت

git-status-clean = پاكىز
git-status-modified = ئۆزگەرتىلگەن
git-status-staged = سەھنىلەنگەن
git-status-staged-modified = سەھنىلەنگەن*
git-status-untracked = ئىز قوغلىمىغان
git-status-deleted = ئۆچۈرۈلگەن
git-status-conflict = توقۇنۇش
git-accept-all = ✓ ھەممىنى قوبۇل قىل
git-unstage = سەھنىدىن قايتۇر
git-confirm-deny-all = ھەممىنى رەت قىلىشنى جەزملە
git-deny-all = ✗ ھەممىنى رەت قىل
git-commit-message = commit ئۇچۇرى
git-commit = Commit ({ $count })
git-push = ↑ يوللا
git-loading-diff = پەرق يۈكلىنىۋاتىدۇ…
git-no-changes = كۆرسىتىدىغان ئۆزگىرىش يوق
git-accept = ✓ قوبۇل قىل
git-deny = ✗ رەت قىل
git-show-unchanged-lines = { $count } ئۆزگەرمىگەن قۇرنى كۆرسەت

terminal-loading = يۈكلىنىۋاتىدۇ…
terminal-runs-when-ready = تەييار بولغاندا ئىجرا بولىدۇ · Ctrl+C تازىلايدۇ · Esc ئاتلايدۇ
terminal-booting = قوزغىلىۋاتىدۇ
terminal-type-command = بۇيرۇق كىرگۈزۈڭ · تەييار بولغاندا ئىجرا بولىدۇ · Esc ئاتلايدۇ

setup-tagline-claude = Anthropic نىڭ كودلاش ۋاكالەتچىسى، Vmux ئىچىدە
setup-tagline-codex = OpenAI نىڭ كودلاش ۋاكالەتچىسى، Vmux ئىچىدە
setup-tagline-vibe = Mistral نىڭ كودلاش ۋاكالەتچىسى، Vmux ئىچىدە
setup-install-title = { $name } CLI نى ئورنات
setup-homebrew-required = { $command } نى ئورنىتىش ئۈچۈن Homebrew كېرەك، ئەمما تېخى تەڭشەلمىگەن. Vmux ئالدى بىلەن Homebrew نى، ئاندىن { $name } نى ئورنىتىدۇ.
setup-terminal-instructions = تېرمىنالدا Return نى بېسىپ باشلاڭ، سورالغاندا Mac پارولىڭىزنى كىرگۈزۈڭ.
setup-command-missing = يەرلىك { $command } بۇيرۇقى تېخى ئورنىتىلمىغانلىقى ئۈچۈن Vmux بۇ بەتنى ئاچتى. ئېلىش ئۈچۈن تۆۋەندىكى بۇيرۇقنى ئىجرا قىلىڭ.
setup-install-failed = ئورنىتىش تاماملانمىدى. تەپسىلات ئۈچۈن تېرمىنالنى تەكشۈرۈپ، قايتا سىناڭ.
setup-installing = ئورنىتىلىۋاتىدۇ…
setup-install-homebrew = Homebrew + { $name } نى ئورنات
setup-run-install = ئورنىتىش بۇيرۇقىنى ئىجرا قىل
setup-auto-reload = Vmux ئۇنى تېرمىنالدا ئىجرا قىلىدۇ، { $command } تەييار بولغاندا قايتا يۈكلەيدۇ.

debug-title = سازلاش
debug-auto-update = ئاپتوماتىك يېڭىلاش
debug-simulate-update = يېڭىلانما بارلىقىنى سىناپ كۆرسەت
debug-simulate-download = چۈشۈرۈشنى سىناپ كۆرسەت
debug-clear-update = يېڭىلانمىنى تازىلا
debug-trigger-restart = قايتا قوزغىتىشنى قوزغات

command-manage-spaces = بوشلۇقلارنى باشقۇرۇش…
command-pane-stack-location = كۆزنەكچە { $pane } / دۆۋە { $stack }
command-space-pane-stack-location = { $space } / كۆزنەكچە { $pane } / دۆۋە { $stack }
command-terminal-path = تېرمىنال ({ $path })
command-group-interactive-mode = ئۆزئارا تەسىرلىشىش ھالىتى
command-group-window = كۆزنەك
command-group-tab = بەتكۈچ
command-group-pane = كۆزنەكچە
command-group-stack = دۆۋە
command-group-space = بوشلۇق
command-group-navigation = يۆتكىلىش
command-group-open = ئېچىش
command-group-view = كۆرۈنۈش
command-group-bar = بالداق

menu-close-vmux = Vmux نى تاقاش

agents-terminal-coding-agent = تېرمىنال ئاساسىدىكى كود يېزىش ۋاكالەتچىسى
