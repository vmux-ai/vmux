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
start-tagline = Un mensaje. Cualquier cosa, listo.

agents-title = Agentes
agents-search = Buscar agentes ACP y CLI…
agents-empty = Sin agentes coincidentes
agents-empty-detail = Prueba con un nombre, entorno de ejecución o ACP/CLI.
agents-install-failed = Error de instalación
agents-updating = Actualizando…
agents-retrying = Reintentando…
agents-preparing = Preparando…

extensions-title = Extensiones
extensions-search = Buscar instaladas o en Chrome Web Store…
extensions-relaunch = Reiniciar para aplicar
extensions-empty = Sin extensiones instaladas
extensions-no-match = Sin extensiones coincidentes
extensions-empty-detail = Busca en Chrome Web Store arriba y pulsa Return.
extensions-no-match-detail = Prueba otro nombre o ID de extensión.
extensions-on = Activada
extensions-off = Desactivada
extensions-enable-confirm = ¿Activar { $name }?
extensions-enable-permissions = Activar { $name } y permitir:

lsp-title = Servidores de lenguaje
lsp-search = Buscar servidores de lenguaje, linters, formateadores…
lsp-loading = Cargando catálogo…
lsp-empty = Sin servidores de lenguaje coincidentes
lsp-empty-detail = Prueba otro lenguaje, linter o formateador.
lsp-needs = requiere { $tool }
lsp-status-available = Disponible
lsp-status-on-path = En PATH
lsp-status-installing = Instalando…
lsp-status-installed = Instalado
lsp-status-outdated = Actualización disponible
lsp-status-running = En ejecución
lsp-status-failed = Error

spaces-title = Espacios
spaces-new-placeholder = Nombre del nuevo espacio
spaces-empty = Sin espacios
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
services-kill-all = Terminar todos
services-not-running = El servicio no está en ejecución
services-start-with = Iniciar con:
services-empty = Sin procesos activos
services-filter = Filtrar procesos…
services-no-match = Sin procesos coincidentes
services-connected = Conectado
services-disconnected = Desconectado
services-attached = adjunto
services-kill = Terminar
services-memory = Memoria
services-size = Tamaño
services-shell = Shell

error-title = Error

history-search = Buscar historial
history-clear-all = Borrar todo
history-clear-confirm = ¿Borrar todo el historial?
history-clear-warning = Esta acción no se puede deshacer.
history-cancel = Cancelar
history-today = Hoy
history-yesterday = Ayer
history-days-ago = Hace { $count } días
history-day-offset = Día -{ $count }

settings-title = Configuración
settings-loading = Cargando configuración…
settings-stored = Guardado en ~/.vmux/settings.ron
settings-other = Otro
settings-software-update = Actualización de software
settings-check-updates = Buscar actualizaciones
settings-check-updates-hint = Comprueba automáticamente al iniciar y cada hora cuando la actualización automática está activada.
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
settings-update-failed = No se pudo comprobar las actualizaciones.
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
composer-remove-attachment = Eliminar archivo adjunto

layout-back = Atrás
layout-forward = Adelante
layout-reload = Recargar
layout-bookmark-page = Añadir a favoritos
layout-remove-bookmark = Eliminar favorito
layout-pin-page = Fijar esta página
layout-unpin-page = Desfijar esta página
layout-manage-extensions = Administrar extensiones
layout-new-stack = Nueva pila
layout-close-tab = Cerrar pestaña
layout-bookmark = Favorito
layout-pin = Fijar
layout-new-tab = Nueva pestaña
layout-team = Equipo

command-switch-space = Cambiar espacio…
command-search-ask = Buscar o preguntar…
command-new-tab-placeholder = Busca o escribe una URL, o selecciona Terminal…
command-placeholder = Escribe una URL, busca pestañas o > para comandos…
command-composer-placeholder = Escribe / para comandos o @ para multimedia
command-send = Enviar (Enter)
command-terminal = Terminal
command-open-terminal = Abrir en Terminal
command-stack = Pila
command-tabs = { $count ->
    [one] 1 pestaña
   *[other] { $count } pestañas
}
command-prompt = Mensaje
command-new-tab = Nueva pestaña
command-search = Buscar
command-open-value = Abrir "{ $value }"
command-search-value = Buscar "{ $value }"

schema-appearance = Apariencia
schema-general = General
schema-layout = Diseño
schema-layout-detail = Ventana, paneles, barra lateral y anillo de enfoque.
schema-agent = Agente
schema-agent-detail = Comportamiento del agente y permisos de herramientas.
schema-shortcuts = Atajos
schema-shortcuts-detail = Vista de solo lectura. Edita settings.ron directamente para cambiar los atajos.
schema-terminal = Terminal
schema-browser = Navegador
schema-mode = Modo
schema-mode-detail = Esquema de color para páginas web. El dispositivo sigue tu sistema.
schema-device = Dispositivo
schema-light = Claro
schema-dark = Oscuro
schema-language = Idioma
schema-language-detail = Usa el sistema, en-US, ja, o cualquier etiqueta BCP 47 con un catálogo ~/.vmux/locales/<tag>.ftl correspondiente.
schema-auto-update = Actualización automática
schema-auto-update-detail = Busca e instala actualizaciones al iniciar y cada hora.
schema-startup-url = URL de inicio
schema-startup-url-detail = Vacío abre el indicador de la barra de comandos.
schema-search-engine = Motor de búsqueda
schema-search-engine-detail = Usado para búsquedas web desde Inicio y la barra de comandos.
schema-window = Ventana
schema-pane = Panel
schema-side-sheet = Panel lateral
schema-focus-ring = Anillo de enfoque
schema-run-placement = Permitir anulación de ubicación de ejecución
schema-run-placement-detail = Permite que los agentes elijan el modo, dirección y anclaje del panel de ejecución.
schema-leader = Líder
schema-leader-detail = Tecla prefijo para atajos de acordes.
schema-chord-timeout = Tiempo de espera del acorde
schema-chord-timeout-detail = Milisegundos antes de que expire un prefijo de acorde.
schema-bindings = Atajos
schema-confirm-close = Confirmar cierre
schema-confirm-close-detail = Solicitar confirmación antes de cerrar un terminal con un proceso en ejecución.
schema-default-theme = Tema predeterminado
schema-default-theme-detail = Nombre del tema activo de la lista de temas.
