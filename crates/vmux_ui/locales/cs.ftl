common-open = Otevřít
common-close = Zavřít
common-install = Instalovat
common-uninstall = Odinstalovat
common-update = Aktualizovat
common-retry = Zkuste to znovu
common-refresh = Obnovit
common-remove = Odebrat
common-enable = Povolit
common-disable = Zakázat
common-new = Nové
common-active = aktivní
common-running = běžící
common-done = hotovo
common-failed = Nepodařilo se
common-installed = Instalováno
common-items = { $count ->
    [one] { $count } položka
   *[other] { $count } položek
}
start-title = Začněte
start-tagline = Jedna výzva. Cokoli, hotovo.

agents-title = Agenti
agents-search = Hledat agenty ACP a CLI…
agents-empty = Žádní odpovídající agenti
agents-empty-detail = Zkuste název, běhové prostředí nebo ACP/CLI.
agents-install-failed = Instalace se nezdařila
agents-updating = Aktualizace…
agents-retrying = Opakování…
agents-preparing = Příprava…

extensions-title = Rozšíření
extensions-search = Hledání nainstalováno nebo Chrome Web Store…
extensions-relaunch = Chcete-li použít, znovu spusťte
extensions-empty = Nejsou nainstalována žádná rozšíření
extensions-no-match = Žádná odpovídající rozšíření
extensions-empty-detail = Vyhledejte Chrome Web Store výše a stiskněte Return.
extensions-no-match-detail = Zkuste jiné jméno nebo ID rozšíření.
extensions-on = Zapnuto
extensions-off = Vypnuto
extensions-enable-confirm = Povolit { $name }?
extensions-enable-permissions = Povolit { $name } a povolit:

lsp-title = Jazykové servery
lsp-search = Prohledejte jazykové servery, lintry, formátovače…
lsp-loading = Načítání katalogu…
lsp-empty = Žádné odpovídající jazykové servery
lsp-empty-detail = Zkuste jiný jazyk, linter nebo formátovač.
lsp-needs = potřebuje { $tool }
lsp-status-available = K dispozici
lsp-status-on-path = Na PATH
lsp-status-installing = Instalace…
lsp-status-installed = Instalováno
lsp-status-outdated = Aktualizace k dispozici
lsp-status-running = Běh
lsp-status-failed = Nepodařilo se

spaces-title = Prostory
spaces-new-placeholder = Nový název prostoru
spaces-empty = Žádné mezery
spaces-default-name = Prostor { $number }
spaces-tabs = { $count ->
    [one] 1 karta
   *[other] { $count } karet
}
spaces-delete = Smazat prostor

team-title = Tým
team-just-you = Jen vy v tomto prostoru
team-agents = { $count ->
    [one] Vy a 1 agent
   *[other] Vy a { $count } agenti
}
team-empty = Tady ještě nikdo
team-you = vy
team-agent = Agent

services-title = Služby na pozadí
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procesů
}
services-kill-all = Zabít všechny
services-not-running = Služba neběží
services-start-with = Začněte s:
services-empty = Žádné aktivní procesy
services-filter = Filtrovat procesy…
services-no-match = Žádné odpovídající procesy
services-connected = Připojeno
services-disconnected = Odpojeno
services-attached = připojeno
services-kill = Zabít
services-memory = Paměť
services-size = Velikost
services-shell = Shell

error-title = Chyba

history-search = Historie vyhledávání
history-clear-all = Vymazat vše
history-clear-confirm = Vymazat celou historii?
history-clear-warning = Toto nelze vrátit zpět.
history-cancel = Zrušit
history-today = dnes
history-yesterday = včera
history-days-ago = před { $count } dny
history-day-offset = Den -{ $count }

settings-title = Nastavení
settings-loading = Načítání nastavení…
settings-stored = Uloženo v ~/.vmux/settings.ron
settings-other = Jiné
settings-software-update = Aktualizace softwaru
settings-check-updates = Zkontrolujte aktualizace
settings-check-updates-hint = Kontroluje se automaticky při spuštění a každou hodinu, když je povolena automatická aktualizace.
settings-update-unavailable = Není k dispozici
settings-update-unavailable-hint = Updater není součástí tohoto sestavení.
settings-update-checking = Kontrola…
settings-update-checking-hint = Kontrola aktualizací…
settings-update-check-again = Zkontrolujte znovu
settings-update-current = Vmux je aktuální.
settings-update-downloading = Stahování…
settings-update-downloading-hint = Stahování Vmux { $version }…
settings-update-installing = Instalace…
settings-update-installing-hint = Instalace Vmux { $version }…
settings-update-ready = Aktualizace připravena
settings-update-ready-hint = Vmux { $version } je připraveno. Chcete-li jej použít, restartujte jej.
settings-update-try-again = Zkuste to znovu
settings-update-failed = Nelze zkontrolovat aktualizace.
settings-item = Položka
settings-item-number = Položka { $number }
settings-press-key = Stiskněte klávesu…
settings-saved = Uloženo
settings-record-key = Kliknutím nahrajete novou kombinaci kláves

