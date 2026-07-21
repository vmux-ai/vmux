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
