locale-name = isiXhosa
common-open = Vula
common-close = Vala
common-install = Faka
common-uninstall = Susa ufakelo
common-update = Hlaziya
common-retry = Zama kwakhona
common-refresh = Hlaziya kwakhona
common-remove = Susa
common-enable = Vula
common-disable = Cima
common-new = Entsha
common-active = iyasebenza
common-running = iyaqhuba
common-done = igqibile
common-failed = Ayiphumelelanga
common-installed = Ifakiwe
common-items = { $count ->
    [one] Into eyi-{ $count }
   *[other] Izinto eziyi-{ $count }
}

tools-title = Izixhobo
tools-search = Khangela iipakethe, iiarhente, MCP, izixhobo zolwimi neefayile zoqwalaselo…
tools-open = Vula izixhobo
tools-fold = Songa izixhobo
tools-unfold = Yandisa izixhobo
tools-scanning = Kuskenwa izixhobo zalapha…
tools-no-installed = Akukho zixhobo zifakiweyo
tools-empty = Akukho zixhobo zihambelanayo
tools-empty-detail = Faka ipakethe okanye wongeze ipakethe yeefayile zoqwalaselo yohlobo lweStow.
tools-apply = Sebenzisa
tools-homebrew = Homebrew
tools-homebrew-sync = Iifomyula neenkqubo ezifakiweyo ziyangqamaniswa ngokuzenzekelayo.
tools-open-brewfile = Vula Brewfile
tools-managed = ilawulwa
tools-provider-homebrew-formulae = Iifomyula zeHomebrew
tools-provider-homebrew-casks = Iinkqubo zeHomebrew
tools-provider-npm = Iipakethe zenpm
tools-provider-acp-agents = Iiarhente zeACP
tools-provider-language-tools = Izixhobo zolwimi
tools-provider-mcp-servers = Iiseva zeMCP
tools-provider-dotfiles = Iifayile zoqwalaselo
tools-status-available = Iyafumaneka
tools-status-missing = Ilahlekile
tools-status-conflict = Ungquzulwano
tools-forget = Libala
tools-manage = Lawula
tools-link = Qhagamshela
tools-unlink = Nqamula
tools-import = Ngenisa
tools-update-count = { $count ->
    [one] Uhlaziyo olu-1
   *[other] Uhlaziyo olu-{ $count }
}
tools-conflict-count = { $count ->
    [one] Ungquzulwano olu-1
   *[other] Ungquzulwano olu-{ $count }
}
tools-result-applied = Izixhobo zisetyenzisiwe
tools-result-imported = Izixhobo zingenisiwe
tools-result-installed = { $name } ifakiwe
tools-result-updated = { $name } ihlaziyiwe
tools-result-uninstalled = { $name } isusiwe
tools-result-forgotten = { $name } ilityelwe
tools-result-managed = { $name } ngoku iyalawulwa
tools-result-linked = { $name } iqhagamshelwe
tools-result-unlinked = { $name } inqamliwe

start-title = Qalisa
start-tagline = Umyalelo omnye. Nantoni na, yenziwe.

agents-title = Iiarhente
agents-search = Khangela iiarhente ze-ACP ne-CLI…
agents-empty = Akukho arhente zingqinelanayo
agents-empty-detail = Zama igama, indawo yokuqhuba, okanye ACP/CLI.
agents-install-failed = Ufakelo aluphumelelanga
agents-updating = Kuyahlaziywa…
agents-retrying = Kuyazanywa kwakhona…
agents-preparing = Kuyalungiselelwa…

extensions-title = Izandiso
extensions-search = Khangela ezifakiweyo okanye kwi-Chrome Web Store…
extensions-relaunch = Qalisa kwakhona ukuze kusebenze
extensions-empty = Akukho zandiso zifakiweyo
extensions-no-match = Akukho zandiso zingqinelanayo
extensions-empty-detail = Khangela kwi-Chrome Web Store ngasentla uze ucinezele u-Return.
extensions-no-match-detail = Zama elinye igama okanye i-ID yesandiso.
extensions-on = Kuvuliwe
extensions-off = Kucinyiwe
extensions-enable-confirm = Vula { $name }?
extensions-enable-permissions = Vula { $name } kwaye uvumele:

lsp-title = Iiseva zeeLwimi
lsp-search = Khangela iiseva zeelwimi, ii-linter, ii-formatter…
lsp-loading = Kulayishwa ikhathalogu…
lsp-empty = Akukho seva zolwimi zingqinelanayo
lsp-empty-detail = Zama olunye ulwimi, i-linter, okanye i-formatter.
lsp-needs = ifuna { $tool }
lsp-status-available = Iyafumaneka
lsp-status-on-path = Ikwi-PATH
lsp-status-installing = Kuyafakwa…
lsp-status-installed = Ifakiwe
lsp-status-outdated = Uhlaziyo luyafumaneka
lsp-status-running = Iyaqhuba
lsp-status-failed = Ayiphumelelanga

spaces-title = Iindawo
spaces-new-placeholder = Igama lendawo entsha
spaces-empty = Akukho ndawo
spaces-default-name = Indawo { $number }
spaces-tabs = { $count ->
    [one] Ithebhu eyi-1
   *[other] Iithebhu eziyi-{ $count }
}
spaces-delete = Cima indawo

team-title = Iqela
team-just-you = Nguwe wedwa kule ndawo
team-agents = { $count ->
    [one] Wena nearhente eyi-1
   *[other] Wena neearhente eziyi-{ $count }
}
team-empty = Akukho mntu apha okwangoku
team-you = Wena
team-agent = Iarhente

services-title = Iinkonzo Zangasemva
services-processes = { $count ->
    [one] Inkqubo eyi-1
   *[other] Iinkqubo eziyi-{ $count }
}
services-kill-all = Nyanzela Ukuphelisa Zonke
services-not-running = Inkonzo ayiqhubi
services-start-with = Qalisa nge:
services-empty = Akukho nkqubo zisebenzayo
services-filter = Hluza iinkqubo…
services-no-match = Akukho nkqubo zingqinelanayo
services-connected = Iqhagamshelwe
services-disconnected = Iqhawukile
services-attached = iqhotyoshelwe
services-kill = Nyanzela ukuphelisa
services-memory = Imemori
services-size = Ubungakanani
services-shell = Shell

error-title = Impazamo

history-search = Khangela imbali
history-clear-all = Cima konke
history-clear-confirm = Cima yonke imbali?
history-clear-warning = Oku akunakubuyiselwa.
history-cancel = Rhoxisa
history-today = Namhlanje
history-yesterday = Izolo
history-days-ago = Kwiintsuku eziyi-{ $count } ezidlulileyo
history-day-offset = Usuku -{ $count }

settings-title = Iisetingi
settings-loading = Kulayishwa iisetingi…
settings-stored = Igcinwe ku-~/.vmux/settings.ron
settings-other = Okunye
settings-software-update = Uhlaziyo Lwesoftware
settings-check-updates = Khangela Uhlaziyo
settings-check-updates-hint = Ikhangela ngokuzenzekela xa iqaliswa nakweyure nganye xa uHlaziyo oluzenzekelayo luvuliwe.
settings-update-unavailable = Ayifumaneki
settings-update-unavailable-hint = Isihlaziyi asiqukwanga kolu lwakhiwo.
settings-update-checking = Kuyakhangelwa…
settings-update-checking-hint = Kukhangelwa uhlaziyo…
settings-update-check-again = Khangela Kwakhona
settings-update-current = I-Vmux isesexesheni.
settings-update-downloading = Kuyakhutshelwa…
settings-update-downloading-hint = Kukhutshelwa i-Vmux { $version }…
settings-update-installing = Kuyafakwa…
settings-update-installing-hint = Kufakwa i-Vmux { $version }…
settings-update-ready = Uhlaziyo Lulungile
settings-update-ready-hint = I-Vmux { $version } ilungile. Qalisa kwakhona ukuze lusebenze.
settings-update-try-again = Zama Kwakhona
settings-update-failed = Ayikwazanga ukukhangela uhlaziyo.
settings-item = Into
settings-item-number = Into { $number }
settings-press-key = Cinezela iqhosha…
settings-saved = Igciniwe
settings-record-key = Cofa ukuze ubambe indibaniselwano entsha yamaqhosha

