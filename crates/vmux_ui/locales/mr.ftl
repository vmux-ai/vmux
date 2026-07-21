common-open = उघडा
common-close = बंद करा
common-install = स्थापित करा
common-uninstall = विस्थापित करा
common-update = अपडेट करा
common-retry = पुन्हा प्रयत्न करा
common-refresh = रिफ्रेश करा
common-remove = काढा
common-enable = सक्षम करा
common-disable = अक्षम करा
common-new = नवीन
common-active = सक्रिय
common-running = धावणे
common-done = पूर्ण
common-failed = अयशस्वी
common-installed = स्थापित केले
common-items = { $count ->
    [one] { $count } आयटम
   *[other] { $count } आयटम
}
start-title = सुरू करा
start-tagline = एक प्रॉम्प्ट. काहीही झाले.

agents-title = एजंट
agents-search = ACP आणि CLI एजंट शोधा…
agents-empty = कोणतेही जुळणारे एजंट नाहीत
agents-empty-detail = नाव, रनटाइम किंवा ACP/CLI वापरून पहा.
agents-install-failed = स्थापना अयशस्वी
agents-updating = अपडेट करत आहे...
agents-retrying = पुन्हा प्रयत्न करत आहे...
agents-preparing = तयारी करत आहे...

extensions-title = विस्तार
extensions-search = स्थापित शोधा किंवा Chrome Web Store…
extensions-relaunch = अर्ज करण्यासाठी पुन्हा लाँच करा
extensions-empty = कोणतेही विस्तार स्थापित नाहीत
extensions-no-match = कोणतेही जुळणारे विस्तार नाहीत
extensions-empty-detail = वरील Chrome Web Store शोधा आणि Return दाबा.
extensions-no-match-detail = दुसरे नाव किंवा एक्स्टेंशन आयडी वापरून पहा.
extensions-on = चालू
extensions-off = बंद
extensions-enable-confirm = { $name } सक्षम करायचे?
extensions-enable-permissions = { $name } सक्षम करा आणि अनुमती द्या:

lsp-title = भाषा सर्व्हर
lsp-search = भाषा सर्व्हर, लिंटर्स, स्वरूपन शोधा…
lsp-loading = कॅटलॉग लोड करत आहे...
lsp-empty = कोणतेही जुळणारे भाषा सर्व्हर नाहीत
lsp-empty-detail = दुसरी भाषा, लिंटर किंवा फॉरमॅटर वापरून पहा.
lsp-needs = गरज आहे { $tool }
lsp-status-available = उपलब्ध
lsp-status-on-path = PATH रोजी
lsp-status-installing = स्थापित करत आहे...
lsp-status-installed = स्थापित केले
lsp-status-outdated = अपडेट उपलब्ध
lsp-status-running = धावत आहे
lsp-status-failed = अयशस्वी

spaces-title = मोकळी जागा
spaces-new-placeholder = नवीन जागेचे नाव
spaces-empty = रिक्त जागा नाहीत
spaces-default-name = जागा { $number }
spaces-tabs = { $count ->
    [one] 1 टॅब
   *[other] { $count } टॅब
}
spaces-delete = जागा हटवा

team-title = संघ
team-just-you = या जागेत फक्त तू
team-agents = { $count ->
    [one] तुम्ही आणि 1 एजंट
   *[other] तुम्ही आणि { $count } एजंट
}
team-empty = इथे अजून कोणी नाही
team-you = आपण
team-agent = एजंट

services-title = पार्श्वभूमी सेवा
services-processes = { $count ->
    [one] 1 प्रक्रिया
   *[other] { $count } प्रक्रिया
}
services-kill-all = सर्व मारुन टाका
services-not-running = सेवा चालू नाही
services-start-with = यासह प्रारंभ करा:
services-empty = सक्रिय प्रक्रिया नाहीत
services-filter = फिल्टर प्रक्रिया...
services-no-match = कोणतीही जुळणारी प्रक्रिया नाही
services-connected = जोडलेले
services-disconnected = डिस्कनेक्ट केले
services-attached = संलग्न
services-kill = मारणे
services-memory = स्मृती
services-size = आकार
services-shell = शेल

error-title = त्रुटी

history-search = शोध इतिहास
history-clear-all = सर्व साफ करा
history-clear-confirm = सर्व इतिहास साफ करायचा?
history-clear-warning = हे पूर्ववत केले जाऊ शकत नाही.
history-cancel = रद्द करा
history-today = आज
history-yesterday = काल
history-days-ago = { $count } दिवसांपूर्वी
history-day-offset = दिवस -{ $count }

