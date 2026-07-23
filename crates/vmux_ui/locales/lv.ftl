locale-name = latviešu
common-open = Atvērt
common-close = Aizvērt
common-install = Instalēt
common-uninstall = Atinstalēt
common-update = Atjaunināt
common-retry = Mēģināt vēlreiz
common-refresh = Atsvaidzināt
common-remove = Noņemt
common-enable = Ieslēgt
common-disable = Izslēgt
common-new = Jauns
common-active = aktīvs
common-running = darbojas
common-done = pabeigts
common-failed = Neizdevās
common-installed = Instalēts
common-items = { $count ->
    [one] { $count } vienums
   *[other] { $count } vienumi
}

tools-title = Rīki
tools-search = Meklēt pakotnes, aģentus, MCP, valodu rīkus un konfigurācijas failus…
tools-open = Atvērt rīkus
tools-fold = Sakļaut rīkus
tools-unfold = Izvērst rīkus
tools-scanning = Vietējo rīku skenēšana…
tools-no-installed = Nav instalētu rīku
tools-empty = Nav atbilstošu rīku
tools-empty-detail = Instalējiet pakotni vai pievienojiet Stow stila konfigurācijas failu pakotni.
tools-apply = Lietot
tools-homebrew = Homebrew
tools-homebrew-sync = Instalētās formulas un lietotnes tiek sinhronizētas automātiski.
tools-open-brewfile = Atvērt Brewfile
tools-managed = pārvaldīts
tools-provider-homebrew-formulae = Homebrew formulas
tools-provider-homebrew-casks = Homebrew lietotnes
tools-provider-npm = npm pakotnes
tools-provider-acp-agents = ACP aģenti
tools-provider-language-tools = Valodu rīki
tools-provider-mcp-servers = MCP serveri
tools-provider-dotfiles = Konfigurācijas faili
tools-status-available = Pieejams
tools-status-missing = Trūkst
tools-status-conflict = Konflikts
tools-forget = Aizmirst
tools-manage = Pārvaldīt
tools-link = Saistīt
tools-unlink = Atsaistīt
tools-import = Importēt
tools-update-count = { $count ->
    [one] 1 atjauninājums
   *[other] { $count } atjauninājumi
}
tools-conflict-count = { $count ->
    [one] 1 konflikts
   *[other] { $count } konflikti
}
tools-result-applied = Rīki lietoti
tools-result-imported = Rīki importēti
tools-result-installed = { $name } instalēts
tools-result-updated = { $name } atjaunināts
tools-result-uninstalled = { $name } atinstalēts
tools-result-forgotten = { $name } aizmirsts
tools-result-managed = { $name } tagad tiek pārvaldīts
tools-result-linked = { $name } saistīts
tools-result-unlinked = { $name } atsaistīts

start-title = Sākums
start-tagline = Viens prompts. Viss izdarīts.

agents-title = Aģenti
agents-search = Meklēt ACP un CLI aģentus…
agents-empty = Nav atbilstošu aģentu
agents-empty-detail = Mēģiniet meklēt pēc nosaukuma, izpildvides vai ACP/CLI.
agents-install-failed = Instalēšana neizdevās
agents-updating = Atjaunina…
agents-retrying = Mēģina vēlreiz…
agents-preparing = Sagatavo…

extensions-title = Paplašinājumi
extensions-search = Meklēt instalētajos vai Chrome Web Store…
extensions-relaunch = Palaidiet no jauna, lai lietotu izmaiņas
extensions-empty = Nav instalētu paplašinājumu
extensions-no-match = Nav atbilstošu paplašinājumu
extensions-empty-detail = Meklējiet Chrome Web Store augstāk un nospiediet Enter.
extensions-no-match-detail = Mēģiniet citu nosaukumu vai paplašinājuma ID.
extensions-on = Ieslēgts
extensions-off = Izslēgts
extensions-enable-confirm = Ieslēgt { $name }?
extensions-enable-permissions = Ieslēgt { $name } un atļaut:

