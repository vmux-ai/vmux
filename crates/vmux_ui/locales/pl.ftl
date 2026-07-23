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

tools-title = Narzędzia
tools-search = Szukaj pakietów, agentów, MCP, narzędzi językowych i plików konfiguracyjnych…
tools-open = Otwórz Narzędzia
tools-fold = Zwiń narzędzia
tools-unfold = Rozwiń narzędzia
tools-scanning = Skanowanie lokalnych narzędzi…
tools-no-installed = Brak zainstalowanych narzędzi
tools-empty = Brak pasujących narzędzi
tools-empty-detail = Zainstaluj pakiet lub dodaj pakiet plików konfiguracyjnych w stylu Stow.
tools-apply = Zastosuj
tools-homebrew = Homebrew
tools-homebrew-sync = Zainstalowane formuły i aplikacje synchronizują się automatycznie.
tools-open-brewfile = Otwórz Brewfile
tools-managed = zarządzane
tools-provider-homebrew-formulae = Formuły Homebrew
tools-provider-homebrew-casks = Aplikacje Homebrew
tools-provider-npm = Pakiety npm
tools-provider-acp-agents = Agenci ACP
tools-provider-language-tools = Narzędzia językowe
tools-provider-mcp-servers = Serwery MCP
tools-provider-dotfiles = Pliki konfiguracyjne
tools-status-available = Dostępne
tools-status-missing = Brakujące
tools-status-conflict = Konflikt
tools-forget = Zapomnij
tools-manage = Zarządzaj
tools-link = Połącz
tools-unlink = Rozłącz
tools-import = Importuj
tools-update-count = { $count ->
    [one] 1 aktualizacja
   *[other] { $count } aktualizacji
}
tools-conflict-count = { $count ->
    [one] 1 konflikt
   *[other] { $count } konfliktów
}
tools-result-applied = Narzędzia zastosowane
tools-result-imported = Narzędzia zaimportowane
tools-result-installed = Zainstalowano { $name }
tools-result-updated = Zaktualizowano { $name }
tools-result-uninstalled = Odinstalowano { $name }
tools-result-forgotten = Zapomniano { $name }
tools-result-managed = { $name } jest teraz zarządzane
tools-result-linked = Połączono { $name }
tools-result-unlinked = Rozłączono { $name }
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Synchronizuj ustawienia, narzędzia, pliki dot i wiedzę z Git.
vault-sync = Synchronizuj
vault-create = Tworzyć
vault-connect = Łączyć
vault-private = Prywatne repozytorium
vault-public-warning = Publiczne repozytoria ujawniają Twoją wiedzę i konfigurację.
vault-choose-repository = Wybierz repozytorium…
vault-empty = pusty
vault-clean = Aktualne
vault-not-connected = Nie podłączony
vault-change-count = Zmiany: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

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

settings-empty = (puste)
settings-none = (brak)

schema-system = System
schema-editor = Edytor
schema-recording = Nagrywanie
schema-radius = Promień
schema-padding = Dopełnienie
schema-gap = Odstęp
schema-width = Szerokość
schema-color = Kolor
schema-red = Czerwony
schema-green = Zielony
schema-blue = Niebieski
schema-follow-files = Śledź pliki
schema-tidy-files = Porządkuj pliki
schema-tidy-files-max = Próg porządkowania plików
schema-tidy-files-auto = Porządkuj pliki automatycznie
schema-app-providers = Dostawcy aplikacji
schema-provider = Dostawca
schema-kind = Typ
schema-models = Modele
schema-acp = Agenci ACP
schema-id = ID
schema-name = Nazwa
schema-command = Polecenie
schema-arguments = Argumenty
schema-environment = Zmienne środowiskowe
schema-working-directory = Katalog roboczy
schema-shell = Powłoka
schema-font-family = Rodzina czcionek
schema-startup-directory = Katalog startowy
schema-themes = Motywy
schema-color-scheme = Schemat kolorów
schema-font-size = Rozmiar czcionki
schema-line-height = Interlinia
schema-cursor-style = Styl kursora
schema-cursor-blink = Miganie kursora
schema-custom-themes = Motywy niestandardowe
schema-foreground = Pierwszy plan
schema-background = Tło
schema-cursor = Kursor
schema-ansi-colors = Kolory ANSI
schema-keymap = Mapa klawiszy
schema-explorer = Eksplorator
schema-visible = Widoczny
schema-language-servers = Serwery językowe
schema-servers = Serwery
schema-language-id = ID języka
schema-root-markers = Znaczniki katalogu głównego
schema-output-directory = Katalog wyjściowy

