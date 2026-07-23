locale-name = galego
common-open = Abrir
common-close = Pechar
common-install = Instalar
common-uninstall = Desinstalar
common-update = Actualizar
common-retry = Tentar de novo
common-refresh = Actualizar
common-remove = Retirar
common-enable = Activar
common-disable = Desactivar
common-new = Novo
common-active = activo
common-running = en execución
common-done = feito
common-failed = Fallou
common-installed = Instalado
common-items = { $count ->
    [one] { $count } elemento
   *[other] { $count } elementos
}

tools-title = Ferramentas
tools-search = Buscar paquetes, axentes, MCP, ferramentas de linguaxe e ficheiros de configuración…
tools-open = Abrir Ferramentas
tools-fold = Pregar as ferramentas
tools-unfold = Despregar as ferramentas
tools-scanning = Analizando as ferramentas locais…
tools-no-installed = Non hai ferramentas instaladas
tools-empty = Non hai ferramentas coincidentes
tools-empty-detail = Instala un paquete ou engade un paquete de ficheiros de configuración ao estilo Stow.
tools-apply = Aplicar
tools-homebrew = Homebrew
tools-homebrew-sync = As fórmulas e aplicacións instaladas sincronízanse automaticamente.
tools-open-brewfile = Abrir Brewfile
tools-managed = xestionado
tools-provider-homebrew-formulae = Fórmulas de Homebrew
tools-provider-homebrew-casks = Aplicacións de Homebrew
tools-provider-npm = Paquetes npm
tools-provider-acp-agents = Axentes ACP
tools-provider-language-tools = Ferramentas de linguaxe
tools-provider-mcp-servers = Servidores MCP
tools-provider-dotfiles = Ficheiros de configuración
tools-status-available = Dispoñible
tools-status-missing = Falta
tools-status-conflict = Conflito
tools-forget = Esquecer
tools-manage = Xestionar
tools-link = Vincular
tools-unlink = Desvincular
tools-import = Importar
tools-update-count = { $count ->
    [one] 1 actualización
   *[other] { $count } actualizacións
}
tools-conflict-count = { $count ->
    [one] 1 conflito
   *[other] { $count } conflitos
}
tools-result-applied = Ferramentas aplicadas
tools-result-imported = Ferramentas importadas
tools-result-installed = { $name } instalado
tools-result-updated = { $name } actualizado
tools-result-uninstalled = { $name } desinstalado
tools-result-forgotten = { $name } esquecido
tools-result-managed = { $name } agora está xestionado
tools-result-linked = { $name } vinculado
tools-result-unlinked = { $name } desvinculado

start-title = Inicio
start-tagline = Unha instrución. Todo feito.

agents-title = Axentes
agents-search = Buscar axentes ACP e CLI…
agents-empty = Non hai axentes coincidentes
agents-empty-detail = Proba cun nome, contorno de execución ou ACP/CLI.
agents-install-failed = Fallou a instalación
agents-updating = Actualizando…
agents-retrying = Tentando de novo…
agents-preparing = Preparando…

extensions-title = Extensións
extensions-search = Buscar instaladas ou na Chrome Web Store…
extensions-relaunch = Reinicia para aplicar
extensions-empty = Non hai extensións instaladas
extensions-no-match = Non hai extensións coincidentes
extensions-empty-detail = Busca na Chrome Web Store arriba e preme Intro.
extensions-no-match-detail = Proba con outro nome ou ID de extensión.
extensions-on = Activada
extensions-off = Desactivada
extensions-enable-confirm = Activar { $name }?
extensions-enable-permissions = Activar { $name } e permitir:

lsp-title = Servidores de linguaxe
lsp-search = Buscar servidores de linguaxe, linters, formatadores…
lsp-loading = Cargando o catálogo…
lsp-empty = Non hai servidores de linguaxe coincidentes
lsp-empty-detail = Proba con outra linguaxe, linter ou formatador.
lsp-needs = precisa { $tool }
lsp-status-available = Dispoñible
lsp-status-on-path = No PATH
lsp-status-installing = Instalando…
lsp-status-installed = Instalado
lsp-status-outdated = Actualización dispoñible
lsp-status-running = En execución
lsp-status-failed = Fallou

