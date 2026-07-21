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
