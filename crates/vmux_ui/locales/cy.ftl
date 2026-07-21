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
