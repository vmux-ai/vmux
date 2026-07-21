common-open = திற
common-close = மூடு
common-install = நிறுவு
common-uninstall = நிறுவல் நீக்கு
common-update = புதுப்பி
common-retry = மீண்டும் முயற்சி செய்
common-refresh = புதுப்பித்துக் காட்டு
common-remove = அகற்று
common-enable = இயக்கு
common-disable = முடக்கு
common-new = புதியது
common-active = செயலில்
common-running = இயங்குகிறது
common-done = முடிந்தது
common-failed = தோல்வியடைந்தது
common-installed = நிறுவப்பட்டது
common-items = { $count ->
    [one] { $count } உருப்படி
   *[other] { $count } உருப்படிகள்
}
start-title = தொடங்கு
start-tagline = ஒரே prompt. எதுவும் முடிந்துவிடும்.

agents-title = ஏஜென்ட்கள்
agents-search = ACP மற்றும் CLI ஏஜென்ட்களைத் தேடு…
agents-empty = பொருந்தும் ஏஜென்ட்கள் இல்லை
agents-empty-detail = பெயர், runtime, அல்லது ACP/CLI மூலம் முயற்சிக்கவும்.
agents-install-failed = நிறுவல் தோல்வியடைந்தது
agents-updating = புதுப்பிக்கிறது…
agents-retrying = மீண்டும் முயற்சிக்கிறது…
agents-preparing = தயாராகிறது…

extensions-title = நீட்டிப்புகள்
extensions-search = நிறுவப்பட்டவை அல்லது Chrome Web Store-இல் தேடு…
extensions-relaunch = செயல்படுத்த மீண்டும் திற
extensions-empty = நீட்டிப்புகள் எதுவும் நிறுவப்படவில்லை
extensions-no-match = பொருந்தும் நீட்டிப்புகள் இல்லை
extensions-empty-detail = மேலே Chrome Web Store-இல் தேடி Return அழுத்தவும்.
extensions-no-match-detail = வேறு பெயர் அல்லது extension ID மூலம் முயற்சிக்கவும்.
extensions-on = இயக்கு
extensions-off = முடக்கு
extensions-enable-confirm = { $name }-ஐ இயக்கவா?
extensions-enable-permissions = { $name }-ஐ இயக்கி இவற்றை அனுமதிக்கவும்:

lsp-title = மொழி சர்வர்கள்
lsp-search = மொழி சர்வர்கள், linters, formatters தேடு…
lsp-loading = பட்டியல் ஏற்றப்படுகிறது…
lsp-empty = பொருந்தும் மொழி சர்வர்கள் இல்லை
lsp-empty-detail = வேறு மொழி, linter, அல்லது formatter மூலம் முயற்சிக்கவும்.
lsp-needs = { $tool } தேவை
lsp-status-available = கிடைக்கிறது
lsp-status-on-path = PATH-இல் உள்ளது
lsp-status-installing = நிறுவுகிறது…
lsp-status-installed = நிறுவப்பட்டது
lsp-status-outdated = புதுப்பிப்பு கிடைக்கிறது
lsp-status-running = இயங்குகிறது
lsp-status-failed = தோல்வியடைந்தது

spaces-title = இடங்கள்
spaces-new-placeholder = புதிய இடத்தின் பெயர்
spaces-empty = இடங்கள் இல்லை
spaces-default-name = இடம் { $number }
spaces-tabs = { $count ->
    [one] 1 தாவல்
   *[other] { $count } தாவல்கள்
}
spaces-delete = இடத்தை நீக்கு

team-title = குழு
team-just-you = இந்த இடத்தில் நீங்கள் மட்டும்
team-agents = { $count ->
    [one] நீங்களும் 1 ஏஜென்டும்
   *[other] நீங்களும் { $count } ஏஜென்ட்களும்
}
team-empty = இங்கே இன்னும் யாரும் இல்லை
team-you = நீங்கள்
team-agent = ஏஜென்ட்

