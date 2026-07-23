locale-name = Sesotho
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

tools-title = Lisebelisoa
tools-search = Batla liphutheloana, baemeli, MCP, lisebelisoa tsa puo le lifaele tsa tlhophiso…
tools-open = Bula lisebelisoa
tools-fold = Mena lisebelisoa
tools-unfold = Atolosa lisebelisoa
tools-scanning = Ho hlahlojoa lisebelisoa tsa lehae…
tools-no-installed = Ha ho lisebelisoa tse kentsoeng
tools-empty = Ha ho lisebelisoa tse tsamaellanang
tools-empty-detail = Kenya sephutheloana kapa u kenye sephutheloana sa lifaele tsa tlhophiso sa mofuta oa Stow.
tools-apply = Sebelisa
tools-homebrew = Homebrew
tools-homebrew-sync = Mefuta le mananeo a kentsoeng li ikamahanya ka bo eona.
tools-open-brewfile = Bula Brewfile
tools-managed = laoloa
tools-provider-homebrew-formulae = Mefuta ea Homebrew
tools-provider-homebrew-casks = Mananeo a Homebrew
tools-provider-npm = Liphutheloana tsa npm
tools-provider-acp-agents = Baemeli ba ACP
tools-provider-language-tools = Lisebelisoa tsa puo
tools-provider-mcp-servers = Li-server tsa MCP
tools-provider-dotfiles = Lifaele tsa tlhophiso
tools-status-available = E teng
tools-status-missing = E sieo
tools-status-conflict = Khohlano
tools-forget = Lebala
tools-manage = Laola
tools-link = Hokela
tools-unlink = Hakolla
tools-import = Kenya ho tsoa kantle
tools-update-count = { $count ->
    [one] Ntlafatso e 1
   *[other] Lintlafatso tse { $count }
}
tools-conflict-count = { $count ->
    [one] Khohlano e 1
   *[other] Likhohlano tse { $count }
}
tools-result-applied = Lisebelisoa li sebelisitsoe
tools-result-imported = Lisebelisoa li kentsoe ho tsoa kantle
tools-result-installed = { $name } e kentsoe
tools-result-updated = { $name } e ntlafalitsoe
tools-result-uninstalled = { $name } e tlositsoe
tools-result-forgotten = { $name } e lebetsoe
tools-result-managed = { $name } joale ea laoloa
tools-result-linked = { $name } e hoketsoe
tools-result-unlinked = { $name } e hakolotsoe
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Litlhophiso tsa sync, lisebelisoa, li-dotfiles, le Tsebo le Git.
vault-sync = Sync
vault-create = Theha
vault-connect = Hokela
vault-private = Sebaka sa polokelo ea poraefete
vault-public-warning = Lipolokelo tsa sechaba li pepesa Tsebo le tlhophiso ea hau.
vault-choose-repository = Khetha sebaka sa polokelo…
vault-empty = se nang letho
vault-clean = E maemong
vault-not-connected = Ha e kopane
vault-change-count = Liphetoho: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

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

settings-empty = (ha ho letho)
settings-none = (ha ho yona)

schema-system = Sistimi
schema-editor = Sehlophisi
schema-recording = Kgatiso
schema-radius = Radiase
schema-padding = Sebaka sa kahare
schema-gap = Lekhalo
schema-width = Bophara
schema-color = Mmala
schema-red = Khubedu
schema-green = Tala
schema-blue = Bolou
schema-follow-files = Latela difaele
schema-tidy-files = Hlwekisa difaele
schema-tidy-files-max = Moedi wa ho hlwekisa difaele
schema-tidy-files-auto = Hlwekisa difaele ka boiketsetso
schema-app-providers = Bafani ba app
schema-provider = Mofani
schema-kind = Mofuta
schema-models = Dimodeli
schema-acp = Baagente ba ACP
schema-id = ID
schema-name = Lebitso
schema-command = Taelo
schema-arguments = Diargumente
schema-environment = Tikoloho
schema-working-directory = Foldara ya tshebetso
schema-shell = Khetla
schema-font-family = Lelapa la fonte
schema-startup-directory = Foldara ya ho qala
schema-themes = Meralo
schema-color-scheme = Moralo wa mebala
schema-font-size = Boholo ba fonte
schema-line-height = Bophahamo ba mola
schema-cursor-style = Setaele sa khesara
schema-cursor-blink = Ho panya ha khesara
schema-custom-themes = Meralo e ikgethetseng
schema-foreground = Bokapele
schema-background = Bokamorao
schema-cursor = Khesara
schema-ansi-colors = Mebala ya ANSI
schema-keymap = Mapa wa dinotlolo
schema-explorer = Sebatli sa difaele
schema-visible = E a bonahala
schema-language-servers = Diseva tsa dipuo
schema-servers = Diseva
schema-language-id = ID ya puo
schema-root-markers = Matshwao a motso
schema-output-directory = Foldara ya tlhahiso

