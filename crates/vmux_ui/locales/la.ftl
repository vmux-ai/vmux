locale-name = lingua Latina
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

tools-title = Instrumenta
tools-search = Quaere fasciculos, agentes, MCP, instrumenta linguarum et tabulas configurationis…
tools-open = Aperi instrumenta
tools-fold = Complica instrumenta
tools-unfold = Explica instrumenta
tools-scanning = Instrumenta localia explorantur…
tools-no-installed = Nulla instrumenta instituta
tools-empty = Nulla instrumenta congruentia
tools-empty-detail = Fasciculum institue aut fasciculum tabularum configurationis more Stow adde.
tools-apply = Applica
tools-homebrew = Homebrew
tools-homebrew-sync = Formulae et applicationes institutae automatice synchronizantur.
tools-open-brewfile = Aperi Brewfile
tools-managed = administratum
tools-provider-homebrew-formulae = Formulae Homebrew
tools-provider-homebrew-casks = Applicationes Homebrew
tools-provider-npm = Fasciculi npm
tools-provider-acp-agents = Agentes ACP
tools-provider-language-tools = Instrumenta linguarum
tools-provider-mcp-servers = Servitores MCP
tools-provider-dotfiles = Tabulae configurationis
tools-status-available = Praesto
tools-status-missing = Abest
tools-status-conflict = Conflictus
tools-forget = Oblivisci
tools-manage = Administra
tools-link = Coniunge
tools-unlink = Disiunge
tools-import = Importa
tools-update-count = { $count ->
    [one] 1 renovatio
   *[other] { $count } renovationes
}
tools-conflict-count = { $count ->
    [one] 1 conflictus
   *[other] { $count } conflictus
}
tools-result-applied = Instrumenta applicata
tools-result-imported = Instrumenta importata
tools-result-installed = { $name } institutum
tools-result-updated = { $name } renovatum
tools-result-uninstalled = { $name } remotum
tools-result-forgotten = { $name } oblitum
tools-result-managed = { $name } nunc administratur
tools-result-linked = { $name } coniunctum
tools-result-unlinked = { $name } disiunctum

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

settings-empty = (vacuum)
settings-none = (nihil)

schema-system = Systema
schema-editor = Editor
schema-recording = Registratio
schema-radius = Radius
schema-padding = Spatium internum
schema-gap = Intervallum
schema-width = Latitudo
schema-color = Color
schema-red = Ruber
schema-green = Viridis
schema-blue = Caeruleus
schema-follow-files = Tabellas sequi
schema-tidy-files = Tabellas ordinare
schema-tidy-files-max = Limen ordinationis tabellarum
schema-tidy-files-auto = Tabellas automatice ordinare
schema-app-providers = Provisores applicationum
schema-provider = Provisor
schema-kind = Genus
schema-models = Exemplaria
schema-acp = Agentes ACP
schema-id = ID
schema-name = Nomen
schema-command = Mandatum
schema-arguments = Argumenta
schema-environment = Ambitus
schema-working-directory = Directorium operis
schema-shell = Testa
schema-font-family = Familia typorum
schema-startup-directory = Directorium initiale
schema-themes = Themata
schema-color-scheme = Schema colorum
schema-font-size = Magnitudo typorum
schema-line-height = Altitudo lineae
schema-cursor-style = Stilus cursoris
schema-cursor-blink = Nictatio cursoris
schema-custom-themes = Themata propria
schema-foreground = Primus planus
schema-background = Fundus
schema-cursor = Cursor
schema-ansi-colors = Colores ANSI
schema-keymap = Mappa clavium
schema-explorer = Explorator
schema-visible = Visibilis
schema-language-servers = Ministri linguarum
schema-servers = Ministri
schema-language-id = ID linguae
schema-root-markers = Notae radicis
schema-output-directory = Directorium exitus