spaces-title = Espazos
spaces-new-placeholder = Nome do novo espazo
spaces-empty = Non hai espazos
spaces-default-name = Espazo { $number }
spaces-tabs = { $count ->
    [one] 1 lapela
   *[other] { $count } lapelas
}
spaces-delete = Eliminar espazo

team-title = Equipo
team-just-you = Só estás ti neste espazo
team-agents = { $count ->
    [one] Ti e 1 axente
   *[other] Ti e { $count } axentes
}
team-empty = Aínda non hai ninguén aquí
team-you = Ti
team-agent = Axente

services-title = Servizos en segundo plano
services-processes = { $count ->
    [one] 1 proceso
   *[other] { $count } procesos
}
services-kill-all = Forzar remate de todos
services-not-running = O servizo non está en execución
services-start-with = Iniciar con:
services-empty = Non hai procesos activos
services-filter = Filtrar procesos…
services-no-match = Non hai procesos coincidentes
services-connected = Conectado
services-disconnected = Desconectado
services-attached = anexado
services-kill = Forzar remate
services-memory = Memoria
services-size = Tamaño
services-shell = Intérprete

error-title = Erro

history-search = Buscar no historial
history-clear-all = Borrar todo
history-clear-confirm = Borrar todo o historial?
history-clear-warning = Isto non se pode desfacer.
history-cancel = Cancelar
history-today = Hoxe
history-yesterday = Onte
history-days-ago = Hai { $count } días
history-day-offset = Día -{ $count }

settings-title = Axustes
settings-loading = Cargando os axustes…
settings-stored = Gardado en ~/.vmux/settings.ron
settings-other = Outros
settings-software-update = Actualización de software
settings-check-updates = Buscar actualizacións
settings-check-updates-hint = Compróbase automaticamente ao iniciar e cada hora cando a actualización automática está activada.
settings-update-unavailable = Non dispoñible
settings-update-unavailable-hint = O actualizador non está incluído nesta compilación.
settings-update-checking = Comprobando…
settings-update-checking-hint = Buscando actualizacións…
settings-update-check-again = Comprobar de novo
settings-update-current = Vmux está actualizado.
settings-update-downloading = Descargando…
settings-update-downloading-hint = Descargando Vmux { $version }…
settings-update-installing = Instalando…
settings-update-installing-hint = Instalando Vmux { $version }…
settings-update-ready = Actualización lista
settings-update-ready-hint = Vmux { $version } está listo. Reinicia para aplicalo.
settings-update-try-again = Tentar de novo
settings-update-failed = Non se puideron buscar actualizacións.
settings-item = Elemento
settings-item-number = Elemento { $number }
settings-press-key = Preme unha tecla…
settings-saved = Gardado
settings-record-key = Fai clic para gravar unha nova combinación de teclas

tray-open-window = Abrir xanela
tray-close-window = Pechar xanela
tray-pause-recording = Pausar gravación
tray-resume-recording = Retomar gravación
tray-finish-recording = Rematar gravación
tray-quit = Saír de Vmux

composer-attach-files = Anexar ficheiros (/upload)
composer-remove-attachment = Retirar anexo

layout-back = Atrás
layout-forward = Adiante
layout-reload = Recargar
layout-bookmark-page = Marcar esta páxina
layout-remove-bookmark = Retirar marcador
layout-pin-page = Fixar esta páxina
layout-unpin-page = Desfixar esta páxina
layout-manage-extensions = Xestionar extensións
layout-new-stack = Nova pila
layout-close-tab = Pechar lapela
layout-bookmark = Marcador
layout-pin = Fixar
layout-new-tab = Nova lapela
layout-team = Equipo

