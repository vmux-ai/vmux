common-open = Բաց
common-close = Փակել
common-install = Տեղադրեք
common-uninstall = Տեղահանել
common-update = Թարմացնել
common-retry = Կրկին փորձեք
common-refresh = Թարմացնել
common-remove = Հեռացնել
common-enable = Միացնել
common-disable = Անջատել
common-new = Նոր
common-active = ակտիվ
common-running = վազում
common-done = կատարված է
common-failed = Չհաջողվեց
common-installed = Տեղադրված է
common-items = { $count ->
    [one] { $count } տարր
   *[other] { $count } տարրեր
}
start-title = Սկսել
start-tagline = Մեկ հուշում. Ամեն ինչ, արված է:

agents-title = Գործակալներ
agents-search = Որոնեք ACP և CLI գործակալները…
agents-empty = Համընկնող գործակալներ չկան
agents-empty-detail = Փորձեք անունը, գործարկման ժամանակը կամ ACP/CLI:
agents-install-failed = Չհաջողվեց տեղադրել
agents-updating = Թարմացվում է…
agents-retrying = Կրկին փորձ…
agents-preparing = Պատրաստվում է…

extensions-title = Ընդլայնումներ
extensions-search = Որոնումը տեղադրված է կամ Chrome Web Store…
extensions-relaunch = Վերագործարկեք՝ կիրառելու համար
extensions-empty = Տեղադրված ընդլայնումներ չկան
extensions-no-match = Համապատասխան ընդլայնումներ չկան
extensions-empty-detail = Որոնեք Chrome Web Store վերևում և սեղմեք Return:
extensions-no-match-detail = Փորձեք մեկ այլ անուն կամ ընդլայնման ID:
extensions-on = Միացված է
extensions-off = Անջատված
extensions-enable-confirm = Միացնե՞լ { $name }-ը:
extensions-enable-permissions = Միացնել { $name }-ը և թույլ տալ՝

lsp-title = Լեզվի սերվերներ
lsp-search = Որոնեք լեզուների սերվերներ, լինտերներ, ձևաչափիչներ…
lsp-loading = Կատալոգի բեռնում…
lsp-empty = Համապատասխան լեզվի սերվերներ չկան
lsp-empty-detail = Փորձեք մեկ այլ լեզու, linter կամ formatter:
lsp-needs = կարիք ունի { $tool }
lsp-status-available = Հասանելի է
lsp-status-on-path = PATH-ին
lsp-status-installing = Տեղադրվում է…
lsp-status-installed = Տեղադրված է
lsp-status-outdated = Թարմացումը հասանելի է
lsp-status-running = Վազում
lsp-status-failed = Չհաջողվեց

spaces-title = Տարածքներ
spaces-new-placeholder = Նոր տիեզերական անուն
spaces-empty = Բացատներ չկան
spaces-default-name = Տարածություն { $number }
spaces-tabs = { $count ->
    [one] 1 ներդիր
   *[other] { $count } ներդիր
}
spaces-delete = Ջնջել տարածությունը

team-title = Թիմ
team-just-you = Միայն դու այս տարածքում
team-agents = { $count ->
    [one] Դուք և 1 գործակալ
   *[other] Դուք և { $count } գործակալները
}
team-empty = Այստեղ դեռ ոչ ոք չկա
team-you = Դուք
team-agent = Գործակալ

services-title = Ֆոնային ծառայություններ
services-processes = { $count ->
    [one] 1 գործընթաց
   *[other] { $count } գործընթացներ
}
services-kill-all = Սպանել բոլորին
services-not-running = Ծառայությունը չի աշխատում
services-start-with = Սկսեք հետևյալով.
services-empty = Ակտիվ գործընթացներ չկան
services-filter = Զտել գործընթացները…
services-no-match = Համապատասխան գործընթացներ չկան
services-connected = Միացված է
services-disconnected = Անջատված է
services-attached = կից
services-kill = Սպանել
services-memory = Հիշողություն
services-size = Չափը
services-shell = Շելլ

error-title = Սխալ

history-search = Որոնման պատմություն
history-clear-all = Մաքրել բոլորը
history-clear-confirm = Մաքրե՞լ ամբողջ պատմությունը:
history-clear-warning = Սա հնարավոր չէ հետարկել:
history-cancel = Չեղարկել
history-today = Այսօր
history-yesterday = Երեկ
history-days-ago = { $count } օր առաջ
history-day-offset = Օր -{ $count }

settings-title = Կարգավորումներ
settings-loading = Կարգավորումների բեռնում…
settings-stored = Պահպանված է ~/.vmux/settings.ron-ում
settings-other = Այլ
settings-software-update = Ծրագրային ապահովման թարմացում
settings-check-updates = Ստուգեք թարմացումների համար
settings-check-updates-hint = Ստուգում է ավտոմատ կերպով գործարկման ժամանակ և ամեն ժամ, երբ ավտոմատ թարմացումը միացված է:
settings-update-unavailable = Անհասանելի է
settings-update-unavailable-hint = Updater-ը ներառված չէ այս նախագծում:
settings-update-checking = Ստուգվում է…
settings-update-checking-hint = Թարմացումների ստուգում…
settings-update-check-again = Կրկին ստուգեք
settings-update-current = Vmux-ը արդիական է:
settings-update-downloading = Ներբեռնվում է…
settings-update-downloading-hint = Ներբեռնվում է Vmux { $version }…
settings-update-installing = Տեղադրվում է…
settings-update-installing-hint = Տեղադրվում է Vmux { $version }…
settings-update-ready = Թարմացնել պատրաստ է
settings-update-ready-hint = Vmux { $version } պատրաստ է: Վերագործարկեք այն կիրառելու համար:
settings-update-try-again = Կրկին փորձեք
settings-update-failed = Հնարավոր չէ ստուգել թարմացումների առկայությունը:
settings-item = Նյութ
settings-item-number = Կետ { $number }
settings-press-key = Սեղմեք ստեղն…
settings-saved = Պահպանված է
settings-record-key = Սեղմեք՝ նոր ստեղնաշարի համադրություն ձայնագրելու համար

