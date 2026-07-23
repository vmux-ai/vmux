locale-name = isiZulu
common-open = Vula
common-close = Vala
common-install = Faka
common-uninstall = Khipha
common-update = Buyekeza
common-retry = Zama futhi
common-refresh = Vuselela
common-remove = Susa
common-enable = Nika amandla
common-disable = Khubaza
common-new = Okusha
common-active = kuyasebenza
common-running = kuyaqhubeka
common-done = kuqediwe
common-failed = Kuhlulekile
common-installed = Kufakiwe
common-items = { $count ->
    [one] { $count } into
   *[other] { $count } izinto
}

tools-title = Amathuluzi
tools-search = Sesha amaphakheji, ama-ejenti, MCP, amathuluzi olimi namafayela okumisa…
tools-open = Vula amathuluzi
tools-fold = Songa amathuluzi
tools-unfold = Nweba amathuluzi
tools-scanning = Kuskenwa amathuluzi endawo…
tools-no-installed = Awekho amathuluzi afakiwe
tools-empty = Awekho amathuluzi afanayo
tools-empty-detail = Faka iphakheji noma engeza iphakheji yamafayela okumisa yesitayela seStow.
tools-apply = Sebenzisa
tools-homebrew = Homebrew
tools-homebrew-sync = Amafomula nezinhlelo ezifakiwe zivumelaniswa ngokuzenzakalelayo.
tools-open-brewfile = Vula Brewfile
tools-managed = kulawulwa
tools-provider-homebrew-formulae = Amafomula eHomebrew
tools-provider-homebrew-casks = Izinhlelo zeHomebrew
tools-provider-npm = Amaphakheji enpm
tools-provider-acp-agents = Ama-ejenti eACP
tools-provider-language-tools = Amathuluzi olimi
tools-provider-mcp-servers = Amaseva eMCP
tools-provider-dotfiles = Amafayela okumisa
tools-status-available = Kuyatholakala
tools-status-missing = Akukho
tools-status-conflict = Ukungqubuzana
tools-forget = Khohlwa
tools-manage = Phatha
tools-link = Xhumanisa
tools-unlink = Nqamula
tools-import = Ngenisa
tools-update-count = { $count ->
    [one] Isibuyekezo esi-1
   *[other] Izibuyekezo ezi-{ $count }
}
tools-conflict-count = { $count ->
    [one] Ukungqubuzana oku-1
   *[other] Ukungqubuzana oku-{ $count }
}
tools-result-applied = Amathuluzi asetshenzisiwe
tools-result-imported = Amathuluzi angenisiwe
tools-result-installed = { $name } ifakiwe
tools-result-updated = { $name } ibuyekeziwe
tools-result-uninstalled = { $name } ikhishiwe
tools-result-forgotten = { $name } ikhohliwe
tools-result-managed = { $name } manje iyalawulwa
tools-result-linked = { $name } ixhunyanisiwe
tools-result-unlinked = { $name } inqanyuliwe
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Vumelanisa izilungiselelo, amathuluzi, amachashazi, kanye nolwazi nge-Git.
vault-sync = Vumelanisa
vault-create = Dala
vault-connect = Xhuma
vault-private = Inqolobane yangasese
vault-public-warning = Amakhosombe asesidlangalaleni aveza Ulwazi lwakho nokucushwa.
vault-choose-repository = Khetha indawo yokugcina...
vault-empty = ayinalutho
vault-clean = Kusesikhathini
vault-not-connected = Ayixhunyiwe
vault-change-count = Izinguquko: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Qala
start-tagline = Umyalo owodwa. Konke kuyenzeka.

agents-title = Ama-ejenti
agents-search = Sesha ama-ejenti e-ACP nawe-CLI…
agents-empty = Awekho ama-ejenti afanayo
agents-empty-detail = Zama igama, indawo yokusebenza, noma i-ACP/CLI.
agents-install-failed = Ukufaka kuhlulekile
agents-updating = Kuyabuyekezwa…
agents-retrying = Kuyazanywa futhi…
agents-preparing = Kuyalungiswa…