command-switch-space = Cambiar de espazo…
command-search-ask = Buscar ou preguntar…
command-new-tab-placeholder = Busca, escribe un URL ou selecciona Terminal…
command-placeholder = Escribe un URL, busca lapelas ou usa > para comandos…
command-composer-placeholder = Escribe / para comandos ou @ para medios
command-send = Enviar (Intro)
command-terminal = Terminal
command-open-terminal = Abrir no Terminal
command-stack = Pila
command-tabs = { $count ->
    [one] 1 lapela
   *[other] { $count } lapelas
}
command-prompt = Instrución
command-new-tab = Nova lapela
command-search = Buscar
command-open-value = Abrir “{ $value }”
command-search-value = Buscar “{ $value }”

schema-appearance = Aparencia
schema-general = Xeral
schema-layout = Disposición
schema-layout-detail = Xanela, paneis, barra lateral e anel de foco.
schema-agent = Axente
schema-agent-detail = Comportamento do axente e permisos das ferramentas.
schema-shortcuts = Atallos
schema-shortcuts-detail = Vista de só lectura. Edita settings.ron directamente para cambiar as asociacións.
schema-terminal = Terminal
schema-browser = Navegador
schema-mode = Modo
schema-mode-detail = Esquema de cores para páxinas web. Dispositivo segue o sistema.
schema-device = Dispositivo
schema-light = Claro
schema-dark = Escuro
schema-language = Lingua
schema-language-detail = Usa o sistema, en-US, ja ou calquera etiqueta BCP 47 cun catálogo ~/.vmux/locales/<tag>.ftl correspondente.
schema-auto-update = Actualización automática
schema-auto-update-detail = Buscar e instalar actualizacións ao iniciar e cada hora.
schema-startup-url = URL de inicio
schema-startup-url-detail = Se está baleiro, ábrese a indicación da barra de comandos.
schema-search-engine = Motor de busca
schema-search-engine-detail = Úsase para buscas web desde Inicio e a barra de comandos.
schema-window = Xanela
schema-pane = Panel
schema-side-sheet = Panel lateral
schema-focus-ring = Anel de foco
schema-run-placement = Permitir substituír a colocación da execución
schema-run-placement-detail = Permitir que os axentes escollan o modo, a dirección e a áncora do panel de execución.
schema-leader = Tecla líder
schema-leader-detail = Tecla de prefixo para atallos por acordes.
schema-chord-timeout = Tempo límite do acorde
schema-chord-timeout-detail = Milisegundos antes de que caduque un prefixo de acorde.
schema-bindings = Asociacións
schema-confirm-close = Confirmar peche
schema-confirm-close-detail = Preguntar antes de pechar un terminal cun proceso en execución.
schema-default-theme = Tema predeterminado
schema-default-theme-detail = Nome do tema activo na lista de temas.

settings-empty = (baleiro)
settings-none = (ningún)

schema-system = Sistema
schema-editor = Editor
schema-recording = Gravación
schema-radius = Raio
schema-padding = Recheo
schema-gap = Separación
schema-width = Largura
schema-color = Cor
schema-red = Vermello
schema-green = Verde
schema-blue = Azul
schema-follow-files = Seguir ficheiros
schema-tidy-files = Limpar ficheiros
schema-tidy-files-max = Limiar de limpeza de ficheiros
schema-tidy-files-auto = Limpar ficheiros automaticamente
schema-app-providers = Provedores de aplicacións
schema-provider = Provedor
schema-kind = Tipo
schema-models = Modelos
schema-acp = Axentes ACP
schema-id = ID
schema-name = Nome
schema-command = Orde
schema-arguments = Argumentos
schema-environment = Contorno
schema-working-directory = Directorio de traballo
schema-shell = Intérprete de ordes
schema-font-family = Familia tipográfica
schema-startup-directory = Directorio inicial
schema-themes = Temas
schema-color-scheme = Esquema de cores
schema-font-size = Tamaño da letra
schema-line-height = Altura de liña
schema-cursor-style = Estilo do cursor
schema-cursor-blink = Intermitencia do cursor
schema-custom-themes = Temas personalizados
schema-foreground = Primeiro plano
schema-background = Fondo
schema-cursor = Cursor
schema-ansi-colors = Cores ANSI
schema-keymap = Mapa de teclas
schema-explorer = Explorador
schema-visible = Visible
schema-language-servers = Servidores de linguaxe
schema-servers = Servidores
schema-language-id = ID de linguaxe
schema-root-markers = Marcadores de raíz
schema-output-directory = Directorio de saída