menu-scene = Scaena
menu-layout = Dispositio
menu-terminal = Terminale
menu-browser = Navigator
menu-service = Ministerium
menu-bookmark = Signum
menu-edit = Editio

layout-knowledge = Scientia
layout-open-knowledge = Aperire Scientiam
layout-open-welcome-knowledge = Aperire “Salve in Scientia”
layout-open-path = Aperire { $path }
layout-fold-knowledge = Scientiam complica
layout-unfold-knowledge = Scientiam expande
layout-bookmarks = Signa
layout-new-folder = Novus fasciculus
layout-add-to-bookmarks = Ad signa adde
layout-move-to-bookmarks = Ad signa move
layout-stack-number = Strues { $number }
layout-fold-stack = Struem complica
layout-unfold-stack = Struem expande
layout-close-stack = Struem claude
layout-bookmark-in = Signum in { $folder }

common-cancel = Abrogare
common-delete = Delere
common-save = Servare
common-rename = Renominare
common-expand = Expandere
common-collapse = Contrahere
common-loading = Oneratur…
common-error = Error
common-output = Exitus
common-pending = Pendens
common-current = praesens
common-stop = Sistere
services-command = Servitium Vmux
services-uptime-seconds = { $seconds }s
services-uptime-minutes = { $minutes }m { $seconds }s
services-uptime-hours = { $hours }h { $minutes }m
services-uptime-days = { $days }d { $hours }h

error-page-failed-load = Pagina onerari non potuit
error-page-not-found = Pagina non inventa
error-unknown-host = Hospes app Vmux ignotus: { $host }

history-title = Historia

