common-open = විවෘත කරන්න
common-close = වසන්න
common-install = ස්ථාපනය කරන්න
common-uninstall = ස්ථාපනය ඉවත් කරන්න
common-update = යාවත්කාලීන කරන්න
common-retry = නැවත උත්සාහ කරන්න
common-refresh = නැවුම් කරන්න
common-remove = ඉවත් කරන්න
common-enable = සක්‍රිය කරන්න
common-disable = අක්‍රිය කරන්න
common-new = නව
common-active = ක්‍රියාකාරී
common-running = ධාවනය වෙමින්
common-done = සම්පූර්ණයි
common-failed = අසාර්ථකයි
common-installed = ස්ථාපිතයි
common-items = { $count ->
    [one] { $count } අයිතමය
   *[other] { $count } අයිතම
}
start-title = ආරම්භය
start-tagline = එක් ඉල්ලීමක්. ඕනෑ දෙයක්, නිමා.

agents-title = නියෝජිතයෝ
agents-search = ACP සහ CLI නියෝජිතයෝ සොයන්න…
agents-empty = ගැලපෙන නියෝජිතයෙකු නැත
agents-empty-detail = නමක්, runtime, හෝ ACP/CLI උත්සාහ කරන්න.
agents-install-failed = ස්ථාපනය අසාර්ථකයි
agents-updating = යාවත්කාලීන කරමින්…
agents-retrying = නැවත උත්සාහ කරමින්…
agents-preparing = සූදානම් කරමින්…

extensions-title = දිගු
extensions-search = ස්ථාපිත හෝ Chrome Web Store සොයන්න…
extensions-relaunch = යෙදීමට නැවත ආරම්භ කරන්න
extensions-empty = දිගු ස්ථාපිත නොවේ
extensions-no-match = ගැලපෙන දිගු නැත
extensions-empty-detail = ඉහත Chrome Web Store සොයා Return ඔබන්න.
extensions-no-match-detail = වෙනත් නමක් හෝ extension ID උත්සාහ කරන්න.
extensions-on = සක්‍රිය
extensions-off = අක්‍රිය
extensions-enable-confirm = { $name } සක්‍රිය කරන්නද?
extensions-enable-permissions = { $name } සක්‍රිය කර ඉඩ දෙන්න:

lsp-title = භාෂා සේවාදායකයෝ
lsp-search = භාෂා සේවාදායකයෝ, linters, formatters සොයන්න…
lsp-loading = නාමාවලිය පූරණය වෙමින්…
lsp-empty = ගැලපෙන භාෂා සේවාදායකයෙකු නැත
lsp-empty-detail = වෙනත් භාෂාවක්, linter, හෝ formatter උත්සාහ කරන්න.
lsp-needs = { $tool } අවශ්‍යයි
lsp-status-available = ලබා ගත හැකිය
lsp-status-on-path = PATH හි ඇත
lsp-status-installing = ස්ථාපනය වෙමින්…
lsp-status-installed = ස්ථාපිතයි
lsp-status-outdated = යාවත්කාලීනය ඇත
lsp-status-running = ධාවනය වෙමින්
lsp-status-failed = අසාර්ථකයි

spaces-title = ස්ථාන
spaces-new-placeholder = නව ස්ථාන නාමය
spaces-empty = ස්ථාන නොමැත
spaces-default-name = ස්ථානය { $number }
spaces-tabs = { $count ->
    [one] tab 1ක්
   *[other] tab { $count }ක්
}
spaces-delete = ස්ථානය මකන්න

team-title = කණ්ඩායම
team-just-you = ඔබ පමණයි මෙම ස්ථානයේ
team-agents = { $count ->
    [one] ඔබ සහ නියෝජිතයෙකු 1ක්
   *[other] ඔබ සහ { $count } නියෝජිතයෝ
}
team-empty = තවම කෙනෙකු නැත
team-you = ඔබ
team-agent = නියෝජිතයා