menu-scene = Escena
menu-layout = Disposición
menu-terminal = Terminal
menu-browser = Navegador
menu-service = Servizo
menu-bookmark = Marcador
menu-edit = Editar

layout-knowledge = Coñecemento
layout-open-knowledge = Abrir Coñecemento
layout-open-welcome-knowledge = Abrir Benvida ao Coñecemento
layout-open-path = Abrir { $path }
layout-fold-knowledge = Pregar coñecemento
layout-unfold-knowledge = Despregar coñecemento
layout-bookmarks = Marcadores
layout-new-folder = Novo cartafol
layout-add-to-bookmarks = Engadir aos marcadores
layout-move-to-bookmarks = Mover aos marcadores
layout-stack-number = Pila { $number }
layout-fold-stack = Pregar pila
layout-unfold-stack = Despregar pila
layout-close-stack = Pechar pila
layout-bookmark-in = Marcar en { $folder }

common-cancel = Cancelar
common-delete = Eliminar
common-save = Gardar
common-rename = Renomear
common-expand = Expandir
common-collapse = Contraer
common-loading = Cargando…
common-error = Erro
common-output = Saída
common-pending = Pendente
common-current = actual
common-stop = Deter
services-command = Servizo de Vmux
services-uptime-seconds = { $seconds } s
services-uptime-minutes = { $minutes } min { $seconds } s
services-uptime-hours = { $hours } h { $minutes } min
services-uptime-days = { $days } d { $hours } h

error-page-failed-load = Non se puido cargar a páxina
error-page-not-found = Páxina non atopada
error-unknown-host = Host de app de Vmux descoñecido: { $host }

history-title = Historial

