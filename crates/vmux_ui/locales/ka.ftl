locale-name = ქართული
common-open = გახსნა
common-close = დახურვა
common-install = დაყენება
common-uninstall = წაშლა
common-update = განახლება
common-retry = ხელახლა ცდა
common-refresh = განახლება
common-remove = ამოშლა
common-enable = ჩართვა
common-disable = გამორთვა
common-new = ახალი
common-active = აქტიური
common-running = გაშვებულია
common-done = დასრულდა
common-failed = ვერ შესრულდა
common-installed = დაყენებულია
common-items = { $count ->
    [one] { $count } ელემენტი
   *[other] { $count } ელემენტი
}

tools-title = ხელსაწყოები
tools-search = პაკეტების, აგენტების, MCP-ის, ენის ხელსაწყოებისა და კონფიგურაციის ფაილების ძიება…
tools-open = ხელსაწყოების გახსნა
tools-fold = ხელსაწყოების აკეცვა
tools-unfold = ხელსაწყოების გაშლა
tools-scanning = ადგილობრივი ხელსაწყოების სკანირება…
tools-no-installed = დაინსტალირებული ხელსაწყოები არ არის
tools-empty = შესაბამისი ხელსაწყოები არ არის
tools-empty-detail = დააინსტალირეთ პაკეტი ან დაამატეთ Stow-ის სტილის კონფიგურაციის ფაილების პაკეტი.
tools-apply = გამოყენება
tools-homebrew = Homebrew
tools-homebrew-sync = დაინსტალირებული ფორმულები და პროგრამები ავტომატურად სინქრონდება.
tools-open-brewfile = Brewfile-ის გახსნა
tools-managed = მართული
tools-provider-homebrew-formulae = Homebrew-ის ფორმულები
tools-provider-homebrew-casks = Homebrew-ის პროგრამები
tools-provider-npm = npm-ის პაკეტები
tools-provider-acp-agents = ACP-ის აგენტები
tools-provider-language-tools = ენის ხელსაწყოები
tools-provider-mcp-servers = MCP-ის სერვერები
tools-provider-dotfiles = კონფიგურაციის ფაილები
tools-status-available = ხელმისაწვდომია
tools-status-missing = აკლია
tools-status-conflict = კონფლიქტი
tools-forget = დავიწყება
tools-manage = მართვა
tools-link = დაკავშირება
tools-unlink = კავშირის გაუქმება
tools-import = იმპორტი
tools-update-count = { $count ->
    [one] 1 განახლება
   *[other] { $count } განახლება
}
tools-conflict-count = { $count ->
    [one] 1 კონფლიქტი
   *[other] { $count } კონფლიქტი
}
tools-result-applied = ხელსაწყოები გამოყენებულია
tools-result-imported = ხელსაწყოები იმპორტირებულია
tools-result-installed = { $name } დაინსტალირდა
tools-result-updated = { $name } განახლდა
tools-result-uninstalled = { $name } წაიშალა
tools-result-forgotten = { $name } დავიწყებულია
tools-result-managed = { $name } ახლა იმართება
tools-result-linked = { $name } დაკავშირებულია
tools-result-unlinked = { $name }-თან კავშირი გაუქმებულია

start-title = დაწყება
start-tagline = ერთი პრომპტი. ყველაფერი მზად.

agents-title = აგენტები
agents-search = ACP და CLI აგენტების ძიება…
agents-empty = შესატყვისი აგენტები არ არის
agents-empty-detail = სცადეთ სახელი, გაშვების გარემო ან ACP/CLI.
agents-install-failed = დაყენება ვერ მოხერხდა
agents-updating = ახლდება…
agents-retrying = ხელახლა ვცდით…
agents-preparing = მზადდება…

extensions-title = გაფართოებები
extensions-search = მოძებნეთ დაყენებულებში ან Chrome Web Store-ში…
extensions-relaunch = ცვლილებების გამოსაყენებლად ხელახლა გაუშვით
extensions-empty = გაფართოებები დაყენებული არ არის
extensions-no-match = შესატყვისი გაფართოებები არ არის
extensions-empty-detail = მოძებნეთ ზემოთ Chrome Web Store-ში და დააჭირეთ Enter-ს.
extensions-no-match-detail = სცადეთ სხვა სახელი ან გაფართოების ID.
extensions-on = ჩართულია
extensions-off = გამორთულია
extensions-enable-confirm = ჩავრთოთ { $name }?
extensions-enable-permissions = ჩაირთოს { $name } და მიენიჭოს წვდომა:

