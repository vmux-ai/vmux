common-open = తెరవు
common-close = మూసివేయి
common-install = ఇన్‌స్టాల్ చేయి
common-uninstall = అన్‌ఇన్‌స్టాల్ చేయి
common-update = నవీకరించు
common-retry = మళ్ళీ ప్రయత్నించు
common-refresh = రిఫ్రెష్ చేయి
common-remove = తొలగించు
common-enable = ప్రారంభించు
common-disable = నిలిపివేయి
common-new = కొత్త
common-active = చురుకుగా
common-running = అమలవుతోంది
common-done = పూర్తయింది
common-failed = విఫలమైంది
common-installed = ఇన్‌స్టాల్ అయింది
common-items = { $count ->
    [one] { $count } అంశం
   *[other] { $count } అంశాలు
}
start-title = ప్రారంభం
start-tagline = ఒక్క సూచన. ఏదైనా, పూర్తయింది.

agents-title = ఏజెంట్లు
agents-search = ACP మరియు CLI ఏజెంట్లను శోధించండి…
agents-empty = సరిపోలే ఏజెంట్లు లేవు
agents-empty-detail = పేరు, రన్‌టైమ్, లేదా ACP/CLI ప్రయత్నించండి.
agents-install-failed = ఇన్‌స్టాల్ విఫలమైంది
agents-updating = నవీకరిస్తోంది…
agents-retrying = మళ్ళీ ప్రయత్నిస్తోంది…
agents-preparing = సిద్ధమవుతోంది…

extensions-title = పొడిగింపులు
extensions-search = ఇన్‌స్టాల్ అయినవి లేదా Chrome Web Store శోధించండి…
extensions-relaunch = వర్తించడానికి మళ్ళీ ప్రారంభించండి
extensions-empty = పొడిగింపులు ఇన్‌స్టాల్ కాలేదు
extensions-no-match = సరిపోలే పొడిగింపులు లేవు
extensions-empty-detail = పై Chrome Web Store శోధించి Return నొక్కండి.
extensions-no-match-detail = మరో పేరు లేదా పొడిగింపు ID ప్రయత్నించండి.
extensions-on = ఆన్
extensions-off = ఆఫ్
extensions-enable-confirm = { $name } ప్రారంభించాలా?
extensions-enable-permissions = { $name } ప్రారంభించి, అనుమతించండి:

lsp-title = భాషా సర్వర్లు
lsp-search = భాషా సర్వర్లు, లింటర్లు, ఫార్మాటర్లు శోధించండి…
lsp-loading = కేటలాగ్ లోడవుతోంది…
lsp-empty = సరిపోలే భాషా సర్వర్లు లేవు
lsp-empty-detail = మరో భాష, లింటర్, లేదా ఫార్మాటర్ ప్రయత్నించండి.
lsp-needs = { $tool } అవసరం
lsp-status-available = అందుబాటులో ఉంది
lsp-status-on-path = PATH లో ఉంది
lsp-status-installing = ఇన్‌స్టాల్ అవుతోంది…
lsp-status-installed = ఇన్‌స్టాల్ అయింది
lsp-status-outdated = నవీకరణ అందుబాటులో ఉంది
lsp-status-running = అమలవుతోంది
lsp-status-failed = విఫలమైంది

spaces-title = స్పేసులు
spaces-new-placeholder = కొత్త స్పేస్ పేరు
spaces-empty = స్పేసులు లేవు
spaces-default-name = స్పేస్ { $number }
spaces-tabs = { $count ->
    [one] 1 ట్యాబ్
   *[other] { $count } ట్యాబులు
}
spaces-delete = స్పేస్ తొలగించు

team-title = బృందం
team-just-you = ఈ స్పేసులో మీరు మాత్రమే
team-agents = { $count ->
    [one] మీరు మరియు 1 ఏజెంట్
   *[other] మీరు మరియు { $count } ఏజెంట్లు
}
team-empty = ఇంకా ఎవరూ లేరు
team-you = మీరు
team-agent = ఏజెంట్

