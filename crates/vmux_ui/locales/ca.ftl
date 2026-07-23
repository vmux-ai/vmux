locale-name = català
common-open = Obre
common-close = Tanca
common-install = Instal·la
common-uninstall = Desinstal·la
common-update = Actualitza
common-retry = Torna-ho a provar
common-refresh = Actualitza
common-remove = Elimina
common-enable = Activa
common-disable = Desactiva
common-new = Nou
common-active = actiu
common-running = en execució
common-done = fet
common-failed = Ha fallat
common-installed = Instal·lat
common-items = { $count ->
    [one] { $count } element
   *[other] { $count } elements
}

tools-title = Eines
tools-search = Cerca paquets, agents, MCP, eines de llenguatge i fitxers de configuració…
tools-open = Obre les eines
tools-fold = Plega les eines
tools-unfold = Desplega les eines
tools-scanning = S’estan analitzant les eines locals…
tools-no-installed = No hi ha eines instal·lades
tools-empty = No hi ha eines coincidents
tools-empty-detail = Instal·la un paquet o afegeix un paquet de fitxers de configuració a l’estil Stow.
tools-apply = Aplica
tools-homebrew = Homebrew
tools-homebrew-sync = Les fórmules i aplicacions instal·lades se sincronitzen automàticament.
tools-open-brewfile = Obre el Brewfile
tools-managed = gestionat
tools-provider-homebrew-formulae = Fórmules de Homebrew
tools-provider-homebrew-casks = Aplicacions de Homebrew
tools-provider-npm = Paquets npm
tools-provider-acp-agents = Agents ACP
tools-provider-language-tools = Eines de llenguatge
tools-provider-mcp-servers = Servidors MCP
tools-provider-dotfiles = Fitxers de configuració
tools-status-available = Disponible
tools-status-missing = Falta
tools-status-conflict = Conflicte
tools-forget = Oblida
tools-manage = Gestiona
tools-link = Enllaça
tools-unlink = Desenllaça
tools-import = Importa
tools-update-count = { $count ->
    [one] 1 actualització
   *[other] { $count } actualitzacions
}
tools-conflict-count = { $count ->
    [one] 1 conflicte
   *[other] { $count } conflictes
}
tools-result-applied = S’han aplicat les eines
tools-result-imported = S’han importat les eines
tools-result-installed = S’ha instal·lat { $name }
tools-result-updated = S’ha actualitzat { $name }
tools-result-uninstalled = S’ha desinstal·lat { $name }
tools-result-forgotten = S’ha oblidat { $name }
tools-result-managed = Ara es gestiona { $name }
tools-result-linked = S’ha enllaçat { $name }
tools-result-unlinked = S’ha desenllaçat { $name }

start-title = Inici
start-tagline = Una sola instrucció. Tot fet.

agents-title = Agents
agents-search = Cerca agents ACP i CLI…
agents-empty = No hi ha cap agent coincident
agents-empty-detail = Prova amb un nom, un entorn d’execució o ACP/CLI.
agents-install-failed = La instal·lació ha fallat
agents-updating = S’està actualitzant…
agents-retrying = S’està tornant a provar…
agents-preparing = S’està preparant…

extensions-title = Extensions
extensions-search = Cerca extensions instal·lades o a Chrome Web Store…
extensions-relaunch = Reinicia per aplicar-ho
extensions-empty = No hi ha cap extensió instal·lada
extensions-no-match = No hi ha cap extensió coincident
extensions-empty-detail = Cerca a Chrome Web Store a dalt i prem Retorn.
extensions-no-match-detail = Prova amb un altre nom o ID d’extensió.
extensions-on = Activat
extensions-off = Desactivat
extensions-enable-confirm = Vols activar { $name }?
extensions-enable-permissions = Activa { $name } i permet:

lsp-title = Servidors de llenguatge
lsp-search = Cerca servidors de llenguatge, linters, formatadors…
lsp-loading = S’està carregant el catàleg…
lsp-empty = No hi ha cap servidor de llenguatge coincident
lsp-empty-detail = Prova amb un altre llenguatge, linter o formatador.
lsp-needs = requereix { $tool }
lsp-status-available = Disponible
lsp-status-on-path = Al PATH
lsp-status-installing = S’està instal·lant…
lsp-status-installed = Instal·lat
lsp-status-outdated = Actualització disponible
lsp-status-running = En execució
lsp-status-failed = Ha fallat