tray-open-window = Otevřete okno
tray-close-window = Zavřít okno
tray-pause-recording = Pozastavit nahrávání
tray-resume-recording = Obnovit nahrávání
tray-finish-recording = Dokončete nahrávání
tray-quit = Ukončit Vmux

composer-attach-files = Připojit soubory (/upload)
composer-remove-attachment = Odstraňte přílohu

layout-back = Zpět
layout-forward = Vpřed
layout-reload = Znovu načíst
layout-bookmark-page = Přidat tuto stránku do záložek
layout-remove-bookmark = Odebrat záložku
layout-pin-page = Připnout tuto stránku
layout-unpin-page = Odepnout tuto stránku
layout-manage-extensions = Správa rozšíření
layout-new-stack = Nový zásobník
layout-close-tab = Zavřít kartu
layout-bookmark = Záložka
layout-pin = Pin
layout-new-tab = Nová karta
layout-team = Tým

command-switch-space = Přepnout prostor…
command-search-ask = Hledej nebo se zeptej…
command-new-tab-placeholder = Vyhledejte nebo zadejte URL nebo vyberte Terminál…
command-placeholder = Zadejte URL, vyhledávací karty nebo > pro příkazy…
command-composer-placeholder = Zadejte / pro příkazy nebo @ pro média
command-send = Odeslat (Enter)
command-terminal = Terminál
command-open-terminal = Otevřít v Terminálu
command-stack = Zásobník
command-tabs = { $count ->
    [one] 1 karta
   *[other] { $count } karet
}
command-prompt = Výzva
command-new-tab = Nová karta
command-search = Hledat
command-open-value = Otevřít „{ $value }“
command-search-value = Hledat „{ $value }“

schema-appearance = Vzhled
schema-general = Generál
schema-layout = Rozložení
schema-layout-detail = Okno, tabule, boční panel a zaostřovací kroužek.
schema-agent = Agent
schema-agent-detail = Chování agenta a oprávnění nástrojů.
schema-shortcuts = Zkratky
schema-shortcuts-detail = Zobrazení pouze pro čtení. Chcete-li změnit vazby, přímo upravte settings.ron.
schema-terminal = Terminál
schema-browser = Prohlížeč
schema-mode = Režim
schema-mode-detail = Barevné schéma pro webové stránky. Zařízení následuje váš systém.
schema-device = Zařízení
schema-light = Světlo
schema-dark = Tmavý
schema-language = Jazyk
schema-language-detail = Použijte systém, en-US, ja nebo jakoukoli značku BCP 47 s odpovídajícím katalogem ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Automatická aktualizace
schema-auto-update-detail = Zkontrolujte a nainstalujte aktualizace při spuštění a každou hodinu.
schema-startup-url = Spuštění URL
schema-startup-url-detail = Prázdné otevře řádek příkazového řádku.
schema-search-engine = Vyhledávač
schema-search-engine-detail = Používá se pro vyhledávání na webu ze Start a příkazového řádku.
schema-window = okno
schema-pane = Podokno
schema-side-sheet = Boční list
schema-focus-ring = Zaostřovací kroužek
schema-run-placement = Povolit přepsání umístění spuštění
schema-run-placement-detail = Nechte agenty vybrat režim podokna spuštění, směr a ukotvení.
schema-leader = vůdce
schema-leader-detail = Předpona pro akordové zkratky.
schema-chord-timeout = Časový limit akordu
schema-chord-timeout-detail = Milisekundy před vypršením předpony akordu.
schema-bindings = Vazby
schema-confirm-close = Potvrďte uzavření
schema-confirm-close-detail = Dotázat se před uzavřením terminálu s běžícím procesem.
schema-default-theme = Výchozí motiv
schema-default-theme-detail = Název aktivního motivu ze seznamu témat.