command-new-app-chat = Novum colloquium { $provider }/{ $model } (App)
command-interactive-mode-user = Scaena > Modus interactorius > Usor
command-interactive-mode-player = Scaena > Modus interactorius > Lusor
command-minimize-window = Dispositio > Fenestra > Minuere
command-toggle-layout = Dispositio > Dispositio > Mutare dispositionem
command-close-tab = Dispositio > Tabula > Claudere tabulam
command-new-task = Dispositio > Tabula > Novum opus…
command-next-tab = Dispositio > Tabula > Tabula sequens
command-prev-tab = Dispositio > Tabula > Tabula prior
command-rename-tab = Dispositio > Tabula > Renominare tabulam
command-tab-select-1 = Dispositio > Tabula > Eligere tabulam 1
command-tab-select-2 = Dispositio > Tabula > Eligere tabulam 2
command-tab-select-3 = Dispositio > Tabula > Eligere tabulam 3
command-tab-select-4 = Dispositio > Tabula > Eligere tabulam 4
command-tab-select-5 = Dispositio > Tabula > Eligere tabulam 5
command-tab-select-6 = Dispositio > Tabula > Eligere tabulam 6
command-tab-select-7 = Dispositio > Tabula > Eligere tabulam 7
command-tab-select-8 = Dispositio > Tabula > Eligere tabulam 8
command-tab-select-last = Dispositio > Tabula > Eligere ultimam tabulam
command-close-pane = Dispositio > Pannus > Claudere pannum
command-select-pane-left = Dispositio > Pannus > Eligere pannum sinistrum
command-select-pane-right = Dispositio > Pannus > Eligere pannum dextrum
command-select-pane-up = Dispositio > Pannus > Eligere pannum superiorem
command-select-pane-down = Dispositio > Pannus > Eligere pannum inferiorem
command-swap-pane-prev = Dispositio > Pannus > Permutare cum panno priore
command-swap-pane-next = Dispositio > Pannus > Permutare cum panno sequente
command-equalize-pane-size = Dispositio > Pannus > Aequari magnitudinem pannorum
command-resize-pane-left = Dispositio > Pannus > Mutare magnitudinem ad sinistram
command-resize-pane-right = Dispositio > Pannus > Mutare magnitudinem ad dextram
command-resize-pane-up = Dispositio > Pannus > Mutare magnitudinem sursum
command-resize-pane-down = Dispositio > Pannus > Mutare magnitudinem deorsum
command-stack-close = Dispositio > Strues > Claudere struem
command-stack-next = Dispositio > Strues > Strues sequens
command-stack-previous = Dispositio > Strues > Strues prior
command-stack-reopen = Dispositio > Strues > Reaperire paginam clausam
command-stack-swap-prev = Dispositio > Strues > Movere struem sinistrorsum
command-stack-swap-next = Dispositio > Strues > Movere struem dextrorsum
command-space-open = Dispositio > Spatium > Spatia
command-terminal-close = Terminale > Claudere terminale
command-terminal-next = Terminale > Terminale sequens
command-terminal-prev = Terminale > Terminale prius
command-terminal-clear = Terminale > Purgare terminale
command-browser-prev-page = Navigator > Navigatio > Retro
command-browser-next-page = Navigator > Navigatio > Porro
command-browser-reload = Navigator > Navigatio > Reonerare
command-browser-hard-reload = Navigator > Navigatio > Reonerare penitus
command-open-in-place = Navigator > Aperire > Aperire hic
command-open-in-new-stack = Navigator > Aperire > Aperire in nova strue
command-open-in-pane-top = Navigator > Aperire > Aperire in panno superiore
command-open-in-pane-right = Navigator > Aperire > Aperire in panno dextro
command-open-in-pane-bottom = Navigator > Aperire > Aperire in panno inferiore
command-open-in-pane-left = Navigator > Aperire > Aperire in panno sinistro
command-open-in-new-tab = Navigator > Aperire > Aperire in nova tabula
command-open-in-new-space = Navigator > Aperire > Aperire in novo spatio
command-browser-zoom-in = Navigator > Aspectus > Amplificare
command-browser-zoom-out = Navigator > Aspectus > Minuere
command-browser-zoom-reset = Navigator > Aspectus > Magnitudo vera
command-browser-dev-tools = Navigator > Aspectus > Instrumenta evolutoris
command-browser-open-command-bar = Navigator > Virgula > Virgula mandatorum
command-browser-open-page-in-command-bar = Navigator > Virgula > Recensere paginam
command-browser-open-path-bar = Navigator > Virgula > Navigator semitae
command-browser-open-commands = Navigator > Virgula > Mandata
command-browser-open-history = Navigator > Virgula > Historia
command-service-open = Servitium > Aperire monitorem servitii
command-bookmark-toggle-active = Signaculum > Signare paginam
command-bookmark-pin-active = Signaculum > Figere paginam

layout-tab = Tabula
layout-no-stacks = Nullae strues
layout-loading = Oneratur…
layout-no-markdown-files = Nulla documenta Markdown
layout-empty-folder = Fasciculus vacuus
layout-worktree = arbor laboris
layout-folder-name = Nomen fasciculi
layout-no-pins-bookmarks = Nullae paginae fixae aut signacula
layout-move-to = Movere ad { $folder }
layout-bookmark-current-page = Signare paginam praesentem
layout-rename-folder = Renominare fasciculum
layout-remove-folder = Removere fasciculum
layout-update-downloading = Renovatio deponitur
layout-update-installing = Renovatio installatur…
layout-update-ready = Nova versio praesto est
layout-restart-update = Restarte ut renoves