tray-open-window = Բացեք Պատուհանը
tray-close-window = Փակել պատուհանը
tray-pause-recording = Դադարեցնել ձայնագրությունը
tray-resume-recording = Վերսկսել ձայնագրությունը
tray-finish-recording = Ավարտել ձայնագրությունը
tray-quit = Դուրս գալ Vmux

composer-attach-files = Կցել ֆայլեր (/upload)
composer-remove-attachment = Հեռացնել հավելվածը

layout-back = Ետ
layout-forward = Առաջ
layout-reload = Վերբեռնել
layout-bookmark-page = Էջանշեք այս էջը
layout-remove-bookmark = Հեռացնել էջանիշը
layout-pin-page = Ամրացրեք այս էջը
layout-unpin-page = Ապամրացնել այս էջը
layout-manage-extensions = Կառավարեք ընդարձակումները
layout-new-stack = Նոր կույտ
layout-close-tab = Փակել ներդիրը
layout-bookmark = Էջանիշ
layout-pin = Փին
layout-new-tab = Նոր ներդիր
layout-team = Թիմ

command-switch-space = Փոխել տարածությունը…
command-search-ask = Որոնել կամ հարցնել…
command-new-tab-placeholder = Որոնեք կամ մուտքագրեք URL կամ ընտրեք Տերմինալ…
command-placeholder = Մուտքագրեք URL, որոնման ներդիրներ կամ > հրամանների համար…
command-composer-placeholder = Մուտքագրեք / հրամանների համար կամ @ լրատվամիջոցների համար
command-send = Ուղարկել (Enter)
command-terminal = Տերմինալ
command-open-terminal = Բացեք տերմինալում
command-stack = Դարձ
command-tabs = { $count ->
    [one] 1 ներդիր
   *[other] { $count } ներդիր
}
command-prompt = Հուշել
command-new-tab = Նոր ներդիր
command-search = Որոնում
command-open-value = Բացեք «{ $value }»
command-search-value = Որոնել «{ $value }»

schema-appearance = Արտաքին տեսք
schema-general = Գեներալ
schema-layout = Դասավորություն
schema-layout-detail = Պատուհան, ապակիներ, կողագոտի և կենտրոնացման օղակ:
schema-agent = Գործակալ
schema-agent-detail = Գործակալի վարքագիծը և գործիքի թույլտվությունները:
schema-shortcuts = Դյուրանցումներ
schema-shortcuts-detail = Միայն կարդալու դիտում: Անմիջապես խմբագրեք settings.ron կապերը փոխելու համար:
schema-terminal = Տերմինալ
schema-browser = Բրաուզեր
schema-mode = Ռեժիմ
schema-mode-detail = Վեբ էջերի գունային սխեման: Սարքը հետևում է ձեր համակարգին:
schema-device = Սարք
schema-light = Լույս
schema-dark = Մութ
schema-language = Լեզու
schema-language-detail = Օգտագործեք համակարգը, en-US, ja կամ ցանկացած BCP 47 թեգ՝ համապատասխան ~/.vmux/locales/<tag>.ftl կատալոգով:
schema-auto-update = Ավտոմատ թարմացում
schema-auto-update-detail = Ստուգեք և տեղադրեք թարմացումներ գործարկման ժամանակ և ամեն ժամ:
schema-startup-url = Գործարկում URL
schema-startup-url-detail = Empty-ը բացում է հրամանի տողի հուշումը:
schema-search-engine = Որոնման համակարգ
schema-search-engine-detail = Օգտագործվում է Start-ից և հրամանի տողից վեբ որոնումների համար:
schema-window = Պատուհան
schema-pane = Պանել
schema-side-sheet = Կողքի թերթիկ
schema-focus-ring = Ֆոկուսի օղակ
schema-run-placement = Թույլատրել գործարկման տեղադրման անտեսումը
schema-run-placement-detail = Թույլ տվեք գործակալներին ընտրել գործարկման վահանակի ռեժիմը, ուղղությունը և խարիսխը:
schema-leader = Առաջնորդ
schema-leader-detail = Նախածանցի ստեղն ակորդի դյուրանցումների համար:
schema-chord-timeout = Աքորդի ժամանակի ընդհատում
schema-chord-timeout-detail = Միլվայրկյան առաջ ակորդի նախածանցի ժամկետի ավարտից առաջ:
schema-bindings = Ամրացումներ
schema-confirm-close = Հաստատեք փակումը
schema-confirm-close-detail = Գործող գործընթացով տերմինալը փակելուց առաջ հուշեք:
schema-default-theme = Կանխադրված թեմա
schema-default-theme-detail = Ակտիվ թեմայի անվանումը թեմաների ցանկից:
