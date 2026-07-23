locale-name = Cymraeg
common-open = Agor
common-close = Cau
common-install = Gosod
common-uninstall = Dadosod
common-update = Diweddaru
common-retry = Ceisio eto
common-refresh = Adnewyddu
common-remove = Tynnu
common-enable = Galluogi
common-disable = Analluogi
common-new = Newydd
common-active = ar waith
common-running = yn rhedeg
common-done = wedi gorffen
common-failed = Wedi methu
common-installed = Wedi’i osod
common-items = { $count ->
    [one] { $count } eitem
   *[other] { $count } eitem
}

tools-title = Offer
tools-search = Chwilio pecynnau, asiantau, MCP, offer iaith a ffeiliau ffurfweddu…
tools-open = Agor Offer
tools-fold = Plygu’r offer
tools-unfold = Dadblygu’r offer
tools-scanning = Wrthi’n sganio offer lleol…
tools-no-installed = Dim offer wedi’u gosod
tools-empty = Dim offer sy’n cyfateb
tools-empty-detail = Gosodwch becyn neu ychwanegwch becyn ffeiliau ffurfweddu arddull Stow.
tools-apply = Gweithredu
tools-homebrew = Homebrew
tools-homebrew-sync = Mae fformiwlâu a rhaglenni sydd wedi’u gosod yn cysoni’n awtomatig.
tools-open-brewfile = Agor Brewfile
tools-managed = dan reolaeth
tools-provider-homebrew-formulae = Fformiwlâu Homebrew
tools-provider-homebrew-casks = Rhaglenni Homebrew
tools-provider-npm = Pecynnau npm
tools-provider-acp-agents = Asiantau ACP
tools-provider-language-tools = Offer iaith
tools-provider-mcp-servers = Gweinyddion MCP
tools-provider-dotfiles = Ffeiliau ffurfweddu
tools-status-available = Ar gael
tools-status-missing = Ar goll
tools-status-conflict = Gwrthdaro
tools-forget = Anghofio
tools-manage = Rheoli
tools-link = Cysylltu
tools-unlink = Datgysylltu
tools-import = Mewnforio
tools-update-count = { $count ->
    [one] 1 diweddariad
   *[other] { $count } diweddariad
}
tools-conflict-count = { $count ->
    [one] 1 gwrthdaro
   *[other] { $count } gwrthdaro
}
tools-result-applied = Offer wedi’u gweithredu
tools-result-imported = Offer wedi’u mewnforio
tools-result-installed = Mae { $name } wedi’i osod
tools-result-updated = Mae { $name } wedi’i ddiweddaru
tools-result-uninstalled = Mae { $name } wedi’i ddadosod
tools-result-forgotten = Mae { $name } wedi’i anghofio
tools-result-managed = Mae { $name } bellach dan reolaeth
tools-result-linked = Mae { $name } wedi’i gysylltu
tools-result-unlinked = Mae { $name } wedi’i ddatgysylltu
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Gosodiadau cysoni, offer, dotfiles, a Gwybodaeth gyda Git.
vault-sync = Cysoni
vault-create = Creu
vault-connect = Cyswllt
vault-private = Ystorfa breifat
vault-public-warning = Mae cadwrfeydd cyhoeddus yn datgelu eich Gwybodaeth a'ch ffurfwedd.
vault-choose-repository = Dewiswch ystorfa…
vault-empty = gwag
vault-clean = Yn gyfoes
vault-not-connected = Heb ei gysylltu
vault-change-count = Newidiadau: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Cychwyn
start-tagline = Un prompt. Popeth wedi’i wneud.

agents-title = Asiantiaid
agents-search = Chwilio asiantiaid ACP a CLI…
agents-empty = Dim asiantiaid sy’n cyfateb
agents-empty-detail = Rhowch enw, runtime, neu ACP/CLI.
agents-install-failed = Methodd y gosod
agents-updating = Yn diweddaru…
agents-retrying = Yn ceisio eto…
agents-preparing = Yn paratoi…

