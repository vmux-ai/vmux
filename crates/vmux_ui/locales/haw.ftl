locale-name = ʻŌlelo Hawaiʻi
common-open = Wehe
common-close = Pani
common-install = Hoʻouka
common-uninstall = Wehe hoʻouka
common-update = Hoʻohou
common-retry = Hoʻāʻo hou
common-refresh = Hōʻano hou
common-remove = Wehe
common-enable = Hoʻā
common-disable = Hoʻopio
common-new = Hou
common-active = ʻeleu
common-running = ke holo nei
common-done = pau
common-failed = Ua hāʻule
common-installed = Ua hoʻouka ʻia
common-items = { $count ->
    [one] { $count } mea
   *[other] { $count } mea
}

tools-title = Nā mea hana
tools-search = Huli i nā pūʻolo, nā ʻelele, MCP, nā mea hana ʻōlelo a me nā waihona hoʻonohonoho…
tools-open = E wehe i nā mea hana
tools-fold = E pelu i nā mea hana
tools-unfold = E wehe aʻe i nā mea hana
tools-scanning = Ke nānā nei i nā mea hana kūloko…
tools-no-installed = ʻAʻohe mea hana i hoʻouka ʻia
tools-empty = ʻAʻohe mea hana kūpono
tools-empty-detail = E hoʻouka i pūʻolo a i ʻole e hoʻohui i pūʻolo waihona hoʻonohonoho ʻano Stow.
tools-apply = Hoʻohana
tools-homebrew = Homebrew
tools-homebrew-sync = Hoʻopili ʻakomi ʻia nā kumumanaʻo a me nā polokalamu i hoʻouka ʻia.
tools-open-brewfile = E wehe i ka Brewfile
tools-managed = mālama ʻia
tools-provider-homebrew-formulae = Nā kumumanaʻo Homebrew
tools-provider-homebrew-casks = Nā polokalamu Homebrew
tools-provider-npm = Nā pūʻolo npm
tools-provider-acp-agents = Nā ʻelele ACP
tools-provider-language-tools = Nā mea hana ʻōlelo
tools-provider-mcp-servers = Nā kikowaena MCP
tools-provider-dotfiles = Nā waihona hoʻonohonoho
tools-status-available = Loaʻa
tools-status-missing = Nalo
tools-status-conflict = Kūʻē
tools-forget = Poina
tools-manage = Hoʻoponopono
tools-link = Hoʻohui
tools-unlink = Wehe i ka hoʻohui
tools-import = Hoʻokomo mai
tools-update-count = { $count ->
    [one] 1 hōʻano hou
   *[other] { $count } hōʻano hou
}
tools-conflict-count = { $count ->
    [one] 1 kūʻē
   *[other] { $count } kūʻē
}
tools-result-applied = Ua hoʻohana ʻia nā mea hana
tools-result-imported = Ua hoʻokomo ʻia nā mea hana
tools-result-installed = Ua hoʻouka ʻia { $name }
tools-result-updated = Ua hōʻano hou ʻia { $name }
tools-result-uninstalled = Ua wehe ʻia { $name }
tools-result-forgotten = Ua poina ʻia { $name }
tools-result-managed = Ke mālama ʻia nei { $name }
tools-result-linked = Ua hoʻohui ʻia { $name }
tools-result-unlinked = Ua wehe ʻia ka hoʻohui o { $name }
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Hoʻonohonoho i nā hoʻonohonoho, nā mea hana, nā dotfiles, a me ka ʻike me Git.
vault-sync = Hoʻopili
vault-create = Hana
vault-connect = Hoʻohui
vault-private = Waihona pilikino
vault-public-warning = Hōʻike nā waihona lehulehu i kāu ʻike a me ka hoʻonohonoho.
vault-choose-repository = E koho i kahi waihona…
vault-empty = nele
vault-clean = ʻikepili hou āpau
vault-not-connected = ʻAʻole pili
vault-change-count = Nā hoʻololi: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = Hoʻomaka
start-tagline = Hoʻokahi prompt. Pau nā mea a pau.

agents-title = Nā ʻākena
agents-search = Huli i nā ʻākena ACP a me CLI…
agents-empty = ʻAʻohe ʻākena kūlike
agents-empty-detail = E hoʻāʻo i ka inoa, ke runtime, a i ʻole ACP/CLI.
agents-install-failed = ʻAʻole i hoʻouka ʻia
agents-updating = Ke hoʻohou nei…
agents-retrying = Ke hoʻāʻo hou nei…
agents-preparing = Ke hoʻomākaukau nei…