menu-scene = Scena
menu-layout = Układ
menu-terminal = Terminal
menu-browser = Przeglądarka
menu-service = Usługa
menu-bookmark = Zakładka
menu-edit = Edycja

layout-knowledge = Wiedza
layout-open-knowledge = Otwórz Wiedzę
layout-open-welcome-knowledge = Otwórz „Witamy w Wiedzy”
layout-open-path = Otwórz { $path }
layout-fold-knowledge = Zwiń Wiedzę
layout-unfold-knowledge = Rozwiń Wiedzę
layout-bookmarks = Zakładki
layout-new-folder = Nowy folder
layout-add-to-bookmarks = Dodaj do zakładek
layout-move-to-bookmarks = Przenieś do zakładek
layout-stack-number = Stos { $number }
layout-fold-stack = Zwiń stos
layout-unfold-stack = Rozwiń stos
layout-close-stack = Zamknij stos
layout-bookmark-in = Dodaj zakładkę w { $folder }

common-cancel = Anuluj
common-delete = Usuń
common-save = Zapisz
common-rename = Zmień nazwę
common-expand = Rozwiń
common-collapse = Zwiń
common-loading = Wczytywanie…
common-error = Błąd
common-output = Wynik
common-pending = Oczekuje
common-current = bieżący
common-stop = Zatrzymaj
services-command = Usługa Vmux
services-uptime-seconds = { $seconds } s
services-uptime-minutes = { $minutes } min { $seconds } s
services-uptime-hours = { $hours } godz. { $minutes } min
services-uptime-days = { $days } d { $hours } godz.

error-page-failed-load = Nie udało się wczytać strony
error-page-not-found = Nie znaleziono strony
error-unknown-host = Nieznany host aplikacji Vmux: { $host }

history-title = Historia

