common-open = Wehe
common-close = Pani
common-install = Hoʻokomo
common-uninstall = Wehe ʻia
common-update = Hōʻano hou
common-retry = E hoao hou
common-refresh = Hōʻano hou
common-remove = Wehe
common-enable = Hiki
common-disable = Hoʻopau
common-new = Hou
common-active = ʻeleu
common-running = e holo ana
common-done = hana
common-failed = Ua hāʻule
common-installed = Hoʻokomo ʻia
common-items = { $count ->
    [one] { $count } mea
   *[other] { $count } mau mea
}
start-title = Hoʻomaka
start-tagline = Hoʻokahi manaʻo. Kekahi mea, hana.

agents-title = Na Agena
agents-search = Huli ACP a me CLI mau ʻelele…
agents-empty = ʻAʻohe mea kūʻai like
agents-empty-detail = E ho'āʻo i inoa, manawa holo, a i ʻole ACP/CLI.
agents-install-failed = ʻAʻole i hoʻokomo ʻia
agents-updating = Ke hōʻano hou nei…
agents-retrying = Ke hoʻāʻo hou nei…
agents-preparing = Ke hoʻomākaukau nei…

extensions-title = Hoʻonui
extensions-search = Huli ʻia a i ʻole Chrome Web Store…
extensions-relaunch = Hoʻomaka hou e noi
extensions-empty = ʻAʻohe mea hoʻonui i kau ʻia
extensions-no-match = ʻAʻohe mea hoʻonui like
extensions-empty-detail = Huli i ka Chrome Web Store ma luna a paʻi iā Return.
extensions-no-match-detail = E hoʻāʻo i kahi inoa a i ʻole ID hoʻonui.
extensions-on = Ma ka
extensions-off = Paʻa
extensions-enable-confirm = Hiki iā { $name }?
extensions-enable-permissions = E ho'ā iā { $name } a ʻae:

lsp-title = Nā Kūlana ʻŌlelo
lsp-search = Huli i nā kikowaena ʻōlelo, linters, formatters…
lsp-loading = Ke hoʻouka nei i ka papa inoa…
lsp-empty = ʻAʻohe kikowaena ʻōlelo like ʻole
lsp-empty-detail = E ho'āʻo i ka ʻōlelo ʻē aʻe, linter, a i ʻole mea hoʻopono.
lsp-needs = pono { $tool }
lsp-status-available = Loaʻa
lsp-status-on-path = Ma PATH
lsp-status-installing = Ke kau nei…
lsp-status-installed = Hoʻokomo ʻia
lsp-status-outdated = Loaʻa ka hōʻano hou
lsp-status-running = Ke holo nei
lsp-status-failed = Ua hāʻule

spaces-title = Nā hakahaka
spaces-new-placeholder = He inoa hakahaka hou
spaces-empty = ʻAʻohe hakahaka
spaces-default-name = Wahi { $number }
spaces-tabs = { $count ->
    [one] 1 papa
   *[other] { $count } papa
}
spaces-delete = Holoi i ka hakahaka

team-title = Hui
team-just-you = ʻO ʻoe wale nō ma kēia wahi
team-agents = { $count ->
    [one] ʻO ʻoe a me 1 luna
   *[other] ʻO ʻoe a me { $count } mau ʻelele
}
team-empty = ʻAʻohe kanaka ma ʻaneʻi
team-you = ʻO ʻoe
team-agent = Agena

services-title = Nā lawelawe kāʻei kua
services-processes = { $count ->
    [one] 1 kaʻina hana
   *[other] { $count } kaʻina hana
}
services-kill-all = Kill All
services-not-running = ʻAʻole holo ka lawelawe
services-start-with = E hoʻomaka me:
services-empty = ʻAʻohe kaʻina hana
services-filter = Nā kaʻina hana kānana…
services-no-match = ʻAʻohe kaʻina hana like
services-connected = Hoʻopili ʻia
services-disconnected = Hoʻokuʻu ʻia
services-attached = pili
services-kill = Pepehi
services-memory = Hoʻomanaʻo
services-size = Nui
services-shell = ʻO ka pūpū

error-title = Kuhihewa

history-search = Huli mōʻaukala
history-clear-all = Holoi i nā mea a pau
history-clear-confirm = Holoi i nā moʻolelo a pau?
history-clear-warning = ʻAʻole hiki ke hoʻopau ʻia kēia.
history-cancel = Hoʻopau
history-today = I kēia lā
history-yesterday = I nehinei
history-days-ago = { $count } mau lā aku nei
history-day-offset = Lā -{ $count }

settings-title = Nā hoʻonohonoho
settings-loading = Ke hoʻouka nei i nā hoʻonohonoho…
settings-stored = Mālama ʻia ma ~/.vmux/settings.ron
settings-other = 'ē aʻe
settings-software-update = Hoʻohou polokalamu
settings-check-updates = E nānā i nā mea hou
settings-check-updates-hint = Hoʻopaʻa maʻalahi i ka hoʻomaka ʻana a i kēlā me kēia hola ke hoʻā ʻia ʻo Auto-update.
settings-update-unavailable = Loaʻa ʻole
settings-update-unavailable-hint = ʻAʻole hoʻokomo ʻia ka Updater i kēia kūkulu.
settings-update-checking = Ke nānā nei…
settings-update-checking-hint = Ke nānā nei i nā mea hou…
settings-update-check-again = Nānā Hou
settings-update-current = Vmux ka mea hou.
settings-update-downloading = Hoʻoiho ʻia…
settings-update-downloading-hint = Hoʻoiho ʻia Vmux { $version }…
settings-update-installing = Ke kau nei…
settings-update-installing-hint = Ke hoʻouka nei iā Vmux { $version }…
settings-update-ready = Mākaukau Hou
settings-update-ready-hint = Vmux { $version } ua mākaukau. Hoʻomaka hou e hoʻohana.
settings-update-try-again = E hoao hou
settings-update-failed = ʻAʻole hiki ke nānā i nā mea hou.
settings-item = 'ikamu
settings-item-number = 'ikamu { $number }
settings-press-key = E kaomi i kahi kī…
settings-saved = Mālama ʻia
settings-record-key = Kaomi no ka hoʻopaʻa ʻana i kahi hui kī hou