menu-scene = Sebaka
menu-layout = Tlhophiso
menu-terminal = Theminale
menu-browser = Sebatli
menu-service = Tshebeletso
menu-bookmark = Letshwao la leqephe
menu-edit = Fetola

layout-knowledge = Tsebo
layout-open-knowledge = Bula Tsebo
layout-open-welcome-knowledge = Bula Rea o amohela ho Tsebo
layout-open-path = Bula { $path }
layout-fold-knowledge = Mena tsebo
layout-unfold-knowledge = Phutholla tsebo
layout-bookmarks = Matshwao a maqephe
layout-new-folder = Foldara e ntjha
layout-add-to-bookmarks = Kenya ho Matshwao a maqephe
layout-move-to-bookmarks = Isa ho Matshwao a maqephe
layout-stack-number = Mokgobo { $number }
layout-fold-stack = Mena mokgobo
layout-unfold-stack = Phutholla mokgobo
layout-close-stack = Kwala mokgobo
layout-bookmark-in = Tshwaya leqephe ho { $folder }

common-cancel = Hlakola
common-delete = Hlakola
common-save = Boloka
common-rename = Reha lebitso bocha
common-expand = Atolosa
common-collapse = Meneha
common-loading = Ea kenya…
common-error = Phoso
common-output = Sephetho
common-pending = E emetse
common-current = ea hajoale
common-stop = Emisa
services-command = Tshebeletso ya Vmux
services-uptime-seconds = { $seconds }s
services-uptime-minutes = { $minutes }m { $seconds }s
services-uptime-hours = { $hours }h { $minutes }m
services-uptime-days = { $days }d { $hours }h

error-page-failed-load = Leqephe le hlotse ho kenya
error-page-not-found = Leqephe ha le a fumanwa
error-unknown-host = Moamohedi wa app ya Vmux ha a tsejwe: { $host }

history-title = Nalane