command-new-app-chat = Nowy czat { $provider }/{ $model } (aplikacja)
command-interactive-mode-user = Scena > Tryb interaktywny > Użytkownik
command-interactive-mode-player = Scena > Tryb interaktywny > Odtwarzacz
command-minimize-window = Układ > Okno > Minimalizuj
command-toggle-layout = Układ > Układ > Przełącz układ
command-close-tab = Układ > Karta > Zamknij kartę
command-new-task = Układ > Karta > Nowe zadanie…
command-next-tab = Układ > Karta > Następna karta
command-prev-tab = Układ > Karta > Poprzednia karta
command-rename-tab = Układ > Karta > Zmień nazwę karty
command-tab-select-1 = Układ > Karta > Wybierz kartę 1
command-tab-select-2 = Układ > Karta > Wybierz kartę 2
command-tab-select-3 = Układ > Karta > Wybierz kartę 3
command-tab-select-4 = Układ > Karta > Wybierz kartę 4
command-tab-select-5 = Układ > Karta > Wybierz kartę 5
command-tab-select-6 = Układ > Karta > Wybierz kartę 6
command-tab-select-7 = Układ > Karta > Wybierz kartę 7
command-tab-select-8 = Układ > Karta > Wybierz kartę 8
command-tab-select-last = Układ > Karta > Wybierz ostatnią kartę
command-close-pane = Układ > Panel > Zamknij panel
command-select-pane-left = Układ > Panel > Wybierz lewy panel
command-select-pane-right = Układ > Panel > Wybierz prawy panel
command-select-pane-up = Układ > Panel > Wybierz panel wyżej
command-select-pane-down = Układ > Panel > Wybierz panel niżej
command-swap-pane-prev = Układ > Panel > Zamień z poprzednim panelem
command-swap-pane-next = Układ > Panel > Zamień z następnym panelem
command-equalize-pane-size = Układ > Panel > Wyrównaj rozmiary paneli
command-resize-pane-left = Układ > Panel > Zmień rozmiar panelu w lewo
command-resize-pane-right = Układ > Panel > Zmień rozmiar panelu w prawo
command-resize-pane-up = Układ > Panel > Zmień rozmiar panelu w górę
command-resize-pane-down = Układ > Panel > Zmień rozmiar panelu w dół
command-stack-close = Układ > Stos > Zamknij stos
command-stack-next = Układ > Stos > Następny stos
command-stack-previous = Układ > Stos > Poprzedni stos
command-stack-reopen = Układ > Stos > Otwórz ponownie zamkniętą stronę
command-stack-swap-prev = Układ > Stos > Przenieś stos w lewo
command-stack-swap-next = Układ > Stos > Przenieś stos w prawo
command-space-open = Układ > Obszar > Obszary
command-terminal-close = Terminal > Zamknij terminal
command-terminal-next = Terminal > Następny terminal
command-terminal-prev = Terminal > Poprzedni terminal
command-terminal-clear = Terminal > Wyczyść terminal
command-browser-prev-page = Przeglądarka > Nawigacja > Wstecz
command-browser-next-page = Przeglądarka > Nawigacja > Dalej
command-browser-reload = Przeglądarka > Nawigacja > Odśwież
command-browser-hard-reload = Przeglądarka > Nawigacja > Odśwież bez pamięci podręcznej
command-open-in-place = Przeglądarka > Otwórz > Otwórz tutaj
command-open-in-new-stack = Przeglądarka > Otwórz > Otwórz w nowym stosie
command-open-in-pane-top = Przeglądarka > Otwórz > Otwórz w panelu powyżej
command-open-in-pane-right = Przeglądarka > Otwórz > Otwórz w panelu po prawej
command-open-in-pane-bottom = Przeglądarka > Otwórz > Otwórz w panelu poniżej
command-open-in-pane-left = Przeglądarka > Otwórz > Otwórz w panelu po lewej
command-open-in-new-tab = Przeglądarka > Otwórz > Otwórz w nowej karcie
command-open-in-new-space = Przeglądarka > Otwórz > Otwórz w nowym obszarze
command-browser-zoom-in = Przeglądarka > Widok > Powiększ
command-browser-zoom-out = Przeglądarka > Widok > Pomniejsz
command-browser-zoom-reset = Przeglądarka > Widok > Rzeczywisty rozmiar
command-browser-dev-tools = Przeglądarka > Widok > Narzędzia deweloperskie
command-browser-open-command-bar = Przeglądarka > Pasek > Pasek poleceń
command-browser-open-page-in-command-bar = Przeglądarka > Pasek > Edytuj stronę
command-browser-open-path-bar = Przeglądarka > Pasek > Nawigator ścieżek
command-browser-open-commands = Przeglądarka > Pasek > Polecenia
command-browser-open-history = Przeglądarka > Pasek > Historia
command-service-open = Usługa > Otwórz monitor usług
command-bookmark-toggle-active = Zakładka > Dodaj stronę do zakładek
command-bookmark-pin-active = Zakładka > Przypnij stronę

layout-tab = Karta
layout-no-stacks = Brak stosów
layout-loading = Wczytywanie…
layout-no-markdown-files = Brak plików Markdown
layout-empty-folder = Pusty folder
layout-worktree = drzewo robocze
layout-folder-name = Nazwa folderu
layout-no-pins-bookmarks = Brak przypięć i zakładek
layout-move-to = Przenieś do { $folder }
layout-bookmark-current-page = Dodaj bieżącą stronę do zakładek
layout-rename-folder = Zmień nazwę folderu
layout-remove-folder = Usuń folder
layout-update-downloading = Pobieranie aktualizacji
layout-update-installing = Instalowanie aktualizacji…
layout-update-ready = Dostępna jest nowa wersja
layout-restart-update = Uruchom ponownie, aby zaktualizować