lsp-title = Valodu serveri
lsp-search = Meklēt valodu serverus, linterus, formatētājus…
lsp-loading = Ielādē katalogu…
lsp-empty = Nav atbilstošu valodu serveru
lsp-empty-detail = Mēģiniet citu valodu, linteri vai formatētāju.
lsp-needs = nepieciešams { $tool }
lsp-status-available = Pieejams
lsp-status-on-path = PATH
lsp-status-installing = Instalē…
lsp-status-installed = Instalēts
lsp-status-outdated = Pieejams atjauninājums
lsp-status-running = Darbojas
lsp-status-failed = Neizdevās

spaces-title = Darbtelpas
spaces-new-placeholder = Jaunas darbtelpas nosaukums
spaces-empty = Nav darbtelpu
spaces-default-name = Darbtelpa { $number }
spaces-tabs = { $count ->
    [one] 1 cilne
   *[other] { $count } cilnes
}
spaces-delete = Dzēst darbtelpu

team-title = Komanda
team-just-you = Šajā darbtelpā esat tikai jūs
team-agents = { $count ->
    [one] Jūs un 1 aģents
   *[other] Jūs un { $count } aģenti
}
team-empty = Te vēl neviena nav
team-you = Jūs
team-agent = Aģents

services-title = Fona pakalpojumi
services-processes = { $count ->
    [one] 1 process
   *[other] { $count } procesi
}
services-kill-all = Piespiedu kārtā apturēt visus
services-not-running = Pakalpojums nedarbojas
services-start-with = Startēt ar:
services-empty = Nav aktīvu procesu
services-filter = Filtrēt procesus…
services-no-match = Nav atbilstošu procesu
services-connected = Savienots
services-disconnected = Atvienots
services-attached = piesaistīts
services-kill = Piespiedu kārtā apturēt
services-memory = Atmiņa
services-size = Izmērs
services-shell = Čaula

error-title = Kļūda

history-search = Meklēt vēsturē
history-clear-all = Notīrīt visu
history-clear-confirm = Notīrīt visu vēsturi?
history-clear-warning = Šo darbību nevar atsaukt.
history-cancel = Atcelt
history-today = Šodien
history-yesterday = Vakar
history-days-ago = pirms { $count } dienām
history-day-offset = Diena -{ $count }

settings-title = Iestatījumi
settings-loading = Ielādē iestatījumus…
settings-stored = Saglabāts ~/.vmux/settings.ron
settings-other = Citi
settings-software-update = Programmatūras atjauninājums
settings-check-updates = Pārbaudīt atjauninājumus
settings-check-updates-hint = Pārbauda automātiski palaišanas brīdī un reizi stundā, ja ir ieslēgta automātiskā atjaunināšana.
settings-update-unavailable = Nav pieejams
settings-update-unavailable-hint = Šajā būvējumā atjauninātājs nav iekļauts.
settings-update-checking = Pārbauda…
settings-update-checking-hint = Pārbauda atjauninājumus…
settings-update-check-again = Pārbaudīt vēlreiz
settings-update-current = Vmux ir atjaunināts.
settings-update-downloading = Lejupielādē…
settings-update-downloading-hint = Lejupielādē Vmux { $version }…
settings-update-installing = Instalē…
settings-update-installing-hint = Instalē Vmux { $version }…
settings-update-ready = Atjauninājums gatavs
settings-update-ready-hint = Vmux { $version } ir gatavs. Restartējiet, lai lietotu atjauninājumu.
settings-update-try-again = Mēģināt vēlreiz
settings-update-failed = Neizdevās pārbaudīt atjauninājumus.
settings-item = Vienums
settings-item-number = Vienums { $number }
settings-press-key = Nospiediet taustiņu…
settings-saved = Saglabāts
settings-record-key = Noklikšķiniet, lai ierakstītu jaunu taustiņu kombināciju

