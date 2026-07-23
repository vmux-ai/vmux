locale-name = Gàidhlig
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

tools-title = Innealan
tools-search = Lorg pacaidean, àidseantan, MCP, innealan cànain agus faidhlichean rèiteachaidh…
tools-open = Fosgail na h-innealan
tools-fold = Paisg na h-innealan
tools-unfold = Leudaich na h-innealan
tools-scanning = A’ sganadh innealan ionadail…
tools-no-installed = Chan eil inneal air a stàladh
tools-empty = Chan eil inneal co-ionnan ann
tools-empty-detail = Stàlaich pacaid no cuir pacaid fhaidhlichean rèiteachaidh ann an stoidhle Stow ris.
tools-apply = Cuir an sàs
tools-homebrew = Homebrew
tools-homebrew-sync = Sioncronaichidh foirmlean agus aplacaidean stàlaichte gu fèin-obrachail.
tools-open-brewfile = Fosgail Brewfile
tools-managed = air a stiùireadh
tools-provider-homebrew-formulae = Foirmlean Homebrew
tools-provider-homebrew-casks = Aplacaidean Homebrew
tools-provider-npm = Pacaidean npm
tools-provider-acp-agents = Àidseantan ACP
tools-provider-language-tools = Innealan cànain
tools-provider-mcp-servers = Frithealaichean MCP
tools-provider-dotfiles = Faidhlichean rèiteachaidh
tools-status-available = Ri fhaighinn
tools-status-missing = A dhìth
tools-status-conflict = Còmhstri
tools-forget = Dìochuimhnich
tools-manage = Stiùirich
tools-link = Ceangail
tools-unlink = Dì-cheangail
tools-import = Ion-phortaich
tools-update-count = { $count ->
    [one] 1 ùrachadh
   *[other] { $count } ùrachaidhean
}
tools-conflict-count = { $count ->
    [one] 1 chòmhstri
   *[other] { $count } còmhstrithean
}
tools-result-applied = Innealan air an cur an sàs
tools-result-imported = Innealan air an ion-phortadh
tools-result-installed = { $name } air a stàladh
tools-result-updated = { $name } air ùrachadh
tools-result-uninstalled = { $name } air a dhì-stàladh
tools-result-forgotten = { $name } air a dhìochuimhneachadh
tools-result-managed = Tha { $name } ga stiùireadh a-nis
tools-result-linked = { $name } air a cheangal
tools-result-unlinked = { $name } air a dhì-cheangal

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

settings-empty = (falamh)
settings-none = (chan eil gin)

schema-system = Siostam
schema-editor = Deasaiche
schema-recording = Clàradh
schema-radius = Rèideas
schema-padding = Iomall a-staigh
schema-gap = Beàrn
schema-width = Leud
schema-color = Dath
schema-red = Dearg
schema-green = Uaine
schema-blue = Gorm
schema-follow-files = Lean faidhlichean
schema-tidy-files = Sgioblaich faidhlichean
schema-tidy-files-max = Stairsneach sgioblachadh fhaidhlichean
schema-tidy-files-auto = Sgioblaich faidhlichean gu fèin-obrachail
schema-app-providers = Solaraichean aplacaidean
schema-provider = Solaraiche
schema-kind = Seòrsa
schema-models = Modailean
schema-acp = Àidseantan ACP
schema-id = ID
schema-name = Ainm
schema-command = Àithne
schema-arguments = Argamaidean
schema-environment = Àrainneachd
schema-working-directory = Eòlaire-obrach
schema-shell = Slige
schema-font-family = Teaghlach crutha-clò
schema-startup-directory = Eòlaire tòiseachaidh
schema-themes = Cuspairean
schema-color-scheme = Sgeama dhathan
schema-font-size = Meud crutha-clò
schema-line-height = Àirde loidhne
schema-cursor-style = Stoidhle cùrsair
schema-cursor-blink = Priobadh a' chùrsair
schema-custom-themes = Cuspairean gnàthaichte
schema-foreground = Aghaidh
schema-background = Cùlaibh
schema-cursor = Cùrsair
schema-ansi-colors = Dathan ANSI
schema-keymap = Mapa iuchraichean
schema-explorer = Rannsaichear
schema-visible = Ri fhaicinn
schema-language-servers = Frithealaichean cànain
schema-servers = Frithealaichean
schema-language-id = ID cànain
schema-root-markers = Comharran freumha
schema-output-directory = Eòlaire toraidh

menu-scene = Sealladh
menu-layout = Co-dhealbh
menu-terminal = Tèirmineal
menu-browser = Brabhsair
menu-service = Seirbheis
menu-bookmark = Comharra-lìn
menu-edit = Deasaich