settings-title = सेटिंग्ज
settings-loading = सेटिंग्ज लोड करत आहे...
settings-stored = ~/.vmux/settings.ron मध्ये संग्रहित
settings-other = इतर
settings-software-update = सॉफ्टवेअर अपडेट
settings-check-updates = अद्यतनांसाठी तपासा
settings-check-updates-hint = लॉन्च झाल्यावर आणि ऑटो-अपडेट सक्षम असताना प्रत्येक तासाला स्वयंचलितपणे तपासले जाते.
settings-update-unavailable = अनुपलब्ध
settings-update-unavailable-hint = या बिल्डमध्ये अपडेटर समाविष्ट नाही.
settings-update-checking = तपासत आहे...
settings-update-checking-hint = अपडेट तपासत आहे...
settings-update-check-again = पुन्हा तपासा
settings-update-current = Vmux अद्ययावत आहे.
settings-update-downloading = डाउनलोड करत आहे...
settings-update-downloading-hint = डाउनलोड करत आहे Vmux { $version }…
settings-update-installing = स्थापित करत आहे...
settings-update-installing-hint = Vmux { $version } स्थापित करत आहे…
settings-update-ready = अपडेट तयार
settings-update-ready-hint = Vmux { $version } तयार आहे. ते लागू करण्यासाठी रीस्टार्ट करा.
settings-update-try-again = पुन्हा प्रयत्न करा
settings-update-failed = अपडेट तपासण्यात अक्षम.
settings-item = आयटम
settings-item-number = आयटम { $number }
settings-press-key = एक कळ दाबा...
settings-saved = जतन केले
settings-record-key = नवीन की कॉम्बो रेकॉर्ड करण्यासाठी क्लिक करा

tray-open-window = विंडो उघडा
tray-close-window = विंडो बंद करा
tray-pause-recording = रेकॉर्डिंगला विराम द्या
tray-resume-recording = रेकॉर्डिंग पुन्हा सुरू करा
tray-finish-recording = रेकॉर्डिंग पूर्ण करा
tray-quit = Vmux सोडा

composer-attach-files = फायली संलग्न करा (/upload)
composer-remove-attachment = संलग्नक काढा

layout-back = मागे
layout-forward = पुढे
layout-reload = रीलोड करा
layout-bookmark-page = हे पृष्ठ बुकमार्क करा
layout-remove-bookmark = बुकमार्क काढा
layout-pin-page = हे पृष्ठ पिन करा
layout-unpin-page = हे पृष्ठ अनपिन करा
layout-manage-extensions = विस्तार व्यवस्थापित करा
layout-new-stack = नवीन स्टॅक
layout-close-tab = टॅब बंद करा
layout-bookmark = बुकमार्क करा
layout-pin = पिन
layout-new-tab = नवीन टॅब
layout-team = संघ

command-switch-space = जागा बदला...
command-search-ask = शोधा किंवा विचारा...
command-new-tab-placeholder = URL शोधा किंवा टाइप करा किंवा टर्मिनल निवडा…
command-placeholder = URL टाइप करा, टॅब शोधा, किंवा > आदेशांसाठी...
command-composer-placeholder = कमांडसाठी / किंवा मीडियासाठी @ टाइप करा
command-send = पाठवा (Enter)
command-terminal = टर्मिनल
command-open-terminal = टर्मिनलमध्ये उघडा
command-stack = स्टॅक
command-tabs = { $count ->
    [one] 1 टॅब
   *[other] { $count } टॅब
}
command-prompt = प्रॉम्प्ट
command-new-tab = नवीन टॅब
command-search = शोधा
command-open-value = “{ $value }” उघडा
command-search-value = “{ $value }” शोधा

schema-appearance = देखावा
schema-general = सामान्य
schema-layout = मांडणी
schema-layout-detail = विंडो, पटल, साइडबार आणि फोकस रिंग.
schema-agent = एजंट
schema-agent-detail = एजंट वर्तन आणि साधन परवानग्या.
schema-shortcuts = शॉर्टकट
schema-shortcuts-detail = केवळ-वाचनीय दृश्य. बाइंडिंग बदलण्यासाठी थेट settings.ron संपादित करा.
schema-terminal = टर्मिनल
schema-browser = ब्राउझर
schema-mode = मोड
schema-mode-detail = वेब पृष्ठांसाठी रंग योजना. डिव्हाइस तुमच्या सिस्टमला फॉलो करते.
schema-device = साधन
schema-light = प्रकाश
schema-dark = गडद
schema-language = भाषा
schema-language-detail = सिस्टीम, en-US, ja, किंवा कोणताही BCP 47 टॅग जुळणाऱ्या ~/.vmux/locales/<tag>.ftl कॅटलॉगसह वापरा.
schema-auto-update = स्वयं-अद्यतन
schema-auto-update-detail = लाँच आणि दर तासाला अद्यतने तपासा आणि स्थापित करा.
schema-startup-url = स्टार्टअप URL
schema-startup-url-detail = रिक्त कमांड बार प्रॉम्प्ट उघडते.
schema-search-engine = शोध इंजिन
schema-search-engine-detail = स्टार्ट आणि कमांड बारमधून वेब शोधांसाठी वापरले जाते.
schema-window = खिडकी
schema-pane = फलक
schema-side-sheet = साइड शीट
schema-focus-ring = फोकस रिंग
schema-run-placement = रन प्लेसमेंट ओव्हरराइडला अनुमती द्या
schema-run-placement-detail = एजंटना रन पेन मोड, दिशा आणि अँकर निवडू द्या.
schema-leader = नेता
schema-leader-detail = जीवा शॉर्टकटसाठी उपसर्ग की.
schema-chord-timeout = जीवा कालबाह्य
schema-chord-timeout-detail = जीवा उपसर्ग कालबाह्य होण्यापूर्वी मिलीसेकंद.
schema-bindings = बांधणी
schema-confirm-close = बंद पुष्टी करा
schema-confirm-close-detail = चालू असलेल्या प्रक्रियेसह टर्मिनल बंद करण्यापूर्वी प्रॉम्प्ट करा.
schema-default-theme = डीफॉल्ट थीम
schema-default-theme-detail = थीम सूचीमधून सक्रिय थीमचे नाव.
