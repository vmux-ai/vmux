locale-name = euskara
common-open = Ireki
common-close = Itxi
common-install = Instalatu
common-uninstall = Desinstalatu
common-update = Eguneratu
common-retry = Saiatu berriro
common-refresh = Freskatu
common-remove = Kendu
common-enable = Gaitu
common-disable = Desgaitu
common-new = Berria
common-active = aktibo
common-running = martxan
common-done = eginda
common-failed = Huts egin du
common-installed = Instalatuta
common-items = { $count ->
    [one] elementu { $count }
   *[other] { $count } elementu
}
start-title = Hasi
start-tagline = Prompt bakarra. Edozer, eginda.

agents-title = Agenteak
agents-search = Bilatu ACP eta CLI agenteak…
agents-empty = Ez dago bat datorren agenterik
agents-empty-detail = Saiatu izen, exekuzio-ingurune edo ACP/CLI batekin.
agents-install-failed = Instalazioak huts egin du
agents-updating = Eguneratzen…
agents-retrying = Berriro saiatzen…
agents-preparing = Prestatzen…

extensions-title = Hedapenak
extensions-search = Bilatu instalatuetan edo Chrome Web Store-n…
extensions-relaunch = Berrabiarazi aplikatzeko
extensions-empty = Ez dago hedapenik instalatuta
extensions-no-match = Ez dago bat datorren hedapenik
extensions-empty-detail = Bilatu goian Chrome Web Store-n eta sakatu Sartu.
extensions-no-match-detail = Saiatu beste izen edo hedapen-ID batekin.
extensions-on = Aktibatuta
extensions-off = Desaktibatuta
extensions-enable-confirm = Gaitu { $name }?
extensions-enable-permissions = Gaitu { $name } eta baimendu:

lsp-title = Hizkuntza-zerbitzariak
lsp-search = Bilatu hizkuntza-zerbitzariak, linterrak, formatutzaileak…
lsp-loading = Katalogoa kargatzen…
lsp-empty = Ez dago bat datorren hizkuntza-zerbitzaririk
lsp-empty-detail = Saiatu beste hizkuntza, linter edo formatutzaile batekin.
lsp-needs = { $tool } behar du
lsp-status-available = Eskuragarri
lsp-status-on-path = PATHen
lsp-status-installing = Instalatzen…
lsp-status-installed = Instalatuta
lsp-status-outdated = Eguneraketa eskuragarri
lsp-status-running = Martxan
lsp-status-failed = Huts egin du

spaces-title = Guneak
spaces-new-placeholder = Gune berriaren izena
spaces-empty = Ez dago gunerik
spaces-default-name = { $number }. gunea
spaces-tabs = { $count ->
    [one] fitxa 1
   *[other] { $count } fitxa
}
spaces-delete = Ezabatu gunea

team-title = Taldea
team-just-you = Zu bakarrik zaude gune honetan
team-agents = { $count ->
    [one] Zu eta agente 1
   *[other] Zu eta { $count } agente
}
team-empty = Oraindik ez dago inor hemen
team-you = Zu
team-agent = Agentea

services-title = Atzeko planoko zerbitzuak
services-processes = { $count ->
    [one] prozesu 1
   *[other] { $count } prozesu
}
services-kill-all = Behartu denak ixtera
services-not-running = Zerbitzua ez dago martxan
services-start-with = Hasi honekin:
services-empty = Ez dago prozesu aktiborik
services-filter = Iragazi prozesuak…
services-no-match = Ez dago bat datorren prozesurik
services-connected = Konektatuta
services-disconnected = Deskonektatuta
services-attached = erantsita
services-kill = Behartu ixtera
services-memory = Memoria
services-size = Tamaina
services-shell = Shell-a

error-title = Errorea

history-search = Bilatu historian
history-clear-all = Garbitu dena
history-clear-confirm = Garbitu historia osoa?
history-clear-warning = Ezin da desegin.
history-cancel = Utzi
history-today = Gaur
history-yesterday = Atzo
history-days-ago = Duela { $count } egun
history-day-offset = Eguna -{ $count }

settings-title = Ezarpenak
settings-loading = Ezarpenak kargatzen…
settings-stored = ~/.vmux/settings.ron fitxategian gordeta
settings-other = Bestelakoak
settings-software-update = Software-eguneraketa
settings-check-updates = Bilatu eguneraketak
settings-check-updates-hint = Abiaraztean automatikoki eta orduro egiaztatzen du, eguneratze automatikoa gaituta badago.
settings-update-unavailable = Ez dago erabilgarri
settings-update-unavailable-hint = Eguneratzailea ez dago build honetan sartuta.
settings-update-checking = Egiaztatzen…
settings-update-checking-hint = Eguneraketak bilatzen…
settings-update-check-again = Egiaztatu berriro
settings-update-current = Vmux eguneratuta dago.
settings-update-downloading = Deskargatzen…
settings-update-downloading-hint = Vmux { $version } deskargatzen…
settings-update-installing = Instalatzen…
settings-update-installing-hint = Vmux { $version } instalatzen…
settings-update-ready = Eguneraketa prest
settings-update-ready-hint = Vmux { $version } prest dago. Berrabiarazi aplikatzeko.
settings-update-try-again = Saiatu berriro
settings-update-failed = Ezin izan dira eguneraketak egiaztatu.
settings-item = Elementua
settings-item-number = { $number }. elementua
settings-press-key = Sakatu tekla bat…
settings-saved = Gordeta
settings-record-key = Egin klik tekla-konbinazio berria grabatzeko