services-title = පසුබිම් සේවා
services-processes = { $count ->
    [one] ක්‍රියාවලිය 1ක්
   *[other] { $count } ක්‍රියාවලි
}
services-kill-all = සියල්ල අවසන් කරන්න
services-not-running = සේවාව ධාවනය නොවේ
services-start-with = සමඟ ආරම්භ කරන්න:
services-empty = ක්‍රියාකාරී ක්‍රියාවලි නොමැත
services-filter = ක්‍රියාවලි පෙරන්න…
services-no-match = ගැලපෙන ක්‍රියාවලි නොමැත
services-connected = සම්බන්ධිතයි
services-disconnected = විසන්ධිතයි
services-attached = සම්බන්ධිත
services-kill = අවසන් කරන්න
services-memory = මතකය
services-size = ප්‍රමාණය
services-shell = Shell

error-title = දෝෂය

history-search = ඉතිහාසය සොයන්න
history-clear-all = සියල්ල මකන්න
history-clear-confirm = සම්පූර්ණ ඉතිහාසය මකන්නද?
history-clear-warning = මෙය ආපසු ගත නොහැක.
history-cancel = අවලංගු කරන්න
history-today = අද
history-yesterday = ඊයේ
history-days-ago = දින { $count }කට පෙර
history-day-offset = දිනය -{ $count }

settings-title = සැකසීම්
settings-loading = සැකසීම් පූරණය වෙමින්…
settings-stored = ~/.vmux/settings.ron හි ගබඩා කර ඇත
settings-other = වෙනත්
settings-software-update = මෘදුකාංග යාවත්කාලීනය
settings-check-updates = යාවත්කාලීන පරීක්ෂා කරන්න
settings-check-updates-hint = ආරම්භයේදී ස්වයංක්‍රීයව පරීක්ෂා කරන අතර Auto-update සක්‍රිය ව ඇත්නම් සෑම පැයකම.
settings-update-unavailable = නොලැබේ
settings-update-unavailable-hint = Updater මෙම build හි ඇතුළත් නොවේ.
settings-update-checking = පරීක්ෂා කරමින්…
settings-update-checking-hint = යාවත්කාලීන පරීක්ෂා කරමින්…
settings-update-check-again = නැවත පරීක්ෂා කරන්න
settings-update-current = Vmux යාවත්කාලීනයි.
settings-update-downloading = බාගත කරමින්…
settings-update-downloading-hint = Vmux { $version } බාගත කරමින්…
settings-update-installing = ස්ථාපනය කරමින්…
settings-update-installing-hint = Vmux { $version } ස්ථාපනය කරමින්…
settings-update-ready = යාවත්කාලීනය සූදානම්
settings-update-ready-hint = Vmux { $version } සූදානම්. යෙදීමට නැවත ආරම්භ කරන්න.
settings-update-try-again = නැවත උත්සාහ කරන්න
settings-update-failed = යාවත්කාලීන පරීක්ෂා කළ නොහැක.
settings-item = අයිතමය
settings-item-number = අයිතමය { $number }
settings-press-key = යතුරක් ඔබන්න…
settings-saved = සුරකින ලදී
settings-record-key = නව key combo සටහන් කිරීමට ක්ලික් කරන්න

tray-open-window = කවුළුව විවෘත කරන්න
tray-close-window = කවුළුව වසන්න
tray-pause-recording = පටිගත කිරීම නතර කරන්න
tray-resume-recording = පටිගත කිරීම නැවත ආරම්භ කරන්න
tray-finish-recording = පටිගත කිරීම නිම කරන්න
tray-quit = Vmux නවත්වන්න

composer-attach-files = ගොනු අමුණන්න (/upload)
composer-remove-attachment = ඇමුණුම ඉවත් කරන්න

