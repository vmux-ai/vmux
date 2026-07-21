locale-name = polski
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
common-active = aktywne
common-running = uruchomione
common-done = gotowe
common-failed = Niepowodzenie
common-installed = Zainstalowano
common-items = { $count ->
    [one] { $count } element
   *[other] { $count } elementów
}
start-title = Start
start-tagline = Jeden prompt. Wszystko załatwione.

agents-title = Agenci
agents-search = Szukaj agentów ACP i CLI…
agents-empty = Brak pasujących agentów
agents-empty-detail = Spróbuj wpisać nazwę, środowisko uruchomieniowe albo ACP/CLI.
agents-install-failed = Instalacja nie powiodła się
agents-updating = Aktualizowanie…
agents-retrying = Ponawianie…
agents-preparing = Przygotowywanie…

extensions-title = Rozszerzenia
extensions-search = Szukaj zainstalowanych lub w Chrome Web Store…
extensions-relaunch = Uruchom ponownie, aby zastosować
extensions-empty = Nie zainstalowano rozszerzeń
extensions-no-match = Brak pasujących rozszerzeń
extensions-empty-detail = Wyszukaj powyżej w Chrome Web Store i naciśnij Return.
extensions-no-match-detail = Spróbuj wpisać inną nazwę lub ID rozszerzenia.
extensions-on = Wł.
extensions-off = Wył.
extensions-enable-confirm = Włączyć { $name }?
extensions-enable-permissions = Włącz { $name } i zezwól na:

lsp-title = Serwery językowe
lsp-search = Szukaj serwerów językowych, linterów, formaterów…
lsp-loading = Wczytywanie katalogu…
lsp-empty = Brak pasujących serwerów językowych
lsp-empty-detail = Spróbuj wpisać inny język, linter lub formater.
lsp-needs = wymaga { $tool }
lsp-status-available = Dostępne
lsp-status-on-path = W PATH
lsp-status-installing = Instalowanie…
lsp-status-installed = Zainstalowano
lsp-status-outdated = Dostępna aktualizacja
lsp-status-running = Uruchomione
lsp-status-failed = Niepowodzenie

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
team-just-you = W tej przestrzeni jesteś tylko Ty
team-agents = { $count ->
    [one] Ty i 1 agent
   *[other] Ty i { $count } agentów
}
team-empty = Nikogo tu jeszcze nie ma
team-you = Ty
team-agent = Agent

services-title = Usługi w tle
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procesów
}
services-kill-all = Zakończ wszystkie
services-not-running = Usługa nie jest uruchomiona
services-start-with = Uruchom przez:
services-empty = Brak aktywnych procesów
services-filter = Filtruj procesy…
services-no-match = Brak pasujących procesów
services-connected = Połączono
services-disconnected = Rozłączono
services-attached = podłączone
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
history-today = Dzisiaj
history-yesterday = Wczoraj
history-days-ago = { $count } dni temu
history-day-offset = Dzień -{ $count }

settings-title = Ustawienia
settings-loading = Wczytywanie ustawień…
settings-stored = Przechowywane w ~/.vmux/settings.ron
settings-other = Inne
settings-software-update = Aktualizacja oprogramowania
settings-check-updates = Sprawdź aktualizacje
settings-check-updates-hint = Sprawdza automatycznie przy uruchomieniu i co godzinę, gdy automatyczne aktualizacje są włączone.
settings-update-unavailable = Niedostępne
settings-update-unavailable-hint = Ten build nie zawiera aktualizatora.
settings-update-checking = Sprawdzanie…
settings-update-checking-hint = Sprawdzanie aktualizacji…
settings-update-check-again = Sprawdź ponownie
settings-update-current = Vmux jest aktualny.
settings-update-downloading = Pobieranie…
settings-update-downloading-hint = Pobieranie Vmux { $version }…
settings-update-installing = Instalowanie…
settings-update-installing-hint = Instalowanie Vmux { $version }…
settings-update-ready = Aktualizacja gotowa
settings-update-ready-hint = Vmux { $version } jest gotowy. Uruchom ponownie, aby zastosować aktualizację.
settings-update-try-again = Spróbuj ponownie
settings-update-failed = Nie można sprawdzić aktualizacji.
settings-item = Element
settings-item-number = Element { $number }
settings-press-key = Naciśnij klawisz…
settings-saved = Zapisano
settings-record-key = Kliknij, aby zarejestrować nowy skrót klawiszowy

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
layout-bookmark-page = Dodaj tę stronę do zakładek
layout-remove-bookmark = Usuń zakładkę
layout-pin-page = Przypnij tę stronę
layout-unpin-page = Odepnij tę stronę
layout-manage-extensions = Zarządzaj rozszerzeniami
layout-new-stack = Nowy stos
layout-close-tab = Zamknij kartę
layout-bookmark = Zakładka
layout-pin = Przypnij
layout-new-tab = Nowa karta
layout-team = Zespół