agent-preparing = Przygotowywanie agenta…
agent-send-all-queued = Wyślij teraz wszystkie prompty z kolejki (Esc)
agent-send = Wyślij (Enter)
agent-ready = Gotowe — możesz zaczynać.
agent-loading-older = Wczytywanie starszych wiadomości…
agent-load-older = Wczytaj starsze wiadomości
agent-continued-from = Kontynuacja z { $source }
agent-older-context-omitted = pominięto starszy kontekst
agent-interrupted = przerwano
agent-allow-tool = Zezwolić na { $tool }?
agent-deny = Odmów
agent-allow-always = Zawsze zezwalaj
agent-allow = Zezwól
agent-loading-sessions = Wczytywanie sesji…
agent-no-resumable-sessions = Nie znaleziono sesji do wznowienia
agent-no-matching-sessions = Brak pasujących sesji
agent-no-matching-models = Brak pasujących modeli
agent-choice-help = ↑/↓ lub Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Wybierz folder repozytorium
agent-choose-repository-detail = Wybierz lokalne repozytorium Git, którego ma używać agent.
agent-choosing = Wybieranie…
agent-choose-folder = Wybierz folder
agent-queued = w kolejce
agent-attached = Załączono:
agent-cancel-queued = Anuluj prompt z kolejki
agent-resume-queued = Wznów prompty z kolejki
agent-clear-queue = Wyczyść kolejkę
agent-send-all-now = wyślij wszystko teraz
agent-choose-option = Wybierz opcję powyżej
agent-loading-media = Wczytywanie multimediów…
agent-no-matching-media = Brak pasujących multimediów
agent-prompt-context = Kontekst promptu
agent-details = Szczegóły
agent-path = Ścieżka
agent-tool = Narzędzie
agent-server = Serwer
agent-bytes = { $count } bajtów
agent-worked-for = Pracował przez { $duration }
agent-worked-for-steps = { $count ->
    [one] Pracował przez { $duration } · 1 krok
   *[other] Pracował przez { $duration } · { $count } kroków
}
agent-tool-guardian-review = Przegląd ochronny
agent-tool-read-files = Odczytano pliki
agent-tool-viewed-image = Wyświetlono obraz
agent-tool-used-browser = Użyto przeglądarki
agent-tool-searched-files = Przeszukano pliki
agent-tool-ran-commands = Uruchomiono polecenia
agent-thinking = Myśli
agent-subagent = Podagent
agent-prompt = Prompt
agent-thread = Wątek
agent-parent = Nadrzędny
agent-children = Podrzędne
agent-call = Wywołanie
agent-raw-event = Surowe zdarzenie
agent-plan = Plan
agent-tasks = { $count ->
    [one] 1 zadanie
   *[other] { $count } zadań
}
agent-edited = Edytowano
agent-reconnecting = Ponowne łączenie { $attempt }/{ $total }
agent-status-running = Działa
agent-status-done = Gotowe
agent-status-failed = Niepowodzenie
agent-status-pending = Oczekuje
agent-slash-attach-files = Załącz pliki
agent-slash-resume-session = Wznów poprzednią sesję
agent-slash-select-model = Wybierz model
agent-slash-continue-cli = Kontynuuj tę sesję w CLI
agent-session-just-now = przed chwilą
agent-session-minutes-ago = { $count } min temu
agent-session-hours-ago = { $count } godz. temu
agent-session-days-ago = { $count } d temu
agent-working-working = Pracuje
agent-working-thinking = Myśli
agent-working-pondering = Rozważa
agent-working-noodling = Kombinuje
agent-working-percolating = Przetwarza
agent-working-conjuring = Wyczarowuje
agent-working-cooking = Warzy
agent-working-brewing = Parzy
agent-working-musing = Duma
agent-working-ruminating = Rozmyśla
agent-working-scheming = Knuje
agent-working-synthesizing = Syntetyzuje
agent-working-tinkering = Majsterkuje
agent-working-churning = Mieli
agent-working-vibing = Łapie flow
agent-working-simmering = Gotuje na wolnym ogniu
agent-working-crafting = Tworzy
agent-working-divining = Wróży
agent-working-mulling = Waży pomysły
agent-working-spelunking = Eksploruje