extensions-title = Nā mea hoʻolōʻihi
extensions-search = Huli i nā mea i hoʻouka ʻia a i ʻole Chrome Web Store…
extensions-relaunch = Hoʻomaka hou no ka hoʻokō
extensions-empty = ʻAʻohe mea hoʻolōʻihi i hoʻouka ʻia
extensions-no-match = ʻAʻohe mea hoʻolōʻihi kūlike
extensions-empty-detail = Huli ma Chrome Web Store ma luna a pēhi iā Return.
extensions-no-match-detail = E hoʻāʻo i inoa ʻē aʻe a i ʻole ID mea hoʻolōʻihi.
extensions-on = ʻĀ
extensions-off = Pio
extensions-enable-confirm = Hoʻā iā { $name }?
extensions-enable-permissions = Hoʻā iā { $name } a ʻae i kēia:

lsp-title = Nā kikowaena ʻōlelo
lsp-search = Huli i nā kikowaena ʻōlelo, linters, formatters…
lsp-loading = Ke hoʻouka nei i ka papa helu…
lsp-empty = ʻAʻohe kikowaena ʻōlelo kūlike
lsp-empty-detail = E hoʻāʻo i ʻōlelo, linter, a i ʻole formatter ʻē aʻe.
lsp-needs = pono iā { $tool }
lsp-status-available = Loaʻa
lsp-status-on-path = Ma PATH
lsp-status-installing = Ke hoʻouka nei…
lsp-status-installed = Ua hoʻouka ʻia
lsp-status-outdated = Loaʻa ka hōʻano hou
lsp-status-running = Ke holo nei
lsp-status-failed = Ua hāʻule

spaces-title = Nā wahi
spaces-new-placeholder = Inoa wahi hou
spaces-empty = ʻAʻohe wahi
spaces-default-name = Wahi { $number }
spaces-tabs = { $count ->
    [one] 1 kapu
   *[other] { $count } kapu
}
spaces-delete = Holoi i ka wahi

team-title = Kime
team-just-you = ʻO ʻoe wale nō ma kēia wahi
team-agents = { $count ->
    [one] ʻO ʻoe a me 1 ʻākena
   *[other] ʻO ʻoe a me { $count } ʻākena
}
team-empty = ʻAʻohe mea ma ʻaneʻi i kēia manawa
team-you = ʻO ʻoe
team-agent = ʻĀkena

services-title = Nā lawelawe hope
services-processes = { $count ->
    [one] 1 kaʻina
   *[other] { $count } kaʻina
}
services-kill-all = Hoʻopau ikaika i nā mea a pau
services-not-running = ʻAʻole holo ka lawelawe
services-start-with = Hoʻomaka me:
services-empty = ʻAʻohe kaʻina ʻeleu
services-filter = Kānana i nā kaʻina…
services-no-match = ʻAʻohe kaʻina kūlike
services-connected = Pili
services-disconnected = Moku
services-attached = hoʻopili ʻia
services-kill = Hoʻopau ikaika
services-memory = Hoʻomanaʻo
services-size = Nui
services-shell = Shell

error-title = Hewa

history-search = Huli mōʻaukala
history-clear-all = Holoi pau
history-clear-confirm = Holoi i ka mōʻaukala a pau?
history-clear-warning = ʻAʻole hiki ke hoʻihoʻi.
history-cancel = Hoʻōki
history-today = I kēia lā
history-yesterday = I nehinei
history-days-ago = { $count } lā i hala
history-day-offset = Lā -{ $count }

settings-title = Nā hoʻonohonoho
settings-loading = Ke hoʻouka nei i nā hoʻonohonoho…
settings-stored = Waiho ʻia ma ~/.vmux/settings.ron
settings-other = ʻĒ aʻe
settings-software-update = Hōʻano hou lako polokalamu
settings-check-updates = Nānā i nā hōʻano hou
settings-check-updates-hint = Nānā ʻakomi ke hoʻomaka a i kēlā me kēia hola inā hoʻā ʻia ka Hoʻohou ʻakomi.
settings-update-unavailable = ʻAʻole loaʻa
settings-update-unavailable-hint = ʻAʻole komo ka mea hoʻohou i kēia kūkulu.
settings-update-checking = Ke nānā nei…
settings-update-checking-hint = Ke nānā nei i nā hōʻano hou…
settings-update-check-again = Nānā hou
settings-update-current = Ua hou loa ʻo Vmux.
settings-update-downloading = Ke hoʻoiho nei…
settings-update-downloading-hint = Ke hoʻoiho nei iā Vmux { $version }…
settings-update-installing = Ke hoʻouka nei…
settings-update-installing-hint = Ke hoʻouka nei iā Vmux { $version }…
settings-update-ready = Mākaukau ka hōʻano hou
settings-update-ready-hint = Mākaukau ʻo Vmux { $version }. Hoʻomaka hou no ka hoʻokō.
settings-update-try-again = Hoʻāʻo hou
settings-update-failed = ʻAʻole hiki ke nānā i nā hōʻano hou.
settings-item = Mea
settings-item-number = Mea { $number }
settings-press-key = Pēhi i kekahi kī…
settings-saved = Mālama ʻia
settings-record-key = Kaomi no ka hoʻopaʻa ʻana i hui kī hou

