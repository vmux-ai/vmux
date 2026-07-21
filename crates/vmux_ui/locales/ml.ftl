common-open = തുറക്കുക
common-close = അടയ്ക്കുക
common-install = ഇൻസ്റ്റാൾ ചെയ്യുക
common-uninstall = അൺഇൻസ്റ്റാൾ ചെയ്യുക
common-update = അപ്ഡേറ്റ്
common-retry = വീണ്ടും ശ്രമിക്കുക
common-refresh = പുതുക്കുക
common-remove = നീക്കം ചെയ്യുക
common-enable = പ്രവർത്തനക്ഷമമാക്കുക
common-disable = പ്രവർത്തനരഹിതമാക്കുക
common-new = പുതിയത്
common-active = സജീവമാണ്
common-running = ഓടുന്നു
common-done = ചെയ്തു
common-failed = പരാജയപ്പെട്ടു
common-installed = ഇൻസ്റ്റാൾ ചെയ്തു
common-items = { $count ->
    [one] { $count } ഇനം
   *[other] { $count } ഇനങ്ങൾ
}
start-title = ആരംഭിക്കുക
start-tagline = ഒരു പ്രോംപ്റ്റ്. എന്തും, ചെയ്തു.

agents-title = ഏജൻ്റുമാർ
agents-search = ACP ഉം CLI ഏജൻ്റുമാരെയും തിരയുക...
agents-empty = പൊരുത്തപ്പെടുന്ന ഏജൻ്റുമാരില്ല
agents-empty-detail = ഒരു പേര്, റൺടൈം അല്ലെങ്കിൽ ACP/CLI പരീക്ഷിക്കുക.
agents-install-failed = ഇൻസ്റ്റാൾ ചെയ്യാനായില്ല
agents-updating = അപ്ഡേറ്റ് ചെയ്യുന്നു...
agents-retrying = വീണ്ടും ശ്രമിക്കുന്നു...
agents-preparing = തയ്യാറെടുക്കുന്നു...

extensions-title = വിപുലീകരണങ്ങൾ
extensions-search = തിരയൽ ഇൻസ്റ്റാൾ ചെയ്തു അല്ലെങ്കിൽ Chrome Web Store...
extensions-relaunch = അപേക്ഷിക്കാൻ വീണ്ടും സമാരംഭിക്കുക
extensions-empty = വിപുലീകരണങ്ങളൊന്നും ഇൻസ്റ്റാൾ ചെയ്തിട്ടില്ല
extensions-no-match = പൊരുത്തപ്പെടുന്ന വിപുലീകരണങ്ങളൊന്നുമില്ല
extensions-empty-detail = മുകളിലുള്ള Chrome Web Store തിരയുക, Return അമർത്തുക.
extensions-no-match-detail = മറ്റൊരു പേരോ വിപുലീകരണ ഐഡിയോ പരീക്ഷിക്കുക.
extensions-on = ഓൺ
extensions-off = ഓഫ്
extensions-enable-confirm = { $name } പ്രവർത്തനക്ഷമമാക്കണോ?
extensions-enable-permissions = { $name } പ്രവർത്തനക്ഷമമാക്കി അനുവദിക്കുക:

lsp-title = ഭാഷാ സെർവറുകൾ
lsp-search = ഭാഷാ സെർവറുകൾ, ലിൻ്ററുകൾ, ഫോർമാറ്ററുകൾ എന്നിവ തിരയുക...
lsp-loading = കാറ്റലോഗ് ലോഡുചെയ്യുന്നു...
lsp-empty = പൊരുത്തപ്പെടുന്ന ഭാഷാ സെർവറുകളൊന്നുമില്ല
lsp-empty-detail = മറ്റൊരു ഭാഷയോ ലിൻ്ററോ ഫോർമാറ്ററോ പരീക്ഷിക്കുക.
lsp-needs = { $tool } ആവശ്യമാണ്
lsp-status-available = ലഭ്യമാണ്
lsp-status-on-path = PATH-ന്
lsp-status-installing = ഇൻസ്റ്റാൾ ചെയ്യുന്നു...
lsp-status-installed = ഇൻസ്റ്റാൾ ചെയ്തു
lsp-status-outdated = അപ്ഡേറ്റ് ലഭ്യമാണ്
lsp-status-running = ഓടുന്നു
lsp-status-failed = പരാജയപ്പെട്ടു

