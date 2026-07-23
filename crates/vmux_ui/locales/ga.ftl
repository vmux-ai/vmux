locale-name = Gaeilge
common-open = Oscail
common-close = Dún
common-install = Suiteáil
common-uninstall = Díshuiteáil
common-update = Nuashonraigh
common-retry = Bain triail eile as
common-refresh = Athnuaigh
common-remove = Bain
common-enable = Cumasaigh
common-disable = Díchumasaigh
common-new = Nua
common-active = gníomhach
common-running = ar siúl
common-done = déanta
common-failed = Theip
common-installed = Suiteáilte
common-items = { $count ->
    [one] { $count } mhír
   *[other] { $count } mír
}

tools-title = Uirlisí
tools-search = Cuardaigh pacáistí, gníomhairí, MCP, uirlisí teanga agus comhaid chumraíochta…
tools-open = Oscail Uirlisí
tools-fold = Fill na huirlisí
tools-unfold = Leathnaigh na huirlisí
tools-scanning = Uirlisí áitiúla á scanadh…
tools-no-installed = Níl aon uirlis suiteáilte
tools-empty = Níl aon uirlis mheaitseála ann
tools-empty-detail = Suiteáil pacáiste nó cuir pacáiste comhad cumraíochta ar nós Stow leis.
tools-apply = Cuir i bhfeidhm
tools-homebrew = Homebrew
tools-homebrew-sync = Sioncronaítear foirmlí agus feidhmchláir shuiteáilte go huathoibríoch.
tools-open-brewfile = Oscail Brewfile
tools-managed = bainistithe
tools-provider-homebrew-formulae = Foirmlí Homebrew
tools-provider-homebrew-casks = Feidhmchláir Homebrew
tools-provider-npm = Pacáistí npm
tools-provider-acp-agents = Gníomhairí ACP
tools-provider-language-tools = Uirlisí teanga
tools-provider-mcp-servers = Freastalaithe MCP
tools-provider-dotfiles = Comhaid chumraíochta
tools-status-available = Ar fáil
tools-status-missing = Ar iarraidh
tools-status-conflict = Coimhlint
tools-forget = Déan dearmad
tools-manage = Bainistigh
tools-link = Nasc
tools-unlink = Dínasc
tools-import = Iompórtáil
tools-update-count = { $count ->
    [one] 1 nuashonrú
   *[other] { $count } nuashonrú
}
tools-conflict-count = { $count ->
    [one] 1 choimhlint
   *[other] { $count } coimhlint
}
tools-result-applied = Uirlisí curtha i bhfeidhm
tools-result-imported = Uirlisí iompórtáilte
tools-result-installed = { $name } suiteáilte
tools-result-updated = { $name } nuashonraithe
tools-result-uninstalled = { $name } díshuiteáilte
tools-result-forgotten = Rinneadh dearmad ar { $name }
tools-result-managed = Tá { $name } á bhainistiú anois
tools-result-linked = { $name } nasctha
tools-result-unlinked = { $name } dínasctha
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Sioncronaigh socruithe, uirlisí, poncchomhaid, agus Eolas le Git.
vault-sync = Sioncrónaigh
vault-create = Cruthaigh
vault-connect = Ceangail
vault-private = Stór príobháideach
vault-public-warning = Nochtann stórtha poiblí do Eolas agus do chumraíocht.
vault-choose-repository = Roghnaigh stór…
vault-empty = folamh
vault-clean = Suas chun dáta
vault-not-connected = Gan ceangal
vault-change-count = Athruithe: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Tosaigh
start-tagline = Leid amháin. Déanta, pé rud é.

agents-title = Gníomhairí
agents-search = Cuardaigh gníomhairí ACP agus CLI…
agents-empty = Níl aon ghníomhairí comhoiriúnacha ann
agents-empty-detail = Bain triail as ainm, am rite, nó ACP/CLI.
agents-install-failed = Theip ar an tsuiteáil
agents-updating = Á nuashonrú…
agents-retrying = Ag triail arís…
agents-preparing = Á ullmhú…