lsp-title = ენის სერვერები
lsp-search = ენის სერვერების, ლინტერებისა და ფორმატერების ძიება…
lsp-loading = კატალოგი იტვირთება…
lsp-empty = შესატყვისი ენის სერვერები არ არის
lsp-empty-detail = სცადეთ სხვა ენა, ლინტერი ან ფორმატერი.
lsp-needs = სჭირდება { $tool }
lsp-status-available = ხელმისაწვდომია
lsp-status-on-path = PATH-შია
lsp-status-installing = ყენდება…
lsp-status-installed = დაყენებულია
lsp-status-outdated = განახლება ხელმისაწვდომია
lsp-status-running = გაშვებულია
lsp-status-failed = ვერ შესრულდა

spaces-title = გარემოები
spaces-new-placeholder = ახალი გარემოს სახელი
spaces-empty = გარემოები არ არის
spaces-default-name = გარემო { $number }
spaces-tabs = { $count ->
    [one] 1 ჩანართი
   *[other] { $count } ჩანართი
}
spaces-delete = გარემოს წაშლა

team-title = გუნდი
team-just-you = ამ გარემოში მხოლოდ თქვენ ხართ
team-agents = { $count ->
    [one] თქვენ და 1 აგენტი
   *[other] თქვენ და { $count } აგენტი
}
team-empty = აქ ჯერ არავინაა
team-you = თქვენ
team-agent = აგენტი

services-title = ფონური სერვისები
services-processes = { $count ->
    [one] 1 პროცესი
   *[other] { $count } პროცესი
}
services-kill-all = ყველას იძულებით დასრულება
services-not-running = სერვისი გაშვებული არ არის
services-start-with = გაშვება:
services-empty = აქტიური პროცესები არ არის
services-filter = პროცესების გაფილტვრა…
services-no-match = შესატყვისი პროცესები არ არის
services-connected = დაკავშირებულია
services-disconnected = გათიშულია
services-attached = მიბმულია
services-kill = იძულებით დასრულება
services-memory = მეხსიერება
services-size = ზომა
services-shell = გარსი

error-title = შეცდომა

history-search = ისტორიის ძიება
history-clear-all = ყველაფრის გასუფთავება
history-clear-confirm = გავასუფთავოთ მთელი ისტორია?
history-clear-warning = ამ მოქმედების გაუქმება შეუძლებელია.
history-cancel = გაუქმება
history-today = დღეს
history-yesterday = გუშინ
history-days-ago = { $count } დღის წინ
history-day-offset = დღე -{ $count }

settings-title = პარამეტრები
settings-loading = პარამეტრები იტვირთება…
settings-stored = ინახება ~/.vmux/settings.ron-ში
settings-other = სხვა
settings-software-update = პროგრამის განახლება
settings-check-updates = განახლებების შემოწმება
settings-check-updates-hint = ავტომატურად მოწმდება გაშვებისას და ყოველ საათში, როცა ავტომატური განახლება ჩართულია.
settings-update-unavailable = მიუწვდომელია
settings-update-unavailable-hint = განახლების მოდული ამ აგებაში არ შედის.
settings-update-checking = მოწმდება…
settings-update-checking-hint = განახლებები მოწმდება…
settings-update-check-again = ხელახლა შემოწმება
settings-update-current = Vmux განახლებულია.
settings-update-downloading = იტვირთება…
settings-update-downloading-hint = იტვირთება Vmux { $version }…
settings-update-installing = ყენდება…
settings-update-installing-hint = ყენდება Vmux { $version }…
settings-update-ready = განახლება მზადაა
settings-update-ready-hint = Vmux { $version } მზადაა. გამოსაყენებლად გადატვირთეთ.
settings-update-try-again = ხელახლა ცდა
settings-update-failed = განახლებების შემოწმება ვერ მოხერხდა.
settings-item = ელემენტი
settings-item-number = ელემენტი { $number }
settings-press-key = დააჭირეთ კლავიშს…
settings-saved = შენახულია
settings-record-key = დააწკაპუნეთ ახალი კლავიშთა კომბინაციის ჩასაწერად

tray-open-window = ფანჯრის გახსნა
tray-close-window = ფანჯრის დახურვა
tray-pause-recording = ჩაწერის შეჩერება
tray-resume-recording = ჩაწერის გაგრძელება
tray-finish-recording = ჩაწერის დასრულება
tray-quit = Vmux-იდან გასვლა

