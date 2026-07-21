common-open = Bula
common-close = Kwala
common-install = Kenya
common-uninstall = Ntsha
common-update = Ntjhafatsa
common-retry = Leka hape
common-refresh = Kgatholla
common-remove = Tlosa
common-enable = Bulela
common-disable = Tima
common-new = Ntjha
common-active = e sebetsa
common-running = e ntse e sebetsa
common-done = ho phethilwe
common-failed = E hlolehile
common-installed = E kentswe
common-items = { $count ->
    [one] { $count } ntho
   *[other] dintho tse { $count }
}
start-title = Qala
start-tagline = Prompt e le nngwe. Eng kapa eng, e phethilwe.

agents-title = Di-agent
agents-search = Batla di-agent tsa ACP le CLI…
agents-empty = Ha ho di-agent tse tshwanelanang
agents-empty-detail = Leka lebitso, runtime, kapa ACP/CLI.
agents-install-failed = Ho kenya ho hlolehile
agents-updating = E a ntjhafatswa…
agents-retrying = E leka hape…
agents-preparing = E a lokisetswa…

extensions-title = Di-extensions
extensions-search = Batla tse kentsweng kapa Chrome Web Store…
extensions-relaunch = Qala hape ho kenya tshebetsong
extensions-empty = Ha ho di-extensions tse kentsweng
extensions-no-match = Ha ho di-extensions tse tshwanelanang
extensions-empty-detail = Batla ho Chrome Web Store ka hodimo, ebe o tobetsa Return.
extensions-no-match-detail = Leka lebitso le leng kapa ID ya extension.
extensions-on = E buletswe
extensions-off = E timilwe
extensions-enable-confirm = Bulela { $name }?
extensions-enable-permissions = Bulela { $name } mme o dumelle:

lsp-title = Diseva tsa Dipuo
lsp-search = Batla diseva tsa dipuo, di-linter, di-formatter…
lsp-loading = E kenya katalog…
lsp-empty = Ha ho diseva tsa dipuo tse tshwanelanang
lsp-empty-detail = Leka puo e nngwe, linter, kapa formatter.
lsp-needs = e hloka { $tool }
lsp-status-available = E teng
lsp-status-on-path = E ho PATH
lsp-status-installing = E a kenngwa…
lsp-status-installed = E kentswe
lsp-status-outdated = Ntjhafatso e teng
lsp-status-running = E ntse e sebetsa
lsp-status-failed = E hlolehile

spaces-title = Dibaka
spaces-new-placeholder = Lebitso la sebaka se setjha
spaces-empty = Ha ho dibaka
spaces-default-name = Sebaka { $number }
spaces-tabs = { $count ->
    [one] tabo e 1
   *[other] ditabo tse { $count }
}
spaces-delete = Hlakola sebaka

team-title = Sehlopha
team-just-you = Ke wena feela sebakeng sena
team-agents = { $count ->
    [one] Wena le agent e 1
   *[other] Wena le di-agent tse { $count }
}
team-empty = Ha ho motho mona hajoale
team-you = Wena
team-agent = Agent

services-title = Ditshebeletso tsa Ka morao
services-processes = { $count ->
    [one] process e 1
   *[other] di-process tse { $count }
}
services-kill-all = Emisa Tsohle ka Qobello
services-not-running = Tshebeletso ha e sebetse
services-start-with = Qala ka:
services-empty = Ha ho di-process tse sebetsang
services-filter = Sefa di-process…
services-no-match = Ha ho di-process tse tshwanelanang
services-connected = E hoketswe
services-disconnected = E kgaotswe
services-attached = e hoketswe
services-kill = Emisa ka qobello
services-memory = Memori
services-size = Boholo
services-shell = Shell

error-title = Phoso

history-search = Batla historing
history-clear-all = Hlakola tsohle
history-clear-confirm = Hlakola histori yohle?
history-clear-warning = Sena se ke ke sa etsollwa.
history-cancel = Hlakola
history-today = Kajeno
history-yesterday = Maobane
history-days-ago = Matsatsi a { $count } a fetileng
history-day-offset = Letsatsi -{ $count }

settings-title = Disetting
settings-loading = E kenya disetting…
settings-stored = E bolokilwe ho ~/.vmux/settings.ron
settings-other = Tse ding
settings-software-update = Ntjhafatso ya Software
settings-check-updates = Hlahloba Dintjhafatso
settings-check-updates-hint = E hlahloba ka bo yona ha e qala le hora le hora ha Auto-update e buletswe.
settings-update-unavailable = Ha e fumanehe
settings-update-unavailable-hint = Sentjhafatsi ha se a kenyeletswa mohahong ona.
settings-update-checking = E a hlahloba…
settings-update-checking-hint = E hlahloba dintjhafatso…
settings-update-check-again = Hlahloba Hape
settings-update-current = Vmux e ntjhafetse.
settings-update-downloading = E a jarolla…
settings-update-downloading-hint = E jarolla Vmux { $version }…
settings-update-installing = E a kenya…
settings-update-installing-hint = E kenya Vmux { $version }…
settings-update-ready = Ntjhafatso e Lokile
settings-update-ready-hint = Vmux { $version } e lokile. Qala hape ho e kenya tshebetsong.
settings-update-try-again = Leka Hape
settings-update-failed = Ha ho kgonehe ho hlahloba dintjhafatso.
settings-item = Ntho
settings-item-number = Ntho { $number }
settings-press-key = Tobetsa konopo…
settings-saved = E bolokilwe
settings-record-key = Tobetsa ho rekota motsoako o motjha wa dikonopo