extensions-title = Eisínteachtaí
extensions-search = Cuardaigh eisínteachtaí suiteáilte nó Chrome Web Store…
extensions-relaunch = Atosaigh chun é a chur i bhfeidhm
extensions-empty = Níl aon eisínteachtaí suiteáilte
extensions-no-match = Níl aon eisínteachtaí comhoiriúnacha ann
extensions-empty-detail = Cuardaigh Chrome Web Store thuas agus brúigh Return.
extensions-no-match-detail = Bain triail as ainm nó ID eisínteachta eile.
extensions-on = Ar siúl
extensions-off = As
extensions-enable-confirm = Cumasaigh { $name }?
extensions-enable-permissions = Cumasaigh { $name } agus ceadaigh:

lsp-title = Freastalaithe Teanga
lsp-search = Cuardaigh freastalaithe teanga, lintéirí, formáiditheoirí…
lsp-loading = Catalóg á lódáil…
lsp-empty = Níl aon fhreastalaithe teanga comhoiriúnacha ann
lsp-empty-detail = Bain triail as teanga, lintéir nó formáiditheoir eile.
lsp-needs = teastaíonn { $tool }
lsp-status-available = Ar fáil
lsp-status-on-path = Ar PATH
lsp-status-installing = Á shuiteáil…
lsp-status-installed = Suiteáilte
lsp-status-outdated = Nuashonrú ar fáil
lsp-status-running = Ar siúl
lsp-status-failed = Theip

spaces-title = Spásanna
spaces-new-placeholder = Ainm an spáis nua
spaces-empty = Níl aon spásanna ann
spaces-default-name = Spás { $number }
spaces-tabs = { $count ->
    [one] 1 chluaisín
   *[other] { $count } cluaisín
}
spaces-delete = Scrios an spás

team-title = Foireann
team-just-you = Tusa amháin sa spás seo
team-agents = { $count ->
    [one] Tusa agus 1 ghníomhaire
   *[other] Tusa agus { $count } gníomhaire
}
team-empty = Níl aon duine anseo fós
team-you = Tusa
team-agent = Gníomhaire

services-title = Seirbhísí Cúlra
services-processes = { $count ->
    [one] 1 phróiseas
   *[other] { $count } próiseas
}
services-kill-all = Cuir Iad Uilig ar Ceal
services-not-running = Níl an tseirbhís ar siúl
services-start-with = Tosaigh le:
services-empty = Níl aon phróisis ghníomhacha ann
services-filter = Scag próisis…
services-no-match = Níl aon phróisis chomhoiriúnacha ann
services-connected = Ceangailte
services-disconnected = Dícheangailte
services-attached = ceangailte leis
services-kill = Cuir ar ceal
services-memory = Cuimhne
services-size = Méid
services-shell = Blaosc

error-title = Earráid

history-search = Cuardaigh an stair
history-clear-all = Glan an t-iomlán
history-clear-confirm = Glan an stair ar fad?
history-clear-warning = Ní féidir é seo a chealú.
history-cancel = Cealaigh
history-today = Inniu
history-yesterday = Inné
history-days-ago = { $count } lá ó shin
history-day-offset = Lá -{ $count }

settings-title = Socruithe
settings-loading = Socruithe á lódáil…
settings-stored = Stóráilte in ~/.vmux/settings.ron
settings-other = Eile
settings-software-update = Nuashonrú Bogearraí
settings-check-updates = Lorg Nuashonruithe
settings-check-updates-hint = Seiceáiltear go huathoibríoch ag am tosaithe agus gach uair an chloig nuair atá Uath-nuashonrú cumasaithe.
settings-update-unavailable = Níl ar fáil
settings-update-unavailable-hint = Níl an nuashonraitheoir san áireamh sa leagan seo.
settings-update-checking = Á sheiceáil…
settings-update-checking-hint = Nuashonruithe á lorg…
settings-update-check-again = Seiceáil Arís
settings-update-current = Tá Vmux cothrom le dáta.
settings-update-downloading = Á íoslódáil…
settings-update-downloading-hint = Vmux { $version } á íoslódáil…
settings-update-installing = Á shuiteáil…
settings-update-installing-hint = Vmux { $version } á shuiteáil…
settings-update-ready = Nuashonrú Réidh
settings-update-ready-hint = Tá Vmux { $version } réidh. Atosaigh chun é a chur i bhfeidhm.
settings-update-try-again = Bain Triail Eile As
settings-update-failed = Níorbh fhéidir nuashonruithe a lorg.
settings-item = Mír
settings-item-number = Mír { $number }
settings-press-key = Brúigh eochair…
settings-saved = Sábháilte
settings-record-key = Cliceáil chun teaglaim eochracha nua a thaifeadadh

