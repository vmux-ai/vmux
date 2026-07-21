common-open = ክፈት
common-close = ዝጋ
common-install = ጫን
common-uninstall = አራግፍ
common-update = አዘምን
common-retry = እንደገና ሞክር
common-refresh = አድስ
common-remove = አስወግድ
common-enable = አንቃ
common-disable = አሰናክል
common-new = አዲስ
common-active = ንቁ
common-running = እየሰራ
common-done = ተጠናቋል
common-failed = አልተሳካም
common-installed = ተጭኗል
common-items = { $count ->
    [one] { $count } ንጥል
   *[other] { $count } ንጥሎች
}
start-title = ጀምር
start-tagline = አንድ prompt። ማንኛውም ስራ፣ ተጠናቋል።

agents-title = ኤጀንቶች
agents-search = የACP እና CLI ኤጀንቶችን ፈልግ…
agents-empty = የሚዛመድ ኤጀንት የለም
agents-empty-detail = ስም፣ runtime፣ ወይም ACP/CLI ይሞክሩ።
agents-install-failed = መጫን አልተሳካም
agents-updating = እየተዘመነ…
agents-retrying = እንደገና እየተሞከረ…
agents-preparing = እየተዘጋጀ…

extensions-title = ቅጥያዎች
extensions-search = የተጫኑትን ወይም Chrome Web Store ፈልግ…
extensions-relaunch = ለማ 적용 እንደገና አስጀምር
extensions-empty = የተጫነ ቅጥያ የለም
extensions-no-match = የሚዛመድ ቅጥያ የለም
extensions-empty-detail = ከላይ Chrome Web Store ውስጥ ፈልገው Return ይጫኑ።
extensions-no-match-detail = ሌላ ስም ወይም የቅጥያ ID ይሞክሩ።
extensions-on = በርቷል
extensions-off = ጠፍቷል
extensions-enable-confirm = { $name }ን ማንቃት?
extensions-enable-permissions = { $name }ን አንቃ እና ፍቀድ፦

lsp-title = የቋንቋ ሰርቨሮች
lsp-search = የቋንቋ ሰርቨሮች፣ linters፣ formatters ፈልግ…
lsp-loading = ካታሎግ እየተጫነ…
lsp-empty = የሚዛመድ የቋንቋ ሰርቨር የለም
lsp-empty-detail = ሌላ ቋንቋ፣ linter፣ ወይም formatter ይሞክሩ።
lsp-needs = { $tool } ያስፈልገዋል
lsp-status-available = ይገኛል
lsp-status-on-path = በPATH ላይ አለ
lsp-status-installing = እየተጫነ…
lsp-status-installed = ተጭኗል
lsp-status-outdated = ዝማኔ ይገኛል
lsp-status-running = እየሰራ
lsp-status-failed = አልተሳካም

spaces-title = ቦታዎች
spaces-new-placeholder = የአዲስ ቦታ ስም
spaces-empty = ቦታ የለም
spaces-default-name = ቦታ { $number }
spaces-tabs = { $count ->
    [one] 1 ትር
   *[other] { $count } ትሮች
}
spaces-delete = ቦታ ሰርዝ

team-title = ቡድን
team-just-you = በዚህ ቦታ ውስጥ እርስዎ ብቻ ነዎት
team-agents = { $count ->
    [one] እርስዎ እና 1 ኤጀንት
   *[other] እርስዎ እና { $count } ኤጀንቶች
}
team-empty = እስካሁን ማንም የለም
team-you = እርስዎ
team-agent = ኤጀንት

services-title = የጀርባ አገልግሎቶች
services-processes = { $count ->
    [one] 1 ሂደት
   *[other] { $count } ሂደቶች
}
services-kill-all = ሁሉንም አስገድደህ አቁም
services-not-running = አገልግሎቱ እየሰራ አይደለም
services-start-with = በዚህ ጀምር፦
services-empty = ንቁ ሂደት የለም
services-filter = ሂደቶችን አጣራ…
services-no-match = የሚዛመድ ሂደት የለም
services-connected = ተገናኝቷል
services-disconnected = ተቋርጧል
services-attached = ተያይዟል
services-kill = አስገድደህ አቁም
services-memory = ማህደረ ትውስታ
services-size = መጠን
services-shell = Shell

error-title = ስህተት

history-search = ታሪክ ፈልግ
history-clear-all = ሁሉንም አጽዳ
history-clear-confirm = ሁሉንም ታሪክ ማጽዳት?
history-clear-warning = ይህን መመለስ አይቻልም።
history-cancel = ይቅር
history-today = ዛሬ
history-yesterday = ትናንት
history-days-ago = ከ{ $count } ቀናት በፊት
history-day-offset = ቀን -{ $count }

settings-title = ቅንብሮች
settings-loading = ቅንብሮች እየተጫኑ…
settings-stored = በ ~/.vmux/settings.ron ውስጥ ተቀምጧል
settings-other = ሌላ
settings-software-update = የሶፍትዌር ዝማኔ
settings-check-updates = ዝማኔዎችን ፈትሽ
settings-check-updates-hint = Auto-update ሲነቃ በመጀመሪያ ማስጀመር እና በየሰዓቱ በራስ-ሰር ይፈትሻል።
settings-update-unavailable = አይገኝም
settings-update-unavailable-hint = አዘማኙ በዚህ build ውስጥ አልተካተተም።
settings-update-checking = እየተፈተሸ…
settings-update-checking-hint = ዝማኔዎች እየተፈተሹ…
settings-update-check-again = እንደገና ፈትሽ
settings-update-current = Vmux ዘምኗል።
settings-update-downloading = እየወረደ…
settings-update-downloading-hint = Vmux { $version } እየወረደ…
settings-update-installing = እየተጫነ…
settings-update-installing-hint = Vmux { $version } እየተጫነ…
settings-update-ready = ዝማኔው ዝግጁ ነው
settings-update-ready-hint = Vmux { $version } ዝግጁ ነው። ለማ 적용 እንደገና አስጀምር።
settings-update-try-again = እንደገና ሞክር
settings-update-failed = ዝማኔዎችን መፈተሽ አልተቻለም።
settings-item = ንጥል
settings-item-number = ንጥል { $number }
settings-press-key = ቁልፍ ይጫኑ…
settings-saved = ተቀምጧል
settings-record-key = አዲስ የቁልፍ ጥምር ለመቅረጽ ጠቅ ያድርጉ

