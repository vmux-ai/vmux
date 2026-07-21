common-open = Fosgail
common-close = Dùin
common-install = Stàlaich
common-uninstall = Dì-stàlaich
common-update = Ùraich
common-retry = Feuch ris a-rithist
common-refresh = Ath-nuadhaich
common-remove = Thoir air falbh
common-enable = Cuir an comas
common-disable = Cuir à comas
common-new = Ùr
common-active = gnìomhach
common-running = a’ ruith
common-done = deiseil
common-failed = Dh’fhàillig
common-installed = Stàlaichte
common-items = { $count ->
    [one] { $count } nì
   *[other] { $count } nithean
}
start-title = Tòisich
start-tagline = Aon phrompt. Rud sam bith, dèanta.

agents-title = Àidseantan
agents-search = Lorg àidseantan ACP is CLI…
agents-empty = Chan eil àidseantan co-ionnan ann
agents-empty-detail = Feuch ainm, runtime, no ACP/CLI.
agents-install-failed = Dh’fhàillig an stàladh
agents-updating = Ag ùrachadh…
agents-retrying = A’ feuchainn ris a-rithist…
agents-preparing = Ag ullachadh…

extensions-title = Leudachain
extensions-search = Lorg sna stàlaichte no ann an Chrome Web Store…
extensions-relaunch = Cuir air bhog a-rithist gus a chur an sàs
extensions-empty = Chan eil leudachan stàlaichte ann
extensions-no-match = Chan eil leudachain co-ionnan ann
extensions-empty-detail = Lorg sa Chrome Web Store gu h-àrd is brùth Return.
extensions-no-match-detail = Feuch ainm eile no ID leudachain.
extensions-on = Air
extensions-off = Dheth
extensions-enable-confirm = A bheil thu airson { $name } a chur an comas?
extensions-enable-permissions = Cuir { $name } an comas is ceadaich:

lsp-title = Frithealaichean cànain
lsp-search = Lorg frithealaichean cànain, linters, formatters…
lsp-loading = A’ luchdadh a’ chatalog…
lsp-empty = Chan eil frithealaichean cànain co-ionnan ann
lsp-empty-detail = Feuch cànan, linter, no formatter eile.
lsp-needs = feumaidh { $tool }
lsp-status-available = Ri fhaighinn
lsp-status-on-path = Air PATH
lsp-status-installing = A’ stàladh…
lsp-status-installed = Stàlaichte
lsp-status-outdated = Tha ùrachadh ri fhaighinn
lsp-status-running = A’ ruith
lsp-status-failed = Dh’fhàillig

spaces-title = Àitichean
spaces-new-placeholder = Ainm àite ùir
spaces-empty = Chan eil àiteachan ann
spaces-default-name = Àite { $number }
spaces-tabs = { $count ->
    [one] 1 taba
   *[other] { $count } tabaichean
}
spaces-delete = Sguab às an t-àite

team-title = Sgioba
team-just-you = ’S tusa a-mhàin san àite seo
team-agents = { $count ->
    [one] Thu fhèin is 1 àidseant
   *[other] Thu fhèin is { $count } àidseantan
}
team-empty = Chan eil duine an seo fhathast
team-you = Thusa
team-agent = Àidseant

services-title = Seirbheisean cùlaibh
services-processes = { $count ->
    [one] 1 phròiseas
   *[other] { $count } pròiseasan
}
services-kill-all = Cuir crìoch orra uile
services-not-running = Chan eil an t-seirbheis a’ ruith
services-start-with = Tòisich le:
services-empty = Chan eil pròiseasan gnìomhach ann
services-filter = Criathraich pròiseasan…
services-no-match = Chan eil pròiseasan co-ionnan ann
services-connected = Ceangailte
services-disconnected = Dì-cheangailte
services-attached = ceangailte ris
services-kill = Cuir crìoch air
services-memory = Cuimhne
services-size = Meud
services-shell = Slige

error-title = Mearachd

history-search = Lorg san eachdraidh
history-clear-all = Falamhaich a h-uile càil
history-clear-confirm = A bheil thu airson an eachdraidh gu lèir fhalamhachadh?
history-clear-warning = Cha ghabh seo a neo-dhèanamh.
history-cancel = Sguir dheth
history-today = An-diugh
history-yesterday = An-dè
history-days-ago = { $count } làithean air ais
history-day-offset = Latha -{ $count }

