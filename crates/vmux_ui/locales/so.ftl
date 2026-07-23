locale-name = Soomaali
common-open = Fur
common-close = Xir
common-install = Rakib
common-uninstall = Ka saar
common-update = Cusboonaysii
common-retry = Mar kale isku day
common-refresh = Cusboonaysii
common-remove = Ka saar
common-enable = Daar
common-disable = Dami
common-new = Cusub
common-active = firfircoon
common-running = socda
common-done = dhammaaday
common-failed = Fashilmay
common-installed = La rakibay
common-items = { $count ->
    [one] { $count } shay
   *[other] { $count } shay
}

tools-title = Qalab
tools-search = Raadi xirmooyinka, wakiillada, MCP, qalabka luqadda iyo faylasha habaynta…
tools-open = Fur qalabka
tools-fold = Laab qalabka
tools-unfold = Kala bixi qalabka
tools-scanning = Baaraya qalabka maxalliga ah…
tools-no-installed = Qalab rakiban ma jiro
tools-empty = Qalab ku habboon ma jiro
tools-empty-detail = Rakib xirmo ama ku dar xirmo faylal habayn ah oo qaabka Stow ah.
tools-apply = Dhaqan geli
tools-homebrew = Homebrew
tools-homebrew-sync = Qaacidooyinka iyo barnaamijyada rakiban si toos ah ayay isu waafajiyaan.
tools-open-brewfile = Fur Brewfile
tools-managed = la maamulo
tools-provider-homebrew-formulae = Qaacidooyinka Homebrew
tools-provider-homebrew-casks = Barnaamijyada Homebrew
tools-provider-npm = Xirmooyinka npm
tools-provider-acp-agents = Wakiillada ACP
tools-provider-language-tools = Qalabka luqadda
tools-provider-mcp-servers = Adeegayaasha MCP
tools-provider-dotfiles = Faylasha habaynta
tools-status-available = La heli karo
tools-status-missing = Maqan
tools-status-conflict = Iskahorimaad
tools-forget = Illow
tools-manage = Maamul
tools-link = Xiriiri
tools-unlink = Xiriirka ka saar
tools-import = Soo dejiso
tools-update-count = { $count ->
    [one] 1 cusboonaysiin
   *[other] { $count } cusboonaysiin
}
tools-conflict-count = { $count ->
    [one] 1 iskahorimaad
   *[other] { $count } iskahorimaad
}
tools-result-applied = Qalabka waa la dhaqan geliyey
tools-result-imported = Qalabka waa la soo dejiyey
tools-result-installed = { $name } waa la rakibay
tools-result-updated = { $name } waa la cusboonaysiiyey
tools-result-uninstalled = { $name } waa la saaray
tools-result-forgotten = { $name } waa la illoobay
tools-result-managed = { $name } hadda waa la maamulaa
tools-result-linked = { $name } waa la xiriiriyey
tools-result-unlinked = Xiriirka { $name } waa la saaray

start-title = Bilow
start-tagline = Hal amar. Wax kasta, waa dhammaaday.

agents-title = Ajanada
agents-search = Ka raadi ajanada ACP iyo CLI…
agents-empty = Ajan u dhigma lama helin
agents-empty-detail = Isku day magac, runtime, ama ACP/CLI.
agents-install-failed = Rakibiddu way fashilantay
agents-updating = Waa la cusboonaysiinayaa…
agents-retrying = Mar kale ayaa la isku dayayaa…
agents-preparing = Waa la diyaarinayaa…

extensions-title = Fidinnada
extensions-search = Ka raadi kuwa rakiban ama Chrome Web Store…
extensions-relaunch = Dib u fur si ay u dhaqan gasho
extensions-empty = Fidinna lama rakibin
extensions-no-match = Fidinnad u dhigma lama helin
extensions-empty-detail = Ka raadi Chrome Web Store kore, dabadeed Riix Return.
extensions-no-match-detail = Isku day magac kale ama aqoonsiga fidinta.
extensions-on = Daaran
extensions-off = Dansan
extensions-enable-confirm = Daar { $name }?
extensions-enable-permissions = Daar { $name } oo oggolow:

