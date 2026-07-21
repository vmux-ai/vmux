common-open = გახსენით
common-close = დახურვა
common-install = დააინსტალირეთ
common-uninstall = დეინსტალაცია
common-update = განახლება
common-retry = ხელახლა სცადეთ
common-refresh = განაახლეთ
common-remove = ამოღება
common-enable = ჩართვა
common-disable = გამორთვა
common-new = ახალი
common-active = აქტიური
common-running = გაშვებული
common-done = შესრულებული
common-failed = ვერ მოხერხდა
common-installed = დაყენებულია
common-items = { $count ->
    [one] { $count } ელემენტი
   *[other] { $count } ელემენტი
}
start-title = დაწყება
start-tagline = ერთი მოწოდება. ყველაფერი, შესრულებულია.

agents-title = აგენტები
agents-search = მოძებნეთ ACP და CLI აგენტები…
agents-empty = შესატყვისი აგენტები არ არის
agents-empty-detail = სცადეთ სახელი, გაშვების დრო ან ACP/CLI.
agents-install-failed = ინსტალაცია ვერ მოხერხდა
agents-updating = მიმდინარეობს განახლება…
agents-retrying = ხელახლა მცდელობა…
agents-preparing = მზადება…

extensions-title = გაფართოებები
extensions-search = ძიება დაინსტალირებულია ან Chrome Web Store…
extensions-relaunch = ხელახლა გაშვება განაცხადისთვის
extensions-empty = არ არის დაინსტალირებული გაფართოებები
extensions-no-match = შესატყვისი გაფართოებები არ არის
extensions-empty-detail = მოძებნეთ ზემოთ Chrome Web Store და დააჭირეთ Return.
extensions-no-match-detail = სცადეთ სხვა სახელი ან გაფართოების ID.
extensions-on = ჩართულია
extensions-off = გამორთულია
extensions-enable-confirm = ჩაირთოს { $name }?
extensions-enable-permissions = ჩართეთ { $name } და დაუშვით:

lsp-title = ენის სერვერები
lsp-search = მოძებნეთ ენის სერვერები, ლინტერები, ფორმატორები…
lsp-loading = კატალოგის ჩატვირთვა…
lsp-empty = შესატყვისი ენის სერვერები არ არის
lsp-empty-detail = სცადეთ სხვა ენა, ლინტერი ან ფორმატორი.
lsp-needs = სჭირდება { $tool }
lsp-status-available = ხელმისაწვდომია
lsp-status-on-path = PATH-ზე
lsp-status-installing = მიმდინარეობს ინსტალაცია…
lsp-status-installed = დაყენებულია
lsp-status-outdated = განახლება ხელმისაწვდომია
lsp-status-running = სირბილი
lsp-status-failed = ვერ მოხერხდა

spaces-title = ფართები
spaces-new-placeholder = ახალი სივრცის სახელი
spaces-empty = არ არის სივრცეები
spaces-default-name = სივრცე { $number }
spaces-tabs = { $count ->
    [one] 1 ჩანართი
   *[other] { $count } ჩანართები
}
spaces-delete = სივრცის წაშლა

team-title = გუნდი
team-just-you = მხოლოდ შენ ამ სივრცეში
team-agents = { $count ->
    [one] შენ და 1 აგენტი
   *[other] თქვენ და { $count } აგენტები
}
team-empty = აქ ჯერ არავინ
team-you = შენ
team-agent = აგენტი

services-title = ფონის სერვისები
services-processes = { $count ->
    [one] 1 პროცესი
   *[other] { $count } პროცესები
}
services-kill-all = მოკალი ყველა
services-not-running = სერვისი არ მუშაობს
services-start-with = დაიწყეთ:
services-empty = არ არის აქტიური პროცესები
services-filter = პროცესების გაფილტვრა…
services-no-match = შესატყვისი პროცესები არ არის
services-connected = დაკავშირებულია
services-disconnected = გათიშულია
services-attached = მიმაგრებული
services-kill = მოკალი
services-memory = მეხსიერება
services-size = ზომა
services-shell = ჭურვი

error-title = შეცდომა

history-search = ძიების ისტორია
history-clear-all = გაასუფთავე ყველა
history-clear-confirm = გსურთ მთელი ისტორიის გასუფთავება?
history-clear-warning = ამის გაუქმება შეუძლებელია.
history-cancel = გაუქმება
history-today = დღეს
history-yesterday = გუშინ
history-days-ago = { $count } დღის წინ
history-day-offset = დღე -{ $count }

settings-title = პარამეტრები
settings-loading = პარამეტრების ჩატვირთვა…
settings-stored = ინახება ~/.vmux/settings.ron-ში
settings-other = სხვა
settings-software-update = პროგრამული უზრუნველყოფის განახლება
settings-check-updates = შეამოწმეთ განახლებები
settings-check-updates-hint = ამოწმებს ავტომატურად გაშვებისას და ყოველ საათში, როდესაც ჩართულია ავტომატური განახლება.
settings-update-unavailable = მიუწვდომელია
settings-update-unavailable-hint = განახლება არ შედის ამ კონსტრუქციაში.
settings-update-checking = მიმდინარეობს შემოწმება…
settings-update-checking-hint = მიმდინარეობს განახლებების შემოწმება…
settings-update-check-again = შეამოწმეთ ისევ
settings-update-current = Vmux განახლებულია.
settings-update-downloading = მიმდინარეობს ჩამოტვირთვა…
settings-update-downloading-hint = მიმდინარეობს ჩამოტვირთვა Vmux { $version }…
settings-update-installing = მიმდინარეობს ინსტალაცია…
settings-update-installing-hint = მიმდინარეობს Vmux { $version } ინსტალაცია…
settings-update-ready = განახლება მზადაა
settings-update-ready-hint = Vmux { $version } მზად არის. გადატვირთეთ მის გამოსაყენებლად.
settings-update-try-again = სცადეთ ისევ
settings-update-failed = განახლებების შემოწმება შეუძლებელია.
settings-item = ელემენტი
settings-item-number = ელემენტი { $number }
settings-press-key = დააჭირეთ ღილაკს…
settings-saved = შენახულია
settings-record-key = დააწკაპუნეთ ახალი კლავიშების კომბინაციის ჩასაწერად