agent-preparing = Agens paratur…
agent-send-all-queued = Mitte nunc omnia prompta in ordine (Esc)
agent-send = Mitte (Enter)
agent-ready = Paratus sum cum tu paratus es.
agent-loading-older = Nuntii antiquiores onerantur…
agent-load-older = Onerare nuntios antiquiores
agent-continued-from = Continuatum ex { $source }
agent-older-context-omitted = contextus antiquior omissus
agent-interrupted = interruptum
agent-allow-tool = Permittere { $tool }?
agent-deny = Negare
agent-allow-always = Semper permittere
agent-allow = Permittere
agent-loading-sessions = Sessiones onerantur…
agent-no-resumable-sessions = Nullae sessiones resumptibiles inventae
agent-no-matching-sessions = Nullae sessiones congruentes
agent-no-matching-models = Nulla exemplaria congruentia
agent-choice-help = ↑/↓ vel Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Elige fasciculum repositorii
agent-choose-repository-detail = Elige repositorium Git locale quo agens uti debet.
agent-choosing = Eligitur…
agent-choose-folder = Elige fasciculum
agent-queued = in ordine
agent-attached = Adiuncta:
agent-cancel-queued = Abrogare promptum in ordine
agent-resume-queued = Resumere prompta in ordine
agent-clear-queue = Purgare ordinem
agent-send-all-now = mitte omnia nunc
agent-choose-option = Elige optionem supra
agent-loading-media = Media onerantur…
agent-no-matching-media = Nulla media congruentia
agent-prompt-context = Contextus prompti
agent-details = Singula
agent-path = Semita
agent-tool = Instrumentum
agent-server = Servitor
agent-bytes = { $count } octeti
agent-worked-for = Laboravit per { $duration }
agent-worked-for-steps = { $count ->
    [one] Laboravit per { $duration } · 1 gradus
   *[other] Laboravit per { $duration } · { $count } gradus
}
agent-tool-guardian-review = Recognitio custodis
agent-tool-read-files = Legit documenta
agent-tool-viewed-image = Inspexit imaginem
agent-tool-used-browser = Usus est navigatro
agent-tool-searched-files = Quaesivit documenta
agent-tool-ran-commands = Currit mandata
agent-thinking = Cogitat
agent-subagent = Subagens
agent-prompt = Promptum
agent-thread = Filum
agent-parent = Parens
agent-children = Liberi
agent-call = Vocatio
agent-raw-event = Eventus rudis
agent-plan = Consilium
agent-tasks = { $count ->
    [one] 1 opus
   *[other] { $count } opera
}
agent-edited = Recensitum
agent-reconnecting = Reconiungitur { $attempt }/{ $total }
agent-status-running = Currit
agent-status-done = Perfectum
agent-status-failed = Defecit
agent-status-pending = Pendens
agent-slash-attach-files = Adiungere documenta
agent-slash-resume-session = Resumere sessionem priorem
agent-slash-select-model = Eligere exemplar
agent-slash-continue-cli = Continuare hanc sessionem in CLI
agent-session-just-now = modo
agent-session-minutes-ago = abhinc { $count }m
agent-session-hours-ago = abhinc { $count }h
agent-session-days-ago = abhinc { $count }d
agent-working-working = Laborat
agent-working-thinking = Cogitat
agent-working-pondering = Meditatur
agent-working-noodling = Versat
agent-working-percolating = Percolat
agent-working-conjuring = Coniurat
agent-working-cooking = Coquit
agent-working-brewing = Infundit
agent-working-musing = Musitat
agent-working-ruminating = Ruminat
agent-working-scheming = Molitur
agent-working-synthesizing = Synthesin facit
agent-working-tinkering = Tentat
agent-working-churning = Agitat
agent-working-vibing = Vibat
agent-working-simmering = Lente fervet
agent-working-crafting = Fingit
agent-working-divining = Divinat
agent-working-mulling = Perpendit
agent-working-spelunking = Explorando descendit