composer-attach-files = ფაილების მიმაგრება (/upload)
composer-remove-attachment = დანართის ამოშლა

layout-back = უკან
layout-forward = წინ
layout-reload = ხელახლა ჩატვირთვა
layout-bookmark-page = გვერდის სანიშნეებში დამატება
layout-remove-bookmark = სანიშნის ამოშლა
layout-pin-page = გვერდის მიმაგრება
layout-unpin-page = გვერდის მოხსნა
layout-manage-extensions = გაფართოებების მართვა
layout-new-stack = ახალი სტეკი
layout-close-tab = ჩანართის დახურვა
layout-bookmark = სანიშნე
layout-pin = მიმაგრება
layout-new-tab = ახალი ჩანართი
layout-team = გუნდი

command-switch-space = გარემოს შეცვლა…
command-search-ask = ძიება ან კითხვა…
command-new-tab-placeholder = მოძებნეთ, შეიყვანეთ URL ან აირჩიეთ ტერმინალი…
command-placeholder = შეიყვანეთ URL, მოძებნეთ ჩანართებში ან ბრძანებებისთვის აკრიფეთ >…
command-composer-placeholder = ბრძანებებისთვის აკრიფეთ /, მედიისთვის — @
command-send = გაგზავნა (Enter)
command-terminal = ტერმინალი
command-open-terminal = ტერმინალში გახსნა
command-stack = სტეკი
command-tabs = { $count ->
    [one] 1 ჩანართი
   *[other] { $count } ჩანართი
}
command-prompt = პრომპტი
command-new-tab = ახალი ჩანართი
command-search = ძიება
command-open-value = „{ $value }“-ის გახსნა
command-search-value = „{ $value }“-ის ძიება

schema-appearance = იერსახე
schema-general = ზოგადი
schema-layout = განლაგება
schema-layout-detail = ფანჯარა, პანელები, გვერდითი ზოლი და ფოკუსის ჩარჩო.
schema-agent = აგენტი
schema-agent-detail = აგენტის ქცევა და ხელსაწყოების ნებართვები.
schema-shortcuts = მალსახმობები
schema-shortcuts-detail = მხოლოდ ნახვა. კომბინაციების შესაცვლელად პირდაპირ settings.ron ჩაასწორეთ.
schema-terminal = ტერმინალი
schema-browser = ბრაუზერი
schema-mode = რეჟიმი
schema-mode-detail = ვებგვერდების ფერთა სქემა. მოწყობილობა მიჰყვება სისტემურ პარამეტრს.
schema-device = მოწყობილობა
schema-light = ღია
schema-dark = მუქი
schema-language = ენა
schema-language-detail = გამოიყენეთ სისტემური, en-US, ja ან ნებისმიერი BCP 47 ტეგი შესაბამისი ~/.vmux/locales/<tag>.ftl კატალოგით.
schema-auto-update = ავტომატური განახლება
schema-auto-update-detail = განახლებების შემოწმება და დაყენება გაშვებისას და ყოველ საათში.
schema-startup-url = საწყისი URL
schema-startup-url-detail = ცარიელი მნიშვნელობა ხსნის ბრძანების ზოლის პრომპტს.
schema-search-engine = საძიებო სისტემა
schema-search-engine-detail = გამოიყენება ვებძიებისთვის დაწყების გვერდიდან და ბრძანების ზოლიდან.
schema-window = ფანჯარა
schema-pane = პანელი
schema-side-sheet = გვერდითი ფურცელი
schema-focus-ring = ფოკუსის ჩარჩო
schema-run-placement = გაშვების მდებარეობის გადაფარვის დაშვება
schema-run-placement-detail = მიეცით აგენტებს გაშვების პანელის რეჟიმის, მიმართულებისა და მიბმის წერტილის არჩევის უფლება.
schema-leader = ლიდერი
schema-leader-detail = პრეფიქს-კლავიში ქორდული მალსახმობებისთვის.
schema-chord-timeout = ქორდის ვადა
schema-chord-timeout-detail = მილიწამები, სანამ ქორდის პრეფიქსი გაუქმდება.
schema-bindings = კომბინაციები
schema-confirm-close = დახურვის დადასტურება
schema-confirm-close-detail = დადასტურების მოთხოვნა გაშვებული პროცესის მქონე ტერმინალის დახურვამდე.
schema-default-theme = ნაგულისხმევი თემა
schema-default-theme-detail = აქტიური თემის სახელი თემების სიიდან.