tray-open-window = Ireki leihoa
tray-close-window = Itxi leihoa
tray-pause-recording = Pausatu grabazioa
tray-resume-recording = Berrekin grabazioari
tray-finish-recording = Amaitu grabazioa
tray-quit = Irten Vmuxetik

composer-attach-files = Erantsi fitxategiak (/upload)
composer-remove-attachment = Kendu eranskina

layout-back = Atzera
layout-forward = Aurrera
layout-reload = Birkargatu
layout-bookmark-page = Gehitu orri hau laster-marketara
layout-remove-bookmark = Kendu laster-marka
layout-pin-page = Finkatu orri hau
layout-unpin-page = Askatu orri hau
layout-manage-extensions = Kudeatu hedapenak
layout-new-stack = Pila berria
layout-close-tab = Itxi fitxa
layout-bookmark = Laster-marka
layout-pin = Finkatu
layout-new-tab = Fitxa berria
layout-team = Taldea

command-switch-space = Aldatu gunea…
command-search-ask = Bilatu edo galdetu…
command-new-tab-placeholder = Bilatu edo idatzi URL bat, edo hautatu Terminala…
command-placeholder = Idatzi URL bat, bilatu fitxak, edo > komandoetarako…
command-composer-placeholder = Idatzi / komandoetarako edo @ multimedia eransteko
command-send = Bidali (Sartu)
command-terminal = Terminala
command-open-terminal = Ireki terminalean
command-stack = Pila
command-tabs = { $count ->
    [one] fitxa 1
   *[other] { $count } fitxa
}
command-prompt = Prompta
command-new-tab = Fitxa berria
command-search = Bilatu
command-open-value = Ireki “{ $value }”
command-search-value = Bilatu “{ $value }”

schema-appearance = Itxura
schema-general = Orokorra
schema-layout = Diseinua
schema-layout-detail = Leihoa, panelak, alboko barra eta foku-eraztuna.
schema-agent = Agentea
schema-agent-detail = Agentearen portaera eta tresnen baimenak.
schema-shortcuts = Lasterbideak
schema-shortcuts-detail = Irakurtzeko soilik. Lasterbideak aldatzeko, editatu settings.ron zuzenean.
schema-terminal = Terminala
schema-browser = Arakatzailea
schema-mode = Modua
schema-mode-detail = Web-orrien kolore-eskema. Gailuak zure sistemari jarraitzen dio.
schema-device = Gailua
schema-light = Argia
schema-dark = Iluna
schema-language = Hizkuntza
schema-language-detail = Erabili sistema, en-US, ja, edo edozein BCP 47 etiketa, dagokion ~/.vmux/locales/<tag>.ftl katalogoarekin.
schema-auto-update = Eguneratze automatikoa
schema-auto-update-detail = Bilatu eta instalatu eguneraketak abiaraztean eta orduro.
schema-startup-url = Abioko URLa
schema-startup-url-detail = Hutsik badago, komando-barraren prompta irekitzen du.
schema-search-engine = Bilaketa-motorra
schema-search-engine-detail = Hasieratik eta komando-barratik egindako web-bilaketetarako erabiltzen da.
schema-window = Leihoa
schema-pane = Panela
schema-side-sheet = Alboko orria
schema-focus-ring = Foku-eraztuna
schema-run-placement = Baimendu exekuzio-kokapena gainidaztea
schema-run-placement-detail = Utzi agenteei exekuzio-panelaren modua, norabidea eta aingura aukeratzen.
schema-leader = Aitzindaria
schema-leader-detail = Lasterbide-sekuentzietarako aurrizki-tekla.
schema-chord-timeout = Sekuentziaren denbora-muga
schema-chord-timeout-detail = Milisegundoak, sekuentzia-aurrizkia iraungi aurretik.
schema-bindings = Lasterbide-loturak
schema-confirm-close = Berretsi ixtea
schema-confirm-close-detail = Galdetu martxan dagoen prozesu bat duen terminala itxi aurretik.
schema-default-theme = Gai lehenetsia
schema-default-theme-detail = Gaien zerrendako gai aktiboaren izena.
