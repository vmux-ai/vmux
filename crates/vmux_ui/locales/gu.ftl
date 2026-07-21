common-open = ખોલો
common-close = બંધ કરો
common-install = ઇન્સ્ટોલ કરો
common-uninstall = અનઇન્સ્ટોલ કરો
common-update = અપડેટ કરો
common-retry = ફરી પ્રયાસ કરો
common-refresh = રિફ્રેશ કરો
common-remove = દૂર કરો
common-enable = ચાલુ કરો
common-disable = બંધ કરો
common-new = નવું
common-active = સક્રિય
common-running = ચાલી રહ્યું છે
common-done = પૂર્ણ
common-failed = નિષ્ફળ
common-installed = ઇન્સ્ટોલ થયેલ
common-items = { $count ->
    [one] { $count } આઇટમ
   *[other] { $count } આઇટમ
}
start-title = શરૂ કરો
start-tagline = એક prompt. કંઈ પણ, તૈયાર.

agents-title = એજન્ટ્સ
agents-search = ACP અને CLI એજન્ટ્સ શોધો…
agents-empty = મેળ ખાતા એજન્ટ્સ નથી
agents-empty-detail = નામ, runtime અથવા ACP/CLI અજમાવો.
agents-install-failed = ઇન્સ્ટોલ નિષ્ફળ
agents-updating = અપડેટ થઈ રહ્યું છે…
agents-retrying = ફરી પ્રયાસ થઈ રહ્યો છે…
agents-preparing = તૈયારી થઈ રહી છે…

extensions-title = એક્સ્ટેન્શન્સ
extensions-search = ઇન્સ્ટોલ થયેલ અથવા Chrome Web Store માં શોધો…
extensions-relaunch = લાગુ કરવા ફરી શરૂ કરો
extensions-empty = કોઈ એક્સ્ટેન્શન ઇન્સ્ટોલ નથી
extensions-no-match = મેળ ખાતું એક્સ્ટેન્શન નથી
extensions-empty-detail = ઉપર Chrome Web Store માં શોધો અને Return દબાવો.
extensions-no-match-detail = બીજું નામ અથવા એક્સ્ટેન્શન ID અજમાવો.
extensions-on = ચાલુ
extensions-off = બંધ
extensions-enable-confirm = { $name } ચાલુ કરવું છે?
extensions-enable-permissions = { $name } ચાલુ કરો અને મંજૂરી આપો:

lsp-title = Language Servers
lsp-search = language servers, linters, formatters શોધો…
lsp-loading = કૅટલોગ લોડ થઈ રહ્યો છે…
lsp-empty = મેળ ખાતા language servers નથી
lsp-empty-detail = બીજી ભાષા, linter અથવા formatter અજમાવો.
lsp-needs = { $tool } જરૂરી છે
lsp-status-available = ઉપલબ્ધ
lsp-status-on-path = PATH પર
lsp-status-installing = ઇન્સ્ટોલ થઈ રહ્યું છે…
lsp-status-installed = ઇન્સ્ટોલ થયેલ
lsp-status-outdated = અપડેટ ઉપલબ્ધ
lsp-status-running = ચાલી રહ્યું છે
lsp-status-failed = નિષ્ફળ

spaces-title = વર્કસ્પેસ
spaces-new-placeholder = નવા વર્કસ્પેસનું નામ
spaces-empty = કોઈ વર્કસ્પેસ નથી
spaces-default-name = વર્કસ્પેસ { $number }
spaces-tabs = { $count ->
    [one] 1 ટૅબ
   *[other] { $count } ટૅબ
}
spaces-delete = વર્કસ્પેસ કાઢી નાખો

team-title = ટીમ
team-just-you = આ વર્કસ્પેસમાં માત્ર તમે
team-agents = { $count ->
    [one] તમે અને 1 એજન્ટ
   *[other] તમે અને { $count } એજન્ટ્સ
}
team-empty = હજી અહીં કોઈ નથી
team-you = તમે
team-agent = એજન્ટ