command-new-app-chat = Novo chat de { $provider }/{ $model } (app)
command-interactive-mode-user = Escena > Modo interactivo > Usuario
command-interactive-mode-player = Escena > Modo interactivo > Reprodutor
command-minimize-window = Disposición > Xanela > Minimizar
command-toggle-layout = Disposición > Disposición > Alternar disposición
command-close-tab = Disposición > Lapela > Pechar lapela
command-new-task = Disposición > Lapela > Nova tarefa…
command-next-tab = Disposición > Lapela > Lapela seguinte
command-prev-tab = Disposición > Lapela > Lapela anterior
command-rename-tab = Disposición > Lapela > Renomear lapela
command-tab-select-1 = Disposición > Lapela > Seleccionar lapela 1
command-tab-select-2 = Disposición > Lapela > Seleccionar lapela 2
command-tab-select-3 = Disposición > Lapela > Seleccionar lapela 3
command-tab-select-4 = Disposición > Lapela > Seleccionar lapela 4
command-tab-select-5 = Disposición > Lapela > Seleccionar lapela 5
command-tab-select-6 = Disposición > Lapela > Seleccionar lapela 6
command-tab-select-7 = Disposición > Lapela > Seleccionar lapela 7
command-tab-select-8 = Disposición > Lapela > Seleccionar lapela 8
command-tab-select-last = Disposición > Lapela > Seleccionar última lapela
command-close-pane = Disposición > Panel > Pechar panel
command-select-pane-left = Disposición > Panel > Seleccionar panel da esquerda
command-select-pane-right = Disposición > Panel > Seleccionar panel da dereita
command-select-pane-up = Disposición > Panel > Seleccionar panel superior
command-select-pane-down = Disposición > Panel > Seleccionar panel inferior
command-swap-pane-prev = Disposición > Panel > Intercambiar co panel anterior
command-swap-pane-next = Disposición > Panel > Intercambiar co panel seguinte
command-equalize-pane-size = Disposición > Panel > Igualar tamaño dos paneis
command-resize-pane-left = Disposición > Panel > Redimensionar panel á esquerda
command-resize-pane-right = Disposición > Panel > Redimensionar panel á dereita
command-resize-pane-up = Disposición > Panel > Redimensionar panel cara arriba
command-resize-pane-down = Disposición > Panel > Redimensionar panel cara abaixo
command-stack-close = Disposición > Pila > Pechar pila
command-stack-next = Disposición > Pila > Pila seguinte
command-stack-previous = Disposición > Pila > Pila anterior
command-stack-reopen = Disposición > Pila > Reabrir páxina pechada
command-stack-swap-prev = Disposición > Pila > Mover pila á esquerda
command-stack-swap-next = Disposición > Pila > Mover pila á dereita
command-space-open = Disposición > Espazo > Espazos
command-terminal-close = Terminal > Pechar terminal
command-terminal-next = Terminal > Terminal seguinte
command-terminal-prev = Terminal > Terminal anterior
command-terminal-clear = Terminal > Limpar terminal
command-browser-prev-page = Navegador > Navegación > Atrás
command-browser-next-page = Navegador > Navegación > Adiante
command-browser-reload = Navegador > Navegación > Recargar
command-browser-hard-reload = Navegador > Navegación > Recarga completa
command-open-in-place = Navegador > Abrir > Abrir aquí
command-open-in-new-stack = Navegador > Abrir > Abrir nunha pila nova
command-open-in-pane-top = Navegador > Abrir > Abrir no panel superior
command-open-in-pane-right = Navegador > Abrir > Abrir no panel da dereita
command-open-in-pane-bottom = Navegador > Abrir > Abrir no panel inferior
command-open-in-pane-left = Navegador > Abrir > Abrir no panel da esquerda
command-open-in-new-tab = Navegador > Abrir > Abrir nunha lapela nova
command-open-in-new-space = Navegador > Abrir > Abrir nun espazo novo
command-browser-zoom-in = Navegador > Vista > Ampliar
command-browser-zoom-out = Navegador > Vista > Reducir
command-browser-zoom-reset = Navegador > Vista > Tamaño real
command-browser-dev-tools = Navegador > Vista > Ferramentas de desenvolvemento
command-browser-open-command-bar = Navegador > Barra > Barra de comandos
command-browser-open-page-in-command-bar = Navegador > Barra > Editar páxina
command-browser-open-path-bar = Navegador > Barra > Navegador de rutas
command-browser-open-commands = Navegador > Barra > Comandos
command-browser-open-history = Navegador > Barra > Historial
command-service-open = Servizo > Abrir monitor de servizos
command-bookmark-toggle-active = Marcador > Gardar páxina nos marcadores
command-bookmark-pin-active = Marcador > Fixar páxina

layout-tab = Lapela
layout-no-stacks = Sen pilas
layout-loading = Cargando…
layout-no-markdown-files = Non hai ficheiros Markdown
layout-empty-folder = Cartafol baleiro
layout-worktree = árbore de traballo
layout-folder-name = Nome do cartafol
layout-no-pins-bookmarks = Non hai fixados nin marcadores
layout-move-to = Mover a { $folder }
layout-bookmark-current-page = Gardar a páxina actual nos marcadores
layout-rename-folder = Renomear cartafol
layout-remove-folder = Eliminar cartafol
layout-update-downloading = Descargando actualización
layout-update-installing = Instalando actualización…
layout-update-ready = Hai unha versión nova dispoñible
layout-restart-update = Reiniciar para actualizar