tray-open-window = Wehe puka makani
tray-close-window = Pani pukaaniani
tray-pause-recording = Hoʻomaha i ka hoʻopaʻa ʻana
tray-resume-recording = Hoʻomaka i ka hoʻopaʻa ʻana
tray-finish-recording = Hoʻopau Hoʻopaʻa
tray-quit = Haʻalele Vmux

composer-attach-files = Hoʻopili i nā faila (/upload)
composer-remove-attachment = Wehe i ka hoʻopili

layout-back = Ke kua
layout-forward = Imua
layout-reload = Hoʻouka hou
layout-bookmark-page = E kaha puke i kēia ʻaoʻao
layout-remove-bookmark = Wehe i ka bookmark
layout-pin-page = Pin i kēia ʻaoʻao
layout-unpin-page = Wehe i kēia ʻaoʻao
layout-manage-extensions = Mālama i nā hoʻonui
layout-new-stack = Puʻu Hou
layout-close-tab = Pani i ka pā
layout-bookmark = Kaha puke
layout-pin = Pin
layout-new-tab = Pahu hou
layout-team = Hui

command-switch-space = E hoʻololi i ka hakahaka…
command-search-ask = Huli a nīnau paha…
command-new-tab-placeholder = Huli a kikokiko i kahi URL, a i ʻole koho i ka Terminal…
command-placeholder = Kākau i URL, ʻimi ʻaoʻao, a i ʻole > no nā kauoha…
command-composer-placeholder = E kikokiko / no nā kauoha a i ʻole @ no ka media
command-send = Hoʻouna (Enter)
command-terminal = Terminal
command-open-terminal = Wehe ma Terminal
command-stack = Hoʻopaʻa
command-tabs = { $count ->
    [one] 1 papa
   *[other] { $count } papa
}
command-prompt = Hoʻomaka
command-new-tab = Pahu hou
command-search = Huli
command-open-value = Wehe "{ $value }"
command-search-value = Huli “{ $value }”

schema-appearance = Ka nana aku
schema-general = Generala
schema-layout = Hoʻolālā
schema-layout-detail = ʻO ka puka makani, nā pane, ka ʻaoʻao ʻaoʻao, a me ke apo kiko.
schema-agent = Agena
schema-agent-detail = Nā ʻae ʻae a me nā mea hana.
schema-shortcuts = Pōkole
schema-shortcuts-detail = Nānā heluhelu wale nō. Hoʻoponopono pololei iā settings.ron e hoʻololi i nā mea paʻa.
schema-terminal = Terminal
schema-browser = Pūnaewele
schema-mode = Ke ano
schema-mode-detail = Hoʻolālā kala no nā ʻaoʻao pūnaewele. Ke hahai nei ka polokalamu i kāu ʻōnaehana.
schema-device = Mea lako
schema-light = Māmā
schema-dark = Poʻeleʻele
schema-language = ʻŌlelo
schema-language-detail = E hoʻohana i ka ʻōnaehana, en-US, ja, a i ʻole kekahi hōʻailona BCP 47 me kahi waihona ~/.vmux/locales/<tag>.ftl like.
schema-auto-update = Hoʻohou ʻakomi
schema-auto-update-detail = E nānā a hoʻokomo i nā mea hou ma ka hoʻomaka ʻana a me kēlā me kēia hola.
schema-startup-url = Hoʻomaka URL
schema-startup-url-detail = Wehe ʻia ʻo Empty i ke kauoha bar prompt.
schema-search-engine = ʻenekini huli
schema-search-engine-detail = Hoʻohana ʻia no ka ʻimi pūnaewele mai ka Start a me ka pahu kauoha.
schema-window = pukaaniani
schema-pane = Pane
schema-side-sheet = Pepa ʻaoʻao
schema-focus-ring = Hoʻopaʻa apo
schema-run-placement = E ʻae i ka hoʻolele ʻana i ke kau ʻana
schema-run-placement-detail = E koho nā ʻelele i ke ʻano holo pane, kuhikuhi, a me ka heleuma.
schema-leader = Alakai
schema-leader-detail = Kiʻi prefix no nā ʻaoʻao pōkole.
schema-chord-timeout = Hoʻopau manawa hoʻokani pila
schema-chord-timeout-detail = Milikikona ma mua o ka pau ʻana o kahi prefix chord.
schema-bindings = Hoʻopaʻa
schema-confirm-close = E hōʻoia kokoke
schema-confirm-close-detail = E hoʻomaka ma mua o ka pani ʻana i kahi pahu me kahi kaʻina holo.
schema-default-theme = Ke kumuhana paʻamau
schema-default-theme-detail = Ka inoa o ka manaʻo hana mai ka papa inoa o nā kumuhana.
