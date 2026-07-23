locale-name = հայերեն
common-open = Բացել
common-close = Փակել
common-install = Տեղադրել
common-uninstall = Հեռացնել տեղադրումը
common-update = Թարմացնել
common-retry = Կրկնել
common-refresh = Վերաթարմացնել
common-remove = Հեռացնել
common-enable = Միացնել
common-disable = Անջատել
common-new = Նոր
common-active = ակտիվ
common-running = աշխատում է
common-done = պատրաստ է
common-failed = Ձախողվեց
common-installed = Տեղադրված է
common-items = { $count ->
    [one] { $count } տարր
   *[other] { $count } տարր
}

tools-title = Գործիքներ
tools-search = Փնտրել փաթեթներ, գործակալներ, MCP, լեզվական գործիքներ և կազմաձևման ֆայլեր…
tools-open = Բացել գործիքները
tools-fold = Ծալել գործիքները
tools-unfold = Բացել գործիքների ցանկը
tools-scanning = Տեղային գործիքների սկանավորում…
tools-no-installed = Տեղադրված գործիքներ չկան
tools-empty = Համապատասխան գործիքներ չկան
tools-empty-detail = Տեղադրեք փաթեթ կամ ավելացրեք Stow ոճի կազմաձևման ֆայլերի փաթեթ։
tools-apply = Կիրառել
tools-homebrew = Homebrew
tools-homebrew-sync = Տեղադրված բանաձևերն ու հավելվածները ինքնաբերաբար համաժամացվում են։
tools-open-brewfile = Բացել Brewfile-ը
tools-managed = կառավարվող
tools-provider-homebrew-formulae = Homebrew բանաձևեր
tools-provider-homebrew-casks = Homebrew հավելվածներ
tools-provider-npm = npm փաթեթներ
tools-provider-acp-agents = ACP գործակալներ
tools-provider-language-tools = Լեզվական գործիքներ
tools-provider-mcp-servers = MCP սերվերներ
tools-provider-dotfiles = Կազմաձևման ֆայլեր
tools-status-available = Հասանելի
tools-status-missing = Բացակայում է
tools-status-conflict = Հակասություն
tools-forget = Մոռանալ
tools-manage = Կառավարել
tools-link = Կապել
tools-unlink = Անջատել
tools-import = Ներմուծել
tools-update-count = { $count ->
    [one] 1 թարմացում
   *[other] { $count } թարմացում
}
tools-conflict-count = { $count ->
    [one] 1 հակասություն
   *[other] { $count } հակասություն
}
tools-result-applied = Գործիքները կիրառվեցին
tools-result-imported = Գործիքները ներմուծվեցին
tools-result-installed = { $name }-ը տեղադրվեց
tools-result-updated = { $name }-ը թարմացվեց
tools-result-uninstalled = { $name }-ը հեռացվեց
tools-result-forgotten = { $name }-ը մոռացվեց
tools-result-managed = { $name }-ն այժմ կառավարվում է
tools-result-linked = { $name }-ը կապվեց
tools-result-unlinked = { $name }-ն անջատվեց

start-title = Սկիզբ
start-tagline = Մեկ prompt, և ամեն ինչ՝ պատրաստ։

agents-title = Գործակալներ
agents-search = Որոնել ACP և CLI գործակալներ…
agents-empty = Համապատասխան գործակալներ չկան
agents-empty-detail = Փորձեք անուն, runtime կամ ACP/CLI։
agents-install-failed = Տեղադրումը ձախողվեց
agents-updating = Թարմացվում է…
agents-retrying = Կրկին փորձ է արվում…
agents-preparing = Նախապատրաստվում է…

extensions-title = Ընդլայնումներ
extensions-search = Որոնել տեղադրվածներում կամ Chrome Web Store-ում…
extensions-relaunch = Կիրառելու համար վերագործարկեք
extensions-empty = Ընդլայնումներ տեղադրված չեն
extensions-no-match = Համապատասխան ընդլայնումներ չկան
extensions-empty-detail = Վերևում որոնեք Chrome Web Store-ում և սեղմեք Enter։
extensions-no-match-detail = Փորձեք այլ անուն կամ ընդլայնման ID։
extensions-on = Միացված
extensions-off = Անջատված
extensions-enable-confirm = Միացնե՞լ { $name }-ը։
extensions-enable-permissions = Միացնել { $name }-ը և թույլատրել՝

