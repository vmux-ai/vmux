common-open = Sokafy
common-close = Akatony
common-install = Apetraho
common-uninstall = Esory
common-update = Havaozy
common-retry = Andramo indray
common-refresh = Havaozy
common-remove = Esory
common-enable = Alefaso
common-disable = Atsaharo
common-new = Vaovao
common-active = mavitrika
common-running = mandeha
common-done = vita
common-failed = Tsy nahomby
common-installed = Voapetraka
common-items = { $count ->
    [one] singa { $count }
   *[other] singa { $count }
}
start-title = Fanombohana
start-tagline = Prompt iray. Vita izay rehetra ilaina.

agents-title = Agent
agents-search = Hikaroka agent ACP sy CLI…
agents-empty = Tsy misy agent mifanaraka
agents-empty-detail = Andramo anarana, runtime, na ACP/CLI.
agents-install-failed = Tsy nahomby ny fametrahana
agents-updating = Manavao…
agents-retrying = Manandrana indray…
agents-preparing = Manomana…

extensions-title = Fanitarana
extensions-search = Hikaroka amin’ny voapetraka na Chrome Web Store…
extensions-relaunch = Alefaso indray hampiharana
extensions-empty = Tsy misy fanitarana voapetraka
extensions-no-match = Tsy misy fanitarana mifanaraka
extensions-empty-detail = Karohy ao amin’ny Chrome Web Store etsy ambony dia tsindrio Return.
extensions-no-match-detail = Andramo anarana hafa na ID fanitarana.
extensions-on = Mandeha
extensions-off = Atsahatra
extensions-enable-confirm = Alefa ny { $name }?
extensions-enable-permissions = Alefa ny { $name } ary avela:

lsp-title = Servera fiteny
lsp-search = Hikaroka servera fiteny, linter, formatter…
lsp-loading = Maka katalaogy…
lsp-empty = Tsy misy servera fiteny mifanaraka
lsp-empty-detail = Andramo fiteny, linter, na formatter hafa.
lsp-needs = mila { $tool }
lsp-status-available = Misy
lsp-status-on-path = Ao amin’ny PATH
lsp-status-installing = Mametraka…
lsp-status-installed = Voapetraka
lsp-status-outdated = Misy fanavaozana
lsp-status-running = Mandeha
lsp-status-failed = Tsy nahomby

spaces-title = Sehatra
spaces-new-placeholder = Anaran’ny sehatra vaovao
spaces-empty = Tsy misy sehatra
spaces-default-name = Sehatra { $number }
spaces-tabs = { $count ->
    [one] kiheba 1
   *[other] kiheba { $count }
}
spaces-delete = Fafao ny sehatra

team-title = Ekipa
team-just-you = Ianao irery ato amin’ity sehatra ity
team-agents = { $count ->
    [one] Ianao sy agent 1
   *[other] Ianao sy agent { $count }
}
team-empty = Tsy mbola misy eto
team-you = Ianao
team-agent = Agent

services-title = Serivisy ao ambadika
services-processes = { $count ->
    [one] dingana 1
   *[other] dingana { $count }
}
services-kill-all = Tapaho daholo
services-not-running = Tsy mandeha ny serivisy
services-start-with = Atombohy amin’ny:
services-empty = Tsy misy dingana mavitrika
services-filter = Sivano ny dingana…
services-no-match = Tsy misy dingana mifanaraka
services-connected = Mifandray
services-disconnected = Tapaka
services-attached = miraikitra
services-kill = Tapaho
services-memory = Fahatsiarovana
services-size = Habe
services-shell = Shell

error-title = Hadisoana

history-search = Hikaroka tantara
history-clear-all = Fafao daholo
history-clear-confirm = Fafana daholo ny tantara?
history-clear-warning = Tsy azo averina izany.
history-cancel = Foano
history-today = Androany
history-yesterday = Omaly
history-days-ago = { $count } andro lasa
history-day-offset = Andro -{ $count }

settings-title = Fikirana
settings-loading = Maka fikirana…
settings-stored = Voatahiry ao amin’ny ~/.vmux/settings.ron
settings-other = Hafa
settings-software-update = Fanavaozana rindranasa
settings-check-updates = Jereo raha misy fanavaozana
settings-check-updates-hint = Hamarina ho azy rehefa manomboka ary isan’ora raha alefa ny Auto-update.
settings-update-unavailable = Tsy misy
settings-update-unavailable-hint = Tsy tafiditra ato amin’ity build ity ny mpanavao.
settings-update-checking = Manamarina…
settings-update-checking-hint = Manamarina fanavaozana…
settings-update-check-again = Hamarino indray
settings-update-current = Efa vaovao ny Vmux.
settings-update-downloading = Misintona…
settings-update-downloading-hint = Misintona Vmux { $version }…
settings-update-installing = Mametraka…
settings-update-installing-hint = Mametraka Vmux { $version }…
settings-update-ready = Vonona ny fanavaozana
settings-update-ready-hint = Vonona ny Vmux { $version }. Avereno alefa hampiharana azy.
settings-update-try-again = Andramo indray
settings-update-failed = Tsy afaka nanamarina fanavaozana.
settings-item = Singa
settings-item-number = Singa { $number }
settings-press-key = Tsindrio bokotra…
settings-saved = Voatahiry
settings-record-key = Tsindrio handraketana fitambaran-bokotra vaovao