tray-open-window = Wehe pukaaniani
tray-close-window = Pani pukaaniani
tray-pause-recording = Hoʻomaha i ka hoʻopaʻa ʻana
tray-resume-recording = Hoʻomau i ka hoʻopaʻa ʻana
tray-finish-recording = Hoʻopau i ka hoʻopaʻa ʻana
tray-quit = Haʻalele iā Vmux

composer-attach-files = Hoʻopili i nā waihona (/upload)
composer-remove-attachment = Wehe i ka hoʻopili

layout-back = Hoʻi
layout-forward = Imua
layout-reload = Hoʻouka hou
layout-bookmark-page = Kaha puke i kēia ʻaoʻao
layout-remove-bookmark = Wehe i ke kaha puke
layout-pin-page = Pin i kēia ʻaoʻao
layout-unpin-page = Wehe pin i kēia ʻaoʻao
layout-manage-extensions = Mālama i nā mea hoʻolōʻihi
layout-new-stack = Ahu hou
layout-close-tab = Pani i ke kapu
layout-bookmark = Kaha puke
layout-pin = Pin
layout-new-tab = Kapu hou
layout-team = Kime

command-switch-space = Kuapo wahi…
command-search-ask = Huli a nīnau…
command-new-tab-placeholder = Huli a kikokiko i URL, a i ʻole koho iā Terminal…
command-placeholder = Kikokiko i URL, huli kapu, a i ʻole > no nā kauoha…
command-composer-placeholder = Kikokiko / no nā kauoha a i ʻole @ no ka pāpaho
command-send = Hoʻouna (Enter)
command-terminal = Terminal
command-open-terminal = Wehe ma Terminal
command-stack = Ahu
command-tabs = { $count ->
    [one] 1 kapu
   *[other] { $count } kapu
}
command-prompt = Prompt
command-new-tab = Kapu hou
command-search = Huli
command-open-value = Wehe iā “{ $value }”
command-search-value = Huli iā “{ $value }”

schema-appearance = ʻIke maka
schema-general = Laulā
schema-layout = Hoʻolālā
schema-layout-detail = Pukaaniani, nā māhele, ʻaoʻao kōkua, a me ke apo kālele.
schema-agent = ʻĀkena
schema-agent-detail = Ka hana a ka ʻākena a me nā ʻae mea hana.
schema-shortcuts = Nā pōkole
schema-shortcuts-detail = Nānā wale. Hoʻoponopono pololei iā settings.ron no ka hoʻololi ʻana i nā paʻa kī.
schema-terminal = Terminal
schema-browser = Mākaʻikaʻi
schema-mode = ʻAno
schema-mode-detail = Hoʻolālā waihoʻoluʻu no nā ʻaoʻao pūnaewele. Hahai ka hāmeʻa i kāu ʻōnaehana.
schema-device = Hāmeʻa
schema-light = Mālamalama
schema-dark = Pouli
schema-language = ʻŌlelo
schema-language-detail = Hoʻohana i ka ʻōnaehana, en-US, ja, a i ʻole kekahi hōʻailona BCP 47 me ka papa ʻōlelo ~/.vmux/locales/<tag>.ftl kūlike.
schema-auto-update = Hoʻohou ʻakomi
schema-auto-update-detail = Nānā a hoʻouka i nā hōʻano hou ke hoʻomaka a i kēlā me kēia hola.
schema-startup-url = URL hoʻomaka
schema-startup-url-detail = Inā hakahaka, wehe ʻia ka prompt o ka pahu kauoha.
schema-search-engine = ʻEnekini huli
schema-search-engine-detail = Hoʻohana ʻia no nā huli pūnaewele mai Hoʻomaka a me ka pahu kauoha.
schema-window = Pukaaniani
schema-pane = Māhele
schema-side-sheet = Pepa ʻaoʻao
schema-focus-ring = Apo kālele
schema-run-placement = ʻAe i ka hoʻokahuli wahi holo
schema-run-placement-detail = ʻAe i nā ʻākena e koho i ke ʻano māhele holo, ke kuhikuhi, a me ka heleuma.
schema-leader = Alakaʻi
schema-leader-detail = Kī mua no nā pōkole chord.
schema-chord-timeout = Manawa pau chord
schema-chord-timeout-detail = Nā millisecond ma mua o ka pau ʻana o kahi mua chord.
schema-bindings = Nā paʻa kī
schema-confirm-close = Hōʻoia ma mua o ka pani
schema-confirm-close-detail = Nīnau ma mua o ka pani ʻana i terminal me kahi kaʻina e holo nei.
schema-default-theme = Kumuhana paʻamau
schema-default-theme-detail = Inoa o ke kumuhana ʻeleu mai ka papa kumuhana.

