common-open = Fosgail
common-close = Dùin
common-install = Stàlaich
common-uninstall = Di-stàlaich
common-update = Ùraich
common-retry = Feuch a-rithist
common-refresh = Ath-nuadhaich
common-remove = Thoir air falbh
common-enable = Cuir an comas
common-disable = Cuir à comas
common-new = Ùr
common-active = gnìomhach
common-running = a' ruith
common-done = deiseil
common-failed = Dh'fhàillig
common-installed = Stàlaichte
common-items = { $count ->
    [one] { $count } nì
   *[other] { $count } nithean
}
start-title = Tòisich
start-tagline = Aon iarrtas. Càil sam bith, deiseil.

agents-title = Àidseantan
agents-search = Lorg àidseantan ACP agus CLI…
agents-empty = Chan eil àidseantan co-fhreagarrach ann
agents-empty-detail = Feuch ainm, rùn-àm, no ACP/CLI.
agents-install-failed = Dh'fhàillig an stàladh
agents-updating = A' ùrachadh…
agents-retrying = A' feuchainn a-rithist…
agents-preparing = A' dèanamh deas…

extensions-title = Leudachaidhean
extensions-search = Lorg leudachaidhean stàlaichte no Stòr-lìn Chrome…
extensions-relaunch = Ath-thòisich gus cur an sàs
extensions-empty = Chan eil leudachaidhean stàlaichte
extensions-no-match = Chan eil leudachaidhean co-fhreagarrach
extensions-empty-detail = Lorg Stòr-lìn Chrome gu h-àrd agus brùth Return.
extensions-no-match-detail = Feuch ainm eile no ID leudachaidh.
extensions-on = Air
extensions-off = Dheth
extensions-enable-confirm = { $name } a chur an comas?
extensions-enable-permissions = { $name } a chur an comas agus ceadachadh:

lsp-title = Frithealaichean Cànan
lsp-search = Lorg frithealaichean cànan, lintearan, cruthaidairean…
lsp-loading = A' luchdadh catalòg…
lsp-empty = Chan eil frithealaichean cànan co-fhreagarrach
lsp-empty-detail = Feuch cànan, lintear, no cruthaidair eile.
lsp-needs = feumaidh { $tool }
lsp-status-available = Ri fhaotainn
lsp-status-on-path = Air PATH
lsp-status-installing = A' stàladh…
lsp-status-installed = Stàlaichte
lsp-status-outdated = Ùrachadh ri fhaotainn
lsp-status-running = A' ruith
lsp-status-failed = Dh'fhàillig

spaces-title = Àiteachan
spaces-new-placeholder = Ainm àite ùir
spaces-empty = Chan eil àiteachan ann
spaces-default-name = Àite { $number }
spaces-tabs = { $count ->
    [one] 1 taba
   *[other] { $count } tabaichean
}
spaces-delete = Sguab às àite

team-title = Sgioba
team-just-you = Thusa a-mhàin san àite seo
team-agents = { $count ->
    [one] Thusa agus 1 àidseant
   *[other] Thusa agus { $count } àidseantan
}
team-empty = Chan eil duine an seo fhathast
team-you = Thusa
team-agent = Àidseant

services-title = Seirbheisean Cùl-raoin
services-processes = { $count ->
    [one] 1 pròiseas
   *[other] { $count } pròiseasan
}
services-kill-all = Marbh Uile
services-not-running = Chan eil seirbheis a' ruith
services-start-with = Tòisich le:
services-empty = Chan eil pròiseasan gnìomhach
services-filter = Sìolaidh pròiseasan…
services-no-match = Chan eil pròiseasan co-fhreagarrach
services-connected = Ceangailte
services-disconnected = Neo-cheangailte
services-attached = ceangailte
services-kill = Marbh
services-memory = Cuimhne
services-size = Meud
services-shell = Shell

error-title = Mearachd

history-search = Lorg eachdraidh
history-clear-all = Glan uile
history-clear-confirm = Glan an eachdraidh uile?
history-clear-warning = Chan urrainnear seo a neo-dhèanamh.
history-cancel = Sguir dheth
history-today = An-diugh
history-yesterday = An-dè
history-days-ago = { $count } làithean air ais
history-day-offset = Latha -{ $count }

settings-title = Roghainnean
settings-loading = A' luchdadh roghainnean…
settings-stored = Stòraichte ann an ~/.vmux/settings.ron
settings-other = Eile
settings-software-update = Ùrachadh Bathar-bog
settings-check-updates = Thoir Sùil airson Ùrachaidhean
settings-check-updates-hint = A' sgrùdadh gu fèin-ghluasadach nuair a thòisichear agus gach uair nuair tha Fèin-ùrachadh an comas.
settings-update-unavailable = Neo-fhaighinn
settings-update-unavailable-hint = Chan eil ùraichear san togail seo.
settings-update-checking = A' sgrùdadh…
settings-update-checking-hint = A' sgrùdadh airson ùrachaidhean…
settings-update-check-again = Sgrùd a-rithist
settings-update-current = Tha Vmux suas ri dàta.
settings-update-downloading = A' luchdadh a-nuas…
settings-update-downloading-hint = A' luchdadh a-nuas Vmux { $version }…
settings-update-installing = A' stàladh…
settings-update-installing-hint = A' stàladh Vmux { $version }…
settings-update-ready = Ùrachadh Deiseil
settings-update-ready-hint = Tha Vmux { $version } deiseil. Ath-thòisich gus cur an sàs.
settings-update-try-again = Feuch a-rithist
settings-update-failed = Neo-chomasach sùil a thoirt airson ùrachaidhean.
settings-item = Nì
settings-item-number = Nì { $number }
settings-press-key = Brùth iuchair…
settings-saved = Sàbhailte
settings-record-key = Cliog gus iuchair-chord ùr a chlàradh