tray-open-window = გახსენით ფანჯარა
tray-close-window = ფანჯრის დახურვა
tray-pause-recording = ჩაწერის პაუზა
tray-resume-recording = ჩაწერის განახლება
tray-finish-recording = ჩაწერის დასრულება
tray-quit = გასვლა Vmux

composer-attach-files = ფაილების მიმაგრება (/upload)
composer-remove-attachment = დანართის წაშლა

layout-back = უკან
layout-forward = წინ
layout-reload = გადატვირთვა
layout-bookmark-page = მონიშნეთ ეს გვერდი
layout-remove-bookmark = ამოიღეთ სანიშნე
layout-pin-page = ჩამაგრება ეს გვერდი
layout-unpin-page = ამ გვერდის ჩამაგრების მოხსნა
layout-manage-extensions = გაფართოებების მართვა
layout-new-stack = ახალი დასტა
layout-close-tab = ჩანართის დახურვა
layout-bookmark = სანიშნე
layout-pin = პინი
layout-new-tab = ახალი ჩანართი
layout-team = გუნდი

command-switch-space = სივრცის გადართვა…
command-search-ask = მოძებნეთ ან იკითხეთ…
command-new-tab-placeholder = მოძებნეთ ან აკრიფეთ URL, ან აირჩიეთ ტერმინალი…
command-placeholder = აკრიფეთ URL, მოძებნეთ ჩანართები ან > ბრძანებებისთვის…
command-composer-placeholder = ჩაწერეთ / ბრძანებებისთვის ან @ მედიისთვის
command-send = გაგზავნა (Enter)
command-terminal = ტერმინალი
command-open-terminal = გახსენით ტერმინალში
command-stack = დასტის
command-tabs = { $count ->
    [one] 1 ჩანართი
   *[other] { $count } ჩანართები
}
command-prompt = მოწოდება
command-new-tab = ახალი ჩანართი
command-search = ძიება
command-open-value = გახსენით „{ $value }“
command-search-value = ძიება „{ $value }“

schema-appearance = გარეგნობა
schema-general = გენერალი
schema-layout = განლაგება
schema-layout-detail = ფანჯარა, პანელები, გვერდითი ზოლი და ფოკუსის რგოლი.
schema-agent = აგენტი
schema-agent-detail = აგენტის ქცევა და ხელსაწყოს ნებართვები.
schema-shortcuts = მალსახმობები
schema-shortcuts-detail = მხოლოდ წაკითხვადი ხედი. შეცვალეთ settings.ron პირდაპირ, რომ შეცვალოთ საკინძები.
schema-terminal = ტერმინალი
schema-browser = ბრაუზერი
schema-mode = რეჟიმი
schema-mode-detail = ფერადი სქემა ვებ გვერდებისთვის. მოწყობილობა მიჰყვება თქვენს სისტემას.
schema-device = მოწყობილობა
schema-light = სინათლე
schema-dark = ბნელი
schema-language = ენა
schema-language-detail = გამოიყენეთ სისტემა, en-US, ja, ან ნებისმიერი BCP 47 ტეგი შესაბამისი ~/.vmux/locales/<tag>.ftl კატალოგით.
schema-auto-update = ავტომატური განახლება
schema-auto-update-detail = შეამოწმეთ და დააინსტალირეთ განახლებები გაშვებისას და ყოველ საათში.
schema-startup-url = გაშვება URL
schema-startup-url-detail = ცარიელი ხსნის ბრძანების ზოლს.
schema-search-engine = საძიებო სისტემა
schema-search-engine-detail = გამოიყენება ვებ ძიებისთვის Start-დან და ბრძანების ზოლიდან.
schema-window = ფანჯარა
schema-pane = პანელი
schema-side-sheet = გვერდითი ფურცელი
schema-focus-ring = ფოკუსის ბეჭედი
schema-run-placement = გაშვების განლაგების უგულებელყოფის დაშვება
schema-run-placement-detail = მიეცით საშუალება აგენტებს აირჩიონ პანელის გაშვების რეჟიმი, მიმართულება და დამაგრება.
schema-leader = ლიდერი
schema-leader-detail = პრეფიქსის ღილაკი აკორდის მალსახმობებისთვის.
schema-chord-timeout = აკორდის დრო
schema-chord-timeout-detail = მილიწამებით ადრე აკორდის პრეფიქსის ვადის ამოწურვამდე.
schema-bindings = საკინძები
schema-confirm-close = დაადასტურეთ დახურვა
schema-confirm-close-detail = მოთხოვნა ტერმინალის დახურვამდე მიმდინარე პროცესით.
schema-default-theme = ნაგულისხმევი თემა
schema-default-theme-detail = აქტიური თემის სახელი თემების სიიდან.
