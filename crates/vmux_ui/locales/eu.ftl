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
common-active = aktiboa
common-running = korrika
common-done = eginda
common-failed = Huts egin du
common-installed = Instalatua
common-items = { $count ->
    [one] { $count } elementua
   *[other] { $count } elementu
}
start-title = Hasi
start-tagline = Galdera bat. Edozer, eginda.

agents-title = Eragileak
agents-search = Bilatu ACP eta CLI agenteak...
agents-empty = Ez dago bat datorren agenterik
agents-empty-detail = Saiatu izen bat, exekuzio-denbora edo ACP/CLI.
agents-install-failed = Ezin izan da instalatu
agents-updating = Eguneratzen…
agents-retrying = Berriro saiatzen…
agents-preparing = Prestatzen…

extensions-title = Luzapenak
extensions-search = Bilatu instalatuta edo Chrome Web Store…
extensions-relaunch = Berrabiarazi eskaera egiteko
extensions-empty = Ez dago luzapenik instalatu
extensions-no-match = Ez dago bat datorren luzapenik
extensions-empty-detail = Bilatu goiko Chrome Web Store eta sakatu Return.
extensions-no-match-detail = Saiatu beste izen edo luzapen ID batekin.
extensions-on = On
extensions-off = Desaktibatuta
extensions-enable-confirm = { $name } gaitu?
extensions-enable-permissions = Gaitu { $name } eta baimendu:

lsp-title = Hizkuntza Zerbitzariak
lsp-search = Bilatu hizkuntza-zerbitzariak, linters, formateatzaileak...
lsp-loading = Katalogoa kargatzen…
lsp-empty = Ez dago bat datorren hizkuntza zerbitzaririk
lsp-empty-detail = Saiatu beste hizkuntza, linter edo formateatu bat.
lsp-needs = behar { $tool }
lsp-status-available = Eskuragarri
lsp-status-on-path = PATH egunean
lsp-status-installing = Instalatzen…
lsp-status-installed = Instalatua
lsp-status-outdated = Eguneratzea eskuragarri
lsp-status-running = Korrika
lsp-status-failed = Huts egin du

spaces-title = Espazioak
spaces-new-placeholder = Espazioaren izen berria
spaces-empty = Espaziorik ez
spaces-default-name = Espazioa { $number }
spaces-tabs = { $count ->
    [one] 1 fitxa
   *[other] { $count } fitxak
}
spaces-delete = Ezabatu espazioa

team-title = Taldea
team-just-you = Espazio honetan zu bakarrik
team-agents = { $count ->
    [one] Zuk eta agente bat
   *[other] Zuk eta { $count } agente
}
team-empty = Hemen oraindik ez dago inor
team-you = Zuk
team-agent = Agentea

services-title = Aurrekariak Zerbitzuak
services-processes = { $count ->
    [one] 1 prozesu
   *[other] { $count } prozesuak
}
services-kill-all = Hil guztiak
services-not-running = Zerbitzua ez dago martxan
services-start-with = Hasi:
services-empty = Ez dago prozesu aktiborik
services-filter = Iragazi prozesuak...
services-no-match = Ez dago bat etortze prozesurik
services-connected = Konektatuta
services-disconnected = Deskonektatuta
services-attached = erantsita
services-kill = Hil
services-memory = Memoria
services-size = Tamaina
services-shell = Maskorra

error-title = Errorea

history-search = Bilaketa historia
history-clear-all = Garbitu dena
history-clear-confirm = Historia guztia garbitu?
history-clear-warning = Hau ezin da desegin.
history-cancel = Utzi
history-today = Gaur
history-yesterday = Atzo
history-days-ago = Duela { $count } egun
history-day-offset = Eguna -{ $count }

settings-title = Ezarpenak
settings-loading = Ezarpenak kargatzen…
settings-stored = ~/.vmux/settings.ron-n gordeta
settings-other = Bestela
settings-software-update = Software eguneratzea
settings-check-updates = Egiaztatu eguneratzeak
settings-check-updates-hint = Automatikoki egiaztatzen du abiaraztean eta orduro eguneratze automatikoa gaituta dagoenean.
settings-update-unavailable = Ez dago erabilgarri
settings-update-unavailable-hint = Eguneratzailea ez dago konpilazio honetan sartzen.
settings-update-checking = Egiaztatzen…
settings-update-checking-hint = Eguneratzeak bilatzen…
settings-update-check-again = Egiaztatu Berriz
settings-update-current = Vmux eguneratuta dago.
settings-update-downloading = Deskargatzen…
settings-update-downloading-hint = Vmux { $version } deskargatzen…
settings-update-installing = Instalatzen…
settings-update-installing-hint = Vmux { $version } instalatzen…
settings-update-ready = Eguneratze prest
settings-update-ready-hint = Vmux { $version } prest dago. Berrabiarazi aplikatzeko.
settings-update-try-again = Saiatu berriro
settings-update-failed = Ezin dira egiaztatu eguneratzeak.
settings-item = Elementua
settings-item-number = { $number } elementua
settings-press-key = Sakatu tekla bat...
settings-saved = Gorde
settings-record-key = Egin klik tekla konbinazio berri bat grabatzeko

