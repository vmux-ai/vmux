common-open = Maak oop
common-close = Maak toe
common-install = Installeer
common-uninstall = Deïnstalleer
common-update = Dateer op
common-retry = Probeer weer
common-refresh = Verfris
common-remove = Verwyder
common-enable = Aktiveer
common-disable = Deaktiveer
common-new = Nuut
common-active = aktief
common-running = hardloop
common-done = gedoen
common-failed = Misluk
common-installed = Geïnstalleer
common-items = { $count ->
    [one] { $count } item
   *[other] { $count } items
}
start-title = Begin
start-tagline = Een opdrag. Enigiets, klaar.

agents-title = Agente
agents-search = Soek ACP en CLI agente …
agents-empty = Geen bypassende agente nie
agents-empty-detail = Probeer 'n naam, looptyd of ACP/CLI.
agents-install-failed = Installering het misluk
agents-updating = Dateer tans op …
agents-retrying = Probeer tans weer …
agents-preparing = Berei tans voor …

extensions-title = Uitbreidings
extensions-search = Soek geïnstalleer of Chrome Web Store...
extensions-relaunch = Herbegin om aansoek te doen
extensions-empty = Geen uitbreidings geïnstalleer nie
extensions-no-match = Geen bypassende uitbreidings nie
extensions-empty-detail = Soek die Chrome Web Store hierbo en druk Return.
extensions-no-match-detail = Probeer 'n ander naam of uitbreiding ID.
extensions-on = Aan
extensions-off = Af
extensions-enable-confirm = Aktiveer { $name }?
extensions-enable-permissions = Aktiveer { $name } en laat toe:

lsp-title = Taalbedieners
lsp-search = Soek taalbedieners, linters, formateerders …
lsp-loading = Laai tans katalogus …
lsp-empty = Geen ooreenstemmende taalbedieners nie
lsp-empty-detail = Probeer 'n ander taal, linter of formateerder.
lsp-needs = benodig { $tool }
lsp-status-available = Beskikbaar
lsp-status-on-path = Op PATH
lsp-status-installing = Installeer tans …
lsp-status-installed = Geïnstalleer
lsp-status-outdated = Opdatering beskikbaar
lsp-status-running = Hardloop
lsp-status-failed = Misluk

spaces-title = Spasies
spaces-new-placeholder = Nuwe spasienaam
spaces-empty = Geen spasies nie
spaces-default-name = Spasie { $number }
spaces-tabs = { $count ->
    [one] 1 oortjie
   *[other] { $count } oortjies
}
spaces-delete = Vee spasie uit

team-title = Span
team-just-you = Net jy in hierdie ruimte
team-agents = { $count ->
    [one] Jy en 1 agent
   *[other] Jy en { $count } agente
}
team-empty = Nog niemand hier nie
team-you = Jy
team-agent = Agent

services-title = Agtergrond Dienste
services-processes = { $count ->
    [one] 1 proses
   *[other] { $count } prosesse
}
services-kill-all = Maak almal dood
services-not-running = Diens loop nie
services-start-with = Begin met:
services-empty = Geen aktiewe prosesse nie
services-filter = Filtreer prosesse …
services-no-match = Geen bypassende prosesse nie
services-connected = Gekoppel
services-disconnected = Ontkoppel
services-attached = aangeheg
services-kill = Doodmaak
services-memory = Geheue
services-size = Grootte
services-shell = Skulp

error-title = Fout

history-search = Soek geskiedenis
history-clear-all = Vee alles uit
history-clear-confirm = Vee alle geskiedenis uit?
history-clear-warning = Dit kan nie ongedaan gemaak word nie.
history-cancel = Kanselleer
history-today = Vandag
history-yesterday = Gister
history-days-ago = { $count } dae gelede
history-day-offset = Dag -{ $count }

