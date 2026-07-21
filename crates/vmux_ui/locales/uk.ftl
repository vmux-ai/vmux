common-open = Відкрити
common-close = Закрити
common-install = Встановити
common-uninstall = Видалити
common-update = Оновити
common-retry = Повторити
common-refresh = Оновити
common-remove = Вилучити
common-enable = Увімкнути
common-disable = Вимкнути
common-new = Новий
common-active = активний
common-running = виконується
common-done = готово
common-failed = Помилка
common-installed = Встановлено
common-items = { $count ->
    [one] { $count } елемент
   *[other] { $count } елементів
}
start-title = Початок
start-tagline = Один запит. Будь-що, виконано.

agents-title = Агенти
agents-search = Пошук агентів ACP і CLI…
agents-empty = Агентів не знайдено
agents-empty-detail = Спробуйте назву, середовище або ACP/CLI.
agents-install-failed = Помилка встановлення
agents-updating = Оновлення…
agents-retrying = Повторна спроба…
agents-preparing = Підготовка…

extensions-title = Розширення
extensions-search = Пошук встановлених або в Chrome Web Store…
extensions-relaunch = Перезапустіть для застосування
extensions-empty = Розширення не встановлені
extensions-no-match = Збігів не знайдено
extensions-empty-detail = Знайдіть розширення у Chrome Web Store вище та натисніть Return.
extensions-no-match-detail = Спробуйте іншу назву або ідентифікатор розширення.
extensions-on = Увімк.
extensions-off = Вимк.
extensions-enable-confirm = Увімкнути { $name }?
extensions-enable-permissions = Увімкнути { $name } та дозволити:

lsp-title = Мовні сервери
lsp-search = Пошук мовних серверів, лінтерів, форматерів…
lsp-loading = Завантаження каталогу…
lsp-empty = Мовних серверів не знайдено
lsp-empty-detail = Спробуйте іншу мову, лінтер або форматер.
lsp-needs = потрібен { $tool }
lsp-status-available = Доступний
lsp-status-on-path = У PATH
lsp-status-installing = Встановлення…
lsp-status-installed = Встановлено
lsp-status-outdated = Доступне оновлення
lsp-status-running = Виконується
lsp-status-failed = Помилка

spaces-title = Простори
spaces-new-placeholder = Назва нового простору
spaces-empty = Немає просторів
spaces-default-name = Простір { $number }
spaces-tabs = { $count ->
    [one] 1 вкладка
   *[other] { $count } вкладок
}
spaces-delete = Видалити простір

team-title = Команда
team-just-you = Тільки ви у цьому просторі
team-agents = { $count ->
    [one] Ви і 1 агент
   *[other] Ви і { $count } агентів
}
team-empty = Тут ще нікого немає
team-you = Ви
team-agent = Агент

services-title = Фонові служби
services-processes = { $count ->
    [one] 1 процес
   *[other] { $count } процесів
}
services-kill-all = Завершити всі
services-not-running = Служба не запущена
services-start-with = Запустити з:
services-empty = Активних процесів немає
services-filter = Фільтр процесів…
services-no-match = Процесів не знайдено
services-connected = Підключено
services-disconnected = Відключено
services-attached = прикріплено
services-kill = Завершити
services-memory = Пам'ять
services-size = Розмір
services-shell = Оболонка

error-title = Помилка

history-search = Пошук в історії
history-clear-all = Очистити все
history-clear-confirm = Очистити всю історію?
history-clear-warning = Це неможливо скасувати.
history-cancel = Скасувати
history-today = Сьогодні
history-yesterday = Вчора
history-days-ago = { $count } днів тому
history-day-offset = День -{ $count }