settings-empty = (ცარიელი)
settings-none = (არცერთი)

schema-system = სისტემა
schema-editor = რედაქტორი
schema-recording = ჩაწერა
schema-radius = რადიუსი
schema-padding = შიდა დაშორება
schema-gap = შუალედი
schema-width = სიგანე
schema-color = ფერი
schema-red = წითელი
schema-green = მწვანე
schema-blue = ლურჯი
schema-follow-files = ფაილების მიყოლა
schema-tidy-files = ფაილების მოწესრიგება
schema-tidy-files-max = ფაილების მოწესრიგების ზღვარი
schema-tidy-files-auto = ფაილების ავტომატურად მოწესრიგება
schema-app-providers = აპის მომწოდებლები
schema-provider = მომწოდებელი
schema-kind = ტიპი
schema-models = მოდელები
schema-acp = ACP აგენტები
schema-id = ID
schema-name = სახელი
schema-command = ბრძანება
schema-arguments = არგუმენტები
schema-environment = გარემო
schema-working-directory = სამუშაო საქაღალდე
schema-shell = გარსი
schema-font-family = შრიფტის ოჯახი
schema-startup-directory = საწყისი საქაღალდე
schema-themes = თემები
schema-color-scheme = ფერთა სქემა
schema-font-size = შრიფტის ზომა
schema-line-height = ხაზის სიმაღლე
schema-cursor-style = კურსორის სტილი
schema-cursor-blink = კურსორის ციმციმი
schema-custom-themes = მორგებული თემები
schema-foreground = წინა პლანი
schema-background = ფონი
schema-cursor = კურსორი
schema-ansi-colors = ANSI ფერები
schema-keymap = კლავიატურის რუკა
schema-explorer = მკვლევარი
schema-visible = ხილული
schema-language-servers = ენის სერვერები
schema-servers = სერვერები
schema-language-id = ენის ID
schema-root-markers = ძირის ნიშნულები
schema-output-directory = გამოტანის საქაღალდე

menu-scene = სცენა
menu-layout = განლაგება
menu-terminal = ტერმინალი
menu-browser = ბრაუზერი
menu-service = სერვისი
menu-bookmark = სანიშნე
menu-edit = რედაქტირება

layout-knowledge = ცოდნა
layout-open-knowledge = ცოდნის გახსნა
layout-open-welcome-knowledge = „კეთილი იყოს თქვენი მობრძანება ცოდნაში“-ს გახსნა
layout-open-path = გახსნა: { $path }
layout-fold-knowledge = ცოდნის დაკეცვა
layout-unfold-knowledge = ცოდნის გაშლა
layout-bookmarks = სანიშნეები
layout-new-folder = ახალი საქაღალდე
layout-add-to-bookmarks = სანიშნეებში დამატება
layout-move-to-bookmarks = სანიშნეებში გადატანა
layout-stack-number = სტეკი { $number }
layout-fold-stack = სტეკის დაკეცვა
layout-unfold-stack = სტეკის გაშლა
layout-close-stack = სტეკის დახურვა
layout-bookmark-in = სანიშნე საქაღალდეში { $folder }

common-cancel = გაუქმება
common-delete = წაშლა
common-save = შენახვა
common-rename = სახელის შეცვლა
common-expand = გაშლა
common-collapse = აკეცვა
common-loading = იტვირთება…
common-error = შეცდომა
common-output = გამოტანა
common-pending = მოლოდინში
common-current = მიმდინარე
common-stop = შეჩერება
services-command = Vmux-ის სერვისი
services-uptime-seconds = { $seconds } წმ
services-uptime-minutes = { $minutes } წთ { $seconds } წმ
services-uptime-hours = { $hours } სთ { $minutes } წთ
services-uptime-days = { $days } დღ { $hours } სთ

error-page-failed-load = გვერდი ვერ ჩაიტვირთა
error-page-not-found = გვერდი ვერ მოიძებნა
error-unknown-host = უცნობი Vmux აპის ჰოსტი: { $host }

history-title = ისტორია