tray-open-window = Ireki leihoa
tray-close-window = Itxi leihoa
tray-pause-recording = Eten grabazioa
tray-resume-recording = Berrekin grabaketari
tray-finish-recording = Amaitu grabaketa
tray-quit = Irten Vmux

composer-attach-files = Erantsi fitxategiak (/upload)
composer-remove-attachment = Kendu eranskina

layout-back = Itzuli
layout-forward = Aurrera
layout-reload = Berriz kargatu
layout-bookmark-page = Markatu orri hau
layout-remove-bookmark = Kendu laster-marka
layout-pin-page = Ainguratu orri hau
layout-unpin-page = Kendu orri hau
layout-manage-extensions = Kudeatu luzapenak
layout-new-stack = Pila berria
layout-close-tab = Itxi fitxa
layout-bookmark = Laster-marka
layout-pin = Pin
layout-new-tab = Fitxa berria
layout-team = Taldea

command-switch-space = Aldatu espazioa…
command-search-ask = Bilatu edo galdetu...
command-new-tab-placeholder = Bilatu edo idatzi URL bat, edo hautatu Terminal...
command-placeholder = Idatzi URL, bilatu fitxak edo > komandoak...
command-composer-placeholder = Idatzi / komandoetarako edo @ multimediarako
command-send = Bidali (Enter)
command-terminal = Terminala
command-open-terminal = Ireki Terminalean
command-stack = Pila
command-tabs = { $count ->
    [one] 1 fitxa
   *[other] { $count } fitxak
}
command-prompt = Galdera
command-new-tab = Fitxa berria
command-search = Bilatu
command-open-value = Ireki "{ $value }"
command-search-value = Bilatu “{ $value }”

schema-appearance = Itxura
schema-general = Orokorra
schema-layout = Diseinua
schema-layout-detail = Leihoa, panelak, alboko barra eta fokatze-eraztuna.
schema-agent = Agentea
schema-agent-detail = Agentearen portaera eta tresnaren baimenak.
schema-shortcuts = Lasterbideak
schema-shortcuts-detail = Irakurtzeko soilik ikuspegia. Editatu settings.ron zuzenean loturak aldatzeko.
schema-terminal = Terminala
schema-browser = Arakatzailea
schema-mode = Modua
schema-mode-detail = Web orrietarako kolore eskema. Gailuak zure sistema jarraitzen du.
schema-device = Gailua
schema-light = Argia
schema-dark = Iluna
schema-language = Hizkuntza
schema-language-detail = Erabili sistema, en-US, ja edo edozein BCP 47 etiketa bat datorren ~/.vmux/locales/<tag>.ftl katalogo batekin.
schema-auto-update = Eguneratze automatikoa
schema-auto-update-detail = Egiaztatu eta instalatu eguneraketak abiaraztean eta orduro.
schema-startup-url = Abiarazi URL
schema-startup-url-detail = Hutsik komando-barrako gonbita irekitzen du.
schema-search-engine = Bilatzailea
schema-search-engine-detail = Hasi eta komando barratik web bilaketak egiteko erabiltzen da.
schema-window = Leihoa
schema-pane = Panela
schema-side-sheet = Alboko orria
schema-focus-ring = Foku eraztuna
schema-run-placement = Baimendu exekuzioaren kokapena gainidaztea
schema-run-placement-detail = Utzi agenteei exekutatzeko panelaren modua, norabidea eta aingura aukeratzen.
schema-leader = Liderra
schema-leader-detail = Aurrizki-tekla akordeen lasterbideetarako.
schema-chord-timeout = Akordeen denbora-muga
schema-chord-timeout-detail = Akorde-aurrizki bat iraungi baino milisegundo lehenago.
schema-bindings = Loturak
schema-confirm-close = Berretsi ixtea
schema-confirm-close-detail = Galdetu abian den prozesu batekin terminal bat itxi aurretik.
schema-default-theme = Gai lehenetsia
schema-default-theme-detail = Gaien zerrendako gai aktiboaren izena.