editor-toggle-explorer = Przełącz Eksplorator (Cmd+B)
editor-unsaved = niezapisane
editor-rendered-markdown = Wyrenderowany Markdown z edycją na żywo
editor-note = Notatka
editor-source-editor = Edytor źródła
editor-editor = Edytor
editor-git-diff = Diff Git
editor-diff = Diff
editor-tidy = Porządkuj
editor-always = Zawsze
editor-unchanged-previews = { $count ->
    [one] ✦ 1 niezmieniony podgląd
   *[other] ✦ { $count } niezmienionych podglądów
}
editor-open-externally = Otwórz zewnętrznie
editor-changed-line = Zmieniony wiersz
editor-go-to-definition = Przejdź do definicji
editor-find-references = Znajdź odwołania
editor-references = { $count ->
    [one] 1 odwołanie
   *[other] { $count } odwołań
}
editor-lsp-starting = Uruchamianie { $server }…
editor-lsp-not-installed = { $server } — nie zainstalowano
editor-explorer = Eksplorator
editor-open-editors = Otwarte edytory
editor-outline = Konspekt
editor-new-file = Nowy plik
editor-new-folder = Nowy folder
editor-delete-confirm = Usunąć „{ $name }”? Tej operacji nie można cofnąć.
editor-created-folder = Utworzono folder { $name }
editor-created-file = Utworzono plik { $name }
editor-renamed-to = Zmieniono nazwę na { $name }
editor-deleted = Usunięto { $name }
editor-failed-decode-image = Nie udało się zdekodować obrazu
editor-preview-large-image = obraz (za duży do podglądu)
editor-preview-binary = plik binarny
editor-preview-file = plik

git-status-clean = czysto
git-status-modified = zmodyfikowano
git-status-staged = w poczekalni
git-status-staged-modified = w poczekalni*
git-status-untracked = nieśledzone
git-status-deleted = usunięto
git-status-conflict = konflikt
git-accept-all = ✓ zaakceptuj wszystko
git-unstage = Usuń z poczekalni
git-confirm-deny-all = Potwierdź odrzucenie wszystkiego
git-deny-all = ✗ odrzuć wszystko
git-commit-message = komunikat commita
git-commit = Commit ({ $count })
git-push = ↑ Wypchnij
git-loading-diff = Wczytywanie diffu…
git-no-changes = Brak zmian do pokazania
git-accept = ✓ zaakceptuj
git-deny = ✗ odrzuć
git-show-unchanged-lines = Pokaż { $count } niezmienionych wierszy

terminal-loading = Wczytywanie…
terminal-runs-when-ready = uruchomi się, gdy będzie gotowe · Ctrl+C czyści · Esc pomija
terminal-booting = uruchamianie
terminal-type-command = wpisz polecenie · uruchomi się, gdy będzie gotowe · Esc pomija

setup-tagline-claude = Agent kodujący Anthropic w Vmux
setup-tagline-codex = Agent kodujący OpenAI w Vmux
setup-tagline-vibe = Agent kodujący Mistral w Vmux
setup-install-title = Zainstaluj CLI { $name }
setup-homebrew-required = Do zainstalowania { $command } wymagany jest Homebrew, który nie jest jeszcze skonfigurowany. Vmux najpierw zainstaluje Homebrew, a potem { $name }.
setup-terminal-instructions = W terminalu naciśnij Return, aby rozpocząć, a następnie wpisz hasło do Maca, gdy pojawi się prośba.
setup-command-missing = Vmux otworzył tę stronę, ponieważ lokalne polecenie { $command } nie jest jeszcze zainstalowane. Uruchom poniższe polecenie, aby je pobrać.
setup-install-failed = Instalacja nie została ukończona. Sprawdź szczegóły w terminalu i spróbuj ponownie.
setup-installing = Instalowanie…
setup-install-homebrew = Zainstaluj Homebrew + { $name }
setup-run-install = Uruchom polecenie instalacji
setup-auto-reload = Vmux uruchomi je w terminalu i odświeży stronę, gdy { $command } będzie gotowe.

debug-title = Debugowanie
debug-auto-update = Automatyczne aktualizacje
debug-simulate-update = Symuluj dostępną aktualizację
debug-simulate-download = Symuluj pobieranie
debug-clear-update = Wyczyść aktualizację
debug-trigger-restart = Wywołaj ponowne uruchomienie

command-manage-spaces = Zarządzaj przestrzeniami…
command-pane-stack-location = panel { $pane } / stos { $stack }
command-space-pane-stack-location = { $space } / panel { $pane } / stos { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Tryb interaktywny
command-group-window = Okno
command-group-tab = Karta
command-group-pane = Panel
command-group-stack = Stos
command-group-space = Przestrzeń
command-group-navigation = Nawigacja
command-group-open = Otwórz
command-group-view = Widok
command-group-bar = Pasek

menu-close-vmux = Zamknij Vmux

agents-terminal-coding-agent = Agent programujący w terminalu
