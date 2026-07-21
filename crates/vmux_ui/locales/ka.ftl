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
