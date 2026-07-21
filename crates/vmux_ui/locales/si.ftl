locale-name = සිංහල
common-open = විවෘත කරන්න
common-close = වසන්න
common-install = ස්ථාපනය කරන්න
common-uninstall = අස්ථාපනය කරන්න
common-update = යාවත්කාලීන කරන්න
common-retry = නැවත උත්සාහ කරන්න
common-refresh = නැවුම් කරන්න
common-remove = ඉවත් කරන්න
common-enable = සක්‍රීය කරන්න
common-disable = අක්‍රීය කරන්න
common-new = නව
common-active = සක්‍රීයයි
common-running = ධාවනය වේ
common-done = අවසන්
common-failed = අසාර්ථකයි
common-installed = ස්ථාපිතයි
common-items = { $count ->
    [one] අයිතම { $count }
   *[other] අයිතම { $count }
}
start-title = ආරම්භය
start-tagline = එක් prompt එකක්. ඕනෑම දෙයක්, කරලා ඉවරයි.

agents-title = ඒජන්ට්
agents-search = ACP සහ CLI ඒජන්ට් සොයන්න…
agents-empty = ගැලපෙන ඒජන්ට් නැත
agents-empty-detail = නමක්, runtime එකක්, හෝ ACP/CLI උත්සාහ කරන්න.
agents-install-failed = ස්ථාපනය අසාර්ථකයි
agents-updating = යාවත්කාලීන වෙමින්…
agents-retrying = නැවත උත්සාහ කරමින්…
agents-preparing = සූදානම් කරමින්…

extensions-title = Extensions
extensions-search = ස්ථාපිත ඒවා හෝ Chrome Web Store සොයන්න…
extensions-relaunch = ක්‍රියාත්මක කිරීමට නැවත ආරම්භ කරන්න
extensions-empty = Extensions ස්ථාපනය කර නැත
extensions-no-match = ගැලපෙන extensions නැත
extensions-empty-detail = ඉහළින් Chrome Web Store සොයා Return ඔබන්න.
extensions-no-match-detail = වෙනත් නමක් හෝ extension ID එකක් උත්සාහ කරන්න.
extensions-on = සක්‍රීයයි
extensions-off = අක්‍රීයයි
extensions-enable-confirm = { $name } සක්‍රීය කරන්නද?
extensions-enable-permissions = { $name } සක්‍රීය කර මෙයට අවසර දෙන්න:

lsp-title = භාෂා සර්වර්
lsp-search = භාෂා සර්වර්, linters, formatters සොයන්න…
lsp-loading = නාමාවලිය පූරණය වෙමින්…
lsp-empty = ගැලපෙන භාෂා සර්වර් නැත
lsp-empty-detail = වෙනත් භාෂාවක්, linter එකක්, හෝ formatter එකක් උත්සාහ කරන්න.
lsp-needs = { $tool } අවශ්‍යයි
lsp-status-available = ලබාගත හැක
lsp-status-on-path = PATH මත ඇත
lsp-status-installing = ස්ථාපනය වෙමින්…
lsp-status-installed = ස්ථාපිතයි
lsp-status-outdated = යාවත්කාලීනයක් ඇත
lsp-status-running = ධාවනය වේ
lsp-status-failed = අසාර්ථකයි

spaces-title = වැඩඅවකාශ
spaces-new-placeholder = නව වැඩඅවකාශ නම
spaces-empty = වැඩඅවකාශ නැත
spaces-default-name = වැඩඅවකාශය { $number }
spaces-tabs = { $count ->
    [one] ටැබ් 1
   *[other] ටැබ් { $count }
}
spaces-delete = වැඩඅවකාශය මකන්න

team-title = කණ්ඩායම
team-just-you = මේ වැඩඅවකාශයේ ඔබ පමණයි
team-agents = { $count ->
    [one] ඔබ සහ ඒජන්ට් 1
   *[other] ඔබ සහ ඒජන්ට් { $count }
}
team-empty = තවම කිසිවෙකු නැත
team-you = ඔබ
team-agent = ඒජන්ට්

