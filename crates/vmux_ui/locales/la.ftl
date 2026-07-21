common-open = Aperi
common-close = Claude
common-install = Installa
common-uninstall = Deinstalla
common-update = Renova
common-retry = Itera
common-refresh = Relege
common-remove = Remove
common-enable = Activa
common-disable = Inactiva
common-new = Novum
common-active = activum
common-running = currens
common-done = factum
common-failed = Defecit
common-installed = Installatum
common-items = { $count ->
    [one] { $count } res
   *[other] { $count } res
}
start-title = Initium
start-tagline = Unum promptum. Quidlibet, perfectum.

agents-title = Agentes
agents-search = Quaere agentes ACP et CLI…
agents-empty = Nulli agentes congruentes
agents-empty-detail = Nomen, ambitum executionis, aut ACP/CLI tenta.
agents-install-failed = Installatio defecit
agents-updating = Renovatur…
agents-retrying = Iteratur…
agents-preparing = Paratur…

extensions-title = Extensiones
extensions-search = Quaere installatas vel in Chrome Web Store…
extensions-relaunch = Reaperi ut valeat
extensions-empty = Nullae extensiones installatae
extensions-no-match = Nullae extensiones congruentes
extensions-empty-detail = Quaere supra in Chrome Web Store et preme Return.
extensions-no-match-detail = Aliud nomen aut ID extensionis tenta.
extensions-on = Activum
extensions-off = Inactivum
extensions-enable-confirm = Activare { $name }?
extensions-enable-permissions = Activare { $name } et permittere:

lsp-title = Servientes linguarum
lsp-search = Quaere servientes linguarum, lintra, formatores…
lsp-loading = Catalogus oneratur…
lsp-empty = Nulli servientes linguarum congruentes
lsp-empty-detail = Aliam linguam, lintrum, aut formatorem tenta.
lsp-needs = eget { $tool }
lsp-status-available = Praesto
lsp-status-on-path = In PATH
lsp-status-installing = Installatur…
lsp-status-installed = Installatum
lsp-status-outdated = Renovatio praesto
lsp-status-running = Currit
lsp-status-failed = Defecit

spaces-title = Spatia
spaces-new-placeholder = Nomen novi spatii
spaces-empty = Nulla spatia
spaces-default-name = Spatium { $number }
spaces-tabs = { $count ->
    [one] 1 scheda
   *[other] { $count } schedae
}
spaces-delete = Dele spatium

team-title = Grex
team-just-you = Tu solus in hoc spatio
team-agents = { $count ->
    [one] Tu et 1 agens
   *[other] Tu et { $count } agentes
}
team-empty = Nemo hic adhuc
team-you = Tu
team-agent = Agens

services-title = Ministeria in fundo
services-processes = { $count ->
    [one] 1 processus
   *[other] { $count } processus
}
services-kill-all = Omnia opprime
services-not-running = Ministerium non currit
services-start-with = Incipe cum:
services-empty = Nulli processus activi
services-filter = Filtra processus…
services-no-match = Nulli processus congruentes
services-connected = Coniunctum
services-disconnected = Disiunctum
services-attached = adnexum
services-kill = Opprime
services-memory = Memoria
services-size = Magnitudo
services-shell = Testa

error-title = Error

history-search = Quaere historiam
history-clear-all = Omnia dele
history-clear-confirm = Delere totam historiam?
history-clear-warning = Hoc revocari non potest.
history-cancel = Abice
history-today = Hodie
history-yesterday = Heri
history-days-ago = ante { $count } dies
history-day-offset = Dies -{ $count }

settings-title = Optiones
settings-loading = Optiones onerantur…
settings-stored = Servatur in ~/.vmux/settings.ron
settings-other = Alia
settings-software-update = Renovatio programmatis
settings-check-updates = Quaere renovationes
settings-check-updates-hint = Automate quaerit in initio et singulis horis, si renovatio automatica activa est.
settings-update-unavailable = Non praesto
settings-update-unavailable-hint = Renovator huic aedificationi non inclusus est.
settings-update-checking = Quaeritur…
settings-update-checking-hint = Renovationes quaeruntur…
settings-update-check-again = Iterum quaere
settings-update-current = Vmux recentissimum est.
settings-update-downloading = Excipitur…
settings-update-downloading-hint = Vmux { $version } excipitur…
settings-update-installing = Installatur…
settings-update-installing-hint = Vmux { $version } installatur…
settings-update-ready = Renovatio parata
settings-update-ready-hint = Vmux { $version } paratum est. Restarte ut valeat.
settings-update-try-again = Iterum tenta
settings-update-failed = Renovationes quaeri non potuerunt.
settings-item = Res
settings-item-number = Res { $number }
settings-press-key = Preme clavem…
settings-saved = Servatum
settings-record-key = Preme ut novam clavium compositionem capias

