common-open = Відкрити
common-close = Закрити
common-install = Інсталювати
common-uninstall = Видалити
common-update = Оновити
common-retry = Повторити
common-refresh = Оновити
common-remove = Прибрати
common-enable = Увімкнути
common-disable = Вимкнути
common-new = Створити
common-active = активний
common-running = виконується
common-done = готово
common-failed = Помилка
common-installed = Інстальовано
common-items = { $count ->
    [one] { $count } елемент
   *[other] { $count } елементів
}
start-title = Початок
start-tagline = Один prompt — і все готово.

agents-title = Агенти
agents-search = Пошук агентів ACP і CLI…
agents-empty = Відповідних агентів немає
agents-empty-detail = Спробуйте назву, середовище виконання або ACP/CLI.
agents-install-failed = Не вдалося інсталювати
agents-updating = Оновлення…
agents-retrying = Повторна спроба…
agents-preparing = Підготовка…

extensions-title = Розширення
extensions-search = Пошук серед інстальованих або в Chrome Web Store…
extensions-relaunch = Перезапустіть, щоб застосувати
extensions-empty = Розширення не інстальовано
extensions-no-match = Відповідних розширень немає
extensions-empty-detail = Знайдіть розширення в Chrome Web Store вище й натисніть Enter.
extensions-no-match-detail = Спробуйте іншу назву або ID розширення.
extensions-on = Увімкнено
extensions-off = Вимкнено
extensions-enable-confirm = Увімкнути { $name }?
extensions-enable-permissions = Увімкнути { $name } і дозволити:

lsp-title = Мовні сервери
lsp-search = Пошук мовних серверів, лінтерів, форматерів…
lsp-loading = Завантаження каталогу…
lsp-empty = Відповідних мовних серверів немає
lsp-empty-detail = Спробуйте іншу мову, лінтер або форматер.
lsp-needs = потрібен { $tool }
lsp-status-available = Доступно
lsp-status-on-path = У PATH
lsp-status-installing = Інсталяція…
lsp-status-installed = Інстальовано
lsp-status-outdated = Доступне оновлення
lsp-status-running = Запущено
lsp-status-failed = Помилка

spaces-title = Простори
spaces-new-placeholder = Назва нового простору
spaces-empty = Просторів немає
spaces-default-name = Простір { $number }
spaces-tabs = { $count ->
    [one] 1 вкладка
   *[other] { $count } вкладок
}
spaces-delete = Видалити простір

team-title = Команда
team-just-you = У цьому просторі лише ви
team-agents = { $count ->
    [one] Ви й 1 агент
   *[other] Ви й { $count } агентів
}
team-empty = Тут поки нікого немає
team-you = Ви
team-agent = Агент

services-title = Фонові служби
services-processes = { $count ->
    [one] 1 процес
   *[other] { $count } процесів
}
services-kill-all = Примусово завершити всі
services-not-running = Службу не запущено
services-start-with = Запустити з:
services-empty = Активних процесів немає
services-filter = Фільтр процесів…
services-no-match = Відповідних процесів немає
services-connected = Під’єднано
services-disconnected = Від’єднано
services-attached = приєднано
services-kill = Примусово завершити
services-memory = Пам’ять
services-size = Розмір
services-shell = Оболонка

error-title = Помилка

history-search = Пошук в історії
history-clear-all = Очистити все
history-clear-confirm = Очистити всю історію?
history-clear-warning = Цю дію не можна скасувати.
history-cancel = Скасувати
history-today = Сьогодні
history-yesterday = Учора
history-days-ago = { $count } дн. тому
history-day-offset = День -{ $count }

settings-title = Налаштування
settings-loading = Завантаження налаштувань…
settings-stored = Зберігається в ~/.vmux/settings.ron
settings-other = Інше
settings-software-update = Оновлення ПЗ
settings-check-updates = Перевірити оновлення
settings-check-updates-hint = Перевіряється автоматично під час запуску та щогодини, якщо ввімкнено автооновлення.
settings-update-unavailable = Недоступно
settings-update-unavailable-hint = У цій збірці немає модуля оновлення.
settings-update-checking = Перевірка…
settings-update-checking-hint = Перевірка наявності оновлень…
settings-update-check-again = Перевірити ще раз
settings-update-current = Vmux оновлено до актуальної версії.
settings-update-downloading = Завантаження…
settings-update-downloading-hint = Завантаження Vmux { $version }…
settings-update-installing = Інсталяція…
settings-update-installing-hint = Інсталяція Vmux { $version }…
settings-update-ready = Оновлення готове
settings-update-ready-hint = Vmux { $version } готовий. Перезапустіть, щоб застосувати оновлення.
settings-update-try-again = Спробувати ще раз
settings-update-failed = Не вдалося перевірити оновлення.
settings-item = Елемент
settings-item-number = Елемент { $number }
settings-press-key = Натисніть клавішу…
settings-saved = Збережено
settings-record-key = Натисніть, щоб записати нову комбінацію клавіш

