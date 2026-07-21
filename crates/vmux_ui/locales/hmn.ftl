common-open = Qhib
common-close = Kaw
common-install = Nruab
common-uninstall = Uninstall
common-update = Hloov tshiab
common-retry = Rov sim dua
common-refresh = Hloov tshiab
common-remove = Tshem tawm
common-enable = Pab
common-disable = Disable
common-new = Tshiab
common-active = nquag
common-running = khiav
common-done = ua tiav
common-failed = Ua tsis tiav
common-installed = Nruab
common-items = { $count ->
    [one] { $count } yam
   *[other] { $count } cov khoom
}
start-title = Pib
start-tagline = Ib qho lus ceeb toom. Txhua yam, ua tiav.

agents-title = Cov neeg sawv cev
agents-search = Nrhiav ACP thiab CLI tus neeg sawv cev…
agents-empty = Tsis muaj tus neeg sawv cev zoo sib xws
agents-empty-detail = Sim lub npe, lub sijhawm ua haujlwm, lossis ACP/CLI.
agents-install-failed = Kev teeb tsa ua tsis tiav
agents-updating = Kev hloov kho…
agents-retrying = Rov sim dua…
agents-preparing = Kev npaj…

extensions-title = Extensions
extensions-search = Nrhiav ntsia los yog Chrome Web Store…
extensions-relaunch = Rov qhib dua los thov
extensions-empty = Tsis muaj extensions ntsia
extensions-no-match = Tsis muaj kev sib txuas ntxiv
extensions-empty-detail = Nrhiav Chrome Web Store saum toj no thiab nias Return.
extensions-no-match-detail = Sim lwm lub npe lossis txuas ntxiv ID.
extensions-on = Ntawm
extensions-off = Tawm
extensions-enable-confirm = Qhib { $name }?
extensions-enable-permissions = Qhib { $name } thiab tso cai:

lsp-title = Lus Servers
lsp-search = Nrhiav cov lus servers, linters, formatters…
lsp-loading = Loading catalog…
lsp-empty = Tsis sib xws cov lus servers
lsp-empty-detail = Sim lwm hom lus, linter, lossis formatter.
lsp-needs = xav tau { $tool }
lsp-status-available = Muaj
lsp-status-on-path = Ntawm PATH
lsp-status-installing = Kev txhim kho…
lsp-status-installed = Nruab
lsp-status-outdated = Hloov tshiab muaj
lsp-status-running = Khiav
lsp-status-failed = Ua tsis tiav

spaces-title = Qhov chaw
spaces-new-placeholder = Lub npe tshiab
spaces-empty = Tsis muaj chaw
spaces-default-name = Space { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tabs
}
spaces-delete = Rho tawm qhov chaw

team-title = Pab neeg
team-just-you = Tsuas yog koj nyob hauv qhov chaw no
team-agents = { $count ->
    [one] Koj thiab 1 tus neeg sawv cev
   *[other] Koj thiab { $count } tus neeg sawv cev
}
team-empty = Tsis muaj leej twg nyob ntawm no
team-you = Koj
team-agent = Tus neeg sawv cev

services-title = Cov kev pab cuam tom qab
services-processes = { $count ->
    [one] 1 txheej txheem
   *[other] { $count } cov txheej txheem
}
services-kill-all = Tua tag nrho
services-not-running = Kev pabcuam tsis ua haujlwm
services-start-with = Pib nrog:
services-empty = Tsis muaj cov txheej txheem nquag
services-filter = Lim cov txheej txheem…
services-no-match = Tsis muaj cov txheej txheem sib txuam
services-connected = Txuas nrog
services-disconnected = Txiav tawm
services-attached = txuas
services-kill = Tua
services-memory = Nco
services-size = Loj
services-shell = Plhaub

error-title = yuam kev

history-search = Nrhiav keeb kwm
history-clear-all = Clear tag nrho
history-clear-confirm = Clear tag nrho cov keeb kwm?
history-clear-warning = Qhov no tsis tuaj yeem thim rov qab.
history-cancel = Tso tseg
history-today = Hnub no
history-yesterday = Nag hmo
history-days-ago = { $count } hnub dhau los
history-day-offset = Hnub -{ $count }