tray-open-window = Atvērt logu
tray-close-window = Aizvērt logu
tray-pause-recording = Pauzēt ierakstīšanu
tray-resume-recording = Atsākt ierakstīšanu
tray-finish-recording = Pabeigt ierakstīšanu
tray-quit = Iziet no Vmux

composer-attach-files = Pievienot failus (/upload)
composer-remove-attachment = Noņemt pielikumu

layout-back = Atpakaļ
layout-forward = Uz priekšu
layout-reload = Pārlādēt
layout-bookmark-page = Pievienot šo lapu grāmatzīmēm
layout-remove-bookmark = Noņemt grāmatzīmi
layout-pin-page = Piespraust šo lapu
layout-unpin-page = Atspraust šo lapu
layout-manage-extensions = Pārvaldīt paplašinājumus
layout-new-stack = Jauns slānis
layout-close-tab = Aizvērt cilni
layout-bookmark = Grāmatzīme
layout-pin = Piespraust
layout-new-tab = Jauna cilne
layout-team = Komanda

command-switch-space = Pārslēgt darbtelpu…
command-search-ask = Meklēt vai jautāt…
command-new-tab-placeholder = Meklējiet, ievadiet URL vai izvēlieties termināli…
command-placeholder = Ievadiet URL, meklējiet cilnes vai izmantojiet > komandām…
command-composer-placeholder = Ievadiet / komandām vai @ multividei
command-send = Sūtīt (Enter)
command-terminal = Terminālis
command-open-terminal = Atvērt terminālī
command-stack = Slānis
command-tabs = { $count ->
    [one] 1 cilne
   *[other] { $count } cilnes
}
command-prompt = Prompts
command-new-tab = Jauna cilne
command-search = Meklēt
command-open-value = Atvērt “{ $value }”
command-search-value = Meklēt “{ $value }”

schema-appearance = Izskats
schema-general = Vispārīgi
schema-layout = Izkārtojums
schema-layout-detail = Logs, rūtis, sānu josla un fokusa apmale.
schema-agent = Aģents
schema-agent-detail = Aģenta darbība un rīku atļaujas.
schema-shortcuts = Saīsnes
schema-shortcuts-detail = Tikai skatīšanai. Lai mainītu piesaistes, rediģējiet settings.ron tieši.
schema-terminal = Terminālis
schema-browser = Pārlūks
schema-mode = Režīms
schema-mode-detail = Tīmekļa lapu krāsu shēma. Ierīces režīms seko sistēmai.
schema-device = Ierīce
schema-light = Gaišs
schema-dark = Tumšs
schema-language = Valoda
schema-language-detail = Izmantojiet sistēmas valodu, en-US, ja vai jebkuru BCP 47 tagu ar atbilstošu ~/.vmux/locales/<tag>.ftl katalogu.
schema-auto-update = Automātiskā atjaunināšana
schema-auto-update-detail = Pārbaudīt un instalēt atjauninājumus palaišanas brīdī un reizi stundā.
schema-startup-url = Sākuma URL
schema-startup-url-detail = Ja tukšs, tiek atvērta komandu joslas uzvedne.
schema-search-engine = Meklētājprogramma
schema-search-engine-detail = Izmanto tīmekļa meklēšanai no sākuma skata un komandu joslas.
schema-window = Logs
schema-pane = Rūts
schema-side-sheet = Sānu panelis
schema-focus-ring = Fokusa apmale
schema-run-placement = Atļaut izpildes izvietojuma pārrakstīšanu
schema-run-placement-detail = Ļaut aģentiem izvēlēties izpildes rūts režīmu, virzienu un enkuru.
schema-leader = Līderis
schema-leader-detail = Prefiksa taustiņš akordu saīsnēm.
schema-chord-timeout = Akorda noildze
schema-chord-timeout-detail = Milisekundes, līdz akorda prefikss beidzas.
schema-bindings = Piesaistes
schema-confirm-close = Apstiprināt aizvēršanu
schema-confirm-close-detail = Vaicāt pirms termināļa aizvēršanas, ja tajā darbojas process.
schema-default-theme = Noklusējuma motīvs
schema-default-theme-detail = Aktīvā motīva nosaukums no motīvu saraksta.