agent-preparing = Preparando o axente…
agent-send-all-queued = Enviar agora todas as indicacións na cola (Esc)
agent-send = Enviar (Intro)
agent-ready = Listo cando queiras.
agent-loading-older = Cargando mensaxes anteriores…
agent-load-older = Cargar mensaxes anteriores
agent-continued-from = Continuación de { $source }
agent-older-context-omitted = omitiuse o contexto anterior
agent-interrupted = interrompido
agent-allow-tool = Permitir { $tool }?
agent-deny = Denegar
agent-allow-always = Permitir sempre
agent-allow = Permitir
agent-loading-sessions = Cargando sesións…
agent-no-resumable-sessions = Non se atoparon sesións que retomar
agent-no-matching-sessions = Non hai sesións coincidentes
agent-no-matching-models = Non hai modelos coincidentes
agent-choice-help = ↑/↓ ou Ctrl+N/Ctrl+P · 1–9 · Intro
agent-choose-repository = Escoller cartafol do repositorio
agent-choose-repository-detail = Selecciona o repositorio Git local que debe usar o axente.
agent-choosing = Escollendo…
agent-choose-folder = Escoller cartafol
agent-queued = na cola
agent-attached = Anexado:
agent-cancel-queued = Cancelar indicación na cola
agent-resume-queued = Retomar indicacións na cola
agent-clear-queue = Baleirar cola
agent-send-all-now = enviar todo agora
agent-choose-option = Escolle unha opción arriba
agent-loading-media = Cargando contido multimedia…
agent-no-matching-media = Non hai contido multimedia coincidente
agent-prompt-context = Contexto da indicación
agent-details = Detalles
agent-path = Ruta
agent-tool = Ferramenta
agent-server = Servidor
agent-bytes = { $count } bytes
agent-worked-for = Traballou durante { $duration }
agent-worked-for-steps = { $count ->
    [one] Traballou durante { $duration } · 1 paso
   *[other] Traballou durante { $duration } · { $count } pasos
}
agent-tool-guardian-review = Revisión de Guardian
agent-tool-read-files = Leu ficheiros
agent-tool-viewed-image = Viu unha imaxe
agent-tool-used-browser = Usou o navegador
agent-tool-searched-files = Buscou ficheiros
agent-tool-ran-commands = Executou comandos
agent-thinking = Pensando
agent-subagent = Subaxente
agent-prompt = Indicación
agent-thread = Fío
agent-parent = Pai
agent-children = Fillos
agent-call = Chamada
agent-raw-event = Evento en bruto
agent-plan = Plan
agent-tasks = { $count ->
    [one] 1 tarefa
   *[other] { $count } tarefas
}
agent-edited = Editado
agent-reconnecting = Reconectando { $attempt }/{ $total }
agent-status-running = En execución
agent-status-done = Feito
agent-status-failed = Fallou
agent-status-pending = Pendente
agent-slash-attach-files = Anexar ficheiros
agent-slash-resume-session = Retomar unha sesión anterior
agent-slash-select-model = Seleccionar modelo
agent-slash-continue-cli = Continuar esta sesión na CLI
agent-session-just-now = agora mesmo
agent-session-minutes-ago = hai { $count } min
agent-session-hours-ago = hai { $count } h
agent-session-days-ago = hai { $count } d
agent-working-working = Traballando
agent-working-thinking = Pensando
agent-working-pondering = Reflexionando
agent-working-noodling = Dándolle voltas
agent-working-percolating = Madurando
agent-working-conjuring = Conxurando
agent-working-cooking = Cociñando
agent-working-brewing = Preparando
agent-working-musing = Cavilando
agent-working-ruminating = Rumiando
agent-working-scheming = Tramando
agent-working-synthesizing = Sintetizando
agent-working-tinkering = Axustando
agent-working-churning = Procesando
agent-working-vibing = Collendo o ton
agent-working-simmering = A lume lento
agent-working-crafting = Elaborando
agent-working-divining = Adiviñando
agent-working-mulling = Meditando
agent-working-spelunking = Explorando a fondo

