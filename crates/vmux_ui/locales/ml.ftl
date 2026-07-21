common-open = തുറക്കുക
common-close = അടയ്ക്കുക
common-install = ഇൻസ്റ്റാൾ ചെയ്യുക
common-uninstall = അൺഇൻസ്റ്റാൾ ചെയ്യുക
common-update = അപ്ഡേറ്റ് ചെയ്യുക
common-retry = വീണ്ടും ശ്രമിക്കുക
common-refresh = പുതുക്കുക
common-remove = നീക്കംചെയ്യുക
common-enable = പ്രവർത്തനക്ഷമമാക്കുക
common-disable = പ്രവർത്തനരഹിതമാക്കുക
common-new = പുതിയത്
common-active = സജീവം
common-running = പ്രവർത്തിക്കുന്നു
common-done = പൂർത്തിയായി
common-failed = പരാജയപ്പെട്ടു
common-installed = ഇൻസ്റ്റാൾ ചെയ്തു
common-items = { $count ->
    [one] { $count } ഇനം
   *[other] { $count } ഇനങ്ങൾ
}
start-title = തുടങ്ങുക
start-tagline = ഒരു prompt. എന്തും പൂർത്തിയാക്കാം.

agents-title = ഏജന്റുകൾ
agents-search = ACP, CLI ഏജന്റുകൾ തിരയുക…
agents-empty = പൊരുത്തപ്പെടുന്ന ഏജന്റുകളില്ല
agents-empty-detail = പേര്, റൺടൈം, അല്ലെങ്കിൽ ACP/CLI ഉപയോഗിച്ച് നോക്കൂ.
agents-install-failed = ഇൻസ്റ്റാൾ ചെയ്യാനായില്ല
agents-updating = അപ്ഡേറ്റ് ചെയ്യുന്നു…
agents-retrying = വീണ്ടും ശ്രമിക്കുന്നു…
agents-preparing = തയ്യാറാക്കുന്നു…

extensions-title = എക്സ്റ്റൻഷനുകൾ
extensions-search = ഇൻസ്റ്റാൾ ചെയ്തതോ Chrome Web Store-ലേതോ തിരയുക…
extensions-relaunch = പ്രയോഗിക്കാൻ വീണ്ടും തുറക്കുക
extensions-empty = എക്സ്റ്റൻഷനുകൾ ഇൻസ്റ്റാൾ ചെയ്തിട്ടില്ല
extensions-no-match = പൊരുത്തപ്പെടുന്ന എക്സ്റ്റൻഷനുകളില്ല
extensions-empty-detail = മുകളിൽ Chrome Web Store-ൽ തിരഞ്ഞ് Return അമർത്തുക.
extensions-no-match-detail = മറ്റൊരു പേരോ എക്സ്റ്റൻഷൻ ID-യോ ശ്രമിക്കുക.
extensions-on = ഓൺ
extensions-off = ഓഫ്
extensions-enable-confirm = { $name } പ്രവർത്തനക്ഷമമാക്കണോ?
extensions-enable-permissions = { $name } പ്രവർത്തനക്ഷമമാക്കി അനുവദിക്കുക:

lsp-title = ഭാഷാ സെർവറുകൾ
lsp-search = ഭാഷാ സെർവറുകൾ, ലിന്ററുകൾ, ഫോർമാറ്ററുകൾ തിരയുക…
lsp-loading = കാറ്റലോഗ് ലോഡ് ചെയ്യുന്നു…
lsp-empty = പൊരുത്തപ്പെടുന്ന ഭാഷാ സെർവറുകളില്ല
lsp-empty-detail = മറ്റൊരു ഭാഷ, ലിന്റർ, അല്ലെങ്കിൽ ഫോർമാറ്റർ ശ്രമിക്കുക.
lsp-needs = { $tool } ആവശ്യമുണ്ട്
lsp-status-available = ലഭ്യമാണ്
lsp-status-on-path = PATH-ൽ ഉണ്ട്
lsp-status-installing = ഇൻസ്റ്റാൾ ചെയ്യുന്നു…
lsp-status-installed = ഇൻസ്റ്റാൾ ചെയ്തു
lsp-status-outdated = അപ്ഡേറ്റ് ലഭ്യമാണ്
lsp-status-running = പ്രവർത്തിക്കുന്നു
lsp-status-failed = പരാജയപ്പെട്ടു

