common-open = Aberto
common-close = Pechar
common-install = Instalar
common-uninstall = Desinstalar
common-update = Actualizar
common-retry = Volve tentar
common-refresh = Actualizar
common-remove = Eliminar
common-enable = Activar
common-disable = Desactivar
common-new = Novo
common-active = activo
common-running = correndo
common-done = feito
common-failed = Fallou
common-installed = Instalado
common-items = { $count ->
    [one] { $count } elemento
   *[other] { $count } elementos
}
start-title = Comeza
start-tagline = Un aviso. Calquera cousa, feito.

agents-title = Axentes
agents-search = Busca axentes ACP e CLI...
agents-empty = Non hai axentes coincidentes
agents-empty-detail = Proba cun nome, tempo de execución ou ACP/CLI.
agents-install-failed = Produciuse un erro na instalación
agents-updating = Actualizando…
agents-retrying = Tentando de novo…
agents-preparing = Preparando…

extensions-title = Extensións
extensions-search = Busca instalada ou Chrome Web Store…
extensions-relaunch = Reiniciar para solicitar
extensions-empty = Non hai extensións instaladas
extensions-no-match = Non hai extensións coincidentes
extensions-empty-detail = Busca no Chrome Web Store anterior e preme Return.
extensions-no-match-detail = Proba con outro nome ou ID de extensión.
extensions-on = Activado
extensions-off = Desactivado
extensions-enable-confirm = Activar { $name }?
extensions-enable-permissions = Activa { $name } e permite:

lsp-title = Servidores de idiomas
lsp-search = Busca servidores de idiomas, linters, formateadores...
lsp-loading = Cargando catálogo...
lsp-empty = Non hai servidores de idiomas coincidentes
lsp-empty-detail = Proba outro idioma, linter ou formateador.
lsp-needs = precisa { $tool }
lsp-status-available = Dispoñible
lsp-status-on-path = En PATH
lsp-status-installing = Instalando…
lsp-status-installed = Instalado
lsp-status-outdated = Actualización dispoñible
lsp-status-running = Correndo
lsp-status-failed = Fallou

spaces-title = Espazos
spaces-new-placeholder = Novo nome do espazo
spaces-empty = Sen espazos
spaces-default-name = Espazo { $number }
spaces-tabs = { $count ->
    [one] 1 pestana
   *[other] { $count } pestanas
}
spaces-delete = Eliminar espazo

team-title = Equipo
team-just-you = Só ti neste espazo
team-agents = { $count ->
    [one] Ti e 1 axente
   *[other] Ti e os axentes { $count }
}
team-empty = Ninguén aquí aínda
team-you = Ti
team-agent = Axente

services-title = Servizos de antecedentes
services-processes = { $count ->
    [one] 1 proceso
   *[other] { $count } procesos
}
services-kill-all = Matar a todos
services-not-running = O servizo non está funcionando
services-start-with = Comeza con:
services-empty = Non hai procesos activos
services-filter = Procesos de filtrado...
services-no-match = Non hai procesos de coincidencia
services-connected = Conectado
services-disconnected = Desconectado
services-attached = adxunto
services-kill = Matar
services-memory = Memoria
services-size = Tamaño
services-shell = Concha

error-title = Erro

history-search = Historial de busca
history-clear-all = Borrar todo
history-clear-confirm = Borrar todo o historial?
history-clear-warning = Isto non se pode desfacer.
history-cancel = Cancelar
history-today = Hoxe
history-yesterday = Onte
history-days-ago = Hai { $count } días
history-day-offset = Día -{ $count }

settings-title = Configuración
settings-loading = Cargando a configuración...
settings-stored = Almacenado en ~/.vmux/settings.ron
settings-other = Outros
settings-software-update = Actualización de software
settings-check-updates = Consulta as actualizacións
settings-check-updates-hint = Comproba automaticamente no inicio e cada hora cando a actualización automática está activada.
settings-update-unavailable = Non dispoñible
settings-update-unavailable-hint = O actualizador non está incluído nesta compilación.
settings-update-checking = Comprobando…
settings-update-checking-hint = Buscando actualizacións...
settings-update-check-again = Comproba de novo
settings-update-current = Vmux está actualizado.
settings-update-downloading = Descargando…
settings-update-downloading-hint = Descargando Vmux { $version }…
settings-update-installing = Instalando…
settings-update-installing-hint = Instalando Vmux { $version }…
settings-update-ready = Actualización lista
settings-update-ready-hint = Vmux { $version } está listo. Reinicia para aplicalo.
settings-update-try-again = Téntao de novo
settings-update-failed = Non se poden buscar actualizacións.
settings-item = Elemento
settings-item-number = Elemento { $number }
settings-press-key = Preme unha tecla...
settings-saved = Gardado
settings-record-key = Fai clic para gravar unha nova combinación de teclas