settings-title = Налаштування
settings-loading = Завантаження налаштувань…
settings-stored = Зберігається у ~/.vmux/settings.ron
settings-other = Інше
settings-software-update = Оновлення програмного забезпечення
settings-check-updates = Перевірити оновлення
settings-check-updates-hint = Перевіряється автоматично під час запуску та щогодини, якщо увімкнено автооновлення.
settings-update-unavailable = Недоступно
settings-update-unavailable-hint = Оновлювач не входить до цієї збірки.
settings-update-checking = Перевірка…
settings-update-checking-hint = Перевірка оновлень…
settings-update-check-again = Перевірити знову
settings-update-current = Vmux актуальний.
settings-update-downloading = Завантаження…
settings-update-downloading-hint = Завантаження Vmux { $version }…
settings-update-installing = Встановлення…
settings-update-installing-hint = Встановлення Vmux { $version }…
settings-update-ready = Оновлення готове
settings-update-ready-hint = Vmux { $version } готовий. Перезапустіть для застосування.
settings-update-try-again = Спробувати знову
settings-update-failed = Не вдалося перевірити оновлення.
settings-item = Елемент
settings-item-number = Елемент { $number }
settings-press-key = Натисніть клавішу…
settings-saved = Збережено
settings-record-key = Натисніть для запису нової комбінації клавіш

tray-open-window = Відкрити вікно
tray-close-window = Закрити вікно
tray-pause-recording = Призупинити запис
tray-resume-recording = Відновити запис
tray-finish-recording = Завершити запис
tray-quit = Вийти з Vmux

composer-attach-files = Прикріпити файли (/upload)
composer-remove-attachment = Видалити вкладення

layout-back = Назад
layout-forward = Вперед
layout-reload = Оновити
layout-bookmark-page = Додати сторінку до закладок
layout-remove-bookmark = Видалити закладку
layout-pin-page = Закріпити сторінку
layout-unpin-page = Відкріпити сторінку
layout-manage-extensions = Керування розширеннями
layout-new-stack = Новий стек
layout-close-tab = Закрити вкладку
layout-bookmark = Закладка
layout-pin = Закріпити
layout-new-tab = Нова вкладка
layout-team = Команда

command-switch-space = Перемкнути простір…
command-search-ask = Пошук або запит…
command-new-tab-placeholder = Пошук або введіть URL, чи виберіть Термінал…
command-placeholder = Введіть URL, пошук вкладок або > для команд…
command-composer-placeholder = Введіть / для команд або @ для медіа
command-send = Надіслати (Enter)
command-terminal = Термінал
command-open-terminal = Відкрити у терміналі
command-stack = Стек
command-tabs = { $count ->
    [one] 1 вкладка
   *[other] { $count } вкладок
}
command-prompt = Запит
command-new-tab = Нова вкладка
command-search = Пошук
command-open-value = Відкрити "{ $value }"
command-search-value = Пошук "{ $value }"

schema-appearance = Зовнішній вигляд
schema-general = Загальні
schema-layout = Макет
schema-layout-detail = Вікно, панелі, бічна панель і кільце фокусу.
schema-agent = Агент
schema-agent-detail = Поведінка агента та дозволи інструментів.
schema-shortcuts = Ярлики
schema-shortcuts-detail = Лише перегляд. Редагуйте settings.ron безпосередньо для зміни прив'язок.
schema-terminal = Термінал
schema-browser = Браузер
schema-mode = Режим
schema-mode-detail = Колірна схема для веб-сторінок. Пристрій слідує вашій системі.
schema-device = Пристрій
schema-light = Світлий
schema-dark = Темний
schema-language = Мова
schema-language-detail = Використовуйте системну, en-US, ja або будь-який тег BCP 47 з відповідним каталогом ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Автооновлення
schema-auto-update-detail = Перевіряти та встановлювати оновлення під час запуску та щогодини.
schema-startup-url = URL при запуску
schema-startup-url-detail = Порожнє відкриває командний рядок.
schema-search-engine = Пошукова система
schema-search-engine-detail = Використовується для веб-пошуку з початкової сторінки та командного рядка.
schema-window = Вікно
schema-pane = Панель
schema-side-sheet = Бічний аркуш
schema-focus-ring = Кільце фокусу
schema-run-placement = Дозволити зміну розміщення запуску
schema-run-placement-detail = Дозволити агентам вибирати режим, напрямок та прив'язку панелі запуску.
schema-leader = Лідер
schema-leader-detail = Клавіша-префікс для акордних скорочень.
schema-chord-timeout = Таймаут акорду
schema-chord-timeout-detail = Мілісекунди до закінчення префікса акорду.
schema-bindings = Прив'язки
schema-confirm-close = Підтвердити закриття
schema-confirm-close-detail = Запитувати перед закриттям терміналу із запущеним процесом.
schema-default-theme = Тема за замовчуванням
schema-default-theme-detail = Назва активної теми зі списку тем.