lsp-title = Լեզվային սերվերներ
lsp-search = Որոնել լեզվային սերվերներ, լինթերներ, ձևաչափիչներ…
lsp-loading = Կատալոգը բեռնվում է…
lsp-empty = Համապատասխան լեզվային սերվերներ չկան
lsp-empty-detail = Փորձեք այլ լեզու, լինթեր կամ ձևաչափիչ։
lsp-needs = պահանջում է { $tool }
lsp-status-available = Հասանելի է
lsp-status-on-path = PATH-ում է
lsp-status-installing = Տեղադրվում է…
lsp-status-installed = Տեղադրված է
lsp-status-outdated = Թարմացում կա
lsp-status-running = Աշխատում է
lsp-status-failed = Ձախողվեց

spaces-title = Աշխատատարածքներ
spaces-new-placeholder = Նոր աշխատատարածքի անուն
spaces-empty = Աշխատատարածքներ չկան
spaces-default-name = Աշխատատարածք { $number }
spaces-tabs = { $count ->
    [one] 1 ներդիր
   *[other] { $count } ներդիր
}
spaces-delete = Ջնջել աշխատատարածքը

team-title = Թիմ
team-just-you = Այս աշխատատարածքում միայն դուք եք
team-agents = { $count ->
    [one] Դուք և 1 գործակալ
   *[other] Դուք և { $count } գործակալ
}
team-empty = Այստեղ դեռ ոչ ոք չկա
team-you = Դուք
team-agent = Գործակալ

services-title = Ֆոնային ծառայություններ
services-processes = { $count ->
    [one] 1 գործընթաց
   *[other] { $count } գործընթաց
}
services-kill-all = Կասեցնել բոլորը
services-not-running = Ծառայությունը չի աշխատում
services-start-with = Գործարկել՝
services-empty = Ակտիվ գործընթացներ չկան
services-filter = Զտել գործընթացները…
services-no-match = Համապատասխան գործընթացներ չկան
services-connected = Միացված է
services-disconnected = Անջատված է
services-attached = կցված է
services-kill = Կասեցնել
services-memory = Հիշողություն
services-size = Չափ
services-shell = Shell

error-title = Սխալ

history-search = Որոնել պատմության մեջ
history-clear-all = Մաքրել բոլորը
history-clear-confirm = Մաքրե՞լ ամբողջ պատմությունը։
history-clear-warning = Սա հնարավոր չի լինի հետարկել։
history-cancel = Չեղարկել
history-today = Այսօր
history-yesterday = Երեկ
history-days-ago = { $count } օր առաջ
history-day-offset = Օր -{ $count }

settings-title = Կարգավորումներ
settings-loading = Կարգավորումները բեռնվում են…
settings-stored = Պահվում է ~/.vmux/settings.ron-ում
settings-other = Այլ
settings-software-update = Ծրագրի թարմացում
settings-check-updates = Ստուգել թարմացումները
settings-check-updates-hint = Ավտոմատ ստուգվում է մեկնարկի պահին և ամեն ժամ, երբ ավտոթարմացումը միացված է։
settings-update-unavailable = Հասանելի չէ
settings-update-unavailable-hint = Թարմացուցիչը ներառված չէ այս build-ում։
settings-update-checking = Ստուգվում է…
settings-update-checking-hint = Թարմացումների ստուգում…
settings-update-check-again = Կրկին ստուգել
settings-update-current = Vmux-ը արդի է։
settings-update-downloading = Ներբեռնվում է…
settings-update-downloading-hint = Ներբեռնվում է Vmux { $version }…
settings-update-installing = Տեղադրվում է…
settings-update-installing-hint = Տեղադրվում է Vmux { $version }…
settings-update-ready = Թարմացումը պատրաստ է
settings-update-ready-hint = Vmux { $version }-ը պատրաստ է։ Կիրառելու համար վերագործարկեք։
settings-update-try-again = Կրկին փորձել
settings-update-failed = Չհաջողվեց ստուգել թարմացումները։
settings-item = Տարր
settings-item-number = Տարր { $number }
settings-press-key = Սեղմեք ստեղն…
settings-saved = Պահված է
settings-record-key = Սեղմեք՝ նոր ստեղների համակցություն գրանցելու համար

tray-open-window = Բացել պատուհանը
tray-close-window = Փակել պատուհանը
tray-pause-recording = Դադարեցնել ձայնագրումը
tray-resume-recording = Շարունակել ձայնագրումը
tray-finish-recording = Ավարտել ձայնագրումը
tray-quit = Փակել Vmux-ը