spaces-title = Espais
spaces-new-placeholder = Nom del nou espai
spaces-empty = No hi ha cap espai
spaces-default-name = Espai { $number }
spaces-tabs = { $count ->
    [one] 1 pestanya
   *[other] { $count } pestanyes
}
spaces-delete = Suprimeix l’espai

team-title = Equip
team-just-you = Només tu en aquest espai
team-agents = { $count ->
    [one] Tu i 1 agent
   *[other] Tu i { $count } agents
}
team-empty = Encara no hi ha ningú
team-you = Tu
team-agent = Agent

services-title = Serveis en segon pla
services-processes = { $count ->
    [one] 1 procés
   *[other] { $count } processos
}
services-kill-all = Força la finalització de tots
services-not-running = El servei no s’està executant
services-start-with = Inicia amb:
services-empty = No hi ha cap procés actiu
services-filter = Filtra processos…
services-no-match = No hi ha cap procés coincident
services-connected = Connectat
services-disconnected = Desconnectat
services-attached = adjunt
services-kill = Força la finalització
services-memory = Memòria
services-size = Mida
services-shell = Intèrpret d’ordres

error-title = Error

history-search = Cerca a l’historial
history-clear-all = Esborra-ho tot
history-clear-confirm = Vols esborrar tot l’historial?
history-clear-warning = Aquesta acció no es pot desfer.
history-cancel = Cancel·la
history-today = Avui
history-yesterday = Ahir
history-days-ago = Fa { $count } dies
history-day-offset = Dia -{ $count }

settings-title = Configuració
settings-loading = S’està carregant la configuració…
settings-stored = Desat a ~/.vmux/settings.ron
settings-other = Altres
settings-software-update = Actualització de programari
settings-check-updates = Comprova si hi ha actualitzacions
settings-check-updates-hint = Es comprova automàticament en iniciar i cada hora quan l’actualització automàtica està activada.
settings-update-unavailable = No disponible
settings-update-unavailable-hint = Aquest muntatge no inclou l’actualitzador.
settings-update-checking = S’està comprovant…
settings-update-checking-hint = S’estan cercant actualitzacions…
settings-update-check-again = Torna a comprovar-ho
settings-update-current = Vmux està actualitzat.
settings-update-downloading = S’està baixant…
settings-update-downloading-hint = S’està baixant Vmux { $version }…
settings-update-installing = S’està instal·lant…
settings-update-installing-hint = S’està instal·lant Vmux { $version }…
settings-update-ready = Actualització a punt
settings-update-ready-hint = Vmux { $version } és a punt. Reinicia per aplicar-la.
settings-update-try-again = Torna-ho a provar
settings-update-failed = No s’han pogut comprovar les actualitzacions.
settings-item = Element
settings-item-number = Element { $number }
settings-press-key = Prem una tecla…
settings-saved = Desat
settings-record-key = Fes clic per enregistrar una combinació de tecles nova

tray-open-window = Obre la finestra
tray-close-window = Tanca la finestra
tray-pause-recording = Posa en pausa l’enregistrament
tray-resume-recording = Reprèn l’enregistrament
tray-finish-recording = Acaba l’enregistrament
tray-quit = Surt de Vmux

composer-attach-files = Adjunta fitxers (/upload)
composer-remove-attachment = Elimina l’adjunt

layout-back = Enrere
layout-forward = Endavant
layout-reload = Torna a carregar
layout-bookmark-page = Afegeix aquesta pàgina als marcadors
layout-remove-bookmark = Elimina el marcador
layout-pin-page = Fixa aquesta pàgina
layout-unpin-page = Deixa de fixar aquesta pàgina
layout-manage-extensions = Gestiona les extensions
layout-new-stack = Pila nova
layout-close-tab = Tanca la pestanya
layout-bookmark = Marcador
layout-pin = Fixa
layout-new-tab = Pestanya nova
layout-team = Equip