tray-open-window = Oscail Fuinneog
tray-close-window = Dún Fuinneog
tray-pause-recording = Cuir Taifeadadh ar Sos
tray-resume-recording = Lean den Taifeadadh
tray-finish-recording = Críochnaigh an Taifeadadh
tray-quit = Scoir de Vmux

composer-attach-files = Ceangail comhaid (/upload)
composer-remove-attachment = Bain ceangaltán

layout-back = Siar
layout-forward = Ar aghaidh
layout-reload = Athlódáil
layout-bookmark-page = Cuir leabharmharc leis an leathanach seo
layout-remove-bookmark = Bain leabharmharc
layout-pin-page = Pionnáil an leathanach seo
layout-unpin-page = Díphionnáil an leathanach seo
layout-manage-extensions = Bainistigh eisínteachtaí
layout-new-stack = Cruach nua
layout-close-tab = Dún cluaisín
layout-bookmark = Leabharmharc
layout-pin = Pionnáil
layout-new-tab = Cluaisín nua
layout-team = Foireann

command-switch-space = Athraigh spás…
command-search-ask = Cuardaigh nó fiafraigh…
command-new-tab-placeholder = Cuardaigh nó clóscríobh URL, nó roghnaigh Teirminéal…
command-placeholder = Clóscríobh URL, cuardaigh cluaisíní, nó > le haghaidh orduithe…
command-composer-placeholder = Clóscríobh / le haghaidh orduithe nó @ le haghaidh meán
command-send = Seol (Enter)
command-terminal = Teirminéal
command-open-terminal = Oscail sa Teirminéal
command-stack = Cruach
command-tabs = { $count ->
    [one] 1 chluaisín
   *[other] { $count } cluaisín
}
command-prompt = Leid
command-new-tab = Cluaisín nua
command-search = Cuardaigh
command-open-value = Oscail “{ $value }”
command-search-value = Cuardaigh “{ $value }”

schema-appearance = Cuma
schema-general = Ginearálta
schema-layout = Leagan amach
schema-layout-detail = Fuinneog, pánaí, barra taoibh agus fáinne fócais.
schema-agent = Gníomhaire
schema-agent-detail = Iompar gníomhairí agus ceadanna uirlisí.
schema-shortcuts = Aicearraí
schema-shortcuts-detail = Amharc inléite amháin. Cuir settings.ron in eagar go díreach chun ceangail a athrú.
schema-terminal = Teirminéal
schema-browser = Brabhsálaí
schema-mode = Mód
schema-mode-detail = Scéim dathanna do leathanaigh ghréasáin. Leanann Gléas do chóras.
schema-device = Gléas
schema-light = Geal
schema-dark = Dorcha
schema-language = Teanga
schema-language-detail = Úsáid an córas, en-US, ja, nó clib BCP 47 ar bith a bhfuil catalóg ~/.vmux/locales/<tag>.ftl mheaitseála aici.
schema-auto-update = Uath-nuashonrú
schema-auto-update-detail = Lorg agus suiteáil nuashonruithe ag am tosaithe agus gach uair an chloig.
schema-startup-url = URL tosaithe
schema-startup-url-detail = Má bhíonn sé folamh, osclaítear leid an bharra orduithe.
schema-search-engine = Inneall cuardaigh
schema-search-engine-detail = Úsáidtear é do chuardaigh ghréasáin ó Tosaigh agus ón mbarra orduithe.
schema-window = Fuinneog
schema-pane = Pána
schema-side-sheet = Bileog thaobh
schema-focus-ring = Fáinne fócais
schema-run-placement = Ceadaigh sárú ar shuíomh rite
schema-run-placement-detail = Lig do ghníomhairí mód, treo agus ancair an phána rite a roghnú.
schema-leader = Ceannaire
schema-leader-detail = Eochair réimíre d’aicearraí corda.
schema-chord-timeout = Teorainn ama corda
schema-chord-timeout-detail = Milleasoicindí sula dtéann réimír corda in éag.
schema-bindings = Ceangail
schema-confirm-close = Deimhnigh dúnadh
schema-confirm-close-detail = Tabhair leid roimh theirminéal a dhúnadh a bhfuil próiseas ar siúl ann.
schema-default-theme = Téama réamhshocraithe
schema-default-theme-detail = Ainm an téama ghníomhaigh ón liosta téamaí.

