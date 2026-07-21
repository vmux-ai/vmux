common-open = खुला
common-close = बंद करें
common-install = स्थापित करें
common-uninstall = अनइंस्टॉल करें
common-update = अद्यतन करें
common-retry = पुनः प्रयास करें
common-refresh = ताज़ा करें
common-remove = हटाओ
common-enable = सक्षम करें
common-disable = अक्षम करें
common-new = नया
common-active = सक्रिय
common-running = चल रहा है
common-done = किया
common-failed = असफल
common-installed = स्थापित
common-items = { $count ->
    [one] { $count } आइटम
   *[other] { $count } आइटम
}
start-title = प्रारंभ करें
start-tagline = एक संकेत. कुछ भी, किया.

agents-title = एजेंट
agents-search = ACP और CLI एजेंट खोजें...
agents-empty = कोई मिलान एजेंट नहीं
agents-empty-detail = कोई नाम, रनटाइम, या ACP/CLI आज़माएं।
agents-install-failed = इंस्टॉल विफल
agents-updating = अपडेट हो रहा है...
agents-retrying = पुनः प्रयास किया जा रहा है...
agents-preparing = तैयारी हो रही है...

extensions-title = एक्सटेंशन
extensions-search = स्थापित खोजें या Chrome Web Store…
extensions-relaunch = लागू करने के लिए पुनः लॉन्च करें
extensions-empty = कोई एक्सटेंशन इंस्टॉल नहीं किया गया
extensions-no-match = कोई मेल खाता एक्सटेंशन नहीं
extensions-empty-detail = ऊपर Chrome Web Store खोजें और Return दबाएँ।
extensions-no-match-detail = कोई अन्य नाम या एक्सटेंशन आईडी आज़माएं.
extensions-on = पर
extensions-off = बंद
extensions-enable-confirm = { $name } सक्षम करें?
extensions-enable-permissions = { $name } सक्षम करें और अनुमति दें:

lsp-title = भाषा सर्वर
lsp-search = भाषा सर्वर, लिंटर, फ़ॉर्मेटर खोजें...
lsp-loading = कैटलॉग लोड हो रहा है...
lsp-empty = कोई मेल खाने वाला भाषा सर्वर नहीं
lsp-empty-detail = कोई अन्य भाषा, लिंटर या फ़ॉर्मेटर आज़माएँ।
lsp-needs = आवश्यकता है { $tool }
lsp-status-available = उपलब्ध
lsp-status-on-path = PATH पर
lsp-status-installing = इंस्टाल किया जा रहा है...
lsp-status-installed = स्थापित
lsp-status-outdated = अपडेट उपलब्ध है
lsp-status-running = चल रहा है
lsp-status-failed = असफल

spaces-title = रिक्त स्थान
spaces-new-placeholder = नया स्थान नाम
spaces-empty = कोई रिक्त स्थान नहीं
spaces-default-name = अंतरिक्ष { $number }
spaces-tabs = { $count ->
    [one] 1 टैब
   *[other] { $count } टैब
}
spaces-delete = स्थान हटाएँ

team-title = टीम
team-just-you = इस स्थान पर केवल आप हैं
team-agents = { $count ->
    [one] आप और 1 एजेंट
   *[other] आप और { $count } एजेंट
}
team-empty = अभी तक यहां कोई नहीं है
team-you = आप
team-agent = एजेंट

services-title = पृष्ठभूमि सेवाएँ
services-processes = { $count ->
    [one] 1 प्रक्रिया
   *[other] { $count } प्रक्रियाएं
}
services-kill-all = सबको मार डालो
services-not-running = सेवा नहीं चल रही है
services-start-with = इसके साथ प्रारंभ करें:
services-empty = कोई सक्रिय प्रक्रिया नहीं
services-filter = फ़िल्टर प्रक्रियाएँ…
services-no-match = कोई मिलान प्रक्रिया नहीं
services-connected = जुड़ा हुआ
services-disconnected = विच्छेदित
services-attached = संलग्न
services-kill = मार डालो
services-memory = स्मृति
services-size = आकार
services-shell = शैल

error-title = त्रुटि

history-search = खोज इतिहास
history-clear-all = सब साफ़ करें
history-clear-confirm = सारा इतिहास साफ़ करें?
history-clear-warning = इसे पूर्ववत नहीं किया जा सकता.
history-cancel = रद्द करें
history-today = आज
history-yesterday = कल
history-days-ago = { $count } दिन पहले
history-day-offset = दिन -{ $count }