command-switch-space = Canvia d’espai…
command-search-ask = Cerca o pregunta…
command-new-tab-placeholder = Cerca o escriu un URL, o selecciona Terminal…
command-placeholder = Escriu un URL, cerca pestanyes o usa > per a ordres…
command-composer-placeholder = Escriu / per a ordres o @ per a contingut multimèdia
command-send = Envia (Retorn)
command-terminal = Terminal
command-open-terminal = Obre al Terminal
command-stack = Pila
command-tabs = { $count ->
    [one] 1 pestanya
   *[other] { $count } pestanyes
}
command-prompt = Instrucció
command-new-tab = Pestanya nova
command-search = Cerca
command-open-value = Obre «{ $value }»
command-search-value = Cerca «{ $value }»

schema-appearance = Aparença
schema-general = General
schema-layout = Disposició
schema-layout-detail = Finestra, subfinestres, barra lateral i anell de focus.
schema-agent = Agent
schema-agent-detail = Comportament de l’agent i permisos d’eines.
schema-shortcuts = Dreceres
schema-shortcuts-detail = Vista només de lectura. Edita settings.ron directament per canviar les assignacions.
schema-terminal = Terminal
schema-browser = Navegador
schema-mode = Mode
schema-mode-detail = Esquema de colors de les pàgines web. Dispositiu segueix el sistema.
schema-device = Dispositiu
schema-light = Clar
schema-dark = Fosc
schema-language = Llengua
schema-language-detail = Fes servir el sistema, en-US, ja o qualsevol etiqueta BCP 47 amb un catàleg ~/.vmux/locales/<tag>.ftl corresponent.
schema-auto-update = Actualització automàtica
schema-auto-update-detail = Cerca i instal·la actualitzacions en iniciar i cada hora.
schema-startup-url = URL d’inici
schema-startup-url-detail = Si és buit, obre la barra d’ordres.
schema-search-engine = Motor de cerca
schema-search-engine-detail = S’utilitza per a cerques web des de l’Inici i la barra d’ordres.
schema-window = Finestra
schema-pane = Subfinestra
schema-side-sheet = Full lateral
schema-focus-ring = Anell de focus
schema-run-placement = Permet substituir la ubicació d’execució
schema-run-placement-detail = Permet que els agents triïn el mode, la direcció i l’ancoratge de la subfinestra d’execució.
schema-leader = Tecla líder
schema-leader-detail = Tecla prefix per a dreceres amb acord.
schema-chord-timeout = Temps d’espera de l’acord
schema-chord-timeout-detail = Mil·lisegons abans que caduqui un prefix d’acord.
schema-bindings = Assignacions
schema-confirm-close = Confirma el tancament
schema-confirm-close-detail = Demana confirmació abans de tancar un terminal amb un procés en execució.
schema-default-theme = Tema per defecte
schema-default-theme-detail = Nom del tema actiu de la llista de temes.

settings-empty = (buit)
settings-none = (cap)

schema-system = Sistema
schema-editor = Editor
schema-recording = Enregistrament
schema-radius = Radi
schema-padding = Farciment
schema-gap = Espai
schema-width = Amplada
schema-color = Color
schema-red = Vermell
schema-green = Verd
schema-blue = Blau
schema-follow-files = Segueix fitxers
schema-tidy-files = Endreça fitxers
schema-tidy-files-max = Llindar d'endreça de fitxers
schema-tidy-files-auto = Endreça fitxers automàticament
schema-app-providers = Proveïdors d'aplicacions
schema-provider = Proveïdor
schema-kind = Tipus
schema-models = Models
schema-acp = Agents ACP
schema-id = ID
schema-name = Nom
schema-command = Ordre
schema-arguments = Arguments
schema-environment = Entorn
schema-working-directory = Directori de treball
schema-shell = Intèrpret d'ordres
schema-font-family = Família de lletra
schema-startup-directory = Directori d'inici
schema-themes = Temes
schema-color-scheme = Esquema de colors
schema-font-size = Mida de la lletra
schema-line-height = Alçada de línia
schema-cursor-style = Estil del cursor
schema-cursor-blink = Parpelleig del cursor
schema-custom-themes = Temes personalitzats
schema-foreground = Primer pla
schema-background = Fons
schema-cursor = Cursor
schema-ansi-colors = Colors ANSI
schema-keymap = Mapa de tecles
schema-explorer = Explorador
schema-visible = Visible
schema-language-servers = Servidors de llenguatge
schema-servers = Servidors
schema-language-id = ID de llenguatge
schema-root-markers = Marcadors d'arrel
schema-output-directory = Directori de sortida

