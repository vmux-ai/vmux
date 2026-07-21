common-open = खोल्नुहोस्
common-close = बन्द गर्नुहोस्
common-install = स्थापना गर्नुहोस्
common-uninstall = हटाउनुहोस्
common-update = अद्यावधिक गर्नुहोस्
common-retry = पुनः प्रयास गर्नुहोस्
common-refresh = ताजा गर्नुहोस्
common-remove = हटाउनुहोस्
common-enable = सक्षम गर्नुहोस्
common-disable = असक्षम गर्नुहोस्
common-new = नयाँ
common-active = सक्रिय
common-running = चलिरहेको
common-done = सम्पन्न
common-failed = असफल
common-installed = स्थापित
common-items = { $count ->
    [one] { $count } वस्तु
   *[other] { $count } वस्तुहरू
}
start-title = सुरुआत
start-tagline = एउटा प्रम्प्ट। जे पनि, सम्पन्न।

agents-title = एजेन्टहरू
agents-search = ACP र CLI एजेन्टहरू खोज्नुहोस्…
agents-empty = कुनै मिल्दो एजेन्ट छैन
agents-empty-detail = नाम, रनटाइम, वा ACP/CLI प्रयास गर्नुहोस्।
agents-install-failed = स्थापना असफल भयो
agents-updating = अद्यावधिक हुँदैछ…
agents-retrying = पुनः प्रयास हुँदैछ…
agents-preparing = तयारी हुँदैछ…

extensions-title = विस्तारहरू
extensions-search = स्थापित वा Chrome Web Store खोज्नुहोस्…
extensions-relaunch = लागू गर्न पुनः सुरु गर्नुहोस्
extensions-empty = कुनै विस्तार स्थापित छैन
extensions-no-match = कुनै मिल्दो विस्तार छैन
extensions-empty-detail = माथि Chrome Web Store खोज्नुहोस् र Return थिच्नुहोस्।
extensions-no-match-detail = अर्को नाम वा विस्तार ID प्रयास गर्नुहोस्।
extensions-on = चालु
extensions-off = बन्द
extensions-enable-confirm = { $name } सक्षम गर्ने?
extensions-enable-permissions = { $name } सक्षम गर्नुहोस् र अनुमति दिनुहोस्:

lsp-title = भाषा सर्भरहरू
lsp-search = भाषा सर्भर, लिन्टर, फर्म्याटर खोज्नुहोस्…
lsp-loading = क्याटलग लोड हुँदैछ…
lsp-empty = कुनै मिल्दो भाषा सर्भर छैन
lsp-empty-detail = अर्को भाषा, लिन्टर, वा फर्म्याटर प्रयास गर्नुहोस्।
lsp-needs = { $tool } आवश्यक छ
lsp-status-available = उपलब्ध
lsp-status-on-path = PATH मा
lsp-status-installing = स्थापना हुँदैछ…
lsp-status-installed = स्थापित
lsp-status-outdated = अद्यावधिक उपलब्ध
lsp-status-running = चलिरहेको
lsp-status-failed = असफल

spaces-title = स्पेसहरू
spaces-new-placeholder = नयाँ स्पेसको नाम
spaces-empty = कुनै स्पेस छैन
spaces-default-name = स्पेस { $number }
spaces-tabs = { $count ->
    [one] १ ट्याब
   *[other] { $count } ट्याबहरू
}
spaces-delete = स्पेस मेटाउनुहोस्

team-title = टिम
team-just-you = यस स्पेसमा केवल तपाईं
team-agents = { $count ->
    [one] तपाईं र १ एजेन्ट
   *[other] तपाईं र { $count } एजेन्टहरू
}
team-empty = अहिले यहाँ कोही छैन
team-you = तपाईं
team-agent = एजेन्ट

