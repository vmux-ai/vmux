locale-name = नेपाली
common-open = खोल्नुहोस्
common-close = बन्द गर्नुहोस्
common-install = स्थापना गर्नुहोस्
common-uninstall = हटाउनुहोस्
common-update = अपडेट गर्नुहोस्
common-retry = फेरि प्रयास गर्नुहोस्
common-refresh = ताजा गर्नुहोस्
common-remove = हटाउनुहोस्
common-enable = सक्षम गर्नुहोस्
common-disable = असक्षम गर्नुहोस्
common-new = नयाँ
common-active = सक्रिय
common-running = चलिरहेको
common-done = सकियो
common-failed = असफल
common-installed = स्थापित
common-items = { $count ->
    [one] { $count } वस्तु
   *[other] { $count } वस्तुहरू
}

tools-title = उपकरणहरू
tools-search = प्याकेज, एजेन्ट, MCP, भाषा उपकरण र कन्फिगरेसन फाइलहरू खोज्नुहोस्…
tools-open = उपकरणहरू खोल्नुहोस्
tools-fold = उपकरणहरू खुम्च्याउनुहोस्
tools-unfold = उपकरणहरू फैलाउनुहोस्
tools-scanning = स्थानीय उपकरणहरू स्क्यान गरिँदै छ…
tools-no-installed = कुनै उपकरण स्थापना गरिएको छैन
tools-empty = मिल्दो उपकरण छैन
tools-empty-detail = प्याकेज स्थापना गर्नुहोस् वा Stow-शैलीको कन्फिगरेसन फाइल प्याकेज थप्नुहोस्।
tools-apply = लागू गर्नुहोस्
tools-homebrew = Homebrew
tools-homebrew-sync = स्थापना गरिएका सूत्र र अनुप्रयोगहरू स्वतः सिङ्क हुन्छन्।
tools-open-brewfile = Brewfile खोल्नुहोस्
tools-managed = व्यवस्थित
tools-provider-homebrew-formulae = Homebrew सूत्रहरू
tools-provider-homebrew-casks = Homebrew अनुप्रयोगहरू
tools-provider-npm = npm प्याकेजहरू
tools-provider-acp-agents = ACP एजेन्टहरू
tools-provider-language-tools = भाषा उपकरणहरू
tools-provider-mcp-servers = MCP सर्भरहरू
tools-provider-dotfiles = कन्फिगरेसन फाइलहरू
tools-status-available = उपलब्ध
tools-status-missing = हराइरहेको
tools-status-conflict = द्वन्द्व
tools-forget = बिर्सनुहोस्
tools-manage = व्यवस्थापन गर्नुहोस्
tools-link = लिङ्क गर्नुहोस्
tools-unlink = लिङ्क हटाउनुहोस्
tools-import = आयात गर्नुहोस्
tools-update-count = { $count ->
    [one] १ अद्यावधिक
   *[other] { $count } अद्यावधिक
}
tools-conflict-count = { $count ->
    [one] १ द्वन्द्व
   *[other] { $count } द्वन्द्व
}
tools-result-applied = उपकरणहरू लागू गरिए
tools-result-imported = उपकरणहरू आयात गरिए
tools-result-installed = { $name } स्थापना गरियो
tools-result-updated = { $name } अद्यावधिक गरियो
tools-result-uninstalled = { $name } हटाइयो
tools-result-forgotten = { $name } बिर्सियो
tools-result-managed = { $name } अब व्यवस्थित छ
tools-result-linked = { $name } लिङ्क गरियो
tools-result-unlinked = { $name } को लिङ्क हटाइयो
vault-title = Vault
vault-open = { common-open } Vault
vault-description = सेटिङहरू, उपकरणहरू, डटफाइलहरू, र Git सँग ज्ञान सिंक गर्नुहोस्।
vault-sync = सिंक
vault-create = सिर्जना गर्नुहोस्
vault-connect = जडान गर्नुहोस्
vault-private = निजी भण्डार
vault-public-warning = सार्वजनिक भण्डारहरूले तपाईंको ज्ञान र कन्फिगरेसनलाई उजागर गर्दछ।
vault-choose-repository = एउटा भण्डार छान्नुहोस्...
vault-empty = खाली
vault-clean = अप टु डेट
vault-not-connected = जोडिएको छैन
vault-change-count = परिवर्तनहरू: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = सुरु गर्नुहोस्
start-tagline = एउटै Prompt। जे पनि, पूरा।

agents-title = Agentहरू
agents-search = ACP र CLI Agent खोज्नुहोस्…
agents-empty = मिल्ने Agent भेटिएन
agents-empty-detail = नाम, रनटाइम, वा ACP/CLI प्रयोग गरी हेर्नुहोस्।
agents-install-failed = स्थापना असफल भयो
agents-updating = अपडेट हुँदै…
agents-retrying = फेरि प्रयास हुँदै…
agents-preparing = तयारी हुँदै…

