common-open = খোলা
common-close = বন্ধ
common-install = ইনস্টল করুন
common-uninstall = আনইনস্টল করুন
common-update = আপডেট
common-retry = আবার চেষ্টা করুন
common-refresh = রিফ্রেশ
common-remove = সরান
common-enable = সক্ষম করুন
common-disable = নিষ্ক্রিয় করুন
common-new = নতুন
common-active = সক্রিয়
common-running = চলমান
common-done = সম্পন্ন
common-failed = ব্যর্থ হয়েছে
common-installed = ইনস্টল করা হয়েছে
common-items = { $count ->
    [one] { $count } আইটেম
   *[other] { $count } আইটেম
}
start-title = শুরু করুন
start-tagline = এক প্রম্পট. কিছু, সম্পন্ন.

agents-title = এজেন্ট
agents-search = ACP এবং CLI এজেন্ট অনুসন্ধান করুন...
agents-empty = কোন মিল এজেন্ট
agents-empty-detail = একটি নাম, রানটাইম বা ACP/CLI চেষ্টা করুন৷
agents-install-failed = ইনস্টল ব্যর্থ হয়েছে
agents-updating = আপডেট হচ্ছে...
agents-retrying = আবার চেষ্টা করা হচ্ছে...
agents-preparing = প্রস্তুত হচ্ছে...

extensions-title = এক্সটেনশন
extensions-search = অনুসন্ধান ইনস্টল করা বা Chrome Web Store…
extensions-relaunch = আবেদন করতে পুনরায় লঞ্চ করুন
extensions-empty = কোন এক্সটেনশন ইনস্টল করা নেই
extensions-no-match = কোন মিল এক্সটেনশন নেই
extensions-empty-detail = উপরের Chrome Web Store অনুসন্ধান করুন এবং Return টিপুন।
extensions-no-match-detail = অন্য নাম বা এক্সটেনশন আইডি চেষ্টা করুন.
extensions-on = চালু
extensions-off = বন্ধ
extensions-enable-confirm = { $name } সক্ষম করবেন?
extensions-enable-permissions = { $name } সক্ষম করুন এবং অনুমতি দিন:

lsp-title = ভাষা সার্ভার
lsp-search = ভাষা সার্ভার, লিন্টার, ফরম্যাটার অনুসন্ধান করুন...
lsp-loading = ক্যাটালগ লোড হচ্ছে...
lsp-empty = কোন মিলিত ভাষা সার্ভার
lsp-empty-detail = অন্য ভাষা, লিন্টার বা ফর্ম্যাটার চেষ্টা করুন।
lsp-needs = প্রয়োজন { $tool }
lsp-status-available = পাওয়া যায়
lsp-status-on-path = PATH এ
lsp-status-installing = ইনস্টল করা হচ্ছে...
lsp-status-installed = ইনস্টল করা হয়েছে
lsp-status-outdated = আপডেট উপলব্ধ
lsp-status-running = চলমান
lsp-status-failed = ব্যর্থ হয়েছে

spaces-title = স্পেস
spaces-new-placeholder = নতুন স্থানের নাম
spaces-empty = কোনো স্পেস নেই
spaces-default-name = স্থান { $number }
spaces-tabs = { $count ->
    [one] 1 ট্যাব
   *[other] { $count } ট্যাব
}
spaces-delete = স্থান মুছুন

team-title = দল
team-just-you = এই জায়গায় শুধু তুমি
team-agents = { $count ->
    [one] আপনি এবং 1 এজেন্ট
   *[other] আপনি এবং { $count } এজেন্ট
}
team-empty = এখানে এখনো কেউ নেই
team-you = আপনি
team-agent = এজেন্ট