command-new-app-chat = ახალი { $provider }/{ $model } ჩატი (აპი)
command-interactive-mode-user = სცენა > ინტერაქტიული რეჟიმი > მომხმარებელი
command-interactive-mode-player = სცენა > ინტერაქტიული რეჟიმი > მოთამაშე
command-minimize-window = განლაგება > ფანჯარა > ჩაკეცვა
command-toggle-layout = განლაგება > განლაგება > განლაგების გადართვა
command-close-tab = განლაგება > ჩანართი > ჩანართის დახურვა
command-new-task = განლაგება > ჩანართი > ახალი ამოცანა…
command-next-tab = განლაგება > ჩანართი > შემდეგი ჩანართი
command-prev-tab = განლაგება > ჩანართი > წინა ჩანართი
command-rename-tab = განლაგება > ჩანართი > ჩანართის სახელის შეცვლა
command-tab-select-1 = განლაგება > ჩანართი > ჩანართი 1-ის არჩევა
command-tab-select-2 = განლაგება > ჩანართი > ჩანართი 2-ის არჩევა
command-tab-select-3 = განლაგება > ჩანართი > ჩანართი 3-ის არჩევა
command-tab-select-4 = განლაგება > ჩანართი > ჩანართი 4-ის არჩევა
command-tab-select-5 = განლაგება > ჩანართი > ჩანართი 5-ის არჩევა
command-tab-select-6 = განლაგება > ჩანართი > ჩანართი 6-ის არჩევა
command-tab-select-7 = განლაგება > ჩანართი > ჩანართი 7-ის არჩევა
command-tab-select-8 = განლაგება > ჩანართი > ჩანართი 8-ის არჩევა
command-tab-select-last = განლაგება > ჩანართი > ბოლო ჩანართის არჩევა
command-close-pane = განლაგება > პანელი > პანელის დახურვა
command-select-pane-left = განლაგება > პანელი > მარცხენა პანელის არჩევა
command-select-pane-right = განლაგება > პანელი > მარჯვენა პანელის არჩევა
command-select-pane-up = განლაგება > პანელი > ზედა პანელის არჩევა
command-select-pane-down = განლაგება > პანელი > ქვედა პანელის არჩევა
command-swap-pane-prev = განლაგება > პანელი > პანელის წინა პოზიციაზე გადატანა
command-swap-pane-next = განლაგება > პანელი > პანელის შემდეგ პოზიციაზე გადატანა
command-equalize-pane-size = განლაგება > პანელი > პანელების ზომების გათანაბრება
command-resize-pane-left = განლაგება > პანელი > პანელის მარცხნივ ზომის შეცვლა
command-resize-pane-right = განლაგება > პანელი > პანელის მარჯვნივ ზომის შეცვლა
command-resize-pane-up = განლაგება > პანელი > პანელის ზემოთ ზომის შეცვლა
command-resize-pane-down = განლაგება > პანელი > პანელის ქვემოთ ზომის შეცვლა
command-stack-close = განლაგება > სტეკი > სტეკის დახურვა
command-stack-next = განლაგება > სტეკი > შემდეგი სტეკი
command-stack-previous = განლაგება > სტეკი > წინა სტეკი
command-stack-reopen = განლაგება > სტეკი > დახურული გვერდის ხელახლა გახსნა
command-stack-swap-prev = განლაგება > სტეკი > სტეკის მარცხნივ გადატანა
command-stack-swap-next = განლაგება > სტეკი > სტეკის მარჯვნივ გადატანა
command-space-open = განლაგება > სივრცე > სივრცეები
command-terminal-close = ტერმინალი > ტერმინალის დახურვა
command-terminal-next = ტერმინალი > შემდეგი ტერმინალი
command-terminal-prev = ტერმინალი > წინა ტერმინალი
command-terminal-clear = ტერმინალი > ტერმინალის გასუფთავება
command-browser-prev-page = ბრაუზერი > ნავიგაცია > უკან
command-browser-next-page = ბრაუზერი > ნავიგაცია > წინ
command-browser-reload = ბრაუზერი > ნავიგაცია > ხელახლა ჩატვირთვა
command-browser-hard-reload = ბრაუზერი > ნავიგაცია > სრული გადატვირთვა
command-open-in-place = ბრაუზერი > გახსნა > აქ გახსნა
command-open-in-new-stack = ბრაუზერი > გახსნა > ახალ სტეკში გახსნა
command-open-in-pane-top = ბრაუზერი > გახსნა > ზედა პანელში გახსნა
command-open-in-pane-right = ბრაუზერი > გახსნა > მარჯვენა პანელში გახსნა
command-open-in-pane-bottom = ბრაუზერი > გახსნა > ქვედა პანელში გახსნა
command-open-in-pane-left = ბრაუზერი > გახსნა > მარცხენა პანელში გახსნა
command-open-in-new-tab = ბრაუზერი > გახსნა > ახალ ჩანართში გახსნა
command-open-in-new-space = ბრაუზერი > გახსნა > ახალ სივრცეში გახსნა
command-browser-zoom-in = ბრაუზერი > ხედი > გადიდება
command-browser-zoom-out = ბრაუზერი > ხედი > დაპატარავება
command-browser-zoom-reset = ბრაუზერი > ხედი > ნამდვილი ზომა
command-browser-dev-tools = ბრაუზერი > ხედი > დეველოპერის ხელსაწყოები
command-browser-open-command-bar = ბრაუზერი > ზოლი > ბრძანებების ზოლი
command-browser-open-page-in-command-bar = ბრაუზერი > ზოლი > გვერდის რედაქტირება
command-browser-open-path-bar = ბრაუზერი > ზოლი > ბილიკის ნავიგატორი
command-browser-open-commands = ბრაუზერი > ზოლი > ბრძანებები
command-browser-open-history = ბრაუზერი > ზოლი > ისტორია
command-service-open = სერვისი > სერვისების მონიტორის გახსნა
command-bookmark-toggle-active = სანიშნე > გვერდის სანიშნეებში დამატება
command-bookmark-pin-active = სანიშნე > გვერდის მიმაგრება

