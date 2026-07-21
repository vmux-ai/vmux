locale-name = हिन्दी
common-open = खोलें
common-close = बंद करें
common-install = इंस्टॉल करें
common-uninstall = अनइंस्टॉल करें
common-update = अपडेट करें
common-retry = फिर कोशिश करें
common-refresh = रीफ़्रेश करें
common-remove = हटाएँ
common-enable = सक्षम करें
common-disable = अक्षम करें
common-new = नया
common-active = सक्रिय
common-running = चल रहा है
common-done = पूरा
common-failed = विफल
common-installed = इंस्टॉल हो चुका
common-items = { $count ->
    [one] { $count } आइटम
   *[other] { $count } आइटम
}
start-title = शुरू करें
start-tagline = एक प्रॉम्प्ट। कुछ भी, पूरा।

agents-title = एजेंट
agents-search = ACP और CLI एजेंट खोजें…
agents-empty = कोई मिलता-जुलता एजेंट नहीं
agents-empty-detail = नाम, रनटाइम या ACP/CLI आज़माएँ।
agents-install-failed = इंस्टॉल विफल रहा
agents-updating = अपडेट हो रहा है…
agents-retrying = फिर कोशिश हो रही है…
agents-preparing = तैयारी हो रही है…

extensions-title = एक्सटेंशन
extensions-search = इंस्टॉल किए गए या Chrome Web Store में खोजें…
extensions-relaunch = लागू करने के लिए फिर से खोलें
extensions-empty = कोई एक्सटेंशन इंस्टॉल नहीं है
extensions-no-match = कोई मिलता-जुलता एक्सटेंशन नहीं
extensions-empty-detail = ऊपर Chrome Web Store में खोजें और Return दबाएँ।
extensions-no-match-detail = कोई दूसरा नाम या एक्सटेंशन ID आज़माएँ।
extensions-on = चालू
extensions-off = बंद
extensions-enable-confirm = { $name } सक्षम करें?
extensions-enable-permissions = { $name } सक्षम करें और अनुमति दें:

lsp-title = भाषा सर्वर
lsp-search = भाषा सर्वर, लिंटर, फ़ॉर्मैटर खोजें…
lsp-loading = कैटलॉग लोड हो रहा है…
lsp-empty = कोई मिलता-जुलता भाषा सर्वर नहीं
lsp-empty-detail = कोई दूसरी भाषा, लिंटर या फ़ॉर्मैटर आज़माएँ।
lsp-needs = { $tool } चाहिए
lsp-status-available = उपलब्ध
lsp-status-on-path = PATH में है
lsp-status-installing = इंस्टॉल हो रहा है…
lsp-status-installed = इंस्टॉल हो चुका
lsp-status-outdated = अपडेट उपलब्ध है
lsp-status-running = चल रहा है
lsp-status-failed = विफल

spaces-title = स्पेस
spaces-new-placeholder = नए स्पेस का नाम
spaces-empty = कोई स्पेस नहीं
spaces-default-name = स्पेस { $number }
spaces-tabs = { $count ->
    [one] 1 टैब
   *[other] { $count } टैब
}
spaces-delete = स्पेस हटाएँ

team-title = टीम
team-just-you = इस स्पेस में अभी सिर्फ़ आप हैं
team-agents = { $count ->
    [one] आप और 1 एजेंट
   *[other] आप और { $count } एजेंट
}
team-empty = यहाँ अभी कोई नहीं है
team-you = आप
team-agent = एजेंट

services-title = बैकग्राउंड सेवाएँ
services-processes = { $count ->
    [one] 1 प्रक्रिया
   *[other] { $count } प्रक्रियाएँ
}
services-kill-all = सभी बंद करें
services-not-running = सेवा नहीं चल रही है
services-start-with = इससे शुरू करें:
services-empty = कोई सक्रिय प्रक्रिया नहीं
services-filter = प्रक्रियाएँ फ़िल्टर करें…
services-no-match = कोई मिलती-जुलती प्रक्रिया नहीं
services-connected = कनेक्टेड
services-disconnected = डिस्कनेक्टेड
services-attached = अटैच्ड
services-kill = बंद करें
services-memory = मेमोरी
services-size = आकार
services-shell = शेल

error-title = त्रुटि

history-search = इतिहास खोजें
history-clear-all = सब साफ़ करें
history-clear-confirm = पूरा इतिहास साफ़ करें?
history-clear-warning = इसे वापस नहीं किया जा सकता।
history-cancel = रद्द करें
history-today = आज
history-yesterday = कल
history-days-ago = { $count } दिन पहले
history-day-offset = दिन -{ $count }