settings-empty = (hakahaka)
settings-none = (ʻaʻohe)

schema-system = ʻŌnaehana
schema-editor = Hoʻoponopono
schema-recording = Hoʻopaʻa
schema-radius = Pōʻai kihi
schema-padding = Kāʻei loko
schema-gap = Kowa
schema-width = Laulā
schema-color = Waihoʻoluʻu
schema-red = ʻUlaʻula
schema-green = ʻŌmaʻomaʻo
schema-blue = Polū
schema-follow-files = Hahai i nā faila
schema-tidy-files = Hoʻomaʻemaʻe i nā faila
schema-tidy-files-max = Palena hoʻomaʻemaʻe faila
schema-tidy-files-auto = Hoʻomaʻemaʻe ʻakomi i nā faila
schema-app-providers = Nā mea hoʻolako polokalamu
schema-provider = Mea hoʻolako
schema-kind = ʻAno
schema-models = Nā kükohu
schema-acp = Nā ʻākena ACP
schema-id = ID
schema-name = Inoa
schema-command = Kauoha
schema-arguments = Nā ʻāpana kauoha
schema-environment = Nā loli kaiapuni
schema-working-directory = Papa kuhikuhi hana
schema-shell = Pūpū
schema-font-family = ʻOhana kinona hua
schema-startup-directory = Papa kuhikuhi hoʻomaka
schema-themes = Nā kumuhana
schema-color-scheme = Papahana waihoʻoluʻu
schema-font-size = Nui kinona hua
schema-line-height = Kiʻekiʻe laina
schema-cursor-style = Kaila kuhi
schema-cursor-blink = ʻAnapu kuhi
schema-custom-themes = Nā kumuhana hoʻopilikino
schema-foreground = Ili mua
schema-background = Kāʻei kua
schema-cursor = Kuhi
schema-ansi-colors = Nā waihoʻoluʻu ANSI
schema-keymap = Palapala kī
schema-explorer = Mea mākaʻikaʻi
schema-visible = ʻIke ʻia
schema-language-servers = Nā kikowaena ʻōlelo
schema-servers = Nā kikowaena
schema-language-id = ID ʻōlelo
schema-root-markers = Nā māka mole
schema-output-directory = Papa kuhikuhi hoʻopuka

menu-scene = Hiʻohiʻona
menu-layout = Hoʻonohonoho
menu-terminal = Kahua kauoha
menu-browser = Mea huli pūnaewele
menu-service = Lawelawe
menu-bookmark = Lepe puke
menu-edit = Hoʻoponopono

layout-knowledge = ʻIke
layout-open-knowledge = Wehe i ka ʻIke
layout-open-welcome-knowledge = Wehe i ka Welina i ka ʻIke
layout-open-path = Wehe iā { $path }
layout-fold-knowledge = Pelu i ka ʻike
layout-unfold-knowledge = Wehe i ka ʻike
layout-bookmarks = Nā lepe puke
layout-new-folder = Waihona hou
layout-add-to-bookmarks = Hoʻohui i nā lepe puke
layout-move-to-bookmarks = Neʻe i nā lepe puke
layout-stack-number = Pūʻulu { $number }
layout-fold-stack = Pelu i ka pūʻulu
layout-unfold-stack = Wehe i ka pūʻulu
layout-close-stack = Pani i ka pūʻulu
layout-bookmark-in = Lepe puke ma { $folder }

common-cancel = Hoʻopau
common-delete = Holoi
common-save = Mālama
common-rename = Kapa hou
common-expand = Hoʻonui
common-collapse = Hoʻēmi
common-loading = Ke hoʻouka nei…
common-error = Hewa
common-output = Puka
common-pending = Ke kali nei
common-current = i kēia manawa
common-stop = Kāohi
services-command = lawelawe Vmux
services-uptime-seconds = { $seconds } kek.
services-uptime-minutes = { $minutes } min. { $seconds } kek.
services-uptime-hours = { $hours } hola { $minutes } min.
services-uptime-days = { $days } lā { $hours } hola