services-title = பின்னணி சேவைகள்
services-processes = { $count ->
    [one] 1 செயல்முறை
   *[other] { $count } செயல்முறைகள்
}
services-kill-all = அனைத்தையும் நிறுத்து
services-not-running = சேவை இயங்கவில்லை
services-start-with = இதன்மூலம் தொடங்கு:
services-empty = செயலில் உள்ள செயல்முறைகள் இல்லை
services-filter = செயல்முறைகளை வடிகட்டு…
services-no-match = பொருந்தும் செயல்முறைகள் இல்லை
services-connected = இணைந்தது
services-disconnected = துண்டிக்கப்பட்டது
services-attached = இணைக்கப்பட்டது
services-kill = நிறுத்து
services-memory = நினைவகம்
services-size = அளவு
services-shell = Shell

error-title = பிழை

history-search = வரலாற்றைத் தேடு
history-clear-all = அனைத்தையும் அழி
history-clear-confirm = முழு வரலாறையும் அழிக்கவா?
history-clear-warning = இதை மீட்டெடுக்க முடியாது.
history-cancel = ரத்து செய்
history-today = இன்று
history-yesterday = நேற்று
history-days-ago = { $count } நாட்களுக்கு முன்
history-day-offset = நாள் -{ $count }

settings-title = அமைப்புகள்
settings-loading = அமைப்புகள் ஏற்றப்படுகின்றன…
settings-stored = ~/.vmux/settings.ron-இல் சேமிக்கப்பட்டுள்ளது
settings-other = மற்றவை
settings-software-update = மென்பொருள் புதுப்பிப்பு
settings-check-updates = புதுப்பிப்புகளைச் சரிபார்
settings-check-updates-hint = Auto-update இயக்கப்பட்டிருந்தால், தொடக்கத்திலும் ஒவ்வொரு மணிநேரமும் தானாகச் சரிபார்க்கும்.
settings-update-unavailable = கிடைக்கவில்லை
settings-update-unavailable-hint = இந்த build-இல் updater சேர்க்கப்படவில்லை.
settings-update-checking = சரிபார்க்கிறது…
settings-update-checking-hint = புதுப்பிப்புகள் சரிபார்க்கப்படுகின்றன…
settings-update-check-again = மீண்டும் சரிபார்
settings-update-current = Vmux சமீபத்திய பதிப்பில் உள்ளது.
settings-update-downloading = பதிவிறக்குகிறது…
settings-update-downloading-hint = Vmux { $version } பதிவிறக்கப்படுகிறது…
settings-update-installing = நிறுவுகிறது…
settings-update-installing-hint = Vmux { $version } நிறுவப்படுகிறது…
settings-update-ready = புதுப்பிப்பு தயாராக உள்ளது
settings-update-ready-hint = Vmux { $version } தயாராக உள்ளது. செயல்படுத்த மறுதொடக்கம் செய்யவும்.
settings-update-try-again = மீண்டும் முயற்சி செய்
settings-update-failed = புதுப்பிப்புகளைச் சரிபார்க்க முடியவில்லை.
settings-item = உருப்படி
settings-item-number = உருப்படி { $number }
settings-press-key = ஒரு விசையை அழுத்தவும்…
settings-saved = சேமிக்கப்பட்டது
settings-record-key = புதிய விசை சேர்க்கையைப் பதிவு செய்ய கிளிக் செய்க

tray-open-window = சாளரத்தைத் திற
tray-close-window = சாளரத்தை மூடு
tray-pause-recording = பதிவை இடைநிறுத்து
tray-resume-recording = பதிவைத் தொடரு
tray-finish-recording = பதிவை முடி
tray-quit = Vmux-இலிருந்து வெளியேறு

composer-attach-files = கோப்புகளை இணை (/upload)
composer-remove-attachment = இணைப்பை அகற்று

layout-back = பின்
layout-forward = முன்
layout-reload = மீண்டும் ஏற்று
layout-bookmark-page = இந்தப் பக்கத்தைப் புத்தகக்குறியாக்கு
layout-remove-bookmark = புத்தகக்குறியை அகற்று
layout-pin-page = இந்தப் பக்கத்தைப் பின் செய்
layout-unpin-page = இந்தப் பக்கத்தின் பின்னை நீக்கு
layout-manage-extensions = நீட்டிப்புகளை நிர்வகி
layout-new-stack = புதிய அடுக்கு
layout-close-tab = தாவலை மூடு
layout-bookmark = புத்தகக்குறி
layout-pin = பின் செய்
layout-new-tab = புதிய தாவல்
layout-team = குழு