settings-empty = (folamh)
settings-none = (dada)

schema-system = Córas
schema-editor = Eagarthóir
schema-recording = Taifeadadh
schema-radius = Ga
schema-padding = Stuáil
schema-gap = Bearna
schema-width = Leithead
schema-color = Dath
schema-red = Dearg
schema-green = Glas
schema-blue = Gorm
schema-follow-files = Lean comhaid
schema-tidy-files = Glan comhaid
schema-tidy-files-max = Tairseach glanta comhad
schema-tidy-files-auto = Glan comhaid go huathoibríoch
schema-app-providers = Soláthraithe aipeanna
schema-provider = Soláthraí
schema-kind = Cineál
schema-models = Samhlacha
schema-acp = Gníomhairí ACP
schema-id = ID
schema-name = Ainm
schema-command = Ordú
schema-arguments = Argóintí
schema-environment = Timpeallacht
schema-working-directory = Comhadlann oibre
schema-shell = Blaosc
schema-font-family = Fine chló
schema-startup-directory = Comhadlann tosaithe
schema-themes = Téamaí
schema-color-scheme = Scéim dathanna
schema-font-size = Clómhéid
schema-line-height = Airde líne
schema-cursor-style = Stíl chúrsóra
schema-cursor-blink = Caochadh cúrsóra
schema-custom-themes = Téamaí saincheaptha
schema-foreground = Tulra
schema-background = Cúlra
schema-cursor = Cúrsóir
schema-ansi-colors = Dathanna ANSI
schema-keymap = Mapa eochracha
schema-explorer = Taiscéalaí
schema-visible = Infheicthe
schema-language-servers = Freastalaithe teanga
schema-servers = Freastalaithe
schema-language-id = ID teanga
schema-root-markers = Marcóirí fréimhe
schema-output-directory = Comhadlann aschuir

menu-scene = Radharc
menu-layout = Leagan Amach
menu-terminal = Teirminéal
menu-browser = Brabhsálaí
menu-service = Seirbhís
menu-bookmark = Leabharmharc
menu-edit = Eagar

layout-knowledge = Eolas
layout-open-knowledge = Oscail Eolas
layout-open-welcome-knowledge = Oscail Fáilte chuig Eolas
layout-open-path = Oscail { $path }
layout-fold-knowledge = Fill eolas
layout-unfold-knowledge = Dífhill eolas
layout-bookmarks = Leabharmharcanna
layout-new-folder = Fillteán Nua
layout-add-to-bookmarks = Cuir le Leabharmharcanna
layout-move-to-bookmarks = Bog go Leabharmharcanna
layout-stack-number = Cruach { $number }
layout-fold-stack = Fill cruach
layout-unfold-stack = Dífhill cruach
layout-close-stack = Dún cruach
layout-bookmark-in = Cuir leabharmharc in { $folder }

common-cancel = Cealaigh
common-delete = Scrios
common-save = Sábháil
common-rename = Athainmnigh
common-expand = Leathnaigh
common-collapse = Laghdaigh
common-loading = Á lódáil…
common-error = Earráid
common-output = Aschur
common-pending = Ar feitheamh
common-current = reatha
common-stop = Stop
services-command = Seirbhís Vmux
services-uptime-seconds = { $seconds }s
services-uptime-minutes = { $minutes }n { $seconds }s
services-uptime-hours = { $hours }u { $minutes }n
services-uptime-days = { $days }l { $hours }u

error-page-failed-load = Níor lódáil an leathanach
error-page-not-found = Níor aimsíodh an leathanach
error-unknown-host = Óstríomhaire aipe Vmux anaithnid: { $host }