composer-attach-files = Կցել ֆայլեր (/upload)
composer-remove-attachment = Հեռացնել կցորդը

layout-back = Հետ
layout-forward = Առաջ
layout-reload = Վերբեռնել
layout-bookmark-page = Ավելացնել էջանիշերում
layout-remove-bookmark = Հեռացնել էջանիշը
layout-pin-page = Ամրացնել այս էջը
layout-unpin-page = Ապամրացնել այս էջը
layout-manage-extensions = Կառավարել ընդլայնումները
layout-new-stack = Նոր շերտ
layout-close-tab = Փակել ներդիրը
layout-bookmark = Էջանիշ
layout-pin = Ամրացնել
layout-new-tab = Նոր ներդիր
layout-team = Թիմ

command-switch-space = Փոխել աշխատատարածքը…
command-search-ask = Որոնել կամ հարցնել…
command-new-tab-placeholder = Որոնեք, մուտքագրեք URL կամ ընտրեք Terminal…
command-placeholder = Մուտքագրեք URL, որոնեք ներդիրներ կամ >՝ հրամանների համար…
command-composer-placeholder = Մուտքագրեք /՝ հրամանների կամ @՝ մեդիայի համար
command-send = Ուղարկել (Enter)
command-terminal = Terminal
command-open-terminal = Բացել Terminal-ում
command-stack = Շերտ
command-tabs = { $count ->
    [one] 1 ներդիր
   *[other] { $count } ներդիր
}
command-prompt = Prompt
command-new-tab = Նոր ներդիր
command-search = Որոնել
command-open-value = Բացել «{ $value }»
command-search-value = Որոնել «{ $value }»

schema-appearance = Արտաքին տեսք
schema-general = Ընդհանուր
schema-layout = Դասավորություն
schema-layout-detail = Պատուհան, վահանակներ, կողագոտի և ֆոկուսի շրջանակ։
schema-agent = Գործակալ
schema-agent-detail = Գործակալի վարքագիծ և գործիքների թույլտվություններ։
schema-shortcuts = Դյուրանցումներ
schema-shortcuts-detail = Միայն դիտման համար։ Կապակցումները փոխելու համար խմբագրեք settings.ron-ը։
schema-terminal = Terminal
schema-browser = Դիտարկիչ
schema-mode = Ռեժիմ
schema-mode-detail = Վեբ էջերի գունային սխեմա։ «Սարք» տարբերակը հետևում է համակարգին։
schema-device = Սարք
schema-light = Բաց
schema-dark = Մուգ
schema-language = Լեզու
schema-language-detail = Օգտագործեք համակարգայինը, en-US, ja կամ ցանկացած BCP 47 պիտակ՝ համապատասխան ~/.vmux/locales/<tag>.ftl կատալոգով։
schema-auto-update = Ավտոթարմացում
schema-auto-update-detail = Ստուգել և տեղադրել թարմացումները մեկնարկի պահին և ամեն ժամ։
schema-startup-url = Մեկնարկային URL
schema-startup-url-detail = Դատարկ լինելու դեպքում բացվում է հրամանների տողի prompt-ը։
schema-search-engine = Որոնման համակարգ
schema-search-engine-detail = Օգտագործվում է Սկզբից և հրամանների տողից վեբ որոնումների համար։
schema-window = Պատուհան
schema-pane = Վահանակ
schema-side-sheet = Կողային թերթ
schema-focus-ring = Ֆոկուսի շրջանակ
schema-run-placement = Թույլատրել գործարկման տեղադրման վերագրում
schema-run-placement-detail = Թույլ տալ գործակալներին ընտրել գործարկման վահանակի ռեժիմը, ուղղությունը և խարիսխը։
schema-leader = Առաջնորդ ստեղն
schema-leader-detail = Նախածանց ստեղն chord դյուրանցումների համար։
schema-chord-timeout = Chord-ի սպասման ժամանակը
schema-chord-timeout-detail = Միլիվայրկյաններ՝ մինչև chord նախածանցի ժամկետը լրանա։
schema-bindings = Կապակցումներ
schema-confirm-close = Հաստատել փակումը
schema-confirm-close-detail = Հարցնել՝ նախքան աշխատող գործընթացով Terminal-ը փակելը։
schema-default-theme = Լռելյայն թեմա
schema-default-theme-detail = Ակտիվ թեմայի անունը թեմաների ցանկից։

