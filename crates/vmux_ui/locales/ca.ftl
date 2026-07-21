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