menu-scene = Escena
menu-layout = Disposició
menu-terminal = Terminal
menu-browser = Navegador
menu-service = Servei
menu-bookmark = Marcador
menu-edit = Edició

layout-knowledge = Coneixement
layout-open-knowledge = Obre Coneixement
layout-open-welcome-knowledge = Obre Benvinguda al coneixement
layout-open-path = Obre { $path }
layout-fold-knowledge = Plega el coneixement
layout-unfold-knowledge = Desplega el coneixement
layout-bookmarks = Marcadors
layout-new-folder = Carpeta nova
layout-add-to-bookmarks = Afegeix als marcadors
layout-move-to-bookmarks = Mou als marcadors
layout-stack-number = Pila { $number }
layout-fold-stack = Plega la pila
layout-unfold-stack = Desplega la pila
layout-close-stack = Tanca la pila
layout-bookmark-in = Marcador a { $folder }

common-cancel = Cancel·la
common-delete = Suprimeix
common-save = Desa
common-rename = Canvia el nom
common-expand = Desplega
common-collapse = Replega
common-loading = S’està carregant…
common-error = Error
common-output = Sortida
common-pending = Pendent
common-current = actual
common-stop = Atura
services-command = Servei de Vmux
services-uptime-seconds = { $seconds } s
services-uptime-minutes = { $minutes } min { $seconds } s
services-uptime-hours = { $hours } h { $minutes } min
services-uptime-days = { $days } d { $hours } h

error-page-failed-load = No s’ha pogut carregar la pàgina
error-page-not-found = No s’ha trobat la pàgina
error-unknown-host = Amfitrió d’app de Vmux desconegut: { $host }

history-title = Historial