extensions-title = Izandiso
extensions-search = Sesha ezifakiwe noma i-Chrome Web Store…
extensions-relaunch = Qalisa kabusha ukuze kusebenze
extensions-empty = Azikho izandiso ezifakiwe
extensions-no-match = Azikho izandiso ezifanayo
extensions-empty-detail = Sesha ku-Chrome Web Store ngenhla bese ucindezela u-Return.
extensions-no-match-detail = Zama elinye igama noma i-ID yesandiso.
extensions-on = Kuvuliwe
extensions-off = Kuvaliwe
extensions-enable-confirm = Nika amandla i-{ $name }?
extensions-enable-permissions = Nika amandla i-{ $name } bese uvumela:

lsp-title = Amaseva Olimi
lsp-search = Sesha amaseva olimi, ama-linter, ama-formatter…
lsp-loading = Kulayishwa ikhathalogi…
lsp-empty = Awekho amaseva olimi afanayo
lsp-empty-detail = Zama olunye ulimi, i-linter, noma i-formatter.
lsp-needs = idinga { $tool }
lsp-status-available = Iyatholakala
lsp-status-on-path = Iku-PATH
lsp-status-installing = Kuyafakwa…
lsp-status-installed = Kufakiwe
lsp-status-outdated = Isibuyekezo siyatholakala
lsp-status-running = Iyaqhubeka
lsp-status-failed = Kuhlulekile

spaces-title = Izindawo
spaces-new-placeholder = Igama lendawo entsha
spaces-empty = Azikho izindawo
spaces-default-name = Indawo { $number }
spaces-tabs = { $count ->
    [one] ithebhu engu-1
   *[other] amathebhu angu-{ $count }
}
spaces-delete = Susa indawo

team-title = Ithimba
team-just-you = Nguwe kuphela kule ndawo
team-agents = { $count ->
    [one] Wena ne-ejenti engu-1
   *[other] Wena nama-ejenti angu-{ $count }
}
team-empty = Akekho lapha okwamanje
team-you = Wena
team-agent = I-ejenti

services-title = Amasevisi Angemuva
services-processes = { $count ->
    [one] inqubo engu-1
   *[other] izinqubo ezingu-{ $count }
}
services-kill-all = Misa Zonke Ngempoqo
services-not-running = Isevisi ayiqhubeki
services-start-with = Qala ngo-:
services-empty = Azikho izinqubo ezisebenzayo
services-filter = Hlunga izinqubo…
services-no-match = Azikho izinqubo ezifanayo
services-connected = Kuxhunyiwe
services-disconnected = Kunqanyuliwe
services-attached = kunamathiselwe
services-kill = Misa ngempoqo
services-memory = Imemori
services-size = Usayizi
services-shell = Shell

error-title = Iphutha

history-search = Sesha umlando
history-clear-all = Sula konke
history-clear-confirm = Sula wonke umlando?
history-clear-warning = Lokhu ngeke kubuyiselwe emuva.
history-cancel = Khansela
history-today = Namuhla
history-yesterday = Izolo
history-days-ago = Ezinsukwini ezingu-{ $count } ezedlule
history-day-offset = Usuku -{ $count }

settings-title = Izilungiselelo
settings-loading = Kulayishwa izilungiselelo…
settings-stored = Kugcinwe ku-~/.vmux/settings.ron
settings-other = Okunye
settings-software-update = Isibuyekezo Sesofthiwe
settings-check-updates = Hlola Izibuyekezo
settings-check-updates-hint = Ihlola ngokuzenzakalelayo uma kuqalwa futhi njalo ngehora uma i-Auto-update inikwe amandla.
settings-update-unavailable = Akutholakali
settings-update-unavailable-hint = Isibuyekezi asifakiwe kule build.
settings-update-checking = Kuyahlolwa…
settings-update-checking-hint = Kuhlolwa izibuyekezo…
settings-update-check-again = Hlola Futhi
settings-update-current = I-Vmux isesikhathini.
settings-update-downloading = Kuyalandwa…
settings-update-downloading-hint = Kulandwa i-Vmux { $version }…
settings-update-installing = Kuyafakwa…
settings-update-installing-hint = Kufakwa i-Vmux { $version }…
settings-update-ready = Isibuyekezo Silungile
settings-update-ready-hint = I-Vmux { $version } isilungile. Qalisa kabusha ukuze sisebenze.
settings-update-try-again = Zama Futhi
settings-update-failed = Ayikwazanga ukuhlola izibuyekezo.
settings-item = Into
settings-item-number = Into { $number }
settings-press-key = Cindezela ukhiye…
settings-saved = Kugciniwe
settings-record-key = Chofoza ukuze uqophe inhlanganisela entsha yokhiye

