common-open = خلاصول
common-close = بندول
common-install = نصبول
common-uninstall = لرې کول
common-update = تازه کول
common-retry = بیا هڅه
common-refresh = تازه کول
common-remove = لرې کول
common-enable = فعالول
common-disable = غیر فعالول
common-new = نوی
common-active = فعال
common-running = روان
common-done = بشپړ
common-failed = ناکام
common-installed = نصب شوی
common-items = { $count ->
    [one] { $count } توکی
   *[other] { $count } توکي
}
start-title = پیل
start-tagline = یو اشاره. هر شی، بشپړ.

agents-title = اجنټان
agents-search = د ACP او CLI اجنټان لټول…
agents-empty = هیڅ ورته اجنټ نشته
agents-empty-detail = یو نوم، د چلولو وخت، یا ACP/CLI هڅه وکړئ.
agents-install-failed = نصب ناکام شو
agents-updating = تازه کیږي…
agents-retrying = بیا هڅه کیږي…
agents-preparing = چمتو کیږي…

extensions-title = توسعې
extensions-search = نصب شوي یا د Chrome Web Store لټول…
extensions-relaunch = د پلي کولو لپاره بیا پیل کړئ
extensions-empty = هیڅ توسعه نصب نه ده
extensions-no-match = هیڅ ورته توسعه نشته
extensions-empty-detail = پورته د Chrome Web Store لټول وکړئ او Return فشار ورکړئ.
extensions-no-match-detail = بل نوم یا د توسعې ID هڅه وکړئ.
extensions-on = فعال
extensions-off = غیر فعال
extensions-enable-confirm = { $name } فعال کړئ؟
extensions-enable-permissions = { $name } فعال کړئ او اجازه ورکړئ:

lsp-title = د ژبې سرورونه
lsp-search = د ژبې سرورونه، لینټران، فارمیټران لټول…
lsp-loading = کټالوګ بار کیږي…
lsp-empty = هیڅ ورته د ژبې سرور نشته
lsp-empty-detail = بله ژبه، لینټر، یا فارمیټر هڅه وکړئ.
lsp-needs = { $tool } ته اړتیا ده
lsp-status-available = موجود
lsp-status-on-path = د PATH پر سر
lsp-status-installing = نصبیږي…
lsp-status-installed = نصب شوی
lsp-status-outdated = تازه کول موجود دي
lsp-status-running = روان
lsp-status-failed = ناکام

spaces-title = ځایونه
spaces-new-placeholder = د نوي ځای نوم
spaces-empty = هیڅ ځای نشته
spaces-default-name = ځای { $number }
spaces-tabs = { $count ->
    [one] ۱ ټب
   *[other] { $count } ټبونه
}
spaces-delete = ځای ړنګ کړئ

team-title = ټیم
team-just-you = یوازې تاسو پدې ځای کې
team-agents = { $count ->
    [one] تاسو او ۱ اجنټ
   *[other] تاسو او { $count } اجنټان
}
team-empty = لاهم هیڅوک دلته نشته
team-you = تاسو
team-agent = اجنټ

services-title = شالید خدمتونه
services-processes = { $count ->
    [one] ۱ پروسه
   *[other] { $count } پروسې
}
services-kill-all = ټول بند کړئ
services-not-running = خدمت نه چلیږي
services-start-with = له دې سره پیل کړئ:
services-empty = هیڅ فعاله پروسه نشته
services-filter = پروسې فلټر کړئ…
services-no-match = هیڅ ورته پروسه نشته
services-connected = وصل
services-disconnected = غیر وصل
services-attached = ضمیم
services-kill = بند کړئ
services-memory = حافظه
services-size = اندازه
services-shell = شیل

error-title = تیروتنه

history-search = تاریخچه لټول
history-clear-all = ټول پاکول
history-clear-confirm = ټوله تاریخچه پاکه کړئ؟
history-clear-warning = دا بیرته نه شي کیدلای.
history-cancel = لغوه کول
history-today = نن
history-yesterday = پرون
history-days-ago = { $count } ورځې مخکې
history-day-offset = ورځ -{ $count }

settings-title = ترتیبات
settings-loading = ترتیبات بار کیږي…
settings-stored = ~/.vmux/settings.ron کې زیرمه شوی
settings-other = نور
settings-software-update = د سافټویر تازه کول
settings-check-updates = تازه کونې وګورئ
settings-check-updates-hint = د پیل پر مهال او هر ساعت اتوماتیک ګوري کله چې اتو-تازه کول فعال وي.
settings-update-unavailable = موجود نه دی
settings-update-unavailable-hint = تازه کونکی پدې جوړونه کې شامل نه دی.
settings-update-checking = کتل کیږي…
settings-update-checking-hint = تازه کونې لټول کیږي…
settings-update-check-again = بیا وګورئ
settings-update-current = Vmux تازه دی.
settings-update-downloading = ډاونلوډ کیږي…
settings-update-downloading-hint = Vmux { $version } ډاونلوډ کیږي…
settings-update-installing = نصبیږي…
settings-update-installing-hint = Vmux { $version } نصبیږي…
settings-update-ready = تازه کول چمتو دي
settings-update-ready-hint = Vmux { $version } چمتو دی. د پلي کولو لپاره بیا پیل کړئ.
settings-update-try-again = بیا هڅه وکړئ
settings-update-failed = د تازه کونو کتل ممکن نه دي.
settings-item = توکی
settings-item-number = توکی { $number }
settings-press-key = یوه کیلي فشار ورکړئ…
settings-saved = خوندي شو
settings-record-key = د نوي کیلي ترکیب ثبتولو لپاره کلیک وکړئ