spaces-title = ഇടങ്ങൾ
spaces-new-placeholder = പുതിയ ബഹിരാകാശ നാമം
spaces-empty = ഇടങ്ങളൊന്നുമില്ല
spaces-default-name = സ്പേസ് { $number }
spaces-tabs = { $count ->
    [one] 1 ടാബ്
   *[other] { $count } ടാബുകൾ
}
spaces-delete = ഇടം ഇല്ലാതാക്കുക

team-title = ടീം
team-just-you = ഈ സ്ഥലത്ത് നിങ്ങൾ മാത്രം
team-agents = { $count ->
    [one] നിങ്ങളും 1 ഏജൻ്റും
   *[other] നിങ്ങളും { $count } ഏജൻ്റുമാരും
}
team-empty = ഇവിടെ ഇതുവരെ ആരുമില്ല
team-you = നിങ്ങൾ
team-agent = ഏജൻ്റ്

services-title = പശ്ചാത്തല സേവനങ്ങൾ
services-processes = { $count ->
    [one] 1 പ്രക്രിയ
   *[other] { $count } പ്രക്രിയകൾ
}
services-kill-all = എല്ലാവരെയും കൊല്ലുക
services-not-running = സർവീസ് നടക്കുന്നില്ല
services-start-with = ആരംഭിക്കുക:
services-empty = സജീവമായ പ്രക്രിയകളൊന്നുമില്ല
services-filter = ഫിൽട്ടർ പ്രക്രിയകൾ...
services-no-match = പൊരുത്തപ്പെടുന്ന പ്രക്രിയകളൊന്നുമില്ല
services-connected = ബന്ധിപ്പിച്ചു
services-disconnected = വിച്ഛേദിച്ചു
services-attached = ഘടിപ്പിച്ചിരിക്കുന്നു
services-kill = കൊല്ലുക
services-memory = മെമ്മറി
services-size = വലിപ്പം
services-shell = ഷെൽ

error-title = പിശക്

history-search = തിരയൽ ചരിത്രം
history-clear-all = എല്ലാം മായ്‌ക്കുക
history-clear-confirm = എല്ലാ ചരിത്രവും മായ്‌ക്കണോ?
history-clear-warning = ഇത് പഴയപടിയാക്കാനാകില്ല.
history-cancel = റദ്ദാക്കുക
history-today = ഇന്ന്
history-yesterday = ഇന്നലെ
history-days-ago = { $count } ദിവസം മുമ്പ്
history-day-offset = ദിവസം -{ $count }

settings-title = ക്രമീകരണങ്ങൾ
settings-loading = ക്രമീകരണങ്ങൾ ലോഡുചെയ്യുന്നു...
settings-stored = ~/.vmux/settings.ron എന്നതിൽ സംഭരിച്ചു
settings-other = മറ്റുള്ളവ
settings-software-update = സോഫ്റ്റ്വെയർ അപ്ഡേറ്റ്
settings-check-updates = അപ്‌ഡേറ്റുകൾക്കായി പരിശോധിക്കുക
settings-check-updates-hint = സമാരംഭിക്കുമ്പോഴും ഓരോ മണിക്കൂറിലും സ്വയമേവ അപ്‌ഡേറ്റ് പ്രവർത്തനക്ഷമമാക്കുമ്പോൾ സ്വയമേവ പരിശോധിക്കുന്നു.
settings-update-unavailable = ലഭ്യമല്ല
settings-update-unavailable-hint = ഈ ബിൽഡിൽ അപ്ഡേറ്റർ ഉൾപ്പെടുത്തിയിട്ടില്ല.
settings-update-checking = പരിശോധിക്കുന്നു...
settings-update-checking-hint = അപ്‌ഡേറ്റുകൾക്കായി പരിശോധിക്കുന്നു...
settings-update-check-again = വീണ്ടും പരിശോധിക്കുക
settings-update-current = Vmux കാലികമാണ്.
settings-update-downloading = ഡൗൺലോഡ് ചെയ്യുന്നു...
settings-update-downloading-hint = ഡൗൺലോഡ് ചെയ്യുന്നു Vmux { $version }…
settings-update-installing = ഇൻസ്റ്റാൾ ചെയ്യുന്നു...
settings-update-installing-hint = Vmux { $version } ഇൻസ്റ്റാൾ ചെയ്യുന്നു…
settings-update-ready = അപ്‌ഡേറ്റ് തയ്യാറാണ്
settings-update-ready-hint = Vmux { $version } തയ്യാറാണ്. ഇത് പ്രയോഗിക്കാൻ പുനരാരംഭിക്കുക.
settings-update-try-again = വീണ്ടും ശ്രമിക്കുക
settings-update-failed = അപ്‌ഡേറ്റുകൾക്കായി പരിശോധിക്കാനായില്ല.
settings-item = ഇനം
settings-item-number = ഇനം { $number }
settings-press-key = ഒരു കീ അമർത്തുക...
settings-saved = സംരക്ഷിച്ചു
settings-record-key = ഒരു പുതിയ കീ കോംബോ റെക്കോർഡ് ചെയ്യാൻ ക്ലിക്ക് ചെയ്യുക