tray-open-window = Vula Ifestile
tray-close-window = Vala Ifestile
tray-pause-recording = Misa Ukurekhoda
tray-resume-recording = Qhubeka Nokurekhoda
tray-finish-recording = Gqiba Ukurekhoda
tray-quit = Phuma ku-Vmux

composer-attach-files = Qhoboshela iifayile (/upload)
composer-remove-attachment = Susa isiqhoboshelo

layout-back = Emva
layout-forward = Phambili
layout-reload = Layisha kwakhona
layout-bookmark-page = Phawula eli phepha
layout-remove-bookmark = Susa uphawu
layout-pin-page = Qhobosha eli phepha
layout-unpin-page = Susa uqhobosho kweli phepha
layout-manage-extensions = Lawula izandiso
layout-new-stack = Isitaki esitsha
layout-close-tab = Vala ithebhu
layout-bookmark = Phawula
layout-pin = Qhobosha
layout-new-tab = Ithebhu entsha
layout-team = Iqela

command-switch-space = Tshintshela kwenye indawo…
command-search-ask = Khangela okanye ubuze…
command-new-tab-placeholder = Khangela okanye chwetheza i-URL, okanye ukhethe i-Terminal…
command-placeholder = Chwetheza i-URL, khangela iithebhu, okanye > yemiyalelo…
command-composer-placeholder = Chwetheza / yemiyalelo okanye @ yemidiya
command-send = Thumela (Enter)
command-terminal = Terminal
command-open-terminal = Vula kwi-Terminal
command-stack = Isitaki
command-tabs = { $count ->
    [one] Ithebhu eyi-1
   *[other] Iithebhu eziyi-{ $count }
}
command-prompt = Umyalelo
command-new-tab = Ithebhu entsha
command-search = Khangela
command-open-value = Vula “{ $value }”
command-search-value = Khangela “{ $value }”

schema-appearance = Inkangeleko
schema-general = Ngokubanzi
schema-layout = Uyilo
schema-layout-detail = Ifestile, iiphaneli, ibha esecaleni, nesangqa sogxininiso.
schema-agent = Iarhente
schema-agent-detail = Ukuziphatha kwearhente neemvume zezixhobo.
schema-shortcuts = Iindlela ezimfutshane
schema-shortcuts-detail = Umbono wokufunda kuphela. Hlela settings.ron ngqo ukuze utshintshe izibophelelo.
schema-terminal = Terminal
schema-browser = Isikhangeli
schema-mode = Imo
schema-mode-detail = Isikimu sombala samaphepha ewebhu. Isixhobo silandela isixokelelwano sakho.
schema-device = Isixhobo
schema-light = Ukukhanya
schema-dark = Ubumnyama
schema-language = Ulwimi
schema-language-detail = Sebenzisa esesixokelelwano, en-US, ja, okanye nayiphi na ithegi ye-BCP 47 enekhathalogu ehambelanayo ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Uhlaziyo oluzenzekelayo
schema-auto-update-detail = Khangela uze ufake uhlaziyo xa kuqaliswa nakweyure nganye.
schema-startup-url = I-URL yokuqalisa
schema-startup-url-detail = Xa ingenanto kuvulwa umyalelo webha yemiyalelo.
schema-search-engine = Injini yokukhangela
schema-search-engine-detail = Isetyenziswa kukhangelo lwewebhu olusuka kuQalisa nakwibha yemiyalelo.
schema-window = Ifestile
schema-pane = Iphaneli
schema-side-sheet = Iphepha lasecaleni
schema-focus-ring = Isangqa sogxininiso
schema-run-placement = Vumela ukugqithisa indawo yokuqhuba
schema-run-placement-detail = Vumela iiarhente zikhethe imo yephaneli yokuqhuba, icala, nendawo yokubambelela.
schema-leader = Inkokeli
schema-leader-detail = Iqhosha lokuqala kwiindlela ezimfutshane ze-chord.
schema-chord-timeout = Ixesha lokuphela kwe-chord
schema-chord-timeout-detail = Iimilisekondi phambi kokuba isimaphambili se-chord siphelelwe.
schema-bindings = Izibophelelo
schema-confirm-close = Qinisekisa ukuvala
schema-confirm-close-detail = Cela ukuqinisekisa ngaphambi kokuvala i-terminal enenkqubo eqhubayo.
schema-default-theme = Umxholo omiselweyo
schema-default-theme-detail = Igama lomxholo osebenzayo kuluhlu lwemixholo.