command-new-app-chat = Puisano e ntjha ya { $provider }/{ $model } (App)
command-interactive-mode-user = Scene > Mokgwa wa puisano > Mosebedisi
command-interactive-mode-player = Scene > Mokgwa wa puisano > Sebapadi
command-minimize-window = Layout > Fensetere > Nyenyefatsa
command-toggle-layout = Layout > Moralo > Fetola moralo
command-close-tab = Layout > Thebe > Kwala thebe
command-new-task = Layout > Thebe > Mosebetsi o motjha…
command-next-tab = Layout > Thebe > Thebe e latelang
command-prev-tab = Layout > Thebe > Thebe e fetileng
command-rename-tab = Layout > Thebe > Reha thebe lebitso bocha
command-tab-select-1 = Layout > Thebe > Kgetha Thebe 1
command-tab-select-2 = Layout > Thebe > Kgetha Thebe 2
command-tab-select-3 = Layout > Thebe > Kgetha Thebe 3
command-tab-select-4 = Layout > Thebe > Kgetha Thebe 4
command-tab-select-5 = Layout > Thebe > Kgetha Thebe 5
command-tab-select-6 = Layout > Thebe > Kgetha Thebe 6
command-tab-select-7 = Layout > Thebe > Kgetha Thebe 7
command-tab-select-8 = Layout > Thebe > Kgetha Thebe 8
command-tab-select-last = Layout > Thebe > Kgetha thebe ya ho qetela
command-close-pane = Layout > Pane > Kwala pane
command-select-pane-left = Layout > Pane > Kgetha pane ya leqele
command-select-pane-right = Layout > Pane > Kgetha pane ya le letona
command-select-pane-up = Layout > Pane > Kgetha pane e hodimo
command-select-pane-down = Layout > Pane > Kgetha pane e tlase
command-swap-pane-prev = Layout > Pane > Fapanyetsana ka pane e fetileng
command-swap-pane-next = Layout > Pane > Fapanyetsana ka pane e latelang
command-equalize-pane-size = Layout > Pane > Lekantsha boholo ba dipane
command-resize-pane-left = Layout > Pane > Fetola boholo ba pane ka ho le letshehadi
command-resize-pane-right = Layout > Pane > Fetola boholo ba pane ka ho le letona
command-resize-pane-up = Layout > Pane > Fetola boholo ba pane hodimo
command-resize-pane-down = Layout > Pane > Fetola boholo ba pane tlase
command-stack-close = Layout > Stack > Kwala stack
command-stack-next = Layout > Stack > Stack e latelang
command-stack-previous = Layout > Stack > Stack e fetileng
command-stack-reopen = Layout > Stack > Bula hape leqephe le kwetsweng
command-stack-swap-prev = Layout > Stack > Suthela stack ka ho le letshehadi
command-stack-swap-next = Layout > Stack > Suthela stack ka ho le letona
command-space-open = Layout > Space > Dibaka
command-terminal-close = Terminal > Kwala terminal
command-terminal-next = Terminal > Terminal e latelang
command-terminal-prev = Terminal > Terminal e fetileng
command-terminal-clear = Terminal > Hlakola terminal
command-browser-prev-page = Browser > Ho tsamaya > Morao
command-browser-next-page = Browser > Ho tsamaya > Pele
command-browser-reload = Browser > Ho tsamaya > Kenya hape
command-browser-hard-reload = Browser > Ho tsamaya > Kenya hape ka botlalo
command-open-in-place = Browser > Bula > Bula mona
command-open-in-new-stack = Browser > Bula > Bula ho stack e ntjha
command-open-in-pane-top = Browser > Bula > Bula pane e hodimo
command-open-in-pane-right = Browser > Bula > Bula pane e ka ho le letona
command-open-in-pane-bottom = Browser > Bula > Bula pane e tlase
command-open-in-pane-left = Browser > Bula > Bula pane e ka ho le letshehadi
command-open-in-new-tab = Browser > Bula > Bula thebeng e ntjha
command-open-in-new-space = Browser > Bula > Bula sebakeng se setjha
command-browser-zoom-in = Browser > Sheba > Hodisa
command-browser-zoom-out = Browser > Sheba > Nyenyefatsa
command-browser-zoom-reset = Browser > Sheba > Boholo ba nnete
command-browser-dev-tools = Browser > Sheba > Disebediswa tsa bahlahisi
command-browser-open-command-bar = Browser > Bara > Bara ya ditaelo
command-browser-open-page-in-command-bar = Browser > Bara > Fetola leqephe
command-browser-open-path-bar = Browser > Bara > Selaodi sa tsela
command-browser-open-commands = Browser > Bara > Ditaelo
command-browser-open-history = Browser > Bara > Nalane
command-service-open = Service > Bula sehlahlobi sa ditshebeletso
command-bookmark-toggle-active = Bookmark > Tshwaya leqephe
command-bookmark-pin-active = Bookmark > Pin-a leqephe

layout-tab = Thebe
layout-no-stacks = Ha ho distack
layout-loading = Ea kenya…
layout-no-markdown-files = Ha ho difaele tsa Markdown
layout-empty-folder = Foldara e se nang letho
layout-worktree = worktree
layout-folder-name = Lebitso la foldara
layout-no-pins-bookmarks = Ha ho dipin kapa matshwao a maqephe
layout-move-to = Suthela ho { $folder }
layout-bookmark-current-page = Tshwaya leqephe la hajoale
layout-rename-folder = Reha foldara lebitso bocha
layout-remove-folder = Tlosa foldara
layout-update-downloading = E jarolla ntjhafatso
layout-update-installing = E kenya ntjhafatso…
layout-update-ready = Mofuta o motjha o teng
layout-restart-update = Qala hape ho ntjhafatsa

