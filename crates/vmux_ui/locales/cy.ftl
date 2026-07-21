common-open = Agor
common-close = Cau
common-install = Gosod
common-uninstall = Dadosod
common-update = Diweddaru
common-retry = Ailgeisio
common-refresh = Adnewyddu
common-remove = Tynnu
common-enable = Galluogi
common-disable = Analluogi
common-new = Newydd
common-active = actif
common-running = yn rhedeg
common-done = wedi gorffen
common-failed = Wedi Methu
common-installed = Wedi'i Osod
common-items = { $count ->
    [one] { $count } eitem
   *[other] { $count } eitem
}
start-title = Dechrau
start-tagline = Un cyfarwyddiad. Unrhyw beth, wedi'i wneud.

agents-title = Asiantau
agents-search = Chwilio asiantau ACP a CLI…
agents-empty = Dim asiantau cyfatebol
agents-empty-detail = Rhowch gynnig ar enw, amser rhedeg, neu ACP/CLI.
agents-install-failed = Gosodiad wedi methu
agents-updating = Yn diweddaru…
agents-retrying = Yn ailgeisio…
agents-preparing = Yn paratoi…

extensions-title = Estyniadau
extensions-search = Chwilio estyniadau gosodedig neu Chrome Web Store…
extensions-relaunch = Ailgychwyn i gymhwyso
extensions-empty = Dim estyniadau wedi'u gosod
extensions-no-match = Dim estyniadau cyfatebol
extensions-empty-detail = Chwiliwch yn Chrome Web Store uchod a gwasgwch Return.
extensions-no-match-detail = Rhowch gynnig ar enw arall neu ID estyniad.
extensions-on = Ymlaen
extensions-off = I Ffwrdd
extensions-enable-confirm = Galluogi { $name }?
extensions-enable-permissions = Galluogi { $name } a chaniatáu:

lsp-title = Gweinyddwyr Iaith
lsp-search = Chwilio gweinyddwyr iaith, rhewlwyr, fformatwyr…
lsp-loading = Yn llwytho catalog…
lsp-empty = Dim gweinyddwyr iaith cyfatebol
lsp-empty-detail = Rhowch gynnig ar iaith, rhewlwr, neu fformatwr arall.
lsp-needs = angen { $tool }
lsp-status-available = Ar Gael
lsp-status-on-path = Ar PATH
lsp-status-installing = Yn gosod…
lsp-status-installed = Wedi'i Osod
lsp-status-outdated = Diweddariad ar gael
lsp-status-running = Yn Rhedeg
lsp-status-failed = Wedi Methu