extensions-title = Extensions
extensions-search = स्थापित वा Chrome Web Store मा खोज्नुहोस्…
extensions-relaunch = लागू गर्न फेरि सुरु गर्नुहोस्
extensions-empty = कुनै Extension स्थापित छैन
extensions-no-match = मिल्ने Extension भेटिएन
extensions-empty-detail = माथि Chrome Web Store मा खोज्नुहोस् र Return थिच्नुहोस्।
extensions-no-match-detail = अर्को नाम वा Extension ID प्रयोग गरी हेर्नुहोस्।
extensions-on = अन
extensions-off = अफ
extensions-enable-confirm = { $name } सक्षम गर्ने?
extensions-enable-permissions = { $name } सक्षम गरी अनुमति दिनुहोस्:

lsp-title = भाषा सर्भरहरू
lsp-search = भाषा सर्भर, linter, formatter खोज्नुहोस्…
lsp-loading = catalog लोड हुँदै…
lsp-empty = मिल्ने भाषा सर्भर भेटिएन
lsp-empty-detail = अर्को भाषा, linter, वा formatter प्रयोग गरी हेर्नुहोस्।
lsp-needs = { $tool } चाहिन्छ
lsp-status-available = उपलब्ध
lsp-status-on-path = PATH मा छ
lsp-status-installing = स्थापना हुँदै…
lsp-status-installed = स्थापित
lsp-status-outdated = अपडेट उपलब्ध छ
lsp-status-running = चलिरहेको
lsp-status-failed = असफल

spaces-title = स्पेसहरू
spaces-new-placeholder = नयाँ स्पेसको नाम
spaces-empty = कुनै स्पेस छैन
spaces-default-name = स्पेस { $number }
spaces-tabs = { $count ->
    [one] 1 ट्याब
   *[other] { $count } ट्याबहरू
}
spaces-delete = स्पेस मेटाउनुहोस्

team-title = टोली
team-just-you = यो स्पेसमा तपाईं मात्र हुनुहुन्छ
team-agents = { $count ->
    [one] तपाईं र 1 Agent
   *[other] तपाईं र { $count } Agentहरू
}
team-empty = यहाँ अझै कोही छैन
team-you = तपाईं
team-agent = Agent

services-title = पृष्ठभूमि सेवाहरू
services-processes = { $count ->
    [one] 1 प्रक्रिया
   *[other] { $count } प्रक्रियाहरू
}
services-kill-all = सबै जबर्जस्ती बन्द गर्नुहोस्
services-not-running = सेवा चलिरहेको छैन
services-start-with = यससँग सुरु गर्नुहोस्:
services-empty = सक्रिय प्रक्रिया छैन
services-filter = प्रक्रिया फिल्टर गर्नुहोस्…
services-no-match = मिल्ने प्रक्रिया भेटिएन
services-connected = जडान भयो
services-disconnected = जडान विच्छेद भयो
services-attached = संलग्न
services-kill = जबर्जस्ती बन्द गर्नुहोस्
services-memory = मेमोरी
services-size = साइज
services-shell = Shell

error-title = त्रुटि

history-search = इतिहास खोज्नुहोस्
history-clear-all = सबै मेटाउनुहोस्
history-clear-confirm = सबै इतिहास मेटाउने?
history-clear-warning = यो कार्य फिर्ता गर्न सकिँदैन।
history-cancel = रद्द गर्नुहोस्
history-today = आज
history-yesterday = हिजो
history-days-ago = { $count } दिनअघि
history-day-offset = दिन -{ $count }

settings-title = सेटिङहरू
settings-loading = सेटिङ लोड हुँदै…
settings-stored = ~/.vmux/settings.ron मा भण्डार गरिएको
settings-other = अन्य
settings-software-update = सफ्टवेयर अपडेट
settings-check-updates = अपडेट जाँच गर्नुहोस्
settings-check-updates-hint = Auto-update सक्षम हुँदा सुरु गर्दा र हरेक घण्टा स्वतः जाँच गर्छ।
settings-update-unavailable = उपलब्ध छैन
settings-update-unavailable-hint = यो build मा updater समावेश छैन।
settings-update-checking = जाँचिँदै…
settings-update-checking-hint = अपडेट जाँचिँदै…
settings-update-check-again = फेरि जाँच गर्नुहोस्
settings-update-current = Vmux पछिल्लो संस्करणमा छ।
settings-update-downloading = डाउनलोड हुँदै…
settings-update-downloading-hint = Vmux { $version } डाउनलोड हुँदै…
settings-update-installing = स्थापना हुँदै…
settings-update-installing-hint = Vmux { $version } स्थापना हुँदै…
settings-update-ready = अपडेट तयार छ
settings-update-ready-hint = Vmux { $version } तयार छ। लागू गर्न पुनः सुरु गर्नुहोस्।
settings-update-try-again = फेरि प्रयास गर्नुहोस्
settings-update-failed = अपडेट जाँच गर्न सकिएन।
settings-item = वस्तु
settings-item-number = वस्तु { $number }
settings-press-key = कुनै कुञ्जी थिच्नुहोस्…
settings-saved = सुरक्षित भयो
settings-record-key = नयाँ कुञ्जी संयोजन रेकर्ड गर्न क्लिक गर्नुहोस्