settings-empty = (դատարկ)
settings-none = (չկա)

schema-system = Համակարգ
schema-editor = Խմբագրիչ
schema-recording = Ձայնագրում
schema-radius = Շառավիղ
schema-padding = Ներդիր
schema-gap = Բացատ
schema-width = Լայնություն
schema-color = Գույն
schema-red = Կարմիր
schema-green = Կանաչ
schema-blue = Կապույտ
schema-follow-files = Հետևել ֆայլերին
schema-tidy-files = Կարգի բերել ֆայլերը
schema-tidy-files-max = Ֆայլերի կարգաբերման շեմ
schema-tidy-files-auto = Ինքնաբերաբար կարգի բերել ֆայլերը
schema-app-providers = Հավելվածների մատակարարներ
schema-provider = Մատակարար
schema-kind = Տեսակ
schema-models = Մոդելներ
schema-acp = ACP գործակալներ
schema-id = ID
schema-name = Անուն
schema-command = Հրաման
schema-arguments = Արգումենտներ
schema-environment = Միջավայր
schema-working-directory = Աշխատանքային պանակ
schema-shell = Վահանակ
schema-font-family = Տառատեսակ
schema-startup-directory = Մեկնարկային պանակ
schema-themes = Թեմաներ
schema-color-scheme = Գունային սխեմա
schema-font-size = Տառաչափ
schema-line-height = Տողերի բարձրություն
schema-cursor-style = Նշիչի ոճ
schema-cursor-blink = Թարթող նշիչ
schema-custom-themes = Անհատական թեմաներ
schema-foreground = Առաջնային գույն
schema-background = Ֆոն
schema-cursor = Նշիչ
schema-ansi-colors = ANSI գույներ
schema-keymap = Ստեղնաշարի քարտեզ
schema-explorer = Զննիչ
schema-visible = Տեսանելի
schema-language-servers = Լեզվական սերվերներ
schema-servers = Սերվերներ
schema-language-id = Լեզվի ID
schema-root-markers = Արմատային նշիչներ
schema-output-directory = Ելքային պանակ

menu-scene = Տեսարան
menu-layout = Դասավորություն
menu-terminal = Տերմինալ
menu-browser = Դիտարկիչ
menu-service = Ծառայություն
menu-bookmark = Էջանիշ
menu-edit = Խմբագրել

layout-knowledge = Գիտելիք
layout-open-knowledge = Բացել գիտելիքը
layout-open-welcome-knowledge = Բացել «Բարի գալուստ Գիտելիք»
layout-open-path = Բացել { $path }
layout-fold-knowledge = Ծալել գիտելիքը
layout-unfold-knowledge = Բացել գիտելիքը
layout-bookmarks = Էջանիշեր
layout-new-folder = Նոր պանակ
layout-add-to-bookmarks = Ավելացնել էջանիշերին
layout-move-to-bookmarks = Տեղափոխել էջանիշեր
layout-stack-number = Շեղջ { $number }
layout-fold-stack = Ծալել շեղջը
layout-unfold-stack = Բացել շեղջը
layout-close-stack = Փակել շեղջը
layout-bookmark-in = Էջանիշ՝ { $folder }-ում

common-cancel = Չեղարկել
common-delete = Ջնջել
common-save = Պահել
common-rename = Վերանվանել
common-expand = Բացել
common-collapse = Ծալել
common-loading = Բեռնվում է…
common-error = Սխալ
common-output = Արտածում
common-pending = Սպասման մեջ
common-current = ընթացիկ
common-stop = Կանգնեցնել
services-command = Vmux ծառայություն
services-uptime-seconds = { $seconds } վրկ
services-uptime-minutes = { $minutes } ր { $seconds } վրկ
services-uptime-hours = { $hours } ժ { $minutes } ր
services-uptime-days = { $days } օր { $hours } ժ

error-page-failed-load = Էջը չհաջողվեց բեռնել
error-page-not-found = Էջը չի գտնվել
error-unknown-host = Անհայտ Vmux հավելվածի հոսթ՝ { $host }

history-title = Պատմություն