layout-knowledge = Eòlas
layout-open-knowledge = Fosgail Eòlas
layout-open-welcome-knowledge = Fosgail Fàilte gu Eòlas
layout-open-path = Fosgail { $path }
layout-fold-knowledge = Paisg Eòlas
layout-unfold-knowledge = Leudaich Eòlas
layout-bookmarks = Comharran-lìn
layout-new-folder = Pasgan ùr
layout-add-to-bookmarks = Cuir ris na comharran-lìn
layout-move-to-bookmarks = Gluais gu na comharran-lìn
layout-stack-number = Staca { $number }
layout-fold-stack = Paisg an staca
layout-unfold-stack = Leudaich an staca
layout-close-stack = Dùin an staca
layout-bookmark-in = Comharra-lìn ann an { $folder }

common-cancel = Sguir dheth
common-delete = Sguab às
common-save = Sàbhail
common-rename = Ath-ainmich
common-expand = Leudaich
common-collapse = Co-theannaich
common-loading = ’Ga luchdadh…
common-error = Mearachd
common-output = Às-chur
common-pending = Ri feitheamh
common-current = làithreach
common-stop = Stad
services-command = Seirbheis Vmux
services-uptime-seconds = { $seconds }d
services-uptime-minutes = { $minutes }m { $seconds }d
services-uptime-hours = { $hours }u { $minutes }m
services-uptime-days = { $days }l { $hours }u

error-page-failed-load = Dh’fhàillig luchdadh na duilleige
error-page-not-found = Cha deach an duilleag a lorg
error-unknown-host = Òstair aplacaid Vmux neo-aithnichte: { $host }

history-title = Eachdraidh

command-new-app-chat = Cabadaich ùr { $provider }/{ $model } (Aplacaid)
command-interactive-mode-user = Sealladh > Modh eadar-ghnìomhach > Cleachdaiche
command-interactive-mode-player = Sealladh > Modh eadar-ghnìomhach > Cluicheadair
command-minimize-window = Cruth > Uinneag > Lùghdaich
command-toggle-layout = Cruth > Cruth > Toglaich an cruth
command-close-tab = Cruth > Taba > Dùin an taba
command-new-task = Cruth > Taba > Saothair ùr…
command-next-tab = Cruth > Taba > An ath thaba
command-prev-tab = Cruth > Taba > An taba roimhe
command-rename-tab = Cruth > Taba > Ath-ainmich an taba
command-tab-select-1 = Cruth > Taba > Tagh taba 1
command-tab-select-2 = Cruth > Taba > Tagh taba 2
command-tab-select-3 = Cruth > Taba > Tagh taba 3
command-tab-select-4 = Cruth > Taba > Tagh taba 4
command-tab-select-5 = Cruth > Taba > Tagh taba 5
command-tab-select-6 = Cruth > Taba > Tagh taba 6
command-tab-select-7 = Cruth > Taba > Tagh taba 7
command-tab-select-8 = Cruth > Taba > Tagh taba 8
command-tab-select-last = Cruth > Taba > Tagh an taba mu dheireadh
command-close-pane = Cruth > Leòsan > Dùin an leòsan
command-select-pane-left = Cruth > Leòsan > Tagh an leòsan clì
command-select-pane-right = Cruth > Leòsan > Tagh an leòsan deas
command-select-pane-up = Cruth > Leòsan > Tagh an leòsan os a chionn
command-select-pane-down = Cruth > Leòsan > Tagh an leòsan fodha
command-swap-pane-prev = Cruth > Leòsan > Suaip leis an leòsan roimhe
command-swap-pane-next = Cruth > Leòsan > Suaip leis an ath leòsan
command-equalize-pane-size = Cruth > Leòsan > Co-ionannaich meud nan leòsan
command-resize-pane-left = Cruth > Leòsan > Ath-mheudaich an leòsan gu clì
command-resize-pane-right = Cruth > Leòsan > Ath-mheudaich an leòsan gu deas
command-resize-pane-up = Cruth > Leòsan > Ath-mheudaich an leòsan suas
command-resize-pane-down = Cruth > Leòsan > Ath-mheudaich an leòsan sìos
command-stack-close = Cruth > Stac > Dùin an stac
command-stack-next = Cruth > Stac > An ath stac
command-stack-previous = Cruth > Stac > An stac roimhe
command-stack-reopen = Cruth > Stac > Fosgail an duilleag dhùinte a-rithist
command-stack-swap-prev = Cruth > Stac > Gluais an stac gu clì
command-stack-swap-next = Cruth > Stac > Gluais an stac gu deas
command-space-open = Cruth > Àite > Àiteachan
command-terminal-close = Tèirmineal > Dùin an tèirmineal
command-terminal-next = Tèirmineal > An ath thèirmineal
command-terminal-prev = Tèirmineal > An tèirmineal roimhe
command-terminal-clear = Tèirmineal > Falamhaich an tèirmineal
command-browser-prev-page = Brabhsair > Seòladh > Air ais
command-browser-next-page = Brabhsair > Seòladh > Air adhart
command-browser-reload = Brabhsair > Seòladh > Ath-luchdaich
command-browser-hard-reload = Brabhsair > Seòladh > Ath-luchdadh cruaidh
command-open-in-place = Brabhsair > Fosgail > Fosgail an-seo
command-open-in-new-stack = Brabhsair > Fosgail > Fosgail ann an stac ùr
command-open-in-pane-top = Brabhsair > Fosgail > Fosgail san leòsan os a chionn
command-open-in-pane-right = Brabhsair > Fosgail > Fosgail san leòsan deas
command-open-in-pane-bottom = Brabhsair > Fosgail > Fosgail san leòsan fodha
command-open-in-pane-left = Brabhsair > Fosgail > Fosgail san leòsan clì
command-open-in-new-tab = Brabhsair > Fosgail > Fosgail ann an taba ùr
command-open-in-new-space = Brabhsair > Fosgail > Fosgail ann an àite ùr
command-browser-zoom-in = Brabhsair > Sealladh > Sùm a-steach
command-browser-zoom-out = Brabhsair > Sealladh > Sùm a-mach
command-browser-zoom-reset = Brabhsair > Sealladh > Fìor-mheud
command-browser-dev-tools = Brabhsair > Sealladh > Innealan luchd-leasachaidh
command-browser-open-command-bar = Brabhsair > Bàr > Bàr nan àitheantan
command-browser-open-page-in-command-bar = Brabhsair > Bàr > Deasaich an duilleag
command-browser-open-path-bar = Brabhsair > Bàr > Seòladair slighe
command-browser-open-commands = Brabhsair > Bàr > Àitheantan
command-browser-open-history = Brabhsair > Bàr > Eachdraidh
command-service-open = Seirbheis > Fosgail monatair nan seirbheisean
command-bookmark-toggle-active = Comharra-lìn > Cuir comharra-lìn ris an duilleag
command-bookmark-pin-active = Comharra-lìn > Prìnich an duilleag