editor-toggle-explorer = Alternar Explorador (Cmd+B)
editor-unsaved = sen gardar
editor-rendered-markdown = Markdown renderizado con edición en directo
editor-note = Nota
editor-source-editor = Editor de código
editor-editor = Editor
editor-git-diff = Diferenza de Git
editor-diff = Diferenza
editor-tidy = Ordenar
editor-always = Sempre
editor-unchanged-previews = { $count ->
    [one] ✦ 1 vista previa sen cambios
   *[other] ✦ { $count } vistas previas sen cambios
}
editor-open-externally = Abrir externamente
editor-changed-line = Liña modificada
editor-go-to-definition = Ir á definición
editor-find-references = Buscar referencias
editor-references = { $count ->
    [one] 1 referencia
   *[other] { $count } referencias
}
editor-lsp-starting = { $server } iniciando…
editor-lsp-not-installed = { $server } — non instalado
editor-explorer = Explorador
editor-open-editors = Editores abertos
editor-outline = Esquema
editor-new-file = Novo ficheiro
editor-new-folder = Novo cartafol
editor-delete-confirm = Eliminar “{ $name }”? Isto non se pode desfacer.
editor-created-folder = Cartafol { $name } creado
editor-created-file = Ficheiro { $name } creado
editor-renamed-to = Renomeado a { $name }
editor-deleted = { $name } eliminado
editor-failed-decode-image = Non se puido descodificar a imaxe
editor-preview-large-image = imaxe (grande de máis para a vista previa)
editor-preview-binary = binario
editor-preview-file = ficheiro

git-status-clean = limpo
git-status-modified = modificado
git-status-staged = preparado
git-status-staged-modified = preparado*
git-status-untracked = sen seguimento
git-status-deleted = eliminado
git-status-conflict = conflito
git-accept-all = ✓ aceptar todo
git-unstage = Quitar da área de preparación
git-confirm-deny-all = Confirmar denegar todo
git-deny-all = ✗ denegar todo
git-commit-message = mensaxe do commit
git-commit = Commit ({ $count })
git-push = ↑ Enviar
git-loading-diff = Cargando diferenza…
git-no-changes = Non hai cambios que mostrar
git-accept = ✓ aceptar
git-deny = ✗ denegar
git-show-unchanged-lines = Mostrar { $count } liñas sen cambios

terminal-loading = Cargando…
terminal-runs-when-ready = execútase cando estea listo · Ctrl+C limpa · Esc omite
terminal-booting = arrancando
terminal-type-command = escribe un comando · execútase cando estea listo · Esc omite

setup-tagline-claude = O axente de programación de Anthropic, en Vmux
setup-tagline-codex = O axente de programación de OpenAI, en Vmux
setup-tagline-vibe = O axente de programación de Mistral, en Vmux
setup-install-title = Instalar a CLI de { $name }
setup-homebrew-required = Homebrew é necesario para instalar { $command } e aínda non está configurado. Vmux instalará primeiro Homebrew e despois { $name }.
setup-terminal-instructions = No terminal, preme Intro para comezar e despois introduce o contrasinal do Mac cando se che pida.
setup-command-missing = Vmux abriu esta páxina porque o comando local { $command } aínda non está instalado. Executa o comando de abaixo para obtelo.
setup-install-failed = A instalación non rematou. Consulta o terminal para ver os detalles e téntao de novo.
setup-installing = Instalando…
setup-install-homebrew = Instalar Homebrew + { $name }
setup-run-install = Executar comando de instalación
setup-auto-reload = Vmux execútao nun terminal e recarga cando { $command } estea listo.

debug-title = Depuración
debug-auto-update = Actualización automática
debug-simulate-update = Simular actualización dispoñible
debug-simulate-download = Simular descarga
debug-clear-update = Limpar actualización
debug-trigger-restart = Activar reinicio

command-manage-spaces = Xestionar espazos…
command-pane-stack-location = panel { $pane } / pila { $stack }
command-space-pane-stack-location = { $space } / panel { $pane } / pila { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Modo interactivo
command-group-window = Xanela
command-group-tab = Lapela
command-group-pane = Panel
command-group-stack = Pila
command-group-space = Espazo
command-group-navigation = Navegación
command-group-open = Abrir
command-group-view = Vista
command-group-bar = Barra

menu-close-vmux = Pechar Vmux

agents-terminal-coding-agent = Axente de programación baseado no terminal