settings-empty = (akukho nto)
settings-none = (akukho)

schema-system = Inkqubo
schema-editor = Umhleli
schema-recording = Ukurekhoda
schema-radius = Iradiyasi
schema-padding = Isithuba sangaphakathi
schema-gap = Umsantsa
schema-width = Ububanzi
schema-color = Umbala
schema-red = Bomvu
schema-green = Luhlaza
schema-blue = Bhlowu
schema-follow-files = Landela iifayile
schema-tidy-files = Qoqosha iifayile
schema-tidy-files-max = Umda wokuqoqosha iifayile
schema-tidy-files-auto = Qoqosha iifayile ngokuzenzekelayo
schema-app-providers = Ababoneleli beapp
schema-provider = Umboneleli
schema-kind = Uhlobo
schema-models = Iimodeli
schema-acp = Iiarhente ze-ACP
schema-id = ID
schema-name = Igama
schema-command = Umyalelo
schema-arguments = Iimpikiswano
schema-environment = Imekobume
schema-working-directory = Ifolda yokusebenza
schema-shell = Iqokobhe
schema-font-family = Usapho lwefonti
schema-startup-directory = Ifolda yokuqalisa
schema-themes = Imixholo
schema-color-scheme = Iskimu sombala
schema-font-size = Ubungakanani befonti
schema-line-height = Ubude bomgca
schema-cursor-style = Isimbo sekhesa
schema-cursor-blink = Ukuqhwanyaza kwekhesa
schema-custom-themes = Imixholo eyenziweyo
schema-foreground = Umphambili
schema-background = Imvelaphi
schema-cursor = Ikhesa
schema-ansi-colors = Imibala ye-ANSI
schema-keymap = Imephu yezitshixo
schema-explorer = Umkhangeli
schema-visible = Iyabonakala
schema-language-servers = Iiseva zeelwimi
schema-servers = Iiseva
schema-language-id = ID yolwimi
schema-root-markers = Iimpawu zengcambu
schema-output-directory = Ifolda yemveliso

menu-scene = Umboniso
menu-layout = Ubeko
menu-terminal = Itheminali
menu-browser = Isikhangeli
menu-service = Inkonzo
menu-bookmark = Isiphawuli
menu-edit = Hlela

layout-knowledge = Ulwazi
layout-open-knowledge = Vula Ulwazi
layout-open-welcome-knowledge = Vula Wamkelekile kuLwazi
layout-open-path = Vula { $path }
layout-fold-knowledge = Songa ulwazi
layout-unfold-knowledge = Yandisa ulwazi
layout-bookmarks = Iziphawuli
layout-new-folder = Ifolda Entsha
layout-add-to-bookmarks = Yongeza kwiziphawuli
layout-move-to-bookmarks = Hambisa kwiziphawuli
layout-stack-number = Isitaki { $number }
layout-fold-stack = Songa isitaki
layout-unfold-stack = Yandisa isitaki
layout-close-stack = Vala isitaki
layout-bookmark-in = Phawula ku-{ $folder }

common-cancel = Rhoxisa
common-delete = Cima
common-save = Gcina
common-rename = Thiya ngokutsha
common-expand = Yandisa
common-collapse = Songa
common-loading = Kuyalayishwa…
common-error = Impazamo
common-output = Imveliso
common-pending = Ilindile
common-current = yangoku
common-stop = Misa
services-command = Inkonzo ye-Vmux
services-uptime-seconds = { $seconds }s
services-uptime-minutes = { $minutes }m { $seconds }s
services-uptime-hours = { $hours }h { $minutes }m
services-uptime-days = { $days }d { $hours }h

error-page-failed-load = Iphepha alilayishwanga
error-page-not-found = Iphepha alifunyenwanga
error-unknown-host = Umamkeli we-app ye-Vmux ongaziwayo: { $host }

history-title = Imbali

