common-open = Open
common-close = Close
common-install = install
common-uninstall = Uninstall
common-update = Renovatio
common-retry = Retry
common-refresh = Renovare
common-remove = Aufer
common-enable = Admitte
common-disable = inactivare
common-new = New
common-active = active
common-running = cursus
common-done = factum
common-failed = Defecit
common-installed = installed
common-items = { $count ->
    [one] { $count } item
   *[other] { $count } items
}
start-title = Satus
start-tagline = Una prompta. Quidvis, feci.

agents-title = Agentes
agents-search = Quaere ACP et CLI agentium…
agents-empty = Non matching agentium
agents-empty-detail = Nomen experire, runtime, vel ACP/CLI.
agents-install-failed = Install defuit
agents-updating = Adaequationis…
agents-retrying = Retrying…
agents-preparing = Praeparans…

extensions-title = Extensiones
extensions-search = Quaerere institutum vel Chrome Web Store…
extensions-relaunch = Relaunch adhibere
extensions-empty = Nulla extensiones installed
extensions-no-match = Non matching extensiones
extensions-empty-detail = Quaere supra Chrome Web Store et preme Return.
extensions-no-match-detail = Conare nomen aliud seu extensio ID.
extensions-on = On
extensions-off = Off
extensions-enable-confirm = Admitte { $name }?
extensions-enable-permissions = Admitte { $name } et permitte;

lsp-title = Lingua Server
lsp-search = Quaerere servitores linguae, linteamina, formatores...
lsp-loading = Loading catalog...
lsp-empty = Non matching linguarum servers
lsp-empty-detail = Aliam linguam experire, linteolum, aut formatorem.
lsp-needs = necesse est { $tool }
lsp-status-available = Praesto
lsp-status-on-path = Pridie PATH
lsp-status-installing = Installing…
lsp-status-installed = installed
lsp-status-outdated = Renovatio available
lsp-status-running = Running
lsp-status-failed = Defecit

spaces-title = Spatia
spaces-new-placeholder = Novum spatii nomen
spaces-empty = Nulla spatia
spaces-default-name = Spatium { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tabs
}
spaces-delete = Delere spatium

team-title = Team
team-just-you = Iustus in hoc spatio
team-agents = { $count ->
    [one] Tu et I agente
   *[other] Tu et { $count } actores
}
team-empty = Nemo tamen hic
team-you = Tu
team-agent = Agens

services-title = Background Services
services-processes = { $count ->
    [one] 1 processum
   *[other] { $count } processuum
}
services-kill-all = omnes occiditis
services-not-running = Ministerium non est currens
services-start-with = Incipere:
services-empty = Nullae actiones activae
services-filter = Filtrum processuum…
services-no-match = Non matching processuum
services-connected = Coniuncta
services-disconnected = Disiungitur
services-attached = attachiatus
services-kill = Occidere
services-memory = Memoria
services-size = Magnitudo
services-shell = Testa

error-title = Error

history-search = Quaerere historiam
history-clear-all = Patet omnia
history-clear-confirm = Patet omnem historiam?
history-clear-warning = Hoc fieri infectum non potest.
history-cancel = Cancel
history-today = hodie
history-yesterday = heri
history-days-ago = { $count } diebus abhinc
history-day-offset = Dies -{ $count }

settings-title = Occasus
settings-loading = Loading occasus…
settings-stored = Conditum in ~/.vmux/settings.ron
settings-other = Other
settings-software-update = Software Update
settings-check-updates = Reprehendo pro Updates
settings-check-updates-hint = Automatice in launch et in omni hora, cum auto-renovatio est facultas.
settings-update-unavailable = Unavailable
settings-update-unavailable-hint = Renovator in hac aedificatione non comprehenditur.
settings-update-checking = Reperiens…
settings-update-checking-hint = Reprehendo pro updates ...
settings-update-check-again = Iterum reprehendo
settings-update-current = Vmux usque ad date est.
settings-update-downloading = Downloading…
settings-update-downloading-hint = Download Vmux { $version }…
settings-update-installing = Installing…
settings-update-installing-hint = Inaugurari Vmux { $version }…
settings-update-ready = Update Promptus
settings-update-ready-hint = Vmux { $version } parata est. Sileo adhibere.
settings-update-try-again = Iterum conare
settings-update-failed = Posse reprimendam updates.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Preme clavis…
settings-saved = salvus
settings-record-key = Click to recordarentur novam clavis combo