tray-open-window = کړکۍ خلاصول
tray-close-window = کړکۍ بندول
tray-pause-recording = ثبتول ودروئ
tray-resume-recording = ثبتول بیا پیل کړئ
tray-finish-recording = ثبتول بشپړ کړئ
tray-quit = Vmux وتل

composer-attach-files = فایلونه ضمیم کړئ (/upload)
composer-remove-attachment = ضمیمه لرې کړئ

layout-back = شاته
layout-forward = مخکې
layout-reload = بیا لوډ کول
layout-bookmark-page = دا مخ بکمارک کړئ
layout-remove-bookmark = بکمارک لرې کړئ
layout-pin-page = دا مخ پین کړئ
layout-unpin-page = دا مخ پین لرې کړئ
layout-manage-extensions = توسعې اداره کول
layout-new-stack = نوی سټیک
layout-close-tab = ټب بندول
layout-bookmark = بکمارک
layout-pin = پین
layout-new-tab = نوی ټب
layout-team = ټیم

command-switch-space = ځای بدلول…
command-search-ask = لټول یا پوښتنه…
command-new-tab-placeholder = لټول یا URL ولیکئ، یا ټرمینل وټاکئ…
command-placeholder = URL ولیکئ، ټبونه لټوئ، یا > د امرونو لپاره…
command-composer-placeholder = د امرونو لپاره / یا د میډیا لپاره @ ولیکئ
command-send = لیږل (Enter)
command-terminal = ټرمینل
command-open-terminal = په ټرمینل کې خلاصول
command-stack = سټیک
command-tabs = { $count ->
    [one] ۱ ټب
   *[other] { $count } ټبونه
}
command-prompt = پرامپټ
command-new-tab = نوی ټب
command-search = لټول
command-open-value = "{ $value }" خلاصول
command-search-value = "{ $value }" لټول

schema-appearance = ظاهر
schema-general = عمومي
schema-layout = ترتیب
schema-layout-detail = کړکۍ، پینلونه، د اړخ بار، او د تمرکز کړۍ.
schema-agent = اجنټ
schema-agent-detail = د اجنټ چلند او د وسیلو اجازې.
schema-shortcuts = شارټکټونه
schema-shortcuts-detail = یوازې لوستلو لید. د تړاوونو بدلولو لپاره مستقیم settings.ron سمبال کړئ.
schema-terminal = ټرمینل
schema-browser = براوزر
schema-mode = حالت
schema-mode-detail = د ویب مخونو لپاره د رنګ سکیم. وسیله ستاسو سیسټم تعقیبوي.
schema-device = وسیله
schema-light = رڼا
schema-dark = تیاره
schema-language = ژبه
schema-language-detail = سیسټم، en-US، ja، یا هر BCP 47 ټاګ د ورته ~/.vmux/locales/<tag>.ftl کټالوګ سره وکاروئ.
schema-auto-update = اتو-تازه کول
schema-auto-update-detail = د پیل پر مهال او هر ساعت تازه کونې وګورئ او نصب کړئ.
schema-startup-url = د پیل URL
schema-startup-url-detail = خالي د امر بار پرامپټ خلاصوي.
schema-search-engine = د لټون انجن
schema-search-engine-detail = د پیل او د امر بار څخه د ویب لټونونو لپاره کارول کیږي.
schema-window = کړکۍ
schema-pane = پینل
schema-side-sheet = د اړخ پاڼه
schema-focus-ring = د تمرکز کړۍ
schema-run-placement = د چلولو ځای بدلول اجازه ورکړئ
schema-run-placement-detail = اجنټانو ته اجازه ورکړئ د چلولو پینل حالت، لوری، او لنگر وټاکي.
schema-leader = مشر
schema-leader-detail = د کورډ شارټکټونو لپاره مخکنۍ کیلي.
schema-chord-timeout = د کورډ ختمیدو وخت
schema-chord-timeout-detail = د کورډ مخکینۍ پای ته رسیدو مخکې ملي ثانیې.
schema-bindings = تړاوونه
schema-confirm-close = د بندولو تایید
schema-confirm-close-detail = د روانې پروسې سره د ټرمینل بندولو مخکې پوښتنه.
schema-default-theme = ډیفالټ تیم
schema-default-theme-detail = د تیمونو له لیست څخه د فعال تیم نوم.
