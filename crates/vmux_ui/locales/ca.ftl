common-open = Obert
common-close = Tancar
common-install = Instal·lar
common-uninstall = Desinstal·la
common-update = Actualització
common-retry = Torna-ho a provar
common-refresh = Actualitzar
common-remove = Eliminar
common-enable = Activa
common-disable = Desactivar
common-new = Nou
common-active = actiu
common-running = corrent
common-done = fet
common-failed = Ha fallat
common-installed = Instal·lat
common-items = { $count ->
    [one] { $count } element
   *[other] { $count } elements
}
start-title = Comença
start-tagline = Una sol·licitud. Qualsevol cosa, fet.

agents-title = Agents
agents-search = Cerca agents ACP i CLI...
agents-empty = No hi ha agents coincidents
agents-empty-detail = Prova un nom, temps d'execució o ACP/CLI.
agents-install-failed = La instal·lació ha fallat
agents-updating = S'està actualitzant…
agents-retrying = S'està tornant a provar...
agents-preparing = S'està preparant…

extensions-title = Extensions
extensions-search = Cerca instal·lada o Chrome Web Store...
extensions-relaunch = Reinicia per aplicar
extensions-empty = No hi ha extensions instal·lades
extensions-no-match = No hi ha extensions coincidents
extensions-empty-detail = Cerqueu el Chrome Web Store de dalt i premeu Return.
extensions-no-match-detail = Prova amb un altre nom o identificador d'extensió.
extensions-on = Encès
extensions-off = Apagat
extensions-enable-confirm = Habilitar { $name }?
extensions-enable-permissions = Activa { $name } i permet:

lsp-title = Servidors d'idiomes
lsp-search = Cerca servidors d'idiomes, linters, formatedors...
lsp-loading = S'està carregant el catàleg...
lsp-empty = No hi ha servidors d'idiomes coincidents
lsp-empty-detail = Prova un altre idioma, linter o formatador.
lsp-needs = necessita { $tool }
lsp-status-available = Disponible
lsp-status-on-path = El PATH
lsp-status-installing = S'està instal·lant...
lsp-status-installed = Instal·lat
lsp-status-outdated = Actualització disponible
lsp-status-running = Córrer
lsp-status-failed = Ha fallat

spaces-title = Espais
spaces-new-placeholder = Nou nom de l'espai
spaces-empty = Sense espais
spaces-default-name = Espai { $number }
spaces-tabs = { $count ->
    [one] 1 pestanya
   *[other] { $count } pestanyes
}
spaces-delete = Suprimeix l'espai

team-title = Equip
team-just-you = Només tu en aquest espai
team-agents = { $count ->
    [one] Tu i 1 agent
   *[other] Tu i els { $count } agents
}
team-empty = Aquí encara no hi ha ningú
team-you = tu
team-agent = Agent

services-title = Serveis de fons
services-processes = { $count ->
    [one] 1 procés
   *[other] { $count } processos
}
services-kill-all = Matar a tots
services-not-running = El servei no s'està executant
services-start-with = Comença amb:
services-empty = No hi ha processos actius
services-filter = Processos de filtrat...
services-no-match = No hi ha processos de concordança
services-connected = Connectat
services-disconnected = Desconnectat
services-attached = adjunt
services-kill = Matar
services-memory = Memòria
services-size = Mida
services-shell = Shell

error-title = Error

history-search = Historial de cerques
history-clear-all = Esborra-ho tot
history-clear-confirm = Esborrar tot l'historial?
history-clear-warning = Això no es pot desfer.
history-cancel = Cancel·la
history-today = Avui
history-yesterday = Ahir
history-days-ago = Fa { $count } dies
history-day-offset = Dia -{ $count }

settings-title = Configuració
settings-loading = S'està carregant la configuració...
settings-stored = Emmagatzemat a ~/.vmux/settings.ron
settings-other = Altres
settings-software-update = Actualització de programari
settings-check-updates = Comproveu si hi ha actualitzacions
settings-check-updates-hint = Comprova automàticament a l'inici i cada hora quan l'actualització automàtica està activada.
settings-update-unavailable = No disponible
settings-update-unavailable-hint = L'actualitzador no està inclòs en aquesta compilació.
settings-update-checking = S'està comprovant…
settings-update-checking-hint = S'estan buscant actualitzacions...
settings-update-check-again = Comproveu de nou
settings-update-current = Vmux està actualitzat.
settings-update-downloading = S'està baixant…
settings-update-downloading-hint = S'està baixant Vmux { $version }…
settings-update-installing = S'està instal·lant...
settings-update-installing-hint = S'està instal·lant Vmux { $version }…
settings-update-ready = Actualització llesta
settings-update-ready-hint = Vmux { $version } està llest. Reinicieu per aplicar-lo.
settings-update-try-again = Torna-ho a provar
settings-update-failed = No es poden comprovar si hi ha actualitzacions.
settings-item = Item
settings-item-number = Element { $number }
settings-press-key = Premeu una tecla...
settings-saved = Desat
settings-record-key = Feu clic per gravar una nova combinació de tecles