spaces-title = വർക്ക്‌സ്‌പേസുകൾ
spaces-new-placeholder = പുതിയ വർക്ക്‌സ്‌പേസിന്റെ പേര്
spaces-empty = വർക്ക്‌സ്‌പേസുകളില്ല
spaces-default-name = വർക്ക്‌സ്‌പേസ് { $number }
spaces-tabs = { $count ->
    [one] 1 ടാബ്
   *[other] { $count } ടാബുകൾ
}
spaces-delete = വർക്ക്‌സ്‌പേസ് ഇല്ലാതാക്കുക

team-title = ടീം
team-just-you = ഈ വർക്ക്‌സ്‌പേസിൽ നിങ്ങൾ മാത്രം
team-agents = { $count ->
    [one] നിങ്ങൾക്കും 1 ഏജന്റിനും
   *[other] നിങ്ങൾക്കും { $count } ഏജന്റുകൾക്കും
}
team-empty = ഇവിടെ ഇനിയും ആരുമില്ല
team-you = നിങ്ങൾ
team-agent = ഏജന്റ്

services-title = പശ്ചാത്തല സേവനങ്ങൾ
services-processes = { $count ->
    [one] 1 പ്രോസസ്
   *[other] { $count } പ്രോസസുകൾ
}
services-kill-all = എല്ലാം അവസാനിപ്പിക്കുക
services-not-running = സേവനം പ്രവർത്തിക്കുന്നില്ല
services-start-with = ഇതോടൊപ്പം തുടങ്ങുക:
services-empty = സജീവ പ്രോസസുകളില്ല
services-filter = പ്രോസസുകൾ ഫിൽട്ടർ ചെയ്യുക…
services-no-match = പൊരുത്തപ്പെടുന്ന പ്രോസസുകളില്ല
services-connected = കണക്റ്റ് ചെയ്തു
services-disconnected = വിച്ഛേദിച്ചു
services-attached = അറ്റാച്ച് ചെയ്തു
services-kill = അവസാനിപ്പിക്കുക
services-memory = മെമ്മറി
services-size = വലുപ്പം
services-shell = ഷെൽ

error-title = പിശക്

history-search = ചരിത്രം തിരയുക
history-clear-all = എല്ലാം മായ്ക്കുക
history-clear-confirm = മുഴുവൻ ചരിത്രവും മായ്ക്കണോ?
history-clear-warning = ഇത് പഴയപടിയാക്കാനാകില്ല.
history-cancel = റദ്ദാക്കുക
history-today = ഇന്ന്
history-yesterday = ഇന്നലെ
history-days-ago = { $count } ദിവസം മുമ്പ്
history-day-offset = ദിവസം -{ $count }