editor-toggle-explorer = Mutare exploratorem (Cmd+B)
editor-unsaved = non servatum
editor-rendered-markdown = Markdown redditum cum recensione viva
editor-note = Nota
editor-source-editor = Editor fontis
editor-editor = Editor
editor-git-diff = Differentia Git
editor-diff = Differentia
editor-tidy = Mundare
editor-always = Semper
editor-unchanged-previews = { $count ->
    [one] ✦ 1 praevista immutata
   *[other] ✦ { $count } praevistae immutatae
}
editor-open-externally = Aperire externe
editor-changed-line = Linea mutata
editor-go-to-definition = Ire ad definitionem
editor-find-references = Invenire relationes
editor-references = { $count ->
    [one] 1 relatio
   *[other] { $count } relationes
}
editor-lsp-starting = { $server } incipit…
editor-lsp-not-installed = { $server } — non installatus
editor-explorer = Explorator
editor-open-editors = Editores aperti
editor-outline = Lineamenta
editor-new-file = Novum documentum
editor-new-folder = Novus fasciculus
editor-delete-confirm = Delere “{ $name }”? Hoc revocari non potest.
editor-created-folder = Fasciculus { $name } creatus
editor-created-file = Documentum { $name } creatum
editor-renamed-to = Renominatum in { $name }
editor-deleted = Deletum { $name }
editor-failed-decode-image = Imago decodificari non potuit
editor-preview-large-image = imago (nimis magna ad praevistam)
editor-preview-binary = binarium
editor-preview-file = documentum

git-status-clean = mundus
git-status-modified = mutatus
git-status-staged = paratus
git-status-staged-modified = paratus*
git-status-untracked = non vestigatus
git-status-deleted = deletus
git-status-conflict = conflictus
git-accept-all = ✓ accipere omnia
git-unstage = Ex praeparatione removere
git-confirm-deny-all = Confirma negare omnia
git-deny-all = ✗ negare omnia
git-commit-message = nuntius commit
git-commit = Committere ({ $count })
git-push = ↑ Propellere
git-loading-diff = Differentia oneratur…
git-no-changes = Nullae mutationes ostendendae
git-accept = ✓ accipere
git-deny = ✗ negare
git-show-unchanged-lines = Ostendere { $count } lineas immutatas

terminal-loading = Oneratur…
terminal-runs-when-ready = currit cum paratum est · Ctrl+C purgat · Esc praeterit
terminal-booting = initium capit
terminal-type-command = scribe mandatum · currit cum paratum est · Esc praeterit

setup-tagline-claude = Agens programmandi Anthropic, in Vmux
setup-tagline-codex = Agens programmandi OpenAI, in Vmux
setup-tagline-vibe = Agens programmandi Mistral, in Vmux
setup-install-title = Installare CLI { $name }
setup-homebrew-required = Homebrew requiritur ut { $command } installetur, nec adhuc configuratum est. Vmux primum Homebrew installabit, deinde { $name }.
setup-terminal-instructions = In terminali, preme Return ut incipias, deinde insere tesseram Mac cum rogaberis.
setup-command-missing = Vmux hanc paginam aperuit quia mandatum locale { $command } nondum installatum est. Curre mandatum infra ut id accipias.
setup-install-failed = Installatio non perfecta est. Inspice terminale pro singulis, deinde iterum tenta.
setup-installing = Installatur…
setup-install-homebrew = Installare Homebrew + { $name }
setup-run-install = Currere mandatum installationis
setup-auto-reload = Vmux id in terminali currit et reonerat cum { $command } paratum est.

debug-title = Debug
debug-auto-update = Renovatio automatica
debug-simulate-update = Simulare renovationem praesto
debug-simulate-download = Simulare depositionem
debug-clear-update = Purgare renovationem
debug-trigger-restart = Restarte incitare

command-manage-spaces = Spatia administrare…
command-pane-stack-location = area { $pane } / strues { $stack }
command-space-pane-stack-location = { $space } / area { $pane } / strues { $stack }
command-terminal-path = Terminale ({ $path })
command-group-interactive-mode = Modus interactivus
command-group-window = Fenestra
command-group-tab = Tabula
command-group-pane = Area
command-group-stack = Strues
command-group-space = Spatium
command-group-navigation = Navigatio
command-group-open = Aperire
command-group-view = Visus
command-group-bar = Virgula

menu-close-vmux = Vmux claudere

agents-terminal-coding-agent = Agens programmandi in terminali