layout-tab = Taba
layout-no-stacks = Chan eil stacan ann
layout-loading = ’Ga luchdadh…
layout-no-markdown-files = Chan eil faidhlichean Markdown ann
layout-empty-folder = Pasgan falamh
layout-worktree = craobh-obrach
layout-folder-name = Ainm a’ phasgain
layout-no-pins-bookmarks = Chan eil prìneachan no comharran-lìn ann
layout-move-to = Gluais gu { $folder }
layout-bookmark-current-page = Cuir comharra-lìn ris an duilleag làithreach
layout-rename-folder = Ath-ainmich am pasgan
layout-remove-folder = Thoir am pasgan air falbh
layout-update-downloading = ’Ga luchdadh a-nuas
layout-update-installing = ’Ga stàladh…
layout-update-ready = Tha tionndadh ùr ri fhaighinn
layout-restart-update = Ath-thòisich gus ùrachadh

agent-preparing = Ag ullachadh an àidseint…
agent-send-all-queued = Cuir gach proimt sa chiutha an-dràsta (Esc)
agent-send = Cuir (Enter)
agent-ready = Deiseil nuair a bhios tusa.
agent-loading-older = A’ luchdadh seann teachdaireachdan…
agent-load-older = Luchdaich seann teachdaireachdan
agent-continued-from = Air a leantainn o { $source }
agent-older-context-omitted = chaidh co-theacsa nas sine fhàgail às
agent-interrupted = air a bhriseadh
agent-allow-tool = Ceadaich { $tool }?
agent-deny = Diùlt
agent-allow-always = Ceadaich an-còmhnaidh
agent-allow = Ceadaich
agent-loading-sessions = A’ luchdadh seiseanan…
agent-no-resumable-sessions = Cha deach seiseanan a ghabhas ath-thòiseachadh a lorg
agent-no-matching-sessions = Chan eil seiseanan a’ freagairt ann
agent-no-matching-models = Chan eil modailean a’ freagairt ann
agent-choice-help = ↑/↓ no Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Tagh pasgan tasglainn
agent-choose-repository-detail = Tagh an tasglann Git ionadail a chleachdas an t-àidseant.
agent-choosing = ’Ga thaghadh…
agent-choose-folder = Tagh pasgan
agent-queued = sa chiutha
agent-attached = Ceangailte:
agent-cancel-queued = Sguir dhen phroimt sa chiutha
agent-resume-queued = Ath-thòisich proimtan sa chiutha
agent-clear-queue = Falamhaich an ciutha
agent-send-all-now = cuir iad uile an-dràsta
agent-choose-option = Tagh roghainn gu h-àrd
agent-loading-media = A’ luchdadh mheadhanan…
agent-no-matching-media = Chan eil meadhanan a’ freagairt ann
agent-prompt-context = Co-theacsa a’ phroimt
agent-details = Mion-fhiosrachadh
agent-path = Slighe
agent-tool = Inneal
agent-server = Frithealaiche
agent-bytes = { $count } baidht
agent-worked-for = Dh’obraich e fad { $duration }
agent-worked-for-steps = { $count ->
    [one] Dh’obraich e fad { $duration } · 1 cheum
   *[other] Dh’obraich e fad { $duration } · { $count } ceuman
}
agent-tool-guardian-review = Lèirmheas Guardian
agent-tool-read-files = Leugh faidhlichean
agent-tool-viewed-image = Sheall e dealbh
agent-tool-used-browser = Chleachd e am brabhsair
agent-tool-searched-files = Lorg e sna faidhlichean
agent-tool-ran-commands = Ruith e àitheantan
agent-thinking = A’ smaoineachadh
agent-subagent = Fo-àidseant
agent-prompt = Proimt
agent-thread = Snàithlean
agent-parent = Pàrant
agent-children = Clann
agent-call = Gairm
agent-raw-event = Tachartas amh
agent-plan = Plana
agent-tasks = { $count ->
    [one] 1 saothair
   *[other] { $count } saothair
}
agent-edited = Deasaichte
agent-reconnecting = Ag ath-cheangal { $attempt }/{ $total }
agent-status-running = A’ ruith
agent-status-done = Deiseil
agent-status-failed = Dh’fhàillig
agent-status-pending = Ri feitheamh
agent-slash-attach-files = Ceangail faidhlichean
agent-slash-resume-session = Ath-thòisich seisean roimhe
agent-slash-select-model = Tagh modail
agent-slash-continue-cli = Lean air adhart leis an t-seisean seo sa CLI
agent-session-just-now = an-dràsta fhèin
agent-session-minutes-ago = o chionn { $count }m
agent-session-hours-ago = o chionn { $count }u
agent-session-days-ago = o chionn { $count }l
agent-working-working = Ag obair
agent-working-thinking = A’ smaoineachadh
agent-working-pondering = A’ beachdachadh
agent-working-noodling = A’ cnuasachadh
agent-working-percolating = A’ goil air a shocair
agent-working-conjuring = A’ dèanamh draoidheachd
agent-working-cooking = A’ còcaireachd
agent-working-brewing = A’ grùdadh
agent-working-musing = A’ meòrachadh
agent-working-ruminating = A’ cnuasachadh
agent-working-scheming = A’ dealbhadh
agent-working-synthesizing = A’ co-chur
agent-working-tinkering = A’ gleusadh
agent-working-churning = A’ saothrachadh
agent-working-vibing = A’ glacadh an ruitheim
agent-working-simmering = A’ suathadh
agent-working-crafting = A’ ciùird
agent-working-divining = A’ fàidheadaireachd
agent-working-mulling = A’ beachdachadh
agent-working-spelunking = A’ dol domhainn