settings-title = Chaw
settings-loading = Loading settings…
settings-stored = Khaws rau hauv ~/.vmux/settings.ron
settings-other = Lwm yam
settings-software-update = Hloov tshiab Software
settings-check-updates = Txheeb xyuas qhov hloov tshiab
settings-check-updates-hint = Txheeb xyuas tau ntawm kev tso tawm thiab txhua teev thaum pib hloov kho tshiab yog qhib.
settings-update-unavailable = Tsis muaj
settings-update-unavailable-hint = Updater tsis suav nrog hauv qhov tsim no.
settings-update-checking = Kev tshuaj xyuas…
settings-update-checking-hint = Nrhiav kev hloov tshiab…
settings-update-check-again = Xyuas dua
settings-update-current = Vmux yog hloov tshiab.
settings-update-downloading = Downloading…
settings-update-downloading-hint = Downloading Vmux { $version }…
settings-update-installing = Kev txhim kho…
settings-update-installing-hint = Txhim kho Vmux { $version }…
settings-update-ready = Hloov tshiab Npaj
settings-update-ready-hint = Vmux { $version } npaj txhij. Rov pib dua los siv nws.
settings-update-try-again = Sim dua
settings-update-failed = Tsis tuaj yeem tshawb xyuas qhov hloov tshiab.
settings-item = Yam khoom
settings-item-number = Yam khoom { $number }
settings-press-key = Nias tus yuam sij…
settings-saved = Txuag
settings-record-key = Nyem rau sau ib qho tseem ceeb combo tshiab

tray-open-window = Qhib Qhov rai
tray-close-window = Kaw Qhov rai
tray-pause-recording = Ncua tseg
tray-resume-recording = Resume Recording
tray-finish-recording = Sau tiav
tray-quit = Tawm Vmux

composer-attach-files = Txuas cov ntaub ntawv (/upload)
composer-remove-attachment = Tshem tawm cov ntawv txuas

layout-back = Rov qab
layout-forward = Tom ntej
layout-reload = Rov qab
layout-bookmark-page = Bookmark nplooj ntawv no
layout-remove-bookmark = Tshem tawm bookmark
layout-pin-page = Pin nplooj ntawv no
layout-unpin-page = Unpin nplooj ntawv no
layout-manage-extensions = Tswj cov extensions
layout-new-stack = Pawg tshiab
layout-close-tab = Kaw tab
layout-bookmark = Bookmark
layout-pin = Pin
layout-new-tab = New tab
layout-team = Pab neeg

command-switch-space = Hloov chaw…
command-search-ask = Nrhiav los yog nug…
command-new-tab-placeholder = Nrhiav lossis ntaus URL, lossis xaiv Terminal…
command-placeholder = Ntaus URL, tshawb tabs, lossis> rau cov lus txib...
command-composer-placeholder = Ntaus / rau cov lus txib lossis @ rau kev tshaj tawm
command-send = Xa (Enter)
command-terminal = Terminal
command-open-terminal = Qhib hauv Terminal
command-stack = Pob
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tabs
}
command-prompt = Ceev
command-new-tab = New tab
command-search = Nrhiav
command-open-value = Qhib “{ $value }”
command-search-value = Nrhiav “{ $value }”

schema-appearance = Qhov tshwm sim
schema-general = General
schema-layout = Layout
schema-layout-detail = Qhov rai, panes, sidebar, thiab tsom nplhaib.
schema-agent = Tus neeg sawv cev
schema-agent-detail = Tus neeg saib xyuas tus cwj pwm thiab cov cuab yeej tso cai.
schema-shortcuts = Shortcuts
schema-shortcuts-detail = Nyeem nkaus xwb. Kho kom raug settings.ron ncaj qha los hloov kev khi.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Hom
schema-mode-detail = Xim qauv rau cov nplooj ntawv web. Ntaus raws li koj qhov system.
schema-device = Ntaus
schema-light = Teeb
schema-dark = Tsaus
schema-language = Lus
schema-language-detail = Siv qhov system, en-US, ja, lossis ib qho BCP 47 tag nrog rau qhov sib piv ~/.vmux/locales/<tag>.ftl catalog.
schema-auto-update = Nws pib hloov tshiab
schema-auto-update-detail = Tshawb xyuas thiab teeb tsa cov hloov tshiab ntawm kev tso tawm thiab txhua teev.
schema-startup-url = Pib URL
schema-startup-url-detail = Empty qhib qhov hais kom ua bar tam sim.
schema-search-engine = Tshawb nrhiav cav
schema-search-engine-detail = Siv rau kev tshawb nrhiav lub vev xaib los ntawm Pib thiab qhov hais kom ua bar.
schema-window = Qhov rai
schema-pane = Pane
schema-side-sheet = Sab ntawv
schema-focus-ring = Lub nplhaib tsom
schema-run-placement = Tso cai khiav qhov chaw override
schema-run-placement-detail = Cia cov neeg sawv cev xaiv khiav pane hom, kev taw qhia, thiab thauj tog rau nkoj.
schema-leader = Thawj coj
schema-leader-detail = Prefix key rau chord shortcuts.
schema-chord-timeout = Chord sij hawm
schema-chord-timeout-detail = Milliseconds ua ntej chord prefix tas sij hawm.
schema-bindings = Kev khi
schema-confirm-close = Paub meej tias kaw
schema-confirm-close-detail = Qhia ua ntej kaw lub davhlau ya nyob twg nrog cov txheej txheem khiav.
schema-default-theme = Default ntsiab
schema-default-theme-detail = Lub npe ntawm lub ntsiab lus tseem ceeb los ntawm cov npe ntawm cov ntsiab lus.