tray-open-window = መስኮት ክፈት
tray-close-window = መስኮት ዝጋ
tray-pause-recording = ቀረጻ አቁም ለጊዜው
tray-resume-recording = ቀረጻ ቀጥል
tray-finish-recording = ቀረጻ ጨርስ
tray-quit = Vmuxን ውጣ

composer-attach-files = ፋይሎችን አያይዝ (/upload)
composer-remove-attachment = አባሪ አስወግድ

layout-back = ተመለስ
layout-forward = ወደፊት
layout-reload = ዳግም ጫን
layout-bookmark-page = ይህን ገጽ ዕልባት አድርግ
layout-remove-bookmark = ዕልባት አስወግድ
layout-pin-page = ይህን ገጽ ሰካ
layout-unpin-page = ይህን ገጽ ንቀል
layout-manage-extensions = ቅጥያዎችን አስተዳድር
layout-new-stack = አዲስ ቁልል
layout-close-tab = ትር ዝጋ
layout-bookmark = ዕልባት
layout-pin = ሰካ
layout-new-tab = አዲስ ትር
layout-team = ቡድን

command-switch-space = ቦታ ቀይር…
command-search-ask = ፈልግ ወይም ጠይቅ…
command-new-tab-placeholder = ፈልግ፣ URL ጻፍ፣ ወይም Terminal ምረጥ…
command-placeholder = URL ጻፍ፣ ትሮችን ፈልግ፣ ወይም ለትዕዛዞች > ጻፍ…
command-composer-placeholder = ለትዕዛዞች / ወይም ለሚዲያ @ ጻፍ
command-send = ላክ (Enter)
command-terminal = Terminal
command-open-terminal = በTerminal ክፈት
command-stack = ቁልል
command-tabs = { $count ->
    [one] 1 ትር
   *[other] { $count } ትሮች
}
command-prompt = Prompt
command-new-tab = አዲስ ትር
command-search = ፈልግ
command-open-value = “{ $value }” ክፈት
command-search-value = “{ $value }” ፈልግ

schema-appearance = መልክ
schema-general = አጠቃላይ
schema-layout = አቀማመጥ
schema-layout-detail = መስኮት፣ ክፍሎች፣ የጎን አሞሌ፣ እና የትኩረት ቀለበት።
schema-agent = ኤጀንት
schema-agent-detail = የኤጀንት ባህሪ እና የመሳሪያ ፈቃዶች።
schema-shortcuts = አቋራጮች
schema-shortcuts-detail = ለንባብ ብቻ። ጥምሮችን ለመቀየር settings.ronን በቀጥታ ያርትዑ።
schema-terminal = Terminal
schema-browser = አሳሽ
schema-mode = ሁነታ
schema-mode-detail = ለድር ገጾች የቀለም ገጽታ። Device የስርዓትዎን ይከተላል።
schema-device = Device
schema-light = ብርሃን
schema-dark = ጨለማ
schema-language = ቋንቋ
schema-language-detail = ስርዓትን፣ en-US፣ ja፣ ወይም ከተዛመደ ~/.vmux/locales/<tag>.ftl ካታሎግ ጋር ማንኛውንም BCP 47 tag ተጠቀም።
schema-auto-update = Auto-update
schema-auto-update-detail = በመጀመሪያ ማስጀመር እና በየሰዓቱ ዝማኔዎችን ፈትሽና ጫን።
schema-startup-url = የመነሻ URL
schema-startup-url-detail = ባዶ ከሆነ የትዕዛዝ አሞሌ prompt ይከፍታል።
schema-search-engine = የፍለጋ ሞተር
schema-search-engine-detail = ከመነሻ እና ከትዕዛዝ አሞሌ ለድር ፍለጋዎች ይጠቀማል።
schema-window = መስኮት
schema-pane = ክፍል
schema-side-sheet = የጎን ሉህ
schema-focus-ring = የትኩረት ቀለበት
schema-run-placement = የማስኬጃ ቦታ መተካትን ፍቀድ
schema-run-placement-detail = ኤጀንቶች የማስኬጃ ክፍል ሁነታ፣ አቅጣጫ፣ እና መያዣ እንዲመርጡ ፍቀድ።
schema-leader = Leader
schema-leader-detail = ለchord አቋራጮች የቅድመ ቁልፍ።
schema-chord-timeout = የChord ጊዜ ገደብ
schema-chord-timeout-detail = የchord ቅድመ ቁልፍ ከማብቃቱ በፊት የሚቆዩ ሚሊሰከንዶች።
schema-bindings = ጥምሮች
schema-confirm-close = መዝጋት አረጋግጥ
schema-confirm-close-detail = እየሰራ ያለ ሂደት ያለውን terminal ከመዝጋት በፊት ጠይቅ።
schema-default-theme = ነባሪ ገጽታ
schema-default-theme-detail = ከገጽታዎች ዝርዝር ውስጥ የንቁ ገጽታ ስም።