tray-open-window = Vula Iwindi
tray-close-window = Vala Iwindi
tray-pause-recording = Misa Isiqophi Kancane
tray-resume-recording = Qhubeka Nokuqopha
tray-finish-recording = Qeda Ukuqopha
tray-quit = Phuma ku-Vmux

composer-attach-files = Namathisela amafayela (/upload)
composer-remove-attachment = Susa okunamathiselwe

layout-back = Emuva
layout-forward = Phambili
layout-reload = Layisha kabusha
layout-bookmark-page = Faka leli khasi kumabhukhimakhi
layout-remove-bookmark = Susa ibhukhimakhi
layout-pin-page = Phina leli khasi
layout-unpin-page = Susa ukuphina leli khasi
layout-manage-extensions = Phatha izandiso
layout-new-stack = Isitaki Esisha
layout-close-tab = Vala ithebhu
layout-bookmark = Ibhukhimakhi
layout-pin = Phina
layout-new-tab = Ithebhu entsha
layout-team = Ithimba

command-switch-space = Shintsha indawo…
command-search-ask = Sesha noma ubuze…
command-new-tab-placeholder = Sesha noma thayipha i-URL, noma khetha i-Terminal…
command-placeholder = Thayipha i-URL, sesha amathebhu, noma > ukuze uthole imiyalo…
command-composer-placeholder = Thayipha / ukuze uthole imiyalo noma @ ukuze uthole imidiya
command-send = Thumela (Enter)
command-terminal = Terminal
command-open-terminal = Vula ku-Terminal
command-stack = Isitaki
command-tabs = { $count ->
    [one] ithebhu engu-1
   *[other] amathebhu angu-{ $count }
}
command-prompt = Umyalo
command-new-tab = Ithebhu entsha
command-search = Sesha
command-open-value = Vula “{ $value }”
command-search-value = Sesha “{ $value }”

schema-appearance = Ukubukeka
schema-general = Okuvamile
schema-layout = Isakhiwo
schema-layout-detail = Iwindi, amaphaneli, ibha eseceleni, nendandatho yokugxila.
schema-agent = I-ejenti
schema-agent-detail = Ukuziphatha kwe-ejenti nezimvume zamathuluzi.
schema-shortcuts = Izinqamuleli
schema-shortcuts-detail = Ukubuka kuphela. Hlela settings.ron ngqo ukuze ushintshe izibopho zokhiye.
schema-terminal = Terminal
schema-browser = Isiphequluli
schema-mode = Imodi
schema-mode-detail = Isikimu sombala samakhasi ewebhu. Idivayisi ilandela uhlelo lwakho.
schema-device = Idivayisi
schema-light = Okukhanyayo
schema-dark = Okumnyama
schema-language = Ulimi
schema-language-detail = Sebenzisa uhlelo, en-US, ja, noma noma iyiphi ithegi ye-BCP 47 enekhathalogi efanayo ethi ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Auto-update
schema-auto-update-detail = Hlola bese ufaka izibuyekezo uma kuqalwa futhi njalo ngehora.
schema-startup-url = I-URL yokuqalisa
schema-startup-url-detail = Uma kungenalutho kuvula umyalo webha yemiyalo.
schema-search-engine = Injini yokusesha
schema-search-engine-detail = Isetshenziselwa ukusesha iwebhu kusuka ku-Qala nakubha yemiyalo.
schema-window = Iwindi
schema-pane = Iphaneli
schema-side-sheet = Ishidi laseceleni
schema-focus-ring = Indandatho yokugxila
schema-run-placement = Vumela ukweqa ukubekwa kokusebenza
schema-run-placement-detail = Vumela ama-ejenti akhethe imodi yephaneli yokusebenza, inkomba, ne-ankile.
schema-leader = Umholi
schema-leader-detail = Ukhiye wesiqalo wezinqamuleli ze-chord.
schema-chord-timeout = Isikhathi sokuphela kwe-chord
schema-chord-timeout-detail = Amamilisekhondi ngaphambi kokuthi isiqalo se-chord siphelelwe isikhathi.
schema-bindings = Izibopho zokhiye
schema-confirm-close = Qinisekisa ukuvala
schema-confirm-close-detail = Cela ukuqinisekisa ngaphambi kokuvala i-terminal enenqubo eqhubekayo.
schema-default-theme = Itimu ezenzakalelayo
schema-default-theme-detail = Igama letimu esebenzayo ohlwini lwamatimu.