tray-open-window = विन्डो खोल्नुहोस्
tray-close-window = विन्डो बन्द गर्नुहोस्
tray-pause-recording = रेकर्डिङ रोक्नुहोस्
tray-resume-recording = रेकर्डिङ जारी राख्नुहोस्
tray-finish-recording = रेकर्डिङ समाप्त गर्नुहोस्
tray-quit = Vmux बन्द गर्नुहोस्

composer-attach-files = फाइलहरू संलग्न गर्नुहोस् (/upload)
composer-remove-attachment = संलग्न फाइल हटाउनुहोस्

layout-back = पछाडि
layout-forward = अगाडि
layout-reload = फेरि लोड गर्नुहोस्
layout-bookmark-page = यो पृष्ठ बुकमार्क गर्नुहोस्
layout-remove-bookmark = बुकमार्क हटाउनुहोस्
layout-pin-page = यो पृष्ठ पिन गर्नुहोस्
layout-unpin-page = यो पृष्ठ अनपिन गर्नुहोस्
layout-manage-extensions = Extensions व्यवस्थापन गर्नुहोस्
layout-new-stack = नयाँ स्ट्याक
layout-close-tab = ट्याब बन्द गर्नुहोस्
layout-bookmark = बुकमार्क
layout-pin = पिन
layout-new-tab = नयाँ ट्याब
layout-team = टोली

command-switch-space = स्पेस बदल्नुहोस्…
command-search-ask = खोज्नुहोस् वा सोध्नुहोस्…
command-new-tab-placeholder = खोज्नुहोस् वा URL टाइप गर्नुहोस्, वा Terminal छान्नुहोस्…
command-placeholder = URL टाइप गर्नुहोस्, ट्याब खोज्नुहोस्, वा आदेशका लागि > टाइप गर्नुहोस्…
command-composer-placeholder = आदेशका लागि / वा मिडियाका लागि @ टाइप गर्नुहोस्
command-send = पठाउनुहोस् (Enter)
command-terminal = Terminal
command-open-terminal = Terminal मा खोल्नुहोस्
command-stack = स्ट्याक
command-tabs = { $count ->
    [one] 1 ट्याब
   *[other] { $count } ट्याबहरू
}
command-prompt = Prompt
command-new-tab = नयाँ ट्याब
command-search = खोज्नुहोस्
command-open-value = “{ $value }” खोल्नुहोस्
command-search-value = “{ $value }” खोज्नुहोस्

schema-appearance = रूपरङ
schema-general = सामान्य
schema-layout = लेआउट
schema-layout-detail = विन्डो, पेन, साइडबार, र focus ring।
schema-agent = Agent
schema-agent-detail = Agent को व्यवहार र tool अनुमतिहरू।
schema-shortcuts = सर्टकटहरू
schema-shortcuts-detail = हेर्न मात्र। binding परिवर्तन गर्न settings.ron सिधै सम्पादन गर्नुहोस्।
schema-terminal = Terminal
schema-browser = Browser
schema-mode = मोड
schema-mode-detail = वेब पृष्ठहरूको रङ योजना। Device ले तपाईंको सिस्टम पछ्याउँछ।
schema-device = Device
schema-light = उज्यालो
schema-dark = अँध्यारो
schema-language = भाषा
schema-language-detail = system, en-US, ja, वा मिल्दो ~/.vmux/locales/<tag>.ftl catalog भएको कुनै पनि BCP 47 tag प्रयोग गर्नुहोस्।
schema-auto-update = Auto-update
schema-auto-update-detail = सुरु गर्दा र हरेक घण्टा अपडेट जाँच गरी स्थापना गर्नुहोस्।
schema-startup-url = Startup URL
schema-startup-url-detail = खाली भए command bar prompt खुल्छ।
schema-search-engine = खोज इन्जिन
schema-search-engine-detail = Start र command bar बाट वेब खोजीका लागि प्रयोग हुन्छ।
schema-window = विन्डो
schema-pane = पेन
schema-side-sheet = साइड sheet
schema-focus-ring = focus ring
schema-run-placement = run placement override अनुमति दिनुहोस्
schema-run-placement-detail = Agentहरूलाई run pane mode, दिशा, र anchor छान्न दिनुहोस्।
schema-leader = Leader
schema-leader-detail = chord सर्टकटहरूको prefix कुञ्जी।
schema-chord-timeout = chord timeout
schema-chord-timeout-detail = chord prefix समाप्त हुनुअघि मिलिसेकेन्ड।
schema-bindings = Bindings
schema-confirm-close = बन्द गर्दा पुष्टि गर्नुहोस्
schema-confirm-close-detail = चलिरहेको प्रक्रिया भएको terminal बन्द गर्नु अघि सोध्नुहोस्।
schema-default-theme = Default theme
schema-default-theme-detail = themes सूचीबाट सक्रिय theme को नाम।