error-page-failed-load = ʻAʻole i hoʻouka ʻia ka ʻaoʻao
error-page-not-found = ʻAʻole i loaʻa ka ʻaoʻao
error-unknown-host = Mea hoʻokipa polokalamu Vmux ʻike ʻole ʻia: { $host }

history-title = Mōʻaukala

command-new-app-chat = Kamaʻilio { $provider }/{ $model } hou (Polokalamu)
command-interactive-mode-user = Kahua > ʻAno pilina > Mea hoʻohana
command-interactive-mode-player = Kahua > ʻAno pilina > Mea pāʻani
command-minimize-window = Hoʻonohonoho > Puka aniani > Hoʻēmi
command-toggle-layout = Hoʻonohonoho > Hoʻonohonoho > Hoʻololi hoʻonohonoho
command-close-tab = Hoʻonohonoho > ʻAoʻao kau > Pani ʻaoʻao kau
command-new-task = Hoʻonohonoho > ʻAoʻao kau > Hana hou…
command-next-tab = Hoʻonohonoho > ʻAoʻao kau > ʻAoʻao kau aʻe
command-prev-tab = Hoʻonohonoho > ʻAoʻao kau > ʻAoʻao kau mua
command-rename-tab = Hoʻonohonoho > ʻAoʻao kau > Kapa hou i ka ʻaoʻao kau
command-tab-select-1 = Hoʻonohonoho > ʻAoʻao kau > Koho i ka ʻaoʻao kau 1
command-tab-select-2 = Hoʻonohonoho > ʻAoʻao kau > Koho i ka ʻaoʻao kau 2
command-tab-select-3 = Hoʻonohonoho > ʻAoʻao kau > Koho i ka ʻaoʻao kau 3
command-tab-select-4 = Hoʻonohonoho > ʻAoʻao kau > Koho i ka ʻaoʻao kau 4
command-tab-select-5 = Hoʻonohonoho > ʻAoʻao kau > Koho i ka ʻaoʻao kau 5
command-tab-select-6 = Hoʻonohonoho > ʻAoʻao kau > Koho i ka ʻaoʻao kau 6
command-tab-select-7 = Hoʻonohonoho > ʻAoʻao kau > Koho i ka ʻaoʻao kau 7
command-tab-select-8 = Hoʻonohonoho > ʻAoʻao kau > Koho i ka ʻaoʻao kau 8
command-tab-select-last = Hoʻonohonoho > ʻAoʻao kau > Koho i ka ʻaoʻao kau hope
command-close-pane = Hoʻonohonoho > Māhele > Pani māhele
command-select-pane-left = Hoʻonohonoho > Māhele > Koho i ka māhele hema
command-select-pane-right = Hoʻonohonoho > Māhele > Koho i ka māhele ʻākau
command-select-pane-up = Hoʻonohonoho > Māhele > Koho i ka māhele luna
command-select-pane-down = Hoʻonohonoho > Māhele > Koho i ka māhele lalo
command-swap-pane-prev = Hoʻonohonoho > Māhele > Kūapo me ka māhele mua
command-swap-pane-next = Hoʻonohonoho > Māhele > Kūapo me ka māhele aʻe
command-equalize-pane-size = Hoʻonohonoho > Māhele > Hoʻokaulike nui māhele
command-resize-pane-left = Hoʻonohonoho > Māhele > Hoʻololi nui i ka hema
command-resize-pane-right = Hoʻonohonoho > Māhele > Hoʻololi nui i ka ʻākau
command-resize-pane-up = Hoʻonohonoho > Māhele > Hoʻololi nui i luna
command-resize-pane-down = Hoʻonohonoho > Māhele > Hoʻololi nui i lalo
command-stack-close = Hoʻonohonoho > Ahu > Pani ahu
command-stack-next = Hoʻonohonoho > Ahu > Ahu aʻe
command-stack-previous = Hoʻonohonoho > Ahu > Ahu mua
command-stack-reopen = Hoʻonohonoho > Ahu > Wehe hou i ka ʻaoʻao i pani ʻia
command-stack-swap-prev = Hoʻonohonoho > Ahu > Neʻe i ka ahu i ka hema
command-stack-swap-next = Hoʻonohonoho > Ahu > Neʻe i ka ahu i ka ʻākau
command-space-open = Hoʻonohonoho > Wahi > Nā wahi
command-terminal-close = Kahua kauoha > Pani kahua kauoha
command-terminal-next = Kahua kauoha > Kahua kauoha aʻe
command-terminal-prev = Kahua kauoha > Kahua kauoha mua
command-terminal-clear = Kahua kauoha > Holoi kahua kauoha
command-browser-prev-page = Pūnaewele > Hoʻokele > Hope
command-browser-next-page = Pūnaewele > Hoʻokele > Imua
command-browser-reload = Pūnaewele > Hoʻokele > Hoʻouka hou
command-browser-hard-reload = Pūnaewele > Hoʻokele > Hoʻouka hou piha
command-open-in-place = Pūnaewele > Wehe > Wehe ma ʻaneʻi
command-open-in-new-stack = Pūnaewele > Wehe > Wehe ma ahu hou
command-open-in-pane-top = Pūnaewele > Wehe > Wehe ma ka māhele luna
command-open-in-pane-right = Pūnaewele > Wehe > Wehe ma ka māhele ʻākau
command-open-in-pane-bottom = Pūnaewele > Wehe > Wehe ma ka māhele lalo
command-open-in-pane-left = Pūnaewele > Wehe > Wehe ma ka māhele hema
command-open-in-new-tab = Pūnaewele > Wehe > Wehe ma ʻaoʻao kau hou
command-open-in-new-space = Pūnaewele > Wehe > Wehe ma wahi hou
command-browser-zoom-in = Pūnaewele > Nānā > Hoʻonui
command-browser-zoom-out = Pūnaewele > Nānā > Hoʻēmi
command-browser-zoom-reset = Pūnaewele > Nānā > Nui maoli
command-browser-dev-tools = Pūnaewele > Nānā > Mea hana hoʻomohala
command-browser-open-command-bar = Pūnaewele > Pā > Pā kauoha
command-browser-open-page-in-command-bar = Pūnaewele > Pā > Hoʻoponopono ʻaoʻao
command-browser-open-path-bar = Pūnaewele > Pā > Mea hoʻokele ala
command-browser-open-commands = Pūnaewele > Pā > Nā kauoha
command-browser-open-history = Pūnaewele > Pā > Mōʻaukala
command-service-open = Lawelawe > Wehe nānā lawelawe
command-bookmark-toggle-active = Māka puke > Māka puke i ka ʻaoʻao
command-bookmark-pin-active = Māka puke > Pin i ka ʻaoʻao

