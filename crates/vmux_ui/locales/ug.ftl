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