tray-open-window = Finestra oberta
tray-close-window = Tanca la finestra
tray-pause-recording = Posa en pausa la gravació
tray-resume-recording = Reprendre la gravació
tray-finish-recording = Acabar la gravació
tray-quit = Surt de Vmux

composer-attach-files = Adjunta fitxers (/upload)
composer-remove-attachment = Elimina el fitxer adjunt

layout-back = Enrere
layout-forward = Endavant
layout-reload = Torna a carregar
layout-bookmark-page = Afegiu aquesta pàgina a les adreces d'interès
layout-remove-bookmark = Elimina el marcador
layout-pin-page = Fixeu aquesta pàgina
layout-unpin-page = Deixa de fixar aquesta pàgina
layout-manage-extensions = Gestionar extensions
layout-new-stack = Nova pila
layout-close-tab = Tanca la pestanya
layout-bookmark = Marcador
layout-pin = Pin
layout-new-tab = Pestanya nova
layout-team = Equip

command-switch-space = Canvia d'espai...
command-search-ask = Cerca o pregunta...
command-new-tab-placeholder = Cerqueu o escriviu un URL, o seleccioneu Terminal...
command-placeholder = Escriviu un URL, cerca pestanyes o > per a ordres...
command-composer-placeholder = Escriviu / per a les ordres o @ per als mitjans
command-send = Envia (Enter)
command-terminal = Terminal
command-open-terminal = Obre a la terminal
command-stack = Pila
command-tabs = { $count ->
    [one] 1 pestanya
   *[other] { $count } pestanyes
}
command-prompt = Avís
command-new-tab = Pestanya nova
command-search = Cerca
command-open-value = Obre "{ $value }"
command-search-value = Cerca "{ $value }"

schema-appearance = Aparença
schema-general = General
schema-layout = Disseny
schema-layout-detail = Finestra, panells, barra lateral i anell de focus.
schema-agent = Agent
schema-agent-detail = Comportament de l'agent i permisos d'eina.
schema-shortcuts = Dreceres
schema-shortcuts-detail = Visualització de només lectura. Editeu settings.ron directament per canviar els enllaços.
schema-terminal = Terminal
schema-browser = Navegador
schema-mode = Mode
schema-mode-detail = Esquema de colors per a pàgines web. El dispositiu segueix el vostre sistema.
schema-device = Dispositiu
schema-light = Llum
schema-dark = Fosc
schema-language = Llengua
schema-language-detail = Utilitzeu el sistema, en-US, ja o qualsevol etiqueta BCP 47 amb un catàleg ~/.vmux/locales/<tag>.ftl coincident.
schema-auto-update = Actualització automàtica
schema-auto-update-detail = Comproveu i instal·leu actualitzacions al llançament i cada hora.
schema-startup-url = Inici URL
schema-startup-url-detail = Buida obre el indicador de la barra d'ordres.
schema-search-engine = Motor de cerca
schema-search-engine-detail = S'utilitza per a cerques web des d'Inici i la barra d'ordres.
schema-window = Finestra
schema-pane = Panell
schema-side-sheet = Fulla lateral
schema-focus-ring = Anell de focus
schema-run-placement = Permet la substitució de la ubicació d'execució
schema-run-placement-detail = Deixeu que els agents escullin el mode, la direcció i l'ancoratge del panell d'execució.
schema-leader = Líder
schema-leader-detail = Tecla de prefix per a dreceres d'acords.
schema-chord-timeout = Temps d'espera de l'acord
schema-chord-timeout-detail = Mil·lisegons abans que caduqui un prefix d'acord.
schema-bindings = Enquadernacions
schema-confirm-close = Confirmeu el tancament
schema-confirm-close-detail = Pregunta abans de tancar un terminal amb un procés en execució.
schema-default-theme = Tema predeterminat
schema-default-theme-detail = Nom del tema actiu de la llista de temes.