agent-preparing = E lokisa agent…
agent-send-all-queued = Romela dipotso tsohle tse letetseng hona jwale (Esc)
agent-send = Romela (Enter)
agent-ready = Ke lokile ha o lokile.
agent-loading-older = E kenya melaetsa ya kgale…
agent-load-older = Kenya melaetsa ya kgale
agent-continued-from = E tswetswe pele ho tswa ho { $source }
agent-older-context-omitted = moelelo wa kgale o tlositswe
agent-interrupted = e sitisitswe
agent-allow-tool = Dumella { $tool }?
agent-deny = Hana
agent-allow-always = Dumella kamehla
agent-allow = Dumella
agent-loading-sessions = E kenya diseshene…
agent-no-resumable-sessions = Ha ho diseshene tse ka tswetswang pele tse fumanweng
agent-no-matching-sessions = Ha ho diseshene tse tshwanang
agent-no-matching-models = Ha ho dimodelo tse tshwanang
agent-choice-help = ↑/↓ kapa Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Kgetha foldara ya polokelo
agent-choose-repository-detail = Kgetha polokelo ya Git ya lehae eo agent e lokelang ho e sebedisa.
agent-choosing = E kgetha…
agent-choose-folder = Kgetha foldara
agent-queued = e behilwe moleng
agent-attached = E kgomaretswe:
agent-cancel-queued = Hlakola potso e moleng
agent-resume-queued = Tswetsa pele dipotso tse moleng
agent-clear-queue = Hlakola mola
agent-send-all-now = romela tsohle hona jwale
agent-choose-option = Kgetha kgetho e ka hodimo
agent-loading-media = E kenya media…
agent-no-matching-media = Ha ho media e tshwanang
agent-prompt-context = Moelelo wa potso
agent-details = Dintlha
agent-path = Tsela
agent-tool = Sesebediswa
agent-server = Seva
agent-bytes = { $count } bytes
agent-worked-for = E sebeditse metsotso { $duration }
agent-worked-for-steps = { $count ->
    [one] E sebeditse metsotso { $duration } · mohato o 1
   *[other] E sebeditse metsotso { $duration } · mehato e { $count }
}
agent-tool-guardian-review = Tlhahlobo ya Guardian
agent-tool-read-files = E badile difaele
agent-tool-viewed-image = E shebile setshwantsho
agent-tool-used-browser = E sebedisitse sebatli
agent-tool-searched-files = E batlile difaele
agent-tool-ran-commands = E tsamaisitse ditaelo
agent-thinking = E nahana
agent-subagent = Subagent
agent-prompt = Potso
agent-thread = Thapo
agent-parent = Motswadi
agent-children = Bana
agent-call = Pitso
agent-raw-event = Ketsahalo e tala
agent-plan = Moralo
agent-tasks = { $count ->
    [one] mosebetsi o 1
   *[other] mesebetsi e { $count }
}
agent-edited = E fetotswe
agent-reconnecting = E hokela hape { $attempt }/{ $total }
agent-status-running = E sebetsa
agent-status-done = E qetile
agent-status-failed = E hlotse
agent-status-pending = E emetse
agent-slash-attach-files = Kgomaretsa difaele
agent-slash-resume-session = Tswetsa pele seshene ya kgale
agent-slash-select-model = Kgetha model
agent-slash-continue-cli = Tswetsa pele seshene ena ho CLI
agent-session-just-now = hona jwale
agent-session-minutes-ago = mets { $count } e fetileng
agent-session-hours-ago = dihora tse { $count } tse fetileng
agent-session-days-ago = matsatsi a { $count } a fetileng
agent-working-working = E sebetsa
agent-working-thinking = E nahana
agent-working-pondering = E tebisitse maikutlo
agent-working-noodling = E qapa
agent-working-percolating = E butswela mohopolo
agent-working-conjuring = E hlahisa
agent-working-cooking = E pheha
agent-working-brewing = E butswa
agent-working-musing = E nahanisisa
agent-working-ruminating = E thuisa
agent-working-scheming = E rera
agent-working-synthesizing = E kopanya
agent-working-tinkering = E leka-leka
agent-working-churning = E sebetsa ka matla
agent-working-vibing = E kena moyeng
agent-working-simmering = E budula butle
agent-working-crafting = E bopa
agent-working-divining = E batla tharollo
agent-working-mulling = E nahanisisa
agent-working-spelunking = E phenya botebong