tray-open-window = വിൻഡോ തുറക്കുക
tray-close-window = വിൻഡോ അടയ്ക്കുക
tray-pause-recording = റെക്കോർഡിംഗ് താൽക്കാലികമായി നിർത്തുക
tray-resume-recording = റെക്കോർഡിംഗ് പുനരാരംഭിക്കുക
tray-finish-recording = റെക്കോർഡിംഗ് പൂർത്തിയാക്കുക
tray-quit = പുറത്തുകടക്കുക Vmux

composer-attach-files = ഫയലുകൾ അറ്റാച്ചുചെയ്യുക (/upload)
composer-remove-attachment = അറ്റാച്ച്മെൻ്റ് നീക്കം ചെയ്യുക

layout-back = തിരികെ
layout-forward = മുന്നോട്ട്
layout-reload = വീണ്ടും ലോഡുചെയ്യുക
layout-bookmark-page = ഈ പേജ് ബുക്ക്മാർക്ക് ചെയ്യുക
layout-remove-bookmark = ബുക്ക്മാർക്ക് നീക്കം ചെയ്യുക
layout-pin-page = ഈ പേജ് പിൻ ചെയ്യുക
layout-unpin-page = ഈ പേജ് അൺപിൻ ചെയ്യുക
layout-manage-extensions = വിപുലീകരണങ്ങൾ നിയന്ത്രിക്കുക
layout-new-stack = പുതിയ സ്റ്റാക്ക്
layout-close-tab = ടാബ് അടയ്ക്കുക
layout-bookmark = ബുക്ക്മാർക്ക്
layout-pin = പിൻ
layout-new-tab = പുതിയ ടാബ്
layout-team = ടീം

command-switch-space = ഇടം മാറുക...
command-search-ask = തിരയുക അല്ലെങ്കിൽ ചോദിക്കുക...
command-new-tab-placeholder = ഒരു URL തിരയുക അല്ലെങ്കിൽ ടൈപ്പ് ചെയ്യുക, അല്ലെങ്കിൽ ടെർമിനൽ തിരഞ്ഞെടുക്കുക...
command-placeholder = ഒരു URL ടൈപ്പ് ചെയ്യുക, ടാബുകൾ തിരയുക, അല്ലെങ്കിൽ > കമാൻഡുകൾക്കായി...
command-composer-placeholder = കമാൻഡുകൾക്കായി / അല്ലെങ്കിൽ മീഡിയയ്ക്ക് @ എന്ന് ടൈപ്പ് ചെയ്യുക
command-send = അയയ്‌ക്കുക (Enter)
command-terminal = ടെർമിനൽ
command-open-terminal = ടെർമിനലിൽ തുറക്കുക
command-stack = സ്റ്റാക്ക്
command-tabs = { $count ->
    [one] 1 ടാബ്
   *[other] { $count } ടാബുകൾ
}
command-prompt = പ്രോംപ്റ്റ്
command-new-tab = പുതിയ ടാബ്
command-search = തിരയുക
command-open-value = “{ $value }” തുറക്കുക
command-search-value = “{ $value }” തിരയുക

