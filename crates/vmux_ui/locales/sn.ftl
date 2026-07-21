common-open = Vhura
common-close = Vhara
common-install = Isa
common-uninstall = Bvisa
common-update = Shandura
common-retry = Edza Zvakare
common-refresh = Vandudza
common-remove = Bvisa
common-enable = Bvumira
common-disable = Dzima
common-new = Itsva
common-active = inoshanda
common-running = ichimhanya
common-done = yapera
common-failed = Yakundikana
common-installed = Yaaiswa
common-items = { $count ->
    [one] { $count } chinhu
   *[other] { $count } zvinhu
}
start-title = Tanga
start-tagline = Murayiro umwe. Chero chinhu, chapera.

agents-title = Maajenti
agents-search = Tsvaga maajenti eACP neCLI…
agents-empty = Hapana maajenti anowanikwa
agents-empty-detail = Edza zita, runtime, kana ACP/CLI.
agents-install-failed = Kuisa kwakundikana
agents-updating = Ichishandurwa…
agents-retrying = Ichiedzazve…
agents-preparing = Ichirongwa…

extensions-title = Zvowedzerwa
extensions-search = Tsvaga zvaiswa kana Chrome Web Store…
extensions-relaunch = Tanga patsva kugamuchira
extensions-empty = Hapana zvowedzerwa zvaaiswa
extensions-no-match = Hapana zvowedzerwa zvinowanikwa
extensions-empty-detail = Tsvaga muChrome Web Store pamusoro uye baya Return.
extensions-no-match-detail = Edza rimwe zita kana extension ID.
extensions-on = Kwazvo
extensions-off = Kwete
extensions-enable-confirm = Bvumira { $name }?
extensions-enable-permissions = Bvumira { $name } uye tendera:

lsp-title = Maseva eMutauro
lsp-search = Tsvaga maseva emutauro, malinters, maformatters…
lsp-loading = Ichitakura katalogu…
lsp-empty = Hapana maseva emutauro anowanikwa
lsp-empty-detail = Edza mumwe mutauro, linter, kana formatter.
lsp-needs = inoda { $tool }
lsp-status-available = Iripo
lsp-status-on-path = Mu PATH
lsp-status-installing = Ichiiswa…
lsp-status-installed = Yaaiswa
lsp-status-outdated = Shanduro iripo
lsp-status-running = Ichimhanya
lsp-status-failed = Yakundikana

spaces-title = Nzvimbo
spaces-new-placeholder = Zita renzvimbo itsva
spaces-empty = Hapana nzvimbo
spaces-default-name = Nzvimbo { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } matab
}
spaces-delete = Dzima nzvimbo

team-title = Timu
team-just-you = Iwe woga munzvimbo ino
team-agents = { $count ->
    [one] Iwe ne1 mugari
   *[other] Iwe ne{ $count } vagari
}
team-empty = Hapana munhu pano zvino
team-you = Iwe
team-agent = Mugari

services-title = Masevhisi Emumashure
services-processes = { $count ->
    [one] 1 basa
   *[other] { $count } mabasa
}
services-kill-all = Dzima Zvose
services-not-running = Sevhisi haina kushanda
services-start-with = Tanga ne:
services-empty = Hapana mabasa anoshanda
services-filter = Sarudza mabasa…
services-no-match = Hapana mabasa anowanikwa
services-connected = Yakabatana
services-disconnected = Yakabviswa
services-attached = yakabatwa
services-kill = Dzima
services-memory = Ndangariro
services-size = Ukuru
services-shell = Shell

error-title = Kukanganisa

history-search = Tsvaga nhoroondo
history-clear-all = Bvisa zvose
history-clear-confirm = Bvisa nhoroondo yose?
history-clear-warning = Izvi hazvigadzirisiki.
history-cancel = Kanzura
history-today = Nhasi
history-yesterday = Nezuro
history-days-ago = { $count } mazuva apfuura
history-day-offset = Zuva -{ $count }