settings-empty = (akunalutho)
settings-none = (akukho)

schema-system = Isistimu
schema-editor = Umhleli
schema-recording = Ukurekhoda
schema-radius = Irediyasi
schema-padding = Isikhala sangaphakathi
schema-gap = Isikhala
schema-width = Ububanzi
schema-color = Umbala
schema-red = Obomvu
schema-green = Oluhlaza
schema-blue = Oluhlaza okwesibhakabhaka
schema-follow-files = Landela amafayela
schema-tidy-files = Hlanza amafayela
schema-tidy-files-max = Umkhawulo wokuhlanza amafayela
schema-tidy-files-auto = Hlanza amafayela ngokuzenzakalelayo
schema-app-providers = Abahlinzeki bohlelo lokusebenza
schema-provider = Umhlinzeki
schema-kind = Uhlobo
schema-models = Amamodeli
schema-acp = Ama-ejenti e-ACP
schema-id = ID
schema-name = Igama
schema-command = Umyalo
schema-arguments = Ama-agumenti
schema-environment = Okuguquguqukayo kwemvelo
schema-working-directory = Ifolda yokusebenza
schema-shell = Igobolondo
schema-font-family = Umndeni wefonti
schema-startup-directory = Ifolda yokuqalisa
schema-themes = Amatimu
schema-color-scheme = Isikimu sombala
schema-font-size = Usayizi wefonti
schema-line-height = Ukuphakama komugqa
schema-cursor-style = Isitayela sekhesa
schema-cursor-blink = Ukucwayiza kwekhesa
schema-custom-themes = Amatimu enziwe ngokwezifiso
schema-foreground = Okungaphambili
schema-background = Ingemuva
schema-cursor = Ikhesa
schema-ansi-colors = Imibala ye-ANSI
schema-keymap = Imephu yezinkinobho
schema-explorer = Isiphequluli
schema-visible = Kuyabonakala
schema-language-servers = Amaseva ezilimi
schema-servers = Amaseva
schema-language-id = ID yolimi
schema-root-markers = Omaka bempande
schema-output-directory = Ifolda yokukhipha

menu-scene = Isigcawu
menu-layout = Isakhiwo
menu-terminal = Itheminali
menu-browser = Isiphequluli
menu-service = Isevisi
menu-bookmark = Ibhukhimakhi
menu-edit = Hlela

layout-knowledge = Ulwazi
layout-open-knowledge = Vula Ulwazi
layout-open-welcome-knowledge = Vula okuthi Siyakwamukela kuLwazi
layout-open-path = Vula { $path }
layout-fold-knowledge = Songa ulwazi
layout-unfold-knowledge = Vula ulwazi
layout-bookmarks = Amabhukhimakhi
layout-new-folder = Ifolda Entsha
layout-add-to-bookmarks = Engeza Kumabhukhimakhi
layout-move-to-bookmarks = Hambisa Kumabhukhimakhi
layout-stack-number = Isitaki { $number }
layout-fold-stack = Songa isitaki
layout-unfold-stack = Vula isitaki
layout-close-stack = Vala isitaki
layout-bookmark-in = Bhukhimaka ku-{ $folder }

common-cancel = Khansela
common-delete = Susa
common-save = Londoloza
common-rename = Qamba kabusha
common-expand = Nweba
common-collapse = Goqa
common-loading = Iyalayisha…
common-error = Iphutha
common-output = Okuphumayo
common-pending = Kusalindile
common-current = kwamanje
common-stop = Misa
services-command = Isevisi ye-Vmux
services-uptime-seconds = { $seconds }s
services-uptime-minutes = { $minutes }m { $seconds }s
services-uptime-hours = { $hours }h { $minutes }m
services-uptime-days = { $days }d { $hours }h

error-page-failed-load = Ikhasi lehlulekile ukulayisha
error-page-not-found = Ikhasi alitholakali
error-unknown-host = Umsingathi wohlelo lokusebenza lwe-Vmux ongaziwa: { $host }