editor-toggle-explorer = Toglaich an Rùraiche (Cmd+B)
editor-unsaved = gun sàbhaladh
editor-rendered-markdown = Markdown rendarraichte le deasachadh beò
editor-note = Nota
editor-source-editor = Deasaiche tùs-chòd
editor-editor = Deasaiche
editor-git-diff = Diff Git
editor-diff = Diff
editor-tidy = Sgiobalta
editor-always = An-còmhnaidh
editor-unchanged-previews = { $count ->
    [one] ✦ 1 ro-shealladh gun atharrachadh
   *[other] ✦ { $count } ro-sheallaidhean gun atharrachadh
}
editor-open-externally = Fosgail air an taobh a-muigh
editor-changed-line = Loidhne atharraichte
editor-go-to-definition = Rach dhan mhìneachadh
editor-find-references = Lorg iomraidhean
editor-references = { $count ->
    [one] 1 iomradh
   *[other] { $count } iomraidhean
}
editor-lsp-starting = { $server } a’ tòiseachadh…
editor-lsp-not-installed = { $server } — gun stàladh
editor-explorer = Rùraiche
editor-open-editors = Deasaichean fosgailte
editor-outline = Oir-loidhne
editor-new-file = Faidhle ùr
editor-new-folder = Pasgan ùr
editor-delete-confirm = A bheil thu airson “{ $name }” a sguabadh às? Cha ghabh seo a neo-dhèanamh.
editor-created-folder = Chaidh am pasgan { $name } a chruthachadh
editor-created-file = Chaidh am faidhle { $name } a chruthachadh
editor-renamed-to = Chaidh ath-ainmeachadh gu { $name }
editor-deleted = Chaidh { $name } a sguabadh às
editor-failed-decode-image = Dh’fhàillig dì-chòdachadh an deilbh
editor-preview-large-image = dealbh (ro mhòr airson ro-shealladh)
editor-preview-binary = bìnearaidh
editor-preview-file = faidhle