command-new-app-chat = Xat nou amb { $provider }/{ $model } (app)
command-interactive-mode-user = Escena > Mode interactiu > Usuari
command-interactive-mode-player = Escena > Mode interactiu > Reproductor
command-minimize-window = Disposició > Finestra > Minimitza
command-toggle-layout = Disposició > Disposició > Commuta la disposició
command-close-tab = Disposició > Pestanya > Tanca la pestanya
command-new-task = Disposició > Pestanya > Tasca nova…
command-next-tab = Disposició > Pestanya > Pestanya següent
command-prev-tab = Disposició > Pestanya > Pestanya anterior
command-rename-tab = Disposició > Pestanya > Canvia el nom de la pestanya
command-tab-select-1 = Disposició > Pestanya > Selecciona la pestanya 1
command-tab-select-2 = Disposició > Pestanya > Selecciona la pestanya 2
command-tab-select-3 = Disposició > Pestanya > Selecciona la pestanya 3
command-tab-select-4 = Disposició > Pestanya > Selecciona la pestanya 4
command-tab-select-5 = Disposició > Pestanya > Selecciona la pestanya 5
command-tab-select-6 = Disposició > Pestanya > Selecciona la pestanya 6
command-tab-select-7 = Disposició > Pestanya > Selecciona la pestanya 7
command-tab-select-8 = Disposició > Pestanya > Selecciona la pestanya 8
command-tab-select-last = Disposició > Pestanya > Selecciona l’última pestanya
command-close-pane = Disposició > Plafó > Tanca el plafó
command-select-pane-left = Disposició > Plafó > Selecciona el plafó de l’esquerra
command-select-pane-right = Disposició > Plafó > Selecciona el plafó de la dreta
command-select-pane-up = Disposició > Plafó > Selecciona el plafó superior
command-select-pane-down = Disposició > Plafó > Selecciona el plafó inferior
command-swap-pane-prev = Disposició > Plafó > Intercanvia amb el plafó anterior
command-swap-pane-next = Disposició > Plafó > Intercanvia amb el plafó següent
command-equalize-pane-size = Disposició > Plafó > Igualar la mida dels plafons
command-resize-pane-left = Disposició > Plafó > Redimensiona el plafó cap a l’esquerra
command-resize-pane-right = Disposició > Plafó > Redimensiona el plafó cap a la dreta
command-resize-pane-up = Disposició > Plafó > Redimensiona el plafó cap amunt
command-resize-pane-down = Disposició > Plafó > Redimensiona el plafó cap avall
command-stack-close = Disposició > Pila > Tanca la pila
command-stack-next = Disposició > Pila > Pila següent
command-stack-previous = Disposició > Pila > Pila anterior
command-stack-reopen = Disposició > Pila > Torna a obrir la pàgina tancada
command-stack-swap-prev = Disposició > Pila > Mou la pila a l’esquerra
command-stack-swap-next = Disposició > Pila > Mou la pila a la dreta
command-space-open = Disposició > Espai > Espais
command-terminal-close = Terminal > Tanca el terminal
command-terminal-next = Terminal > Terminal següent
command-terminal-prev = Terminal > Terminal anterior
command-terminal-clear = Terminal > Neteja el terminal
command-browser-prev-page = Navegador > Navegació > Enrere
command-browser-next-page = Navegador > Navegació > Endavant
command-browser-reload = Navegador > Navegació > Recarrega
command-browser-hard-reload = Navegador > Navegació > Força la recàrrega
command-open-in-place = Navegador > Obre > Obre aquí
command-open-in-new-stack = Navegador > Obre > Obre en una pila nova
command-open-in-pane-top = Navegador > Obre > Obre al plafó superior
command-open-in-pane-right = Navegador > Obre > Obre al plafó de la dreta
command-open-in-pane-bottom = Navegador > Obre > Obre al plafó inferior
command-open-in-pane-left = Navegador > Obre > Obre al plafó de l’esquerra
command-open-in-new-tab = Navegador > Obre > Obre en una pestanya nova
command-open-in-new-space = Navegador > Obre > Obre en un espai nou
command-browser-zoom-in = Navegador > Visualització > Apropa
command-browser-zoom-out = Navegador > Visualització > Allunya
command-browser-zoom-reset = Navegador > Visualització > Mida real
command-browser-dev-tools = Navegador > Visualització > Eines per a desenvolupadors
command-browser-open-command-bar = Navegador > Barra > Barra d’ordres
command-browser-open-page-in-command-bar = Navegador > Barra > Edita la pàgina
command-browser-open-path-bar = Navegador > Barra > Navegador de camins
command-browser-open-commands = Navegador > Barra > Ordres
command-browser-open-history = Navegador > Barra > Historial
command-service-open = Servei > Obre el monitor de serveis
command-bookmark-toggle-active = Marcador > Desa la pàgina als marcadors
command-bookmark-pin-active = Marcador > Fixa la pàgina

layout-tab = Pestanya
layout-no-stacks = No hi ha piles
layout-loading = S’està carregant…
layout-no-markdown-files = No hi ha fitxers Markdown
layout-empty-folder = Carpeta buida
layout-worktree = arbre de treball
layout-folder-name = Nom de la carpeta
layout-no-pins-bookmarks = No hi ha fixats ni marcadors
layout-move-to = Mou a { $folder }
layout-bookmark-current-page = Desa la pàgina actual als marcadors
layout-rename-folder = Canvia el nom de la carpeta
layout-remove-folder = Elimina la carpeta
layout-update-downloading = S’està baixant l’actualització
layout-update-installing = S’està instal·lant l’actualització…
layout-update-ready = Hi ha una versió nova disponible
layout-restart-update = Reinicia per actualitzar