spaces-title = Gofodau
spaces-new-placeholder = Enw gofod newydd
spaces-empty = Dim gofodau
spaces-default-name = Gofod { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
spaces-delete = Dileu gofod

team-title = Tîm
team-just-you = Chi yn unig yn y gofod hwn
team-agents = { $count ->
    [one] Chi a 1 asiant
   *[other] Chi a { $count } asiant
}
team-empty = Neb yma eto
team-you = Chi
team-agent = Asiant

services-title = Gwasanaethau Cefndir
services-processes = { $count ->
    [one] 1 proses
   *[other] { $count } proses
}
services-kill-all = Lladd Pob Un
services-not-running = Nid yw'r gwasanaeth yn rhedeg
services-start-with = Cychwyn gyda:
services-empty = Dim prosesau actif
services-filter = Hidlo prosesau…
services-no-match = Dim prosesau cyfatebol
services-connected = Wedi Cysylltu
services-disconnected = Wedi Datgysylltu
services-attached = wedi atodi
services-kill = Lladd
services-memory = Cof
services-size = Maint
services-shell = Shell

error-title = Gwall

history-search = Chwilio hanes
history-clear-all = Clirio pob un
history-clear-confirm = Clirio'r holl hanes?
history-clear-warning = Ni ellir dad-wneud hyn.
history-cancel = Canslo
history-today = Heddiw
history-yesterday = Ddoe
history-days-ago = { $count } diwrnod yn ôl
history-day-offset = Diwrnod -{ $count }

settings-title = Gosodiadau
settings-loading = Yn llwytho gosodiadau…
settings-stored = Wedi'i storio yn ~/.vmux/settings.ron
settings-other = Arall
settings-software-update = Diweddariad Meddalwedd
settings-check-updates = Gwirio am Ddiweddariadau
settings-check-updates-hint = Yn gwirio'n awtomatig wrth gychwyn a phob awr pan fo Awtodiweddaru wedi'i alluogi.
settings-update-unavailable = Ddim ar Gael
settings-update-unavailable-hint = Nid yw'r diweddarwr wedi'i gynnwys yn yr adeilad hwn.
settings-update-checking = Yn gwirio…
settings-update-checking-hint = Yn gwirio am ddiweddariadau…
settings-update-check-again = Gwirio Eto
settings-update-current = Mae Vmux yn gyfredol.
settings-update-downloading = Yn lawrlwytho…
settings-update-downloading-hint = Yn lawrlwytho Vmux { $version }…
settings-update-installing = Yn gosod…
settings-update-installing-hint = Yn gosod Vmux { $version }…
settings-update-ready = Diweddariad yn Barod
settings-update-ready-hint = Mae Vmux { $version } yn barod. Ailgychwyn i'w gymhwyso.
settings-update-try-again = Rhoi Cynnig Eto
settings-update-failed = Methu gwirio am ddiweddariadau.
settings-item = Eitem
settings-item-number = Eitem { $number }
settings-press-key = Gwasgwch allwedd…
settings-saved = Wedi'i Gadw
settings-record-key = Cliciwch i recordio cyfuniad allwedd newydd

tray-open-window = Agor Ffenestr
tray-close-window = Cau Ffenestr
tray-pause-recording = Oedi Recordio
tray-resume-recording = Ailddechrau Recordio
tray-finish-recording = Gorffen Recordio
tray-quit = Gadael Vmux

composer-attach-files = Atodi ffeiliau (/upload)
composer-remove-attachment = Tynnu atodiad

layout-back = Yn Ôl
layout-forward = Ymlaen
layout-reload = Ail-lwytho
layout-bookmark-page = Nodi'r dudalen hon
layout-remove-bookmark = Tynnu nod tudalen
layout-pin-page = Pinio'r dudalen hon
layout-unpin-page = Dad-binio'r dudalen hon
layout-manage-extensions = Rheoli estyniadau
layout-new-stack = Pentwr Newydd
layout-close-tab = Cau tab
layout-bookmark = Nod Tudalen
layout-pin = Pinio
layout-new-tab = Tab newydd
layout-team = Tîm

command-switch-space = Newid gofod…
command-search-ask = Chwilio neu ofyn…
command-new-tab-placeholder = Chwilio neu deipio URL, neu ddewis Terfynell…
command-placeholder = Teipiwch URL, chwiliwch dabiau, neu > ar gyfer gorchymynion…
command-composer-placeholder = Teipiwch / ar gyfer gorchymynion neu @ ar gyfer cyfryngau
command-send = Anfon (Enter)
command-terminal = Terfynell
command-open-terminal = Agor mewn Terfynell
command-stack = Pentwr
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
command-prompt = Anogaeth
command-new-tab = Tab newydd
command-search = Chwilio
command-open-value = Agor "{ $value }"
command-search-value = Chwilio "{ $value }"

schema-appearance = Ymddangosiad
schema-general = Cyffredinol
schema-layout = Cynllun
schema-layout-detail = Ffenestr, panelau, bar ochrol, a modrwy ffocws.
schema-agent = Asiant
schema-agent-detail = Ymddygiad asiant a chaniatâd offer.
schema-shortcuts = Llwybrau Byr
schema-shortcuts-detail = Golwg ddarllen yn unig. Golygwch settings.ron yn uniongyrchol i newid rhwymiadau.
schema-terminal = Terfynell
schema-browser = Porwr
schema-mode = Modd
schema-mode-detail = Cynllun lliw ar gyfer tudalennau gwe. Mae Dyfais yn dilyn eich system.
schema-device = Dyfais
schema-light = Golau
schema-dark = Tywyll
schema-language = Iaith
schema-language-detail = Defnyddio system, en-US, ja, neu unrhyw dag BCP 47 gyda catalog ~/.vmux/locales/<tag>.ftl cyfatebol.
schema-auto-update = Awtodiweddaru
schema-auto-update-detail = Gwirio am ddiweddariadau a'u gosod wrth gychwyn a phob awr.
schema-startup-url = URL Cychwyn
schema-startup-url-detail = Mae'n wag yn agor anogaeth y bar gorchymynion.
schema-search-engine = Peiriant chwilio
schema-search-engine-detail = Defnyddir ar gyfer chwiliadau gwe o'r Dechrau a'r bar gorchymynion.
schema-window = Ffenestr
schema-pane = Panel
schema-side-sheet = Dalen Ochrol
schema-focus-ring = Modrwy Ffocws
schema-run-placement = Caniatáu trosysgrifo lleoliad rhedeg
schema-run-placement-detail = Gadewch i asiantau ddewis modd panel rhedeg, cyfeiriad, ac angor.
schema-leader = Arweinydd
schema-leader-detail = Allwedd rhagddodiad ar gyfer llwybrau byr cord.
schema-chord-timeout = Amser Allan Cord
schema-chord-timeout-detail = Milieiliadau cyn i ragddodiad cord ddarfod.
schema-bindings = Rhwymiadau
schema-confirm-close = Cadarnhau cau
schema-confirm-close-detail = Gofyn cyn cau terfynell gyda phroses yn rhedeg.
schema-default-theme = Thema Ragosodedig
schema-default-theme-detail = Enw'r thema actif o'r rhestr themâu.