history-title = Stair

command-new-app-chat = Comhrá nua { $provider }/{ $model } (Aip)
command-interactive-mode-user = Radharc > Mód Idirghníomhach > Úsáideoir
command-interactive-mode-player = Radharc > Mód Idirghníomhach > Imreoir
command-minimize-window = Leagan Amach > Fuinneog > Íoslaghdaigh
command-toggle-layout = Leagan Amach > Leagan Amach > Scoránaigh Leagan Amach
command-close-tab = Leagan Amach > Cluaisín > Dún Cluaisín
command-new-task = Leagan Amach > Cluaisín > Tasc Nua…
command-next-tab = Leagan Amach > Cluaisín > An Chéad Chluaisín Eile
command-prev-tab = Leagan Amach > Cluaisín > An Cluaisín Roimhe Seo
command-rename-tab = Leagan Amach > Cluaisín > Athainmnigh Cluaisín
command-tab-select-1 = Leagan Amach > Cluaisín > Roghnaigh Cluaisín 1
command-tab-select-2 = Leagan Amach > Cluaisín > Roghnaigh Cluaisín 2
command-tab-select-3 = Leagan Amach > Cluaisín > Roghnaigh Cluaisín 3
command-tab-select-4 = Leagan Amach > Cluaisín > Roghnaigh Cluaisín 4
command-tab-select-5 = Leagan Amach > Cluaisín > Roghnaigh Cluaisín 5
command-tab-select-6 = Leagan Amach > Cluaisín > Roghnaigh Cluaisín 6
command-tab-select-7 = Leagan Amach > Cluaisín > Roghnaigh Cluaisín 7
command-tab-select-8 = Leagan Amach > Cluaisín > Roghnaigh Cluaisín 8
command-tab-select-last = Leagan Amach > Cluaisín > Roghnaigh an Cluaisín Deireanach
command-close-pane = Leagan Amach > Pána > Dún Pána
command-select-pane-left = Leagan Amach > Pána > Roghnaigh an Pána ar Chlé
command-select-pane-right = Leagan Amach > Pána > Roghnaigh an Pána ar Dheis
command-select-pane-up = Leagan Amach > Pána > Roghnaigh an Pána Thuas
command-select-pane-down = Leagan Amach > Pána > Roghnaigh an Pána Thíos
command-swap-pane-prev = Leagan Amach > Pána > Babhtáil leis an bPána Roimhe Seo
command-swap-pane-next = Leagan Amach > Pána > Babhtáil leis an gCéad Phána Eile
command-equalize-pane-size = Leagan Amach > Pána > Cothromaigh Méid na bPánaí
command-resize-pane-left = Leagan Amach > Pána > Athraigh Méid Pána ar Chlé
command-resize-pane-right = Leagan Amach > Pána > Athraigh Méid Pána ar Dheis
command-resize-pane-up = Leagan Amach > Pána > Athraigh Méid Pána Suas
command-resize-pane-down = Leagan Amach > Pána > Athraigh Méid Pána Síos
command-stack-close = Leagan Amach > Cruach > Dún Cruach
command-stack-next = Leagan Amach > Cruach > An Chéad Chruach Eile
command-stack-previous = Leagan Amach > Cruach > An Chruach Roimhe Seo
command-stack-reopen = Leagan Amach > Cruach > Athoscail Leathanach Dúnta
command-stack-swap-prev = Leagan Amach > Cruach > Bog Cruach Ar Chlé
command-stack-swap-next = Leagan Amach > Cruach > Bog Cruach Ar Dheis
command-space-open = Leagan Amach > Spás > Spásanna
command-terminal-close = Teirminéal > Dún Teirminéal
command-terminal-next = Teirminéal > An Chéad Teirminéal Eile
command-terminal-prev = Teirminéal > An Teirminéal Roimhe Seo
command-terminal-clear = Teirminéal > Glan Teirminéal
command-browser-prev-page = Brabhsálaí > Nascleanúint > Siar
command-browser-next-page = Brabhsálaí > Nascleanúint > Ar Aghaidh
command-browser-reload = Brabhsálaí > Nascleanúint > Athlódáil
command-browser-hard-reload = Brabhsálaí > Nascleanúint > Athlódáil Iomlán
command-open-in-place = Brabhsálaí > Oscail > Oscail Anseo
command-open-in-new-stack = Brabhsálaí > Oscail > Oscail i gCruach Nua
command-open-in-pane-top = Brabhsálaí > Oscail > Oscail i bPána Thuas
command-open-in-pane-right = Brabhsálaí > Oscail > Oscail i bPána ar Dheis
command-open-in-pane-bottom = Brabhsálaí > Oscail > Oscail i bPána Thíos
command-open-in-pane-left = Brabhsálaí > Oscail > Oscail i bPána ar Chlé
command-open-in-new-tab = Brabhsálaí > Oscail > Oscail i gCluaisín Nua
command-open-in-new-space = Brabhsálaí > Oscail > Oscail i Spás Nua
command-browser-zoom-in = Brabhsálaí > Amharc > Zúmáil Isteach
command-browser-zoom-out = Brabhsálaí > Amharc > Zúmáil Amach
command-browser-zoom-reset = Brabhsálaí > Amharc > Méid Iarbhír
command-browser-dev-tools = Brabhsálaí > Amharc > Uirlisí Forbróra
command-browser-open-command-bar = Brabhsálaí > Barra > Barra Orduithe
command-browser-open-page-in-command-bar = Brabhsálaí > Barra > Cuir Leathanach in Eagar
command-browser-open-path-bar = Brabhsálaí > Barra > Nascleanóir Conaire
command-browser-open-commands = Brabhsálaí > Barra > Orduithe
command-browser-open-history = Brabhsálaí > Barra > Stair
command-service-open = Seirbhís > Oscail Monatóir Seirbhíse
command-bookmark-toggle-active = Leabharmharc > Cuir Leabharmharc leis an Leathanach
command-bookmark-pin-active = Leabharmharc > Pionnáil Leathanach