settings-empty = (खाली)
settings-none = (कुनै छैन)

schema-system = प्रणाली
schema-editor = सम्पादक
schema-recording = रेकर्डिङ
schema-radius = अर्धव्यास
schema-padding = प्याडिङ
schema-gap = अन्तर
schema-width = चौडाइ
schema-color = रङ
schema-red = रातो
schema-green = हरियो
schema-blue = नीलो
schema-follow-files = फाइलहरू पछ्याउनुहोस्
schema-tidy-files = फाइलहरू मिलाउनुहोस्
schema-tidy-files-max = फाइल मिलाउने सीमा
schema-tidy-files-auto = फाइलहरू स्वतः मिलाउनुहोस्
schema-app-providers = एप प्रदायकहरू
schema-provider = प्रदायक
schema-kind = प्रकार
schema-models = मोडेलहरू
schema-acp = ACP एजेन्टहरू
schema-id = ID
schema-name = नाम
schema-command = आदेश
schema-arguments = आर्गुमेन्टहरू
schema-environment = वातावरण
schema-working-directory = कार्य डाइरेक्टरी
schema-shell = शेल
schema-font-family = फन्ट परिवार
schema-startup-directory = सुरु डाइरेक्टरी
schema-themes = थिमहरू
schema-color-scheme = रङ योजना
schema-font-size = फन्ट आकार
schema-line-height = लाइन उचाइ
schema-cursor-style = कर्सर शैली
schema-cursor-blink = कर्सर झिम्काइ
schema-custom-themes = अनुकूल थिमहरू
schema-foreground = अग्रभूमि
schema-background = पृष्ठभूमि
schema-cursor = कर्सर
schema-ansi-colors = ANSI रङहरू
schema-keymap = कीम्याप
schema-explorer = अन्वेषक
schema-visible = देखिने
schema-language-servers = भाषा सर्भरहरू
schema-servers = सर्भरहरू
schema-language-id = भाषा ID
schema-root-markers = रुट मार्करहरू
schema-output-directory = आउटपुट डाइरेक्टरी

menu-scene = दृश्य
menu-layout = लेआउट
menu-terminal = टर्मिनल
menu-browser = ब्राउजर
menu-service = सेवा
menu-bookmark = बुकमार्क
menu-edit = सम्पादन

layout-knowledge = ज्ञान
layout-open-knowledge = ज्ञान खोल्नुहोस्
layout-open-welcome-knowledge = ज्ञानमा स्वागत खोल्नुहोस्
layout-open-path = { $path } खोल्नुहोस्
layout-fold-knowledge = ज्ञान फोल्ड गर्नुहोस्
layout-unfold-knowledge = ज्ञान अनफोल्ड गर्नुहोस्
layout-bookmarks = बुकमार्कहरू
layout-new-folder = नयाँ फोल्डर
layout-add-to-bookmarks = बुकमार्कहरूमा थप्नुहोस्
layout-move-to-bookmarks = बुकमार्कहरूमा सार्नुहोस्
layout-stack-number = स्ट्याक { $number }
layout-fold-stack = स्ट्याक फोल्ड गर्नुहोस्
layout-unfold-stack = स्ट्याक अनफोल्ड गर्नुहोस्
layout-close-stack = स्ट्याक बन्द गर्नुहोस्
layout-bookmark-in = { $folder } मा बुकमार्क गर्नुहोस्

common-cancel = रद्द गर्नुहोस्
common-delete = मेटाउनुहोस्
common-save = सेभ गर्नुहोस्
common-rename = नाम फेर्नुहोस्
common-expand = फैलाउनुहोस्
common-collapse = खुम्च्याउनुहोस्
common-loading = लोड हुँदै…
common-error = त्रुटि
common-output = आउटपुट
common-pending = बाँकी
common-current = हालको
common-stop = रोक्नुहोस्
services-command = Vmux सेवा
services-uptime-seconds = { $seconds }से
services-uptime-minutes = { $minutes }मि { $seconds }से
services-uptime-hours = { $hours }घ { $minutes }मि
services-uptime-days = { $days }दिन { $hours }घ

