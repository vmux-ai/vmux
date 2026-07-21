locale-name = Afrikaans
common-open = Maak oop
common-close = Sluit
common-install = Installeer
common-uninstall = Deïnstalleer
common-update = Werk by
common-retry = Probeer weer
common-refresh = Verfris
common-remove = Verwyder
common-enable = Aktiveer
common-disable = Deaktiveer
common-new = Nuut
common-active = aktief
common-running = loop
common-done = klaar
common-failed = Misluk
common-installed = Geïnstalleer
common-items = { $count ->
    [one] { $count } item
   *[other] { $count } items
}
start-title = Begin
start-tagline = Een prompt. Enigiets, klaar.

agents-title = Agente
agents-search = Soek ACP- en CLI-agente…
agents-empty = Geen ooreenstemmende agente nie
agents-empty-detail = Probeer ’n naam, looptyd, of ACP/CLI.
agents-install-failed = Installering het misluk
agents-updating = Werk by…
agents-retrying = Probeer weer…
agents-preparing = Berei voor…

extensions-title = Uitbreidings
extensions-search = Soek geïnstalleerde uitbreidings of in die Chrome Web Store…
extensions-relaunch = Herbegin om toe te pas
extensions-empty = Geen uitbreidings geïnstalleer nie
extensions-no-match = Geen ooreenstemmende uitbreidings nie
extensions-empty-detail = Soek hierbo in die Chrome Web Store en druk Enter.
extensions-no-match-detail = Probeer ’n ander naam of uitbreidings-ID.
extensions-on = Aan
extensions-off = Af
extensions-enable-confirm = Aktiveer { $name }?
extensions-enable-permissions = Aktiveer { $name } en laat toe:

lsp-title = Taalbedieners
lsp-search = Soek taalbedieners, linters, formatteerders…
lsp-loading = Laai katalogus…
lsp-empty = Geen ooreenstemmende taalbedieners nie
lsp-empty-detail = Probeer ’n ander taal, linter of formatteerder.
lsp-needs = benodig { $tool }
lsp-status-available = Beskikbaar
lsp-status-on-path = Op PATH
lsp-status-installing = Installeer…
lsp-status-installed = Geïnstalleer
lsp-status-outdated = Bywerking beskikbaar
lsp-status-running = Loop
lsp-status-failed = Misluk

spaces-title = Werkruimtes
spaces-new-placeholder = Nuwe werkruimtenaam
spaces-empty = Geen werkruimtes nie
spaces-default-name = Werkruimte { $number }
spaces-tabs = { $count ->
    [one] 1 oortjie
   *[other] { $count } oortjies
}
spaces-delete = Skrap werkruimte

team-title = Span
team-just-you = Net jy in hierdie werkruimte
team-agents = { $count ->
    [one] Jy en 1 agent
   *[other] Jy en { $count } agente
}
team-empty = Nog niemand hier nie
team-you = Jy
team-agent = Agent

services-title = Agtergronddienste
services-processes = { $count ->
    [one] 1 proses
   *[other] { $count } prosesse
}
services-kill-all = Beëindig almal
services-not-running = Diens loop nie
services-start-with = Begin met:
services-empty = Geen aktiewe prosesse nie
services-filter = Filter prosesse…
services-no-match = Geen ooreenstemmende prosesse nie
services-connected = Gekoppel
services-disconnected = Ontkoppel
services-attached = aangeheg
services-kill = Beëindig
services-memory = Geheue
services-size = Grootte
services-shell = Dop

error-title = Fout

history-search = Soek geskiedenis
history-clear-all = Maak alles skoon
history-clear-confirm = Maak alle geskiedenis skoon?
history-clear-warning = Dit kan nie ongedaan gemaak word nie.
history-cancel = Kanselleer
history-today = Vandag
history-yesterday = Gister
history-days-ago = { $count } dae gelede
history-day-offset = Dag -{ $count }