settings-title = सेटिंग्स
settings-loading = सेटिंग्स लोड हो रही हैं…
settings-stored = ~/.vmux/settings.ron में सेव
settings-other = अन्य
settings-software-update = सॉफ़्टवेयर अपडेट
settings-check-updates = अपडेट जाँचें
settings-check-updates-hint = ऑटो-अपडेट सक्षम होने पर लॉन्च पर और हर घंटे अपने-आप जाँचता है।
settings-update-unavailable = उपलब्ध नहीं
settings-update-unavailable-hint = इस बिल्ड में अपडेटर शामिल नहीं है।
settings-update-checking = जाँच हो रही है…
settings-update-checking-hint = अपडेट जाँचे जा रहे हैं…
settings-update-check-again = फिर जाँचें
settings-update-current = Vmux अप टू डेट है।
settings-update-downloading = डाउनलोड हो रहा है…
settings-update-downloading-hint = Vmux { $version } डाउनलोड हो रहा है…
settings-update-installing = इंस्टॉल हो रहा है…
settings-update-installing-hint = Vmux { $version } इंस्टॉल हो रहा है…
settings-update-ready = अपडेट तैयार है
settings-update-ready-hint = Vmux { $version } तैयार है। लागू करने के लिए रीस्टार्ट करें।
settings-update-try-again = फिर कोशिश करें
settings-update-failed = अपडेट की जाँच नहीं हो सकी।
settings-item = आइटम
settings-item-number = आइटम { $number }
settings-press-key = कोई कुंजी दबाएँ…
settings-saved = सेव हो गया
settings-record-key = नया कुंजी संयोजन रिकॉर्ड करने के लिए क्लिक करें

tray-open-window = विंडो खोलें
tray-close-window = विंडो बंद करें
tray-pause-recording = रिकॉर्डिंग रोकें
tray-resume-recording = रिकॉर्डिंग फिर शुरू करें
tray-finish-recording = रिकॉर्डिंग समाप्त करें
tray-quit = Vmux बंद करें

composer-attach-files = फ़ाइलें संलग्न करें (/upload)
composer-remove-attachment = संलग्न फ़ाइल हटाएँ

layout-back = पीछे
layout-forward = आगे
layout-reload = फिर लोड करें
layout-bookmark-page = इस पेज को बुकमार्क करें
layout-remove-bookmark = बुकमार्क हटाएँ
layout-pin-page = इस पेज को पिन करें
layout-unpin-page = इस पेज को अनपिन करें
layout-manage-extensions = एक्सटेंशन प्रबंधित करें
layout-new-stack = नया स्टैक
layout-close-tab = टैब बंद करें
layout-bookmark = बुकमार्क
layout-pin = पिन करें
layout-new-tab = नया टैब
layout-team = टीम

command-switch-space = स्पेस बदलें…
command-search-ask = खोजें या पूछें…
command-new-tab-placeholder = खोजें या URL टाइप करें, या Terminal चुनें…
command-placeholder = URL टाइप करें, टैब खोजें, या कमांड के लिए > लिखें…
command-composer-placeholder = कमांड के लिए / या मीडिया के लिए @ लिखें
command-send = भेजें (Enter)
command-terminal = टर्मिनल
command-open-terminal = टर्मिनल में खोलें
command-stack = स्टैक
command-tabs = { $count ->
    [one] 1 टैब
   *[other] { $count } टैब
}
command-prompt = प्रॉम्प्ट
command-new-tab = नया टैब
command-search = खोजें
command-open-value = “{ $value }” खोलें
command-search-value = “{ $value }” खोजें

schema-appearance = रूप-रंग
schema-general = सामान्य
schema-layout = लेआउट
schema-layout-detail = विंडो, पेन, साइडबार और फ़ोकस रिंग।
schema-agent = एजेंट
schema-agent-detail = एजेंट का व्यवहार और टूल अनुमतियाँ।
schema-shortcuts = शॉर्टकट
schema-shortcuts-detail = केवल देखने के लिए। बाइंडिंग बदलने के लिए सीधे settings.ron संपादित करें।
schema-terminal = टर्मिनल
schema-browser = ब्राउज़र
schema-mode = मोड
schema-mode-detail = वेब पेजों के लिए रंग योजना। Device आपके सिस्टम का अनुसरण करता है।
schema-device = डिवाइस
schema-light = हल्का
schema-dark = गहरा
schema-language = भाषा
schema-language-detail = सिस्टम, en-US, ja, या मेल खाते ~/.vmux/locales/<tag>.ftl कैटलॉग वाला कोई भी BCP 47 टैग इस्तेमाल करें।
schema-auto-update = ऑटो-अपडेट
schema-auto-update-detail = लॉन्च पर और हर घंटे अपडेट जाँचें और इंस्टॉल करें।
schema-startup-url = स्टार्टअप URL
schema-startup-url-detail = खाली रखने पर कमांड बार प्रॉम्प्ट खुलता है।
schema-search-engine = सर्च इंजन
schema-search-engine-detail = Start और कमांड बार से वेब खोजों के लिए इस्तेमाल होता है।
schema-window = विंडो
schema-pane = पेन
schema-side-sheet = साइड शीट
schema-focus-ring = फ़ोकस रिंग
schema-run-placement = रन प्लेसमेंट ओवरराइड की अनुमति दें
schema-run-placement-detail = एजेंट को रन पेन मोड, दिशा और एंकर चुनने दें।
schema-leader = लीडर
schema-leader-detail = कॉर्ड शॉर्टकट के लिए प्रीफ़िक्स कुंजी।
schema-chord-timeout = कॉर्ड टाइमआउट
schema-chord-timeout-detail = कॉर्ड प्रीफ़िक्स समाप्त होने से पहले मिलीसेकंड।
schema-bindings = बाइंडिंग
schema-confirm-close = बंद करने की पुष्टि
schema-confirm-close-detail = चल रही प्रक्रिया वाले टर्मिनल को बंद करने से पहले पूछें।
schema-default-theme = डिफ़ॉल्ट थीम
schema-default-theme-detail = थीम सूची से सक्रिय थीम का नाम।