tray-open-window = Відкрити вікно
tray-close-window = Закрити вікно
tray-pause-recording = Призупинити запис
tray-resume-recording = Відновити запис
tray-finish-recording = Завершити запис
tray-quit = Вийти з Vmux

composer-attach-files = Прикріпити файли (/upload)
composer-remove-attachment = Прибрати вкладення

layout-back = Назад
layout-forward = Уперед
layout-reload = Перезавантажити
layout-bookmark-page = Додати сторінку в закладки
layout-remove-bookmark = Видалити закладку
layout-pin-page = Закріпити сторінку
layout-unpin-page = Відкріпити сторінку
layout-manage-extensions = Керувати розширеннями
layout-new-stack = Новий стек
layout-close-tab = Закрити вкладку
layout-bookmark = Закладка
layout-pin = Закріпити
layout-new-tab = Нова вкладка
layout-team = Команда

command-switch-space = Перемкнути простір…
command-search-ask = Пошук або запит…
command-new-tab-placeholder = Шукайте, введіть URL або виберіть термінал…
command-placeholder = Введіть URL, шукайте вкладки або > для команд…
command-composer-placeholder = Введіть / для команд або @ для медіа
command-send = Надіслати (Enter)
command-terminal = Термінал
command-open-terminal = Відкрити в терміналі
command-stack = Стек
command-tabs = { $count ->
    [one] 1 вкладка
   *[other] { $count } вкладок
}
command-prompt = Prompt
command-new-tab = Нова вкладка
command-search = Пошук
command-open-value = Відкрити «{ $value }»
command-search-value = Шукати «{ $value }»

schema-appearance = Вигляд
schema-general = Загальні
schema-layout = Макет
schema-layout-detail = Вікно, області, бічна панель і рамка фокуса.
schema-agent = Агент
schema-agent-detail = Поведінка агента й дозволи для інструментів.
schema-shortcuts = Скорочення
schema-shortcuts-detail = Лише для перегляду. Щоб змінити прив’язки, редагуйте settings.ron напряму.
schema-terminal = Термінал
schema-browser = Браузер
schema-mode = Режим
schema-mode-detail = Колірна схема вебсторінок. «Пристрій» використовує системну тему.
schema-device = Пристрій
schema-light = Світла
schema-dark = Темна
schema-language = Мова
schema-language-detail = Використовуйте системну мову, en-US, ja або будь-який тег BCP 47 з відповідним каталогом ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Автооновлення
schema-auto-update-detail = Перевіряти й інсталювати оновлення під час запуску та щогодини.
schema-startup-url = URL запуску
schema-startup-url-detail = Якщо порожньо, відкривається prompt командного рядка.
schema-search-engine = Пошукова система
schema-search-engine-detail = Використовується для вебпошуку з екрана «Початок» і командного рядка.
schema-window = Вікно
schema-pane = Область
schema-side-sheet = Бічна панель
schema-focus-ring = Рамка фокуса
schema-run-placement = Дозволити перевизначення розміщення запуску
schema-run-placement-detail = Дозволити агентам вибирати режим області запуску, напрямок і прив’язку.
schema-leader = Leader
schema-leader-detail = Префіксна клавіша для chord-скорочень.
schema-chord-timeout = Тайм-аут chord
schema-chord-timeout-detail = Мілісекунди до завершення дії chord-префікса.
schema-bindings = Прив’язки
schema-confirm-close = Підтверджувати закриття
schema-confirm-close-detail = Запитувати підтвердження перед закриттям термінала із запущеним процесом.
schema-default-theme = Типова тема
schema-default-theme-detail = Назва активної теми зі списку тем.