agent-preparing = S’està preparant l’agent…
agent-send-all-queued = Envia ara tots els prompts en cua (Esc)
agent-send = Envia (Retorn)
agent-ready = A punt quan vulguis.
agent-loading-older = S’estan carregant missatges anteriors…
agent-load-older = Carrega missatges anteriors
agent-continued-from = Continuat des de { $source }
agent-older-context-omitted = s’ha omès el context anterior
agent-interrupted = interromput
agent-allow-tool = Vols permetre { $tool }?
agent-deny = Denega
agent-allow-always = Permet sempre
agent-allow = Permet
agent-loading-sessions = S’estan carregant les sessions…
agent-no-resumable-sessions = No s’ha trobat cap sessió que es pugui reprendre
agent-no-matching-sessions = No hi ha cap sessió coincident
agent-no-matching-models = No hi ha cap model coincident
agent-choice-help = ↑/↓ o Ctrl+N/Ctrl+P · 1–9 · Retorn
agent-choose-repository = Tria la carpeta del repositori
agent-choose-repository-detail = Selecciona el repositori Git local que ha d’utilitzar l’agent.
agent-choosing = S’està triant…
agent-choose-folder = Tria una carpeta
agent-queued = en cua
agent-attached = Adjunt:
agent-cancel-queued = Cancel·la el prompt en cua
agent-resume-queued = Reprèn els prompts en cua
agent-clear-queue = Buida la cua
agent-send-all-now = envia-ho tot ara
agent-choose-option = Tria una opció de dalt
agent-loading-media = S’estan carregant els mitjans…
agent-no-matching-media = No hi ha cap mitjà coincident
agent-prompt-context = Context del prompt
agent-details = Detalls
agent-path = Camí
agent-tool = Eina
agent-server = Servidor
agent-bytes = { $count } bytes
agent-worked-for = Ha treballat durant { $duration }
agent-worked-for-steps = { $count ->
    [one] Ha treballat durant { $duration } · 1 pas
   *[other] Ha treballat durant { $duration } · { $count } passos
}
agent-tool-guardian-review = Revisió de Guardian
agent-tool-read-files = Ha llegit fitxers
agent-tool-viewed-image = Ha vist una imatge
agent-tool-used-browser = Ha utilitzat el navegador
agent-tool-searched-files = Ha cercat fitxers
agent-tool-ran-commands = Ha executat ordres
agent-thinking = Pensant
agent-subagent = Subagent
agent-prompt = Prompt
agent-thread = Fil
agent-parent = Pare
agent-children = Fills
agent-call = Crida
agent-raw-event = Esdeveniment en brut
agent-plan = Pla
agent-tasks = { $count ->
    [one] 1 tasca
   *[other] { $count } tasques
}
agent-edited = Editat
agent-reconnecting = S’està reconnectant { $attempt }/{ $total }
agent-status-running = En execució
agent-status-done = Fet
agent-status-failed = Ha fallat
agent-status-pending = Pendent
agent-slash-attach-files = Adjunta fitxers
agent-slash-resume-session = Reprèn una sessió anterior
agent-slash-select-model = Selecciona un model
agent-slash-continue-cli = Continua aquesta sessió a la CLI
agent-session-just-now = ara mateix
agent-session-minutes-ago = fa { $count } min
agent-session-hours-ago = fa { $count } h
agent-session-days-ago = fa { $count } d
agent-working-working = Treballant
agent-working-thinking = Pensant
agent-working-pondering = Reflexionant
agent-working-noodling = Donant-hi voltes
agent-working-percolating = Madurant idees
agent-working-conjuring = Invocant idees
agent-working-cooking = Cuinant
agent-working-brewing = Fermentant
agent-working-musing = Rumiant
agent-working-ruminating = Rumiant a fons
agent-working-scheming = Maquinant
agent-working-synthesizing = Sintetitzant
agent-working-tinkering = Fent proves
agent-working-churning = Processant
agent-working-vibing = Fluint
agent-working-simmering = Fent xup-xup
agent-working-crafting = Elaborant
agent-working-divining = Esbrinant
agent-working-mulling = Rumiant
agent-working-spelunking = Explorant a fons