settings-title = Instellings
settings-loading = Laai instellings…
settings-stored = Gestoor in ~/.vmux/settings.ron
settings-other = Ander
settings-software-update = Sagtewarebywerking
settings-check-updates = Kyk vir bywerkings
settings-check-updates-hint = Kontroleer outomaties by aanvang en elke uur wanneer outobywerking geaktiveer is.
settings-update-unavailable = Nie beskikbaar nie
settings-update-unavailable-hint = Die bywerker is nie by hierdie bou ingesluit nie.
settings-update-checking = Kontroleer…
settings-update-checking-hint = Kontroleer vir bywerkings…
settings-update-check-again = Kontroleer weer
settings-update-current = Vmux is op datum.
settings-update-downloading = Laai af…
settings-update-downloading-hint = Laai Vmux { $version } af…
settings-update-installing = Installeer…
settings-update-installing-hint = Installeer Vmux { $version }…
settings-update-ready = Bywerking gereed
settings-update-ready-hint = Vmux { $version } is gereed. Herbegin om dit toe te pas.
settings-update-try-again = Probeer weer
settings-update-failed = Kan nie vir bywerkings kontroleer nie.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Druk ’n sleutel…
settings-saved = Gestoor
settings-record-key = Klik om ’n nuwe sleutelkombinasie op te neem

tray-open-window = Maak venster oop
tray-close-window = Sluit venster
tray-pause-recording = Onderbreek opname
tray-resume-recording = Hervat opname
tray-finish-recording = Voltooi opname
tray-quit = Sluit Vmux af

composer-attach-files = Heg lêers aan (/upload)
composer-remove-attachment = Verwyder aanhegsel

layout-back = Terug
layout-forward = Vorentoe
layout-reload = Herlaai
layout-bookmark-page = Voeg hierdie bladsy by boekmerke
layout-remove-bookmark = Verwyder boekmerk
layout-pin-page = Speld hierdie bladsy vas
layout-unpin-page = Maak hierdie bladsy los
layout-manage-extensions = Bestuur uitbreidings
layout-new-stack = Nuwe stapel
layout-close-tab = Sluit oortjie
layout-bookmark = Boekmerk
layout-pin = Speld vas
layout-new-tab = Nuwe oortjie
layout-team = Span

command-switch-space = Wissel werkruimte…
command-search-ask = Soek of vra…
command-new-tab-placeholder = Soek of tik ’n URL, of kies Terminaal…
command-placeholder = Tik ’n URL, soek oortjies, of > vir opdragte…
command-composer-placeholder = Tik / vir opdragte of @ vir media
command-send = Stuur (Enter)
command-terminal = Terminaal
command-open-terminal = Maak in Terminaal oop
command-stack = Stapel
command-tabs = { $count ->
    [one] 1 oortjie
   *[other] { $count } oortjies
}
command-prompt = Prompt
command-new-tab = Nuwe oortjie
command-search = Soek
command-open-value = Maak “{ $value }” oop
command-search-value = Soek “{ $value }”

schema-appearance = Voorkoms
schema-general = Algemeen
schema-layout = Uitleg
schema-layout-detail = Venster, panele, sybalk en fokusring.
schema-agent = Agent
schema-agent-detail = Agentgedrag en gereedskaptoestemmings.
schema-shortcuts = Kortpaaie
schema-shortcuts-detail = Leesalleen-aansig. Wysig settings.ron direk om bindings te verander.
schema-terminal = Terminaal
schema-browser = Blaaier
schema-mode = Modus
schema-mode-detail = Kleurskema vir webbladsye. Toestel volg jou stelsel.
schema-device = Toestel
schema-light = Lig
schema-dark = Donker
schema-language = Taal
schema-language-detail = Gebruik die stelsel, en-US, ja, of enige BCP 47-etiket met ’n ooreenstemmende ~/.vmux/locales/<tag>.ftl-katalogus.
schema-auto-update = Outobywerking
schema-auto-update-detail = Kontroleer vir en installeer bywerkings by aanvang en elke uur.
schema-startup-url = Aanvangs-URL
schema-startup-url-detail = Leeg maak die opdragbalk se prompt oop.
schema-search-engine = Soekenjin
schema-search-engine-detail = Gebruik vir websoektogte vanaf Begin en die opdragbalk.
schema-window = Venster
schema-pane = Paneel
schema-side-sheet = Syblad
schema-focus-ring = Fokusring
schema-run-placement = Laat loopplasing-oorskrywing toe
schema-run-placement-detail = Laat agente die looppaneelmodus, rigting en anker kies.
schema-leader = Leier
schema-leader-detail = Voorvoegsleutel vir akkoordkortpaaie.
schema-chord-timeout = Akkoord-uitteltyd
schema-chord-timeout-detail = Millisekondes voordat ’n akkoordvoorvoegsel verval.
schema-bindings = Bindings
schema-confirm-close = Bevestig sluiting
schema-confirm-close-detail = Vra voordat ’n terminaal met ’n lopende proses gesluit word.
schema-default-theme = Verstektema
schema-default-theme-detail = Naam van die aktiewe tema uit die temalys.