settings-title = ക്രമീകരണങ്ങൾ
settings-loading = ക്രമീകരണങ്ങൾ ലോഡ് ചെയ്യുന്നു…
settings-stored = ~/.vmux/settings.ron-ൽ സംഭരിച്ചു
settings-other = മറ്റ്
settings-software-update = സോഫ്റ്റ്‌വെയർ അപ്ഡേറ്റ്
settings-check-updates = അപ്ഡേറ്റുകൾ പരിശോധിക്കുക
settings-check-updates-hint = Auto-update പ്രവർത്തനക്ഷമമെങ്കിൽ ലോഞ്ച് ചെയ്യുമ്പോഴും ഓരോ മണിക്കൂറിലും സ്വയം പരിശോധിക്കും.
settings-update-unavailable = ലഭ്യമല്ല
settings-update-unavailable-hint = ഈ ബിൽഡിൽ അപ്ഡേറ്റർ ഉൾപ്പെടുത്തിയിട്ടില്ല.
settings-update-checking = പരിശോധിക്കുന്നു…
settings-update-checking-hint = അപ്ഡേറ്റുകൾ പരിശോധിക്കുന്നു…
settings-update-check-again = വീണ്ടും പരിശോധിക്കുക
settings-update-current = Vmux പുതുക്കിയ നിലയിലാണ്.
settings-update-downloading = ഡൗൺലോഡ് ചെയ്യുന്നു…
settings-update-downloading-hint = Vmux { $version } ഡൗൺലോഡ് ചെയ്യുന്നു…
settings-update-installing = ഇൻസ്റ്റാൾ ചെയ്യുന്നു…
settings-update-installing-hint = Vmux { $version } ഇൻസ്റ്റാൾ ചെയ്യുന്നു…
settings-update-ready = അപ്ഡേറ്റ് തയ്യാറായി
settings-update-ready-hint = Vmux { $version } തയ്യാറാണ്. പ്രയോഗിക്കാൻ വീണ്ടും ആരംഭിക്കുക.
settings-update-try-again = വീണ്ടും ശ്രമിക്കുക
settings-update-failed = അപ്ഡേറ്റുകൾ പരിശോധിക്കാനായില്ല.
settings-item = ഇനം
settings-item-number = ഇനം { $number }
settings-press-key = ഒരു കീ അമർത്തുക…
settings-saved = സംരക്ഷിച്ചു
settings-record-key = പുതിയ കീ കോമ്പോ രേഖപ്പെടുത്താൻ ക്ലിക്ക് ചെയ്യുക

tray-open-window = വിൻഡോ തുറക്കുക
tray-close-window = വിൻഡോ അടയ്ക്കുക
tray-pause-recording = റെക്കോർഡിംഗ് നിർത്തിവെക്കുക
tray-resume-recording = റെക്കോർഡിംഗ് പുനരാരംഭിക്കുക
tray-finish-recording = റെക്കോർഡിംഗ് പൂർത്തിയാക്കുക
tray-quit = Vmux വിട്ടുപോവുക

composer-attach-files = ഫയലുകൾ അറ്റാച്ച് ചെയ്യുക (/upload)
composer-remove-attachment = അറ്റാച്ച്മെന്റ് നീക്കംചെയ്യുക

layout-back = പിന്നോട്ട്
layout-forward = മുന്നോട്ട്
layout-reload = വീണ്ടും ലോഡ് ചെയ്യുക
layout-bookmark-page = ഈ പേജ് ബുക്ക്‌മാർക്ക് ചെയ്യുക
layout-remove-bookmark = ബുക്ക്‌മാർക്ക് നീക്കംചെയ്യുക
layout-pin-page = ഈ പേജ് പിൻ ചെയ്യുക
layout-unpin-page = ഈ പേജ് അൺപിൻ ചെയ്യുക
layout-manage-extensions = എക്സ്റ്റൻഷനുകൾ നിയന്ത്രിക്കുക
layout-new-stack = പുതിയ സ്റ്റാക്ക്
layout-close-tab = ടാബ് അടയ്ക്കുക
layout-bookmark = ബുക്ക്‌മാർക്ക്
layout-pin = പിൻ ചെയ്യുക
layout-new-tab = പുതിയ ടാബ്
layout-team = ടീം

command-switch-space = വർക്ക്‌സ്‌പേസ് മാറ്റുക…
command-search-ask = തിരയുക അല്ലെങ്കിൽ ചോദിക്കുക…
command-new-tab-placeholder = തിരയുക, URL ടൈപ്പ് ചെയ്യുക, അല്ലെങ്കിൽ Terminal തിരഞ്ഞെടുക്കുക…
command-placeholder = URL ടൈപ്പ് ചെയ്യുക, ടാബുകൾ തിരയുക, അല്ലെങ്കിൽ കമാൻഡുകൾക്കായി > നൽകുക…
command-composer-placeholder = കമാൻഡുകൾക്കായി / അല്ലെങ്കിൽ മീഡിയക്കായി @ ടൈപ്പ് ചെയ്യുക
command-send = അയയ്ക്കുക (Enter)
command-terminal = ടെർമിനൽ
command-open-terminal = ടെർമിനലിൽ തുറക്കുക
command-stack = സ്റ്റാക്ക്
command-tabs = { $count ->
    [one] 1 ടാബ്
   *[other] { $count } ടാബുകൾ
}
command-prompt = Prompt
command-new-tab = പുതിയ ടാബ്
command-search = തിരയുക
command-open-value = “{ $value }” തുറക്കുക
command-search-value = “{ $value }” തിരയുക

