common-open = Tsegula
common-close = Tseka
common-install = Ikani
common-uninstall = Chotsani
common-update = Kakololezani
common-retry = Yesaninso
common-refresh = Bwezerani
common-remove = Chotsani
common-enable = Yatsani
common-disable = Letsani
common-new = Watsopano
common-active = wogwira ntchito
common-running = ikuyenda
common-done = zachita
common-failed = Zalephera
common-installed = Yaikidwa
common-items = { $count ->
    [one] { $count } chinthu
   *[other] { $count } zinthu
}
start-title = Yambani
start-tagline = Uthenga umodzi. Chilichonse, zachita.

agents-title = Agents
agents-search = Sakani ACP ndi CLI agents…
agents-empty = Palibe agents oyendana
agents-empty-detail = Yesani dzina, runtime, kapena ACP/CLI.
agents-install-failed = Kuika kulephera
agents-updating = Kukakololeza…
agents-retrying = Kuyesaninso…
agents-preparing = Kukonzekera…

extensions-title = Zowonjezera
extensions-search = Sakani zoikidwa kapena Chrome Web Store…
extensions-relaunch = Yambitsaninso kugwiritsa ntchito
extensions-empty = Palibe zowonjezera zoikidwa
extensions-no-match = Palibe zowonjezera zoyendana
extensions-empty-detail = Sakani Chrome Web Store pamwamba ndi ponse Return.
extensions-no-match-detail = Yesani dzina lina kapena ID ya extension.
extensions-on = Wayatsa
extensions-off = Wachetsa
extensions-enable-confirm = Yatsani { $name }?
extensions-enable-permissions = Yatsani { $name } ndi kulola:

lsp-title = Seva za Zinenero
lsp-search = Sakani seva za zinenero, linters, formatters…
lsp-loading = Kukweza mawonekedwe…
lsp-empty = Palibe seva za zinenero zoyendana
lsp-empty-detail = Yesani chinenero china, linter, kapena formatter.
lsp-needs = ikufuna { $tool }
lsp-status-available = Yakhalapo
lsp-status-on-path = Pa PATH
lsp-status-installing = Kuikidwa…
lsp-status-installed = Yaikidwa
lsp-status-outdated = Kakololedwe lipo
lsp-status-running = Ikuyenda
lsp-status-failed = Yalephera

spaces-title = Malo
spaces-new-placeholder = Dzina la malo atsopano
spaces-empty = Palibe malo
spaces-default-name = Malo { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tabs
}
spaces-delete = Chotsani malo

team-title = Gulu
team-just-you = Inu nokha m'malo awa
team-agents = { $count ->
    [one] Inu ndi agent 1
   *[other] Inu ndi agents { $count }
}
team-empty = Palibe wina pano
team-you = Inu
team-agent = Agent

services-title = Ntchito Zapambuyo
services-processes = { $count ->
    [one] 1 ndondomeko
   *[other] { $count } ndondomeko
}
services-kill-all = Imitsani Zonse
services-not-running = Ntchito siikuyenda
services-start-with = Yambani ndi:
services-empty = Palibe ndondomeko zotseguka
services-filter = Sefa ndondomeko…
services-no-match = Palibe ndondomeko zoyendana
services-connected = Yalumikizidwa
services-disconnected = Silumikizidwa
services-attached = yokhazikika
services-kill = Imitsani
services-memory = Memore
services-size = Kukula
services-shell = Shell

error-title = Cholakwa

history-search = Sakani mbiri
history-clear-all = Chotsani zonse
history-clear-confirm = Chotsani mbiri yonse?
history-clear-warning = Izi sizingabwerere.
history-cancel = Lekani
history-today = Lero
history-yesterday = Dzulo
history-days-ago = { $count } masiku apitayo
history-day-offset = Tsiku -{ $count }

settings-title = Zikhazikiko
settings-loading = Kukweza zikhazikiko…
settings-stored = Zosungidwa mu ~/.vmux/settings.ron
settings-other = Zina
settings-software-update = Kakololedwe la Software
settings-check-updates = Fufuzani Kakololedwe
settings-check-updates-hint = Imafufuza yokha pamayambiriro ndi ola lililonse Auto-update ikayatsa.
settings-update-unavailable = Siilipo
settings-update-unavailable-hint = Chokakolozetsa sichili mu chipangizo ichi.
settings-update-checking = Kufufuza…
settings-update-checking-hint = Kufufuza kakololedwe…
settings-update-check-again = Fufuzaninso
settings-update-current = Vmux ndi watsopano.
settings-update-downloading = Kutsitsa…
settings-update-downloading-hint = Kutsitsa Vmux { $version }…
settings-update-installing = Kuikidwa…
settings-update-installing-hint = Kuika Vmux { $version }…
settings-update-ready = Kakololedwe Lokonzeka
settings-update-ready-hint = Vmux { $version } lokonzeka. Yambitsaninso kugwiritsa ntchito.
settings-update-try-again = Yesaninso
settings-update-failed = Sikutheka kufufuza kakololedwe.
settings-item = Chinthu
settings-item-number = Chinthu { $number }
settings-press-key = Sindani kiyi…
settings-saved = Zasungidwa
settings-record-key = Dinani kulemba gulu latsopano la kiyi

