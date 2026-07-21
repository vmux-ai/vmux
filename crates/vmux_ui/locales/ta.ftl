common-open = திற
common-close = மூடு
common-install = நிறுவு
common-uninstall = நீக்கு
common-update = புதுப்பி
common-retry = மீண்டும் முயற்சி
common-refresh = புதுப்பி
common-remove = அகற்று
common-enable = இயக்கு
common-disable = முடக்கு
common-new = புதியது
common-active = செயலில்
common-running = இயங்குகிறது
common-done = முடிந்தது
common-failed = தோல்வி
common-installed = நிறுவப்பட்டது
common-items = { $count ->
    [one] { $count } உருப்படி
   *[other] { $count } உருப்படிகள்
}
start-title = தொடக்கம்
start-tagline = ஒரு வினவல். எதுவும், முடிந்தது.

agents-title = முகவர்கள்
agents-search = ACP மற்றும் CLI முகவர்களைத் தேடு…
agents-empty = பொருந்தும் முகவர்கள் இல்லை
agents-empty-detail = பெயர், இயக்க சூழல் அல்லது ACP/CLI முயற்சிக்கவும்.
agents-install-failed = நிறுவல் தோல்வியுற்றது
agents-updating = புதுப்பிக்கிறது…
agents-retrying = மீண்டும் முயற்சிக்கிறது…
agents-preparing = தயார் செய்கிறது…

extensions-title = நீட்டிப்புகள்
extensions-search = நிறுவப்பட்டவை அல்லது Chrome Web Store இல் தேடு…
extensions-relaunch = பயன்படுத்த மறுதொடக்கம் செய்யவும்
extensions-empty = நீட்டிப்புகள் எதுவும் நிறுவப்படவில்லை
extensions-no-match = பொருந்தும் நீட்டிப்புகள் இல்லை
extensions-empty-detail = மேலே Chrome Web Store இல் தேடி Return அழுத்தவும்.
extensions-no-match-detail = வேறொரு பெயர் அல்லது நீட்டிப்பு ID முயற்சிக்கவும்.
extensions-on = இயக்கம்
extensions-off = முடக்கம்
extensions-enable-confirm = { $name } இயக்கவா?
extensions-enable-permissions = { $name } இயக்கி அனுமதி வழங்கு:

lsp-title = மொழி சேவையகங்கள்
lsp-search = மொழி சேவையகங்கள், linters, formatters தேடு…
lsp-loading = பட்டியல் ஏற்றுகிறது…
lsp-empty = பொருந்தும் மொழி சேவையகங்கள் இல்லை
lsp-empty-detail = வேறொரு மொழி, linter அல்லது formatter முயற்சிக்கவும்.
lsp-needs = { $tool } தேவை
lsp-status-available = கிடைக்கிறது
lsp-status-on-path = PATH இல் உள்ளது
lsp-status-installing = நிறுவுகிறது…
lsp-status-installed = நிறுவப்பட்டது
lsp-status-outdated = புதுப்பிப்பு கிடைக்கிறது
lsp-status-running = இயங்குகிறது
lsp-status-failed = தோல்வி

spaces-title = இடங்கள்
spaces-new-placeholder = புதிய இட பெயர்
spaces-empty = இடங்கள் இல்லை
spaces-default-name = இடம் { $number }
spaces-tabs = { $count ->
    [one] 1 தாவல்
   *[other] { $count } தாவல்கள்
}
spaces-delete = இடம் நீக்கு

team-title = குழு
team-just-you = இந்த இடத்தில் நீங்கள் மட்டும்
team-agents = { $count ->
    [one] நீங்களும் 1 முகவரும்
   *[other] நீங்களும் { $count } முகவர்களும்
}
team-empty = இன்னும் யாரும் இங்கில்லை
team-you = நீங்கள்
team-agent = முகவர்