settings-empty = (tukšs)
settings-none = (nav)

schema-system = Sistēma
schema-editor = Redaktors
schema-recording = Ierakstīšana
schema-radius = Rādiuss
schema-padding = Atstarpe
schema-gap = Sprauga
schema-width = Platums
schema-color = Krāsa
schema-red = Sarkans
schema-green = Zaļš
schema-blue = Zils
schema-follow-files = Sekot failiem
schema-tidy-files = Sakopt failus
schema-tidy-files-max = Failu sakopšanas slieksnis
schema-tidy-files-auto = Automātiski sakopt failus
schema-app-providers = Lietotņu nodrošinātāji
schema-provider = Nodrošinātājs
schema-kind = Veids
schema-models = Modeļi
schema-acp = ACP aģenti
schema-id = ID
schema-name = Nosaukums
schema-command = Komanda
schema-arguments = Argumenti
schema-environment = Vides mainīgie
schema-working-directory = Darba direktorijs
schema-shell = Čaula
schema-font-family = Fontu saime
schema-startup-directory = Sākuma direktorijs
schema-themes = Motīvi
schema-color-scheme = Krāsu shēma
schema-font-size = Fonta lielums
schema-line-height = Rindas augstums
schema-cursor-style = Kursora stils
schema-cursor-blink = Kursora mirgošana
schema-custom-themes = Pielāgoti motīvi
schema-foreground = Priekšplāns
schema-background = Fons
schema-cursor = Kursors
schema-ansi-colors = ANSI krāsas
schema-keymap = Taustiņu kartējums
schema-explorer = Pārlūks
schema-visible = Redzams
schema-language-servers = Valodu serveri
schema-servers = Serveri
schema-language-id = Valodas ID
schema-root-markers = Saknes marķieri
schema-output-directory = Izvades direktorijs

menu-scene = Aina
menu-layout = Izkārtojums
menu-terminal = Terminālis
menu-browser = Pārlūks
menu-service = Pakalpojums
menu-bookmark = Grāmatzīme
menu-edit = Rediģēt

layout-knowledge = Zināšanas
layout-open-knowledge = Atvērt Zināšanas
layout-open-welcome-knowledge = Atvērt “Laipni lūdzam Zināšanās”
layout-open-path = Atvērt { $path }
layout-fold-knowledge = Sakļaut zināšanas
layout-unfold-knowledge = Izvērst zināšanas
layout-bookmarks = Grāmatzīmes
layout-new-folder = Jauna mape
layout-add-to-bookmarks = Pievienot grāmatzīmēm
layout-move-to-bookmarks = Pārvietot uz grāmatzīmēm
layout-stack-number = Steks { $number }
layout-fold-stack = Sakļaut steku
layout-unfold-stack = Izvērst steku
layout-close-stack = Aizvērt steku
layout-bookmark-in = Grāmatzīme mapē { $folder }

common-cancel = Atcelt
common-delete = Dzēst
common-save = Saglabāt
common-rename = Pārdēvēt
common-expand = Izvērst
common-collapse = Sakļaut
common-loading = Ielādē…
common-error = Kļūda
common-output = Izvade
common-pending = Gaida
common-current = pašreizējais
common-stop = Apturēt
services-command = Vmux pakalpojums
services-uptime-seconds = { $seconds } s
services-uptime-minutes = { $minutes } min { $seconds } s
services-uptime-hours = { $hours } h { $minutes } min
services-uptime-days = { $days } d { $hours } h

error-page-failed-load = Neizdevās ielādēt lapu
error-page-not-found = Lapa nav atrasta
error-unknown-host = Nezināms Vmux lietotnes resursdators: { $host }

history-title = Vēsture