tray-open-window = Fosgail Uinneag
tray-close-window = Dùin Uinneag
tray-pause-recording = Cuir Clàradh air Stad
tray-resume-recording = Lean air Clàradh
tray-finish-recording = Crìochnaich Clàradh
tray-quit = Fàg Vmux

composer-attach-files = Ceangail faidhlichean (/upload)
composer-remove-attachment = Thoir air falbh ceangal

layout-back = Air ais
layout-forward = Air adhart
layout-reload = Ath-luchdaich
layout-bookmark-page = Comharraich an duilleag seo
layout-remove-bookmark = Thoir air falbh comharra
layout-pin-page = Prìnich an duilleag seo
layout-unpin-page = Di-phrìnich an duilleag seo
layout-manage-extensions = Stiùirich leudachaidhean
layout-new-stack = Cruach Ùr
layout-close-tab = Dùin taba
layout-bookmark = Comharra
layout-pin = Prìn
layout-new-tab = Taba ùr
layout-team = Sgioba

command-switch-space = Atharraich àite…
command-search-ask = Lorg no faighnich…
command-new-tab-placeholder = Lorg no clò-sgrìobh URL, no tagh Terminal…
command-placeholder = Clò-sgrìobh URL, lorg tabaichean, no > airson òrdughan…
command-composer-placeholder = Clò-sgrìobh / airson òrdughan no @ airson meadhanan
command-send = Cuir (Enter)
command-terminal = Terminal
command-open-terminal = Fosgail ann an Terminal
command-stack = Cruach
command-tabs = { $count ->
    [one] 1 taba
   *[other] { $count } tabaichean
}
command-prompt = Loidhne-òrdugh
command-new-tab = Taba ùr
command-search = Lorg
command-open-value = Fosgail "{ $value }"
command-search-value = Lorg "{ $value }"

schema-appearance = Coltas
schema-general = Coitcheann
schema-layout = Clàr-aghaidh
schema-layout-detail = Uinneag, panaichean, cliath-taoibh, agus fàinne fòcais.
schema-agent = Àidseant
schema-agent-detail = Giùlan àidseant agus ceadan innealan.
schema-shortcuts = Ath-ghoiridean
schema-shortcuts-detail = Seallaidh leughaidh a-mhàin. Deasaich settings.ron gu dìreach gus ceanglaichean atharrachadh.
schema-terminal = Terminal
schema-browser = Brabhsair
schema-mode = Modh
schema-mode-detail = Sgeama dathan airson duilleagan lìn. Tha an inneal a' leantainn do shiostam.
schema-device = Inneal
schema-light = Soilleir
schema-dark = Dorcha
schema-language = Cànan
schema-language-detail = Cleachd siostam, en-US, ja, no taga BCP 47 sam bith le catalòg ~/.vmux/locales/<tag>.ftl co-fhreagarrach.
schema-auto-update = Fèin-ùrachadh
schema-auto-update-detail = Thoir sùil airson ùrachaidhean agus stàlaich iad nuair a thòisichear agus gach uair.
schema-startup-url = URL Tòiseachaidh
schema-startup-url-detail = Ma tha falamh, fosgailte am bàr òrdugh.
schema-search-engine = Inneal-luirg
schema-search-engine-detail = Air a chleachdadh airson luirg-lìn bho Tòiseachadh agus am bàr òrdugh.
schema-window = Uinneag
schema-pane = Pana
schema-side-sheet = Duilleag-taoibh
schema-focus-ring = Fàinne fòcais
schema-run-placement = Ceadaich gnàthachadh àite ruith
schema-run-placement-detail = Leig le àidseantan modh pana ruith, stiùireadh, agus acair a thaghadh.
schema-leader = Ceannard
schema-leader-detail = Prìomh iuchair airson geàrr-iuchraichean chord.
schema-chord-timeout = Ceann-ùine chord
schema-chord-timeout-detail = Milliseconds mus falbh prìomh-iuchair chord.
schema-bindings = Ceanglaichean
schema-confirm-close = Dearbhaich dùnadh
schema-confirm-close-detail = Faighnich mus dùin thu terminal le pròiseas a' ruith.
schema-default-theme = Cuspair bunaiteach
schema-default-theme-detail = Ainm a' chuspair gnìomhach bhon liosta chuspairean.