services-title = பின்னணி சேவைகள்
services-processes = { $count ->
    [one] 1 செயல்முறை
   *[other] { $count } செயல்முறைகள்
}
services-kill-all = அனைத்தையும் நிறுத்து
services-not-running = சேவை இயங்கவில்லை
services-start-with = இதனுடன் தொடங்கு:
services-empty = செயலில் உள்ள செயல்முறைகள் இல்லை
services-filter = செயல்முறைகளை வடிகட்டு…
services-no-match = பொருந்தும் செயல்முறைகள் இல்லை
services-connected = இணைக்கப்பட்டது
services-disconnected = துண்டிக்கப்பட்டது
services-attached = இணைந்துள்ளது
services-kill = நிறுத்து
services-memory = நினைவகம்
services-size = அளவு
services-shell = Shell

error-title = பிழை

history-search = வரலாறு தேடு
history-clear-all = அனைத்தையும் அழி
history-clear-confirm = முழு வரலாற்றையும் அழிக்கவா?
history-clear-warning = இதை மீட்டெடுக்க முடியாது.
history-cancel = ரத்து
history-today = இன்று
history-yesterday = நேற்று
history-days-ago = { $count } நாட்களுக்கு முன்
history-day-offset = நாள் -{ $count }

settings-title = அமைப்புகள்
settings-loading = அமைப்புகள் ஏற்றுகிறது…
settings-stored = ~/.vmux/settings.ron இல் சேமிக்கப்பட்டது
settings-other = மற்றவை
settings-software-update = மென்பொருள் புதுப்பிப்பு
settings-check-updates = புதுப்பிப்புகளை சரிபார்
settings-check-updates-hint = தொடக்கத்தில் தானாக சரிபார்க்கும், Auto-update இயக்கப்பட்டிருந்தால் ஒவ்வொரு மணி நேரமும் சரிபார்க்கும்.
settings-update-unavailable = கிடைக்கவில்லை
settings-update-unavailable-hint = புதுப்பிப்பான் இந்த பில்டில் சேர்க்கப்படவில்லை.
settings-update-checking = சரிபார்க்கிறது…
settings-update-checking-hint = புதுப்பிப்புகளை சரிபார்க்கிறது…
settings-update-check-again = மீண்டும் சரிபார்
settings-update-current = Vmux புதுப்பித்த நிலையில் உள்ளது.
settings-update-downloading = பதிவிறக்குகிறது…
settings-update-downloading-hint = Vmux { $version } பதிவிறக்குகிறது…
settings-update-installing = நிறுவுகிறது…
settings-update-installing-hint = Vmux { $version } நிறுவுகிறது…
settings-update-ready = புதுப்பிப்பு தயார்
settings-update-ready-hint = Vmux { $version } தயார். பயன்படுத்த மறுதொடக்கம் செய்யவும்.
settings-update-try-again = மீண்டும் முயற்சி
settings-update-failed = புதுப்பிப்புகளை சரிபார்க்க முடியவில்லை.
settings-item = உருப்படி
settings-item-number = உருப்படி { $number }
settings-press-key = ஒரு விசையை அழுத்தவும்…
settings-saved = சேமிக்கப்பட்டது
settings-record-key = புதிய விசை சேர்க்கை பதிவு செய்ய கிளிக் செய்யவும்

tray-open-window = சாளரம் திற
tray-close-window = சாளரம் மூடு
tray-pause-recording = பதிவை இடைநிறுத்து
tray-resume-recording = பதிவை மீண்டும் தொடங்கு
tray-finish-recording = பதிவை முடி
tray-quit = Vmux வெளியேறு

composer-attach-files = கோப்புகளை இணை (/upload)
composer-remove-attachment = இணைப்பை அகற்று

layout-back = பின்
layout-forward = முன்
layout-reload = மீண்டும் ஏற்று
layout-bookmark-page = இந்த பக்கத்தை புக்மார்க் செய்
layout-remove-bookmark = புக்மார்க் அகற்று
layout-pin-page = இந்த பக்கத்தை பின் செய்
layout-unpin-page = பின்னை நீக்கு
layout-manage-extensions = நீட்டிப்புகளை நிர்வகி
layout-new-stack = புதிய Stack
layout-close-tab = தாவல் மூடு
layout-bookmark = புக்மார்க்
layout-pin = பின்
layout-new-tab = புதிய தாவல்
layout-team = குழு