command-new-app-chat = Jauna { $provider }/{ $model } tērzēšana (lietotne)
command-interactive-mode-user = Aina > Interaktīvais režīms > Lietotājs
command-interactive-mode-player = Aina > Interaktīvais režīms > Atskaņotājs
command-minimize-window = Izkārtojums > Logs > Minimizēt
command-toggle-layout = Izkārtojums > Izkārtojums > Pārslēgt izkārtojumu
command-close-tab = Izkārtojums > Cilne > Aizvērt cilni
command-new-task = Izkārtojums > Cilne > Jauns uzdevums…
command-next-tab = Izkārtojums > Cilne > Nākamā cilne
command-prev-tab = Izkārtojums > Cilne > Iepriekšējā cilne
command-rename-tab = Izkārtojums > Cilne > Pārdēvēt cilni
command-tab-select-1 = Izkārtojums > Cilne > Atlasīt 1. cilni
command-tab-select-2 = Izkārtojums > Cilne > Atlasīt 2. cilni
command-tab-select-3 = Izkārtojums > Cilne > Atlasīt 3. cilni
command-tab-select-4 = Izkārtojums > Cilne > Atlasīt 4. cilni
command-tab-select-5 = Izkārtojums > Cilne > Atlasīt 5. cilni
command-tab-select-6 = Izkārtojums > Cilne > Atlasīt 6. cilni
command-tab-select-7 = Izkārtojums > Cilne > Atlasīt 7. cilni
command-tab-select-8 = Izkārtojums > Cilne > Atlasīt 8. cilni
command-tab-select-last = Izkārtojums > Cilne > Atlasīt pēdējo cilni
command-close-pane = Izkārtojums > Rūts > Aizvērt rūti
command-select-pane-left = Izkārtojums > Rūts > Atlasīt kreiso rūti
command-select-pane-right = Izkārtojums > Rūts > Atlasīt labo rūti
command-select-pane-up = Izkārtojums > Rūts > Atlasīt augšējo rūti
command-select-pane-down = Izkārtojums > Rūts > Atlasīt apakšējo rūti
command-swap-pane-prev = Izkārtojums > Rūts > Samainīt ar iepriekšējo rūti
command-swap-pane-next = Izkārtojums > Rūts > Samainīt ar nākamo rūti
command-equalize-pane-size = Izkārtojums > Rūts > Izlīdzināt rūšu izmērus
command-resize-pane-left = Izkārtojums > Rūts > Mainīt rūts izmēru pa kreisi
command-resize-pane-right = Izkārtojums > Rūts > Mainīt rūts izmēru pa labi
command-resize-pane-up = Izkārtojums > Rūts > Mainīt rūts izmēru uz augšu
command-resize-pane-down = Izkārtojums > Rūts > Mainīt rūts izmēru uz leju
command-stack-close = Izkārtojums > Steks > Aizvērt steku
command-stack-next = Izkārtojums > Steks > Nākamais steks
command-stack-previous = Izkārtojums > Steks > Iepriekšējais steks
command-stack-reopen = Izkārtojums > Steks > Atvērt aizvērto lapu vēlreiz
command-stack-swap-prev = Izkārtojums > Steks > Pārvietot steku pa kreisi
command-stack-swap-next = Izkārtojums > Steks > Pārvietot steku pa labi
command-space-open = Izkārtojums > Telpa > Telpas
command-terminal-close = Terminālis > Aizvērt termināli
command-terminal-next = Terminālis > Nākamais terminālis
command-terminal-prev = Terminālis > Iepriekšējais terminālis
command-terminal-clear = Terminālis > Notīrīt termināli
command-browser-prev-page = Pārlūks > Navigācija > Atpakaļ
command-browser-next-page = Pārlūks > Navigācija > Uz priekšu
command-browser-reload = Pārlūks > Navigācija > Pārlādēt
command-browser-hard-reload = Pārlūks > Navigācija > Pilnībā pārlādēt
command-open-in-place = Pārlūks > Atvērt > Atvērt šeit
command-open-in-new-stack = Pārlūks > Atvērt > Atvērt jaunā stekā
command-open-in-pane-top = Pārlūks > Atvērt > Atvērt rūtī augšā
command-open-in-pane-right = Pārlūks > Atvērt > Atvērt rūtī pa labi
command-open-in-pane-bottom = Pārlūks > Atvērt > Atvērt rūtī apakšā
command-open-in-pane-left = Pārlūks > Atvērt > Atvērt rūtī pa kreisi
command-open-in-new-tab = Pārlūks > Atvērt > Atvērt jaunā cilnē
command-open-in-new-space = Pārlūks > Atvērt > Atvērt jaunā telpā
command-browser-zoom-in = Pārlūks > Skats > Tuvināt
command-browser-zoom-out = Pārlūks > Skats > Tālināt
command-browser-zoom-reset = Pārlūks > Skats > Faktiskais izmērs
command-browser-dev-tools = Pārlūks > Skats > Izstrādātāja rīki
command-browser-open-command-bar = Pārlūks > Josla > Komandu josla
command-browser-open-page-in-command-bar = Pārlūks > Josla > Rediģēt lapu
command-browser-open-path-bar = Pārlūks > Josla > Ceļa navigators
command-browser-open-commands = Pārlūks > Josla > Komandas
command-browser-open-history = Pārlūks > Josla > Vēsture
command-service-open = Pakalpojums > Atvērt pakalpojumu pārraugu
command-bookmark-toggle-active = Grāmatzīme > Pievienot lapu grāmatzīmēm
command-bookmark-pin-active = Grāmatzīme > Piespraust lapu