editor-toggle-explorer = Mostra/amaga l’Explorador (Cmd+B)
editor-unsaved = no desat
editor-rendered-markdown = Markdown renderitzat amb edició en directe
editor-note = Nota
editor-source-editor = Editor de codi font
editor-editor = Editor
editor-git-diff = Diferència de Git
editor-diff = Diferència
editor-tidy = Endreça
editor-always = Sempre
editor-unchanged-previews = { $count ->
    [one] ✦ 1 previsualització sense canvis
   *[other] ✦ { $count } previsualitzacions sense canvis
}
editor-open-externally = Obre externament
editor-changed-line = Línia canviada
editor-go-to-definition = Vés a la definició
editor-find-references = Cerca referències
editor-references = { $count ->
    [one] 1 referència
   *[other] { $count } referències
}
editor-lsp-starting = { $server } s’està iniciant…
editor-lsp-not-installed = { $server } — no està instal·lat
editor-explorer = Explorador
editor-open-editors = Editors oberts
editor-outline = Esquema
editor-new-file = Fitxer nou
editor-new-folder = Carpeta nova
editor-delete-confirm = Vols suprimir “{ $name }”? Aquesta acció no es pot desfer.
editor-created-folder = S’ha creat la carpeta { $name }
editor-created-file = S’ha creat el fitxer { $name }
editor-renamed-to = S’ha canviat el nom a { $name }
editor-deleted = S’ha suprimit { $name }
editor-failed-decode-image = No s’ha pogut descodificar la imatge
editor-preview-large-image = imatge (massa gran per previsualitzar-la)
editor-preview-binary = binari
editor-preview-file = fitxer

git-status-clean = net
git-status-modified = modificat
git-status-staged = preparat
git-status-staged-modified = preparat*
git-status-untracked = sense seguiment
git-status-deleted = suprimit
git-status-conflict = conflicte
git-accept-all = ✓ accepta-ho tot
git-unstage = Treu de l’àrea de preparació
git-confirm-deny-all = Confirma que vols denegar-ho tot
git-deny-all = ✗ denega-ho tot
git-commit-message = missatge del commit
git-commit = Fes commit ({ $count })
git-push = ↑ Puja
git-loading-diff = S’està carregant la diferència…
git-no-changes = No hi ha canvis per mostrar
git-accept = ✓ accepta
git-deny = ✗ denega
git-show-unchanged-lines = Mostra { $count } línies sense canvis

terminal-loading = S’està carregant…
terminal-runs-when-ready = s’executa quan estigui a punt · Ctrl+C neteja · Esc salta
terminal-booting = s’està arrencant
terminal-type-command = escriu una ordre · s’executa quan estigui a punt · Esc salta

setup-tagline-claude = L’agent de programació d’Anthropic, a Vmux
setup-tagline-codex = L’agent de programació d’OpenAI, a Vmux
setup-tagline-vibe = L’agent de programació de Mistral, a Vmux
setup-install-title = Instal·la la CLI de { $name }
setup-homebrew-required = Cal Homebrew per instal·lar { $command } i encara no està configurat. Vmux instal·larà primer Homebrew i després { $name }.
setup-terminal-instructions = Al terminal, prem Retorn per començar i introdueix la contrasenya del Mac quan se’t demani.
setup-command-missing = Vmux ha obert aquesta pàgina perquè l’ordre local { $command } encara no està instal·lada. Executa l’ordre següent per obtenir-la.
setup-install-failed = La instal·lació no ha acabat. Consulta el terminal per veure’n els detalls i torna-ho a provar.
setup-installing = S’està instal·lant…
setup-install-homebrew = Instal·la Homebrew + { $name }
setup-run-install = Executa l’ordre d’instal·lació
setup-auto-reload = Vmux l’executa en un terminal i es recarrega quan { $command } està a punt.

debug-title = Depuració
debug-auto-update = Actualització automàtica
debug-simulate-update = Simula que hi ha una actualització disponible
debug-simulate-download = Simula la baixada
debug-clear-update = Esborra l’actualització
debug-trigger-restart = Activa el reinici

command-manage-spaces = Gestiona els espais…
command-pane-stack-location = subfinestra { $pane } / pila { $stack }
command-space-pane-stack-location = { $space } / subfinestra { $pane } / pila { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Mode interactiu
command-group-window = Finestra
command-group-tab = Pestanya
command-group-pane = Subfinestra
command-group-stack = Pila
command-group-space = Espai
command-group-navigation = Navegació
command-group-open = Obre
command-group-view = Visualització
command-group-bar = Barra

menu-close-vmux = Tanca Vmux

agents-terminal-coding-agent = Agent de programació basat en el terminal
