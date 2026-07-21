common-open = Vhura
common-close = Vhara
common-install = Isa
common-uninstall = Bvisa
common-update = Gadziridza
common-retry = Edzazve
common-refresh = Vandudza
common-remove = Bvisa
common-enable = Batidza
common-disable = Dzima
common-new = Itsva
common-active = iri kushanda
common-running = iri kumhanya
common-done = zvaitwa
common-failed = Zvaramba
common-installed = Yakaiswa
common-items = { $count ->
    [one] chinhu { $count }
   *[other] zvinhu { $count }
}
start-title = Tanga
start-tagline = Murayiro mumwe. Zvese zvaitwa.

agents-title = Maagent
agents-search = Tsvaga maagent eACP neCLI…
agents-empty = Hapana maagent anoenderana
agents-empty-detail = Edza zita, runtime, kana ACP/CLI.
agents-install-failed = Kuisa kwaramba
agents-updating = Kugadziridza…
agents-retrying = Kuedzazve…
agents-preparing = Kugadzirira…

extensions-title = Maextension
extensions-search = Tsvaga akaiswa kana muChrome Web Store…
extensions-relaunch = Tangazve kuti zvishande
extensions-empty = Hapana maextension akaiswa
extensions-no-match = Hapana maextension anoenderana
extensions-empty-detail = Tsvaga muChrome Web Store pamusoro wodzvanya Return.
extensions-no-match-detail = Edza rimwe zita kana extension ID.
extensions-on = Yakabatidzwa
extensions-off = Yakadzimwa
extensions-enable-confirm = Batidza { $name }?
extensions-enable-permissions = Batidza { $name } wobvumira:

lsp-title = Maseva Emitauro
lsp-search = Tsvaga maseva emitauro, linters, mafomata…
lsp-loading = Kuverenga catalog…
lsp-empty = Hapana maseva emitauro anoenderana
lsp-empty-detail = Edza mumwe mutauro, linter, kana fomata.
lsp-needs = inoda { $tool }
lsp-status-available = Iripo
lsp-status-on-path = Iri paPATH
lsp-status-installing = Kuisa…
lsp-status-installed = Yakaiswa
lsp-status-outdated = Gadziridzo iripo
lsp-status-running = Iri kumhanya
lsp-status-failed = Yaramba

spaces-title = Nzvimbo
spaces-new-placeholder = Zita renzvimbo itsva
spaces-empty = Hapana nzvimbo
spaces-default-name = Nzvimbo { $number }
spaces-tabs = { $count ->
    [one] tabhu 1
   *[other] matabhu { $count }
}
spaces-delete = Dzima nzvimbo

team-title = Chikwata
team-just-you = Uri wega munzvimbo iyi
team-agents = { $count ->
    [one] Iwe neagent 1
   *[other] Iwe nemaagent { $count }
}
team-empty = Hapana munhu pano parizvino
team-you = Iwe
team-agent = Agent

services-title = Masevhisi Ekumashure
services-processes = { $count ->
    [one] process 1
   *[other] maprocess { $count }
}
services-kill-all = Misa Zvese Nechisimba
services-not-running = Sevhisi haisi kumhanya
services-start-with = Tanga ne:
services-empty = Hapana maprocess ari kushanda
services-filter = Sefa maprocess…
services-no-match = Hapana maprocess anoenderana
services-connected = Yakabatana
services-disconnected = Haina kubatana
services-attached = yakabatanidzwa
services-kill = Misa nechisimba
services-memory = Memory
services-size = Saizi
services-shell = Shell

error-title = Kukanganisa

history-search = Tsvaga munhoroondo
history-clear-all = Bvisa zvese
history-clear-confirm = Bvisa nhoroondo yese?
history-clear-warning = Izvi hazvigoni kudzoserwa.
history-cancel = Kanzura
history-today = Nhasi
history-yesterday = Nezuro
history-days-ago = mazuva { $count } apfuura
history-day-offset = Zuva -{ $count }