command-new-app-chat = Նոր { $provider }/{ $model } զրույց (հավելված)
command-interactive-mode-user = Տեսարան > Ինտերակտիվ ռեժիմ > Օգտատեր
command-interactive-mode-player = Տեսարան > Ինտերակտիվ ռեժիմ > Խաղացող
command-minimize-window = Դասավորություն > Պատուհան > Ծալել
command-toggle-layout = Դասավորություն > Դասավորություն > Փոխարկել դասավորությունը
command-close-tab = Դասավորություն > Ներդիր > Փակել ներդիրը
command-new-task = Դասավորություն > Ներդիր > Նոր առաջադրանք…
command-next-tab = Դասավորություն > Ներդիր > Հաջորդ ներդիրը
command-prev-tab = Դասավորություն > Ներդիր > Նախորդ ներդիրը
command-rename-tab = Դասավորություն > Ներդիր > Վերանվանել ներդիրը
command-tab-select-1 = Դասավորություն > Ներդիր > Ընտրել ներդիր 1-ը
command-tab-select-2 = Դասավորություն > Ներդիր > Ընտրել ներդիր 2-ը
command-tab-select-3 = Դասավորություն > Ներդիր > Ընտրել ներդիր 3-ը
command-tab-select-4 = Դասավորություն > Ներդիր > Ընտրել ներդիր 4-ը
command-tab-select-5 = Դասավորություն > Ներդիր > Ընտրել ներդիր 5-ը
command-tab-select-6 = Դասավորություն > Ներդիր > Ընտրել ներդիր 6-ը
command-tab-select-7 = Դասավորություն > Ներդիր > Ընտրել ներդիր 7-ը
command-tab-select-8 = Դասավորություն > Ներդիր > Ընտրել ներդիր 8-ը
command-tab-select-last = Դասավորություն > Ներդիր > Ընտրել վերջին ներդիրը
command-close-pane = Դասավորություն > Վահանակ > Փակել վահանակը
command-select-pane-left = Դասավորություն > Վահանակ > Ընտրել ձախ վահանակը
command-select-pane-right = Դասավորություն > Վահանակ > Ընտրել աջ վահանակը
command-select-pane-up = Դասավորություն > Վահանակ > Ընտրել վերևի վահանակը
command-select-pane-down = Դասավորություն > Վահանակ > Ընտրել ներքևի վահանակը
command-swap-pane-prev = Դասավորություն > Վահանակ > Փոխատեղել նախորդ վահանակի հետ
command-swap-pane-next = Դասավորություն > Վահանակ > Փոխատեղել հաջորդ վահանակի հետ
command-equalize-pane-size = Դասավորություն > Վահանակ > Հավասարեցնել վահանակների չափը
command-resize-pane-left = Դասավորություն > Վահանակ > Չափափոխել վահանակը ձախ
command-resize-pane-right = Դասավորություն > Վահանակ > Չափափոխել վահանակը աջ
command-resize-pane-up = Դասավորություն > Վահանակ > Չափափոխել վահանակը վեր
command-resize-pane-down = Դասավորություն > Վահանակ > Չափափոխել վահանակը վար
command-stack-close = Դասավորություն > Շերտ > Փակել շերտը
command-stack-next = Դասավորություն > Շերտ > Հաջորդ շերտը
command-stack-previous = Դասավորություն > Շերտ > Նախորդ շերտը
command-stack-reopen = Դասավորություն > Շերտ > Վերաբացել փակված էջը
command-stack-swap-prev = Դասավորություն > Շերտ > Տեղափոխել շերտը ձախ
command-stack-swap-next = Դասավորություն > Շերտ > Տեղափոխել շերտը աջ
command-space-open = Դասավորություն > Տարածք > Տարածքներ
command-terminal-close = Տերմինալ > Փակել տերմինալը
command-terminal-next = Տերմինալ > Հաջորդ տերմինալը
command-terminal-prev = Տերմինալ > Նախորդ տերմինալը
command-terminal-clear = Տերմինալ > Մաքրել տերմինալը
command-browser-prev-page = Դիտարկիչ > Նավիգացիա > Հետ
command-browser-next-page = Դիտարկիչ > Նավիգացիա > Առաջ
command-browser-reload = Դիտարկիչ > Նավիգացիա > Վերբեռնել
command-browser-hard-reload = Դիտարկիչ > Նավիգացիա > Լրիվ վերբեռնել
command-open-in-place = Դիտարկիչ > Բացել > Բացել այստեղ
command-open-in-new-stack = Դիտարկիչ > Բացել > Բացել նոր շերտում
command-open-in-pane-top = Դիտարկիչ > Բացել > Բացել վերևի վահանակում
command-open-in-pane-right = Դիտարկիչ > Բացել > Բացել աջ վահանակում
command-open-in-pane-bottom = Դիտարկիչ > Բացել > Բացել ներքևի վահանակում
command-open-in-pane-left = Դիտարկիչ > Բացել > Բացել ձախ վահանակում
command-open-in-new-tab = Դիտարկիչ > Բացել > Բացել նոր ներդիրում
command-open-in-new-space = Դիտարկիչ > Բացել > Բացել նոր տարածքում
command-browser-zoom-in = Դիտարկիչ > Տեսք > Մեծացնել
command-browser-zoom-out = Դիտարկիչ > Տեսք > Փոքրացնել
command-browser-zoom-reset = Դիտարկիչ > Տեսք > Իրական չափը
command-browser-dev-tools = Դիտարկիչ > Տեսք > Մշակողի գործիքներ
command-browser-open-command-bar = Դիտարկիչ > Վահանակ > Հրամանների վահանակ
command-browser-open-page-in-command-bar = Դիտարկիչ > Վահանակ > Խմբագրել էջը
command-browser-open-path-bar = Դիտարկիչ > Վահանակ > Ուղու նավիգատոր
command-browser-open-commands = Դիտարկիչ > Վահանակ > Հրամաններ
command-browser-open-history = Դիտարկիչ > Վահանակ > Պատմություն
command-service-open = Ծառայություն > Բացել ծառայությունների մոնիտորը
command-bookmark-toggle-active = Էջանիշ > Էջը ավելացնել էջանիշների մեջ
command-bookmark-pin-active = Էջանիշ > Ամրացնել էջը