services-title = నేపథ్య సేవలు
services-processes = { $count ->
    [one] 1 ప్రక్రియ
   *[other] { $count } ప్రక్రియలు
}
services-kill-all = అన్నీ ముగించు
services-not-running = సేవ నడవడం లేదు
services-start-with = తో ప్రారంభించు:
services-empty = చురుకైన ప్రక్రియలు లేవు
services-filter = ప్రక్రియలను ఫిల్టర్ చేయి…
services-no-match = సరిపోలే ప్రక్రియలు లేవు
services-connected = అనుసంధానించబడింది
services-disconnected = అనుసంధానం తెగింది
services-attached = జోడించబడింది
services-kill = ముగించు
services-memory = మెమరీ
services-size = పరిమాణం
services-shell = షెల్

error-title = లోపం

history-search = చరిత్రను శోధించు
history-clear-all = అన్నీ తొలగించు
history-clear-confirm = మొత్తం చరిత్రను తొలగించాలా?
history-clear-warning = దీన్ని రద్దు చేయడం సాధ్యం కాదు.
history-cancel = రద్దు చేయి
history-today = నేడు
history-yesterday = నిన్న
history-days-ago = { $count } రోజుల క్రితం
history-day-offset = రోజు -{ $count }

settings-title = సెట్టింగులు
settings-loading = సెట్టింగులు లోడవుతున్నాయి…
settings-stored = ~/.vmux/settings.ron లో నిల్వ చేయబడింది
settings-other = ఇతర
settings-software-update = సాఫ్ట్‌వేర్ నవీకరణ
settings-check-updates = నవీకరణలు తనిఖీ చేయి
settings-check-updates-hint = ప్రారంభంలో స్వయంచాలకంగా తనిఖీ చేస్తుంది; Auto-update ప్రారంభించినప్పుడు ప్రతి గంటకూ.
settings-update-unavailable = అందుబాటులో లేదు
settings-update-unavailable-hint = ఈ బిల్డ్‌లో అప్‌డేటర్ చేర్చబడలేదు.
settings-update-checking = తనిఖీ చేస్తోంది…
settings-update-checking-hint = నవీకరణలు తనిఖీ చేస్తోంది…
settings-update-check-again = మళ్ళీ తనిఖీ చేయి
settings-update-current = Vmux నవీకరించబడింది.
settings-update-downloading = డౌన్‌లోడ్ అవుతోంది…
settings-update-downloading-hint = Vmux { $version } డౌన్‌లోడ్ అవుతోంది…
settings-update-installing = ఇన్‌స్టాల్ అవుతోంది…
settings-update-installing-hint = Vmux { $version } ఇన్‌స్టాల్ అవుతోంది…
settings-update-ready = నవీకరణ సిద్ధంగా ఉంది
settings-update-ready-hint = Vmux { $version } సిద్ధంగా ఉంది. వర్తించడానికి పునఃప్రారంభించండి.
settings-update-try-again = మళ్ళీ ప్రయత్నించు
settings-update-failed = నవీకరణలు తనిఖీ చేయడం సాధ్యం కాలేదు.
settings-item = అంశం
settings-item-number = అంశం { $number }
settings-press-key = ఒక కీ నొక్కండి…
settings-saved = సేవ్ అయింది
settings-record-key = కొత్త కీ కాంబో రికార్డ్ చేయడానికి క్లిక్ చేయండి

tray-open-window = విండో తెరవండి
tray-close-window = విండో మూసివేయండి
tray-pause-recording = రికార్డింగ్ నిలిపివేయి
tray-resume-recording = రికార్డింగ్ కొనసాగించు
tray-finish-recording = రికార్డింగ్ పూర్తి చేయి
tray-quit = Vmux నుండి నిష్క్రమించు

composer-attach-files = ఫైళ్లు జోడించు (/upload)
composer-remove-attachment = జోడింపు తొలగించు

layout-back = వెనుకకు
layout-forward = ముందుకు
layout-reload = రీలోడ్ చేయి
layout-bookmark-page = ఈ పేజీని బుక్‌మార్క్ చేయి
layout-remove-bookmark = బుక్‌మార్క్ తొలగించు
layout-pin-page = ఈ పేజీని పిన్ చేయి
layout-unpin-page = ఈ పేజీని అన్‌పిన్ చేయి
layout-manage-extensions = పొడిగింపులను నిర్వహించు
layout-new-stack = కొత్త స్టాక్
layout-close-tab = ట్యాబ్ మూసివేయి
layout-bookmark = బుక్‌మార్క్
layout-pin = పిన్
layout-new-tab = కొత్త ట్యాబ్
layout-team = బృందం

