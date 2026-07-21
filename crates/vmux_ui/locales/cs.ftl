locale-name = čeština
common-open = Otevřít
common-close = Zavřít
common-install = Nainstalovat
common-uninstall = Odinstalovat
common-update = Aktualizovat
common-retry = Zkusit znovu
common-refresh = Obnovit
common-remove = Odebrat
common-enable = Zapnout
common-disable = Vypnout
common-new = Nový
common-active = aktivní
common-running = běží
common-done = hotovo
common-failed = Selhalo
common-installed = Nainstalováno
common-items = { $count ->
    [one] { $count } položka
   *[other] { $count } položek
}
start-title = Start
start-tagline = Jeden prompt. Hotovo může být cokoli.

agents-title = Agenti
agents-search = Hledat agenty ACP a CLI…
agents-empty = Žádní odpovídající agenti
agents-empty-detail = Zkuste název, běhové prostředí nebo ACP/CLI.
agents-install-failed = Instalace selhala
agents-updating = Aktualizuje se…
agents-retrying = Zkouší se znovu…
agents-preparing = Připravuje se…

extensions-title = Rozšíření
extensions-search = Hledat nainstalovaná nebo v Chrome Web Store…
extensions-relaunch = Pro použití změn restartujte
extensions-empty = Nejsou nainstalovaná žádná rozšíření
extensions-no-match = Žádná odpovídající rozšíření
extensions-empty-detail = Vyhledejte rozšíření v Chrome Web Store výše a stiskněte Enter.
extensions-no-match-detail = Zkuste jiný název nebo ID rozšíření.
extensions-on = Zapnuto
extensions-off = Vypnuto
extensions-enable-confirm = Zapnout { $name }?
extensions-enable-permissions = Zapnout { $name } a povolit:

lsp-title = Jazykové servery
lsp-search = Hledat jazykové servery, lintery, formátovače…
lsp-loading = Načítá se katalog…
lsp-empty = Žádné odpovídající jazykové servery
lsp-empty-detail = Zkuste jiný jazyk, linter nebo formátovač.
lsp-needs = vyžaduje { $tool }
lsp-status-available = Dostupné
lsp-status-on-path = Na PATH
lsp-status-installing = Instaluje se…
lsp-status-installed = Nainstalováno
lsp-status-outdated = Je dostupná aktualizace
lsp-status-running = Běží
lsp-status-failed = Selhalo

spaces-title = Prostory
spaces-new-placeholder = Název nového prostoru
spaces-empty = Žádné prostory
spaces-default-name = Prostor { $number }
spaces-tabs = { $count ->
    [one] 1 karta
   *[other] { $count } karet
}
spaces-delete = Smazat prostor

team-title = Tým
team-just-you = V tomto prostoru jste jen vy
team-agents = { $count ->
    [one] Vy a 1 agent
   *[other] Vy a { $count } agentů
}
team-empty = Zatím tu nikdo není
team-you = Vy
team-agent = Agent

services-title = Služby na pozadí
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procesů
}
services-kill-all = Vynutit ukončení všech
services-not-running = Služba neběží
services-start-with = Spustit pomocí:
services-empty = Žádné aktivní procesy
services-filter = Filtrovat procesy…
services-no-match = Žádné odpovídající procesy
services-connected = Připojeno
services-disconnected = Odpojeno
services-attached = připojeno
services-kill = Vynutit ukončení
services-memory = Paměť
services-size = Velikost
services-shell = Shell

error-title = Chyba

history-search = Hledat v historii
history-clear-all = Vymazat vše
history-clear-confirm = Vymazat celou historii?
history-clear-warning = Tuto akci nelze vrátit zpět.
history-cancel = Zrušit
history-today = Dnes
history-yesterday = Včera
history-days-ago = před { $count } dny
history-day-offset = Den -{ $count }