command-new-app-chat = Incoko entsha ye-{ $provider }/{ $model } (App)
command-interactive-mode-user = Umboniso > Imo yokusebenzisana > Umsebenzisi
command-interactive-mode-player = Umboniso > Imo yokusebenzisana > Umdlali
command-minimize-window = Uyilo > Ifestile > Nciphisa
command-toggle-layout = Uyilo > Uyilo > Tshintsha uyilo
command-close-tab = Uyilo > Ithebhu > Vala ithebhu
command-new-task = Uyilo > Ithebhu > Umsebenzi omtsha…
command-next-tab = Uyilo > Ithebhu > Ithebhu elandelayo
command-prev-tab = Uyilo > Ithebhu > Ithebhu edlulileyo
command-rename-tab = Uyilo > Ithebhu > Thiya ithebhu ngokutsha
command-tab-select-1 = Uyilo > Ithebhu > Khetha ithebhu 1
command-tab-select-2 = Uyilo > Ithebhu > Khetha ithebhu 2
command-tab-select-3 = Uyilo > Ithebhu > Khetha ithebhu 3
command-tab-select-4 = Uyilo > Ithebhu > Khetha ithebhu 4
command-tab-select-5 = Uyilo > Ithebhu > Khetha ithebhu 5
command-tab-select-6 = Uyilo > Ithebhu > Khetha ithebhu 6
command-tab-select-7 = Uyilo > Ithebhu > Khetha ithebhu 7
command-tab-select-8 = Uyilo > Ithebhu > Khetha ithebhu 8
command-tab-select-last = Uyilo > Ithebhu > Khetha ithebhu yokugqibela
command-close-pane = Uyilo > Ipheyini > Vala ipheyini
command-select-pane-left = Uyilo > Ipheyini > Khetha ipheyini yasekhohlo
command-select-pane-right = Uyilo > Ipheyini > Khetha ipheyini yasekunene
command-select-pane-up = Uyilo > Ipheyini > Khetha ipheyini ephezulu
command-select-pane-down = Uyilo > Ipheyini > Khetha ipheyini esezantsi
command-swap-pane-prev = Uyilo > Ipheyini > Tshintsha nepheyini edlulileyo
command-swap-pane-next = Uyilo > Ipheyini > Tshintsha nepheyini elandelayo
command-equalize-pane-size = Uyilo > Ipheyini > Linganisa ubungakanani beepheyini
command-resize-pane-left = Uyilo > Ipheyini > Yenza ipheyini ibe ngasekhohlo
command-resize-pane-right = Uyilo > Ipheyini > Yenza ipheyini ibe ngasekunene
command-resize-pane-up = Uyilo > Ipheyini > Yenza ipheyini ibe phezulu
command-resize-pane-down = Uyilo > Ipheyini > Yenza ipheyini ibe phantsi
command-stack-close = Uyilo > Isitaki > Vala isitaki
command-stack-next = Uyilo > Isitaki > Isitaki esilandelayo
command-stack-previous = Uyilo > Isitaki > Isitaki esidlulileyo
command-stack-reopen = Uyilo > Isitaki > Phinda uvule iphepha elivaliweyo
command-stack-swap-prev = Uyilo > Isitaki > Hambisa isitaki ngasekhohlo
command-stack-swap-next = Uyilo > Isitaki > Hambisa isitaki ngasekunene
command-space-open = Uyilo > Isithuba > Izithuba
command-terminal-close = Iterminal > Vala iterminal
command-terminal-next = Iterminal > Iterminal elandelayo
command-terminal-prev = Iterminal > Iterminal edlulileyo
command-terminal-clear = Iterminal > Coca iterminal
command-browser-prev-page = Isikhangeli > Ukuhamba > Buyela
command-browser-next-page = Isikhangeli > Ukuhamba > Phambili
command-browser-reload = Isikhangeli > Ukuhamba > Layisha kwakhona
command-browser-hard-reload = Isikhangeli > Ukuhamba > Layisha kwakhona ngokupheleleyo
command-open-in-place = Isikhangeli > Vula > Vula apha
command-open-in-new-stack = Isikhangeli > Vula > Vula kwisitaki esitsha
command-open-in-pane-top = Isikhangeli > Vula > Vula kwipheyini engasentla
command-open-in-pane-right = Isikhangeli > Vula > Vula kwipheyini yasekunene
command-open-in-pane-bottom = Isikhangeli > Vula > Vula kwipheyini esezantsi
command-open-in-pane-left = Isikhangeli > Vula > Vula kwipheyini yasekhohlo
command-open-in-new-tab = Isikhangeli > Vula > Vula kwithebhu entsha
command-open-in-new-space = Isikhangeli > Vula > Vula kwisithuba esitsha
command-browser-zoom-in = Isikhangeli > Jonga > Sondeza
command-browser-zoom-out = Isikhangeli > Jonga > Hlehlisa usondezo
command-browser-zoom-reset = Isikhangeli > Jonga > Ubungakanani bokwenene
command-browser-dev-tools = Isikhangeli > Jonga > Izixhobo zabaphuhlisi
command-browser-open-command-bar = Isikhangeli > Ibhari > Ibhari yemiyalelo
command-browser-open-page-in-command-bar = Isikhangeli > Ibhari > Hlela iphepha
command-browser-open-path-bar = Isikhangeli > Ibhari > Umkhombandlela wendlela
command-browser-open-commands = Isikhangeli > Ibhari > Imiyalelo
command-browser-open-history = Isikhangeli > Ibhari > Imbali
command-service-open = Inkonzo > Vula umlindi wenkonzo
command-bookmark-toggle-active = Isiphawuli > Phawula iphepha
command-bookmark-pin-active = Isiphawuli > Qhobosha iphepha