layout-tab = Cluaisín
layout-no-stacks = Níl aon chruacha ann
layout-loading = Á lódáil…
layout-no-markdown-files = Níl aon chomhaid Markdown ann
layout-empty-folder = Fillteán folamh
layout-worktree = crann oibre
layout-folder-name = Ainm fillteáin
layout-no-pins-bookmarks = Níl aon phionnaí ná leabharmharcanna ann
layout-move-to = Bog go { $folder }
layout-bookmark-current-page = Cuir Leabharmharc leis an Leathanach Reatha
layout-rename-folder = Athainmnigh Fillteán
layout-remove-folder = Bain Fillteán
layout-update-downloading = Nuashonrú á íoslódáil
layout-update-installing = Nuashonrú á shuiteáil…
layout-update-ready = Tá leagan nua ar fáil
layout-restart-update = Atosaigh chun nuashonrú

agent-preparing = Gníomhaire á ullmhú…
agent-send-all-queued = Seol gach leid sa chiú anois (Esc)
agent-send = Seol (Enter)
agent-ready = Réidh nuair atá tusa.
agent-loading-older = Teachtaireachtaí níos sine á lódáil…
agent-load-older = Lódáil teachtaireachtaí níos sine
agent-continued-from = Ar lean ó { $source }
agent-older-context-omitted = fágadh comhthéacs níos sine ar lár
agent-interrupted = idirbhriste
agent-allow-tool = Ceadaigh { $tool }?
agent-deny = Diúltaigh
agent-allow-always = Ceadaigh i gcónaí
agent-allow = Ceadaigh
agent-loading-sessions = Seisiúin á lódáil…
agent-no-resumable-sessions = Níor aimsíodh seisiúin in-atosaithe
agent-no-matching-sessions = Níl aon seisiúin mheaitseála ann
agent-no-matching-models = Níl aon samhlacha meaitseála ann
agent-choice-help = ↑/↓ nó Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Roghnaigh fillteán stórais
agent-choose-repository-detail = Roghnaigh an stór Git áitiúil ba chóir don ghníomhaire a úsáid.
agent-choosing = Á roghnú…
agent-choose-folder = Roghnaigh fillteán
agent-queued = sa chiú
agent-attached = Ceangailte:
agent-cancel-queued = Cealaigh leid sa chiú
agent-resume-queued = Atosaigh leideanna sa chiú
agent-clear-queue = Glan an ciú
agent-send-all-now = seol iad uile anois
agent-choose-option = Roghnaigh rogha thuas
agent-loading-media = Meáin á lódáil…
agent-no-matching-media = Níl aon mheáin mheaitseála ann
agent-prompt-context = Comhthéacs leide
agent-details = Sonraí
agent-path = Conair
agent-tool = Uirlis
agent-server = Freastalaí
agent-bytes = { $count } beart
agent-worked-for = D’oibrigh ar feadh { $duration }
agent-worked-for-steps = { $count ->
    [one] D’oibrigh ar feadh { $duration } · 1 chéim
   *[other] D’oibrigh ar feadh { $duration } · { $count } céim
}
agent-tool-guardian-review = Athbhreithniú Cosanta
agent-tool-read-files = Léigh comhaid
agent-tool-viewed-image = D’amharc ar íomhá
agent-tool-used-browser = D’úsáid brabhsálaí
agent-tool-searched-files = Chuardaigh comhaid
agent-tool-ran-commands = Rith orduithe
agent-thinking = Ag smaoineamh
agent-subagent = Foghníomhaire
agent-prompt = Leid
agent-thread = Snáithe
agent-parent = Tuismitheoir
agent-children = Páistí
agent-call = Glao
agent-raw-event = Teagmhas amh
agent-plan = Plean
agent-tasks = { $count ->
    [one] 1 tasc
   *[other] { $count } tasc
}
agent-edited = Curtha in eagar
agent-reconnecting = Ag athcheangal { $attempt }/{ $total }
agent-status-running = Ar siúl
agent-status-done = Déanta
agent-status-failed = Theip
agent-status-pending = Ar feitheamh
agent-slash-attach-files = Ceangail comhaid
agent-slash-resume-session = Atosaigh seisiún roimhe seo
agent-slash-select-model = Roghnaigh samhail
agent-slash-continue-cli = Lean ar aghaidh leis an seisiún seo sa CLI
agent-session-just-now = díreach anois
agent-session-minutes-ago = { $count }n ó shin
agent-session-hours-ago = { $count }u ó shin
agent-session-days-ago = { $count }l ó shin
agent-working-working = Ag obair
agent-working-thinking = Ag smaoineamh
agent-working-pondering = Ag machnamh
agent-working-noodling = Ag cíoradh
agent-working-percolating = Ag coipeadh
agent-working-conjuring = Ag cumadh
agent-working-cooking = Ag cócaráil
agent-working-brewing = Ag grúdú
agent-working-musing = Ag machnamh
agent-working-ruminating = Ag dianmhachnamh
agent-working-scheming = Ag pleanáil
agent-working-synthesizing = Ag sintéisiú
agent-working-tinkering = Ag tincéireacht
agent-working-churning = Ag meascadh
agent-working-vibing = Ag teacht ar an tonn
agent-working-simmering = Ag suanbhruith
agent-working-crafting = Ag ceardú
agent-working-divining = Ag fáistineacht
agent-working-mulling = Ag cur trí chéile
agent-working-spelunking = Ag tochailt

