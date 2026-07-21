common-open = Opið
common-close = Loka
common-install = Settu upp
common-uninstall = Fjarlægðu
common-update = Uppfærsla
common-retry = Reyndu aftur
common-refresh = Endurnýja
common-remove = Fjarlægja
common-enable = Virkja
common-disable = Óvirkja
common-new = Nýtt
common-active = virkur
common-running = hlaupandi
common-done = búið
common-failed = Mistókst
common-installed = Uppsett
common-items = { $count ->
    [one] { $count } atriði
   *[other] { $count } atriði
}
start-title = Byrjaðu
start-tagline = Ein tilvitnun. Hvað sem er, búið.

agents-title = Umboðsmenn
agents-search = Leitaðu að ACP og CLI umboðsmönnum...
agents-empty = Engir samsvarandi umboðsmenn
agents-empty-detail = Prófaðu nafn, keyrslutíma eða ACP/CLI.
agents-install-failed = Uppsetning mistókst
agents-updating = Uppfærir...
agents-retrying = Reynir aftur...
agents-preparing = Undirbýr...

extensions-title = Framlengingar
extensions-search = Leit uppsett eða Chrome Web Store...
extensions-relaunch = Endurræsa til að sækja um
extensions-empty = Engar viðbætur settar upp
extensions-no-match = Engar samsvarandi viðbætur
extensions-empty-detail = Leitaðu í Chrome Web Store hér að ofan og ýttu á Return.
extensions-no-match-detail = Prófaðu annað nafn eða viðbótakenni.
extensions-on = Á
extensions-off = Slökkt
extensions-enable-confirm = Virkja { $name }?
extensions-enable-permissions = Virkjaðu { $name } og leyfðu:

lsp-title = Tungumálaþjónar
lsp-search = Leitaðu að tungumálaþjónum, linters, formatterum...
lsp-loading = Hleður vörulista...
lsp-empty = Engir samsvarandi tungumálaþjónar
lsp-empty-detail = Prófaðu annað tungumál, linter eða formatter.
lsp-needs = þarf { $tool }
lsp-status-available = Í boði
lsp-status-on-path = Á PATH
lsp-status-installing = Setur upp...
lsp-status-installed = Uppsett
lsp-status-outdated = Uppfærsla í boði
lsp-status-running = Hlaupandi
lsp-status-failed = Mistókst

spaces-title = Rými
spaces-new-placeholder = Nýtt rýmisheiti
spaces-empty = Engin bil
spaces-default-name = Rými { $number }
spaces-tabs = { $count ->
    [one] 1 flipi
   *[other] { $count } flipar
}
spaces-delete = Eyða bili

team-title = Lið
team-just-you = Bara þú í þessu rými
team-agents = { $count ->
    [one] Þú og 1 umboðsmaður
   *[other] Þú og { $count } umboðsmenn
}
team-empty = Enginn hér ennþá
team-you = Þú
team-agent = Umboðsmaður

services-title = Bakgrunnsþjónusta
services-processes = { $count ->
    [one] 1 ferli
   *[other] { $count } ferli
}
services-kill-all = Drepa alla
services-not-running = Þjónustan er ekki í gangi
services-start-with = Byrjaðu með:
services-empty = Engir virkir ferlar
services-filter = Sía ferli…
services-no-match = Engir samsvörunarferli
services-connected = Tengdur
services-disconnected = Ótengdur
services-attached = meðfylgjandi
services-kill = Drepa
services-memory = Minni
services-size = Stærð
services-shell = Skel

error-title = Villa

history-search = Leitarferill
history-clear-all = Hreinsaðu allt
history-clear-confirm = Hreinsa allan feril?
history-clear-warning = Þetta er ekki hægt að afturkalla.
history-cancel = Hætta við
history-today = Í dag
history-yesterday = Í gær
history-days-ago = { $count } dögum síðan
history-day-offset = Dagur -{ $count }

settings-title = Stillingar
settings-loading = Hleður stillingum...
settings-stored = Geymt í ~/.vmux/settings.ron
settings-other = Annað
settings-software-update = Hugbúnaðaruppfærsla
settings-check-updates = Leitaðu að uppfærslum
settings-check-updates-hint = Athugar sjálfkrafa við ræsingu og á klukkutíma fresti þegar sjálfvirk uppfærsla er virkjuð.
settings-update-unavailable = Ekki tiltækt
settings-update-unavailable-hint = Uppfærsla er ekki innifalin í þessari byggingu.
settings-update-checking = Athugar...
settings-update-checking-hint = Leitar að uppfærslum...
settings-update-check-again = Athugaðu aftur
settings-update-current = Vmux er uppfærð.
settings-update-downloading = Sækir…
settings-update-downloading-hint = Sækir Vmux { $version }...
settings-update-installing = Setur upp...
settings-update-installing-hint = Setur upp Vmux { $version }...
settings-update-ready = Uppfærsla tilbúin
settings-update-ready-hint = Vmux { $version } er tilbúinn. Endurræstu til að nota það.
settings-update-try-again = Reyndu aftur
settings-update-failed = Ekki er hægt að leita að uppfærslum.
settings-item = Atriði
settings-item-number = Atriði { $number }
settings-press-key = Ýttu á takka…
settings-saved = Vistað
settings-record-key = Smelltu til að taka upp nýtt lyklasamsetningu

