common-open = Tatala
common-close = Tapuni
common-install = Faʻapipiʻi
common-uninstall = Faʻaleaogaina
common-update = Faʻafouina
common-retry = Taumafai Toe
common-refresh = Toe Faʻafoʻi
common-remove = Aveese
common-enable = Faʻaagaina
common-disable = Taofi
common-new = Fou
common-active = faʻagaioi
common-running = faagaioi
common-done = maeʻa
common-failed = Paʻu
common-installed = Faʻapipiʻiina
common-items = { $count ->
    [one] { $count } mea
   *[other] { $count } mea
}
start-title = Amata
start-tagline = E tasi le fautuaga. Soʻo se mea, maeʻa.

agents-title = Agents
agents-search = Saili ACP ma CLI agents…
agents-empty = Leai ni agents fetaui
agents-empty-detail = Taumafai i se igoa, runtime, poʻo ACP/CLI.
agents-install-failed = Paʻu le faʻapipiʻiina
agents-updating = Faʻafouina…
agents-retrying = Taumafai toe…
agents-preparing = Saunia…

extensions-title = Faʻalauteleina
extensions-search = Saili faʻapipiʻiina poʻo Chrome Web Store…
extensions-relaunch = Toe amata e faʻaoga
extensions-empty = Leai ni faʻalauteleina faʻapipiʻiina
extensions-no-match = Leai ni faʻalauteleina fetaui
extensions-empty-detail = Saili le Chrome Web Store i luga ma tʻoomi Return.
extensions-no-match-detail = Taumafai i se isi igoa poʻo ID o le faʻalauteleina.
extensions-on = I Luga
extensions-off = I Lalo
extensions-enable-confirm = Faʻaagaina { $name }?
extensions-enable-permissions = Faʻaagaina { $name } ma faʻatagaina:

lsp-title = Language Servers
lsp-search = Saili language servers, linters, formatters…
lsp-loading = Laʻu mai le catalog…
lsp-empty = Leai ni language servers fetaui
lsp-empty-detail = Taumafai i se isi gagana, linter, poʻo formatter.
lsp-needs = manaʻomia { $tool }
lsp-status-available = Avanoa
lsp-status-on-path = I le PATH
lsp-status-installing = Faʻapipiʻi…
lsp-status-installed = Faʻapipiʻiina
lsp-status-outdated = Avanoa le faʻafouina
lsp-status-running = Faagaioi
lsp-status-failed = Paʻu

spaces-title = Nofoaga
spaces-new-placeholder = Igoa o le nofoaga fou
spaces-empty = Leai ni nofoaga
spaces-default-name = Nofoaga { $number }
spaces-tabs = { $count ->
    [one] 1 laulau
   *[other] { $count } laulau
}
spaces-delete = Tape le nofoaga

team-title = Vaega
team-just-you = O oe naʻo oe i lenei nofoaga
team-agents = { $count ->
    [one] Oe ma le 1 agent
   *[other] Oe ma { $count } agents
}
team-empty = Leai se tasi iinei
team-you = Oe
team-agent = Agent

services-title = Auaunaga Tua
services-processes = { $count ->
    [one] 1 faagaioiga
   *[other] { $count } faagaioiga
}
services-kill-all = Faʻaumatiaina Uma
services-not-running = E le faagaioi le auaunaga
services-start-with = Amata ma:
services-empty = Leai ni faagaioiga faagaioi
services-filter = Fasi faagaioiga…
services-no-match = Leai ni faagaioiga fetaui
services-connected = Fesoʻotaʻi
services-disconnected = Motusia
services-attached = faʻapipiʻiina
services-kill = Faʻaumati
services-memory = Faamanatu
services-size = Tele
services-shell = Shell

error-title = Sese

history-search = Saili talafaasolopito
history-clear-all = Aveese uma
history-clear-confirm = Aveese talafaasolopito uma?
history-clear-warning = E le mafai ona toe faʻafoʻi.
history-cancel = Faʻaleaogaina
history-today = Aso nei
history-yesterday = Ananafi
history-days-ago = { $count } aso ua tuanaʻi
history-day-offset = Aso -{ $count }

settings-title = Faʻatulagaga
settings-loading = Laʻu mai faʻatulagaga…
settings-stored = Teuina i ~/.vmux/settings.ron
settings-other = Isi
settings-software-update = Faʻafouina o Polokalame
settings-check-updates = Siaki Faʻafouina
settings-check-updates-hint = Siakiina otometi i le amataga ma i le itula taʻitasi pe a faʻagaioi le Faʻafouina-otometi.
settings-update-unavailable = E le avanoa
settings-update-unavailable-hint = E le iai le faʻafouina i lenei fausiaina.
settings-update-checking = Siakiina…
settings-update-checking-hint = Siakiina faʻafouina…
settings-update-check-again = Siaki Toe
settings-update-current = Ua faʻafouina Vmux.
settings-update-downloading = Laʻu mai…
settings-update-downloading-hint = Laʻu mai Vmux { $version }…
settings-update-installing = Faʻapipiʻi…
settings-update-installing-hint = Faʻapipiʻi Vmux { $version }…
settings-update-ready = Saunia le Faʻafouina
settings-update-ready-hint = Ua saunia Vmux { $version }. Amata toe e faʻaoga.
settings-update-try-again = Taumafai Toe
settings-update-failed = E le mafai ona siaki faʻafouina.
settings-item = Mea
settings-item-number = Mea { $number }
settings-press-key = Tʻoomi se ki…
settings-saved = Faasaoina
settings-record-key = Kiliki e faʻamauina se ki fou