layout-tab = ჩანართი
layout-no-stacks = სტეკები არ არის
layout-loading = იტვირთება…
layout-no-markdown-files = Markdown ფაილები არ არის
layout-empty-folder = ცარიელი საქაღალდე
layout-worktree = სამუშაო ხე
layout-folder-name = საქაღალდის სახელი
layout-no-pins-bookmarks = მიმაგრებები ან სანიშნეები არ არის
layout-move-to = გადატანა: { $folder }
layout-bookmark-current-page = მიმდინარე გვერდის სანიშნეებში დამატება
layout-rename-folder = საქაღალდის სახელის შეცვლა
layout-remove-folder = საქაღალდის წაშლა
layout-update-downloading = განახლება იტვირთება
layout-update-installing = განახლება ყენდება…
layout-update-ready = ხელმისაწვდომია ახალი ვერსია
layout-restart-update = გადატვირთეთ განახლებისთვის

agent-preparing = აგენტი ემზადება…
agent-send-all-queued = ყველა რიგში მდგომი მოთხოვნის ახლავე გაგზავნა (Esc)
agent-send = გაგზავნა (Enter)
agent-ready = მზად ვარ, როცა თქვენ იქნებით.
agent-loading-older = ძველი შეტყობინებები იტვირთება…
agent-load-older = ძველი შეტყობინებების ჩატვირთვა
agent-continued-from = გაგრძელებულია აქედან: { $source }
agent-older-context-omitted = ძველი კონტექსტი გამოტოვებულია
agent-interrupted = შეწყვეტილია
agent-allow-tool = დავუშვათ { $tool }?
agent-deny = უარყოფა
agent-allow-always = ყოველთვის დაშვება
agent-allow = დაშვება
agent-loading-sessions = სესიები იტვირთება…
agent-no-resumable-sessions = გასაგრძელებელი სესიები ვერ მოიძებნა
agent-no-matching-sessions = შესაბამისი სესიები არ არის
agent-no-matching-models = შესაბამისი მოდელები არ არის
agent-choice-help = ↑/↓ ან Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = აირჩიეთ რეპოზიტორიის საქაღალდე
agent-choose-repository-detail = აირჩიეთ ადგილობრივი Git რეპოზიტორია, რომელსაც აგენტი გამოიყენებს.
agent-choosing = არჩევა…
agent-choose-folder = საქაღალდის არჩევა
agent-queued = რიგშია
agent-attached = მიმაგრებულია:
agent-cancel-queued = რიგში მდგომი მოთხოვნის გაუქმება
agent-resume-queued = რიგში მდგომი მოთხოვნების გაგრძელება
agent-clear-queue = რიგის გასუფთავება
agent-send-all-now = ყველას ახლავე გაგზავნა
agent-choose-option = აირჩიეთ ვარიანტი ზემოთ
agent-loading-media = მედია იტვირთება…
agent-no-matching-media = შესაბამისი მედია არ არის
agent-prompt-context = მოთხოვნის კონტექსტი
agent-details = დეტალები
agent-path = ბილიკი
agent-tool = ხელსაწყო
agent-server = სერვერი
agent-bytes = { $count } ბაიტი
agent-worked-for = იმუშავა { $duration }
agent-worked-for-steps = { $count ->
    [one] იმუშავა { $duration } · 1 ნაბიჯი
   *[other] იმუშავა { $duration } · { $count } ნაბიჯი
}
agent-tool-guardian-review = Guardian-ის შემოწმება
agent-tool-read-files = წაიკითხა ფაილები
agent-tool-viewed-image = ნახა სურათი
agent-tool-used-browser = გამოიყენა ბრაუზერი
agent-tool-searched-files = მოძებნა ფაილები
agent-tool-ran-commands = გაუშვა ბრძანებები
agent-thinking = ფიქრობს
agent-subagent = ქვეაგენტი
agent-prompt = მოთხოვნა
agent-thread = თემა
agent-parent = მშობელი
agent-children = შვილები
agent-call = გამოძახება
agent-raw-event = დაუმუშავებელი მოვლენა
agent-plan = გეგმა
agent-tasks = { $count ->
    [one] 1 ამოცანა
   *[other] { $count } ამოცანა
}
agent-edited = რედაქტირებულია
agent-reconnecting = ხელახლა დაკავშირება { $attempt }/{ $total }
agent-status-running = მუშაობს
agent-status-done = დასრულდა
agent-status-failed = ვერ შესრულდა
agent-status-pending = მოლოდინში
agent-slash-attach-files = ფაილების მიმაგრება
agent-slash-resume-session = წინა სესიის გაგრძელება
agent-slash-select-model = მოდელის არჩევა
agent-slash-continue-cli = ამ სესიის CLI-ში გაგრძელება
agent-session-just-now = ახლახან
agent-session-minutes-ago = { $count } წთ წინ
agent-session-hours-ago = { $count } სთ წინ
agent-session-days-ago = { $count } დღ წინ
agent-working-working = მუშაობს
agent-working-thinking = ფიქრობს
agent-working-pondering = აზროვნებს
agent-working-noodling = იდეებს ამუშავებს
agent-working-percolating = მწიფდება
agent-working-conjuring = ქმნის
agent-working-cooking = ამზადებს
agent-working-brewing = ხარშავს
agent-working-musing = ფიქრებშია
agent-working-ruminating = ჩაუღრმავდა
agent-working-scheming = გეგმას აწყობს
agent-working-synthesizing = აჯამებს
agent-working-tinkering = აწყობს
agent-working-churning = ამუშავებს
agent-working-vibing = ვაიბობს
agent-working-simmering = ნელა ხარშავს
agent-working-crafting = ქმნის
agent-working-divining = გზას ეძებს
agent-working-mulling = განიხილავს
agent-working-spelunking = სიღრმეში ეძებს