layout-tab = Ithebhu
layout-no-stacks = Akukho zitaki
layout-loading = Kuyalayishwa…
layout-no-markdown-files = Akukho fayile ze-Markdown
layout-empty-folder = Ifolda engenanto
layout-worktree = umthi womsebenzi
layout-folder-name = Igama lefolda
layout-no-pins-bookmarks = Akukho ziqhoboshi okanye ziphawuli
layout-move-to = Hambisa ku-{ $folder }
layout-bookmark-current-page = Phawula iphepha langoku
layout-rename-folder = Thiya ifolda ngokutsha
layout-remove-folder = Susa ifolda
layout-update-downloading = Kukhutshelwa uhlaziyo
layout-update-installing = Kuhlohlwa uhlaziyo…
layout-update-ready = Inguqulelo entsha ikhona
layout-restart-update = Qalisa kwakhona ukuze uhlaziye

agent-preparing = Kulungiselelwa i-arhente…
agent-send-all-queued = Thumela yonke imibuzo esemgqeni ngoku (Esc)
agent-send = Thumela (Enter)
agent-ready = Ndilungile xa nawe ulungile.
agent-loading-older = Kulayishwa imiyalezo emidala…
agent-load-older = Layisha imiyalezo emidala
agent-continued-from = Iqhubeke isuka ku-{ $source }
agent-older-context-omitted = umxholo omdala ushiyiwe
agent-interrupted = iphazamisekile
agent-allow-tool = Vumela { $tool }?
agent-deny = Yala
agent-allow-always = Vumela rhoqo
agent-allow = Vumela
agent-loading-sessions = Kulayishwa iiseshoni…
agent-no-resumable-sessions = Akukho seshoni inokuqhutywa efunyenweyo
agent-no-matching-sessions = Akukho seshoni zingqinelanayo
agent-no-matching-models = Akukho modeli zingqinelanayo
agent-choice-help = ↑/↓ okanye Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Khetha ifolda yerepository
agent-choose-repository-detail = Khetha irepository ye-Git yalapha ekufuneka isetyenziswe yi-arhente.
agent-choosing = Kuyakhethwa…
agent-choose-folder = Khetha ifolda
agent-queued = isemgqeni
agent-attached = Kuqhotyoshelwe:
agent-cancel-queued = Rhoxisa umbuzo osemgqeni
agent-resume-queued = Qhubekisa imibuzo esemgqeni
agent-clear-queue = Coca umgca
agent-send-all-now = thumela konke ngoku
agent-choose-option = Khetha ukhetho ngasentla
agent-loading-media = Kulayishwa imidiya…
agent-no-matching-media = Akukho midiya ingqinelanayo
agent-prompt-context = Umxholo wombuzo
agent-details = Iinkcukacha
agent-path = Indlela
agent-tool = Isixhobo
agent-server = Iseva
agent-bytes = { $count } bytes
agent-worked-for = Isebenze ixesha elingu-{ $duration }
agent-worked-for-steps = { $count ->
    [one] Isebenze ixesha elingu-{ $duration } · inyathelo eli-1
   *[other] Isebenze ixesha elingu-{ $duration } · amanyathelo angu-{ $count }
}
agent-tool-guardian-review = Uphononongo lomlindi
agent-tool-read-files = Ifunde iifayile
agent-tool-viewed-image = Ijonge umfanekiso
agent-tool-used-browser = Isebenzise isikhangeli
agent-tool-searched-files = Ikhangele iifayile
agent-tool-ran-commands = Iqhube imiyalelo
agent-thinking = Iyacinga
agent-subagent = I-arhente engaphantsi
agent-prompt = Umbuzo
agent-thread = Umsonto
agent-parent = Umzali
agent-children = Abantwana
agent-call = Ubizo
agent-raw-event = Isiganeko esingacutshungulwanga
agent-plan = Isicwangciso
agent-tasks = { $count ->
    [one] umsebenzi o-1
   *[other] imisebenzi engu-{ $count }
}
agent-edited = Ihleliwe
agent-reconnecting = Kuqhagamshelwa kwakhona { $attempt }/{ $total }
agent-status-running = Iyasebenza
agent-status-done = Kugqityiwe
agent-status-failed = Ayiphumelelanga
agent-status-pending = Ilindile
agent-slash-attach-files = Qhoboshela iifayile
agent-slash-resume-session = Qhubekisa iseshoni yangaphambili
agent-slash-select-model = Khetha imodeli
agent-slash-continue-cli = Qhubeka nale seshoni kwi-CLI
agent-session-just-now = ngoku nje
agent-session-minutes-ago = kwi-{ $count }m edlulileyo
agent-session-hours-ago = kwi-{ $count }h edlulileyo
agent-session-days-ago = kwi-{ $count }d edlulileyo
agent-working-working = Iyasebenza
agent-working-thinking = Iyacinga
agent-working-pondering = Iyacamngca
agent-working-noodling = Iyalungisa iingcinga
agent-working-percolating = Iyavuthwa
agent-working-conjuring = Iyavelisa
agent-working-cooking = Iyapheka
agent-working-brewing = Iyavubela
agent-working-musing = Iyazikisa
agent-working-ruminating = Iyaphonononga
agent-working-scheming = Iyaceba
agent-working-synthesizing = Iyadibanisa
agent-working-tinkering = Iyazama
agent-working-churning = Iyagaya
agent-working-vibing = Ikwimo yayo
agent-working-simmering = Iyabila kancinci
agent-working-crafting = Iyakha
agent-working-divining = Iyaphengulula
agent-working-mulling = Iyacingisisa
agent-working-spelunking = Iyemba nzulu