tray-open-window = Tatala le Faamalumalu
tray-close-window = Tapuni le Faamalumalu
tray-pause-recording = Taofi le Faʻamauina
tray-resume-recording = Faʻaauina le Faʻamauina
tray-finish-recording = Faʻauma le Faʻamauina
tray-quit = Tuʻu Vmux

composer-attach-files = Faʻapipiʻi faila (/upload)
composer-remove-attachment = Aveese mea faʻapipiʻiina

layout-back = Toe Alu
layout-forward = Alu i Luma
layout-reload = Toe Laʻu
layout-bookmark-page = Faʻamaumau lenei itulau
layout-remove-bookmark = Aveese faʻamaumauina
layout-pin-page = Faʻapini lenei itulau
layout-unpin-page = Faʻaleaogaina le faʻapiniina
layout-manage-extensions = Puleaina faʻalauteleina
layout-new-stack = Stack Fou
layout-close-tab = Tapuni le laulau
layout-bookmark = Faʻamaumau
layout-pin = Pini
layout-new-tab = Laulau fou
layout-team = Vaega

command-switch-space = Suia nofoaga…
command-search-ask = Saili poʻo fesili…
command-new-tab-placeholder = Saili poʻo taina se URL, poʻo filifili Terminal…
command-placeholder = Taina se URL, saili laulau, poʻo > mo poloaiga…
command-composer-placeholder = Taina / mo poloaiga poʻo @ mo faʻailoga
command-send = Lafo (Enter)
command-terminal = Terminal
command-open-terminal = Tatala i Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] 1 laulau
   *[other] { $count } laulau
}
command-prompt = Fautuaga
command-new-tab = Laulau fou
command-search = Saili
command-open-value = Tatala "{ $value }"
command-search-value = Saili "{ $value }"

schema-appearance = Foliga
schema-general = Lautele
schema-layout = Faʻasologa
schema-layout-detail = Faamalumalu, pane, sidebar, ma ato faatonu.
schema-agent = Agent
schema-agent-detail = Amio o le agent ma faʻatagaga o meafaigaluega.
schema-shortcuts = Ala Pupuu
schema-shortcuts-detail = Vaʻai naʻo. Faʻatusaʻo settings.ron tuusaʻo e suia fusia.
schema-terminal = Terminal
schema-browser = Tirotiro
schema-mode = Auala
schema-mode-detail = Fuʻa lanu mo itulau o le upega. E mulimuli le masini i lou faiga.
schema-device = Masini
schema-light = Malamalama
schema-dark = Pogisa
schema-language = Gagana
schema-language-detail = Faʻaaoga le faiga, en-US, ja, poʻo soʻo se BCP 47 faʻailoga ma le fetaui ~/.vmux/locales/<tag>.ftl catalog.
schema-auto-update = Faʻafouina-otometi
schema-auto-update-detail = Siaki ma faʻapipiʻi faʻafouina i le amataga ma i le itula taʻitasi.
schema-startup-url = URL Amataga
schema-startup-url-detail = Tatala le faatofi poloaiga pe a avanoa.
schema-search-engine = Masini Saili
schema-search-engine-detail = Faʻaaogaina mo sailiga o le upega mai Start ma le faatofi poloaiga.
schema-window = Faamalumalu
schema-pane = Pane
schema-side-sheet = Laupepa Tafatafa
schema-focus-ring = Taʻavale Faatonu
schema-run-placement = Faʻataga suiga o tulaga faagaioiga
schema-run-placement-detail = Faʻataga agents e filifili le auala, itu, ma le faʻafouina o le pane.
schema-leader = Taʻitaʻi
schema-leader-detail = Ki muamua mo faʻailo ki.
schema-chord-timeout = Taimi Faʻauma Chord
schema-chord-timeout-detail = Milliseconds aʻo leʻi faʻauma le ki muamua o le chord.
schema-bindings = Fusia
schema-confirm-close = Faʻamaonia le Tapuniina
schema-confirm-close-detail = Fesili aʻo tapunia se terminal ma se faagaioiga faagaioi.
schema-default-theme = Autu Masani
schema-default-theme-detail = Igoa o le autu faagaioi mai le lisi o autu.