services-title = પૃષ્ઠભૂમિ સેવાઓ
services-processes = { $count ->
    [one] 1 પ્રક્રિયા
   *[other] { $count } પ્રક્રિયાઓ
}
services-kill-all = બધું બંધ કરો
services-not-running = સેવા ચાલી રહી નથી
services-start-with = આથી શરૂ કરો:
services-empty = કોઈ સક્રિય પ્રક્રિયા નથી
services-filter = પ્રક્રિયાઓ ફિલ્ટર કરો…
services-no-match = મેળ ખાતી પ્રક્રિયા નથી
services-connected = જોડાયેલ
services-disconnected = ડિસ્કનેક્ટેડ
services-attached = જોડાયેલ
services-kill = બળજબરીથી બંધ કરો
services-memory = મેમરી
services-size = કદ
services-shell = Shell

error-title = ભૂલ

history-search = ઇતિહાસ શોધો
history-clear-all = બધું સાફ કરો
history-clear-confirm = આખો ઇતિહાસ સાફ કરવો છે?
history-clear-warning = આ પાછું ફેરવી શકાશે નહીં.
history-cancel = રદ કરો
history-today = આજે
history-yesterday = ગઈકાલે
history-days-ago = { $count } દિવસ પહેલાં
history-day-offset = દિવસ -{ $count }

settings-title = સેટિંગ્સ
settings-loading = સેટિંગ્સ લોડ થઈ રહી છે…
settings-stored = ~/.vmux/settings.ron માં સંગ્રહિત
settings-other = અન્ય
settings-software-update = સોફ્ટવેર અપડેટ
settings-check-updates = અપડેટ્સ તપાસો
settings-check-updates-hint = Auto-update ચાલુ હોય ત્યારે લૉન્ચ સમયે અને દર કલાકે આપમેળે તપાસે છે.
settings-update-unavailable = ઉપલબ્ધ નથી
settings-update-unavailable-hint = આ બિલ્ડમાં updater સામેલ નથી.
settings-update-checking = તપાસી રહ્યું છે…
settings-update-checking-hint = અપડેટ્સ તપાસી રહ્યું છે…
settings-update-check-again = ફરી તપાસો
settings-update-current = Vmux અદ્યતન છે.
settings-update-downloading = ડાઉનલોડ થઈ રહ્યું છે…
settings-update-downloading-hint = Vmux { $version } ડાઉનલોડ થઈ રહ્યું છે…
settings-update-installing = ઇન્સ્ટોલ થઈ રહ્યું છે…
settings-update-installing-hint = Vmux { $version } ઇન્સ્ટોલ થઈ રહ્યું છે…
settings-update-ready = અપડેટ તૈયાર
settings-update-ready-hint = Vmux { $version } તૈયાર છે. લાગુ કરવા ફરી શરૂ કરો.
settings-update-try-again = ફરી પ્રયાસ કરો
settings-update-failed = અપડેટ્સ તપાસી શક્યાં નથી.
settings-item = આઇટમ
settings-item-number = આઇટમ { $number }
settings-press-key = કોઈ કી દબાવો…
settings-saved = સાચવ્યું
settings-record-key = નવું કી કોમ્બો રેકોર્ડ કરવા ક્લિક કરો

tray-open-window = વિન્ડો ખોલો
tray-close-window = વિન્ડો બંધ કરો
tray-pause-recording = રેકોર્ડિંગ થોભાવો
tray-resume-recording = રેકોર્ડિંગ ફરી શરૂ કરો
tray-finish-recording = રેકોર્ડિંગ પૂર્ણ કરો
tray-quit = Vmux છોડો

composer-attach-files = ફાઇલો જોડો (/upload)
composer-remove-attachment = જોડાણ દૂર કરો