services-title = पृष्ठभूमि सेवाहरू
services-processes = { $count ->
    [one] १ प्रक्रिया
   *[other] { $count } प्रक्रियाहरू
}
services-kill-all = सबै बन्द गर्नुहोस्
services-not-running = सेवा चलिरहेको छैन
services-start-with = सँग सुरु गर्नुहोस्:
services-empty = कुनै सक्रिय प्रक्रिया छैन
services-filter = प्रक्रियाहरू फिल्टर गर्नुहोस्…
services-no-match = कुनै मिल्दो प्रक्रिया छैन
services-connected = जोडिएको
services-disconnected = विच्छेद भएको
services-attached = संलग्न
services-kill = बन्द गर्नुहोस्
services-memory = मेमोरी
services-size = साइज
services-shell = शेल

error-title = त्रुटि

history-search = इतिहास खोज्नुहोस्
history-clear-all = सबै हटाउनुहोस्
history-clear-confirm = सबै इतिहास हटाउने?
history-clear-warning = यो पूर्ववत गर्न सकिँदैन।
history-cancel = रद्द गर्नुहोस्
history-today = आज
history-yesterday = हिजो
history-days-ago = { $count } दिन अघि
history-day-offset = दिन -{ $count }

settings-title = सेटिङहरू
settings-loading = सेटिङहरू लोड हुँदैछ…
settings-stored = ~/.vmux/settings.ron मा भण्डारित
settings-other = अन्य
settings-software-update = सफ्टवेयर अद्यावधिक
settings-check-updates = अद्यावधिक जाँच गर्नुहोस्
settings-check-updates-hint = स्वतः-अद्यावधिक सक्षम हुँदा सुरुमा र प्रत्येक घण्टामा स्वचालित रूपमा जाँच गर्दछ।
settings-update-unavailable = अनुपलब्ध
settings-update-unavailable-hint = यस बिल्डमा अपडेटर समावेश छैन।
settings-update-checking = जाँच हुँदैछ…
settings-update-checking-hint = अद्यावधिक जाँच हुँदैछ…
settings-update-check-again = फेरि जाँच गर्नुहोस्
settings-update-current = Vmux अद्यावधिक छ।
settings-update-downloading = डाउनलोड हुँदैछ…
settings-update-downloading-hint = Vmux { $version } डाउनलोड हुँदैछ…
settings-update-installing = स्थापना हुँदैछ…
settings-update-installing-hint = Vmux { $version } स्थापना हुँदैछ…
settings-update-ready = अद्यावधिक तयार छ
settings-update-ready-hint = Vmux { $version } तयार छ। लागू गर्न पुनः सुरु गर्नुहोस्।
settings-update-try-again = फेरि प्रयास गर्नुहोस्
settings-update-failed = अद्यावधिक जाँच गर्न असमर्थ।
settings-item = वस्तु
settings-item-number = वस्तु { $number }
settings-press-key = कुञ्जी थिच्नुहोस्…
settings-saved = सुरक्षित भयो
settings-record-key = नयाँ कुञ्जी कम्बो रेकर्ड गर्न क्लिक गर्नुहोस्

tray-open-window = विन्डो खोल्नुहोस्
tray-close-window = विन्डो बन्द गर्नुहोस्
tray-pause-recording = रेकर्डिङ रोक्नुहोस्
tray-resume-recording = रेकर्डिङ जारी राख्नुहोस्
tray-finish-recording = रेकर्डिङ समाप्त गर्नुहोस्
tray-quit = Vmux बन्द गर्नुहोस्

composer-attach-files = फाइलहरू संलग्न गर्नुहोस् (/upload)
composer-remove-attachment = संलग्नक हटाउनुहोस्

layout-back = पछाडि
layout-forward = अगाडि
layout-reload = पुनः लोड गर्नुहोस्
layout-bookmark-page = यो पृष्ठ बुकमार्क गर्नुहोस्
layout-remove-bookmark = बुकमार्क हटाउनुहोस्
layout-pin-page = यो पृष्ठ पिन गर्नुहोस्
layout-unpin-page = यो पृष्ठ अनपिन गर्नुहोस्
layout-manage-extensions = विस्तारहरू व्यवस्थापन गर्नुहोस्
layout-new-stack = नयाँ स्ट्याक
layout-close-tab = ट्याब बन्द गर्नुहोस्
layout-bookmark = बुकमार्क
layout-pin = पिन
layout-new-tab = नयाँ ट्याब
layout-team = टिम