tray-open-window = Sokafy varavarankely
tray-close-window = Akatony varavarankely
tray-pause-recording = Atsaharo vetivety ny firaketana
tray-resume-recording = Tohizo ny firaketana
tray-finish-recording = Farano ny firaketana
tray-quit = Hiala amin’ny Vmux

composer-attach-files = Ampidiro rakitra (/upload)
composer-remove-attachment = Esory ny rakitra nampidirina

layout-back = Miverina
layout-forward = Mandroso
layout-reload = Avereno alaina
layout-bookmark-page = Asio maripejy ity pejy ity
layout-remove-bookmark = Esory ny maripejy
layout-pin-page = Afatory ity pejy ity
layout-unpin-page = Esory fatorana ity pejy ity
layout-manage-extensions = Tantano ny fanitarana
layout-new-stack = Sosona vaovao
layout-close-tab = Akatony kiheba
layout-bookmark = Maripejy
layout-pin = Afatory
layout-new-tab = Kiheba vaovao
layout-team = Ekipa

command-switch-space = Ovay sehatra…
command-search-ask = Hikaroka na hanontany…
command-new-tab-placeholder = Hikaroka na manorata URL, na fidio Terminal…
command-placeholder = Manorata URL, hikaroka kiheba, na > ho an’ny baiko…
command-composer-placeholder = Manorata / ho an’ny baiko na @ ho an’ny haino aman-jery
command-send = Alefaso (Enter)
command-terminal = Terminal
command-open-terminal = Sokafy ao amin’ny Terminal
command-stack = Sosona
command-tabs = { $count ->
    [one] kiheba 1
   *[other] kiheba { $count }
}
command-prompt = Prompt
command-new-tab = Kiheba vaovao
command-search = Hikaroka
command-open-value = Sokafy “{ $value }”
command-search-value = Hikaroka “{ $value }”

schema-appearance = Endrika
schema-general = Ankapobeny
schema-layout = Fandaminana
schema-layout-detail = Varavarankely, tontonana, sisiny, ary peratra fifantohana.
schema-agent = Agent
schema-agent-detail = Fitondran’ny agent sy fahazoan-dàlana hampiasa fitaovana.
schema-shortcuts = Hitsin-dalana
schema-shortcuts-detail = Fijery vakiana fotsiny. Ovay mivantana ny settings.ron raha hanova bindings.
schema-terminal = Terminal
schema-browser = Mpizaha
schema-mode = Fomba
schema-mode-detail = Teti-loko ho an’ny pejy web. Device dia manaraka ny rafitrao.
schema-device = Device
schema-light = Mazava
schema-dark = Maizina
schema-language = Fiteny
schema-language-detail = Ampiasao ny an’ny rafitra, en-US, ja, na marika BCP 47 misy katalaogy ~/.vmux/locales/<tag>.ftl mifanaraka.
schema-auto-update = Auto-update
schema-auto-update-detail = Jereo sy apetraho ny fanavaozana rehefa manomboka ary isan’ora.
schema-startup-url = URL fanombohana
schema-startup-url-detail = Raha banga dia manokatra ny prompt amin’ny bara baiko.
schema-search-engine = Motera fikarohana
schema-search-engine-detail = Ampiasaina amin’ny fikarohana web avy amin’ny Fanombohana sy ny bara baiko.
schema-window = Varavarankely
schema-pane = Tontonana
schema-side-sheet = Takelaka sisiny
schema-focus-ring = Peratra fifantohana
schema-run-placement = Avelao hovaina ny fametrahana run
schema-run-placement-detail = Avelao ny agent hifidy fomba tontonana run, làlana, ary vatofantsika.
schema-leader = Leader
schema-leader-detail = Bokotra mialoha ho an’ny hitsin-dalana chord.
schema-chord-timeout = Fe-potoana chord
schema-chord-timeout-detail = Milisegondra alohan’ny hahataperan’ny prefix chord.
schema-bindings = Bindings
schema-confirm-close = Hamafiso alohan’ny hanakatonana
schema-confirm-close-detail = Anontanio alohan’ny hanakatonana terminal misy dingana mbola mandeha.
schema-default-theme = Lohahevitra mahazatra
schema-default-theme-detail = Anaran’ny lohahevitra mavitrika ao amin’ny lisitry ny lohahevitra.