layout-tab = Cilne
layout-no-stacks = Nav steku
layout-loading = Ielādē…
layout-no-markdown-files = Nav Markdown failu
layout-empty-folder = Tukša mape
layout-worktree = darba koks
layout-folder-name = Mapes nosaukums
layout-no-pins-bookmarks = Nav piesprausto lapu vai grāmatzīmju
layout-move-to = Pārvietot uz { $folder }
layout-bookmark-current-page = Pievienot pašreizējo lapu grāmatzīmēm
layout-rename-folder = Pārdēvēt mapi
layout-remove-folder = Noņemt mapi
layout-update-downloading = Lejupielādē atjauninājumu
layout-update-installing = Instalē atjauninājumu…
layout-update-ready = Pieejama jauna versija
layout-restart-update = Restartēt, lai atjauninātu

agent-preparing = Sagatavo aģentu…
agent-send-all-queued = Nosūtīt visas rindā esošās uzvednes tagad (Esc)
agent-send = Nosūtīt (Enter)
agent-ready = Gatavs, kad esat gatavs.
agent-loading-older = Ielādē vecākus ziņojumus…
agent-load-older = Ielādēt vecākus ziņojumus
agent-continued-from = Turpināts no { $source }
agent-older-context-omitted = vecāks konteksts izlaists
agent-interrupted = pārtraukts
agent-allow-tool = Atļaut { $tool }?
agent-deny = Noraidīt
agent-allow-always = Vienmēr atļaut
agent-allow = Atļaut
agent-loading-sessions = Ielādē sesijas…
agent-no-resumable-sessions = Nav atrastu atsākamu sesiju
agent-no-matching-sessions = Nav atbilstošu sesiju
agent-no-matching-models = Nav atbilstošu modeļu
agent-choice-help = ↑/↓ vai Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Izvēlieties repozitorija mapi
agent-choose-repository-detail = Atlasiet lokālo Git repozitoriju, ko aģentam izmantot.
agent-choosing = Izvēlas…
agent-choose-folder = Izvēlieties mapi
agent-queued = rindā
agent-attached = Pievienots:
agent-cancel-queued = Atcelt rindā esošo uzvedni
agent-resume-queued = Atsākt rindā esošās uzvednes
agent-clear-queue = Notīrīt rindu
agent-send-all-now = nosūtīt visu tagad
agent-choose-option = Izvēlieties opciju augstāk
agent-loading-media = Ielādē multividi…
agent-no-matching-media = Nav atbilstošas multivides
agent-prompt-context = Uzvednes konteksts
agent-details = Detalizēti
agent-path = Ceļš
agent-tool = Rīks
agent-server = Serveris
agent-bytes = { $count } baiti
agent-worked-for = Strādāja { $duration }
agent-worked-for-steps = { $count ->
    [one] Strādāja { $duration } · 1 solis
   *[other] Strādāja { $duration } · { $count } soļi
}
agent-tool-guardian-review = Guardian pārskatīšana
agent-tool-read-files = Lasīja failus
agent-tool-viewed-image = Skatīja attēlu
agent-tool-used-browser = Izmantoja pārlūku
agent-tool-searched-files = Meklēja failos
agent-tool-ran-commands = Izpildīja komandas
agent-thinking = Domā
agent-subagent = Apakšaģents
agent-prompt = Uzvedne
agent-thread = Pavediens
agent-parent = Vecāks
agent-children = Bērni
agent-call = Izsaukums
agent-raw-event = Neapstrādāts notikums
agent-plan = Plāns
agent-tasks = { $count ->
    [one] 1 uzdevums
   *[other] { $count } uzdevumi
}
agent-edited = Rediģēts
agent-reconnecting = Atkārtoti savieno { $attempt }/{ $total }
agent-status-running = Darbojas
agent-status-done = Gatavs
agent-status-failed = Neizdevās
agent-status-pending = Gaida
agent-slash-attach-files = Pievienot failus
agent-slash-resume-session = Atsākt iepriekšēju sesiju
agent-slash-select-model = Atlasīt modeli
agent-slash-continue-cli = Turpināt šo sesiju CLI
agent-session-just-now = tikko
agent-session-minutes-ago = pirms { $count } min
agent-session-hours-ago = pirms { $count } h
agent-session-days-ago = pirms { $count } d
agent-working-working = Strādā
agent-working-thinking = Domā
agent-working-pondering = Apdomā
agent-working-noodling = Prāto
agent-working-percolating = Nobriest
agent-working-conjuring = Uzbur
agent-working-cooking = Gatavo
agent-working-brewing = Brūvē
agent-working-musing = Pārdomā
agent-working-ruminating = Pārcilā
agent-working-scheming = Plāno
agent-working-synthesizing = Sintezē
agent-working-tinkering = Meistaro
agent-working-churning = Apstrādā
agent-working-vibing = Noskaņojas
agent-working-simmering = Vāra uz lēnas uguns
agent-working-crafting = Veido
agent-working-divining = Pareģo
agent-working-mulling = Apsver
agent-working-spelunking = Rok dziļāk