history-title = Umlando

command-new-app-chat = Ingxoxo entsha ye-{ $provider }/{ $model } (Uhlelo lokusebenza)
command-interactive-mode-user = Isigcawu > Imodi yokuxhumana > Umsebenzisi
command-interactive-mode-player = Isigcawu > Imodi yokuxhumana > Umdlali
command-minimize-window = Isakhiwo > Iwindi > Nciphisa
command-toggle-layout = Isakhiwo > Isakhiwo > Guqula isakhiwo
command-close-tab = Isakhiwo > Ithebhu > Vala ithebhu
command-new-task = Isakhiwo > Ithebhu > Umsebenzi omusha…
command-next-tab = Isakhiwo > Ithebhu > Ithebhu elandelayo
command-prev-tab = Isakhiwo > Ithebhu > Ithebhu eyedlule
command-rename-tab = Isakhiwo > Ithebhu > Qamba kabusha ithebhu
command-tab-select-1 = Isakhiwo > Ithebhu > Khetha ithebhu 1
command-tab-select-2 = Isakhiwo > Ithebhu > Khetha ithebhu 2
command-tab-select-3 = Isakhiwo > Ithebhu > Khetha ithebhu 3
command-tab-select-4 = Isakhiwo > Ithebhu > Khetha ithebhu 4
command-tab-select-5 = Isakhiwo > Ithebhu > Khetha ithebhu 5
command-tab-select-6 = Isakhiwo > Ithebhu > Khetha ithebhu 6
command-tab-select-7 = Isakhiwo > Ithebhu > Khetha ithebhu 7
command-tab-select-8 = Isakhiwo > Ithebhu > Khetha ithebhu 8
command-tab-select-last = Isakhiwo > Ithebhu > Khetha ithebhu yokugcina
command-close-pane = Isakhiwo > Ifasitelana > Vala ifasitelana
command-select-pane-left = Isakhiwo > Ifasitelana > Khetha ifasitelana lesokunxele
command-select-pane-right = Isakhiwo > Ifasitelana > Khetha ifasitelana lesokudla
command-select-pane-up = Isakhiwo > Ifasitelana > Khetha ifasitelana eliphezulu
command-select-pane-down = Isakhiwo > Ifasitelana > Khetha ifasitelana eliphansi
command-swap-pane-prev = Isakhiwo > Ifasitelana > Shintshanisa nefasitelana langaphambilini
command-swap-pane-next = Isakhiwo > Ifasitelana > Shintshanisa nefasitelana elilandelayo
command-equalize-pane-size = Isakhiwo > Ifasitelana > Linganisa usayizi wamafasitelana
command-resize-pane-left = Isakhiwo > Ifasitelana > Shintsha usayizi uye kwesokunxele
command-resize-pane-right = Isakhiwo > Ifasitelana > Shintsha usayizi uye kwesokudla
command-resize-pane-up = Isakhiwo > Ifasitelana > Shintsha usayizi uye phezulu
command-resize-pane-down = Isakhiwo > Ifasitelana > Shintsha usayizi uye phansi
command-stack-close = Isakhiwo > Isitaki > Vala isitaki
command-stack-next = Isakhiwo > Isitaki > Isitaki esilandelayo
command-stack-previous = Isakhiwo > Isitaki > Isitaki esedlule
command-stack-reopen = Isakhiwo > Isitaki > Phinda uvule ikhasi elivaliwe
command-stack-swap-prev = Isakhiwo > Isitaki > Hambisa isitaki kwesokunxele
command-stack-swap-next = Isakhiwo > Isitaki > Hambisa isitaki kwesokudla
command-space-open = Isakhiwo > Isikhala > Izikhala
command-terminal-close = Itheminali > Vala itheminali
command-terminal-next = Itheminali > Itheminali elandelayo
command-terminal-prev = Itheminali > Itheminali eyedlule
command-terminal-clear = Itheminali > Sula itheminali
command-browser-prev-page = Isiphequluli > Ukuzulazula > Emuva
command-browser-next-page = Isiphequluli > Ukuzulazula > Phambili
command-browser-reload = Isiphequluli > Ukuzulazula > Layisha kabusha
command-browser-hard-reload = Isiphequluli > Ukuzulazula > Layisha kabusha ngokuphelele
command-open-in-place = Isiphequluli > Vula > Vula lapha
command-open-in-new-stack = Isiphequluli > Vula > Vula esitakini esisha
command-open-in-pane-top = Isiphequluli > Vula > Vula kufasitelana elingenhla
command-open-in-pane-right = Isiphequluli > Vula > Vula kufasitelana langakwesokudla
command-open-in-pane-bottom = Isiphequluli > Vula > Vula kufasitelana elingezansi
command-open-in-pane-left = Isiphequluli > Vula > Vula kufasitelana langakwesokunxele
command-open-in-new-tab = Isiphequluli > Vula > Vula kuthebhu entsha
command-open-in-new-space = Isiphequluli > Vula > Vula esikhaleni esisha
command-browser-zoom-in = Isiphequluli > Ukubuka > Sondeza
command-browser-zoom-out = Isiphequluli > Ukubuka > Hlehlisa ukusondeza
command-browser-zoom-reset = Isiphequluli > Ukubuka > Usayizi wangempela
command-browser-dev-tools = Isiphequluli > Ukubuka > Amathuluzi onjiniyela
command-browser-open-command-bar = Isiphequluli > Ibha > Ibha yemiyalo
command-browser-open-page-in-command-bar = Isiphequluli > Ibha > Hlela ikhasi
command-browser-open-path-bar = Isiphequluli > Ibha > Isizulazuli sendlela
command-browser-open-commands = Isiphequluli > Ibha > Imiyalo
command-browser-open-history = Isiphequluli > Ibha > Umlando
command-service-open = Isevisi > Vula isiqaphi sesevisi
command-bookmark-toggle-active = Ibhukhimakhi > Bhukhimakha ikhasi
command-bookmark-pin-active = Ibhukhimakhi > Phina ikhasi