layout-tab = Ներդիր
layout-no-stacks = Շերտեր չկան
layout-loading = Բեռնվում է…
layout-no-markdown-files = Markdown ֆայլեր չկան
layout-empty-folder = Դատարկ պանակ
layout-worktree = աշխատանքային ծառ
layout-folder-name = Պանակի անուն
layout-no-pins-bookmarks = Ամրացումներ կամ էջանիշներ չկան
layout-move-to = Տեղափոխել { $folder }
layout-bookmark-current-page = Ավելացնել ընթացիկ էջը էջանիշների մեջ
layout-rename-folder = Վերանվանել պանակը
layout-remove-folder = Հեռացնել պանակը
layout-update-downloading = Թարմացումը ներբեռնվում է
layout-update-installing = Թարմացումը տեղադրվում է…
layout-update-ready = Նոր տարբերակ կա
layout-restart-update = Վերագործարկել՝ թարմացնելու համար

agent-preparing = Գործակալը պատրաստվում է…
agent-send-all-queued = Ուղարկել հերթագրված բոլոր հուշումները հիմա (Esc)
agent-send = Ուղարկել (Enter)
agent-ready = Պատրաստ եմ, երբ դուք պատրաստ լինեք։
agent-loading-older = Բեռնվում են հին հաղորդագրությունները…
agent-load-older = Բեռնել հին հաղորդագրությունները
agent-continued-from = Շարունակված է { $source }-ից
agent-older-context-omitted = հին համատեքստը բաց է թողնված
agent-interrupted = ընդհատված
agent-allow-tool = Թույլատրե՞լ { $tool }-ը
agent-deny = Մերժել
agent-allow-always = Միշտ թույլատրել
agent-allow = Թույլատրել
agent-loading-sessions = Նիստերը բեռնվում են…
agent-no-resumable-sessions = Շարունակելի նիստեր չեն գտնվել
agent-no-matching-sessions = Համապատասխան նիստեր չկան
agent-no-matching-models = Համապատասխան մոդելներ չկան
agent-choice-help = ↑/↓ կամ Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Ընտրել ռեպոզիտորիայի պանակը
agent-choose-repository-detail = Ընտրեք տեղական Git ռեպոզիտորիան, որը պետք է օգտագործի գործակալը։
agent-choosing = Ընտրվում է…
agent-choose-folder = Ընտրել պանակ
agent-queued = հերթագրված
agent-attached = Կցված է՝
agent-cancel-queued = Չեղարկել հերթագրված հուշումը
agent-resume-queued = Շարունակել հերթագրված հուշումները
agent-clear-queue = Մաքրել հերթը
agent-send-all-now = ուղարկել բոլորը հիմա
agent-choose-option = Ընտրեք տարբերակ վերևում
agent-loading-media = Մեդիան բեռնվում է…
agent-no-matching-media = Համապատասխան մեդիա չկա
agent-prompt-context = Հուշման համատեքստ
agent-details = Մանրամասներ
agent-path = Ուղի
agent-tool = Գործիք
agent-server = Սերվեր
agent-bytes = { $count } բայթ
agent-worked-for = Աշխատեց { $duration }
agent-worked-for-steps = { $count ->
    [one] Աշխատեց { $duration } · 1 քայլ
   *[other] Աշխատեց { $duration } · { $count } քայլ
}
agent-tool-guardian-review = Guardian ստուգում
agent-tool-read-files = Կարդաց ֆայլերը
agent-tool-viewed-image = Դիտեց պատկերը
agent-tool-used-browser = Օգտագործեց դիտարկիչը
agent-tool-searched-files = Որոնեց ֆայլերում
agent-tool-ran-commands = Գործարկեց հրամաններ
agent-thinking = Մտածում է
agent-subagent = Ենթագործակալ
agent-prompt = Հուշում
agent-thread = Շղթա
agent-parent = Ծնող
agent-children = Զավակներ
agent-call = Կանչ
agent-raw-event = Չմշակված իրադարձություն
agent-plan = Պլան
agent-tasks = { $count ->
    [one] 1 առաջադրանք
   *[other] { $count } առաջադրանք
}
agent-edited = Խմբագրված է
agent-reconnecting = Վերամիացում { $attempt }/{ $total }
agent-status-running = Աշխատում է
agent-status-done = Ավարտված է
agent-status-failed = Չհաջողվեց
agent-status-pending = Սպասման մեջ
agent-slash-attach-files = Կցել ֆայլեր
agent-slash-resume-session = Շարունակել նախորդ նիստը
agent-slash-select-model = Ընտրել մոդել
agent-slash-continue-cli = Շարունակել այս նիստը CLI-ում
agent-session-just-now = հենց հիմա
agent-session-minutes-ago = { $count } ր առաջ
agent-session-hours-ago = { $count } ժ առաջ
agent-session-days-ago = { $count } օր առաջ
agent-working-working = Աշխատում է
agent-working-thinking = Մտածում է
agent-working-pondering = Խորհում է
agent-working-noodling = Մշակում է մտքերը
agent-working-percolating = Հասունացնում է
agent-working-conjuring = Կերտում է
agent-working-cooking = Եփում է
agent-working-brewing = Եփում է մտքերը
agent-working-musing = Մտորում է
agent-working-ruminating = Խորությամբ մտածում է
agent-working-scheming = Ծրագիր է կազմում
agent-working-synthesizing = Սինթեզում է
agent-working-tinkering = Փորձարկում է
agent-working-churning = Մշակում է
agent-working-vibing = Տրամադրվում է
agent-working-simmering = Հանդարտ եփում է
agent-working-crafting = Պատրաստում է
agent-working-divining = Կռահում է
agent-working-mulling = Ծանրութեթև է անում
agent-working-spelunking = Խորքում փնտրում է