schema-appearance = രൂപഭാവം
schema-general = ജനറൽ
schema-layout = ലേഔട്ട്
schema-layout-detail = വിൻഡോ, പാളികൾ, സൈഡ്‌ബാർ, ഫോക്കസ് റിംഗ്.
schema-agent = ഏജൻ്റ്
schema-agent-detail = ഏജൻ്റ് പെരുമാറ്റവും ടൂൾ അനുമതികളും.
schema-shortcuts = കുറുക്കുവഴികൾ
schema-shortcuts-detail = വായന-മാത്രം കാഴ്ച. ബൈൻഡിംഗുകൾ മാറ്റാൻ settings.ron നേരിട്ട് എഡിറ്റ് ചെയ്യുക.
schema-terminal = ടെർമിനൽ
schema-browser = ബ്രൗസർ
schema-mode = മോഡ്
schema-mode-detail = വെബ് പേജുകൾക്കുള്ള വർണ്ണ സ്കീം. ഉപകരണം നിങ്ങളുടെ സിസ്റ്റത്തെ പിന്തുടരുന്നു.
schema-device = ഉപകരണം
schema-light = വെളിച്ചം
schema-dark = ഇരുട്ട്
schema-language = ഭാഷ
schema-language-detail = പൊരുത്തപ്പെടുന്ന ~/.vmux/locales/<tag>.ftl കാറ്റലോഗിനൊപ്പം സിസ്റ്റം, en-US, ja, അല്ലെങ്കിൽ ഏതെങ്കിലും BCP 47 ടാഗ് ഉപയോഗിക്കുക.
schema-auto-update = സ്വയമേവ അപ്ഡേറ്റ്
schema-auto-update-detail = ലോഞ്ചിംഗിലും ഓരോ മണിക്കൂറിലും അപ്‌ഡേറ്റുകൾ പരിശോധിക്കുകയും ഇൻസ്റ്റാൾ ചെയ്യുകയും ചെയ്യുക.
schema-startup-url = സ്റ്റാർട്ടപ്പ് URL
schema-startup-url-detail = ശൂന്യമായ കമാൻഡ് ബാർ പ്രോംപ്റ്റ് തുറക്കുന്നു.
schema-search-engine = തിരയൽ എഞ്ചിൻ
schema-search-engine-detail = ആരംഭത്തിൽ നിന്നും കമാൻഡ് ബാറിൽ നിന്നും വെബ് തിരയലുകൾക്കായി ഉപയോഗിക്കുന്നു.
schema-window = ജാലകം
schema-pane = പാളി
schema-side-sheet = സൈഡ് ഷീറ്റ്
schema-focus-ring = ഫോക്കസ് റിംഗ്
schema-run-placement = റൺ പ്ലേസ്മെൻ്റ് അസാധുവാക്കാൻ അനുവദിക്കുക
schema-run-placement-detail = റൺ പാളി മോഡ്, ദിശ, ആങ്കർ എന്നിവ തിരഞ്ഞെടുക്കാൻ ഏജൻ്റുമാരെ അനുവദിക്കുക.
schema-leader = നേതാവ്
schema-leader-detail = കോർഡ് കുറുക്കുവഴികൾക്കുള്ള പ്രിഫിക്സ് കീ.
schema-chord-timeout = കോർഡ് കാലഹരണപ്പെട്ടു
schema-chord-timeout-detail = ഒരു കോഡ് പ്രിഫിക്‌സ് കാലഹരണപ്പെടുന്നതിന് മുമ്പ് മില്ലിസെക്കൻഡ്.
schema-bindings = ബൈൻഡിംഗുകൾ
schema-confirm-close = അടുത്തതായി സ്ഥിരീകരിക്കുക
schema-confirm-close-detail = ഒരു റണ്ണിംഗ് പ്രോസസ് ഉപയോഗിച്ച് ഒരു ടെർമിനൽ അടയ്ക്കുന്നതിന് മുമ്പ് ആവശ്യപ്പെടുക.
schema-default-theme = ഡിഫോൾട്ട് തീം
schema-default-theme-detail = തീമുകളുടെ ലിസ്റ്റിൽ നിന്നുള്ള സജീവ തീമിൻ്റെ പേര്.