editor-toggle-explorer = Tshintsha i-Explorer (Cmd+B)
editor-unsaved = ayigcinwanga
editor-rendered-markdown = I-Markdown ebonisiweyo enokuhlelwa ngqo
editor-note = Inqaku
editor-source-editor = Umhleli wekhowudi
editor-editor = Umhleli
editor-git-diff = Umahluko we-Git
editor-diff = Umahluko
editor-tidy = Coca
editor-always = Rhoqo
editor-unchanged-previews = { $count ->
    [one] ✦ imboniso e-1 engatshintshanga
   *[other] ✦ iimboniso ezingu-{ $count } ezingatshintshanga
}
editor-open-externally = Vula ngaphandle
editor-changed-line = Umgca otshintshileyo
editor-go-to-definition = Yiya kwinkcazelo
editor-find-references = Fumana izalathiso
editor-references = { $count ->
    [one] isalathiso esi-1
   *[other] izalathiso ezingu-{ $count }
}
editor-lsp-starting = { $server } iyaqalisa…
editor-lsp-not-installed = { $server } — ayihlohlwanga
editor-explorer = Explorer
editor-open-editors = Abahleli abavulekileyo
editor-outline = Isishwankathelo
editor-new-file = Ifayile entsha
editor-new-folder = Ifolda entsha
editor-delete-confirm = Cima “{ $name }”? Oku akunakubuyiselwa.
editor-created-folder = Kudalwe ifolda { $name }
editor-created-file = Kudalwe ifayile { $name }
editor-renamed-to = Ithiywe ngokutsha yaba ngu-{ $name }
editor-deleted = Kucinywe { $name }
editor-failed-decode-image = Ayiphumelelanga ukucacisa umfanekiso
editor-preview-large-image = umfanekiso (mkhulu kakhulu ukuba uboniswe)
editor-preview-binary = ibhayinari
editor-preview-file = ifayile

