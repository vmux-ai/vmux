common-open = ဖွင့်မည်
common-close = ပိတ်မည်
common-install = ထည့်သွင်းမည်
common-uninstall = ဖယ်ရှားမည်
common-update = အပ်ဒိတ်လုပ်မည်
common-retry = ထပ်ကြိုးစားမည်
common-refresh = ပြန်လည်တင်မည်
common-remove = ဖယ်မည်
common-enable = ဖွင့်မည်
common-disable = ပိတ်မည်
common-new = အသစ်
common-active = အသုံးပြုနေ
common-running = လည်ပတ်နေ
common-done = ပြီးပါပြီ
common-failed = မအောင်မြင်ပါ
common-installed = ထည့်သွင်းပြီး
common-items = { $count ->
    [one] { $count } ခု
   *[other] { $count } ခု
}
start-title = စတင်ရန်
start-tagline = prompt တစ်ခုတည်းနဲ့ ဘာမဆို ပြီးစီးစေပါ။

agents-title = Agent များ
agents-search = ACP နှင့် CLI agent များကို ရှာရန်…
agents-empty = ကိုက်ညီသည့် agent မရှိပါ
agents-empty-detail = အမည်၊ runtime သို့မဟုတ် ACP/CLI ဖြင့် ရှာကြည့်ပါ။
agents-install-failed = ထည့်သွင်းမှု မအောင်မြင်ပါ
agents-updating = အပ်ဒိတ်လုပ်နေသည်…
agents-retrying = ထပ်ကြိုးစားနေသည်…
agents-preparing = ပြင်ဆင်နေသည်…

extensions-title = Extension များ
extensions-search = ထည့်သွင်းထားပြီးသား သို့မဟုတ် Chrome Web Store တွင် ရှာရန်…
extensions-relaunch = အကျိုးသက်ရောက်စေရန် ပြန်ဖွင့်ပါ
extensions-empty = Extension မထည့်သွင်းထားပါ
extensions-no-match = ကိုက်ညီသည့် extension မရှိပါ
extensions-empty-detail = အပေါ်ရှိ Chrome Web Store တွင် ရှာပြီး Return ကို နှိပ်ပါ။
extensions-no-match-detail = အခြားအမည် သို့မဟုတ် extension ID ဖြင့် စမ်းကြည့်ပါ။
extensions-on = ဖွင့်ထား
extensions-off = ပိတ်ထား
extensions-enable-confirm = { $name } ကို ဖွင့်မလား။
extensions-enable-permissions = { $name } ကို ဖွင့်ပြီး အောက်ပါတို့ကို ခွင့်ပြုပါ-

lsp-title = Language Server များ
lsp-search = language server၊ linter၊ formatter များကို ရှာရန်…
lsp-loading = catalog ကို တင်နေသည်…
lsp-empty = ကိုက်ညီသည့် language server မရှိပါ
lsp-empty-detail = အခြားဘာသာစကား၊ linter သို့မဟုတ် formatter ဖြင့် စမ်းကြည့်ပါ။
lsp-needs = { $tool } လိုအပ်သည်
lsp-status-available = ရနိုင်သည်
lsp-status-on-path = PATH တွင် ရှိသည်
lsp-status-installing = ထည့်သွင်းနေသည်…
lsp-status-installed = ထည့်သွင်းပြီး
lsp-status-outdated = အပ်ဒိတ် ရနိုင်သည်
lsp-status-running = လည်ပတ်နေသည်
lsp-status-failed = မအောင်မြင်ပါ

spaces-title = Space များ
spaces-new-placeholder = Space အသစ်အမည်
spaces-empty = Space မရှိပါ
spaces-default-name = Space { $number }
spaces-tabs = { $count ->
    [one] တက်ဘ် 1 ခု
   *[other] တက်ဘ် { $count } ခု
}
spaces-delete = Space ဖျက်မည်

team-title = အဖွဲ့
team-just-you = ဤ space တွင် သင်တစ်ဦးတည်းရှိသည်
team-agents = { $count ->
    [one] သင်နှင့် agent 1 ခု
   *[other] သင်နှင့် agent { $count } ခု
}
team-empty = ဒီနေရာမှာ မည်သူမျှ မရှိသေးပါ
team-you = သင်
team-agent = Agent

