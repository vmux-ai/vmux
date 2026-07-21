common-open = Otwórz
common-close = Zamknij
common-install = Zainstaluj
common-uninstall = Odinstaluj
common-update = Aktualizuj
common-retry = Ponów
common-refresh = Odśwież
common-remove = Usuń
common-enable = Włącz
common-disable = Wyłącz
common-new = Nowy
common-active = aktywny
common-running = uruchomiony
common-done = gotowe
common-failed = Błąd
common-installed = Zainstalowany
common-items = { $count ->
    [one] { $count } element
   *[other] { $count } elementów
}
start-title = Start
start-tagline = Jedno polecenie. Wszystko zrobione.

agents-title = Agenci
agents-search = Szukaj agentów ACP i CLI…
agents-empty = Brak pasujących agentów
agents-empty-detail = Podaj nazwę, środowisko lub ACP/CLI.
agents-install-failed = Instalacja nie powiodła się
agents-updating = Aktualizowanie…
agents-retrying = Ponawiam…
agents-preparing = Przygotowywanie…

extensions-title = Rozszerzenia
extensions-search = Szukaj w zainstalowanych lub Chrome Web Store…
extensions-relaunch = Uruchom ponownie, aby zastosować
extensions-empty = Brak zainstalowanych rozszerzeń
extensions-no-match = Brak pasujących rozszerzeń
extensions-empty-detail = Wyszukaj w Chrome Web Store powyżej i naciśnij Return.
extensions-no-match-detail = Spróbuj innej nazwy lub ID rozszerzenia.
extensions-on = Wł.
extensions-off = Wył.
extensions-enable-confirm = Włączyć { $name }?
extensions-enable-permissions = Włącz { $name } i zezwól na:

lsp-title = Serwery językowe
lsp-search = Szukaj serwerów językowych, linterów, formaterów…
lsp-loading = Ładowanie katalogu…
lsp-empty = Brak pasujących serwerów językowych
lsp-empty-detail = Spróbuj innego języka, lintera lub formatera.
lsp-needs = wymaga { $tool }
lsp-status-available = Dostępny
lsp-status-on-path = W PATH
lsp-status-installing = Instalowanie…
lsp-status-installed = Zainstalowany
lsp-status-outdated = Dostępna aktualizacja
lsp-status-running = Uruchomiony
lsp-status-failed = Błąd

spaces-title = Przestrzenie
spaces-new-placeholder = Nazwa nowej przestrzeni
spaces-empty = Brak przestrzeni
spaces-default-name = Przestrzeń { $number }
spaces-tabs = { $count ->
    [one] 1 karta
   *[other] { $count } kart
}
spaces-delete = Usuń przestrzeń

team-title = Zespół
team-just-you = Tylko ty w tej przestrzeni
team-agents = { $count ->
    [one] Ty i 1 agent
   *[other] Ty i { $count } agentów
}
team-empty = Nikt tu jeszcze nie jest
team-you = Ty
team-agent = Agent

services-title = Usługi w tle
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procesów
}
services-kill-all = Zakończ wszystkie
services-not-running = Usługa nie działa
services-start-with = Uruchom z:
services-empty = Brak aktywnych procesów
services-filter = Filtruj procesy…
services-no-match = Brak pasujących procesów
services-connected = Połączony
services-disconnected = Rozłączony
services-attached = dołączony
services-kill = Zakończ
services-memory = Pamięć
services-size = Rozmiar
services-shell = Powłoka

error-title = Błąd

history-search = Szukaj w historii
history-clear-all = Wyczyść wszystko
history-clear-confirm = Wyczyścić całą historię?
history-clear-warning = Tej operacji nie można cofnąć.
history-cancel = Anuluj
history-today = Dziś
history-yesterday = Wczoraj
history-days-ago = { $count } dni temu
history-day-offset = Dzień -{ $count }

settings-title = Ustawienia
settings-loading = Ładowanie ustawień…
settings-stored = Przechowywane w ~/.vmux/settings.ron
settings-other = Inne
settings-software-update = Aktualizacja oprogramowania
settings-check-updates = Sprawdź aktualizacje
settings-check-updates-hint = Sprawdza automatycznie przy uruchomieniu i co godzinę, gdy włączona jest automatyczna aktualizacja.
settings-update-unavailable = Niedostępne
settings-update-unavailable-hint = Narzędzie do aktualizacji nie jest zawarte w tej kompilacji.
settings-update-checking = Sprawdzanie…
settings-update-checking-hint = Sprawdzanie dostępności aktualizacji…
settings-update-check-again = Sprawdź ponownie
settings-update-current = Vmux jest aktualny.
settings-update-downloading = Pobieranie…
settings-update-downloading-hint = Pobieranie Vmux { $version }…
settings-update-installing = Instalowanie…
settings-update-installing-hint = Instalowanie Vmux { $version }…
settings-update-ready = Aktualizacja gotowa
settings-update-ready-hint = Vmux { $version } jest gotowy. Uruchom ponownie, aby zastosować.
settings-update-try-again = Spróbuj ponownie
settings-update-failed = Nie można sprawdzić aktualizacji.
settings-item = Element
settings-item-number = Element { $number }
settings-press-key = Naciśnij klawisz…
settings-saved = Zapisano
settings-record-key = Kliknij, aby nagrać nowy skrót klawiszowy