command-switch-space = स्पेस बदल्नुहोस्…
command-search-ask = खोज्नुहोस् वा सोध्नुहोस्…
command-new-tab-placeholder = खोज्नुहोस् वा URL टाइप गर्नुहोस्, वा टर्मिनल छान्नुहोस्…
command-placeholder = URL टाइप गर्नुहोस्, ट्याब खोज्नुहोस्, वा आदेशका लागि >…
command-composer-placeholder = आदेशका लागि / वा मिडियाका लागि @ टाइप गर्नुहोस्
command-send = पठाउनुहोस् (Enter)
command-terminal = टर्मिनल
command-open-terminal = टर्मिनलमा खोल्नुहोस्
command-stack = स्ट्याक
command-tabs = { $count ->
    [one] १ ट्याब
   *[other] { $count } ट्याबहरू
}
command-prompt = प्रम्प्ट
command-new-tab = नयाँ ट्याब
command-search = खोज्नुहोस्
command-open-value = "{ $value }" खोल्नुहोस्
command-search-value = "{ $value }" खोज्नुहोस्

schema-appearance = रूपरेखा
schema-general = सामान्य
schema-layout = लेआउट
schema-layout-detail = विन्डो, प्यानल, साइडबार, र फोकस रिङ।
schema-agent = एजेन्ट
schema-agent-detail = एजेन्ट व्यवहार र उपकरण अनुमतिहरू।
schema-shortcuts = सर्टकटहरू
schema-shortcuts-detail = पढ्न-मात्र दृश्य। बाइन्डिङ परिवर्तन गर्न settings.ron सिधा सम्पादन गर्नुहोस्।
schema-terminal = टर्मिनल
schema-browser = ब्राउजर
schema-mode = मोड
schema-mode-detail = वेब पृष्ठहरूको रङ योजना। यन्त्रले तपाईंको प्रणाली अनुसरण गर्दछ।
schema-device = यन्त्र
schema-light = हल्का
schema-dark = गाढा
schema-language = भाषा
schema-language-detail = प्रणाली, en-US, ja, वा कुनै BCP 47 ट्याग र मिल्दो ~/.vmux/locales/<tag>.ftl क्याटलग प्रयोग गर्नुहोस्।
schema-auto-update = स्वतः-अद्यावधिक
schema-auto-update-detail = सुरुमा र प्रत्येक घण्टामा अद्यावधिक जाँच र स्थापना गर्नुहोस्।
schema-startup-url = स्टार्टअप URL
schema-startup-url-detail = खाली छोड्दा आदेश बार प्रम्प्ट खुल्दछ।
schema-search-engine = खोज इन्जिन
schema-search-engine-detail = सुरुआत र आदेश बारबाट वेब खोजका लागि प्रयोग हुन्छ।
schema-window = विन्डो
schema-pane = प्यानल
schema-side-sheet = साइड शिट
schema-focus-ring = फोकस रिङ
schema-run-placement = रन प्लेसमेन्ट ओभरराइड अनुमति दिनुहोस्
schema-run-placement-detail = एजेन्टहरूलाई रन प्यान मोड, दिशा र एङ्कर छान्न दिनुहोस्।
schema-leader = लिडर
schema-leader-detail = कर्ड सर्टकटका लागि प्रिफिक्स कुञ्जी।
schema-chord-timeout = कर्ड टाइमआउट
schema-chord-timeout-detail = कर्ड प्रिफिक्स समाप्त हुनुभन्दा पहिलेको मिलिसेकेन्ड।
schema-bindings = बाइन्डिङहरू
schema-confirm-close = बन्द गर्नु अघि पुष्टि गर्नुहोस्
schema-confirm-close-detail = चलिरहेको प्रक्रियासहितको टर्मिनल बन्द गर्नुअघि सोध्नुहोस्।
schema-default-theme = पूर्वनिर्धारित थिम
schema-default-theme-detail = थिम सूचीबाट सक्रिय थिमको नाम।