editor-toggle-explorer = Scoránaigh Taiscéalaí (Cmd+B)
editor-unsaved = gan sábháil
editor-rendered-markdown = Markdown rindreáilte le heagarthóireacht bheo
editor-note = Nóta
editor-source-editor = Eagarthóir foinse
editor-editor = Eagarthóir
editor-git-diff = Difríocht Git
editor-diff = Difríocht
editor-tidy = Slacht
editor-always = I gcónaí
editor-unchanged-previews = { $count ->
    [one] ✦ 1 réamhamharc gan athrú
   *[other] ✦ { $count } réamhamharc gan athrú
}
editor-open-externally = Oscail go seachtrach
editor-changed-line = Líne athraithe
editor-go-to-definition = Téigh go Sainmhíniú
editor-find-references = Aimsigh Tagairtí
editor-references = { $count ->
    [one] 1 tagairt
   *[other] { $count } tagairt
}
editor-lsp-starting = { $server } á thosú…
editor-lsp-not-installed = { $server } — gan suiteáil
editor-explorer = Taiscéalaí
editor-open-editors = Eagarthóirí Oscailte
editor-outline = Imlíne
editor-new-file = Comhad Nua
editor-new-folder = Fillteán Nua
editor-delete-confirm = Scrios “{ $name }”? Ní féidir é seo a chealú.
editor-created-folder = Cruthaíodh fillteán { $name }
editor-created-file = Cruthaíodh comhad { $name }
editor-renamed-to = Athainmníodh go { $name }
editor-deleted = Scriosadh { $name }
editor-failed-decode-image = Theip ar dhíchódú na híomhá
editor-preview-large-image = íomhá (rómhór le réamhamharc)
editor-preview-binary = dénártha
editor-preview-file = comhad