settings-title = Zvigadziriso
settings-loading = Ichitakura zvigadziriso…
settings-stored = Yakachengeterwa mu~/.vmux/settings.ron
settings-other = Zvimwe
settings-software-update = Shanduro yeSoftware
settings-check-updates = Tarisa Shanduro
settings-check-updates-hint = Inotarisa otomatiki pakutanga nepaawa rega rega Auto-update inobvumirwa.
settings-update-unavailable = Haipo
settings-update-unavailable-hint = Updater hairimo mubuild ino.
settings-update-checking = Ichitarisa…
settings-update-checking-hint = Ichitarisa shanduro…
settings-update-check-again = Tarisa Zvakare
settings-update-current = Vmux iri pachiyero.
settings-update-downloading = Ichidownload…
settings-update-downloading-hint = Ichidownload Vmux { $version }…
settings-update-installing = Ichiisa…
settings-update-installing-hint = Ichiisa Vmux { $version }…
settings-update-ready = Shanduro Yakagadzirira
settings-update-ready-hint = Vmux { $version } yakagadzirira. Tambira zvakare kugamuchira.
settings-update-try-again = Edza Zvakare
settings-update-failed = Kusakwanisa kutarisa shanduro.
settings-item = Chinhu
settings-item-number = Chinhu { $number }
settings-press-key = Baya kiyi…
settings-saved = Yakachengetwa
settings-record-key = Dzvanya kurekodhera kiyi combo itsva

tray-open-window = Vhura Hwindo
tray-close-window = Vhara Hwindo
tray-pause-recording = Mira Pakati Kurekodha
tray-resume-recording = Pindazve Kurekodha
tray-finish-recording = Pedzisa Kurekodha
tray-quit = Buda Vmux

composer-attach-files = Batanidza mafaili (/upload)
composer-remove-attachment = Bvisa chinhu chakabatanidzwa

layout-back = Shure
layout-forward = Mberi
layout-reload = Rodha Zvakare
layout-bookmark-page = Bookmark peji ino
layout-remove-bookmark = Bvisa bookmark
layout-pin-page = Bika peji ino
layout-unpin-page = Bvisa kubika peji ino
layout-manage-extensions = Gadzirisa zvowedzerwa
layout-new-stack = Stack Itsva
layout-close-tab = Vhara tab
layout-bookmark = Bookmark
layout-pin = Bika
layout-new-tab = Tab itsva
layout-team = Timu

command-switch-space = Shandura nzvimbo…
command-search-ask = Tsvaga kana bvunza…
command-new-tab-placeholder = Tsvaga kana nyora URL, kana sarudza Terminal…
command-placeholder = Nyora URL, tsvaga matab, kana > yemirayiro…
command-composer-placeholder = Nyora / yemirayiro kana @ yemamedia
command-send = Tumira (Enter)
command-terminal = Terminal
command-open-terminal = Vhura muTerminal
command-stack = Stack
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } matab
}
command-prompt = Murayiro
command-new-tab = Tab itsva
command-search = Tsvaga
command-open-value = Vhura "{ $value }"
command-search-value = Tsvaga "{ $value }"

schema-appearance = Kuonekwa
schema-general = Zvakajairika
schema-layout = Layout
schema-layout-detail = Hwindo, mapane, sidebar, nering yefokasi.
schema-agent = Mugari
schema-agent-detail = Maitiro emugari nemaruramiro ezvishandiso.
schema-shortcuts = Nzira Pfupi
schema-shortcuts-detail = Onai chete. Gadzirisa settings.ron pakare kuchinja mabatanidzo.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Mode
schema-mode-detail = Ruvara rwemapeji eweb. Mucherechedzo unotevera system yako.
schema-device = Mucherechedzo
schema-light = Chiedza
schema-dark = Rima
schema-language = Mutauro
schema-language-detail = Shandisa system, en-US, ja, kana BCP 47 tag ipi neipi ine katalogu inopindana ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Auto-shandura
schema-auto-update-detail = Tarisa uye isa shanduro pakutanga nepaawa rega rega.
schema-startup-url = URL yekutanga
schema-startup-url-detail = Isina chinhu inhovhura murayiro webaa yemirayiro.
schema-search-engine = Injini yekutsvaga
schema-search-engine-detail = Inoshandiswa kutsvaga web kubva kuStart nebaa yemirayiro.
schema-window = Hwindo
schema-pane = Pane
schema-side-sheet = Pepa repamabviro
schema-focus-ring = Ring yefokasi
schema-run-placement = Bvumira kusandura nzvimbo yekumhanya
schema-run-placement-detail = Bvumira maajenti kusarudza mode yepane, nzira, neanchora.
schema-leader = Mutungamiriri
schema-leader-detail = Kiyi yeprefix yenzira pfupi dzechord.
schema-chord-timeout = Nguva yechord
schema-chord-timeout-detail = Milliseconds chord prefix isati yapera nguva.
schema-bindings = Mabatanidzo
schema-confirm-close = Simbisa kuvhara
schema-confirm-close-detail = Bvunza usati wavhara terminal ine basa rinoshanda.
schema-default-theme = Theme remudzimu
schema-default-theme-detail = Zita retheme inoshanda kubva murondedzero yamathemes.