editor-toggle-explorer = Փոխարկել Explorer-ը (Cmd+B)
editor-unsaved = չպահված
editor-rendered-markdown = Արտապատկերված Markdown՝ կենդանի խմբագրմամբ
editor-note = Նշում
editor-source-editor = Աղբյուրի խմբագրիչ
editor-editor = Խմբագրիչ
editor-git-diff = Git տարբերություն
editor-diff = Տարբերություն
editor-tidy = Կարգի բերել
editor-always = Միշտ
editor-unchanged-previews = { $count ->
    [one] ✦ 1 անփոփոխ նախադիտում
   *[other] ✦ { $count } անփոփոխ նախադիտում
}
editor-open-externally = Բացել արտաքին հավելվածով
editor-changed-line = Փոփոխված տող
editor-go-to-definition = Անցնել սահմանմանը
editor-find-references = Գտնել հղումները
editor-references = { $count ->
    [one] 1 հղում
   *[other] { $count } հղում
}
editor-lsp-starting = { $server } մեկնարկում է…
editor-lsp-not-installed = { $server } — տեղադրված չէ
editor-explorer = Explorer
editor-open-editors = Բաց խմբագրիչներ
editor-outline = Կառուցվածք
editor-new-file = Նոր ֆայլ
editor-new-folder = Նոր պանակ
editor-delete-confirm = Ջնջե՞լ «{ $name }»-ը։ Սա հնարավոր չէ հետարկել։
editor-created-folder = Ստեղծվեց { $name } պանակը
editor-created-file = Ստեղծվեց { $name } ֆայլը
editor-renamed-to = Վերանվանվեց՝ { $name }
editor-deleted = Ջնջվեց { $name }
editor-failed-decode-image = Պատկերը չհաջողվեց վերծանել
editor-preview-large-image = պատկեր (նախադիտման համար չափազանց մեծ է)
editor-preview-binary = բինար
editor-preview-file = ֆայլ