editor-toggle-explorer = Explorer-ის ჩვენება/დამალვა (Cmd+B)
editor-unsaved = შეუნახავი
editor-rendered-markdown = რენდერირებული Markdown ცოცხალი რედაქტირებით
editor-note = შენიშვნა
editor-source-editor = კოდის რედაქტორი
editor-editor = რედაქტორი
editor-git-diff = Git სხვაობა
editor-diff = სხვაობა
editor-tidy = მოწესრიგება
editor-always = ყოველთვის
editor-unchanged-previews = { $count ->
    [one] ✦ 1 უცვლელი გადახედვა
   *[other] ✦ { $count } უცვლელი გადახედვა
}
editor-open-externally = გარეთ გახსნა
editor-changed-line = შეცვლილი ხაზი
editor-go-to-definition = განსაზღვრებაზე გადასვლა
editor-find-references = მითითებების პოვნა
editor-references = { $count ->
    [one] 1 მითითება
   *[other] { $count } მითითება
}
editor-lsp-starting = { $server } ირთვება…
editor-lsp-not-installed = { $server } — დაყენებული არ არის
editor-explorer = Explorer
editor-open-editors = ღია რედაქტორები
editor-outline = სტრუქტურა
editor-new-file = ახალი ფაილი
editor-new-folder = ახალი საქაღალდე
editor-delete-confirm = წაიშალოს „{ $name }“? ამას ვერ გააუქმებთ.
editor-created-folder = შეიქმნა საქაღალდე { $name }
editor-created-file = შეიქმნა ფაილი { $name }
editor-renamed-to = სახელი შეიცვალა: { $name }
editor-deleted = წაიშალა { $name }
editor-failed-decode-image = სურათის დეკოდირება ვერ მოხერხდა
editor-preview-large-image = სურათი (გადახედვისთვის ძალიან დიდია)
editor-preview-binary = ბინარული
editor-preview-file = ფაილი