layout-tab = Ithebhu
layout-no-stacks = Azikho izitaki
layout-loading = Iyalayisha…
layout-no-markdown-files = Awekho amafayela e-Markdown
layout-empty-folder = Ifolda engenalutho
layout-worktree = isihlahla somsebenzi
layout-folder-name = Igama lefolda
layout-no-pins-bookmarks = Awekho amaphini noma amabhukhimakhi
layout-move-to = Hambisa ku-{ $folder }
layout-bookmark-current-page = Bhukhimakha ikhasi lamanje
layout-rename-folder = Qamba kabusha ifolda
layout-remove-folder = Susa ifolda
layout-update-downloading = Kulandwa isibuyekezo
layout-update-installing = Kufakwa isibuyekezo…
layout-update-ready = Inguqulo entsha iyatholakala
layout-restart-update = Qalisa kabusha ukuze ubuyekeze

agent-preparing = Kulungiswa i-ejenti…
agent-send-all-queued = Thumela yonke imiyalezo esemgqeni manje (Esc)
agent-send = Thumela (Enter)
agent-ready = Ngilungele uma usulungile.
agent-loading-older = Kulayishwa imiyalezo emidala…
agent-load-older = Layisha imiyalezo emidala
agent-continued-from = Kuqhubeke kusuka ku-{ $source }
agent-older-context-omitted = umongo omdala ushiyiwe
agent-interrupted = kuphazamisekile
agent-allow-tool = Vumela i-{ $tool }?
agent-deny = Yenqaba
agent-allow-always = Vumela njalo
agent-allow = Vumela
agent-loading-sessions = Kulayishwa amaseshini…
agent-no-resumable-sessions = Awekho amaseshini angaqhutshwa atholakele
agent-no-matching-sessions = Awekho amaseshini afanayo
agent-no-matching-models = Awekho amamodeli afanayo
agent-choice-help = ↑/↓ noma Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Khetha ifolda yenqolobane
agent-choose-repository-detail = Khetha inqolobane ye-Git yasendaweni ezosetshenziswa i-ejenti.
agent-choosing = Kuyakhethwa…
agent-choose-folder = Khetha ifolda
agent-queued = kusemgqeni
agent-attached = Okunamathiselwe:
agent-cancel-queued = Khansela umlayezo osemgqeni
agent-resume-queued = Qhuba imiyalezo esemgqeni
agent-clear-queue = Sula umugqa
agent-send-all-now = thumela konke manje
agent-choose-option = Khetha inketho ngenhla
agent-loading-media = Kulayishwa imidiya…
agent-no-matching-media = Ayikho imidiya efanayo
agent-prompt-context = Umongo womlayezo
agent-details = Imininingwane
agent-path = Indlela
agent-tool = Ithuluzi
agent-server = Iseva
agent-bytes = { $count } bytes
agent-worked-for = Isebenze isikhathi esingu-{ $duration }
agent-worked-for-steps = { $count ->
    [one] Isebenze isikhathi esingu-{ $duration } · isinyathelo esingu-1
   *[other] Isebenze isikhathi esingu-{ $duration } · izinyathelo ezingu-{ $count }
}
agent-tool-guardian-review = Ukubuyekezwa kwe-Guardian
agent-tool-read-files = Ifunde amafayela
agent-tool-viewed-image = Ibuke isithombe
agent-tool-used-browser = Isebenzise isiphequluli
agent-tool-searched-files = Iseshe amafayela
agent-tool-ran-commands = Isebenzise imiyalo
agent-thinking = Iyacabanga
agent-subagent = I-ejenti engaphansi
agent-prompt = Umlayezo
agent-thread = Uchungechunge
agent-parent = Umzali
agent-children = Izingane
agent-call = Ucingo
agent-raw-event = Isenzakalo esingacutshunguliwe
agent-plan = Uhlelo
agent-tasks = { $count ->
    [one] umsebenzi ongu-1
   *[other] imisebenzi engu-{ $count }
}
agent-edited = Kuhleliwe
agent-reconnecting = Ixhuma kabusha { $attempt }/{ $total }
agent-status-running = Iyasebenza
agent-status-done = Kuqediwe
agent-status-failed = Kuhlulekile
agent-status-pending = Kusalindile
agent-slash-attach-files = Namathisela amafayela
agent-slash-resume-session = Qhuba iseshini edlule
agent-slash-select-model = Khetha imodeli
agent-slash-continue-cli = Qhubeka nale seshini ku-CLI
agent-session-just-now = manje nje
agent-session-minutes-ago = emizuzwini engu-{ $count } edlule
agent-session-hours-ago = emahoreni angu-{ $count } edlule
agent-session-days-ago = ezinsukwini ezingu-{ $count } edlule
agent-working-working = Iyasebenza
agent-working-thinking = Iyacabanga
agent-working-pondering = Iyazindla
agent-working-noodling = Iyacabanga-cabanga
agent-working-percolating = Iyapheka kancane
agent-working-conjuring = Iyabumba
agent-working-cooking = Iyapheka
agent-working-brewing = Iyabilisa
agent-working-musing = Iyazindla
agent-working-ruminating = Iyahluzisisa
agent-working-scheming = Ihlela
agent-working-synthesizing = Ihlanganisa
agent-working-tinkering = Iyazama-zama
agent-working-churning = Iyanyakaza
agent-working-vibing = Isekugelezeni
agent-working-simmering = Iyabila kancane
agent-working-crafting = Yakha
agent-working-divining = Ihlwaya
agent-working-mulling = Iyakucabangisisa
agent-working-spelunking = Iphenya ngokujulile