error-page-failed-load = पृष्ठ लोड हुन सकेन
error-page-not-found = पृष्ठ भेटिएन
error-unknown-host = अज्ञात Vmux एप होस्ट: { $host }

history-title = इतिहास

command-new-app-chat = नयाँ { $provider }/{ $model } च्याट (एप)
command-interactive-mode-user = दृश्य > अन्तरक्रियात्मक मोड > प्रयोगकर्ता
command-interactive-mode-player = दृश्य > अन्तरक्रियात्मक मोड > प्लेयर
command-minimize-window = लेआउट > विन्डो > सानो बनाउनुहोस्
command-toggle-layout = लेआउट > लेआउट > लेआउट टगल गर्नुहोस्
command-close-tab = लेआउट > ट्याब > ट्याब बन्द गर्नुहोस्
command-new-task = लेआउट > ट्याब > नयाँ कार्य…
command-next-tab = लेआउट > ट्याब > अर्को ट्याब
command-prev-tab = लेआउट > ट्याब > अघिल्लो ट्याब
command-rename-tab = लेआउट > ट्याब > ट्याबको नाम फेर्नुहोस्
command-tab-select-1 = लेआउट > ट्याब > ट्याब १ छान्नुहोस्
command-tab-select-2 = लेआउट > ट्याब > ट्याब २ छान्नुहोस्
command-tab-select-3 = लेआउट > ट्याब > ट्याब ३ छान्नुहोस्
command-tab-select-4 = लेआउट > ट्याब > ट्याब ४ छान्नुहोस्
command-tab-select-5 = लेआउट > ट्याब > ट्याब ५ छान्नुहोस्
command-tab-select-6 = लेआउट > ट्याब > ट्याब ६ छान्नुहोस्
command-tab-select-7 = लेआउट > ट्याब > ट्याब ७ छान्नुहोस्
command-tab-select-8 = लेआउट > ट्याब > ट्याब ८ छान्नुहोस्
command-tab-select-last = लेआउट > ट्याब > अन्तिम ट्याब छान्नुहोस्
command-close-pane = लेआउट > पेन > पेन बन्द गर्नुहोस्
command-select-pane-left = लेआउट > पेन > बायाँ पेन छान्नुहोस्
command-select-pane-right = लेआउट > पेन > दायाँ पेन छान्नुहोस्
command-select-pane-up = लेआउट > पेन > माथिको पेन छान्नुहोस्
command-select-pane-down = लेआउट > पेन > तलको पेन छान्नुहोस्
command-swap-pane-prev = लेआउट > पेन > अघिल्लो पेनसँग साट्नुहोस्
command-swap-pane-next = लेआउट > पेन > अर्को पेनसँग साट्नुहोस्
command-equalize-pane-size = लेआउट > पेन > पेनको आकार बराबर बनाउनुहोस्
command-resize-pane-left = लेआउट > पेन > पेन बायाँतिर रिसाइज गर्नुहोस्
command-resize-pane-right = लेआउट > पेन > पेन दायाँतिर रिसाइज गर्नुहोस्
command-resize-pane-up = लेआउट > पेन > पेन माथितिर रिसाइज गर्नुहोस्
command-resize-pane-down = लेआउट > पेन > पेन तलतिर रिसाइज गर्नुहोस्
command-stack-close = लेआउट > स्ट्याक > स्ट्याक बन्द गर्नुहोस्
command-stack-next = लेआउट > स्ट्याक > अर्को स्ट्याक
command-stack-previous = लेआउट > स्ट्याक > अघिल्लो स्ट्याक
command-stack-reopen = लेआउट > स्ट्याक > बन्द पृष्ठ फेरि खोल्नुहोस्
command-stack-swap-prev = लेआउट > स्ट्याक > स्ट्याक बायाँ सार्नुहोस्
command-stack-swap-next = लेआउट > स्ट्याक > स्ट्याक दायाँ सार्नुहोस्
command-space-open = लेआउट > स्पेस > स्पेसहरू
command-terminal-close = टर्मिनल > टर्मिनल बन्द गर्नुहोस्
command-terminal-next = टर्मिनल > अर्को टर्मिनल
command-terminal-prev = टर्मिनल > अघिल्लो टर्मिनल
command-terminal-clear = टर्मिनल > टर्मिनल खाली गर्नुहोस्
command-browser-prev-page = ब्राउजर > नेभिगेसन > पछाडि
command-browser-next-page = ब्राउजर > नेभिगेसन > अगाडि
command-browser-reload = ब्राउजर > नेभिगेसन > पुनः लोड गर्नुहोस्
command-browser-hard-reload = ब्राउजर > नेभिगेसन > पूर्ण पुनः लोड
command-open-in-place = ब्राउजर > खोल्नुहोस् > यहीँ खोल्नुहोस्
command-open-in-new-stack = ब्राउजर > खोल्नुहोस् > नयाँ स्ट्याकमा खोल्नुहोस्
command-open-in-pane-top = ब्राउजर > खोल्नुहोस् > माथिको पेनमा खोल्नुहोस्
command-open-in-pane-right = ब्राउजर > खोल्नुहोस् > दायाँ पेनमा खोल्नुहोस्
command-open-in-pane-bottom = ब्राउजर > खोल्नुहोस् > तलको पेनमा खोल्नुहोस्
command-open-in-pane-left = ब्राउजर > खोल्नुहोस् > बायाँ पेनमा खोल्नुहोस्
command-open-in-new-tab = ब्राउजर > खोल्नुहोस् > नयाँ ट्याबमा खोल्नुहोस्
command-open-in-new-space = ब्राउजर > खोल्नुहोस् > नयाँ स्पेसमा खोल्नुहोस्
command-browser-zoom-in = ब्राउजर > दृश्य > जुम इन
command-browser-zoom-out = ब्राउजर > दृश्य > जुम आउट
command-browser-zoom-reset = ब्राउजर > दृश्य > वास्तविक आकार
command-browser-dev-tools = ब्राउजर > दृश्य > डेभलपर टुल्स
command-browser-open-command-bar = ब्राउजर > बार > कमाण्ड बार
command-browser-open-page-in-command-bar = ब्राउजर > बार > पृष्ठ सम्पादन गर्नुहोस्
command-browser-open-path-bar = ब्राउजर > बार > पाथ नेभिगेटर
command-browser-open-commands = ब्राउजर > बार > कमाण्डहरू
command-browser-open-history = ब्राउजर > बार > इतिहास
command-service-open = सेवा > सेवा मनिटर खोल्नुहोस्
command-bookmark-toggle-active = बुकमार्क > पृष्ठ बुकमार्क गर्नुहोस्
command-bookmark-pin-active = बुकमार्क > पृष्ठ पिन गर्नुहोस्