git-status-clean = icocekile
git-status-modified = itshintshiwe
git-status-staged = ilungiselelwe
git-status-staged-modified = ilungiselelwe*
git-status-untracked = ayilandelwa
git-status-deleted = icinyiwe
git-status-conflict = ungquzulwano
git-accept-all = ✓ yamkela konke
git-unstage = Susa kulungiselelo
git-confirm-deny-all = Qinisekisa ukwala konke
git-deny-all = ✗ yala konke
git-commit-message = umyalezo we-commit
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Kulayishwa umahluko…
git-no-changes = Akukho lutshintsho luboniswayo
git-accept = ✓ yamkela
git-deny = ✗ yala
git-show-unchanged-lines = Bonisa imigca engatshintshanga engu-{ $count }

terminal-loading = Kuyalayishwa…
terminal-runs-when-ready = iqhuba xa ilungile · Ctrl+C iyacoca · Esc iyatsiba
terminal-booting = iyaqalisa
terminal-type-command = chwetheza umyalelo · iqhuba xa ilungile · Esc iyatsiba

setup-tagline-claude = I-arhente yokukhowuda ka-Anthropic, kwi-Vmux
setup-tagline-codex = I-arhente yokukhowuda ka-OpenAI, kwi-Vmux
setup-tagline-vibe = I-arhente yokukhowuda ka-Mistral, kwi-Vmux
setup-install-title = Hlohla i-CLI ye-{ $name }
setup-homebrew-required = I-Homebrew iyafuneka ukuze kuhlohlwe { $command }, kwaye ayikamiselwa. I-Vmux iza kuhlohla i-Homebrew kuqala, ize ihlohle { $name }.
setup-terminal-instructions = Kwi-terminal, cofa u-Return ukuze uqalise, uze ufake igama lokugqithisa le-Mac yakho xa ucelwa.
setup-command-missing = I-Vmux ivule eli phepha kuba umyalelo walapha { $command } awukahlohlwa. Qhuba umyalelo ongezantsi ukuze uwufumane.
setup-install-failed = Ukuhlohla akugqitywanga. Jonga i-terminal ngeenkcukacha, uze uzame kwakhona.
setup-installing = Kuyahlohlwa…
setup-install-homebrew = Hlohla i-Homebrew + { $name }
setup-run-install = Qhuba umyalelo wokuhlohla
setup-auto-reload = I-Vmux iwusebenzisa kwi-terminal ize ilayishe kwakhona xa { $command } ilungile.

debug-title = Lungisa iimpazamo
debug-auto-update = Uhlaziyo oluzenzekelayo
debug-simulate-update = Linganisa uhlaziyo olukhoyo
debug-simulate-download = Linganisa ukukhuphela
debug-clear-update = Coca uhlaziyo
debug-trigger-restart = Qalisa ukuqalisa kwakhona

command-manage-spaces = Lawula izithuba…
command-pane-stack-location = ipheyini { $pane } / istaki { $stack }
command-space-pane-stack-location = { $space } / ipheyini { $pane } / istaki { $stack }
command-terminal-path = Itheminali ({ $path })
command-group-interactive-mode = Imo yonxibelelwano
command-group-window = Ifestile
command-group-tab = Ithebhu
command-group-pane = Ipheyini
command-group-stack = Istaki
command-group-space = Isithuba
command-group-navigation = Ukuzulazula
command-group-open = Vula
command-group-view = Jonga
command-group-bar = Ibha

menu-close-vmux = Vala i-Vmux

agents-terminal-coding-agent = I-arhente yokukhowuda esekelwe kwitheminali