command-switch-space = இடம் மாற்று…
command-search-ask = தேடு அல்லது கேள்…
command-new-tab-placeholder = தேடவும், URL உள்ளிடவும், அல்லது Terminal தேர்ந்தெடுக்கவும்…
command-placeholder = URL உள்ளிடவும், தாவல்களைத் தேடவும், அல்லது கட்டளைகளுக்கு > உள்ளிடவும்…
command-composer-placeholder = கட்டளைகளுக்கு / அல்லது ஊடகத்திற்கு @ உள்ளிடவும்
command-send = அனுப்பு (Enter)
command-terminal = Terminal
command-open-terminal = Terminal-இல் திற
command-stack = அடுக்கு
command-tabs = { $count ->
    [one] 1 தாவல்
   *[other] { $count } தாவல்கள்
}
command-prompt = Prompt
command-new-tab = புதிய தாவல்
command-search = தேடு
command-open-value = “{ $value }” திற
command-search-value = “{ $value }” தேடு

schema-appearance = தோற்றம்
schema-general = பொது
schema-layout = தளவமைப்பு
schema-layout-detail = சாளரம், பலகங்கள், பக்கப்பட்டி, மற்றும் focus ring.
schema-agent = ஏஜென்ட்
schema-agent-detail = ஏஜென்ட் நடத்தை மற்றும் கருவி அனுமதிகள்.
schema-shortcuts = குறுக்குவழிகள்
schema-shortcuts-detail = படிக்க மட்டும். bindings-ஐ மாற்ற settings.ron-ஐ நேரடியாகத் திருத்தவும்.
schema-terminal = Terminal
schema-browser = உலாவி
schema-mode = பயன்முறை
schema-mode-detail = இணையப் பக்கங்களுக்கான வண்ணத் திட்டம். Device உங்கள் கணினியைப் பின்பற்றும்.
schema-device = Device
schema-light = வெளிர்
schema-dark = இருள்
schema-language = மொழி
schema-language-detail = கணினி மொழி, en-US, ja, அல்லது பொருந்தும் ~/.vmux/locales/<tag>.ftl catalog உள்ள எந்த BCP 47 tag-ஐயும் பயன்படுத்தவும்.
schema-auto-update = Auto-update
schema-auto-update-detail = தொடக்கத்திலும் ஒவ்வொரு மணிநேரமும் புதுப்பிப்புகளைச் சரிபார்த்து நிறுவும்.
schema-startup-url = தொடக்க URL
schema-startup-url-detail = காலியாக விட்டால் command bar prompt திறக்கும்.
schema-search-engine = தேடுபொறி
schema-search-engine-detail = Start மற்றும் command bar-இலிருந்து இணையத் தேடல்களுக்கு பயன்படுத்தப்படும்.
schema-window = சாளரம்
schema-pane = பலகம்
schema-side-sheet = பக்கத் தாள்
schema-focus-ring = Focus ring
schema-run-placement = run placement மாற்றத்தை அனுமதி
schema-run-placement-detail = run பலகத்தின் பயன்முறை, திசை, மற்றும் anchor-ஐ ஏஜென்ட்கள் தேர்வு செய்ய அனுமதிக்கவும்.
schema-leader = Leader
schema-leader-detail = chord குறுக்குவழிகளுக்கான முன்னொட்டு விசை.
schema-chord-timeout = Chord timeout
schema-chord-timeout-detail = chord முன்னொட்டு காலாவதியாகும் முன் உள்ள மில்லிவிநாடிகள்.
schema-bindings = Bindings
schema-confirm-close = மூடும் முன் உறுதிப்படுத்து
schema-confirm-close-detail = இயங்கும் செயல்முறை உள்ள terminal-ஐ மூடுவதற்கு முன் கேட்கவும்.
schema-default-theme = இயல்புநிலை தீம்
schema-default-theme-detail = themes பட்டியலில் உள்ள செயலில் உள்ள தீமின் பெயர்.