command-switch-space = Przełącz przestrzeń…
command-search-ask = Szukaj lub zapytaj…
command-new-tab-placeholder = Szukaj, wpisz URL albo wybierz Terminal…
command-placeholder = Wpisz URL, szukaj kart albo użyj > dla poleceń…
command-composer-placeholder = Wpisz / dla poleceń lub @ dla multimediów
command-send = Wyślij (Enter)
command-terminal = Terminal
command-open-terminal = Otwórz w Terminalu
command-stack = Stos
command-tabs = { $count ->
    [one] 1 karta
   *[other] { $count } kart
}
command-prompt = Prompt
command-new-tab = Nowa karta
command-search = Szukaj
command-open-value = Otwórz „{ $value }”
command-search-value = Szukaj „{ $value }”

schema-appearance = Wygląd
schema-general = Ogólne
schema-layout = Układ
schema-layout-detail = Okno, panele, pasek boczny i obwódka fokusu.
schema-agent = Agent
schema-agent-detail = Zachowanie agenta i uprawnienia narzędzi.
schema-shortcuts = Skróty
schema-shortcuts-detail = Widok tylko do odczytu. Aby zmienić skróty, edytuj bezpośrednio settings.ron.
schema-terminal = Terminal
schema-browser = Przeglądarka
schema-mode = Tryb
schema-mode-detail = Schemat kolorów stron internetowych. Urządzenie podąża za ustawieniami systemu.
schema-device = Urządzenie
schema-light = Jasny
schema-dark = Ciemny
schema-language = Język
schema-language-detail = Użyj ustawień systemu, en-US, ja albo dowolnego tagu BCP 47 z pasującym katalogiem ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Automatyczne aktualizacje
schema-auto-update-detail = Sprawdzaj i instaluj aktualizacje przy uruchomieniu oraz co godzinę.
schema-startup-url = URL startowy
schema-startup-url-detail = Puste pole otwiera prompt paska poleceń.
schema-search-engine = Wyszukiwarka
schema-search-engine-detail = Używana do wyszukiwania w sieci ze Startu i z paska poleceń.
schema-window = Okno
schema-pane = Panel
schema-side-sheet = Panel boczny
schema-focus-ring = Obwódka fokusu
schema-run-placement = Zezwalaj na nadpisanie miejsca uruchomienia
schema-run-placement-detail = Pozwól agentom wybierać tryb panelu uruchamiania, kierunek i punkt zaczepienia.
schema-leader = Leader
schema-leader-detail = Klawisz prefiksu dla skrótów chord.
schema-chord-timeout = Limit czasu chorda
schema-chord-timeout-detail = Liczba milisekund, po których prefiks chorda wygasa.
schema-bindings = Skróty
schema-confirm-close = Potwierdzaj zamknięcie
schema-confirm-close-detail = Pytaj przed zamknięciem terminala z uruchomionym procesem.
schema-default-theme = Motyw domyślny
schema-default-theme-detail = Nazwa aktywnego motywu z listy motywów.
