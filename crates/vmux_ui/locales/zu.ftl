common-open = Vula
common-close = Vala
common-install = Faka
common-uninstall = Susa
common-update = Buyekeza
common-retry = Zama futhi
common-refresh = Vuselela
common-remove = Susa
common-enable = Vumela
common-disable = Khubaza
common-new = Okusha
common-active = kusebenza
common-running = kugijima
common-done = kuqediwe
common-failed = Yehlulekile
common-installed = Kufakiwe
common-items = { $count ->
    [one] { $count } into
   *[other] { $count } izinto
}
start-title = Qala
start-tagline = Isimemo esisodwa. Noma ini, kwenziwe.

agents-title = Amagents
agents-search = Sesha amagents e-ACP ne-CLI…
agents-empty = Alikho igent elifanayo
agents-empty-detail = Zama igama, i-runtime, noma i-ACP/CLI.
agents-install-failed = Ukufaka kwehlulekile
agents-updating = Iyabuyekeza…
agents-retrying = Iyazama futhi…
agents-preparing = Ilungisa…

extensions-title = Izandiso
extensions-search = Sesha izandiso ezifakiwe noma i-Chrome Web Store…
extensions-relaunch = Qala kabusha ukuze kusebenze
extensions-empty = Azikho izandiso ezifakiwe
extensions-no-match = Azikho izandiso ezifanayo
extensions-empty-detail = Sesha i-Chrome Web Store ngenhla bese ucindezela Return.
extensions-no-match-detail = Zama elinye igama noma i-ID yesandiso.
extensions-on = Vuliwe
extensions-off = Valiwe
extensions-enable-confirm = Vumela { $name }?
extensions-enable-permissions = Vumela { $name } unikeze:

lsp-title = Amaserver Olimi
lsp-search = Sesha amaserver olimi, izilinter, iziformathi…
lsp-loading = Iyalayisha ikhathalogu…
lsp-empty = Alikho iserver lolimi elifanayo
lsp-empty-detail = Zama olunye ulimi, isilinter, noma isiformathi.
lsp-needs = idinga { $tool }
lsp-status-available = Iyatholakala
lsp-status-on-path = Ku-PATH
lsp-status-installing = Iyafaka…
lsp-status-installed = Kufakiwe
lsp-status-outdated = Ibuyekezo litholakala
lsp-status-running = Iyagijima
lsp-status-failed = Yehlulekile

spaces-title = Izindawo
spaces-new-placeholder = Igama lendawo entsha
spaces-empty = Azikho izindawo
spaces-default-name = Indawo { $number }
spaces-tabs = { $count ->
    [one] 1 ithebhu
   *[other] { $count } amathebu
}
spaces-delete = Susa indawo

team-title = Ithimba
team-just-you = Wena wedwa kule ndawo
team-agents = { $count ->
    [one] Wena ne-agent elilodwa
   *[other] Wena namagents angu-{ $count }
}
team-empty = Akakho muntu lapha
team-you = Wena
team-agent = I-Agent

services-title = Amasevisi Angemuva
services-processes = { $count ->
    [one] 1 inqubo
   *[other] { $count } izinqubo
}
services-kill-all = Bulala Konke
services-not-running = Isevisi ayigijimi
services-start-with = Qala nge:
services-empty = Azikho izinqubo ezisebenzayo
services-filter = Hlunga izinqubo…
services-no-match = Azikho izinqubo ezifanayo
services-connected = Ixhunyiwe
services-disconnected = Ikaqhunyiwe
services-attached = ixhunyiwe
services-kill = Bulala
services-memory = Inkumbulo
services-size = Usayizi
services-shell = I-Shell

error-title = Iphutha

history-search = Sesha umlando
history-clear-all = Sula konke
history-clear-confirm = Sula umlando wonke?
history-clear-warning = Lokhu akukwazi ukubuyiswa.
history-cancel = Khansela
history-today = Namuhla
history-yesterday = Izolo
history-days-ago = Izinsuku ezingu-{ $count } ezedlule
history-day-offset = Usuku -{ $count }

settings-title = Izilungiselelo
settings-loading = Iyalayisha izilungiselelo…
settings-stored = Igcinwe ku-~/.vmux/settings.ron
settings-other = Okunye
settings-software-update = Ibuyekezo Lesoftware
settings-check-updates = Hlola Izibuyekezo
settings-check-updates-hint = Ihlola ngokuzenzekelayo uma kuqalisa nangehora ngalinye uma i-Auto-update ivuliwe.
settings-update-unavailable = Ayitholakali
settings-update-unavailable-hint = Isidluli sibuyekezo asifakiwe kulolu kuhlanganisa.
settings-update-checking = Ihlola…
settings-update-checking-hint = Ihlola izibuyekezo…
settings-update-check-again = Hlola Futhi
settings-update-current = Vmux isibuyekeziwe.
settings-update-downloading = Iyalanda…
settings-update-downloading-hint = Iyalanda Vmux { $version }…
settings-update-installing = Iyafaka…
settings-update-installing-hint = Iyafaka Vmux { $version }…
settings-update-ready = Ibuyekezo Lilungile
settings-update-ready-hint = Vmux { $version } ilungile. Qala kabusha ukuze usebenzise.
settings-update-try-again = Zama Futhi
settings-update-failed = Yehlulekile ukuhlola izibuyekezo.
settings-item = Into
settings-item-number = Into { $number }
settings-press-key = Cindezela isihluthulelo…
settings-saved = Kugcinwe
settings-record-key = Chofoza ukurekhoda uhlanganiso lwesihluthulelo olusha