settings-title = सेटिंग्स
settings-loading = सेटिंग लोड हो रही है...
settings-stored = ~/.vmux/settings.ron में संग्रहीत
settings-other = अन्य
settings-software-update = सॉफ़्टवेयर अद्यतन
settings-check-updates = अद्यतनों के लिए जाँच करें
settings-check-updates-hint = लॉन्च पर और ऑटो-अपडेट सक्षम होने पर हर घंटे स्वचालित रूप से जांच करता है।
settings-update-unavailable = अनुपलब्ध
settings-update-unavailable-hint = इस बिल्ड में अपडेटर शामिल नहीं है.
settings-update-checking = जाँच हो रही है...
settings-update-checking-hint = अपडेट की जांच की जा रही है...
settings-update-check-again = दोबारा जांचें
settings-update-current = Vmux अद्यतित है।
settings-update-downloading = डाउनलोड हो रहा है...
settings-update-downloading-hint = डाउनलोड हो रहा है Vmux { $version }…
settings-update-installing = इंस्टाल किया जा रहा है...
settings-update-installing-hint = Vmux { $version } स्थापित किया जा रहा है...
settings-update-ready = अपडेट तैयार
settings-update-ready-hint = Vmux { $version } तैयार है। इसे लागू करने के लिए पुनः प्रारंभ करें.
settings-update-try-again = पुनः प्रयास करें
settings-update-failed = अद्यतनों की जाँच करने में असमर्थ.
settings-item = वस्तु
settings-item-number = आइटम { $number }
settings-press-key = एक कुंजी दबाएँ...
settings-saved = सहेजा गया
settings-record-key = नई कुंजी कॉम्बो रिकॉर्ड करने के लिए क्लिक करें

tray-open-window = विंडो खोलें
tray-close-window = विंडो बंद करें
tray-pause-recording = रिकॉर्डिंग रोकें
tray-resume-recording = रिकॉर्डिंग फिर से शुरू करें
tray-finish-recording = रिकॉर्डिंग समाप्त करें
tray-quit = Vmux से बाहर निकलें

composer-attach-files = फ़ाइलें संलग्न करें (/upload)
composer-remove-attachment = अनुलग्नक हटाएँ

layout-back = वापस
layout-forward = आगे
layout-reload = पुनः लोड करें
layout-bookmark-page = इस पेज को बुकमार्क करें
layout-remove-bookmark = बुकमार्क हटाएँ
layout-pin-page = इस पेज को पिन करें
layout-unpin-page = इस पेज को अनपिन करें
layout-manage-extensions = एक्सटेंशन प्रबंधित करें
layout-new-stack = नया ढेर
layout-close-tab = टैब बंद करें
layout-bookmark = बुकमार्क
layout-pin = पिन
layout-new-tab = नया टैब
layout-team = टीम

command-switch-space = स्थान बदलें...
command-search-ask = खोजें या पूछें...
command-new-tab-placeholder = URL खोजें या टाइप करें, या टर्मिनल चुनें...
command-placeholder = कमांड के लिए URL टाइप करें, टैब खोजें, या >...
command-composer-placeholder = कमांड के लिए / टाइप करें या मीडिया के लिए @ टाइप करें
command-send = भेजें (Enter)
command-terminal = टर्मिनल
command-open-terminal = टर्मिनल में खोलें
command-stack = ढेर
command-tabs = { $count ->
    [one] 1 टैब
   *[other] { $count } टैब
}
command-prompt = शीघ्र
command-new-tab = नया टैब
command-search = खोजें
command-open-value = “{ $value }” खोलें
command-search-value = “{ $value }” खोजें

schema-appearance = दिखावट
schema-general = सामान्य
schema-layout = लेआउट
schema-layout-detail = विंडो, शीशे, साइडबार और फोकस रिंग।
schema-agent = एजेंट
schema-agent-detail = एजेंट व्यवहार और उपकरण अनुमतियाँ।
schema-shortcuts = शॉर्टकट
schema-shortcuts-detail = केवल पढ़ने योग्य दृश्य. बाइंडिंग बदलने के लिए सीधे settings.ron संपादित करें।
schema-terminal = टर्मिनल
schema-browser = ब्राउज़र
schema-mode = मोड
schema-mode-detail = वेब पेजों के लिए रंग योजना. डिवाइस आपके सिस्टम का अनुसरण करता है.
schema-device = युक्ति
schema-light = रोशनी
schema-dark = अंधेरा
schema-language = भाषा
schema-language-detail = मिलान ~/.vmux/locales/<tag>.ftl कैटलॉग के साथ सिस्टम, en-US, ja, या किसी BCP 47 टैग का उपयोग करें।
schema-auto-update = स्वतः अद्यतन
schema-auto-update-detail = लॉन्च और हर घंटे पर अपडेट जांचें और इंस्टॉल करें।
schema-startup-url = स्टार्टअप URL
schema-startup-url-detail = खाली कमांड बार प्रॉम्प्ट खोलता है।
schema-search-engine = खोज इंजन
schema-search-engine-detail = स्टार्ट और कमांड बार से वेब खोजों के लिए उपयोग किया जाता है।
schema-window = खिड़की
schema-pane = फलक
schema-side-sheet = साइड शीट
schema-focus-ring = फोकस रिंग
schema-run-placement = रन प्लेसमेंट ओवरराइड की अनुमति दें
schema-run-placement-detail = एजेंटों को रन पेन मोड, दिशा और एंकर चुनने दें।
schema-leader = नेता
schema-leader-detail = कॉर्ड शॉर्टकट के लिए उपसर्ग कुंजी.
schema-chord-timeout = कॉर्ड टाइमआउट
schema-chord-timeout-detail = किसी कॉर्ड उपसर्ग के समाप्त होने से पहले मिलीसेकंड.
schema-bindings = बंधन
schema-confirm-close = बंद करने की पुष्टि करें
schema-confirm-close-detail = चल रही प्रक्रिया के साथ टर्मिनल को बंद करने से पहले संकेत दें।
schema-default-theme = डिफ़ॉल्ट थीम
schema-default-theme-detail = थीम सूची से सक्रिय थीम का नाम.