tray-open-window = Aperi fenestram
tray-close-window = Claude fenestram
tray-pause-recording = Intermitte inscriptionem
tray-resume-recording = Perge inscriptionem
tray-finish-recording = Perfice inscriptionem
tray-quit = Exi e Vmux

composer-attach-files = Adnecte fasciculos (/upload)
composer-remove-attachment = Remove adnexum

layout-back = Retro
layout-forward = Porro
layout-reload = Relege
layout-bookmark-page = Nota hanc paginam
layout-remove-bookmark = Remove notam
layout-pin-page = Fige hanc paginam
layout-unpin-page = Defige hanc paginam
layout-manage-extensions = Gere extensiones
layout-new-stack = Nova strues
layout-close-tab = Claude schedam
layout-bookmark = Nota
layout-pin = Fige
layout-new-tab = Nova scheda
layout-team = Grex

command-switch-space = Muta spatium…
command-search-ask = Quaere aut interroga…
command-new-tab-placeholder = Quaere aut inscribe URL, vel elige Terminal…
command-placeholder = Inscribe URL, quaere schedas, aut > pro mandatis…
command-composer-placeholder = Inscribe / pro mandatis aut @ pro mediis
command-send = Mitte (Enter)
command-terminal = Terminal
command-open-terminal = Aperi in Terminal
command-stack = Strues
command-tabs = { $count ->
    [one] 1 scheda
   *[other] { $count } schedae
}
command-prompt = Promptum
command-new-tab = Nova scheda
command-search = Quaere
command-open-value = Aperi “{ $value }”
command-search-value = Quaere “{ $value }”

schema-appearance = Species
schema-general = Generalia
schema-layout = Dispositio
schema-layout-detail = Fenestra, regiones, laterale, et anulus focus.
schema-agent = Agens
schema-agent-detail = Mores agentis et permissiones instrumentorum.
schema-shortcuts = Compendia
schema-shortcuts-detail = Prospectus tantum legendus. Edita settings.ron directe ut ligaturas mutes.
schema-terminal = Terminal
schema-browser = Navigator
schema-mode = Modus
schema-mode-detail = Schema colorum paginarum interretialium. Instrumentum systema tuum sequitur.
schema-device = Instrumentum
schema-light = Clarum
schema-dark = Obscurum
schema-language = Lingua
schema-language-detail = Utere systemate, en-US, ja, aut quovis signo BCP 47 cum catalogo congruenti ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Renovatio automatica
schema-auto-update-detail = Quaere et installa renovationes in initio et singulis horis.
schema-startup-url = URL initiale
schema-startup-url-detail = Vacuum aperit promptum vectis mandatorum.
schema-search-engine = Machina quaerendi
schema-search-engine-detail = Adhibetur ad quaestiones interretiales ex Initio et vecte mandatorum.
schema-window = Fenestra
schema-pane = Regio
schema-side-sheet = Lamina lateralis
schema-focus-ring = Anulus focus
schema-run-placement = Permitte agenti locum executionis mutare
schema-run-placement-detail = Sine agentibus eligere modum regionis executionis, directionem, et ancoram.
schema-leader = Dux
schema-leader-detail = Clavis praefixa compendiis chordatis.
schema-chord-timeout = Tempus chordae
schema-chord-timeout-detail = Millisecunda antequam praefixum chordae exspiret.
schema-bindings = Ligaturae
schema-confirm-close = Confirma clausuram
schema-confirm-close-detail = Interroga antequam terminale cum processu currente claudatur.
schema-default-theme = Thema praedefinitum
schema-default-theme-detail = Nomen thematis activi ex indice thematum.