services-title = පසුබිම් සේවා
services-processes = { $count ->
    [one] ක්‍රියාවලිය 1
   *[other] ක්‍රියාවලි { $count }
}
services-kill-all = සියල්ල බලෙන් නවත්වන්න
services-not-running = සේවාව ධාවනය නොවේ
services-start-with = මෙයින් ආරම්භ කරන්න:
services-empty = සක්‍රීය ක්‍රියාවලි නැත
services-filter = ක්‍රියාවලි පෙරන්න…
services-no-match = ගැලපෙන ක්‍රියාවලි නැත
services-connected = සම්බන්ධයි
services-disconnected = සම්බන්ධතාව විසන්ධියි
services-attached = අමුණා ඇත
services-kill = බලෙන් නවත්වන්න
services-memory = මතකය
services-size = ප්‍රමාණය
services-shell = Shell

error-title = දෝෂය

history-search = ඉතිහාසය සොයන්න
history-clear-all = සියල්ල මකන්න
history-clear-confirm = සියලු ඉතිහාසය මකන්නද?
history-clear-warning = මෙය ආපසු හැරවිය නොහැක.
history-cancel = අවලංගු කරන්න
history-today = අද
history-yesterday = ඊයේ
history-days-ago = දින { $count }කට පෙර
history-day-offset = දිනය -{ $count }

settings-title = සැකසුම්
settings-loading = සැකසුම් පූරණය වෙමින්…
settings-stored = ~/.vmux/settings.ron හි සුරක්ෂිත කර ඇත
settings-other = වෙනත්
settings-software-update = මෘදුකාංග යාවත්කාලීන
settings-check-updates = යාවත්කාලීන පරීක්ෂා කරන්න
settings-check-updates-hint = Auto-update සක්‍රීය නම් ආරම්භයේදී සහ පැයකට වරක් ස්වයංක්‍රීයව පරීක්ෂා කරයි.
settings-update-unavailable = ලබාගත නොහැක
settings-update-unavailable-hint = මෙම build එකට updater ඇතුළත් නැත.
settings-update-checking = පරීක්ෂා කරමින්…
settings-update-checking-hint = යාවත්කාලීන පරීක්ෂා කරමින්…
settings-update-check-again = නැවත පරීක්ෂා කරන්න
settings-update-current = Vmux යාවත්කාලීනයි.
settings-update-downloading = බාගනිමින්…
settings-update-downloading-hint = Vmux { $version } බාගනිමින්…
settings-update-installing = ස්ථාපනය වෙමින්…
settings-update-installing-hint = Vmux { $version } ස්ථාපනය වෙමින්…
settings-update-ready = යාවත්කාලීනය සූදානම්
settings-update-ready-hint = Vmux { $version } සූදානම්. ක්‍රියාත්මක කිරීමට නැවත ආරම්භ කරන්න.
settings-update-try-again = නැවත උත්සාහ කරන්න
settings-update-failed = යාවත්කාලීන පරීක්ෂා කළ නොහැක.
settings-item = අයිතමය
settings-item-number = අයිතමය { $number }
settings-press-key = යතුරක් ඔබන්න…
settings-saved = සුරැකිණි
settings-record-key = නව යතුරු සංයෝජනයක් සටහන් කිරීමට ක්ලික් කරන්න

tray-open-window = කවුළුව විවෘත කරන්න
tray-close-window = කවුළුව වසන්න
tray-pause-recording = පටිගත කිරීම විරාම කරන්න
tray-resume-recording = පටිගත කිරීම නැවත ආරම්භ කරන්න
tray-finish-recording = පටිගත කිරීම අවසන් කරන්න
tray-quit = Vmux ඉවත් වන්න

composer-attach-files = ගොනු අමුණන්න (/upload)
composer-remove-attachment = ඇමුණුම ඉවත් කරන්න

layout-back = ආපසු
layout-forward = ඉදිරියට
layout-reload = නැවත පූරණය කරන්න
layout-bookmark-page = මෙම පිටුව bookmark කරන්න
layout-remove-bookmark = bookmark ඉවත් කරන්න
layout-pin-page = මෙම පිටුව pin කරන්න
layout-unpin-page = මෙම පිටුව unpin කරන්න
layout-manage-extensions = extensions කළමනාකරණය කරන්න
layout-new-stack = නව ස්ටැක්
layout-close-tab = ටැබ් වසන්න
layout-bookmark = Bookmark
layout-pin = Pin
layout-new-tab = නව ටැබ්
layout-team = කණ්ඩායම