layout-back = પાછળ
layout-forward = આગળ
layout-reload = ફરી લોડ કરો
layout-bookmark-page = આ પેજ બુકમાર્ક કરો
layout-remove-bookmark = બુકમાર્ક દૂર કરો
layout-pin-page = આ પેજ પિન કરો
layout-unpin-page = આ પેજ અનપિન કરો
layout-manage-extensions = એક્સ્ટેન્શન્સ મેનેજ કરો
layout-new-stack = નવો સ્ટેક
layout-close-tab = ટૅબ બંધ કરો
layout-bookmark = બુકમાર્ક
layout-pin = પિન કરો
layout-new-tab = નવી ટૅબ
layout-team = ટીમ

command-switch-space = વર્કસ્પેસ બદલો…
command-search-ask = શોધો અથવા પૂછો…
command-new-tab-placeholder = શોધો અથવા URL લખો, અથવા Terminal પસંદ કરો…
command-placeholder = URL લખો, ટૅબ શોધો, અથવા કમાન્ડ માટે > લખો…
command-composer-placeholder = કમાન્ડ માટે / અથવા મીડિયા માટે @ લખો
command-send = મોકલો (Enter)
command-terminal = Terminal
command-open-terminal = Terminal માં ખોલો
command-stack = સ્ટેક
command-tabs = { $count ->
    [one] 1 ટૅબ
   *[other] { $count } ટૅબ
}
command-prompt = Prompt
command-new-tab = નવી ટૅબ
command-search = શોધો
command-open-value = “{ $value }” ખોલો
command-search-value = “{ $value }” શોધો

schema-appearance = દેખાવ
schema-general = સામાન્ય
schema-layout = લેઆઉટ
schema-layout-detail = વિન્ડો, પેન, સાઇડબાર અને ફોકસ રિંગ.
schema-agent = એજન્ટ
schema-agent-detail = એજન્ટનું વર્તન અને ટૂલ પરવાનગીઓ.
schema-shortcuts = શોર્ટકટ્સ
schema-shortcuts-detail = માત્ર વાંચવા માટે. bindings બદલવા settings.ron સીધું સંપાદિત કરો.
schema-terminal = Terminal
schema-browser = બ્રાઉઝર
schema-mode = મોડ
schema-mode-detail = વેબ પેજ માટે રંગ યોજના. Device તમારા સિસ્ટમને અનુસરે છે.
schema-device = Device
schema-light = લાઇટ
schema-dark = ડાર્ક
schema-language = ભાષા
schema-language-detail = system, en-US, ja, અથવા મેળ ખાતા ~/.vmux/locales/<tag>.ftl કૅટલોગ સાથે કોઈપણ BCP 47 tag વાપરો.
schema-auto-update = Auto-update
schema-auto-update-detail = લૉન્ચ સમયે અને દર કલાકે અપડેટ્સ તપાસો અને ઇન્સ્ટોલ કરો.
schema-startup-url = શરૂઆતનું URL
schema-startup-url-detail = ખાલી હોય તો કમાન્ડ બાર prompt ખૂલે છે.
schema-search-engine = શોધ એન્જિન
schema-search-engine-detail = Start અને કમાન્ડ બારમાંથી વેબ શોધ માટે વપરાય છે.
schema-window = વિન્ડો
schema-pane = પેન
schema-side-sheet = સાઇડ શીટ
schema-focus-ring = ફોકસ રિંગ
schema-run-placement = run placement override મંજૂર કરો
schema-run-placement-detail = એજન્ટ્સને run પેન મોડ, દિશા અને anchor પસંદ કરવા દો.
schema-leader = લીડર
schema-leader-detail = chord શોર્ટકટ્સ માટે prefix કી.
schema-chord-timeout = Chord timeout
schema-chord-timeout-detail = chord prefix સમાપ્ત થાય તે પહેલાંના મિલિસેકન્ડ.
schema-bindings = Bindings
schema-confirm-close = બંધ કરતા પહેલાં પુષ્ટિ
schema-confirm-close-detail = ચાલી રહેલી પ્રક્રિયા ધરાવતા terminal ને બંધ કરતા પહેલાં પૂછો.
schema-default-theme = ડિફોલ્ટ થીમ
schema-default-theme-detail = થીમ્સ સૂચિમાંથી સક્રિય થીમનું નામ.