extensions-title = Estyniadau
extensions-search = Chwilio’r rhai sydd wedi’u gosod neu Chrome Web Store…
extensions-relaunch = Ail-lansiwch i gymhwyso
extensions-empty = Dim estyniadau wedi’u gosod
extensions-no-match = Dim estyniadau sy’n cyfateb
extensions-empty-detail = Chwiliwch Chrome Web Store uchod a phwyswch Return.
extensions-no-match-detail = Rhowch enw neu ID estyniad arall.
extensions-on = Ymlaen
extensions-off = I ffwrdd
extensions-enable-confirm = Galluogi { $name }?
extensions-enable-permissions = Galluogi { $name } a chaniatáu:

lsp-title = Gweinyddion Iaith
lsp-search = Chwilio gweinyddion iaith, linters, fformatwyr…
lsp-loading = Yn llwytho’r catalog…
lsp-empty = Dim gweinyddion iaith sy’n cyfateb
lsp-empty-detail = Rhowch iaith, linter neu fformatiwr arall.
lsp-needs = angen { $tool }
lsp-status-available = Ar gael
lsp-status-on-path = Ar PATH
lsp-status-installing = Yn gosod…
lsp-status-installed = Wedi’i osod
lsp-status-outdated = Diweddariad ar gael
lsp-status-running = Yn rhedeg
lsp-status-failed = Wedi methu