settings-title = Roghainnean
settings-loading = A’ luchdadh nan roghainnean…
settings-stored = Air a stòradh ann an ~/.vmux/settings.ron
settings-other = Eile
settings-software-update = Ùrachadh bathair-bhog
settings-check-updates = Lorg ùrachaidhean
settings-check-updates-hint = Thèid sgrùdadh gu fèin-obrachail aig cur air bhog agus gach uair a thìde nuair a bhios Auto-update an comas.
settings-update-unavailable = Chan eil ri fhaighinn
settings-update-unavailable-hint = Chan eil an t-ùraichear sa build seo.
settings-update-checking = A’ sgrùdadh…
settings-update-checking-hint = A’ lorg ùrachaidhean…
settings-update-check-again = Sgrùd a-rithist
settings-update-current = Tha Vmux cho ùr ’s a ghabhas.
settings-update-downloading = A’ luchdadh a-nuas…
settings-update-downloading-hint = A’ luchdadh Vmux { $version } a-nuas…
settings-update-installing = A’ stàladh…
settings-update-installing-hint = A’ stàladh Vmux { $version }…
settings-update-ready = Ùrachadh deiseil
settings-update-ready-hint = Tha Vmux { $version } deiseil. Ath-thòisich gus a chur an sàs.
settings-update-try-again = Feuch a-rithist
settings-update-failed = Cha b’ urrainn dhuinn ùrachaidhean a lorg.
settings-item = Nì
settings-item-number = Nì { $number }
settings-press-key = Brùth iuchair…
settings-saved = Air a shàbhaladh
settings-record-key = Briog gus combo iuchrach ùr a chlàradh

tray-open-window = Fosgail uinneag
tray-close-window = Dùin uinneag
tray-pause-recording = Cuir an clàradh ’na stad
tray-resume-recording = Lean air a’ chlàradh
tray-finish-recording = Crìochnaich an clàradh
tray-quit = Fàg Vmux

composer-attach-files = Ceangail faidhlichean (/upload)
composer-remove-attachment = Thoir an ceanglachan air falbh

layout-back = Air ais
layout-forward = Air adhart
layout-reload = Ath-luchdaich
layout-bookmark-page = Cuir comharra-leabhair ris an duilleag seo
layout-remove-bookmark = Thoir an comharra-leabhair air falbh
layout-pin-page = Prìnich an duilleag seo
layout-unpin-page = Dì-phrìnich an duilleag seo
layout-manage-extensions = Stiùirich leudachain
layout-new-stack = Stac ùr
layout-close-tab = Dùin an taba
layout-bookmark = Comharra-leabhair
layout-pin = Prìnich
layout-new-tab = Taba ùr
layout-team = Sgioba

command-switch-space = Atharraich àite…
command-search-ask = Lorg no faighnich…
command-new-tab-placeholder = Lorg no sgrìobh URL, no tagh Terminal…
command-placeholder = Sgrìobh URL, lorg tabaichean, no > airson àitheantan…
command-composer-placeholder = Sgrìobh / airson àitheantan no @ airson meadhanan
command-send = Cuir (Enter)
command-terminal = Terminal
command-open-terminal = Fosgail ann an Terminal
command-stack = Stac
command-tabs = { $count ->
    [one] 1 taba
   *[other] { $count } tabaichean
}
command-prompt = Prompt
command-new-tab = Taba ùr
command-search = Lorg
command-open-value = Fosgail “{ $value }”
command-search-value = Lorg “{ $value }”

schema-appearance = Coltas
schema-general = Coitcheann
schema-layout = Co-dhealbhadh
schema-layout-detail = Uinneag, panaichean, bàr-taoibh, is fàinne fòcais.
schema-agent = Àidseant
schema-agent-detail = Giùlan an àidseint is ceadan innealan.
schema-shortcuts = Ath-ghoiridean
schema-shortcuts-detail = Sealladh ri leughadh a-mhàin. Deasaich settings.ron gu dìreach gus bindings atharrachadh.
schema-terminal = Terminal
schema-browser = Brabhsair
schema-mode = Modh
schema-mode-detail = Sgeama dhathan airson duilleagan-lìn. Leanaidh an t-inneal an siostam agad.
schema-device = Inneal
schema-light = Soilleir
schema-dark = Dorcha
schema-language = Cànan
schema-language-detail = Cleachd an siostam, en-US, ja, no taga BCP 47 sam bith le catalog ~/.vmux/locales/<tag>.ftl co-fhreagarrach.
schema-auto-update = Auto-update
schema-auto-update-detail = Lorg is stàlaich ùrachaidhean aig cur air bhog agus gach uair a thìde.
schema-startup-url = URL tòiseachaidh
schema-startup-url-detail = Ma tha e falamh, fosglaidh prompt bàr nan àitheantan.
schema-search-engine = Einnsean-luirg
schema-search-engine-detail = Air a chleachdadh airson luirg-lìn bho Tòisich agus bho bhàr nan àitheantan.
schema-window = Uinneag
schema-pane = Pana
schema-side-sheet = Siota-taoibh
schema-focus-ring = Fàinne fòcais
schema-run-placement = Ceadaich tar-àithne air àite ruith
schema-run-placement-detail = Leig le àidseantan modh, stiùir agus acair a’ phana ruith a thaghadh.
schema-leader = Leader
schema-leader-detail = Iuchair ro-leasachain airson ath-ghoiridean chord.
schema-chord-timeout = Crìoch-ùine chord
schema-chord-timeout-detail = Mille-dhiog mus falbh ro-leasachan chord à ùine.
schema-bindings = Bindings
schema-confirm-close = Dearbh dùnadh
schema-confirm-close-detail = Faighnich mus dùin thu terminal le pròiseas a’ ruith.
schema-default-theme = Cuspair bunaiteach
schema-default-theme-detail = Ainm a’ chuspair ghnìomhaich bho liosta nan cuspairean.