command-switch-space = වැඩඅවකාශය මාරු කරන්න…
command-search-ask = සොයන්න හෝ අසන්න…
command-new-tab-placeholder = සොයන්න, URL එකක් ටයිප් කරන්න, හෝ Terminal තෝරන්න…
command-placeholder = URL එකක් ටයිප් කරන්න, ටැබ් සොයන්න, හෝ විධාන සඳහා > ටයිප් කරන්න…
command-composer-placeholder = විධාන සඳහා / හෝ මාධ්‍ය සඳහා @ ටයිප් කරන්න
command-send = යවන්න (Enter)
command-terminal = Terminal
command-open-terminal = Terminal තුළ විවෘත කරන්න
command-stack = ස්ටැක්
command-tabs = { $count ->
    [one] ටැබ් 1
   *[other] ටැබ් { $count }
}
command-prompt = Prompt
command-new-tab = නව ටැබ්
command-search = සොයන්න
command-open-value = “{ $value }” විවෘත කරන්න
command-search-value = “{ $value }” සොයන්න

schema-appearance = පෙනුම
schema-general = සාමාන්‍ය
schema-layout = පිරිසැලසුම
schema-layout-detail = කවුළුව, පැනල, පැති තීරුව, සහ focus ring.
schema-agent = ඒජන්ට්
schema-agent-detail = ඒජන්ට් හැසිරීම සහ මෙවලම් අවසර.
schema-shortcuts = කෙටිමං
schema-shortcuts-detail = කියවීමට පමණයි. bindings වෙනස් කිරීමට settings.ron සෘජුව සංස්කරණය කරන්න.
schema-terminal = Terminal
schema-browser = බ්‍රවුසරය
schema-mode = ප්‍රකාරය
schema-mode-detail = වෙබ් පිටු සඳහා වර්ණ සැලසුම. Device ඔබේ පද්ධතිය අනුගමනය කරයි.
schema-device = උපාංගය
schema-light = ආලෝක
schema-dark = අඳුරු
schema-language = භාෂාව
schema-language-detail = පද්ධතිය, en-US, ja, හෝ ගැලපෙන ~/.vmux/locales/<tag>.ftl නාමාවලියක් ඇති ඕනෑම BCP 47 tag එකක් භාවිත කරන්න.
schema-auto-update = ස්වයංක්‍රීය යාවත්කාලීන
schema-auto-update-detail = ආරම්භයේදී සහ පැයකට වරක් යාවත්කාලීන පරීක්ෂා කර ස්ථාපනය කරන්න.
schema-startup-url = ආරම්භක URL
schema-startup-url-detail = හිස් නම් command bar prompt එක විවෘත වේ.
schema-search-engine = සෙවුම් එන්ජිම
schema-search-engine-detail = Start සහ command bar වෙතින් වෙබ් සෙවීම් සඳහා භාවිත වේ.
schema-window = කවුළුව
schema-pane = පැනලය
schema-side-sheet = පැති පත්‍රය
schema-focus-ring = Focus ring
schema-run-placement = ධාවන ස්ථාන override කිරීමට ඉඩ දෙන්න
schema-run-placement-detail = ධාවන පැනල ප්‍රකාරය, දිශාව, සහ anchor තේරීමට ඒජන්ට්වලට ඉඩ දෙන්න.
schema-leader = Leader
schema-leader-detail = chord කෙටිමං සඳහා prefix යතුර.
schema-chord-timeout = Chord කල් ඉකුත් වීම
schema-chord-timeout-detail = chord prefix එකක් කල් ඉකුත් වීමට පෙර මිලිසෙකන්ඩ්.
schema-bindings = Bindings
schema-confirm-close = වසාදැමීම තහවුරු කරන්න
schema-confirm-close-detail = ධාවනය වන ක්‍රියාවලියක් ඇති terminal එකක් වසීමට පෙර prompt කරන්න.
schema-default-theme = පෙරනිමි තේමාව
schema-default-theme-detail = themes ලැයිස්තුවෙන් සක්‍රීය තේමාවේ නම.