settings-title = Instellings
settings-loading = Laai tans instellings …
settings-stored = Geberg in ~/.vmux/settings.ron
settings-other = Ander
settings-software-update = Sagteware-opdatering
settings-check-updates = Kyk vir opdaterings
settings-check-updates-hint = Kontroleer outomaties by bekendstelling en elke uur wanneer Outo-opdatering geaktiveer is.
settings-update-unavailable = Onbeskikbaar
settings-update-unavailable-hint = Updater is nie by hierdie gebou ingesluit nie.
settings-update-checking = Kontroleer tans …
settings-update-checking-hint = Kyk tans vir opdaterings …
settings-update-check-again = Kyk weer
settings-update-current = Vmux is op datum.
settings-update-downloading = Laai tans af …
settings-update-downloading-hint = Laai tans Vmux { $version } af...
settings-update-installing = Installeer tans …
settings-update-installing-hint = Installeer tans Vmux { $version }...
settings-update-ready = Opdatering gereed
settings-update-ready-hint = Vmux { $version } is gereed. Herbegin om dit toe te pas.
settings-update-try-again = Probeer weer
settings-update-failed = Kan nie kyk vir opdaterings nie.
settings-item = Item
settings-item-number = Item { $number }
settings-press-key = Druk 'n sleutel...
settings-saved = Gestoor
settings-record-key = Klik om 'n nuwe sleutelkombinasie op te neem

tray-open-window = Maak venster oop
tray-close-window = Maak venster toe
tray-pause-recording = Onderbreek opname
tray-resume-recording = Hervat opname
tray-finish-recording = Voltooi opname
tray-quit = Verlaat Vmux

composer-attach-files = Heg lêers aan (/upload)
composer-remove-attachment = Verwyder aanhegsel

layout-back = Terug
layout-forward = Vorentoe
layout-reload = Herlaai
layout-bookmark-page = Boekmerk hierdie bladsy
layout-remove-bookmark = Verwyder boekmerk
layout-pin-page = Speld hierdie bladsy vas
layout-unpin-page = Ontspeld hierdie bladsy
layout-manage-extensions = Bestuur uitbreidings
layout-new-stack = Nuwe stapel
layout-close-tab = Maak oortjie toe
layout-bookmark = Boekmerk
layout-pin = Speld vas
layout-new-tab = Nuwe oortjie
layout-team = Span

command-switch-space = Wissel spasie …
command-search-ask = Soek of vra …
command-new-tab-placeholder = Soek of tik 'n URL, of kies Terminal...
command-placeholder = Tik 'n URL, soekoortjies of > vir opdragte...
command-composer-placeholder = Tik / vir opdragte of @ vir media
command-send = Stuur (Enter)
command-terminal = Terminale
command-open-terminal = Maak oop in Terminal
command-stack = Stapel
command-tabs = { $count ->
    [one] 1 oortjie
   *[other] { $count } oortjies
}
command-prompt = Prompt
command-new-tab = Nuwe oortjie
command-search = Soek
command-open-value = Maak "{ $value }" oop
command-search-value = Soek "{ $value }"

schema-appearance = Voorkoms
schema-general = Algemeen
schema-layout = Uitleg
schema-layout-detail = Venster, ruite, sybalk en fokusring.
schema-agent = Agent
schema-agent-detail = Agentgedrag en nutsgoedtoestemmings.
schema-shortcuts = Kortpaaie
schema-shortcuts-detail = Leesalleen-aansig. Wysig settings.ron direk om bindings te verander.
schema-terminal = Terminale
schema-browser = Blaaier
schema-mode = Modus
schema-mode-detail = Kleurskema vir webblaaie. Toestel volg jou stelsel.
schema-device = Toestel
schema-light = Lig
schema-dark = Donker
schema-language = Taal
schema-language-detail = Gebruik stelsel, en-US, ja, of enige BCP 47 merker met 'n bypassende ~/.vmux/locales/<tag>.ftl katalogus.
schema-auto-update = Outo-opdatering
schema-auto-update-detail = Kyk vir en installeer opdaterings by bekendstelling en elke uur.
schema-startup-url = Begin URL
schema-startup-url-detail = Leeg maak die opdragbalkprompt oop.
schema-search-engine = Soekenjin
schema-search-engine-detail = Word gebruik vir websoektogte vanaf Start en die opdragbalk.
schema-window = Venster
schema-pane = Paneel
schema-side-sheet = Syblad
schema-focus-ring = Fokusring
schema-run-placement = Laat loopplasing ignoreer toe
schema-run-placement-detail = Laat agente hardloopvenstermodus, rigting en anker kies.
schema-leader = Leier
schema-leader-detail = Voorvoegselsleutel vir akkoordkortpaaie.
schema-chord-timeout = Akkoord uitteltyd
schema-chord-timeout-detail = Millisekondes voordat 'n akkoordvoorvoegsel verval.
schema-bindings = Bindings
schema-confirm-close = Bevestig sluiting
schema-confirm-close-detail = Vra voor die sluiting van 'n terminaal met 'n lopende proses.
schema-default-theme = Verstek tema
schema-default-theme-detail = Naam van die aktiewe tema uit die temalys.