settings-title = Zvirongwa
settings-loading = Kuverenga zvirongwa…
settings-stored = Zvakachengetwa mu ~/.vmux/settings.ron
settings-other = Zvimwe
settings-software-update = Gadziridzo yeSoftware
settings-check-updates = Tarisa Gadziridzo
settings-check-updates-hint = Inotarisa yega pakuvhura uye awa rega rega kana Auto-update yakabatidzwa.
settings-update-unavailable = Haisi kuwanikwa
settings-update-unavailable-hint = Updater haina kuisirwa mubuild iyi.
settings-update-checking = Kutarisa…
settings-update-checking-hint = Kutarisa magadziridzo…
settings-update-check-again = Tarisa Zvakare
settings-update-current = Vmux iri pagadziridzo yazvino.
settings-update-downloading = Kudhaunirodha…
settings-update-downloading-hint = Kudhaunirodha Vmux { $version }…
settings-update-installing = Kuisa…
settings-update-installing-hint = Kuisa Vmux { $version }…
settings-update-ready = Gadziridzo Yagadzirira
settings-update-ready-hint = Vmux { $version } yagadzirira. Tangazve kuti ishande.
settings-update-try-again = Edzazve
settings-update-failed = Hatina kukwanisa kutarisa magadziridzo.
settings-item = Chinhu
settings-item-number = Chinhu { $number }
settings-press-key = Dzvanya kiyi…
settings-saved = Zvachengetwa
settings-record-key = Dzvanya kuti urekodhe combo itsva yemakiyi

tray-open-window = Vhura Hwindo
tray-close-window = Vhara Hwindo
tray-pause-recording = Misa Kurekodha Kwechinguva
tray-resume-recording = Enderera Kurekodha
tray-finish-recording = Pedza Kurekodha
tray-quit = Buda muVmux

composer-attach-files = Batanidza mafaera (/upload)
composer-remove-attachment = Bvisa chakabatanidzwa

layout-back = Dzokera
layout-forward = Enda mberi
layout-reload = Rodhazve
layout-bookmark-page = Chengeta peji iri
layout-remove-bookmark = Bvisa bookmark
layout-pin-page = Pinza peji iri
layout-unpin-page = Bvisa pin yepeji iri
layout-manage-extensions = Tonga maextension
layout-new-stack = Stack Itsva
layout-close-tab = Vhara tabhu
layout-bookmark = Bookmark
layout-pin = Pinza
layout-new-tab = Tabhu itsva
layout-team = Chikwata

command-switch-space = Chinja nzvimbo…
command-search-ask = Tsvaga kana kubvunza…
command-new-tab-placeholder = Tsvaga kana nyora URL, kana sarudza Terminal…
command-placeholder = Nyora URL, tsvaga matabhu, kana > yemirairo…
command-composer-placeholder = Nyora / yemirairo kana @ yemedia
command-send = Tumira (Enter)
command-terminal = Terminal
command-open-terminal = Vhura muTerminal
command-stack = Stack
command-tabs = { $count ->
    [one] tabhu 1
   *[other] matabhu { $count }
}
command-prompt = Murayiro
command-new-tab = Tabhu itsva
command-search = Tsvaga
command-open-value = Vhura “{ $value }”
command-search-value = Tsvaga “{ $value }”

schema-appearance = Chitarisiko
schema-general = Zvakajairika
schema-layout = Marongerwo
schema-layout-detail = Hwindo, mapani, sidebar, uye focus ring.
schema-agent = Agent
schema-agent-detail = Maitiro eagent nemvumo yezvishandiso.
schema-shortcuts = Mashortcut
schema-shortcuts-detail = Kuona chete. Rongedza settings.ron zvakananga kuti uchinje mabinding.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Mode
schema-mode-detail = Ruvara rwemapeji ewebhu. Device inotevera system yako.
schema-device = Device
schema-light = Chiedza
schema-dark = Rima
schema-language = Mutauro
schema-language-detail = Shandisa system, en-US, ja, kana chero BCP 47 tag ine catalog inoenderana pa ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Auto-update
schema-auto-update-detail = Tarisa uye isa magadziridzo pakuvhura uye awa rega rega.
schema-startup-url = Startup URL
schema-startup-url-detail = Kana isina chinhu, inovhura prompt yebhaa remirairo.
schema-search-engine = Injini yekutsvaga
schema-search-engine-detail = Inoshandiswa pakutsvaga pawebhu kubva paTanga nebhaa remirairo.
schema-window = Hwindo
schema-pane = Pani
schema-side-sheet = Side sheet
schema-focus-ring = Focus ring
schema-run-placement = Bvumira kuchinja pekumhanyisa
schema-run-placement-detail = Rega maagent asarudze mode yepani yekumhanyisa, direction, uye anchor.
schema-leader = Leader
schema-leader-detail = Kiyi yekutanga mashortcut echord.
schema-chord-timeout = Nguva yekumirira chord
schema-chord-timeout-detail = Mamillisecond chord prefix isati yapera.
schema-bindings = Mabinding
schema-confirm-close = Simbisa kuvhara
schema-confirm-close-detail = Bvunza usati wavhara terminal ine process iri kumhanya.
schema-default-theme = Theme yekutanga
schema-default-theme-detail = Zita retheme iri kushanda kubva parondedzero yemitheme.
