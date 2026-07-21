common-open = उघडा
common-close = बंद करा
common-install = इंस्टॉल करा
common-uninstall = अनइंस्टॉल करा
common-update = अपडेट करा
common-retry = पुन्हा प्रयत्न करा
common-refresh = रिफ्रेश करा
common-remove = काढा
common-enable = सुरू करा
common-disable = बंद करा
common-new = नवीन
common-active = सक्रिय
common-running = चालू
common-done = पूर्ण
common-failed = अयशस्वी
common-installed = इंस्टॉल केलेले
common-items = { $count ->
    [one] { $count } आयटम
   *[other] { $count } आयटम
}
start-title = सुरुवात
start-tagline = एक प्रॉम्प्ट. काहीही काम पूर्ण.

agents-title = एजंट
agents-search = ACP आणि CLI एजंट शोधा…
agents-empty = जुळणारे एजंट नाहीत
agents-empty-detail = नाव, रनटाइम किंवा ACP/CLI वापरून पाहा.
agents-install-failed = इंस्टॉल अयशस्वी
agents-updating = अपडेट करत आहे…
agents-retrying = पुन्हा प्रयत्न करत आहे…
agents-preparing = तयार करत आहे…

extensions-title = एक्स्टेंशन्स
extensions-search = इंस्टॉल केलेले किंवा Chrome Web Store शोधा…
extensions-relaunch = लागू करण्यासाठी पुन्हा सुरू करा
extensions-empty = कोणतेही एक्स्टेंशन इंस्टॉल केलेले नाही
extensions-no-match = जुळणारे एक्स्टेंशन नाही
extensions-empty-detail = वर Chrome Web Store मध्ये शोधा आणि Return दाबा.
extensions-no-match-detail = दुसरे नाव किंवा एक्स्टेंशन ID वापरून पाहा.
extensions-on = सुरू
extensions-off = बंद
extensions-enable-confirm = { $name } सुरू करायचे?
extensions-enable-permissions = { $name } सुरू करून परवानगी द्या:

lsp-title = भाषा सर्व्हर
lsp-search = भाषा सर्व्हर, लिंटर, फॉरमॅटर शोधा…
lsp-loading = कॅटलॉग लोड करत आहे…
lsp-empty = जुळणारे भाषा सर्व्हर नाहीत
lsp-empty-detail = दुसरी भाषा, लिंटर किंवा फॉरमॅटर वापरून पाहा.
lsp-needs = { $tool } आवश्यक
lsp-status-available = उपलब्ध
lsp-status-on-path = PATH वर आहे
lsp-status-installing = इंस्टॉल करत आहे…
lsp-status-installed = इंस्टॉल केलेले
lsp-status-outdated = अपडेट उपलब्ध
lsp-status-running = चालू
lsp-status-failed = अयशस्वी

spaces-title = वर्कस्पेस
spaces-new-placeholder = नवीन वर्कस्पेसचे नाव
spaces-empty = वर्कस्पेस नाहीत
spaces-default-name = वर्कस्पेस { $number }
spaces-tabs = { $count ->
    [one] 1 टॅब
   *[other] { $count } टॅब
}
spaces-delete = वर्कस्पेस हटवा

team-title = टीम
team-just-you = या वर्कस्पेसमध्ये फक्त तुम्ही आहात
team-agents = { $count ->
    [one] तुम्ही आणि 1 एजंट
   *[other] तुम्ही आणि { $count } एजंट
}
team-empty = अजून कोणीही येथे नाही
team-you = तुम्ही
team-agent = एजंट

services-title = पार्श्वभूमी सेवा
services-processes = { $count ->
    [one] 1 प्रोसेस
   *[other] { $count } प्रोसेस
}
services-kill-all = सर्व जबरदस्तीने बंद करा
services-not-running = सेवा चालू नाही
services-start-with = याने सुरू करा:
services-empty = सक्रिय प्रोसेस नाहीत
services-filter = प्रोसेस फिल्टर करा…
services-no-match = जुळणाऱ्या प्रोसेस नाहीत
services-connected = कनेक्ट केलेले
services-disconnected = डिस्कनेक्ट केलेले
services-attached = जोडलेले
services-kill = जबरदस्तीने बंद करा
services-memory = मेमरी
services-size = आकार
services-shell = शेल

error-title = त्रुटी

history-search = इतिहास शोधा
history-clear-all = सर्व साफ करा
history-clear-confirm = सर्व इतिहास साफ करायचा?
history-clear-warning = हे पूर्ववत करता येणार नाही.
history-cancel = रद्द करा
history-today = आज
history-yesterday = काल
history-days-ago = { $count } दिवसांपूर्वी
history-day-offset = दिवस -{ $count }

