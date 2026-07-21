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
