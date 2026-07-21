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