layout-back = පෙරට
layout-forward = ඉදිරියට
layout-reload = නැවත පූරණය
layout-bookmark-page = මෙම පිටුව Bookmark කරන්න
layout-remove-bookmark = Bookmark ඉවත් කරන්න
layout-pin-page = මෙම පිටුව Pin කරන්න
layout-unpin-page = Pin ඉවත් කරන්න
layout-manage-extensions = දිගු කළමනාකරණය
layout-new-stack = නව Stack
layout-close-tab = tab වසන්න
layout-bookmark = Bookmark
layout-pin = Pin
layout-new-tab = නව tab
layout-team = කණ්ඩායම

command-switch-space = ස්ථානය මාරු කරන්න…
command-search-ask = සොයන්න හෝ අසන්න…
command-new-tab-placeholder = සොයන්න හෝ URL ටයිප් කරන්න, හෝ Terminal තෝරන්න…
command-placeholder = URL ටයිප් කරන්න, tabs සොයන්න, හෝ commands සඳහා >…
command-composer-placeholder = commands සඳහා / හෝ media සඳහා @ ටයිප් කරන්න
command-send = යවන්න (Enter)
command-terminal = Terminal
command-open-terminal = Terminal හි විවෘත කරන්න
command-stack = Stack
command-tabs = { $count ->
    [one] tab 1ක්
   *[other] { $count } tabs
}
command-prompt = Prompt
command-new-tab = නව tab
command-search = සොයන්න
command-open-value = "{ $value }" විවෘත කරන්න
command-search-value = "{ $value }" සොයන්න

schema-appearance = පෙනුම
schema-general = සාමාන්‍ය
schema-layout = සැලැස්ම
schema-layout-detail = කවුළුව, panes, sidebar, සහ focus ring.
schema-agent = නියෝජිතයා
schema-agent-detail = නියෝජිත හැසිරීම සහ tool අවසර.
schema-shortcuts = කෙටිමං
schema-shortcuts-detail = කියවීමට පමණි. bindings වෙනස් කිරීමට settings.ron කෙලින් සංස්කරණය කරන්න.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = ප්‍රකාරය
schema-mode-detail = වෙබ් පිටු සඳහා වර්ණ සැලැස්ම. Device ඔබේ system අනුව.
schema-device = Device
schema-light = ආලෝකිත
schema-dark = අඳුරු
schema-language = භාෂාව
schema-language-detail = system, en-US, ja, හෝ ගැලපෙන ~/.vmux/locales/<tag>.ftl නාමාවලියක් සහිත BCP 47 tag භාවිත කරන්න.
schema-auto-update = ස්වයං-යාවත්කාලීනය
schema-auto-update-detail = ආරම්භයේදී සහ සෑම පැයකම යාවත්කාලීන පරීක්ෂා කර ස්ථාපනය කරන්න.
schema-startup-url = ආරම්භක URL
schema-startup-url-detail = හිස් නම් command bar prompt විවෘත වේ.
schema-search-engine = සෙවුම් එන්ජිම
schema-search-engine-detail = ආරම්භය සහ command bar හි වෙබ් සෙවුම් සඳහා භාවිත කෙරේ.
schema-window = කවුළුව
schema-pane = Pane
schema-side-sheet = Side sheet
schema-focus-ring = Focus ring
schema-run-placement = Run placement override ඉඩ දෙන්න
schema-run-placement-detail = නියෝජිතයන්ට run pane mode, direction, සහ anchor තෝරාගැනීමට ඉඩ දෙන්න.
schema-leader = Leader
schema-leader-detail = chord shortcuts සඳහා prefix key.
schema-chord-timeout = Chord timeout
schema-chord-timeout-detail = chord prefix කල් ඉකුත් වීමට පෙර milliseconds.
schema-bindings = Bindings
schema-confirm-close = වසීමට තහවුරු කරන්න
schema-confirm-close-detail = ධාවනය වෙමින් ඇති ක්‍රියාවලියක් සහිත terminal වසීමට පෙර ඇසීම.
schema-default-theme = පෙරනිමි theme
schema-default-theme-detail = themes ලැයිස්තුවෙන් ක්‍රියාකාරී theme නාමය.