layout-tab = ट्याब
layout-no-stacks = कुनै स्ट्याक छैन
layout-loading = लोड हुँदै…
layout-no-markdown-files = Markdown फाइल छैन
layout-empty-folder = खाली फोल्डर
layout-worktree = वर्कट्री
layout-folder-name = फोल्डरको नाम
layout-no-pins-bookmarks = पिन वा बुकमार्क छैन
layout-move-to = { $folder } मा सार्नुहोस्
layout-bookmark-current-page = हालको पृष्ठ बुकमार्क गर्नुहोस्
layout-rename-folder = फोल्डरको नाम फेर्नुहोस्
layout-remove-folder = फोल्डर हटाउनुहोस्
layout-update-downloading = अपडेट डाउनलोड हुँदै
layout-update-installing = अपडेट स्थापना हुँदै…
layout-update-ready = नयाँ संस्करण उपलब्ध छ
layout-restart-update = अपडेट गर्न पुनः सुरु गर्नुहोस्

agent-preparing = एजेन्ट तयार हुँदै…
agent-send-all-queued = पङ्क्तिबद्ध सबै प्रम्प्ट अहिले पठाउनुहोस् (Esc)
agent-send = पठाउनुहोस् (Enter)
agent-ready = तपाईं तयार हुँदा म तयार छु।
agent-loading-older = पुराना सन्देश लोड हुँदै…
agent-load-older = पुराना सन्देश लोड गर्नुहोस्
agent-continued-from = { $source } बाट जारी
agent-older-context-omitted = पुरानो सन्दर्भ हटाइएको छ
agent-interrupted = अवरुद्ध भयो
agent-allow-tool = { $tool } अनुमति दिने?
agent-deny = अस्वीकार
agent-allow-always = सधैं अनुमति दिनुहोस्
agent-allow = अनुमति दिनुहोस्
agent-loading-sessions = सत्रहरू लोड हुँदै…
agent-no-resumable-sessions = फेरि सुरु गर्न मिल्ने सत्र भेटिएन
agent-no-matching-sessions = मिल्ने सत्र छैन
agent-no-matching-models = मिल्ने मोडेल छैन
agent-choice-help = ↑/↓ वा Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = रिपोजिटरी फोल्डर छान्नुहोस्
agent-choose-repository-detail = एजेन्टले प्रयोग गर्ने स्थानीय Git रिपोजिटरी छान्नुहोस्।
agent-choosing = छानिँदै…
agent-choose-folder = फोल्डर छान्नुहोस्
agent-queued = पङ्क्तिमा
agent-attached = संलग्न:
agent-cancel-queued = पङ्क्तिबद्ध प्रम्प्ट रद्द गर्नुहोस्
agent-resume-queued = पङ्क्तिबद्ध प्रम्प्टहरू फेरि सुरु गर्नुहोस्
agent-clear-queue = पङ्क्ति खाली गर्नुहोस्
agent-send-all-now = सबै अहिले पठाउनुहोस्
agent-choose-option = माथिबाट विकल्प छान्नुहोस्
agent-loading-media = मिडिया लोड हुँदै…
agent-no-matching-media = मिल्ने मिडिया छैन
agent-prompt-context = प्रम्प्ट सन्दर्भ
agent-details = विवरण
agent-path = पाथ
agent-tool = टुल
agent-server = सर्भर
agent-bytes = { $count } बाइट
agent-worked-for = { $duration } काम गर्‍यो
agent-worked-for-steps = { $count ->
    [one] { $duration } काम गर्‍यो · १ चरण
   *[other] { $duration } काम गर्‍यो · { $count } चरण
}
agent-tool-guardian-review = गार्डियन समीक्षा
agent-tool-read-files = फाइलहरू पढ्यो
agent-tool-viewed-image = छवि हेर्‍यो
agent-tool-used-browser = ब्राउजर प्रयोग गर्‍यो
agent-tool-searched-files = फाइलहरू खोज्यो
agent-tool-ran-commands = कमाण्डहरू चलायो
agent-thinking = सोच्दै
agent-subagent = सहायक एजेन्ट
agent-prompt = प्रम्प्ट
agent-thread = थ्रेड
agent-parent = अभिभावक
agent-children = सन्तान
agent-call = कल
agent-raw-event = कच्चा घटना
agent-plan = योजना
agent-tasks = { $count ->
    [one] १ कार्य
   *[other] { $count } कार्य
}
agent-edited = सम्पादित
agent-reconnecting = फेरि जडान हुँदै { $attempt }/{ $total }
agent-status-running = चल्दै
agent-status-done = सकियो
agent-status-failed = असफल
agent-status-pending = बाँकी
agent-slash-attach-files = फाइलहरू संलग्न गर्नुहोस्
agent-slash-resume-session = अघिल्लो सत्र फेरि सुरु गर्नुहोस्
agent-slash-select-model = मोडेल छान्नुहोस्
agent-slash-continue-cli = यो सत्र CLI मा जारी राख्नुहोस्
agent-session-just-now = भर्खरै
agent-session-minutes-ago = { $count }मि अघि
agent-session-hours-ago = { $count }घ अघि
agent-session-days-ago = { $count }दिन अघि
agent-working-working = काम गर्दै
agent-working-thinking = सोच्दै
agent-working-pondering = विचार गर्दै
agent-working-noodling = उपाय खोज्दै
agent-working-percolating = परिपक्व गर्दै
agent-working-conjuring = जादू गर्दै
agent-working-cooking = पकाउँदै
agent-working-brewing = तयार गर्दै
agent-working-musing = मनन गर्दै
agent-working-ruminating = गहिरिएर सोच्दै
agent-working-scheming = योजना बुन्दै
agent-working-synthesizing = संश्लेषण गर्दै
agent-working-tinkering = मिलाउँदै
agent-working-churning = प्रशोधन गर्दै
agent-working-vibing = लय मिलाउँदै
agent-working-simmering = मन्द उमाल्दै
agent-working-crafting = बनाउँदै
agent-working-divining = अनुमान गर्दै
agent-working-mulling = सोचविचार गर्दै
agent-working-spelunking = भित्रसम्म खोज्दै

