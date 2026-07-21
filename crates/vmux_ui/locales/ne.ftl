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