tray-open-window = Tsegula Zenera
tray-close-window = Tseka Zenera
tray-pause-recording = Imitsani Kulemba
tray-resume-recording = Yambitsaninso Kulemba
tray-finish-recording = Maliza Kulemba
tray-quit = Chotsani Vmux

composer-attach-files = Onjezerani mafayilo (/upload)
composer-remove-attachment = Chotsani choonjezerwa

layout-back = Bwerani
layout-forward = Pitsani
layout-reload = Tsanzaninso
layout-bookmark-page = Sungani tsamba ili
layout-remove-bookmark = Chotsani chizindikiro
layout-pin-page = Sinkizani tsamba ili
layout-unpin-page = Tulutsani sinkizo
layout-manage-extensions = Yanganirani zowonjezera
layout-new-stack = Stack Watsopano
layout-close-tab = Tseka tab
layout-bookmark = Chizindikiro
layout-pin = Sinkizo
layout-new-tab = Tab yatsopano
layout-team = Gulu

command-switch-space = Sinthani malo…
command-search-ask = Sakani kapena funsani…
command-new-tab-placeholder = Sakani kapena lembani URL, kapena sankhani Terminal…
command-placeholder = Lembani URL, sakani tabs, kapena > kwa malamulo…
command-composer-placeholder = Lembani / kwa malamulo kapena @ kwa zithunzi
command-send = Tumizani (Enter)
command-terminal = Terminal
command-open-terminal = Tsegula mu Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tabs
}
command-prompt = Mafunso
command-new-tab = Tab yatsopano
command-search = Sakani
command-open-value = Tsegula "{ $value }"
command-search-value = Sakani "{ $value }"

schema-appearance = Mawonekedwe
schema-general = Wamba
schema-layout = Dongosolo
schema-layout-detail = Zenera, mapane, msewu wammbali, ndi mphete yosonyeza.
schema-agent = Agent
schema-agent-detail = Khalidwe la agent ndi chilolezo cha zida.
schema-shortcuts = Njira Zamfupi
schema-shortcuts-detail = Mawonekedwe okha owerenga. Sinthani settings.ron mwachindunji kusintha makumanika.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Mtundu
schema-mode-detail = Ndondomeko ya mitundu ya masamba. Chipangizo chimatengera nyengo yanu ya machini.
schema-device = Chipangizo
schema-light = Wowala
schema-dark = Mdima
schema-language = Chinenero
schema-language-detail = Gwiritsa ntchito machini, en-US, ja, kapena chizindikiro chilichonse cha BCP 47 chokhala ndi ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Kakololedwe Lokha
schema-auto-update-detail = Fufuzani ndi kuika kakololedwe pamayambiriro ndi ola lililonse.
schema-startup-url = URL ya Chiyambi
schema-startup-url-detail = Wopanda chilichonse kutsegula mwendo wa malamulo.
schema-search-engine = Injini ya Kusaka
schema-search-engine-detail = Imagwiritsidwa ntchito kwa kusaka kwamba kuchokera pa Start ndi mwendo wa malamulo.
schema-window = Zenera
schema-pane = Pane
schema-side-sheet = Tsamba Lammbali
schema-focus-ring = Mphete Yosonyeza
schema-run-placement = Lolani kusintha kuimika kwa run
schema-run-placement-detail = Lolani agents kusankha mtundu wa pane ya run, njira, ndi anchor.
schema-leader = Wotsogolera
schema-leader-detail = Kiyi yoyamba kwa njira za chord.
schema-chord-timeout = Nthawi ya Chord
schema-chord-timeout-detail = Milliseconds asanatha nthawi prefix ya chord.
schema-bindings = Makumanika
schema-confirm-close = Tsimikizani kutseka
schema-confirm-close-detail = Funsani musanatseke terminal yokhala ndi ndondomeko yoyenda.
schema-default-theme = Mtundu Woyamba
schema-default-theme-detail = Dzina la mtundu wotseguka ku mndandanda wa mitundu.