services-title = နောက်ခံဝန်ဆောင်မှုများ
services-processes = { $count ->
    [one] process 1 ခု
   *[other] process { $count } ခု
}
services-kill-all = အားလုံး ရပ်တန့်မည်
services-not-running = ဝန်ဆောင်မှု မလည်ပတ်နေပါ
services-start-with = စတင်ပုံ-
services-empty = လည်ပတ်နေသော process မရှိပါ
services-filter = process များကို စစ်ထုတ်ရန်…
services-no-match = ကိုက်ညီသည့် process မရှိပါ
services-connected = ချိတ်ဆက်ထားသည်
services-disconnected = ချိတ်ဆက်မှု ပြတ်ထားသည်
services-attached = ပူးတွဲထားသည်
services-kill = ရပ်တန့်မည်
services-memory = မမ်မိုရီ
services-size = အရွယ်အစား
services-shell = Shell

error-title = အမှား

history-search = မှတ်တမ်း ရှာရန်
history-clear-all = အားလုံးရှင်းမည်
history-clear-confirm = မှတ်တမ်းအားလုံး ရှင်းမလား။
history-clear-warning = ဤလုပ်ဆောင်ချက်ကို ပြန်ဖျက်၍မရပါ။
history-cancel = မလုပ်တော့ပါ
history-today = ယနေ့
history-yesterday = မနေ့က
history-days-ago = လွန်ခဲ့သော { $count } ရက်
history-day-offset = ရက် -{ $count }

settings-title = ဆက်တင်များ
settings-loading = ဆက်တင်များ တင်နေသည်…
settings-stored = ~/.vmux/settings.ron တွင် သိမ်းထားသည်
settings-other = အခြား
settings-software-update = ဆော့ဖ်ဝဲအပ်ဒိတ်
settings-check-updates = အပ်ဒိတ် စစ်ဆေးမည်
settings-check-updates-hint = Auto-update ဖွင့်ထားလျှင် စတင်ချိန်နှင့် နာရီတိုင်း အလိုအလျောက် စစ်ဆေးသည်။
settings-update-unavailable = မရနိုင်ပါ
settings-update-unavailable-hint = ဤ build တွင် updater မပါဝင်ပါ။
settings-update-checking = စစ်ဆေးနေသည်…
settings-update-checking-hint = အပ်ဒိတ်များကို စစ်ဆေးနေသည်…
settings-update-check-again = ထပ်စစ်ဆေးမည်
settings-update-current = Vmux သည် နောက်ဆုံးဗားရှင်းဖြစ်သည်။
settings-update-downloading = ဒေါင်းလုဒ်လုပ်နေသည်…
settings-update-downloading-hint = Vmux { $version } ကို ဒေါင်းလုဒ်လုပ်နေသည်…
settings-update-installing = ထည့်သွင်းနေသည်…
settings-update-installing-hint = Vmux { $version } ကို ထည့်သွင်းနေသည်…
settings-update-ready = အပ်ဒိတ် အသင့်ဖြစ်ပြီ
settings-update-ready-hint = Vmux { $version } အသင့်ဖြစ်ပါပြီ။ အသုံးပြုရန် ပြန်လည်စတင်ပါ။
settings-update-try-again = ထပ်ကြိုးစားမည်
settings-update-failed = အပ်ဒိတ်များကို စစ်ဆေး၍မရပါ။
settings-item = အရာ
settings-item-number = အရာ { $number }
settings-press-key = ခလုတ်တစ်ခု နှိပ်ပါ…
settings-saved = သိမ်းပြီး
settings-record-key = key combo အသစ် မှတ်တမ်းတင်ရန် နှိပ်ပါ

tray-open-window = ဝင်းဒိုး ဖွင့်မည်
tray-close-window = ဝင်းဒိုး ပိတ်မည်
tray-pause-recording = မှတ်တမ်းတင်မှု ခဏရပ်မည်
tray-resume-recording = မှတ်တမ်းတင်မှု ပြန်စမည်
tray-finish-recording = မှတ်တမ်းတင်မှု ပြီးစီးမည်
tray-quit = Vmux မှ ထွက်မည်

composer-attach-files = ဖိုင်များ ပူးတွဲမည် (/upload)
composer-remove-attachment = ပူးတွဲဖိုင် ဖယ်မည်