lsp-title = Adeegayaasha Luqadaha
lsp-search = Raadi adeegayaasha luqadaha, linters, formatters…
lsp-loading = Liiska ayaa la rarayaa…
lsp-empty = Adeegayaal luqadeed oo u dhigma lama helin
lsp-empty-detail = Isku day luqad kale, linter, ama formatter.
lsp-needs = wuxuu u baahan yahay { $tool }
lsp-status-available = La heli karo
lsp-status-on-path = PATH ku jira
lsp-status-installing = Waa la rakibayaa…
lsp-status-installed = La rakibay
lsp-status-outdated = Cusboonaysiin baa jirta
lsp-status-running = Socda
lsp-status-failed = Fashilmay

spaces-title = Goobaha
spaces-new-placeholder = Magaca goobta cusub
spaces-empty = Goobo ma jiraan
spaces-default-name = Goob { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
spaces-delete = Tirtir goobta

team-title = Koox
team-just-you = Adiga keliya ayaa goobtan ku jira
team-agents = { $count ->
    [one] Adiga iyo 1 ajan
   *[other] Adiga iyo { $count } ajan
}
team-empty = Weli cidna halkan ma joogto
team-you = Adiga
team-agent = Ajan

services-title = Adeegyada Gadaal
services-processes = { $count ->
    [one] 1 geeddi-socod
   *[other] { $count } geeddi-socod
}
services-kill-all = Dhammaan khasab ku jooji
services-not-running = Adeeggu ma socdo
services-start-with = Ku bilow:
services-empty = Geeddi-socodyo firfircoon ma jiraan
services-filter = Sifee geeddi-socodyada…
services-no-match = Geeddi-socodyo u dhigma lama helin
services-connected = Ku xiran
services-disconnected = Go’an
services-attached = ku lifaaqan
services-kill = Khasab ku jooji
services-memory = Xusuus
services-size = Cabbir
services-shell = Shell

error-title = Khalad

history-search = Raadi taariikhda
history-clear-all = Dhammaan nadiifi
history-clear-confirm = Ma nadiifinaysaa taariikhda oo dhan?
history-clear-warning = Tan dib looma celin karo.
history-cancel = Jooji
history-today = Maanta
history-yesterday = Shalay
history-days-ago = { $count } maalmood ka hor
history-day-offset = Maalin -{ $count }

settings-title = Dejimaha
settings-loading = Dejimaha ayaa la rarayaa…
settings-stored = Waxaa lagu kaydiyey ~/.vmux/settings.ron
settings-other = Kale
settings-software-update = Cusboonaysiinta Barnaamijka
settings-check-updates = Hubi cusboonaysiin
settings-check-updates-hint = Si toos ah ayuu u hubiyaa marka la bilaabo iyo saacad kasta marka Auto-update daaran yahay.
settings-update-unavailable = Lama heli karo
settings-update-unavailable-hint = Cusboonaysiiye kuma jiro dhismahan.
settings-update-checking = Waa la hubinayaa…
settings-update-checking-hint = Cusboonaysiin ayaa la hubinayaa…
settings-update-check-again = Mar kale hubi
settings-update-current = Vmux waa cusub yahay.
settings-update-downloading = Waa la soo dejinayaa…
settings-update-downloading-hint = Vmux { $version } ayaa la soo dejinayaa…
settings-update-installing = Waa la rakibayaa…
settings-update-installing-hint = Vmux { $version } ayaa la rakibayaa…
settings-update-ready = Cusboonaysiin diyaar ah
settings-update-ready-hint = Vmux { $version } waa diyaar. Dib u bilow si ay u dhaqan gasho.
settings-update-try-again = Mar kale isku day
settings-update-failed = Cusboonaysiin lama hubin karo.
settings-item = Shay
settings-item-number = Shay { $number }
settings-press-key = Riix furaha…
settings-saved = La kaydiyey
settings-record-key = Guji si aad u duubto isku-darka furayaal cusub

tray-open-window = Fur daaqad
tray-close-window = Xir daaqad
tray-pause-recording = Hakad geli duubista
tray-resume-recording = Sii wad duubista
tray-finish-recording = Dhammee duubista
tray-quit = Ka bax Vmux

composer-attach-files = Ku lifaaq faylal (/upload)
composer-remove-attachment = Ka saar lifaaqa

layout-back = Dib
layout-forward = Hore
layout-reload = Dib u rar
layout-bookmark-page = Boggan calaamadee
layout-remove-bookmark = Ka saar calaamadda
layout-pin-page = Boggan ku dheji
layout-unpin-page = Ka fur boggan
layout-manage-extensions = Maamul fidinnada
layout-new-stack = Lakab cusub
layout-close-tab = Xir tab
layout-bookmark = Calaamadee
layout-pin = Ku dheji
layout-new-tab = Tab cusub
layout-team = Koox

command-switch-space = Beddel goobta…
command-search-ask = Raadi ama weydii…
command-new-tab-placeholder = Raadi ama qor URL, ama dooro Terminal…
command-placeholder = Qor URL, raadi tab, ama > amarrada…
command-composer-placeholder = Qor / amarrada ama @ warbaahinta
command-send = Dir (Enter)
command-terminal = Terminal
command-open-terminal = Ku fur Terminal
command-stack = Lakab
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
command-prompt = Amar
command-new-tab = Tab cusub
command-search = Raadi
command-open-value = Fur “{ $value }”
command-search-value = Raadi “{ $value }”

schema-appearance = Muuqaal
schema-general = Guud
schema-layout = Qaabayn
schema-layout-detail = Daaqad, qaybo, dhinac, iyo giraanta diiradda.
schema-agent = Ajan
schema-agent-detail = Habdhaqanka ajanka iyo oggolaanshaha qalabka.
schema-shortcuts = Gaagaabiyeyaasha
schema-shortcuts-detail = Aragti akhris-keliya ah. Si aad u beddesho isku-xirnaanta, si toos ah u tafatir settings.ron.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Hab
schema-mode-detail = Qaabka midabka ee bogagga webka. Device wuxuu raacaa nidaamkaaga.
schema-device = Device
schema-light = Iftiin
schema-dark = Madow
schema-language = Luqad
schema-language-detail = Adeegso nidaamka, en-US, ja, ama summad kasta oo BCP 47 ah oo leh katalog ~/.vmux/locales/<tag>.ftl u dhigma.
schema-auto-update = Auto-update
schema-auto-update-detail = Hubi oo rakib cusboonaysiin marka la bilaabo iyo saacad kasta.
schema-startup-url = URL-ka bilowga
schema-startup-url-detail = Haddii uu madhan yahay, wuxuu furaa amarka baarka amarrada.
schema-search-engine = Matoorka raadinta
schema-search-engine-detail = Waxaa loo adeegsadaa raadinta webka ee Bilowga iyo baarka amarrada.
schema-window = Daaqad
schema-pane = Qayb
schema-side-sheet = Xaashi dhinac
schema-focus-ring = Giraanta diiradda
schema-run-placement = Oggolow in meelaynta socodsiinta la beddelo
schema-run-placement-detail = U oggolow ajanada inay doortaan habka qaybta socodsiinta, jihada, iyo barroosinka.
schema-leader = Horgeeye
schema-leader-detail = Furaha hordhaca ee gaagaabiyeyaasha chord.
schema-chord-timeout = Waqtiga chord
schema-chord-timeout-detail = Millisekanno ka hor inta horudhaca chord-ku dhicin.
schema-bindings = Isku-xirnaanno
schema-confirm-close = Xaqiiji xiritaanka
schema-confirm-close-detail = Weydii ka hor inta aan la xirin terminal leh geeddi-socod socda.
schema-default-theme = Dulucda caadiga ah
schema-default-theme-detail = Magaca dulucda firfircoon ee liiska dulucyada.

settings-empty = (madhan)
settings-none = (midna)

schema-system = Nidaam
schema-editor = Tafatire
schema-recording = Duubis
schema-radius = Wareeg
schema-padding = Suufayn
schema-gap = Farqi
schema-width = Ballac
schema-color = Midab
schema-red = Cas
schema-green = Cagaar
schema-blue = Buluug
schema-follow-files = Raac faylasha
schema-tidy-files = Hagaaji faylasha
schema-tidy-files-max = Xadka hagaajinta faylasha
schema-tidy-files-auto = Si toos ah u hagaaji faylasha
schema-app-providers = Bixiyeyaasha abka
schema-provider = Bixiye
schema-kind = Nooc
schema-models = Moodallo
schema-acp = Wakiillada ACP
schema-id = ID
schema-name = Magac
schema-command = Amar
schema-arguments = Doodo
schema-environment = Deegaan
schema-working-directory = Galka shaqada
schema-shell = Shell
schema-font-family = Qoyska farta
schema-startup-directory = Galka bilowga
schema-themes = Mawduucyo
schema-color-scheme = Qaabka midabka
schema-font-size = Cabbirka farta
schema-line-height = Dhererka sadar
schema-cursor-style = Qaabka tilmaamaha
schema-cursor-blink = Biligga tilmaamaha
schema-custom-themes = Mawduucyo gaar ah
schema-foreground = Hore
schema-background = Gadaal
schema-cursor = Tilmaame
schema-ansi-colors = Midabada ANSI
schema-keymap = Khariidadda furayaasha
schema-explorer = Sahamiye
schema-visible = Muuqda
schema-language-servers = Adeegayaasha luqadda
schema-servers = Adeegayaal
schema-language-id = ID-ga luqadda
schema-root-markers = Calaamadaha xididka
schema-output-directory = Galka wax-soo-saarka

menu-scene = Muuqaal
menu-layout = Habayn
menu-terminal = Terminal
menu-browser = Biraawsar
menu-service = Adeeg
menu-bookmark = Calaamad
menu-edit = Tafatir

layout-knowledge = Aqoon
layout-open-knowledge = Fur Aqoon
layout-open-welcome-knowledge = Fur Ku soo dhawoow Aqoon
layout-open-path = Fur { $path }
layout-fold-knowledge = Laab aqoonta
layout-unfold-knowledge = Kala bixi aqoonta
layout-bookmarks = Calaamado
layout-new-folder = Gal cusub
layout-add-to-bookmarks = Ku dar Calaamadaha
layout-move-to-bookmarks = U rar Calaamadaha
layout-stack-number = Ras { $number }
layout-fold-stack = Laab raska
layout-unfold-stack = Kala bixi raska
layout-close-stack = Xir raska
layout-bookmark-in = Ku calaamadee { $folder }

common-cancel = Jooji
common-delete = Tirtir
common-save = Kaydi
common-rename = Magac beddel
common-expand = Ballaadhi
common-collapse = Isku laab
common-loading = Soo raranaya…
common-error = Khalad
common-output = Natiijo
common-pending = Sugaya
common-current = hadda
common-stop = Jooji
services-command = Adeegga Vmux
services-uptime-seconds = { $seconds } ilbiriqsi
services-uptime-minutes = { $minutes } daqiiqo { $seconds } ilbiriqsi
services-uptime-hours = { $hours } saac { $minutes } daqiiqo
services-uptime-days = { $days } maalmood { $hours } saac

error-page-failed-load = Boggu wuu rarmi waayay
error-page-not-found = Bogga lama helin
error-unknown-host = Martigeliye app Vmux oo aan la aqoon: { $host }

history-title = Taariikh

command-new-app-chat = Wadahadal cusub { $provider }/{ $model } (App)
command-interactive-mode-user = Muuqaal > Habka Isdhexgalka > Isticmaale
command-interactive-mode-player = Muuqaal > Habka Isdhexgalka > Ciyaaryahan
command-minimize-window = Habayn > Daaqad > Yaree
command-toggle-layout = Habayn > Habayn > Beddel habaynta
command-close-tab = Habayn > Tab > Xir tabka
command-new-task = Habayn > Tab > Hawl cusub…
command-next-tab = Habayn > Tab > Tabka xiga
command-prev-tab = Habayn > Tab > Tabkii hore
command-rename-tab = Habayn > Tab > Magac beddel tabka
command-tab-select-1 = Habayn > Tab > Dooro tabka 1
command-tab-select-2 = Habayn > Tab > Dooro tabka 2
command-tab-select-3 = Habayn > Tab > Dooro tabka 3
command-tab-select-4 = Habayn > Tab > Dooro tabka 4
command-tab-select-5 = Habayn > Tab > Dooro tabka 5
command-tab-select-6 = Habayn > Tab > Dooro tabka 6
command-tab-select-7 = Habayn > Tab > Dooro tabka 7
command-tab-select-8 = Habayn > Tab > Dooro tabka 8
command-tab-select-last = Habayn > Tab > Dooro tabka ugu dambeeya
command-close-pane = Habayn > Qayb > Xir qaybta
command-select-pane-left = Habayn > Qayb > Dooro qaybta bidix
command-select-pane-right = Habayn > Qayb > Dooro qaybta midig
command-select-pane-up = Habayn > Qayb > Dooro qaybta kore
command-select-pane-down = Habayn > Qayb > Dooro qaybta hoose
command-swap-pane-prev = Habayn > Qayb > Isweydaari qaybta hore
command-swap-pane-next = Habayn > Qayb > Isweydaari qaybta xigta
command-equalize-pane-size = Habayn > Qayb > Sim cabbirka qaybaha
command-resize-pane-left = Habayn > Qayb > U cabbir qaybta bidix
command-resize-pane-right = Habayn > Qayb > U cabbir qaybta midig
command-resize-pane-up = Habayn > Qayb > U cabbir qaybta kore
command-resize-pane-down = Habayn > Qayb > U cabbir qaybta hoose
command-stack-close = Habayn > Lakab > Xir lakabka
command-stack-next = Habayn > Lakab > Lakabka xiga
command-stack-previous = Habayn > Lakab > Lakabkii hore
command-stack-reopen = Habayn > Lakab > Dib u fur boggii la xiray
command-stack-swap-prev = Habayn > Lakab > U rar lakabka bidix
command-stack-swap-next = Habayn > Lakab > U rar lakabka midig
command-space-open = Habayn > Goob > Goobo
command-terminal-close = Terminal > Xir Terminal-ka
command-terminal-next = Terminal > Terminal-ka xiga
command-terminal-prev = Terminal > Terminal-kii hore
command-terminal-clear = Terminal > Nadiifi Terminal-ka
command-browser-prev-page = Baraawsar > Socod > Dib
command-browser-next-page = Baraawsar > Socod > Hore
command-browser-reload = Baraawsar > Socod > Dib u rar
command-browser-hard-reload = Baraawsar > Socod > Dib u rar buuxa
command-open-in-place = Baraawsar > Fur > Halkan ka fur
command-open-in-new-stack = Baraawsar > Fur > Ku fur lakab cusub
command-open-in-pane-top = Baraawsar > Fur > Ku fur qaybta kore
command-open-in-pane-right = Baraawsar > Fur > Ku fur qaybta midig
command-open-in-pane-bottom = Baraawsar > Fur > Ku fur qaybta hoose
command-open-in-pane-left = Baraawsar > Fur > Ku fur qaybta bidix
command-open-in-new-tab = Baraawsar > Fur > Ku fur tab cusub
command-open-in-new-space = Baraawsar > Fur > Ku fur goob cusub
command-browser-zoom-in = Baraawsar > Muuqaal > Weynee
command-browser-zoom-out = Baraawsar > Muuqaal > Yaree
command-browser-zoom-reset = Baraawsar > Muuqaal > Cabbirka dhabta ah
command-browser-dev-tools = Baraawsar > Muuqaal > Qalabka horumariyaha
command-browser-open-command-bar = Baraawsar > Bar > Baarka amarrada
command-browser-open-page-in-command-bar = Baraawsar > Bar > Tafatir bogga
command-browser-open-path-bar = Baraawsar > Bar > Hagaha waddada
command-browser-open-commands = Baraawsar > Bar > Amarro
command-browser-open-history = Baraawsar > Bar > Taariikh
command-service-open = Adeeg > Fur kormeeraha adeegga
command-bookmark-toggle-active = Calaamad > Calaamadee bogga
command-bookmark-pin-active = Calaamad > Ku dheji bogga

layout-tab = Tab
layout-no-stacks = Lakabyo ma jiraan
layout-loading = Soo raranaya…
layout-no-markdown-files = Faylal Markdown ma jiraan
layout-empty-folder = Gal madhan
layout-worktree = geed-shaqo
layout-folder-name = Magaca galka
layout-no-pins-bookmarks = Ku-dhejin ama calaamado ma jiraan
layout-move-to = U rar { $folder }
layout-bookmark-current-page = Calaamadee bogga hadda
layout-rename-folder = Magac beddel galka
layout-remove-folder = Ka saar galka
layout-update-downloading = Cusboonaysiin ayaa la soo dejinayaa
layout-update-installing = Cusboonaysiin ayaa la rakibayaa…
layout-update-ready = Nooc cusub ayaa diyaar ah
layout-restart-update = Dib u bilow si loo cusboonaysiiyo

agent-preparing = Wakiilka waa la diyaarinayaa…
agent-send-all-queued = Hadda dir dhammaan codsiyada safka ku jira (Esc)
agent-send = Dir (Enter)
agent-ready = Diyaar baan ahay markaad diyaar tahay.
agent-loading-older = Fariimihii hore ayaa la raranayaa…
agent-load-older = Rar fariimihii hore
agent-continued-from = Laga sii waday { $source }
agent-older-context-omitted = macnaha hore waa laga tagay
agent-interrupted = la hakiyay
agent-allow-tool = Ma oggolaataa { $tool }?
agent-deny = Diid
agent-allow-always = Had iyo jeer oggolow
agent-allow = Oggolow
agent-loading-sessions = Fadhiyo ayaa la raranayaa…
agent-no-resumable-sessions = Fadhiyo dib loo bilaabi karo lama helin
agent-no-matching-sessions = Fadhiyo ku habboon ma jiraan
agent-no-matching-models = Moodallo ku habboon ma jiraan
agent-choice-help = ↑/↓ ama Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Dooro galka kaydka
agent-choose-repository-detail = Dooro kaydka Git ee maxalliga ah ee wakiilku adeegsanayo.
agent-choosing = Dooranaya…
agent-choose-folder = Dooro gal
agent-queued = saf ku jira
agent-attached = Ku lifaaqan:
agent-cancel-queued = Jooji codsiga safka ku jira
agent-resume-queued = Dib u bilow codsiyada safka ku jira
agent-clear-queue = Nadiifi safka
agent-send-all-now = hadda dhammaan dir
agent-choose-option = Dooro ikhtiyaar kor ku yaal
agent-loading-media = Warbaahin ayaa la raranayaa…
agent-no-matching-media = Warbaahin ku habboon ma jirto
agent-prompt-context = Macnaha codsiga
agent-details = Faahfaahin
agent-path = Waddo
agent-tool = Qalab
agent-server = Seerfar
agent-bytes = { $count } bayt
agent-worked-for = Shaqeeyay { $duration }
agent-worked-for-steps = { $count ->
    [one] Shaqeeyay { $duration } · 1 tallaabo
   *[other] Shaqeeyay { $duration } · { $count } tallaabo
}
agent-tool-guardian-review = Dib-u-eegista Ilaaliyaha
agent-tool-read-files = Akhriyey faylal
agent-tool-viewed-image = Daawaday sawir
agent-tool-used-browser = Adeegsaday baraawsar
agent-tool-searched-files = Baaray faylal
agent-tool-ran-commands = Fuliyay amarro
agent-thinking = Ka fikiraya
agent-subagent = Wakiil-hoosaad
agent-prompt = Codsi
agent-thread = Taxane
agent-parent = Waalid
agent-children = Carruur
agent-call = Wicitaan
agent-raw-event = Dhacdo cayriin
agent-plan = Qorshe
agent-tasks = { $count ->
    [one] 1 hawl
   *[other] { $count } hawlood
}
agent-edited = La tafatiray
agent-reconnecting = Dib u xirmaya { $attempt }/{ $total }
agent-status-running = Socda
agent-status-done = La dhammeeyay
agent-status-failed = Fashilmay
agent-status-pending = Sugaya
agent-slash-attach-files = Ku lifaaq faylal
agent-slash-resume-session = Dib u bilow fadhi hore
agent-slash-select-model = Dooro moodal
agent-slash-continue-cli = Ku sii wad fadhigan CLI-ga
agent-session-just-now = hadda uun
agent-session-minutes-ago = { $count } daqiiqo ka hor
agent-session-hours-ago = { $count } saac ka hor
agent-session-days-ago = { $count } maalmood ka hor
agent-working-working = Shaqaynaya
agent-working-thinking = Ka fikiraya
agent-working-pondering = Ka baaraan-degaya
agent-working-noodling = Fikrado rogrogiya
agent-working-percolating = Bislaanaya
agent-working-conjuring = Soo saaraya
agent-working-cooking = Kariyaya
agent-working-brewing = Diyaarinaya
agent-working-musing = Milicsanaya
agent-working-ruminating = Ku celcelinaya fikirka
agent-working-scheming = Qorshaynaya
agent-working-synthesizing = Isku ururinaya
agent-working-tinkering = Tijaabinaya
agent-working-churning = Ka shaqaynaya
agent-working-vibing = La jaanqaadaya
agent-working-simmering = Si tartiib ah u bislaynaya
agent-working-crafting = Sameynaya
agent-working-divining = Raadraacaya
agent-working-mulling = Ka fikiraya
agent-working-spelunking = Qodqodaya

editor-toggle-explorer = Daar/dami Explorer (Cmd+B)
editor-unsaved = aan la kaydin
editor-rendered-markdown = Markdown la soo bandhigay oo tafatir toos ah leh
editor-note = Qoraal
editor-source-editor = Tafatiraha koodhka
editor-editor = Tafatire
editor-git-diff = Farqiga Git
editor-diff = Farqi
editor-tidy = Nadiifi
editor-always = Had iyo jeer
editor-unchanged-previews = { $count ->
    [one] ✦ 1 hordhac aan isbeddelin
   *[other] ✦ { $count } hordhac oo aan isbeddelin
}
editor-open-externally = Dibadda ku fur
editor-changed-line = Sadarka la beddelay
editor-go-to-definition = U gudub qeexidda
editor-find-references = Raadi tixraacyo
editor-references = { $count ->
    [one] 1 tixraac
   *[other] { $count } tixraac
}
editor-lsp-starting = { $server } wuu bilaabmayaa…
editor-lsp-not-installed = { $server } — lama rakibin
editor-explorer = Explorer
editor-open-editors = Tafatirayaal furan
editor-outline = Dulmar
editor-new-file = Fayl cusub
editor-new-folder = Gal cusub
editor-delete-confirm = Ma tirtiraysaa “{ $name }”? Tan lama soo celin karo.
editor-created-folder = Gal ayaa la abuuray { $name }
editor-created-file = Fayl ayaa la abuuray { $name }
editor-renamed-to = Waxaa loo beddelay { $name }
editor-deleted = La tirtiray { $name }
editor-failed-decode-image = Sawirka waa la furfuri kari waayay
editor-preview-large-image = sawir (aad buu ugu weyn yahay hordhac)
editor-preview-binary = binary
editor-preview-file = fayl

git-status-clean = nadiif
git-status-modified = la beddelay
git-status-staged = la diyaariyay
git-status-staged-modified = la diyaariyay*
git-status-untracked = aan la raacin
git-status-deleted = la tirtiray
git-status-conflict = iskhilaaf
git-accept-all = ✓ aqbal dhammaan
git-unstage = Ka saar diyaarinta
git-confirm-deny-all = Xaqiiji diidmada dhammaan
git-deny-all = ✗ diid dhammaan
git-commit-message = farriinta commit-ka
git-commit = Commit ({ $count })
git-push = ↑ Riix
git-loading-diff = Farqi ayaa la raranayaa…
git-no-changes = Isbeddello la muujiyo ma jiraan
git-accept = ✓ aqbal
git-deny = ✗ diid
git-show-unchanged-lines = Muuji { $count } sadar oo aan isbeddelin

terminal-loading = Soo raranaya…
terminal-runs-when-ready = wuu soconayaa marka uu diyaar noqdo · Ctrl+C nadiifiya · Esc ka boodaya
terminal-booting = bilaabmaya
terminal-type-command = qor amar · wuu soconayaa marka uu diyaar noqdo · Esc ka boodaya

setup-tagline-claude = Wakiilka koodh-qorista Anthropic, gudaha Vmux
setup-tagline-codex = Wakiilka koodh-qorista OpenAI, gudaha Vmux
setup-tagline-vibe = Wakiilka koodh-qorista Mistral, gudaha Vmux
setup-install-title = Rakib CLI-ga { $name }
setup-homebrew-required = Homebrew ayaa loo baahan yahay si loo rakibo { $command }, welina lama dejin. Vmux ayaa marka hore rakibi doona Homebrew, kadibna { $name }.
setup-terminal-instructions = Terminal-ka dhexdiisa, riix Return si aad u bilowdo, kadibna geli erayga sirta ah ee Mac-gaaga marka lagu weydiiyo.
setup-command-missing = Vmux wuxuu furay boggan sababtoo ah amarka maxalliga ah { $command } weli lama rakibin. Ful amar hoos ku qoran si aad u hesho.
setup-install-failed = Rakibiddu ma dhammaan. Terminal-ka ka eeg faahfaahinta, kadibna mar kale isku day.
setup-installing = La rakibayaa…
setup-install-homebrew = Rakib Homebrew + { $name }
setup-run-install = Ful amarka rakibidda
setup-auto-reload = Vmux wuxuu ku socodsiiyaa terminal, wuuna dib u raraa marka { $command } diyaar noqdo.

debug-title = Cilad-baaris
debug-auto-update = Is-cusboonaysiin
debug-simulate-update = Jil cusboonaysiin diyaar ah
debug-simulate-download = Jil soo dejin
debug-clear-update = Nadiifi cusboonaysiinta
debug-trigger-restart = Kici dib-u-bilow

command-manage-spaces = Maamul goobaha…
command-pane-stack-location = daaqad-qayb { $pane } / lakab { $stack }
command-space-pane-stack-location = { $space } / daaqad-qayb { $pane } / lakab { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Habka is-dhexgalka
command-group-window = Daaqad
command-group-tab = Tab
command-group-pane = Daaqad-qayb
command-group-stack = Lakab
command-group-space = Goob
command-group-navigation = Hagid
command-group-open = Fur
command-group-view = Muuqaal
command-group-bar = Baar

menu-close-vmux = Xir Vmux

agents-terminal-coding-agent = Wakiilka koodh-qorista ee Terminal ku salaysan