layout-tab = ʻAoʻao kau
layout-no-stacks = ʻAʻohe ahu
layout-loading = Ke hoʻouka nei…
layout-no-markdown-files = ʻAʻohe waihona Markdown
layout-empty-folder = Waihona hakahaka
layout-worktree = lāʻau hana
layout-folder-name = Inoa waihona
layout-no-pins-bookmarks = ʻAʻohe pin a māka puke paha
layout-move-to = Neʻe i { $folder }
layout-bookmark-current-page = Māka puke i kēia ʻaoʻao
layout-rename-folder = Kapa hou i ka waihona
layout-remove-folder = Wehe i ka waihona
layout-update-downloading = Ke hoʻoiho nei i ka hōʻano hou
layout-update-installing = Ke hoʻouka nei i ka hōʻano hou…
layout-update-ready = Loaʻa ka mana hou
layout-restart-update = Hoʻomaka hou no ka hōʻano hou

agent-preparing = Ke hoʻomākaukau nei i ka ʻākena…
agent-send-all-queued = Hoʻouna i nā paipai a pau i lālani i kēia manawa (Esc)
agent-send = Hoʻouna (Enter)
agent-ready = Mākaukau ke mākaukau ʻoe.
agent-loading-older = Ke hoʻouka nei i nā memo kahiko…
agent-load-older = Hoʻouka i nā memo kahiko
agent-continued-from = Hoʻomau ʻia mai { $source }
agent-older-context-omitted = ua kāpae ʻia ka pōʻaiapili kahiko
agent-interrupted = ua ʻoki ʻia
agent-allow-tool = ʻAe iā { $tool }?
agent-deny = Hōʻole
agent-allow-always = ʻAe mau
agent-allow = ʻAe
agent-loading-sessions = Ke hoʻouka nei i nā kau…
agent-no-resumable-sessions = ʻAʻohe kau hiki ke hoʻomau i loaʻa
agent-no-matching-sessions = ʻAʻohe kau kūlike
agent-no-matching-models = ʻAʻohe kükohu kūlike
agent-choice-help = ↑/↓ a i ʻole Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Koho waihona waihona kumu
agent-choose-repository-detail = Koho i ka waihona Git kūloko e hoʻohana ai ka ʻākena.
agent-choosing = Ke koho nei…
agent-choose-folder = Koho waihona
agent-queued = ma ka lālani
agent-attached = Hoʻopili ʻia:
agent-cancel-queued = Hoʻopau i ka paipai i lālani
agent-resume-queued = Hoʻomau i nā paipai i lālani
agent-clear-queue = Holoi lālani
agent-send-all-now = hoʻouna i nā mea a pau i kēia manawa
agent-choose-option = Koho i koho ma luna
agent-loading-media = Ke hoʻouka nei i ka pāpaho…
agent-no-matching-media = ʻAʻohe pāpaho kūlike
agent-prompt-context = Pōʻaiapili paipai
agent-details = ʻIke kikoʻī
agent-path = Ala
agent-tool = Mea hana
agent-server = Kikowaena
agent-bytes = { $count } bytes
agent-worked-for = Hana no { $duration }
agent-worked-for-steps = { $count ->
    [one] Hana no { $duration } · 1 ʻanuʻu
   *[other] Hana no { $duration } · { $count } ʻanuʻu
}
agent-tool-guardian-review = Loiloi Kahu
agent-tool-read-files = Heluhelu waihona
agent-tool-viewed-image = Nānā kiʻi
agent-tool-used-browser = Hoʻohana pūnaewele
agent-tool-searched-files = Huli waihona
agent-tool-ran-commands = Holo kauoha
agent-thinking = Ke noʻonoʻo nei
agent-subagent = ʻĀkena liʻiliʻi
agent-prompt = Paipai
agent-thread = Pae kamaʻilio
agent-parent = Makua
agent-children = Keiki
agent-call = Kāhea
agent-raw-event = Hanana maka
agent-plan = Papahana
agent-tasks = { $count ->
    [one] 1 hana
   *[other] { $count } hana
}
agent-edited = Hoʻoponopono ʻia
agent-reconnecting = Ke hoʻohui hou nei { $attempt }/{ $total }
agent-status-running = Ke holo nei
agent-status-done = Pau
agent-status-failed = Hāʻule
agent-status-pending = Ke kali nei
agent-slash-attach-files = Hoʻopili waihona
agent-slash-resume-session = Hoʻomau i kau ma mua
agent-slash-select-model = Koho kükohu
agent-slash-continue-cli = Hoʻomau i kēia kau ma ka CLI
agent-session-just-now = i kēia manawa nō
agent-session-minutes-ago = { $count } min. i hala
agent-session-hours-ago = { $count } hola i hala
agent-session-days-ago = { $count } lā i hala
agent-working-working = Ke hana nei
agent-working-thinking = Ke noʻonoʻo nei
agent-working-pondering = Ke noonoo hohonu nei
agent-working-noodling = Ke hoʻokolohua manaʻo nei
agent-working-percolating = Ke hoʻomaʻemaʻe manaʻo nei
agent-working-conjuring = Ke haku nei
agent-working-cooking = Ke kuke nei
agent-working-brewing = Ke kāwili nei
agent-working-musing = Ke noʻonoʻo mālie nei
agent-working-ruminating = Ke kālailai nei
agent-working-scheming = Ke hoʻolālā maalea nei
agent-working-synthesizing = Ke hoʻohui manaʻo nei
agent-working-tinkering = Ke hoʻoponopono liʻiliʻi nei
agent-working-churning = Ke kāwili ikaika nei
agent-working-vibing = Ke holo pū nei
agent-working-simmering = Ke hoʻolapalapa mālie nei
agent-working-crafting = Ke kālai nei
agent-working-divining = Ke ʻimi ʻike nei
agent-working-mulling = Ke noonoo nei
agent-working-spelunking = Ke ʻimi hohonu nei

