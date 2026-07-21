common-open = Abrir
common-close = Cerrar
common-install = Instalar
common-uninstall = Desinstalar
common-update = Actualizar
common-retry = Reintentar
common-refresh = Actualizar
common-remove = Eliminar
common-enable = Activar
common-disable = Desactivar
common-new = Nuevo
common-active = activo
common-running = en ejecución
common-done = listo
common-failed = Error
common-installed = Instalado
common-items = { $count ->
    [one] { $count } elemento
   *[other] { $count } elementos
}
start-title = Inicio
start-tagline = Un prompt. Cualquier cosa, hecha.

agents-title = Agentes
agents-search = Buscar agentes ACP y CLI…
agents-empty = No hay agentes coincidentes
agents-empty-detail = Prueba con un nombre, entorno de ejecución o ACP/CLI.
agents-install-failed = Error al instalar
agents-updating = Actualizando…
agents-retrying = Reintentando…
agents-preparing = Preparando…

extensions-title = Extensiones
extensions-search = Buscar instaladas o en Chrome Web Store…
extensions-relaunch = Reinicia para aplicar
extensions-empty = No hay extensiones instaladas
extensions-no-match = No hay extensiones coincidentes
extensions-empty-detail = Busca en Chrome Web Store arriba y pulsa Intro.
extensions-no-match-detail = Prueba con otro nombre o ID de extensión.
extensions-on = Activadas
extensions-off = Desactivadas
extensions-enable-confirm = ¿Activar { $name }?
extensions-enable-permissions = Activar { $name } y permitir:

lsp-title = Servidores de lenguaje
lsp-search = Buscar servidores de lenguaje, linters, formateadores…
lsp-loading = Cargando catálogo…
lsp-empty = No hay servidores de lenguaje coincidentes
lsp-empty-detail = Prueba con otro lenguaje, linter o formateador.
lsp-needs = necesita { $tool }
lsp-status-available = Disponible
lsp-status-on-path = En PATH
lsp-status-installing = Instalando…
lsp-status-installed = Instalado
lsp-status-outdated = Actualización disponible
lsp-status-running = En ejecución
lsp-status-failed = Error

spaces-title = Espacios
spaces-new-placeholder = Nombre del nuevo espacio
spaces-empty = No hay espacios
spaces-default-name = Espacio { $number }
spaces-tabs = { $count ->
    [one] 1 pestaña
   *[other] { $count } pestañas
}
spaces-delete = Eliminar espacio

team-title = Equipo
team-just-you = Solo tú en este espacio
team-agents = { $count ->
    [one] Tú y 1 agente
   *[other] Tú y { $count } agentes
}
team-empty = Aún no hay nadie aquí
team-you = Tú
team-agent = Agente

services-title = Servicios en segundo plano
services-processes = { $count ->
    [one] 1 proceso
   *[other] { $count } procesos
}
services-kill-all = Forzar cierre de todos
services-not-running = El servicio no se está ejecutando
services-start-with = Iniciar con:
services-empty = No hay procesos activos
services-filter = Filtrar procesos…
services-no-match = No hay procesos coincidentes
services-connected = Conectado
services-disconnected = Desconectado
services-attached = adjunto
services-kill = Forzar cierre
services-memory = Memoria
services-size = Tamaño
services-shell = Shell

error-title = Error

history-search = Buscar en el historial
history-clear-all = Borrar todo
history-clear-confirm = ¿Borrar todo el historial?
history-clear-warning = Esta acción no se puede deshacer.
history-cancel = Cancelar
history-today = Hoy
history-yesterday = Ayer
history-days-ago = Hace { $count } días
history-day-offset = Día -{ $count }

settings-title = Ajustes
settings-loading = Cargando ajustes…
settings-stored = Guardado en ~/.vmux/settings.ron
settings-other = Otros
settings-software-update = Actualización de software
settings-check-updates = Buscar actualizaciones
settings-check-updates-hint = Se comprueba automáticamente al iniciar y cada hora si la actualización automática está activada.
settings-update-unavailable = No disponible
settings-update-unavailable-hint = El actualizador no está incluido en esta compilación.
settings-update-checking = Comprobando…
settings-update-checking-hint = Buscando actualizaciones…
settings-update-check-again = Comprobar de nuevo
settings-update-current = Vmux está actualizado.
settings-update-downloading = Descargando…
settings-update-downloading-hint = Descargando Vmux { $version }…
settings-update-installing = Instalando…
settings-update-installing-hint = Instalando Vmux { $version }…
settings-update-ready = Actualización lista
settings-update-ready-hint = Vmux { $version } está listo. Reinicia para aplicarlo.
settings-update-try-again = Intentar de nuevo
settings-update-failed = No se pudo buscar actualizaciones.
settings-item = Elemento
settings-item-number = Elemento { $number }
settings-press-key = Pulsa una tecla…
settings-saved = Guardado
settings-record-key = Haz clic para grabar una nueva combinación de teclas

