common-open = Vula
common-close = Vala
common-install = Faka
common-uninstall = Susa
common-update = Hlaziya
common-retry = Zama kwakhona
common-refresh = Vuselela
common-remove = Susa
common-enable = Vula
common-disable = Khubaza
common-new = Entsha
common-active = iyasebenza
common-running = iyaqhuba
common-done = Igqityiwe
common-failed = Yehlulekile
common-installed = Ifakiwe
common-items = { $count ->
    [one] { $count } into
   *[other] { $count } izinto
}
start-title = Qala
start-tagline = Umyalelo omnye. Nantoni na, igqityiwe.

agents-title = Ii-Agents
agents-search = Khangela ii-ACP ne-CLI agents…
agents-empty = Akukho agents zifanelekileyo
agents-empty-detail = Zama igama, runtime, okanye ACP/CLI.
agents-install-failed = Ukufaka kuhlulekile
agents-updating = Iyahlaziya…
agents-retrying = Izama kwakhona…
agents-preparing = Ilungiselela…

extensions-title = Ii-Extensions
extensions-search = Khangela ezifakiweyo okanye Chrome Web Store…
extensions-relaunch = Qala kwakhona ukusebenzisa
extensions-empty = Akukho extensions ezifakiweyo
extensions-no-match = Akukho extensions zifanelekileyo
extensions-empty-detail = Khangela ku-Chrome Web Store ngasentla ubhale uye Return.
extensions-no-match-detail = Zama elinye igama okanye i-ID ye-extension.
extensions-on = Ivuliwe
extensions-off = Ivalwe
extensions-enable-confirm = Vula { $name }?
extensions-enable-permissions = Vula { $name } uvumele:

lsp-title = Amaseva Olwimi
lsp-search = Khangela amaseva olwimi, ii-linters, ii-formatters…
lsp-loading = Ilayisha katalogi…
lsp-empty = Akukho amaseva olwimi afanelekileyo
lsp-empty-detail = Zama olunye ulwimi, i-linter, okanye i-formatter.
lsp-needs = ifuna { $tool }
lsp-status-available = Iyafumaneka
lsp-status-on-path = Ku-PATH
lsp-status-installing = Iyafakwa…
lsp-status-installed = Ifakiwe
lsp-status-outdated = Uhlaziyo lufumaneka
lsp-status-running = Iyaqhuba
lsp-status-failed = Yehlulekile

spaces-title = Iindawo
spaces-new-placeholder = Igama lendawo entsha
spaces-empty = Akukho ndawo
spaces-default-name = Indawo { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } iitabs
}
spaces-delete = Cima indawo

team-title = Iqela
team-just-you = Wena wedwa kule ndawo
team-agents = { $count ->
    [one] Wena ne-1 agent
   *[other] Wena ne-{ $count } agents
}
team-empty = Akukho mntu apha okwangoku
team-you = Wena
team-agent = I-Agent

services-title = Iinkonzo Zasemva
services-processes = { $count ->
    [one] 1 nkqubo
   *[other] { $count } iinkqubo
}
services-kill-all = Bulala Zonke
services-not-running = Inkonzo ayiqhubi
services-start-with = Qala nge:
services-empty = Akukho nkqubo ziyaqhuba
services-filter = Chonga iinkqubo…
services-no-match = Akukho nkqubo zifanelekileyo
services-connected = Ixhunyiwe
services-disconnected = Ayixhunyiwe
services-attached = ixhunyiwe
services-kill = Bulala
services-memory = Imemori
services-size = Ubungakanani
services-shell = I-Shell

error-title = Impazamo

history-search = Khangela imbali
history-clear-all = Sula konke
history-clear-confirm = Sula yonke imbali?
history-clear-warning = Oku akunakulungiswa.
history-cancel = Rhoxisa
history-today = Namhlanje
history-yesterday = Izolo
history-days-ago = { $count } iimini ezidlulileyo
history-day-offset = Imini -{ $count }

settings-title = Izisethi
settings-loading = Ilayisha izisethi…
settings-stored = Igcinwe ku-~/.vmux/settings.ron
settings-other = Okunye
settings-software-update = Uhlaziyo lweSoftware
settings-check-updates = Khangela Iinhlazizo
settings-check-updates-hint = Ikhangela ngokuzenzekelayo xa isiqala nangawe iyure nganye xa i-Auto-update ivuliwe.
settings-update-unavailable = Ayifumaneki
settings-update-unavailable-hint = I-Updater ayiqukwanga kweli qhosha lokwakha.
settings-update-checking = Ikhangela…
settings-update-checking-hint = Ikhangela iinhlazizo…
settings-update-check-again = Khangela Kwakhona
settings-update-current = Vmux ihlaziyiwe.
settings-update-downloading = Iyalandwa…
settings-update-downloading-hint = Ilanda Vmux { $version }…
settings-update-installing = Iyafakwa…
settings-update-installing-hint = Ifaka Vmux { $version }…
settings-update-ready = Uhlaziyo Lulungile
settings-update-ready-hint = Vmux { $version } ilungile. Qala kwakhona ukusebenzisa.
settings-update-try-again = Zama Kwakhona
settings-update-failed = Ayikwazanga ukukhangela iinhlazizo.
settings-item = Into
settings-item-number = Into { $number }
settings-press-key = Chofoza isitshixo…
settings-saved = Igcinwe
settings-record-key = Cofa ukurekhoda isitshixo esitsha