git-status-clean = մաքուր
git-status-modified = փոփոխված
git-status-staged = պատրաստված
git-status-staged-modified = պատրաստված*
git-status-untracked = չհետևվող
git-status-deleted = ջնջված
git-status-conflict = կոնֆլիկտ
git-accept-all = ✓ ընդունել բոլորը
git-unstage = Հանել պատրաստվածից
git-confirm-deny-all = Հաստատել բոլորի մերժումը
git-deny-all = ✗ մերժել բոլորը
git-commit-message = commit հաղորդագրություն
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Տարբերությունը բեռնվում է…
git-no-changes = Ցուցադրելու փոփոխություններ չկան
git-accept = ✓ ընդունել
git-deny = ✗ մերժել
git-show-unchanged-lines = Ցույց տալ { $count } անփոփոխ տող

terminal-loading = Բեռնվում է…
terminal-runs-when-ready = կգործարկվի, երբ պատրաստ լինի · Ctrl+C՝ մաքրել · Esc՝ բաց թողնել
terminal-booting = մեկնարկում է
terminal-type-command = մուտքագրեք հրաման · կգործարկվի, երբ պատրաստ լինի · Esc՝ բաց թողնել

setup-tagline-claude = Anthropic-ի կոդավորման գործակալը՝ Vmux-ում
setup-tagline-codex = OpenAI-ի կոդավորման գործակալը՝ Vmux-ում
setup-tagline-vibe = Mistral-ի կոդավորման գործակալը՝ Vmux-ում
setup-install-title = Տեղադրել { $name } CLI-ն
setup-homebrew-required = { $command }-ը տեղադրելու համար Homebrew է պահանջվում, բայց այն դեռ կարգավորված չէ։ Vmux-ը նախ կտեղադրի Homebrew-ը, ապա { $name }-ը։
setup-terminal-instructions = Տերմինալում սեղմեք Return՝ սկսելու համար, ապա պահանջվելու դեպքում մուտքագրեք ձեր Mac-ի գաղտնաբառը։
setup-command-missing = Vmux-ը բացել է այս էջը, որովհետև տեղական { $command } հրամանը դեռ տեղադրված չէ։ Գործարկեք ստորևի հրամանը՝ այն ստանալու համար։
setup-install-failed = Տեղադրումը չավարտվեց։ Մանրամասների համար ստուգեք տերմինալը, ապա նորից փորձեք։
setup-installing = Տեղադրվում է…
setup-install-homebrew = Տեղադրել Homebrew + { $name }
setup-run-install = Գործարկել տեղադրման հրամանը
setup-auto-reload = Vmux-ը այն գործարկում է տերմինալում և վերբեռնում, երբ { $command }-ը պատրաստ է։

debug-title = Վրիպազերծում
debug-auto-update = Ավտոթարմացում
debug-simulate-update = Մոդելավորել հասանելի թարմացում
debug-simulate-download = Մոդելավորել ներբեռնում
debug-clear-update = Մաքրել թարմացումը
debug-trigger-restart = Գործարկել վերագործարկում

command-manage-spaces = Կառավարել տարածքները…
command-pane-stack-location = վահանակ { $pane } / շերտ { $stack }
command-space-pane-stack-location = { $space } / վահանակ { $pane } / շերտ { $stack }
command-terminal-path = Տերմինալ ({ $path })
command-group-interactive-mode = Ինտերակտիվ ռեժիմ
command-group-window = Պատուհան
command-group-tab = Ներդիր
command-group-pane = Վահանակ
command-group-stack = Շերտ
command-group-space = Տարածք
command-group-navigation = Նավարկում
command-group-open = Բացել
command-group-view = Դիտել
command-group-bar = Վահանակագոտի

menu-close-vmux = Փակել Vmux-ը

agents-terminal-coding-agent = Տերմինալային կոդավորման գործակալ