tray-open-window = Abrir ventana
tray-close-window = Cerrar ventana
tray-pause-recording = Pausar grabación
tray-resume-recording = Reanudar grabación
tray-finish-recording = Finalizar grabación
tray-quit = Salir de Vmux

composer-attach-files = Adjuntar archivos (/upload)
composer-remove-attachment = Quitar adjunto

layout-back = Atrás
layout-forward = Adelante
layout-reload = Recargar
layout-bookmark-page = Añadir esta página a marcadores
layout-remove-bookmark = Quitar marcador
layout-pin-page = Fijar esta página
layout-unpin-page = Desfijar esta página
layout-manage-extensions = Gestionar extensiones
layout-new-stack = Nueva pila
layout-close-tab = Cerrar pestaña
layout-bookmark = Marcador
layout-pin = Fijar
layout-new-tab = Nueva pestaña
layout-team = Equipo

command-switch-space = Cambiar de espacio…
command-search-ask = Buscar o preguntar…
command-new-tab-placeholder = Busca, escribe una URL o selecciona Terminal…
command-placeholder = Escribe una URL, busca pestañas o usa > para comandos…
command-composer-placeholder = Escribe / para comandos o @ para multimedia
command-send = Enviar (Intro)
command-terminal = Terminal
command-open-terminal = Abrir en Terminal
command-stack = Pila
command-tabs = { $count ->
    [one] 1 pestaña
   *[other] { $count } pestañas
}
command-prompt = Prompt
command-new-tab = Nueva pestaña
command-search = Buscar
command-open-value = Abrir “{ $value }”
command-search-value = Buscar “{ $value }”

schema-appearance = Apariencia
schema-general = General
schema-layout = Disposición
schema-layout-detail = Ventana, paneles, barra lateral y anillo de foco.
schema-agent = Agente
schema-agent-detail = Comportamiento del agente y permisos de herramientas.
schema-shortcuts = Atajos de teclado
schema-shortcuts-detail = Vista de solo lectura. Edita settings.ron directamente para cambiar las combinaciones.
schema-terminal = Terminal
schema-browser = Navegador
schema-mode = Modo
schema-mode-detail = Esquema de color para páginas web. Dispositivo sigue el sistema.
schema-device = Dispositivo
schema-light = Claro
schema-dark = Oscuro
schema-language = Idioma
schema-language-detail = Usa el sistema, en-US, ja o cualquier etiqueta BCP 47 con un catálogo ~/.vmux/locales/<tag>.ftl correspondiente.
schema-auto-update = Actualización automática
schema-auto-update-detail = Buscar e instalar actualizaciones al iniciar y cada hora.
schema-startup-url = URL de inicio
schema-startup-url-detail = Si está vacío, abre el prompt de la barra de comandos.
schema-search-engine = Motor de búsqueda
schema-search-engine-detail = Se usa para búsquedas web desde Inicio y la barra de comandos.
schema-window = Ventana
schema-pane = Panel
schema-side-sheet = Panel lateral
schema-focus-ring = Anillo de foco
schema-run-placement = Permitir anular la ubicación de ejecución
schema-run-placement-detail = Permite que los agentes elijan el modo, la dirección y el anclaje del panel de ejecución.
schema-leader = Líder
schema-leader-detail = Tecla prefijo para atajos en acorde.
schema-chord-timeout = Tiempo de espera del acorde
schema-chord-timeout-detail = Milisegundos antes de que caduque un prefijo de acorde.
schema-bindings = Combinaciones
schema-confirm-close = Confirmar cierre
schema-confirm-close-detail = Preguntar antes de cerrar una terminal con un proceso en ejecución.
schema-default-theme = Tema predeterminado
schema-default-theme-detail = Nombre del tema activo de la lista de temas.