editor-toggle-explorer = Pārslēgt pārlūku (Cmd+B)
editor-unsaved = nesaglabāts
editor-rendered-markdown = Renderēts Markdown ar tiešo rediģēšanu
editor-note = Piezīme
editor-source-editor = Avota redaktors
editor-editor = Redaktors
editor-git-diff = Git atšķirības
editor-diff = Atšķirības
editor-tidy = Sakopt
editor-always = Vienmēr
editor-unchanged-previews = { $count ->
    [one] ✦ 1 nemainīts priekšskatījums
   *[other] ✦ { $count } nemainīti priekšskatījumi
}
editor-open-externally = Atvērt ārēji
editor-changed-line = Mainīta rinda
editor-go-to-definition = Pāriet uz definīciju
editor-find-references = Atrast atsauces
editor-references = { $count ->
    [one] 1 atsauce
   *[other] { $count } atsauces
}
editor-lsp-starting = { $server } startē…
editor-lsp-not-installed = { $server } — nav instalēts
editor-explorer = Pārlūks
editor-open-editors = Atvērtie redaktori
editor-outline = Struktūra
editor-new-file = Jauns fails
editor-new-folder = Jauna mape
editor-delete-confirm = Dzēst “{ $name }”? Šo darbību nevar atsaukt.
editor-created-folder = Izveidota mape { $name }
editor-created-file = Izveidots fails { $name }
editor-renamed-to = Pārdēvēts par { $name }
editor-deleted = Izdzēsts { $name }
editor-failed-decode-image = Neizdevās dekodēt attēlu
editor-preview-large-image = attēls (pārāk liels priekšskatījumam)
editor-preview-binary = binārs
editor-preview-file = fails