services-title = পটভূমি সেবা
services-processes = { $count ->
    [one] 1 প্রক্রিয়া
   *[other] { $count } প্রক্রিয়া
}
services-kill-all = সকলকে হত্যা কর
services-not-running = পরিষেবা চলছে না
services-start-with = দিয়ে শুরু করুন:
services-empty = কোনো সক্রিয় প্রক্রিয়া নেই
services-filter = ফিল্টার প্রক্রিয়া...
services-no-match = কোন মিল প্রক্রিয়া
services-connected = সংযুক্ত
services-disconnected = সংযোগ বিচ্ছিন্ন
services-attached = সংযুক্ত
services-kill = হত্যা
services-memory = স্মৃতি
services-size = আকার
services-shell = শেল

error-title = ত্রুটি

history-search = অনুসন্ধান ইতিহাস
history-clear-all = সব সাফ করুন
history-clear-confirm = সমস্ত ইতিহাস সাফ করবেন?
history-clear-warning = এটি পূর্বাবস্থায় ফেরানো যাবে না।
history-cancel = বাতিল করুন
history-today = আজ
history-yesterday = গতকাল
history-days-ago = { $count } দিন আগে
history-day-offset = দিন -{ $count }

settings-title = সেটিংস
settings-loading = সেটিংস লোড হচ্ছে...
settings-stored = ~/.vmux/settings.ron এ সংরক্ষিত
settings-other = অন্যান্য
settings-software-update = সফটওয়্যার আপডেট
settings-check-updates = আপডেটের জন্য চেক করুন
settings-check-updates-hint = লঞ্চের সময় স্বয়ংক্রিয়ভাবে চেক করে এবং স্বয়ংক্রিয়-আপডেট সক্ষম হলে প্রতি ঘন্টায়।
settings-update-unavailable = অনুপলব্ধ
settings-update-unavailable-hint = আপডেটার এই বিল্ডে অন্তর্ভুক্ত নয়।
settings-update-checking = পরীক্ষা করা হচ্ছে...
settings-update-checking-hint = আপডেটের জন্য পরীক্ষা করা হচ্ছে...
settings-update-check-again = আবার চেক করুন
settings-update-current = Vmux আপ টু ডেট।
settings-update-downloading = ডাউনলোড হচ্ছে...
settings-update-downloading-hint = ডাউনলোড হচ্ছে Vmux { $version }…
settings-update-installing = ইনস্টল করা হচ্ছে...
settings-update-installing-hint = Vmux { $version } ইনস্টল করা হচ্ছে...
settings-update-ready = আপডেট প্রস্তুত
settings-update-ready-hint = Vmux { $version } প্রস্তুত। এটি প্রয়োগ করতে পুনরায় চালু করুন।
settings-update-try-again = আবার চেষ্টা করুন
settings-update-failed = আপডেটের জন্য চেক করতে অক্ষম.
settings-item = আইটেম
settings-item-number = আইটেম { $number }
settings-press-key = একটি কী টিপুন...
settings-saved = সংরক্ষিত
settings-record-key = একটি নতুন কী কম্বো রেকর্ড করতে ক্লিক করুন

tray-open-window = উইন্ডো খুলুন
tray-close-window = উইন্ডো বন্ধ করুন
tray-pause-recording = রেকর্ডিং বিরতি
tray-resume-recording = রেকর্ডিং পুনরায় শুরু করুন
tray-finish-recording = রেকর্ডিং শেষ করুন
tray-quit = প্রস্থান করুন Vmux

composer-attach-files = ফাইল সংযুক্ত করুন (/upload)
composer-remove-attachment = সংযুক্তি সরান

layout-back = ব্যাক
layout-forward = ফরোয়ার্ড
layout-reload = পুনরায় লোড করুন
layout-bookmark-page = এই পৃষ্ঠাটি বুকমার্ক করুন
layout-remove-bookmark = বুকমার্ক সরান
layout-pin-page = এই পৃষ্ঠাটি পিন করুন
layout-unpin-page = এই পৃষ্ঠাটি আনপিন করুন
layout-manage-extensions = এক্সটেনশন পরিচালনা করুন
layout-new-stack = নতুন স্ট্যাক
layout-close-tab = ট্যাব বন্ধ করুন
layout-bookmark = বুকমার্ক
layout-pin = পিন
layout-new-tab = নতুন ট্যাব
layout-team = দল