tray-open-window = Abrir ventá
tray-close-window = Pechar a xanela
tray-pause-recording = Pausa a gravación
tray-resume-recording = Retomar a gravación
tray-finish-recording = Finalizar a gravación
tray-quit = Saír de Vmux

composer-attach-files = Anexar ficheiros (/upload)
composer-remove-attachment = Eliminar anexo

layout-back = De volta
layout-forward = Adiante
layout-reload = Recarga
layout-bookmark-page = Marca esta páxina
layout-remove-bookmark = Eliminar o marcador
layout-pin-page = Fixar esta páxina
layout-unpin-page = Deixa de fixar esta páxina
layout-manage-extensions = Xestionar extensións
layout-new-stack = Nova pila
layout-close-tab = Pechar a pestana
layout-bookmark = Marcador
layout-pin = Pin
layout-new-tab = Nova pestana
layout-team = Equipo

command-switch-space = Cambiar de espazo...
command-search-ask = Busca ou pregunta...
command-new-tab-placeholder = Busca ou escribe un URL ou selecciona Terminal...
command-placeholder = Escribe un URL, busca pestanas ou > comandos...
command-composer-placeholder = Escriba / para os comandos ou @ para os medios
command-send = Enviar (Enter)
command-terminal = Terminal
command-open-terminal = Abrir no Terminal
command-stack = Pila
command-tabs = { $count ->
    [one] 1 pestana
   *[other] { $count } pestanas
}
command-prompt = Aviso
command-new-tab = Nova pestana
command-search = Busca
command-open-value = Abrir "{ $value }"
command-search-value = Busca “{ $value }”

schema-appearance = Aparición
schema-general = Xeral
schema-layout = Maquetación
schema-layout-detail = Fiestra, paneis, barra lateral e anel de enfoque.
schema-agent = Axente
schema-agent-detail = Comportamento do axente e permisos da ferramenta.
schema-shortcuts = Atallos
schema-shortcuts-detail = Vista de só lectura. Edita settings.ron directamente para cambiar as ligazóns.
schema-terminal = Terminal
schema-browser = Navegador
schema-mode = Modo
schema-mode-detail = Esquema de cores para páxinas web. O dispositivo segue o teu sistema.
schema-device = Dispositivo
schema-light = Luz
schema-dark = Escuro
schema-language = Linguaxe
schema-language-detail = Use system, en-US, ja ou calquera etiqueta BCP 47 cun catálogo ~/.vmux/locales/<tag>.ftl coincidente.
schema-auto-update = Actualización automática
schema-auto-update-detail = Busca e instala actualizacións no lanzamento e cada hora.
schema-startup-url = Inicio URL
schema-startup-url-detail = Baleiro abre o indicador da barra de comandos.
schema-search-engine = Buscador
schema-search-engine-detail = Utilízase para buscas web desde Inicio e a barra de comandos.
schema-window = Fiestra
schema-pane = Panel
schema-side-sheet = Folla lateral
schema-focus-ring = Anel de enfoque
schema-run-placement = Permitir a substitución da colocación de execución
schema-run-placement-detail = Permite que os axentes elixan o modo do panel de execución, a dirección e a ancoraxe.
schema-leader = Líder
schema-leader-detail = Tecla de prefixo para atallos de acordes.
schema-chord-timeout = Tempo de espera do acorde
schema-chord-timeout-detail = Milisegundos antes de que caduque un prefixo de acorde.
schema-bindings = Encadernacións
schema-confirm-close = Confirmar o peche
schema-confirm-close-detail = Pregunta antes de pechar un terminal cun proceso en execución.
schema-default-theme = Tema predeterminado
schema-default-theme-detail = Nome do tema activo da lista de temas.