command-switch-space = స్పేస్ మారు…
command-search-ask = శోధించండి లేదా అడగండి…
command-new-tab-placeholder = శోధించండి లేదా URL టైప్ చేయండి, లేదా Terminal ఎంచుకోండి…
command-placeholder = URL టైప్ చేయండి, ట్యాబులు శోధించండి, లేదా ఆదేశాలకు >…
command-composer-placeholder = ఆదేశాలకు / లేదా మీడియాకు @ టైప్ చేయండి
command-send = పంపు (Enter)
command-terminal = టెర్మినల్
command-open-terminal = టెర్మినల్‌లో తెరవు
command-stack = స్టాక్
command-tabs = { $count ->
    [one] 1 ట్యాబ్
   *[other] { $count } ట్యాబులు
}
command-prompt = ప్రాంప్ట్
command-new-tab = కొత్త ట్యాబ్
command-search = శోధన
command-open-value = "{ $value }" తెరవు
command-search-value = "{ $value }" శోధించు

schema-appearance = స్వరూపం
schema-general = సాధారణ
schema-layout = లేఔట్
schema-layout-detail = విండో, పేన్లు, సైడ్‌బార్, మరియు ఫోకస్ రింగ్.
schema-agent = ఏజెంట్
schema-agent-detail = ఏజెంట్ ప్రవర్తన మరియు సాధన అనుమతులు.
schema-shortcuts = సత్వరమార్గాలు
schema-shortcuts-detail = చదవడానికి మాత్రమే వీక్షణ. బైండింగులు మార్చడానికి settings.ron నేరుగా సవరించండి.
schema-terminal = టెర్మినల్
schema-browser = బ్రౌజర్
schema-mode = మోడ్
schema-mode-detail = వెబ్ పేజీల కలర్ స్కీమ్. పరికరం మీ సిస్టమ్‌ను అనుసరిస్తుంది.
schema-device = పరికరం
schema-light = లైట్
schema-dark = డార్క్
schema-language = భాష
schema-language-detail = సిస్టమ్, en-US, ja, లేదా సరిపోలే ~/.vmux/locales/<tag>.ftl కేటలాగ్‌తో ఏదైనా BCP 47 ట్యాగ్ ఉపయోగించండి.
schema-auto-update = స్వయంచాలక నవీకరణ
schema-auto-update-detail = ప్రారంభంలో మరియు ప్రతి గంటకూ నవీకరణలు తనిఖీ చేసి ఇన్‌స్టాల్ చేయి.
schema-startup-url = స్టార్టప్ URL
schema-startup-url-detail = ఖాళీగా ఉంటే కమాండ్ బార్ ప్రాంప్ట్ తెరవబడుతుంది.
schema-search-engine = శోధన ఇంజిన్
schema-search-engine-detail = ప్రారంభం మరియు కమాండ్ బార్ నుండి వెబ్ శోధనలకు ఉపయోగించబడుతుంది.
schema-window = విండో
schema-pane = పేన్
schema-side-sheet = సైడ్ షీట్
schema-focus-ring = ఫోకస్ రింగ్
schema-run-placement = రన్ ప్లేస్‌మెంట్ ఓవర్‌రైడ్ అనుమతించు
schema-run-placement-detail = ఏజెంట్లను రన్ పేన్ మోడ్, దిశ మరియు యాంకర్ ఎంచుకోవడానికి అనుమతించు.
schema-leader = లీడర్
schema-leader-detail = కార్డ్ సత్వరమార్గాలకు ప్రిఫిక్స్ కీ.
schema-chord-timeout = కార్డ్ సమయ వ్యవధి
schema-chord-timeout-detail = కార్డ్ ప్రిఫిక్స్ గడువు తీరే మిల్లీసెకన్లు.
schema-bindings = బైండింగులు
schema-confirm-close = మూసివేత నిర్ధారించు
schema-confirm-close-detail = నడుస్తున్న ప్రక్రియతో టెర్మినల్ మూసివేయడానికి ముందు అడగండి.
schema-default-theme = డిఫాల్ట్ థీమ్
schema-default-theme-detail = థీమ్‌ల జాబితా నుండి చురుకైన థీమ్ పేరు.