settings-title = सेटिंग्ज
settings-loading = सेटिंग्ज लोड करत आहे…
settings-stored = ~/.vmux/settings.ron मध्ये साठवलेले
settings-other = इतर
settings-software-update = सॉफ्टवेअर अपडेट
settings-check-updates = अपडेट तपासा
settings-check-updates-hint = ऑटो-अपडेट सुरू असल्यास, सुरू होताना आणि दर तासाला आपोआप तपासते.
settings-update-unavailable = उपलब्ध नाही
settings-update-unavailable-hint = या बिल्डमध्ये अपडेटर समाविष्ट नाही.
settings-update-checking = तपासत आहे…
settings-update-checking-hint = अपडेट तपासत आहे…
settings-update-check-again = पुन्हा तपासा
settings-update-current = Vmux अद्ययावत आहे.
settings-update-downloading = डाउनलोड करत आहे…
settings-update-downloading-hint = Vmux { $version } डाउनलोड करत आहे…
settings-update-installing = इंस्टॉल करत आहे…
settings-update-installing-hint = Vmux { $version } इंस्टॉल करत आहे…
settings-update-ready = अपडेट तयार
settings-update-ready-hint = Vmux { $version } तयार आहे. लागू करण्यासाठी रीस्टार्ट करा.
settings-update-try-again = पुन्हा प्रयत्न करा
settings-update-failed = अपडेट तपासता आले नाहीत.
settings-item = आयटम
settings-item-number = आयटम { $number }
settings-press-key = एखादी की दाबा…
settings-saved = सेव्ह केले
settings-record-key = नवीन की कॉम्बो रेकॉर्ड करण्यासाठी क्लिक करा

tray-open-window = विंडो उघडा
tray-close-window = विंडो बंद करा
tray-pause-recording = रेकॉर्डिंग थांबवा
tray-resume-recording = रेकॉर्डिंग पुन्हा सुरू करा
tray-finish-recording = रेकॉर्डिंग पूर्ण करा
tray-quit = Vmux बंद करा

composer-attach-files = फाइल्स जोडा (/upload)
composer-remove-attachment = जोडलेली फाइल काढा

layout-back = मागे
layout-forward = पुढे
layout-reload = रीलोड करा
layout-bookmark-page = हे पान बुकमार्क करा
layout-remove-bookmark = बुकमार्क काढा
layout-pin-page = हे पान पिन करा
layout-unpin-page = या पानचे पिन काढा
layout-manage-extensions = एक्स्टेंशन्स व्यवस्थापित करा
layout-new-stack = नवीन स्टॅक
layout-close-tab = टॅब बंद करा
layout-bookmark = बुकमार्क
layout-pin = पिन करा
layout-new-tab = नवीन टॅब
layout-team = टीम

command-switch-space = वर्कस्पेस बदला…
command-search-ask = शोधा किंवा विचारा…
command-new-tab-placeholder = शोधा किंवा URL टाइप करा, किंवा Terminal निवडा…
command-placeholder = URL टाइप करा, टॅब शोधा, किंवा आदेशांसाठी > वापरा…
command-composer-placeholder = आदेशांसाठी / किंवा मीडियासाठी @ टाइप करा
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

schema-appearance = रूप
schema-general = सामान्य
schema-layout = लेआउट
schema-layout-detail = विंडो, पेन, साइडबार आणि फोकस रिंग.
schema-agent = एजंट
schema-agent-detail = एजंटचे वर्तन आणि टूल परवानग्या.
schema-shortcuts = शॉर्टकट्स
schema-shortcuts-detail = फक्त वाचण्यासाठी दृश्य. बाइंडिंग्ज बदलण्यासाठी settings.ron थेट संपादित करा.
schema-terminal = टर्मिनल
schema-browser = ब्राउझर
schema-mode = मोड
schema-mode-detail = वेब पानांसाठी रंग योजना. Device तुमच्या सिस्टमप्रमाणे चालते.
schema-device = Device
schema-light = हलका
schema-dark = गडद
schema-language = भाषा
schema-language-detail = system, en-US, ja, किंवा जुळणाऱ्या ~/.vmux/locales/<tag>.ftl कॅटलॉगसह कोणताही BCP 47 टॅग वापरा.
schema-auto-update = ऑटो-अपडेट
schema-auto-update-detail = सुरू होताना आणि दर तासाला अपडेट तपासा आणि इंस्टॉल करा.
schema-startup-url = स्टार्टअप URL
schema-startup-url-detail = रिकामे ठेवल्यास कमांड बार प्रॉम्प्ट उघडतो.
schema-search-engine = शोध इंजिन
schema-search-engine-detail = Start आणि कमांड बारमधून वेब शोधांसाठी वापरले जाते.
schema-window = विंडो
schema-pane = पेन
schema-side-sheet = साइड शीट
schema-focus-ring = फोकस रिंग
schema-run-placement = रन प्लेसमेंट ओव्हरराइडला परवानगी द्या
schema-run-placement-detail = एजंटना रन पेन मोड, दिशा आणि अँकर निवडू द्या.
schema-leader = लीडर
schema-leader-detail = कॉर्ड शॉर्टकट्ससाठी प्रीफिक्स की.
schema-chord-timeout = कॉर्ड टाइमआउट
schema-chord-timeout-detail = कॉर्ड प्रीफिक्स कालबाह्य होण्यापूर्वीचे मिलिसेकंद.
schema-bindings = बाइंडिंग्ज
schema-confirm-close = बंद करताना पुष्टी करा
schema-confirm-close-detail = चालू प्रोसेस असलेले टर्मिनल बंद करण्यापूर्वी विचारा.
schema-default-theme = डीफॉल्ट थीम
schema-default-theme-detail = थीम्स यादीतील सक्रिय थीमचे नाव.