layout-back = နောက်သို့
layout-forward = ရှေ့သို့
layout-reload = ပြန်လည်တင်မည်
layout-bookmark-page = ဤစာမျက်နှာကို bookmark လုပ်မည်
layout-remove-bookmark = bookmark ဖယ်မည်
layout-pin-page = ဤစာမျက်နှာကို pin လုပ်မည်
layout-unpin-page = ဤစာမျက်နှာကို unpin လုပ်မည်
layout-manage-extensions = extension များ စီမံမည်
layout-new-stack = Stack အသစ်
layout-close-tab = တက်ဘ် ပိတ်မည်
layout-bookmark = Bookmark
layout-pin = Pin
layout-new-tab = တက်ဘ်အသစ်
layout-team = အဖွဲ့

command-switch-space = Space ပြောင်းရန်…
command-search-ask = ရှာရန် သို့မဟုတ် မေးရန်…
command-new-tab-placeholder = ရှာရန်၊ URL ရိုက်ရန် သို့မဟုတ် Terminal ရွေးရန်…
command-placeholder = URL ရိုက်ပါ၊ တက်ဘ်များ ရှာပါ၊ သို့မဟုတ် command အတွက် > ရိုက်ပါ…
command-composer-placeholder = command များအတွက် / သို့မဟုတ် media အတွက် @ ရိုက်ပါ
command-send = ပို့မည် (Enter)
command-terminal = Terminal
command-open-terminal = Terminal တွင် ဖွင့်မည်
command-stack = Stack
command-tabs = { $count ->
    [one] တက်ဘ် 1 ခု
   *[other] တက်ဘ် { $count } ခု
}
command-prompt = Prompt
command-new-tab = တက်ဘ်အသစ်
command-search = ရှာရန်
command-open-value = “{ $value }” ကို ဖွင့်မည်
command-search-value = “{ $value }” ကို ရှာမည်

schema-appearance = အသွင်အပြင်
schema-general = အထွေထွေ
schema-layout = အပြင်အဆင်
schema-layout-detail = ဝင်းဒိုး၊ pane များ၊ sidebar နှင့် focus ring။
schema-agent = Agent
schema-agent-detail = Agent အပြုအမူနှင့် tool ခွင့်ပြုချက်များ။
schema-shortcuts = Shortcut များ
schema-shortcuts-detail = ဖတ်ရန်သာ မြင်ကွင်း။ binding ပြောင်းရန် settings.ron ကို တိုက်ရိုက် တည်းဖြတ်ပါ။
schema-terminal = Terminal
schema-browser = Browser
schema-mode = မုဒ်
schema-mode-detail = ဝဘ်စာမျက်နှာများအတွက် အရောင်စနစ်။ Device သည် သင့်စနစ်အတိုင်း လိုက်နာသည်။
schema-device = Device
schema-light = အလင်း
schema-dark = အမှောင်
schema-language = ဘာသာစကား
schema-language-detail = စနစ်၊ en-US၊ ja သို့မဟုတ် ကိုက်ညီသည့် ~/.vmux/locales/<tag>.ftl catalog ပါသော BCP 47 tag မည်သည့်တစ်ခုမဆို သုံးပါ။
schema-auto-update = Auto-update
schema-auto-update-detail = စတင်ချိန်နှင့် နာရီတိုင်း အပ်ဒိတ် စစ်ဆေးပြီး ထည့်သွင်းပါ။
schema-startup-url = စတင် URL
schema-startup-url-detail = အလွတ်ထားပါက command bar prompt ကို ဖွင့်သည်။
schema-search-engine = ရှာဖွေရေးအင်ဂျင်
schema-search-engine-detail = Start နှင့် command bar မှ ဝဘ်ရှာဖွေမှုများအတွက် အသုံးပြုသည်။
schema-window = ဝင်းဒိုး
schema-pane = Pane
schema-side-sheet = ဘေးစာရွက်
schema-focus-ring = Focus ring
schema-run-placement = run placement override ခွင့်ပြုမည်
schema-run-placement-detail = agent များအား run pane mode၊ direction နှင့် anchor ရွေးချယ်ခွင့်ပေးပါ။
schema-leader = Leader
schema-leader-detail = chord shortcut များအတွက် prefix key။
schema-chord-timeout = Chord timeout
schema-chord-timeout-detail = chord prefix သက်တမ်းကုန်မီ မီလီစက္ကန့်။
schema-bindings = Binding များ
schema-confirm-close = ပိတ်ချိန် အတည်ပြုရန်
schema-confirm-close-detail = လည်ပတ်နေသော process ရှိသည့် terminal ကို ပိတ်မီ မေးပါ။
schema-default-theme = မူရင်း theme
schema-default-theme-detail = theme စာရင်းထဲမှ အသုံးပြုနေသော theme အမည်။