spaces-title = Lleoedd
spaces-new-placeholder = Enw lle newydd
spaces-empty = Dim lleoedd
spaces-default-name = Lle { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
spaces-delete = Dileu lle

team-title = Tîm
team-just-you = Dim ond chi yn y lle hwn
team-agents = { $count ->
    [one] Chi ac 1 asiant
   *[other] Chi a { $count } asiant
}
team-empty = Does neb yma eto
team-you = Chi
team-agent = Asiant

services-title = Gwasanaethau Cefndir
services-processes = { $count ->
    [one] 1 broses
   *[other] { $count } proses
}
services-kill-all = Lladd Pob Un
services-not-running = Nid yw’r gwasanaeth yn rhedeg
services-start-with = Cychwyn gyda:
services-empty = Dim prosesau gweithredol
services-filter = Hidlo prosesau…
services-no-match = Dim prosesau sy’n cyfateb
services-connected = Wedi cysylltu
services-disconnected = Wedi datgysylltu
services-attached = wedi atodi
services-kill = Lladd
services-memory = Cof
services-size = Maint
services-shell = Cragen

error-title = Gwall

history-search = Chwilio hanes
history-clear-all = Clirio popeth
history-clear-confirm = Clirio’r holl hanes?
history-clear-warning = Ni ellir dadwneud hyn.
history-cancel = Canslo
history-today = Heddiw
history-yesterday = Ddoe
history-days-ago = { $count } diwrnod yn ôl
history-day-offset = Diwrnod -{ $count }

settings-title = Gosodiadau
settings-loading = Yn llwytho gosodiadau…
settings-stored = Wedi’i storio yn ~/.vmux/settings.ron
settings-other = Arall
settings-software-update = Diweddariad Meddalwedd
settings-check-updates = Gwirio am Ddiweddariadau
settings-check-updates-hint = Yn gwirio’n awtomatig wrth lansio ac bob awr pan fo awto-ddiweddaru wedi’i alluogi.
settings-update-unavailable = Ddim ar gael
settings-update-unavailable-hint = Nid yw’r diweddarydd wedi’i gynnwys yn yr adeilad hwn.
settings-update-checking = Yn gwirio…
settings-update-checking-hint = Yn gwirio am ddiweddariadau…
settings-update-check-again = Gwirio Eto
settings-update-current = Mae Vmux yn gyfredol.
settings-update-downloading = Yn lawrlwytho…
settings-update-downloading-hint = Yn lawrlwytho Vmux { $version }…
settings-update-installing = Yn gosod…
settings-update-installing-hint = Yn gosod Vmux { $version }…
settings-update-ready = Diweddariad yn Barod
settings-update-ready-hint = Mae Vmux { $version } yn barod. Ailgychwyn i’w gymhwyso.
settings-update-try-again = Ceisio Eto
settings-update-failed = Methu gwirio am ddiweddariadau.
settings-item = Eitem
settings-item-number = Eitem { $number }
settings-press-key = Pwyswch fysell…
settings-saved = Wedi cadw
settings-record-key = Cliciwch i recordio cyfuniad bysellau newydd

tray-open-window = Agor Ffenestr
tray-close-window = Cau Ffenestr
tray-pause-recording = Seibio Recordio
tray-resume-recording = Ailddechrau Recordio
tray-finish-recording = Gorffen Recordio
tray-quit = Gadael Vmux

composer-attach-files = Atodi ffeiliau (/upload)
composer-remove-attachment = Tynnu atodiad

layout-back = Nôl
layout-forward = Ymlaen
layout-reload = Ail-lwytho
layout-bookmark-page = Nod tudalen i’r dudalen hon
layout-remove-bookmark = Tynnu nod tudalen
layout-pin-page = Pinio’r dudalen hon
layout-unpin-page = Dadbinio’r dudalen hon
layout-manage-extensions = Rheoli estyniadau
layout-new-stack = Stac Newydd
layout-close-tab = Cau tab
layout-bookmark = Nod tudalen
layout-pin = Pinio
layout-new-tab = Tab newydd
layout-team = Tîm

command-switch-space = Newid lle…
command-search-ask = Chwilio neu ofyn…
command-new-tab-placeholder = Chwiliwch neu deipiwch URL, neu dewiswch Derfynell…
command-placeholder = Teipiwch URL, chwiliwch dabiau, neu > ar gyfer gorchmynion…
command-composer-placeholder = Teipiwch / ar gyfer gorchmynion neu @ ar gyfer cyfryngau
command-send = Anfon (Enter)
command-terminal = Terfynell
command-open-terminal = Agor yn y Derfynell
command-stack = Stac
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
command-prompt = Prompt
command-new-tab = Tab newydd
command-search = Chwilio
command-open-value = Agor “{ $value }”
command-search-value = Chwilio “{ $value }”

schema-appearance = Golwg
schema-general = Cyffredinol
schema-layout = Cynllun
schema-layout-detail = Ffenestr, cwareli, bar ochr a chylch ffocws.
schema-agent = Asiant
schema-agent-detail = Ymddygiad asiantiaid a chaniatâd offer.
schema-shortcuts = Llwybrau Byr
schema-shortcuts-detail = Golwg darllen-yn-unig. Golygwch settings.ron yn uniongyrchol i newid rhwymiadau.
schema-terminal = Terfynell
schema-browser = Porwr
schema-mode = Modd
schema-mode-detail = Cynllun lliw ar gyfer tudalennau gwe. Mae Dyfais yn dilyn eich system.
schema-device = Dyfais
schema-light = Golau
schema-dark = Tywyll
schema-language = Iaith
schema-language-detail = Defnyddiwch y system, en-US, ja, neu unrhyw dag BCP 47 gyda chatalog ~/.vmux/locales/<tag>.ftl cyfatebol.
schema-auto-update = Awto-ddiweddaru
schema-auto-update-detail = Gwirio am ddiweddariadau a’u gosod wrth lansio ac bob awr.
schema-startup-url = URL Cychwyn
schema-startup-url-detail = Os yw’n wag, agorir prompt y bar gorchmynion.
schema-search-engine = Peiriant chwilio
schema-search-engine-detail = Defnyddir ar gyfer chwiliadau gwe o Cychwyn a’r bar gorchmynion.
schema-window = Ffenestr
schema-pane = Cwarel
schema-side-sheet = Panel ochr
schema-focus-ring = Cylch ffocws
schema-run-placement = Caniatáu i leoliad rhedeg gael ei wrthwneud
schema-run-placement-detail = Caniatáu i asiantiaid ddewis modd cwarel rhedeg, cyfeiriad ac angor.
schema-leader = Arweinydd
schema-leader-detail = Bysell rhagddodi ar gyfer llwybrau byr cord.
schema-chord-timeout = Goramser cord
schema-chord-timeout-detail = Milieiliadau cyn i ragddodiad cord ddod i ben.
schema-bindings = Rhwymiadau
schema-confirm-close = Cadarnhau cau
schema-confirm-close-detail = Gofyn cyn cau terfynell â phroses yn rhedeg.
schema-default-theme = Thema ddiofyn
schema-default-theme-detail = Enw’r thema weithredol o’r rhestr themâu.

settings-empty = (gwag)
settings-none = (dim)

schema-system = System
schema-editor = Golygydd
schema-recording = Recordio
schema-radius = Radiws
schema-padding = Padin
schema-gap = Bwlch
schema-width = Lled
schema-color = Lliw
schema-red = Coch
schema-green = Gwyrdd
schema-blue = Glas
schema-follow-files = Dilyn ffeiliau
schema-tidy-files = Tacluso ffeiliau
schema-tidy-files-max = Trothwy tacluso ffeiliau
schema-tidy-files-auto = Tacluso ffeiliau'n awtomatig
schema-app-providers = Darparwyr apiau
schema-provider = Darparwr
schema-kind = Math
schema-models = Modelau
schema-acp = Asiantau ACP
schema-id = ID
schema-name = Enw
schema-command = Gorchymyn
schema-arguments = Ymresymiadau
schema-environment = Amgylchedd
schema-working-directory = Cyfeiriadur gweithio
schema-shell = Cragen
schema-font-family = Teulu ffont
schema-startup-directory = Cyfeiriadur cychwyn
schema-themes = Themâu
schema-color-scheme = Cynllun lliwiau
schema-font-size = Maint ffont
schema-line-height = Uchder llinell
schema-cursor-style = Arddull cyrchwr
schema-cursor-blink = Amrantiad cyrchwr
schema-custom-themes = Themâu personol
schema-foreground = Blaendir
schema-background = Cefndir
schema-cursor = Cyrchwr
schema-ansi-colors = Lliwiau ANSI
schema-keymap = Map bysellau
schema-explorer = Archwiliwr
schema-visible = Gweladwy
schema-language-servers = Gweinyddion iaith
schema-servers = Gweinyddion
schema-language-id = ID iaith
schema-root-markers = Marcwyr gwraidd
schema-output-directory = Cyfeiriadur allbwn

menu-scene = Golygfa
menu-layout = Cynllun
menu-terminal = Terfynell
menu-browser = Porwr
menu-service = Gwasanaeth
menu-bookmark = Nod Tudalen
menu-edit = Golygu

layout-knowledge = Gwybodaeth
layout-open-knowledge = Agor Gwybodaeth
layout-open-welcome-knowledge = Agor Croeso i Wybodaeth
layout-open-path = Agor { $path }
layout-fold-knowledge = Plygu gwybodaeth
layout-unfold-knowledge = Dadblygu gwybodaeth
layout-bookmarks = Nodau tudalen
layout-new-folder = Ffolder newydd
layout-add-to-bookmarks = Ychwanegu at Nodau Tudalen
layout-move-to-bookmarks = Symud i Nodau Tudalen
layout-stack-number = Stac { $number }
layout-fold-stack = Plygu stac
layout-unfold-stack = Dadblygu stac
layout-close-stack = Cau stac
layout-bookmark-in = Nodi tudalen yn { $folder }

common-cancel = Canslo
common-delete = Dileu
common-save = Cadw
common-rename = Ailenwi
common-expand = Ehangu
common-collapse = Cwympo
common-loading = Yn llwytho…
common-error = Gwall
common-output = Allbwn
common-pending = Yn aros
common-current = presennol
common-stop = Stopio
services-command = Gwasanaeth Vmux
services-uptime-seconds = { $seconds }e
services-uptime-minutes = { $minutes }m { $seconds }e
services-uptime-hours = { $hours }a { $minutes }m
services-uptime-days = { $days }d { $hours }a

error-page-failed-load = Methodd y dudalen â llwytho
error-page-not-found = Heb ganfod y dudalen
error-unknown-host = Gwesteiwr ap Vmux anhysbys: { $host }

history-title = Hanes

command-new-app-chat = Sgwrs { $provider }/{ $model } newydd (Ap)
command-interactive-mode-user = Golygfa > Modd Rhyngweithiol > Defnyddiwr
command-interactive-mode-player = Golygfa > Modd Rhyngweithiol > Chwaraewr
command-minimize-window = Cynllun > Ffenestr > Lleihau
command-toggle-layout = Cynllun > Cynllun > Toglo Cynllun
command-close-tab = Cynllun > Tab > Cau Tab
command-new-task = Cynllun > Tab > Tasg Newydd…
command-next-tab = Cynllun > Tab > Tab Nesaf
command-prev-tab = Cynllun > Tab > Tab Blaenorol
command-rename-tab = Cynllun > Tab > Ailenwi Tab
command-tab-select-1 = Cynllun > Tab > Dewis Tab 1
command-tab-select-2 = Cynllun > Tab > Dewis Tab 2
command-tab-select-3 = Cynllun > Tab > Dewis Tab 3
command-tab-select-4 = Cynllun > Tab > Dewis Tab 4
command-tab-select-5 = Cynllun > Tab > Dewis Tab 5
command-tab-select-6 = Cynllun > Tab > Dewis Tab 6
command-tab-select-7 = Cynllun > Tab > Dewis Tab 7
command-tab-select-8 = Cynllun > Tab > Dewis Tab 8
command-tab-select-last = Cynllun > Tab > Dewis y Tab Olaf
command-close-pane = Cynllun > Paen > Cau Paen
command-select-pane-left = Cynllun > Paen > Dewis Paen Chwith
command-select-pane-right = Cynllun > Paen > Dewis Paen De
command-select-pane-up = Cynllun > Paen > Dewis Paen Uchod
command-select-pane-down = Cynllun > Paen > Dewis Paen Isod
command-swap-pane-prev = Cynllun > Paen > Cyfnewid â’r Paen Blaenorol
command-swap-pane-next = Cynllun > Paen > Cyfnewid â’r Paen Nesaf
command-equalize-pane-size = Cynllun > Paen > Cyfartalu Maint Paenau
command-resize-pane-left = Cynllun > Paen > Newid Maint Paen i’r Chwith
command-resize-pane-right = Cynllun > Paen > Newid Maint Paen i’r Dde
command-resize-pane-up = Cynllun > Paen > Newid Maint Paen i Fyny
command-resize-pane-down = Cynllun > Paen > Newid Maint Paen i Lawr
command-stack-close = Cynllun > Stac > Cau Stac
command-stack-next = Cynllun > Stac > Stac Nesaf
command-stack-previous = Cynllun > Stac > Stac Blaenorol
command-stack-reopen = Cynllun > Stac > Ailagor Tudalen a Gaewyd
command-stack-swap-prev = Cynllun > Stac > Symud Stac i’r Chwith
command-stack-swap-next = Cynllun > Stac > Symud Stac i’r Dde
command-space-open = Cynllun > Gofod > Gofodau
command-terminal-close = Terfynell > Cau Terfynell
command-terminal-next = Terfynell > Terfynell Nesaf
command-terminal-prev = Terfynell > Terfynell Flaenorol
command-terminal-clear = Terfynell > Clirio Terfynell
command-browser-prev-page = Porwr > Llywio > Nôl
command-browser-next-page = Porwr > Llywio > Ymlaen
command-browser-reload = Porwr > Llywio > Ail-lwytho
command-browser-hard-reload = Porwr > Llywio > Ail-lwytho’n Llwyr
command-open-in-place = Porwr > Agor > Agor Yma
command-open-in-new-stack = Porwr > Agor > Agor mewn Stac Newydd
command-open-in-pane-top = Porwr > Agor > Agor yn y Paen Uchod
command-open-in-pane-right = Porwr > Agor > Agor yn y Paen De
command-open-in-pane-bottom = Porwr > Agor > Agor yn y Paen Isod
command-open-in-pane-left = Porwr > Agor > Agor yn y Paen Chwith
command-open-in-new-tab = Porwr > Agor > Agor mewn Tab Newydd
command-open-in-new-space = Porwr > Agor > Agor mewn Gofod Newydd
command-browser-zoom-in = Porwr > Golwg > Chwyddo Mewn
command-browser-zoom-out = Porwr > Golwg > Chwyddo Allan
command-browser-zoom-reset = Porwr > Golwg > Maint Gwirioneddol
command-browser-dev-tools = Porwr > Golwg > Offer Datblygwyr
command-browser-open-command-bar = Porwr > Bar > Bar Gorchmynion
command-browser-open-page-in-command-bar = Porwr > Bar > Golygu Tudalen
command-browser-open-path-bar = Porwr > Bar > Llywiwr Llwybr
command-browser-open-commands = Porwr > Bar > Gorchmynion
command-browser-open-history = Porwr > Bar > Hanes
command-service-open = Gwasanaeth > Agor Monitor Gwasanaeth
command-bookmark-toggle-active = Nod Tudalen > Nodi Tudalen
command-bookmark-pin-active = Nod Tudalen > Pinio Tudalen

layout-tab = Tab
layout-no-stacks = Dim staciau
layout-loading = Yn llwytho…
layout-no-markdown-files = Dim ffeiliau Markdown
layout-empty-folder = Ffolder wag
layout-worktree = coeden waith
layout-folder-name = Enw ffolder
layout-no-pins-bookmarks = Dim pinnau na nodau tudalen
layout-move-to = Symud i { $folder }
layout-bookmark-current-page = Nodi’r Dudalen Bresennol
layout-rename-folder = Ailenwi Ffolder
layout-remove-folder = Tynnu Ffolder
layout-update-downloading = Yn lawrlwytho diweddariad
layout-update-installing = Yn gosod diweddariad…
layout-update-ready = Fersiwn newydd ar gael
layout-restart-update = Ailgychwyn i ddiweddaru

agent-preparing = Yn paratoi’r asiant…
agent-send-all-queued = Anfon pob anogiad yn y ciw nawr (Esc)
agent-send = Anfon (Enter)
agent-ready = Yn barod pan fyddwch chi.
agent-loading-older = Yn llwytho negeseuon hŷn…
agent-load-older = Llwytho negeseuon hŷn
agent-continued-from = Parhawyd o { $source }
agent-older-context-omitted = hepgorwyd cyd-destun hŷn
agent-interrupted = ymyrrwyd
agent-allow-tool = Caniatáu { $tool }?
agent-deny = Gwrthod
agent-allow-always = Caniatáu bob amser
agent-allow = Caniatáu
agent-loading-sessions = Yn llwytho sesiynau…
agent-no-resumable-sessions = Heb ganfod sesiynau y gellir eu hailddechrau
agent-no-matching-sessions = Dim sesiynau sy’n cyfateb
agent-no-matching-models = Dim modelau sy’n cyfateb
agent-choice-help = ↑/↓ neu Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Dewis ffolder y gadwrfa
agent-choose-repository-detail = Dewiswch y gadwrfa Git leol y dylai’r asiant ei defnyddio.
agent-choosing = Yn dewis…
agent-choose-folder = Dewis ffolder
agent-queued = yn y ciw
agent-attached = Atodwyd:
agent-cancel-queued = Canslo anogiad yn y ciw
agent-resume-queued = Ailddechrau anogiadau yn y ciw
agent-clear-queue = Clirio’r ciw
agent-send-all-now = anfon popeth nawr
agent-choose-option = Dewiswch opsiwn uchod
agent-loading-media = Yn llwytho cyfryngau…
agent-no-matching-media = Dim cyfryngau sy’n cyfateb
agent-prompt-context = Cyd-destun anogiad
agent-details = Manylion
agent-path = Llwybr
agent-tool = Offeryn
agent-server = Gweinydd
agent-bytes = { $count } beit
agent-worked-for = Gweithiodd am { $duration }
agent-worked-for-steps = { $count ->
    [one] Gweithiodd am { $duration } · 1 cam
   *[other] Gweithiodd am { $duration } · { $count } cam
}
agent-tool-guardian-review = Adolygiad Guardian
agent-tool-read-files = Darllen ffeiliau
agent-tool-viewed-image = Gweld delwedd
agent-tool-used-browser = Defnyddio porwr
agent-tool-searched-files = Chwilio ffeiliau
agent-tool-ran-commands = Rhedeg gorchmynion
agent-thinking = Yn meddwl
agent-subagent = Is-asiant
agent-prompt = Anogiad
agent-thread = Trywydd
agent-parent = Rhiant
agent-children = Plant
agent-call = Galwad
agent-raw-event = Digwyddiad crai
agent-plan = Cynllun
agent-tasks = { $count ->
    [one] 1 dasg
   *[other] { $count } tasg
}
agent-edited = Golygwyd
agent-reconnecting = Yn ailgysylltu { $attempt }/{ $total }
agent-status-running = Yn rhedeg
agent-status-done = Wedi gorffen
agent-status-failed = Wedi methu
agent-status-pending = Yn aros
agent-slash-attach-files = Atodi ffeiliau
agent-slash-resume-session = Ailddechrau sesiwn flaenorol
agent-slash-select-model = Dewis model
agent-slash-continue-cli = Parhau â’r sesiwn hon yn y CLI
agent-session-just-now = newydd ddigwydd
agent-session-minutes-ago = { $count }m yn ôl
agent-session-hours-ago = { $count }a yn ôl
agent-session-days-ago = { $count }d yn ôl
agent-working-working = Yn gweithio
agent-working-thinking = Yn meddwl
agent-working-pondering = Yn ystyried
agent-working-noodling = Yn chwarae â syniadau
agent-working-percolating = Yn mudferwi
agent-working-conjuring = Yn consurio
agent-working-cooking = Yn coginio
agent-working-brewing = Yn bragu
agent-working-musing = Yn myfyrio
agent-working-ruminating = Yn cnoi cil
agent-working-scheming = Yn cynllunio
agent-working-synthesizing = Yn syntheseiddio
agent-working-tinkering = Yn tincro
agent-working-churning = Yn prosesu
agent-working-vibing = Yn dal y naws
agent-working-simmering = Yn mudferwi
agent-working-crafting = Yn crefftio
agent-working-divining = Yn darogan
agent-working-mulling = Yn pwyso a mesur
agent-working-spelunking = Yn cloddio’n ddwfn

editor-toggle-explorer = Toglo’r Archwiliwr (Cmd+B)
editor-unsaved = heb ei gadw
editor-rendered-markdown = Markdown wedi’i rendro gyda golygu byw
editor-note = Nodyn
editor-source-editor = Golygydd ffynhonnell
editor-editor = Golygydd
editor-git-diff = Diff Git
editor-diff = Diff
editor-tidy = Tacluso
editor-always = Bob amser
editor-unchanged-previews = { $count ->
    [one] ✦ 1 rhagolwg heb ei newid
   *[other] ✦ { $count } rhagolwg heb eu newid
}
editor-open-externally = Agor yn allanol
editor-changed-line = Llinell wedi’i newid
editor-go-to-definition = Mynd i’r Diffiniad
editor-find-references = Canfod Cyfeiriadau
editor-references = { $count ->
    [one] 1 cyfeiriad
   *[other] { $count } cyfeiriad
}
editor-lsp-starting = { $server } yn cychwyn…
editor-lsp-not-installed = { $server } — heb ei osod
editor-explorer = Archwiliwr
editor-open-editors = Golygyddion Agored
editor-outline = Amlinelliad
editor-new-file = Ffeil Newydd
editor-new-folder = Ffolder Newydd
editor-delete-confirm = Dileu “{ $name }”? Ni ellir dadwneud hyn.
editor-created-folder = Crëwyd ffolder { $name }
editor-created-file = Crëwyd ffeil { $name }
editor-renamed-to = Ailenwyd yn { $name }
editor-deleted = Dilëwyd { $name }
editor-failed-decode-image = Methwyd â dadgodio’r ddelwedd
editor-preview-large-image = delwedd (rhy fawr i’w rhagweld)
editor-preview-binary = deuaidd
editor-preview-file = ffeil

git-status-clean = glân
git-status-modified = wedi’i addasu
git-status-staged = wedi’i lwyfannu
git-status-staged-modified = wedi’i lwyfannu*
git-status-untracked = heb ei olrhain
git-status-deleted = wedi’i ddileu
git-status-conflict = gwrthdaro
git-accept-all = ✓ derbyn popeth
git-unstage = Dadlwyfannu
git-confirm-deny-all = Cadarnhau gwrthod popeth
git-deny-all = ✗ gwrthod popeth
git-commit-message = neges commit
git-commit = Commitio ({ $count })
git-push = ↑ Gwthio
git-loading-diff = Yn llwytho diff…
git-no-changes = Dim newidiadau i’w dangos
git-accept = ✓ derbyn
git-deny = ✗ gwrthod
git-show-unchanged-lines = Dangos { $count } llinell heb eu newid

terminal-loading = Yn llwytho…
terminal-runs-when-ready = yn rhedeg pan yn barod · Ctrl+C yn clirio · Esc yn hepgor
terminal-booting = yn cychwyn
terminal-type-command = teipiwch orchymyn · yn rhedeg pan yn barod · Esc yn hepgor

setup-tagline-claude = Asiant codio Anthropic, yn Vmux
setup-tagline-codex = Asiant codio OpenAI, yn Vmux
setup-tagline-vibe = Asiant codio Mistral, yn Vmux
setup-install-title = Gosod CLI { $name }
setup-homebrew-required = Mae angen Homebrew i osod { $command } ac nid yw wedi’i sefydlu eto. Bydd Vmux yn gosod Homebrew yn gyntaf, wedyn { $name }.
setup-terminal-instructions = Yn y derfynell, pwyswch Return i ddechrau, yna rhowch gyfrinair eich Mac pan ofynnir.
setup-command-missing = Agorodd Vmux y dudalen hon oherwydd nad yw’r gorchymyn lleol { $command } wedi’i osod eto. Rhedeg y gorchymyn isod i’w gael.
setup-install-failed = Ni orffennodd y gosod. Gwiriwch y derfynell am fanylion, yna rhowch gynnig arall arni.
setup-installing = Yn gosod…
setup-install-homebrew = Gosod Homebrew + { $name }
setup-run-install = Rhedeg gorchymyn gosod
setup-auto-reload = Mae Vmux yn ei redeg mewn terfynell ac yn ail-lwytho pan fydd { $command } yn barod.

debug-title = Dadfygio
debug-auto-update = Diweddaru’n awtomatig
debug-simulate-update = Efelychu diweddariad ar gael
debug-simulate-download = Efelychu lawrlwythiad
debug-clear-update = Clirio diweddariad
debug-trigger-restart = Sbarduno ailgychwyn

command-manage-spaces = Rheoli mannau…
command-pane-stack-location = cwarel { $pane } / pentwr { $stack }
command-space-pane-stack-location = { $space } / cwarel { $pane } / pentwr { $stack }
command-terminal-path = Terfynell ({ $path })
command-group-interactive-mode = Modd rhyngweithiol
command-group-window = Ffenestr
command-group-tab = Tab
command-group-pane = Cwarel
command-group-stack = Pentwr
command-group-space = Man
command-group-navigation = Llywio
command-group-open = Agor
command-group-view = Golwg
command-group-bar = Bar

menu-close-vmux = Cau Vmux

agents-terminal-coding-agent = Asiant codio yn y derfynell