tray-open-window = Open Fenestra
tray-close-window = Prope Fenestra
tray-pause-recording = Declina Recordatio
tray-resume-recording = Proin Recordatio
tray-finish-recording = Finis Book
tray-quit = quit Vmux

composer-attach-files = Documenta affigere (/upload)
composer-remove-attachment = Remove affectum

layout-back = Retro
layout-forward = Deinceps
layout-reload = Reload
layout-bookmark-page = Hanc paginam signare
layout-remove-bookmark = Aufer Bookmark
layout-pin-page = Hanc paginam pin
layout-unpin-page = Unpin hanc paginam
layout-manage-extensions = Curo extensiones
layout-new-stack = Novum Stack
layout-close-tab = Prope tab
layout-bookmark = Bookmark
layout-pin = Pin
layout-new-tab = Nova tab
layout-team = Team

command-switch-space = Spatium commutandum…
command-search-ask = Quaerere vel quaerere…
command-new-tab-placeholder = Investigare vel typus a URL, vel eligere Terminationem…
command-placeholder = Typus a URL, tabs quaere, vel > ad imperia…
command-composer-placeholder = Typus / ad imperium seu @ ad media
command-send = Mitte (Enter)
command-terminal = Terminal
command-open-terminal = Apertum in Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tabs
}
command-prompt = promptus
command-new-tab = Nova tab
command-search = Investigatio
command-open-value = Aperi "{ $value }"
command-search-value = Quaerere "{ $value }"

schema-appearance = Aspectus
schema-general = General
schema-layout = Layout
schema-layout-detail = Fenestra, panes, pars, et focus anulus.
schema-agent = Agens
schema-agent-detail = Agens morum et instrumentum permissionum.
schema-shortcuts = Compendia
schema-shortcuts-detail = Read-tantum visum. Recensere settings.ron directe ad ligamenta varianda.
schema-terminal = Terminal
schema-browser = Pasco
schema-mode = Modus
schema-mode-detail = Color schema per paginas. Fabrica ratio tua sequitur.
schema-device = Device
schema-light = lux
schema-dark = Dark
schema-language = Linguae
schema-language-detail = Systema utere, en-US, ia, vel quaevis BCP 47 tag cum congruens ~/.vmux/locales/<tag>.ftl catalogo.
schema-auto-update = Auto-renovatio
schema-auto-update-detail = Reprehendo pro ac renovationes in launch et in omni hora niteremur.
schema-startup-url = Satum URL
schema-startup-url-detail = Inani prompta aperiri mandamus forensibus per in.
schema-search-engine = Quaere engine
schema-search-engine-detail = Adhibentur pro inquisitionibus interretialibus ab initio et vecte praecepti.
schema-window = Fenestra
schema-pane = Pane
schema-side-sheet = Parte sheet
schema-focus-ring = Focus anulus
schema-run-placement = Liceat currere collocatione override
schema-run-placement-detail = Agentes eligant modum, directionem, anchoram currunt.
schema-leader = Dux
schema-leader-detail = Clavis praepositionis ad chordas chordarum.
schema-chord-timeout = chorda timeout
schema-chord-timeout-detail = Milliseconds quam praepositionem funem expirat.
schema-bindings = Vincula
schema-confirm-close = Confírma prope
schema-confirm-close-detail = Promptus ante claudendo terminatio currit processus.
schema-default-theme = Default theme
schema-default-theme-detail = Nomen thematis activi a themate ponit.