editor-toggle-explorer = Guqula isihloli (Cmd+B)
editor-unsaved = akulondoloziwe
editor-rendered-markdown = I-Markdown ebonisiwe enokuhlela bukhoma
editor-note = Inothi
editor-source-editor = Isihleli somthombo
editor-editor = Isihleli
editor-git-diff = Umehluko we-Git
editor-diff = Umehluko
editor-tidy = Hlanza
editor-always = Njalo
editor-unchanged-previews = { $count ->
    [one] ✦ ukubuka kuqala okungu-1 okungashintshile
   *[other] ✦ ukubuka kuqala okungu-{ $count } okungashintshile
}
editor-open-externally = Vula ngaphandle
editor-changed-line = Umugqa oshintshiwe
editor-go-to-definition = Iya encazelweni
editor-find-references = Thola izinkomba
editor-references = { $count ->
    [one] inkomba engu-1
   *[other] izinkomba ezingu-{ $count }
}
editor-lsp-starting = { $server } iyaqala…
editor-lsp-not-installed = { $server } — ayifakiwe
editor-explorer = Isihloli
editor-open-editors = Izihleli ezivuliwe
editor-outline = Uhlaka
editor-new-file = Ifayela elisha
editor-new-folder = Ifolda entsha
editor-delete-confirm = Susa “{ $name }”? Lokhu akunakuhlehliswa.
editor-created-folder = Kudaliwe ifolda { $name }
editor-created-file = Kudaliwe ifayela { $name }
editor-renamed-to = Kuqanjwe kabusha kwaba { $name }
editor-deleted = Kususiwe { $name }
editor-failed-decode-image = Kuhlulekile ukuhumusha isithombe
editor-preview-large-image = isithombe (sikhulu kakhulu ukubuka kuqala)
editor-preview-binary = i-binary
editor-preview-file = ifayela