tray-open-window = Opna glugga
tray-close-window = Lokaðu glugga
tray-pause-recording = Gera hlé á upptöku
tray-resume-recording = Halda áfram upptöku
tray-finish-recording = Ljúktu upptöku
tray-quit = Hætta Vmux

composer-attach-files = Hengja skrár (/upload)
composer-remove-attachment = Fjarlægðu viðhengi

layout-back = Til baka
layout-forward = Áfram
layout-reload = Endurhlaða
layout-bookmark-page = Bókamerki þessa síðu
layout-remove-bookmark = Fjarlægja bókamerki
layout-pin-page = Festu þessa síðu
layout-unpin-page = Losaðu þessa síðu
layout-manage-extensions = Stjórna viðbótum
layout-new-stack = Nýr stafli
layout-close-tab = Loka flipa
layout-bookmark = Bókamerki
layout-pin = Pinna
layout-new-tab = Nýr flipi
layout-team = Lið

command-switch-space = Skipta um bil…
command-search-ask = Leitaðu eða spurðu…
command-new-tab-placeholder = Leitaðu eða sláðu inn URL, eða veldu Terminal...
command-placeholder = Sláðu inn URL, leitarflipa eða > fyrir skipanir...
command-composer-placeholder = Sláðu inn / fyrir skipanir eða @ fyrir miðil
command-send = Senda (Enter)
command-terminal = Flugstöð
command-open-terminal = Opið í Terminal
command-stack = Stafla
command-tabs = { $count ->
    [one] 1 flipi
   *[other] { $count } flipar
}
command-prompt = Hvetja
command-new-tab = Nýr flipi
command-search = Leita
command-open-value = Opnaðu „{ $value }“
command-search-value = Leita í „{ $value }“

schema-appearance = Útlit
schema-general = Almennt
schema-layout = Skipulag
schema-layout-detail = Gluggi, rúður, hliðarstika og fókushringur.
schema-agent = Umboðsmaður
schema-agent-detail = Hegðun umboðsmanns og tólaheimildir.
schema-shortcuts = Flýtileiðir
schema-shortcuts-detail = Skrifvarinn skjár. Breyttu settings.ron beint til að breyta bindingum.
schema-terminal = Flugstöð
schema-browser = Vafri
schema-mode = Mode
schema-mode-detail = Litasamsetning fyrir vefsíður. Tækið fylgir kerfinu þínu.
schema-device = Tæki
schema-light = Ljós
schema-dark = Myrkur
schema-language = Tungumál
schema-language-detail = Notaðu system, en-US, ja eða hvaða BCP 47 merk sem er með samsvarandi ~/.vmux/locales/<tag>.ftl vörulista.
schema-auto-update = Sjálfvirk uppfærsla
schema-auto-update-detail = Athugaðu og settu upp uppfærslur við ræsingu og á klukkutíma fresti.
schema-startup-url = Gangsetning URL
schema-startup-url-detail = Tómt opnar skipanastikuna.
schema-search-engine = Leitarvél
schema-search-engine-detail = Notað fyrir vefleit frá Start og skipanastikunni.
schema-window = Gluggi
schema-pane = Rúða
schema-side-sheet = Hliðarblað
schema-focus-ring = Fókus hringur
schema-run-placement = Leyfa hnekkingu á keyrslustaðsetningu
schema-run-placement-detail = Leyfðu umboðsmönnum að velja akstursrúðuham, stefnu og akkeri.
schema-leader = Leiðtogi
schema-leader-detail = Forskeytalykill fyrir flýtileiðir hljóma.
schema-chord-timeout = Hljómatími
schema-chord-timeout-detail = Millisekúndum áður en hljómaforskeyti rennur út.
schema-bindings = Bindingum
schema-confirm-close = Staðfestu lokun
schema-confirm-close-detail = Spyrðu áður en þú lokar flugstöð með ferli í gangi.
schema-default-theme = Sjálfgefið þema
schema-default-theme-detail = Heiti virka þema af þemalistanum.