editor-toggle-explorer = Hoʻololi i ka Mea ʻimi (Cmd+B)
editor-unsaved = ʻaʻole i mālama ʻia
editor-rendered-markdown = Markdown i hōʻike ʻia me ka hoʻoponopono ola
editor-note = Kaha memo
editor-source-editor = Mea hoʻoponopono kumu
editor-editor = Mea hoʻoponopono
editor-git-diff = ʻOkoʻa Git
editor-diff = ʻOkoʻa
editor-tidy = Hoʻomaʻemaʻe
editor-always = Mau
editor-unchanged-previews = { $count ->
    [one] ✦ 1 nānāmua loli ʻole
   *[other] ✦ { $count } nānāmua loli ʻole
}
editor-open-externally = Wehe ma waho
editor-changed-line = Laina i loli
editor-go-to-definition = Hele i ka Wehewehena
editor-find-references = Huli i nā Kuhikuhi
editor-references = { $count ->
    [one] 1 kuhikuhi
   *[other] { $count } kuhikuhi
}
editor-lsp-starting = Ke hoʻomaka nei ʻo { $server }…
editor-lsp-not-installed = { $server } — ʻaʻole i hoʻouka ʻia
editor-explorer = Mea ʻimi
editor-open-editors = Nā mea hoʻoponopono hāmama
editor-outline = Papa kuhikuhi
editor-new-file = Waihona hou
editor-new-folder = Waihona hou
editor-delete-confirm = Holoi iā “{ $name }”? ʻAʻole hiki ke hoʻihoʻi.
editor-created-folder = Ua hana ʻia ka waihona { $name }
editor-created-file = Ua hana ʻia ka waihona { $name }
editor-renamed-to = Ua kapa hou ʻia i { $name }
editor-deleted = Ua holoi ʻia ʻo { $name }
editor-failed-decode-image = ʻAʻole i hiki ke wehewehe i ke kiʻi
editor-preview-large-image = kiʻi (nui loa no ka nānāmua)
editor-preview-binary = pālua
editor-preview-file = waihona