git-status-clean = glan
git-status-modified = atharraichte
git-status-staged = air àrd-ùrlar
git-status-staged-modified = air àrd-ùrlar*
git-status-untracked = gun tracadh
git-status-deleted = sguabte às
git-status-conflict = còmhstri
git-accept-all = ✓ gabh ris a h-uile
git-unstage = Thoir far an àrd-ùrlair
git-confirm-deny-all = Dearbh diùltadh a h-uile
git-deny-all = ✗ diùlt a h-uile
git-commit-message = teachdaireachd commit
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = A’ luchdadh an diff…
git-no-changes = Chan eil atharraichean ri shealltainn
git-accept = ✓ gabh ris
git-deny = ✗ diùlt
git-show-unchanged-lines = Seall { $count } loidhnichean gun atharrachadh

terminal-loading = ’Ga luchdadh…
terminal-runs-when-ready = ruithidh e nuair a bhios e deiseil · glanaidh Ctrl+C · leumaidh Esc seachad air
terminal-booting = a’ tòiseachadh
terminal-type-command = sgrìobh àithne · ruithidh e nuair a bhios e deiseil · leumaidh Esc seachad air

setup-tagline-claude = Àidseant còdaidh Anthropic, ann am Vmux
setup-tagline-codex = Àidseant còdaidh OpenAI, ann am Vmux
setup-tagline-vibe = Àidseant còdaidh Mistral, ann am Vmux
setup-install-title = Stàlaich CLI { $name }
setup-homebrew-required = Tha feum air Homebrew gus { $command } a stàladh agus chan eil e suidhichte fhathast. Stàlaidh Vmux Homebrew an toiseach, agus an uair sin { $name }.
setup-terminal-instructions = San tèirmineal, brùth Return airson tòiseachadh, agus cuir a-steach facal-faire do Mhic nuair a thèid iarraidh ort.
setup-command-missing = Dh’fhosgail Vmux an duilleag seo a chionn ’s nach eil an àithne ionadail { $command } stàlaichte fhathast. Ruith an àithne gu h-ìosal airson fhaighinn.
setup-install-failed = Cha do chrìochnaich an stàladh. Thoir sùil air an tèirmineal airson mion-fhiosrachadh, agus feuch ris a-rithist.
setup-installing = ’Ga stàladh…
setup-install-homebrew = Stàlaich Homebrew + { $name }
setup-run-install = Ruith àithne stàlaidh
setup-auto-reload = Ruithidh Vmux e ann an tèirmineal agus ath-luchdaichidh e nuair a bhios { $command } deiseil.

debug-title = Dì-bhugachadh
debug-auto-update = Fèin-ùrachadh
debug-simulate-update = Samhlaich ùrachadh ri fhaighinn
debug-simulate-download = Samhlaich luchdadh a-nuas
debug-clear-update = Glan an t-ùrachadh
debug-trigger-restart = Brosnaich ath-thòiseachadh

command-manage-spaces = Stiùirich àiteachan…
command-pane-stack-location = leòsan { $pane } / stac { $stack }
command-space-pane-stack-location = { $space } / leòsan { $pane } / stac { $stack }
command-terminal-path = Tèirmineal ({ $path })
command-group-interactive-mode = Modh eadar-ghnìomhach
command-group-window = Uinneag
command-group-tab = Taba
command-group-pane = Leòsan
command-group-stack = Stac
command-group-space = Àite
command-group-navigation = Seòladh
command-group-open = Fosgail
command-group-view = Sealladh
command-group-bar = Bàr

menu-close-vmux = Dùin Vmux

agents-terminal-coding-agent = Àidseant còdaidh stèidhichte air an tèirmineal