git-status-clean = სუფთა
git-status-modified = შეცვლილი
git-status-staged = ინდექსირებული
git-status-staged-modified = ინდექსირებული*
git-status-untracked = უთვალთვალო
git-status-deleted = წაშლილი
git-status-conflict = კონფლიქტი
git-accept-all = ✓ ყველას მიღება
git-unstage = ინდექსიდან ამოღება
git-confirm-deny-all = ყველას უარყოფის დადასტურება
git-deny-all = ✗ ყველას უარყოფა
git-commit-message = კომიტის შეტყობინება
git-commit = კომიტი ({ $count })
git-push = ↑ Push
git-loading-diff = სხვაობა იტვირთება…
git-no-changes = საჩვენებელი ცვლილებები არ არის
git-accept = ✓ მიღება
git-deny = ✗ უარყოფა
git-show-unchanged-lines = { $count } უცვლელი ხაზის ჩვენება

terminal-loading = იტვირთება…
terminal-runs-when-ready = გაეშვება მზადყოფნისას · Ctrl+C ასუფთავებს · Esc გამოტოვებს
terminal-booting = იტვირთება
terminal-type-command = აკრიფეთ ბრძანება · გაეშვება მზადყოფნისას · Esc გამოტოვებს

setup-tagline-claude = Anthropic-ის კოდის აგენტი Vmux-ში
setup-tagline-codex = OpenAI-ის კოდის აგენტი Vmux-ში
setup-tagline-vibe = Mistral-ის კოდის აგენტი Vmux-ში
setup-install-title = { $name } CLI-ის დაყენება
setup-homebrew-required = { $command }-ის დასაყენებლად საჭიროა Homebrew, რომელიც ჯერ გამართული არ არის. Vmux ჯერ Homebrew-ს დააყენებს, შემდეგ — { $name }-ს.
setup-terminal-instructions = ტერმინალში დასაწყებად დააჭირეთ Return-ს, შემდეგ მოთხოვნისას შეიყვანეთ თქვენი Mac-ის პაროლი.
setup-command-missing = Vmux-მა ეს გვერდი გახსნა, რადგან ადგილობრივი { $command } ბრძანება ჯერ დაყენებული არ არის. მის მისაღებად გაუშვით ქვემოთ მოცემული ბრძანება.
setup-install-failed = დაყენება არ დასრულდა. დეტალებისთვის შეამოწმეთ ტერმინალი, შემდეგ სცადეთ ხელახლა.
setup-installing = ყენდება…
setup-install-homebrew = Homebrew + { $name }-ის დაყენება
setup-run-install = დაყენების ბრძანების გაშვება
setup-auto-reload = Vmux მას ტერმინალში გაუშვებს და { $command }-ის მზადყოფნისას ხელახლა ჩაიტვირთება.

debug-title = გამართვა
debug-auto-update = ავტომატური განახლება
debug-simulate-update = ხელმისაწვდომი განახლების სიმულაცია
debug-simulate-download = ჩამოტვირთვის სიმულაცია
debug-clear-update = განახლების გასუფთავება
debug-trigger-restart = გადატვირთვის გაშვება

command-manage-spaces = სივრცეების მართვა…
command-pane-stack-location = პანელი { $pane } / სტეკი { $stack }
command-space-pane-stack-location = { $space } / პანელი { $pane } / სტეკი { $stack }
command-terminal-path = ტერმინალი ({ $path })
command-group-interactive-mode = ინტერაქტიული რეჟიმი
command-group-window = ფანჯარა
command-group-tab = ჩანართი
command-group-pane = პანელი
command-group-stack = სტეკი
command-group-space = სივრცე
command-group-navigation = ნავიგაცია
command-group-open = გახსნა
command-group-view = ხედი
command-group-bar = ზოლი

menu-close-vmux = Vmux-ის დახურვა

agents-terminal-coding-agent = ტერმინალზე დაფუძნებული კოდის აგენტი