git-status-clean = maʻemaʻe
git-status-modified = hoʻololi ʻia
git-status-staged = hoʻopaʻa mua ʻia
git-status-staged-modified = hoʻopaʻa mua ʻia*
git-status-untracked = ʻaʻole ukali ʻia
git-status-deleted = holoi ʻia
git-status-conflict = paio
git-accept-all = ✓ ʻae i nā mea a pau
git-unstage = Wehe mai ka hoʻopaʻa mua
git-confirm-deny-all = Hōʻoia i ka hōʻole ʻana i nā mea a pau
git-deny-all = ✗ hōʻole i nā mea a pau
git-commit-message = memo commit
git-commit = Commit ({ $count })
git-push = ↑ Pahu
git-loading-diff = Ke hoʻouka nei i ka ʻokoʻa…
git-no-changes = ʻAʻohe loli e hōʻike
git-accept = ✓ ʻae
git-deny = ✗ hōʻole
git-show-unchanged-lines = Hōʻike i { $count } laina loli ʻole

terminal-loading = Ke hoʻouka nei…
terminal-runs-when-ready = holo ke mākaukau · hoʻomaʻemaʻe ʻo Ctrl+C · lele ʻo Esc
terminal-booting = ke hoʻomaka nei
terminal-type-command = kikokiko i kauoha · holo ke mākaukau · lele ʻo Esc

setup-tagline-claude = ʻĀkena kālailai pāʻālua a Anthropic, ma Vmux
setup-tagline-codex = ʻĀkena kālailai pāʻālua a OpenAI, ma Vmux
setup-tagline-vibe = ʻĀkena kālailai pāʻālua a Mistral, ma Vmux
setup-install-title = Hoʻouka iā { $name } CLI
setup-homebrew-required = Pono ʻo Homebrew no ka hoʻouka ʻana iā { $command }, a ʻaʻole i hoʻonohonoho ʻia. E hoʻouka mua ʻo Vmux iā Homebrew, a laila iā { $name }.
setup-terminal-instructions = Ma ke kahua kauoha, kaomi Return e hoʻomaka, a laila hoʻokomo i kāu ʻōlelo huna Mac ke noi ʻia.
setup-command-missing = Ua wehe ʻo Vmux i kēia ʻaoʻao no ka mea ʻaʻole i hoʻouka ʻia ke kauoha kūloko { $command }. Holo i ke kauoha ma lalo e kiʻi ai.
setup-install-failed = ʻAʻole i pau ka hoʻouka. Nānā i ke kahua kauoha no nā kikoʻī, a hoʻāʻo hou.
setup-installing = Ke hoʻouka nei…
setup-install-homebrew = Hoʻouka iā Homebrew + { $name }
setup-run-install = Holo i ke kauoha hoʻouka
setup-auto-reload = Hoʻoholo ʻo Vmux iā ia ma ke kahua kauoha a hoʻouka hou ke mākaukau ʻo { $command }.

debug-title = Huli hewa
debug-auto-update = Hōʻano hou ʻakomi
debug-simulate-update = Hoʻohālike i ka loaʻa o ka hōʻano hou
debug-simulate-download = Hoʻohālike hoʻoiho
debug-clear-update = Holoi hōʻano hou
debug-trigger-restart = Hoʻomaka hou koke

command-manage-spaces = Hoʻokele i nā space…
command-pane-stack-location = pane { $pane } / stack { $stack }
command-space-pane-stack-location = { $space } / pane { $pane } / stack { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = ʻAno launa
command-group-window = Puka aniani
command-group-tab = Kapu
command-group-pane = Pane
command-group-stack = Stack
command-group-space = Space
command-group-navigation = Hoʻokele
command-group-open = Wehe
command-group-view = Nānā
command-group-bar = Pā

menu-close-vmux = Pani iā Vmux

agents-terminal-coding-agent = ʻĀkena kākau pāʻālua ma ka Terminal