tray-open-window = Bula Fesetere
tray-close-window = Kwala Fesetere
tray-pause-recording = Emisa Rekoto Nakwana
tray-resume-recording = Tswelapele ka Rekoto
tray-finish-recording = Qeta Rekoto
tray-quit = Tswala Vmux

composer-attach-files = Hokela difaele (/upload)
composer-remove-attachment = Tlosa sehokelo

layout-back = Morao
layout-forward = Pele
layout-reload = Kenya hape
layout-bookmark-page = Tshwaya leqephe lena
layout-remove-bookmark = Tlosa letshwao
layout-pin-page = Penya leqephe lena
layout-unpin-page = Tlohela leqephe lena
layout-manage-extensions = Laola di-extensions
layout-new-stack = Mokgobo o motjha
layout-close-tab = Kwala tabo
layout-bookmark = Letshwao
layout-pin = Penya
layout-new-tab = Tabo e ntjha
layout-team = Sehlopha

command-switch-space = Fetola sebaka…
command-search-ask = Batla kapa botsa…
command-new-tab-placeholder = Batla kapa thaepa URL, kapa kgetha Terminal…
command-placeholder = Thaepa URL, batla ditabo, kapa > bakeng sa ditaelo…
command-composer-placeholder = Thaepa / bakeng sa ditaelo kapa @ bakeng sa media
command-send = Romela (Enter)
command-terminal = Terminal
command-open-terminal = Bula ho Terminal
command-stack = Mokgobo
command-tabs = { $count ->
    [one] tabo e 1
   *[other] ditabo tse { $count }
}
command-prompt = Prompt
command-new-tab = Tabo e ntjha
command-search = Batla
command-open-value = Bula “{ $value }”
command-search-value = Batla “{ $value }”

schema-appearance = Ponahalo
schema-general = Kakaretso
schema-layout = Tlhophiso
schema-layout-detail = Fesetere, dikarolo, bara ya ka thoko, le reng ya focus.
schema-agent = Agent
schema-agent-detail = Boitshwaro ba agent le ditumello tsa dithulusi.
schema-shortcuts = Dikgaoletso
schema-shortcuts-detail = Pono ya ho bala feela. Fetola settings.ron ka kotloloho ho fetola dikopano tsa dikonopo.
schema-terminal = Terminal
schema-browser = Sebatli
schema-mode = Mokgwa
schema-mode-detail = Sekema sa mebala bakeng sa maqephe a web. Device e latela sistimi ya hao.
schema-device = Device
schema-light = E kganyang
schema-dark = E lefifi
schema-language = Puo
schema-language-detail = Sebedisa ya sistimi, en-US, ja, kapa tag efe kapa efe ya BCP 47 e nang le katalog ya ~/.vmux/locales/<tag>.ftl e tshwanelanang.
schema-auto-update = Auto-update
schema-auto-update-detail = Hlahloba le ho kenya dintjhafatso ha e qala le hora le hora.
schema-startup-url = URL ya ho qala
schema-startup-url-detail = Ha e se na letho e bula prompt ya bara ya ditaelo.
schema-search-engine = Enjine ya ho batla
schema-search-engine-detail = E sebediswa bakeng sa dipatlisiso tsa web ho Start le bareng ya ditaelo.
schema-window = Fesetere
schema-pane = Karolo
schema-side-sheet = Leqephe la ka thoko
schema-focus-ring = Reng ya focus
schema-run-placement = Dumella ho feta tlhophiso ya sebaka sa run
schema-run-placement-detail = Dumella di-agent ho kgetha mokgwa wa karolo ya run, tsela, le ankere.
schema-leader = Leader
schema-leader-detail = Konopo ya pele bakeng sa dikgaoletso tsa chord.
schema-chord-timeout = Nako ya ho fela ya chord
schema-chord-timeout-detail = Dimilisecond pele prefix ya chord e felloa ke nako.
schema-bindings = Dikopano tsa dikonopo
schema-confirm-close = Netefatsa ho kwala
schema-confirm-close-detail = Botsa pele o kwala terminal e nang le process e ntseng e sebetsa.
schema-default-theme = Theme ya kamehla
schema-default-theme-detail = Lebitso la theme e sebetsang lenaneng la di-theme.