git-status-clean = glan
git-status-modified = athraithe
git-status-staged = stáitsithe
git-status-staged-modified = stáitsithe*
git-status-untracked = gan rianú
git-status-deleted = scriosta
git-status-conflict = coimhlint
git-accept-all = ✓ glac le gach rud
git-unstage = Dí-stáitsigh
git-confirm-deny-all = Deimhnigh diúltú do gach rud
git-deny-all = ✗ diúltaigh do gach rud
git-commit-message = teachtaireacht tiomnaithe
git-commit = Tiomnaigh ({ $count })
git-push = ↑ Brúigh
git-loading-diff = Difríocht á lódáil…
git-no-changes = Níl aon athruithe le taispeáint
git-accept = ✓ glac
git-deny = ✗ diúltaigh
git-show-unchanged-lines = Taispeáin { $count } líne gan athrú

terminal-loading = Á lódáil…
terminal-runs-when-ready = ritheann nuair atá sé réidh · glanann Ctrl+C · scipeálann Esc
terminal-booting = á thosú
terminal-type-command = clóscríobh ordú · ritheann nuair atá sé réidh · scipeálann Esc

setup-tagline-claude = Gníomhaire códaithe Anthropic, in Vmux
setup-tagline-codex = Gníomhaire códaithe OpenAI, in Vmux
setup-tagline-vibe = Gníomhaire códaithe Mistral, in Vmux
setup-install-title = Suiteáil CLI { $name }
setup-homebrew-required = Tá Homebrew riachtanach chun { $command } a shuiteáil agus níl sé socraithe fós. Suiteálfaidh Vmux Homebrew ar dtús, ansin { $name }.
setup-terminal-instructions = Sa teirminéal, brúigh Return chun tosú, ansin cuir isteach do phasfhocal Mac nuair a iarrtar é.
setup-command-missing = D’oscail Vmux an leathanach seo mar níl an t-ordú áitiúil { $command } suiteáilte fós. Rith an t-ordú thíos chun é a fháil.
setup-install-failed = Níor chríochnaigh an tsuiteáil. Seiceáil an teirminéal le haghaidh sonraí, ansin bain triail eile as.
setup-installing = Á shuiteáil…
setup-install-homebrew = Suiteáil Homebrew + { $name }
setup-run-install = Rith ordú suiteála
setup-auto-reload = Ritheann Vmux é i dteirminéal agus athlódálann sé nuair atá { $command } réidh.

debug-title = Dífhabhtú
debug-auto-update = Uath-nuashonrú
debug-simulate-update = Insamhail nuashonrú ar fáil
debug-simulate-download = Insamhail íoslódáil
debug-clear-update = Glan nuashonrú
debug-trigger-restart = Spreag atosú

command-manage-spaces = Bainistigh spásanna…
command-pane-stack-location = pána { $pane } / cruach { $stack }
command-space-pane-stack-location = { $space } / pána { $pane } / cruach { $stack }
command-terminal-path = Teirminéal ({ $path })
command-group-interactive-mode = Mód Idirghníomhach
command-group-window = Fuinneog
command-group-tab = Cluaisín
command-group-pane = Pána
command-group-stack = Cruach
command-group-space = Spás
command-group-navigation = Nascleanúint
command-group-open = Oscail
command-group-view = Amharc
command-group-bar = Barra

menu-close-vmux = Dún Vmux

agents-terminal-coding-agent = Gníomhaire códaithe sa teirminéal