editor-toggle-explorer = एक्सप्लोरर टगल गर्नुहोस् (Cmd+B)
editor-unsaved = सेभ नभएको
editor-rendered-markdown = लाइभ सम्पादनसहित रेन्डर गरिएको Markdown
editor-note = नोट
editor-source-editor = स्रोत सम्पादक
editor-editor = सम्पादक
editor-git-diff = Git डिफ
editor-diff = डिफ
editor-tidy = सफा गर्नुहोस्
editor-always = सधैं
editor-unchanged-previews = { $count ->
    [one] ✦ १ अपरिवर्तित प्रिभ्यु
   *[other] ✦ { $count } अपरिवर्तित प्रिभ्यु
}
editor-open-externally = बाहिर खोल्नुहोस्
editor-changed-line = परिवर्तन भएको लाइन
editor-go-to-definition = परिभाषामा जानुहोस्
editor-find-references = सन्दर्भहरू खोज्नुहोस्
editor-references = { $count ->
    [one] १ सन्दर्भ
   *[other] { $count } सन्दर्भ
}
editor-lsp-starting = { $server } सुरु हुँदै…
editor-lsp-not-installed = { $server } — स्थापना गरिएको छैन
editor-explorer = एक्सप्लोरर
editor-open-editors = खुला सम्पादकहरू
editor-outline = रूपरेखा
editor-new-file = नयाँ फाइल
editor-new-folder = नयाँ फोल्डर
editor-delete-confirm = “{ $name }” मेटाउने? यसलाई फिर्ता गर्न सकिँदैन।
editor-created-folder = फोल्डर { $name } सिर्जना भयो
editor-created-file = फाइल { $name } सिर्जना भयो
editor-renamed-to = { $name } मा नाम फेरियो
editor-deleted = { $name } मेटियो
editor-failed-decode-image = छवि डिकोड गर्न सकेन
editor-preview-large-image = छवि (प्रिभ्यु गर्न धेरै ठूलो)
editor-preview-binary = बाइनरी
editor-preview-file = फाइल