command-switch-space = இடம் மாற்று…
command-search-ask = தேடு அல்லது கேள்…
command-new-tab-placeholder = தேடு அல்லது URL தட்டச்சு செய், அல்லது Terminal தேர்ந்தெடு…
command-placeholder = URL தட்டச்சு செய், தாவல்களை தேடு, அல்லது கட்டளைகளுக்கு >…
command-composer-placeholder = கட்டளைகளுக்கு / அல்லது மீடியாவுக்கு @
command-send = அனுப்பு (Enter)
command-terminal = Terminal
command-open-terminal = Terminal இல் திற
command-stack = Stack
command-tabs = { $count ->
    [one] 1 தாவல்
   *[other] { $count } தாவல்கள்
}
command-prompt = Prompt
command-new-tab = புதிய தாவல்
command-search = தேடு
command-open-value = "{ $value }" திற
command-search-value = "{ $value }" தேடு

schema-appearance = தோற்றம்
schema-general = பொது
schema-layout = தளவமைப்பு
schema-layout-detail = சாளரம், பலகங்கள், பக்கப்பட்டை மற்றும் ஃபோகஸ் வளையம்.
schema-agent = முகவர்
schema-agent-detail = முகவர் நடவடிக்கை மற்றும் கருவி அனுமதிகள்.
schema-shortcuts = குறுக்குவழிகள்
schema-shortcuts-detail = படிக்க மட்டும். இணைப்புகளை மாற்ற settings.ron நேரடியாக திருத்தவும்.
schema-terminal = Terminal
schema-browser = உலாவி
schema-mode = முறை
schema-mode-detail = வலைப்பக்கங்களுக்கான வண்ண திட்டம். சாதனம் உங்கள் கணினியைப் பின்பற்றும்.
schema-device = சாதனம்
schema-light = வெளிர்
schema-dark = இருள்
schema-language = மொழி
schema-language-detail = கணினி, en-US, ja, அல்லது ~/.vmux/locales/<tag>.ftl பட்டியலுடன் பொருந்தும் BCP 47 குறியீடு பயன்படுத்தவும்.
schema-auto-update = தானியங்கி புதுப்பிப்பு
schema-auto-update-detail = தொடக்கத்தில் மற்றும் ஒவ்வொரு மணி நேரமும் புதுப்பிப்புகளை சரிபார்த்து நிறுவு.
schema-startup-url = தொடக்க URL
schema-startup-url-detail = காலியாக இருந்தால் கட்டளை பட்டை திறக்கும்.
schema-search-engine = தேடுபொறி
schema-search-engine-detail = Start மற்றும் கட்டளை பட்டையிலிருந்து வலை தேடல்களுக்கு பயன்படுகிறது.
schema-window = சாளரம்
schema-pane = பலகம்
schema-side-sheet = பக்க தாள்
schema-focus-ring = ஃபோகஸ் வளையம்
schema-run-placement = இயக்க இடவமைப்பு மாற்றீட்டை அனுமதி
schema-run-placement-detail = முகவர்களை இயக்க பலக முறை, திசை மற்றும் நங்கூரம் தேர்வு செய்ய அனுமதி.
schema-leader = தலைவர்
schema-leader-detail = கீழிசைவு குறுக்குவழிகளுக்கான முன்னொட்டு விசை.
schema-chord-timeout = கீழிசைவு காலவரம்பு
schema-chord-timeout-detail = கீழிசைவு முன்னொட்டு காலாவதியாவதற்கு முன் மில்லிவினாடிகள்.
schema-bindings = இணைப்புகள்
schema-confirm-close = மூடுவதை உறுதிப்படுத்து
schema-confirm-close-detail = இயங்கும் செயல்முறையுடன் terminal மூடும் முன் கேள்.
schema-default-theme = இயல்புநிலை கருப்பொருள்
schema-default-theme-detail = கருப்பொருள் பட்டியலிலிருந்து செயலில் உள்ள கருப்பொருளின் பெயர்.