git-status-clean = kuhlanzekile
git-status-modified = kushintshiwe
git-status-staged = kulungisiwe
git-status-staged-modified = kulungisiwe*
git-status-untracked = akulandelelwa
git-status-deleted = kususiwe
git-status-conflict = ukungqubuzana
git-accept-all = ✓ yamukela konke
git-unstage = Khipha kokulungisiwe
git-confirm-deny-all = Qinisekisa ukwenqaba konke
git-deny-all = ✗ yenqaba konke
git-commit-message = umlayezo we-commit
git-commit = Commit ({ $count })
git-push = ↑ Phusha
git-loading-diff = Kulayishwa umehluko…
git-no-changes = Azikho izinguquko ezizoboniswa
git-accept = ✓ yamukela
git-deny = ✗ yenqaba
git-show-unchanged-lines = Bonisa imigqa engu-{ $count } engashintshile

terminal-loading = Iyalayisha…
terminal-runs-when-ready = isebenza uma isilungile · Ctrl+C iyasula · Esc iyeqa
terminal-booting = iyaqalisa
terminal-type-command = thayipha umyalo · isebenza uma isilungile · Esc iyeqa

setup-tagline-claude = I-ejenti yokubhala ikhodi ye-Anthropic, ku-Vmux
setup-tagline-codex = I-ejenti yokubhala ikhodi ye-OpenAI, ku-Vmux
setup-tagline-vibe = I-ejenti yokubhala ikhodi ye-Mistral, ku-Vmux
setup-install-title = Faka i-{ $name } CLI
setup-homebrew-required = I-Homebrew iyadingeka ukufaka i-{ $command } futhi ayikasethwa. I-Vmux izoqala ifake i-Homebrew, bese ifaka i-{ $name }.
setup-terminal-instructions = Kutheminali, cindezela u-Return ukuze uqale, bese ufaka iphasiwedi yakho ye-Mac uma ucelwa.
setup-command-missing = I-Vmux ivule leli khasi ngoba umyalo wendawo we-{ $command } awukafakwa. Qalisa umyalo ongezansi ukuze uwuthole.
setup-install-failed = Ukufaka akuphelanga. Hlola itheminali ukuze uthole imininingwane, bese uzama futhi.
setup-installing = Kuyafakwa…
setup-install-homebrew = Faka i-Homebrew + { $name }
setup-run-install = Qalisa umyalo wokufaka
setup-auto-reload = I-Vmux iyisebenzisa kutheminali futhi ilayisha kabusha uma i-{ $command } isilungile.

debug-title = Lungisa amaphutha
debug-auto-update = Ukubuyekeza okuzenzakalelayo
debug-simulate-update = Lingisa ukuthi isibuyekezo siyatholakala
debug-simulate-download = Lingisa ukulanda
debug-clear-update = Sula isibuyekezo
debug-trigger-restart = Qalisa ukuqalisa kabusha

command-manage-spaces = Phatha izikhala…
command-pane-stack-location = ifasitelana { $pane } / isitaki { $stack }
command-space-pane-stack-location = { $space } / ifasitelana { $pane } / isitaki { $stack }
command-terminal-path = Itheminali ({ $path })
command-group-interactive-mode = Imodi Yokusebenzisana
command-group-window = Iwindi
command-group-tab = Ithebhu
command-group-pane = Ifasitelana
command-group-stack = Isitaki
command-group-space = Isikhala
command-group-navigation = Ukuzulazula
command-group-open = Vula
command-group-view = Ukubuka
command-group-bar = Ibha

menu-close-vmux = Vala i-Vmux

agents-terminal-coding-agent = I-ejenti yokubhala ikhodi esebenzisa itheminali