tray-open-window = Vula Iwindi
tray-close-window = Vala Iwindi
tray-pause-recording = Misa Ukurekhoda
tray-resume-recording = Qhubeka Nokurekhoda
tray-finish-recording = Qeda Ukurekhoda
tray-quit = Phuma ku-Vmux

composer-attach-files = Namathelisa amafayela (/upload)
composer-remove-attachment = Susa ukuphakanyiswa

layout-back = Emuva
layout-forward = Phambili
layout-reload = Layisha kabusha
layout-bookmark-page = Beka uphawu kuleli khasi
layout-remove-bookmark = Susa uphawu
layout-pin-page = Phina leli khasi
layout-unpin-page = Susa iphini kuleli khasi
layout-manage-extensions = Phatha izandiso
layout-new-stack = Isitaki Esisha
layout-close-tab = Vala ithebhu
layout-bookmark = Uphawu
layout-pin = Iphini
layout-new-tab = Ithebhu entsha
layout-team = Ithimba

command-switch-space = Shintsha indawo…
command-search-ask = Sesha noma ubuze…
command-new-tab-placeholder = Sesha noma ufake i-URL, noma ukhethe I-Terminal…
command-placeholder = Faka i-URL, sesha amathebu, noma > ngemiyalo…
command-composer-placeholder = Faka / ngemiyalo noma @ kwezomfani
command-send = Thumela (Enter)
command-terminal = I-Terminal
command-open-terminal = Vula ku-Terminal
command-stack = Isitaki
command-tabs = { $count ->
    [one] 1 ithebhu
   *[other] { $count } amathebu
}
command-prompt = Isimemo
command-new-tab = Ithebhu entsha
command-search = Sesha
command-open-value = Vula "{ $value }"
command-search-value = Sesha "{ $value }"

schema-appearance = Ukubonakala
schema-general = Okuvamile
schema-layout = Isikhaka
schema-layout-detail = Iwindi, izingcezu, umkhala, nendandatho yokugxila.
schema-agent = I-Agent
schema-agent-detail = Ukuziphatha kwe-agent nezimvume zamathuluzi.
schema-shortcuts = Izinqamuleli
schema-shortcuts-detail = Umbono wokufunda kuphela. Hlela settings.ron ngokuqondile ukushintsha iziboshiwe.
schema-terminal = I-Terminal
schema-browser = Isiphequluli
schema-mode = Imodi
schema-mode-detail = Isema semibala kumawebhu. Idivayisi ilandela uhlelo lwakho.
schema-device = Idivayisi
schema-light = Ukukhanya
schema-dark = Ubumnyama
schema-language = Ulimi
schema-language-detail = Sebenzisa uhlelo, en-US, ja, noma ikhodi ye-BCP 47 enomhlanganiso ofanele we-~/.vmux/locales/<tag>.ftl.
schema-auto-update = Ibuyekezo Eliqhubekayo
schema-auto-update-detail = Hlola futhi ufake izibuyekezo uma kuqalisa nangehora ngalinye.
schema-startup-url = I-URL Yokuqala
schema-startup-url-detail = Ukungagcwalisi kuvula isimemo somudluli wemiyalo.
schema-search-engine = Injini yokusesha
schema-search-engine-detail = Isetshenziswa ukusesha iwebhu kusuka ku-Start nakumudluli wemiyalo.
schema-window = Iwindi
schema-pane = Ingcezu
schema-side-sheet = Ikhasi Elingakuhlangothi
schema-focus-ring = Indandatho Yokugxila
schema-run-placement = Vumela ukushintshwa kokubeka kwegijimo
schema-run-placement-detail = Vumela amagents ukukhetha imodi yephani legijimo, ukhomba, nesikhungo.
schema-leader = Umholi
schema-leader-detail = Isihluthulelo sokuqala sezinqamuleli zamazwi.
schema-chord-timeout = Isikhathi Sewadi
schema-chord-timeout-detail = Amamilisekendi ngaphambi kokuphela kwesihluthulelo sokuqala sewadi.
schema-bindings = Iziboshiwe
schema-confirm-close = Qinisekisa ukuvala
schema-confirm-close-detail = Buza ngaphambi kokuvala i-terminal ene-process egijimayo.
schema-default-theme = Ithemu Ezenzakalelayo
schema-default-theme-detail = Igama lethemu elisebenzayo ohlelweni lwamathemu.
