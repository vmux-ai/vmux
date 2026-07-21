common-open = Tsegula
common-close = Tseka
common-install = Ikani
common-uninstall = Chotsa
common-update = Sinthani
common-retry = Yesaninso
common-refresh = Tsitsimutsa
common-remove = Chotsa
common-enable = Yatsani
common-disable = Zimitsani
common-new = Chatsopano
common-active = yogwira
common-running = ikuyenda
common-done = zatha
common-failed = Zalephera
common-installed = Yayikidwa
common-items = { $count ->
    [one] chinthu { $count }
   *[other] zinthu { $count }
}
start-title = Yambani
start-tagline = Lamulo limodzi. Chilichonse chitheka.

agents-title = Maajenti a AI
agents-search = Sakani maajenti a ACP ndi CLI…
agents-empty = Palibe maajenti ofanana
agents-empty-detail = Yesani dzina, runtime, kapena ACP/CLI.
agents-install-failed = Kuyika kwalephera
agents-updating = Ikusintha…
agents-retrying = Ikuyesanso…
agents-preparing = Ikukonzekera…

extensions-title = Zowonjezera
extensions-search = Sakani zoyikidwa kapena Chrome Web Store…
extensions-relaunch = Yambitsaninso kuti zigwire
extensions-empty = Palibe zowonjezera zoyikidwa
extensions-no-match = Palibe zowonjezera zofanana
extensions-empty-detail = Sakani mu Chrome Web Store pamwambapa kenako dinani Return.
extensions-no-match-detail = Yesani dzina lina kapena ID ya chowonjezera.
extensions-on = Yayatsidwa
extensions-off = Yazimitsidwa
extensions-enable-confirm = Yatsani { $name }?
extensions-enable-permissions = Yatsani { $name } ndi kulola:

lsp-title = Ma seva a zilankhulo
lsp-search = Sakani ma seva a zilankhulo, ma linter, ma formatter…
lsp-loading = Ikutsegula katalogi…
lsp-empty = Palibe ma seva a zilankhulo ofanana
lsp-empty-detail = Yesani chilankhulo china, linter, kapena formatter.
lsp-needs = ikufuna { $tool }
lsp-status-available = Ilipo
lsp-status-on-path = Ili pa PATH
lsp-status-installing = Ikuyika…
lsp-status-installed = Yayikidwa
lsp-status-outdated = Kusintha kulipo
lsp-status-running = Ikuyenda
lsp-status-failed = Yalephera

spaces-title = Malo
spaces-new-placeholder = Dzina la malo atsopano
spaces-empty = Palibe malo
spaces-default-name = Malo { $number }
spaces-tabs = { $count ->
    [one] tabu 1
   *[other] tabu { $count }
}
spaces-delete = Chotsa malo

team-title = Gulu
team-just-you = Muli nokha m'malo muno
team-agents = { $count ->
    [one] Inu ndi ajenti 1
   *[other] Inu ndi maajenti { $count }
}
team-empty = Palibe aliyense pano
team-you = Inu
team-agent = Ajenti

services-title = Ntchito zakumbuyo
services-processes = { $count ->
    [one] process 1
   *[other] ma process { $count }
}
services-kill-all = Imitsani Zonse
services-not-running = Ntchitoyi siyikuyenda
services-start-with = Yambitsani ndi:
services-empty = Palibe ma process omwe akuyenda
services-filter = Sefani ma process…
services-no-match = Palibe ma process ofanana
services-connected = Zalumikizidwa
services-disconnected = Zalekanitsidwa
services-attached = cholumikizidwa
services-kill = Imitsa
services-memory = Memory
services-size = Kukula
services-shell = Shell

error-title = Vuto

history-search = Sakani mbiri
history-clear-all = Chotsani zonse
history-clear-confirm = Chotsani mbiri yonse?
history-clear-warning = Izi sizingabwezedwe.
history-cancel = Letsani
history-today = Lero
history-yesterday = Dzulo
history-days-ago = Masiku { $count } apitawo
history-day-offset = Tsiku -{ $count }