settings-title = Nastavení
settings-loading = Načítá se nastavení…
settings-stored = Uloženo v ~/.vmux/settings.ron
settings-other = Ostatní
settings-software-update = Aktualizace softwaru
settings-check-updates = Vyhledat aktualizace
settings-check-updates-hint = Při zapnutých automatických aktualizacích se kontroluje při spuštění a každou hodinu.
settings-update-unavailable = Nedostupné
settings-update-unavailable-hint = V této sestavě není aktualizační nástroj zahrnutý.
settings-update-checking = Kontroluje se…
settings-update-checking-hint = Kontrolují se aktualizace…
settings-update-check-again = Zkontrolovat znovu
settings-update-current = Vmux je aktuální.
settings-update-downloading = Stahuje se…
settings-update-downloading-hint = Stahuje se Vmux { $version }…
settings-update-installing = Instaluje se…
settings-update-installing-hint = Instaluje se Vmux { $version }…
settings-update-ready = Aktualizace připravena
settings-update-ready-hint = Vmux { $version } je připravený. Pro použití aktualizace restartujte.
settings-update-try-again = Zkusit znovu
settings-update-failed = Aktualizace se nepodařilo zkontrolovat.
settings-item = Položka
settings-item-number = Položka { $number }
settings-press-key = Stiskněte klávesu…
settings-saved = Uloženo
settings-record-key = Kliknutím nahrajete novou klávesovou zkratku

tray-open-window = Otevřít okno
tray-close-window = Zavřít okno
tray-pause-recording = Pozastavit nahrávání
tray-resume-recording = Pokračovat v nahrávání
tray-finish-recording = Dokončit nahrávání
tray-quit = Ukončit Vmux

composer-attach-files = Přiložit soubory (/upload)
composer-remove-attachment = Odebrat přílohu

layout-back = Zpět
layout-forward = Vpřed
layout-reload = Načíst znovu
layout-bookmark-page = Přidat tuto stránku do záložek
layout-remove-bookmark = Odebrat záložku
layout-pin-page = Připnout tuto stránku
layout-unpin-page = Odepnout tuto stránku
layout-manage-extensions = Spravovat rozšíření
layout-new-stack = Nová vrstva
layout-close-tab = Zavřít kartu
layout-bookmark = Záložka
layout-pin = Připnout
layout-new-tab = Nová karta
layout-team = Tým

command-switch-space = Přepnout prostor…
command-search-ask = Hledat nebo se zeptat…
command-new-tab-placeholder = Hledejte, zadejte URL nebo vyberte Terminál…
command-placeholder = Zadejte URL, hledejte karty nebo použijte > pro příkazy…
command-composer-placeholder = Zadejte / pro příkazy nebo @ pro média
command-send = Odeslat (Enter)
command-terminal = Terminál
command-open-terminal = Otevřít v Terminálu
command-stack = Vrstva
command-tabs = { $count ->
    [one] 1 karta
   *[other] { $count } karet
}
command-prompt = Prompt
command-new-tab = Nová karta
command-search = Hledat
command-open-value = Otevřít „{ $value }“
command-search-value = Hledat „{ $value }“

schema-appearance = Vzhled
schema-general = Obecné
schema-layout = Rozložení
schema-layout-detail = Okno, panely, postranní lišta a zvýraznění fokusu.
schema-agent = Agent
schema-agent-detail = Chování agenta a oprávnění k nástrojům.
schema-shortcuts = Zkratky
schema-shortcuts-detail = Jen pro čtení. Vazby změníte přímo v settings.ron.
schema-terminal = Terminál
schema-browser = Prohlížeč
schema-mode = Režim
schema-mode-detail = Barevné schéma webových stránek. Zařízení se řídí systémem.
schema-device = Zařízení
schema-light = Světlý
schema-dark = Tmavý
schema-language = Jazyk
schema-language-detail = Použijte systémový jazyk, en-US, ja nebo libovolný tag BCP 47 s odpovídajícím katalogem ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Automatické aktualizace
schema-auto-update-detail = Kontrolovat a instalovat aktualizace při spuštění a každou hodinu.
schema-startup-url = Úvodní URL
schema-startup-url-detail = Prázdná hodnota otevře prompt v příkazové liště.
schema-search-engine = Vyhledávač
schema-search-engine-detail = Používá se pro webové hledání ze Startu a příkazové lišty.
schema-window = Okno
schema-pane = Panel
schema-side-sheet = Postranní panel
schema-focus-ring = Zvýraznění fokusu
schema-run-placement = Povolit přepsání umístění běhu
schema-run-placement-detail = Umožnit agentům vybrat režim, směr a ukotvení panelu pro běh.
schema-leader = Leader
schema-leader-detail = Prefixová klávesa pro chord zkratky.
schema-chord-timeout = Časový limit chordu
schema-chord-timeout-detail = Počet milisekund, než vyprší prefix chordu.
schema-bindings = Vazby
schema-confirm-close = Potvrdit zavření
schema-confirm-close-detail = Zeptat se před zavřením terminálu se spuštěným procesem.
schema-default-theme = Výchozí motiv
schema-default-theme-detail = Název aktivního motivu ze seznamu motivů.