command-switch-space = স্থান পরিবর্তন করুন...
command-search-ask = অনুসন্ধান করুন বা জিজ্ঞাসা করুন...
command-new-tab-placeholder = একটি URL অনুসন্ধান করুন বা টাইপ করুন, অথবা টার্মিনাল নির্বাচন করুন...
command-placeholder = কমান্ডের জন্য একটি URL, অনুসন্ধান ট্যাব বা > টাইপ করুন...
command-composer-placeholder = টাইপ করুন / কমান্ডের জন্য বা মিডিয়ার জন্য @
command-send = পাঠান (Enter)
command-terminal = টার্মিনাল
command-open-terminal = টার্মিনালে খুলুন
command-stack = স্ট্যাক
command-tabs = { $count ->
    [one] 1 ট্যাব
   *[other] { $count } ট্যাব
}
command-prompt = প্রম্পট
command-new-tab = নতুন ট্যাব
command-search = অনুসন্ধান করুন
command-open-value = "{ $value }" খুলুন
command-search-value = "{ $value }" অনুসন্ধান করুন

schema-appearance = চেহারা
schema-general = সাধারণ
schema-layout = লেআউট
schema-layout-detail = উইন্ডো, প্যান, সাইডবার এবং ফোকাস রিং।
schema-agent = এজেন্ট
schema-agent-detail = এজেন্ট আচরণ এবং টুল অনুমতি.
schema-shortcuts = শর্টকাট
schema-shortcuts-detail = শুধুমাত্র-পঠন দৃশ্য। বাইন্ডিং পরিবর্তন করতে সরাসরি settings.ron সম্পাদনা করুন।
schema-terminal = টার্মিনাল
schema-browser = ব্রাউজার
schema-mode = মোড
schema-mode-detail = ওয়েব পৃষ্ঠাগুলির জন্য রঙের স্কিম। ডিভাইস আপনার সিস্টেম অনুসরণ করে.
schema-device = ডিভাইস
schema-light = আলো
schema-dark = অন্ধকার
schema-language = ভাষা
schema-language-detail = সিস্টেম, en-US, ja, বা যেকোনো BCP 47 ট্যাগ একটি মিলে যাওয়া ~/.vmux/locales/<tag>.ftl ক্যাটালগ ব্যবহার করুন।
schema-auto-update = স্বয়ংক্রিয় আপডেট
schema-auto-update-detail = লঞ্চ এবং প্রতি ঘন্টায় আপডেটগুলি পরীক্ষা করুন এবং ইনস্টল করুন৷
schema-startup-url = স্টার্টআপ URL
schema-startup-url-detail = খালি কমান্ড বার প্রম্পট খোলে।
schema-search-engine = সার্চ ইঞ্জিন
schema-search-engine-detail = স্টার্ট এবং কমান্ড বার থেকে ওয়েব অনুসন্ধানের জন্য ব্যবহৃত হয়।
schema-window = জানালা
schema-pane = ফলক
schema-side-sheet = সাইড শীট
schema-focus-ring = ফোকাস রিং
schema-run-placement = রান প্লেসমেন্ট ওভাররাইডের অনুমতি দিন
schema-run-placement-detail = এজেন্টদের রান প্যান মোড, দিকনির্দেশ এবং অ্যাঙ্কর বেছে নিতে দিন।
schema-leader = নেতা
schema-leader-detail = জ্যা শর্টকাটের জন্য উপসর্গ কী।
schema-chord-timeout = জ্যা টাইমআউট
schema-chord-timeout-detail = একটি জ্যা প্রিফিক্সের মেয়াদ শেষ হওয়ার আগে মিলিসেকেন্ড।
schema-bindings = বাঁধাই
schema-confirm-close = বন্ধ নিশ্চিত করুন
schema-confirm-close-detail = চলমান প্রক্রিয়া সহ একটি টার্মিনাল বন্ধ করার আগে প্রম্পট করুন।
schema-default-theme = ডিফল্ট থিম
schema-default-theme-detail = থিম তালিকা থেকে সক্রিয় থিমের নাম।