settings-title = Zokonda
settings-loading = Ikutsegula zokonda…
settings-stored = Zasungidwa mu ~/.vmux/settings.ron
settings-other = Zina
settings-software-update = Kusintha pulogalamu
settings-check-updates = Fufuzani Zosintha
settings-check-updates-hint = Imadzifufuza yokha ikayambika komanso ola lililonse ngati Auto-update yayatsidwa.
settings-update-unavailable = Sizikupezeka
settings-update-unavailable-hint = Chosinthira sichinaphatikizidwe mu build iyi.
settings-update-checking = Ikufufuza…
settings-update-checking-hint = Ikufufuza zosintha…
settings-update-check-again = Fufuzaninso
settings-update-current = Vmux ili ndi zosintha zaposachedwa.
settings-update-downloading = Ikutsitsa…
settings-update-downloading-hint = Ikutsitsa Vmux { $version }…
settings-update-installing = Ikuyika…
settings-update-installing-hint = Ikuyika Vmux { $version }…
settings-update-ready = Kusintha Kwakonzeka
settings-update-ready-hint = Vmux { $version } yakonzeka. Yambitsaninso kuti igwire.
settings-update-try-again = Yesaninso
settings-update-failed = Zalephera kufufuza zosintha.
settings-item = Chinthu
settings-item-number = Chinthu { $number }
settings-press-key = Dinani kiyibodi…
settings-saved = Zasungidwa
settings-record-key = Dinani kuti mujambule makiyi atsopano

tray-open-window = Tsegula Zenera
tray-close-window = Tseka Zenera
tray-pause-recording = Imitsani Kaye Kujambula
tray-resume-recording = Pitirizani Kujambula
tray-finish-recording = Malizani Kujambula
tray-quit = Tulukani mu Vmux

composer-attach-files = Onjezani mafayilo (/upload)
composer-remove-attachment = Chotsa cholumikiza

layout-back = Bwerera
layout-forward = Pitani patsogolo
layout-reload = Tsegulanso
layout-bookmark-page = Ikani tsambali ku ma bookmark
layout-remove-bookmark = Chotsa bookmark
layout-pin-page = Mangiriza tsambali
layout-unpin-page = Masula tsambali
layout-manage-extensions = Konzani zowonjezera
layout-new-stack = Stack yatsopano
layout-close-tab = Tseka tabu
layout-bookmark = Bookmark
layout-pin = Mangiriza
layout-new-tab = Tabu yatsopano
layout-team = Gulu

command-switch-space = Sinthani malo…
command-search-ask = Sakani kapena funsani…
command-new-tab-placeholder = Sakani kapena lembani URL, kapena sankhani Terminal…
command-placeholder = Lembani URL, sakani ma tabu, kapena > pa malamulo…
command-composer-placeholder = Lembani / pa malamulo kapena @ pa media
command-send = Tumizani (Enter)
command-terminal = Terminal
command-open-terminal = Tsegula mu Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] tabu 1
   *[other] tabu { $count }
}
command-prompt = Prompt
command-new-tab = Tabu yatsopano
command-search = Sakani
command-open-value = Tsegula “{ $value }”
command-search-value = Sakani “{ $value }”

schema-appearance = Maonekedwe
schema-general = Zonse
schema-layout = Kapangidwe
schema-layout-detail = Zenera, magawo, sidebar, ndi focus ring.
schema-agent = Ajenti
schema-agent-detail = Mmene ajenti amachitira ndi zilolezo za zida.
schema-shortcuts = Njira zachidule
schema-shortcuts-detail = Zowerenga zokha. Sinthani settings.ron mwachindunji kuti musinthe ma binding.
schema-terminal = Terminal
schema-browser = Msakatuli
schema-mode = Mode
schema-mode-detail = Mtundu wa masamba a webu. Device imatsatira dongosolo lanu.
schema-device = Device
schema-light = Kuwala
schema-dark = Mdima
schema-language = Chilankhulo
schema-language-detail = Gwiritsani ntchito dongosolo, en-US, ja, kapena tag iliyonse ya BCP 47 yokhala ndi katalogi yofanana ya ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Auto-update
schema-auto-update-detail = Fufuzani ndi kuyika zosintha ikayambika komanso ola lililonse.
schema-startup-url = URL yoyambira
schema-startup-url-detail = Ikakhala yopanda kanthu imatsegula prompt ya command bar.
schema-search-engine = Injini yosakira
schema-search-engine-detail = Imagwiritsidwa ntchito posaka pa webu kuchokera ku Start ndi command bar.
schema-window = Zenera
schema-pane = Gawo
schema-side-sheet = Pepala la m'mbali
schema-focus-ring = Mphete ya focus
schema-run-placement = Lolani kusintha malo oyendetsera
schema-run-placement-detail = Lolani maajenti kusankha mode ya gawo loyendetsera, njira, ndi anchor.
schema-leader = Leader
schema-leader-detail = Kiyi yoyambira ya ma shortcut a chord.
schema-chord-timeout = Nthawi yodikirira chord
schema-chord-timeout-detail = Ma millisecond chord prefix isanathe.
schema-bindings = Ma binding
schema-confirm-close = Tsimikizani kutseka
schema-confirm-close-detail = Funsani musanatseke terminal yokhala ndi process yomwe ikuyenda.
schema-default-theme = Theme yokhazikika
schema-default-theme-detail = Dzina la theme yogwira kuchokera pa mndandanda wa ma theme.