git-status-clean = सफा
git-status-modified = परिमार्जित
git-status-staged = स्टेज गरिएको
git-status-staged-modified = स्टेज गरिएको*
git-status-untracked = ट्र्याक नगरिएको
git-status-deleted = मेटिएको
git-status-conflict = द्वन्द्व
git-accept-all = ✓ सबै स्वीकार
git-unstage = स्टेजबाट हटाउनुहोस्
git-confirm-deny-all = सबै अस्वीकार पुष्टि गर्नुहोस्
git-deny-all = ✗ सबै अस्वीकार
git-commit-message = कमिट सन्देश
git-commit = कमिट ({ $count })
git-push = ↑ पुश
git-loading-diff = डिफ लोड हुँदै…
git-no-changes = देखाउन कुनै परिवर्तन छैन
git-accept = ✓ स्वीकार
git-deny = ✗ अस्वीकार
git-show-unchanged-lines = { $count } अपरिवर्तित लाइन देखाउनुहोस्

terminal-loading = लोड हुँदै…
terminal-runs-when-ready = तयार भएपछि चल्छ · Ctrl+C ले खाली गर्छ · Esc ले छोड्छ
terminal-booting = बुट हुँदै
terminal-type-command = कमाण्ड टाइप गर्नुहोस् · तयार भएपछि चल्छ · Esc ले छोड्छ

setup-tagline-claude = Anthropic को कोडिङ एजेन्ट, Vmux मा
setup-tagline-codex = OpenAI को कोडिङ एजेन्ट, Vmux मा
setup-tagline-vibe = Mistral को कोडिङ एजेन्ट, Vmux मा
setup-install-title = { $name } CLI स्थापना गर्नुहोस्
setup-homebrew-required = { $command } स्थापना गर्न Homebrew चाहिन्छ र यो अझै सेट अप गरिएको छैन। Vmux ले पहिले Homebrew, त्यसपछि { $name } स्थापना गर्नेछ।
setup-terminal-instructions = टर्मिनलमा सुरु गर्न Return थिच्नुहोस्, अनि सोधिएपछि आफ्नो Mac पासवर्ड हाल्नुहोस्।
setup-command-missing = स्थानीय { $command } कमाण्ड अझै स्थापना नभएकाले Vmux ले यो पृष्ठ खोलेको हो। यसलाई प्राप्त गर्न तलको कमाण्ड चलाउनुहोस्।
setup-install-failed = स्थापना पूरा भएन। विवरणका लागि टर्मिनल हेर्नुहोस्, अनि फेरि प्रयास गर्नुहोस्।
setup-installing = स्थापना हुँदै…
setup-install-homebrew = Homebrew + { $name } स्थापना गर्नुहोस्
setup-run-install = स्थापना कमाण्ड चलाउनुहोस्
setup-auto-reload = Vmux ले यसलाई टर्मिनलमा चलाउँछ र { $command } तयार भएपछि पुनः लोड गर्छ।

debug-title = डिबग
debug-auto-update = स्वतः अपडेट
debug-simulate-update = अपडेट उपलब्ध भएको सिमुलेट गर्नुहोस्
debug-simulate-download = डाउनलोड सिमुलेट गर्नुहोस्
debug-clear-update = अपडेट खाली गर्नुहोस्
debug-trigger-restart = पुनः सुरु ट्रिगर गर्नुहोस्

command-manage-spaces = स्पेसहरू व्यवस्थापन गर्नुहोस्…
command-pane-stack-location = पेन { $pane } / स्ट्याक { $stack }
command-space-pane-stack-location = { $space } / पेन { $pane } / स्ट्याक { $stack }
command-terminal-path = टर्मिनल ({ $path })
command-group-interactive-mode = अन्तरक्रियात्मक मोड
command-group-window = विन्डो
command-group-tab = ट्याब
command-group-pane = पेन
command-group-stack = स्ट्याक
command-group-space = स्पेस
command-group-navigation = नेभिगेसन
command-group-open = खोल्नुहोस्
command-group-view = दृश्य
command-group-bar = बार

menu-close-vmux = Vmux बन्द गर्नुहोस्

agents-terminal-coding-agent = टर्मिनल-आधारित कोडिङ एजेन्ट