tray-open-window = Vula Ifestile
tray-close-window = Vala Ifestile
tray-pause-recording = Misa iRekhodi
tray-resume-recording = Qhuba iRekhodi
tray-finish-recording = Gqiba iRekhodi
tray-quit = Phuma ku-Vmux

composer-attach-files = Namathelisa iifayile (/upload)
composer-remove-attachment = Susa isinamathelisi

layout-back = Emuva
layout-forward = Phambili
layout-reload = Layisha kwakhona
layout-bookmark-page = Beka uphawu lwencwadi kule phepha
layout-remove-bookmark = Susa uphawu lwencwadi
layout-pin-page = Phina le phepha
layout-unpin-page = Susa ipin kule phepha
layout-manage-extensions = Phatha ii-extensions
layout-new-stack = I-Stack Entsha
layout-close-tab = Vala itab
layout-bookmark = Uphawu lwencwadi
layout-pin = Ipin
layout-new-tab = Itab entsha
layout-team = Iqela

command-switch-space = Tshintsha indawo…
command-search-ask = Khangela okanye buza…
command-new-tab-placeholder = Khangela okanye chwetheza i-URL, okanye khetha i-Terminal…
command-placeholder = Chwetheza i-URL, khangela iitabs, okanye > ngemiyalelo…
command-composer-placeholder = Chwetheza / ngemiyalelo okanye @ kwimidiya
command-send = Thumela (Enter)
command-terminal = I-Terminal
command-open-terminal = Vula kwi-Terminal
command-stack = I-Stack
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } iitabs
}
command-prompt = Isimemo
command-new-tab = Itab entsha
command-search = Khangela
command-open-value = Vula "{ $value }"
command-search-value = Khangela "{ $value }"

schema-appearance = Ukubonakala
schema-general = Jikelele
schema-layout = Umzimba
schema-layout-detail = Ifestile, amacandelo, isabelo, nebhande lokugxininisa.
schema-agent = I-Agent
schema-agent-detail = Ukuziphatha kwe-agent nezimvume zesethi.
schema-shortcuts = Izikhephe
schema-shortcuts-detail = Isimo sokubukela kuphela. Hlela settings.ron ngqo ukutshintsha iibindi.
schema-terminal = I-Terminal
schema-browser = Isiphequluli
schema-mode = Imodi
schema-mode-detail = Isikim sombala weephepha zeweb. I-Device ilandela inkqubo yakho.
schema-device = Isixhobo
schema-light = Ukukhanya
schema-dark = Bumnyama
schema-language = Ulwimi
schema-language-detail = Sebenzisa inkqubo, en-US, ja, okanye nayiphi na i-tag ye-BCP 47 ene-~/.vmux/locales/<tag>.ftl katalogi efanelekileyo.
schema-auto-update = Hlaziya ngokuzenzekelayo
schema-auto-update-detail = Khangela ufake iinhlazizo xa isiqala nangawe iyure nganye.
schema-startup-url = i-URL yokuQala
schema-startup-url-detail = Ukushiya kungenanto kuvula ibha yomyalelo.
schema-search-engine = Injini yokukhangela
schema-search-engine-detail = Isetyenziswa kukhangelo lweweb ukusuka ku-Start nasebhaarini yomyalelo.
schema-window = Ifestile
schema-pane = Icandelo
schema-side-sheet = Iphepha lelinye icala
schema-focus-ring = Ibhande lokugxininisa
schema-run-placement = Vumela ukuguqulwa kwendawo yokwenza
schema-run-placement-detail = Vumela ii-agents ukukhetha imodi, inkangeleko, nesiseko secandelo sokwenza.
schema-leader = I-Leader
schema-leader-detail = Isitshixo sokuqala sekhodi yesiqinisekiso.
schema-chord-timeout = Ixesha likaChord
schema-chord-timeout-detail = Imizuzwana phambi kokuba isiqalo sekhodi siphele.
schema-bindings = Iibindi
schema-confirm-close = Qinisekisa ukuvala
schema-confirm-close-detail = Buza ngaphambi kokuvala i-terminal ene-nkqubo esaqhuba.
schema-default-theme = Umxholo omisiweyo
schema-default-theme-detail = Igama lomxholo osebenzayo kuhlu lwemixholo.