tray-open-window = Otwórz okno
tray-close-window = Zamknij okno
tray-pause-recording = Wstrzymaj nagrywanie
tray-resume-recording = Wznów nagrywanie
tray-finish-recording = Zakończ nagrywanie
tray-quit = Zamknij Vmux

composer-attach-files = Dołącz pliki (/upload)
composer-remove-attachment = Usuń załącznik

layout-back = Wstecz
layout-forward = Dalej
layout-reload = Odśwież
layout-bookmark-page = Dodaj zakładkę
layout-remove-bookmark = Usuń zakładkę
layout-pin-page = Przypnij stronę
layout-unpin-page = Odepnij stronę
layout-manage-extensions = Zarządzaj rozszerzeniami
layout-new-stack = Nowy stos
layout-close-tab = Zamknij kartę
layout-bookmark = Zakładka
layout-pin = Przypnij
layout-new-tab = Nowa karta
layout-team = Zespół

command-switch-space = Przełącz przestrzeń…
command-search-ask = Szukaj lub pytaj…
command-new-tab-placeholder = Szukaj lub wpisz URL albo wybierz Terminal…
command-placeholder = Wpisz URL, szukaj kart lub > dla poleceń…
command-composer-placeholder = Wpisz / dla poleceń lub @ dla mediów
command-send = Wyślij (Enter)
command-terminal = Terminal
command-open-terminal = Otwórz w terminalu
command-stack = Stos
command-tabs = { $count ->
    [one] 1 karta
   *[other] { $count } kart
}
command-prompt = Monit
command-new-tab = Nowa karta
command-search = Szukaj
command-open-value = Otwórz „{ $value }"
command-search-value = Szukaj „{ $value }"

schema-appearance = Wygląd
schema-general = Ogólne
schema-layout = Układ
schema-layout-detail = Okno, panele, pasek boczny i pierścień fokusu.
schema-agent = Agent
schema-agent-detail = Zachowanie agenta i uprawnienia narzędzi.
schema-shortcuts = Skróty
schema-shortcuts-detail = Widok tylko do odczytu. Edytuj settings.ron bezpośrednio, aby zmienić przypisania.
schema-terminal = Terminal
schema-browser = Przeglądarka
schema-mode = Tryb
schema-mode-detail = Schemat kolorów dla stron internetowych. Urządzenie podąża za ustawieniami systemu.
schema-device = Urządzenie
schema-light = Jasny
schema-dark = Ciemny
schema-language = Język
schema-language-detail = Użyj systemu, en-US, ja lub dowolnego tagu BCP 47 z pasującym katalogiem ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Automatyczna aktualizacja
schema-auto-update-detail = Sprawdzaj i instaluj aktualizacje przy uruchomieniu i co godzinę.
schema-startup-url = Adres URL przy uruchomieniu
schema-startup-url-detail = Puste pole otwiera monit paska poleceń.
schema-search-engine = Wyszukiwarka
schema-search-engine-detail = Używana do wyszukiwania w sieci ze Startu i paska poleceń.
schema-window = Okno
schema-pane = Panel
schema-side-sheet = Panel boczny
schema-focus-ring = Pierścień fokusu
schema-run-placement = Zezwól na nadpisanie miejsca uruchomienia
schema-run-placement-detail = Pozwól agentom wybierać tryb, kierunek i kotwicę panelu uruchomienia.
schema-leader = Lider
schema-leader-detail = Klawisz prefiksu dla skrótów akordowych.
schema-chord-timeout = Limit czasu akordu
schema-chord-timeout-detail = Milisekundy przed wygaśnięciem prefiksu akordu.
schema-bindings = Przypisania
schema-confirm-close = Potwierdź zamknięcie
schema-confirm-close-detail = Pytaj przed zamknięciem terminala z uruchomionym procesem.
schema-default-theme = Domyślny motyw
schema-default-theme-detail = Nazwa aktywnego motywu z listy motywów.