schema-appearance = രൂപഭാവം
schema-general = പൊതുവായത്
schema-layout = ലേഔട്ട്
schema-layout-detail = വിൻഡോ, പെയ്നുകൾ, സൈഡ്‌ബാർ, ഫോക്കസ് റിംഗ്.
schema-agent = ഏജന്റ്
schema-agent-detail = ഏജന്റിന്റെ പെരുമാറ്റവും ടൂൾ അനുമതികളും.
schema-shortcuts = കുറുക്കുവഴികൾ
schema-shortcuts-detail = വായന മാത്രം കാണുന്ന കാഴ്ച. ബൈൻഡിംഗുകൾ മാറ്റാൻ settings.ron നേരിട്ട് തിരുത്തുക.
schema-terminal = ടെർമിനൽ
schema-browser = ബ്രൗസർ
schema-mode = മോഡ്
schema-mode-detail = വെബ് പേജുകൾക്കുള്ള വർണ്ണ സ്കീം. Device സിസ്റ്റത്തെ പിന്തുടരും.
schema-device = Device
schema-light = ലൈറ്റ്
schema-dark = ഡാർക്ക്
schema-language = ഭാഷ
schema-language-detail = സിസ്റ്റം, en-US, ja, അല്ലെങ്കിൽ പൊരുത്തപ്പെടുന്ന ~/.vmux/locales/<tag>.ftl കാറ്റലോഗുള്ള ഏതെങ്കിലും BCP 47 ടാഗ് ഉപയോഗിക്കുക.
schema-auto-update = Auto-update
schema-auto-update-detail = ലോഞ്ച് ചെയ്യുമ്പോഴും ഓരോ മണിക്കൂറിലും അപ്ഡേറ്റുകൾ പരിശോധിച്ച് ഇൻസ്റ്റാൾ ചെയ്യുക.
schema-startup-url = സ്റ്റാർട്ടപ്പ് URL
schema-startup-url-detail = കാലിയായി വെച്ചാൽ കമാൻഡ് ബാർ prompt തുറക്കും.
schema-search-engine = തിരയൽ എഞ്ചിൻ
schema-search-engine-detail = Start-ലും കമാൻഡ് ബാറിലും നിന്ന് വെബ് തിരയലുകൾക്ക് ഉപയോഗിക്കുന്നു.
schema-window = വിൻഡോ
schema-pane = പെയിൻ
schema-side-sheet = സൈഡ് ഷീറ്റ്
schema-focus-ring = ഫോക്കസ് റിംഗ്
schema-run-placement = റൺ പ്ലേസ്മെന്റ് ഓവർറൈഡ് അനുവദിക്കുക
schema-run-placement-detail = റൺ പെയിൻ മോഡ്, ദിശ, ആങ്കർ എന്നിവ ഏജന്റുകൾക്ക് തിരഞ്ഞെടുക്കാൻ അനുവദിക്കുക.
schema-leader = ലീഡർ
schema-leader-detail = chord കുറുക്കുവഴികൾക്കുള്ള പ്രിഫിക്സ് കീ.
schema-chord-timeout = chord സമയം അവസാനിക്കൽ
schema-chord-timeout-detail = chord പ്രിഫിക്സ് കാലഹരണപ്പെടുന്നതിന് മുമ്പുള്ള മില്ലിസെക്കൻഡുകൾ.
schema-bindings = ബൈൻഡിംഗുകൾ
schema-confirm-close = അടയ്ക്കുന്നതിന് മുമ്പ് സ്ഥിരീകരിക്കുക
schema-confirm-close-detail = പ്രവർത്തിക്കുന്ന പ്രോസസുള്ള ടെർമിനൽ അടയ്ക്കുന്നതിന് മുമ്പ് ചോദിക്കുക.
schema-default-theme = ഡിഫോൾട്ട് തീം
schema-default-theme-detail = തീം പട്ടികയിലെ സജീവ തീമിന്റെ പേര്.