git-status-clean = tīrs
git-status-modified = modificēts
git-status-staged = sagatavots
git-status-staged-modified = sagatavots*
git-status-untracked = neizsekots
git-status-deleted = dzēsts
git-status-conflict = konflikts
git-accept-all = ✓ pieņemt visu
git-unstage = Noņemt no sagatavošanas
git-confirm-deny-all = Apstiprināt visa noraidīšanu
git-deny-all = ✗ noraidīt visu
git-commit-message = komita ziņojums
git-commit = Komitēt ({ $count })
git-push = ↑ Nosūtīt
git-loading-diff = Ielādē atšķirības…
git-no-changes = Nav izmaiņu, ko rādīt
git-accept = ✓ pieņemt
git-deny = ✗ noraidīt
git-show-unchanged-lines = Rādīt { $count } nemainītas rindas

terminal-loading = Ielādē…
terminal-runs-when-ready = palaidīs, kad gatavs · Ctrl+C notīra · Esc izlaiž
terminal-booting = startējas
terminal-type-command = ierakstiet komandu · palaidīs, kad gatavs · Esc izlaiž

setup-tagline-claude = Anthropic kodēšanas aģents Vmux vidē
setup-tagline-codex = OpenAI kodēšanas aģents Vmux vidē
setup-tagline-vibe = Mistral kodēšanas aģents Vmux vidē
setup-install-title = Instalēt { $name } CLI
setup-homebrew-required = Lai instalētu { $command }, ir nepieciešams Homebrew, un tas vēl nav iestatīts. Vmux vispirms instalēs Homebrew, pēc tam { $name }.
setup-terminal-instructions = Terminālī nospiediet Return, lai sāktu, pēc tam ievadiet sava Mac paroli, kad tā tiek prasīta.
setup-command-missing = Vmux atvēra šo lapu, jo lokālā komanda { $command } vēl nav instalēta. Palaidiet tālāk esošo komandu, lai to iegūtu.
setup-install-failed = Instalēšana netika pabeigta. Pārbaudiet termināli, lai uzzinātu vairāk, un mēģiniet vēlreiz.
setup-installing = Instalē…
setup-install-homebrew = Instalēt Homebrew + { $name }
setup-run-install = Palaist instalēšanas komandu
setup-auto-reload = Vmux palaiž to terminālī un pārlādē, kad { $command } ir gatavs.

debug-title = Atkļūdošana
debug-auto-update = Automātiska atjaunināšana
debug-simulate-update = Simulēt pieejamu atjauninājumu
debug-simulate-download = Simulēt lejupielādi
debug-clear-update = Notīrīt atjauninājumu
debug-trigger-restart = Aktivizēt restartēšanu

command-manage-spaces = Pārvaldīt telpas…
command-pane-stack-location = panelis { $pane } / steks { $stack }
command-space-pane-stack-location = { $space } / panelis { $pane } / steks { $stack }
command-terminal-path = Terminālis ({ $path })
command-group-interactive-mode = Interaktīvais režīms
command-group-window = Logs
command-group-tab = Cilne
command-group-pane = Panelis
command-group-stack = Steks
command-group-space = Telpa
command-group-navigation = Navigācija
command-group-open = Atvērt
command-group-view = Skats
command-group-bar = Josla

menu-close-vmux = Aizvērt Vmux

agents-terminal-coding-agent = Terminālī balstīts kodēšanas aģents