editor-toggle-explorer = Bontsha/pata Explorer (Cmd+B)
editor-unsaved = ha e so bolokwe
editor-rendered-markdown = Markdown e bontshitsweng ka ho hlophisa ka kotloloho
editor-note = Tlhokomeliso
editor-source-editor = Mohlophisi wa source
editor-editor = Mohlophisi
editor-git-diff = Phapang ya Git
editor-diff = Phapang
editor-tidy = Hlwekisa
editor-always = Kamehla
editor-unchanged-previews = { $count ->
    [one] ✦ preview e 1 e sa fetohang
   *[other] ✦ dipreview tse { $count } tse sa fetohang
}
editor-open-externally = Bula kantle
editor-changed-line = Mola o fetohileng
editor-go-to-definition = Eya tlhalosong
editor-find-references = Fumana ditshupiso
editor-references = { $count ->
    [one] tshupiso e 1
   *[other] ditshupiso tse { $count }
}
editor-lsp-starting = { $server } e qala…
editor-lsp-not-installed = { $server } — ha e a kenngwa
editor-explorer = Explorer
editor-open-editors = Bahlophisi ba bulehileng
editor-outline = Moralo
editor-new-file = Faele e ntjha
editor-new-folder = Foldara e ntjha
editor-delete-confirm = Hlakola “{ $name }”? Sena se ke ke sa etsollwa.
editor-created-folder = Foldara { $name } e entswe
editor-created-file = Faele { $name } e entswe
editor-renamed-to = E rehetswe ho { $name }
editor-deleted = { $name } e hlakotswe
editor-failed-decode-image = Ho hlolehile ho manolla setshwantsho
editor-preview-large-image = setshwantsho (se seholo haholo ho bontshwa pele)
editor-preview-binary = binary
editor-preview-file = faele

git-status-clean = e hlwekile
git-status-modified = e fetotswe
git-status-staged = e behilwe stage
git-status-staged-modified = staged*
git-status-untracked = ha e latellwe
git-status-deleted = e hlakotswe
git-status-conflict = kgohlano
git-accept-all = ✓ amohela tsohle
git-unstage = Tlosa stage
git-confirm-deny-all = Netefatsa ho hana tsohle
git-deny-all = ✗ hana tsohle
git-commit-message = molaetsa wa commit
git-commit = Commit ({ $count })
git-push = ↑ Sutumetsa
git-loading-diff = E kenya phapang…
git-no-changes = Ha ho diphetoho tse ka bontshwang
git-accept = ✓ amohela
git-deny = ✗ hana
git-show-unchanged-lines = Bontsha mela e { $count } e sa fetohang

terminal-loading = Ea kenya…
terminal-runs-when-ready = e sebetsa ha e lokile · Ctrl+C e hlakola · Esc e tlola
terminal-booting = e qala
terminal-type-command = thaepa taelo · e sebetsa ha e lokile · Esc e tlola

setup-tagline-claude = Agent ya Anthropic ya ho khoda, ho Vmux
setup-tagline-codex = Agent ya OpenAI ya ho khoda, ho Vmux
setup-tagline-vibe = Agent ya Mistral ya ho khoda, ho Vmux
setup-install-title = Kenya { $name } CLI
setup-homebrew-required = Homebrew e a hlokahala ho kenya { $command } mme ha e so hlophiswe. Vmux e tla kenya Homebrew pele, ebe { $name }.
setup-terminal-instructions = Ho terminal, tobetsa Return ho qala, ebe kenya phasewete ya Mac ha o kopuwa.
setup-command-missing = Vmux e butse leqephe lena hobane taelo ya lehae ya { $command } ha e so kengwe. Sebedisa taelo e ka tlase ho e fumana.
setup-install-failed = Ho kenya ha hoa phetheha. Sheba terminal bakeng sa dintlha, ebe o leka hape.
setup-installing = E kenya…
setup-install-homebrew = Kenya Homebrew + { $name }
setup-run-install = Sebedisa taelo ya ho kenya
setup-auto-reload = Vmux e e tsamaisa ho terminal mme e kenya hape ha { $command } e se e lokile.

debug-title = Lokisa diphoso
debug-auto-update = Intjhafatso ya othomathiki
debug-simulate-update = Etsa eka ntjhafatso e teng
debug-simulate-download = Etsa eka ho a jarollwa
debug-clear-update = Hlakola ntjhafatso
debug-trigger-restart = Qalisa ho qala hape

command-manage-spaces = Laola libaka…
command-pane-stack-location = pane { $pane } / mokgobo { $stack }
command-space-pane-stack-location = { $space } / pane { $pane } / mokgobo { $stack }
command-terminal-path = Theminale ({ $path })
command-group-interactive-mode = Mokgwa wa tshebedisano
command-group-window = Fensetere
command-group-tab = Thebo
command-group-pane = Pane
command-group-stack = Mokgobo
command-group-space = Sebaka
command-group-navigation = Tsamaiso
command-group-open = Bula
command-group-view = Sheba
command-group-bar = Bara

menu-close-vmux = Kwala Vmux

agents-terminal-coding-agent = Ajente ya ho khoda e sebetsang ka theminale
