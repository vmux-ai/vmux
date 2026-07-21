common-open = Otvoriť
common-close = Zavrieť
common-install = Nainštalovať
common-uninstall = Odinštalovať
common-update = Aktualizovať
common-retry = Skúsiť znova
common-refresh = Obnoviť
common-remove = Odstrániť
common-enable = Povoliť
common-disable = Zakázať
common-new = Nový
common-active = aktívny
common-running = beží
common-done = hotovo
common-failed = Zlyhalo
common-installed = Nainštalované
common-items = { $count ->
    [one] { $count } položka
   *[other] { $count } položiek
}
start-title = Štart
start-tagline = Jeden príkaz. Čokoľvek, hotovo.

agents-title = Agenti
agents-search = Hľadať ACP a CLI agentov…
agents-empty = Žiadni zodpovedajúci agenti
agents-empty-detail = Skúste názov, runtime alebo ACP/CLI.
agents-install-failed = Inštalácia zlyhala
agents-updating = Aktualizuje sa…
agents-retrying = Skúša znova…
agents-preparing = Pripravuje sa…

extensions-title = Rozšírenia
extensions-search = Hľadať nainštalované alebo Chrome Web Store…
extensions-relaunch = Reštartovať na uplatnenie
extensions-empty = Žiadne nainštalované rozšírenia
extensions-no-match = Žiadne zodpovedajúce rozšírenia
extensions-empty-detail = Vyhľadajte v Chrome Web Store vyššie a stlačte Return.
extensions-no-match-detail = Skúste iný názov alebo ID rozšírenia.
extensions-on = Zapnuté
extensions-off = Vypnuté
extensions-enable-confirm = Povoliť { $name }?
extensions-enable-permissions = Povoliť { $name } a umožniť:

lsp-title = Jazykové servery
lsp-search = Hľadať jazykové servery, lintery, formátovače…
lsp-loading = Načítava sa katalóg…
lsp-empty = Žiadne zodpovedajúce jazykové servery
lsp-empty-detail = Skúste iný jazyk, linter alebo formátovač.
lsp-needs = vyžaduje { $tool }
lsp-status-available = Dostupné
lsp-status-on-path = Na PATH
lsp-status-installing = Inštaluje sa…
lsp-status-installed = Nainštalované
lsp-status-outdated = Dostupná aktualizácia
lsp-status-running = Beží
lsp-status-failed = Zlyhalo

spaces-title = Priestory
spaces-new-placeholder = Názov nového priestoru
spaces-empty = Žiadne priestory
spaces-default-name = Priestor { $number }
spaces-tabs = { $count ->
    [one] 1 karta
   *[other] { $count } kariet
}
spaces-delete = Odstrániť priestor

team-title = Tím
team-just-you = Len ty v tomto priestore
team-agents = { $count ->
    [one] Ty a 1 agent
   *[other] Ty a { $count } agentov
}
team-empty = Zatiaľ tu nikto nie je
team-you = Ty
team-agent = Agent

services-title = Služby na pozadí
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procesov
}
services-kill-all = Ukončiť všetky
services-not-running = Služba nebeží
services-start-with = Spustiť s:
services-empty = Žiadne aktívne procesy
services-filter = Filtrovať procesy…
services-no-match = Žiadne zodpovedajúce procesy
services-connected = Pripojené
services-disconnected = Odpojené
services-attached = pripojené
services-kill = Ukončiť
services-memory = Pamäť
services-size = Veľkosť
services-shell = Shell

error-title = Chyba

history-search = Hľadať v histórii
history-clear-all = Vymazať všetko
history-clear-confirm = Vymazať celú históriu?
history-clear-warning = Toto nie je možné vrátiť späť.
history-cancel = Zrušiť
history-today = Dnes
history-yesterday = Včera
history-days-ago = Pred { $count } dňami
history-day-offset = Deň -{ $count }

settings-title = Nastavenia
settings-loading = Načítavajú sa nastavenia…
settings-stored = Uložené v ~/.vmux/settings.ron
settings-other = Iné
settings-software-update = Aktualizácia softvéru
settings-check-updates = Skontrolovať aktualizácie
settings-check-updates-hint = Automaticky kontroluje pri spustení a každú hodinu, keď je povolená automatická aktualizácia.
settings-update-unavailable = Nedostupné
settings-update-unavailable-hint = Aktualizátor nie je zahrnutý v tomto zostavení.
settings-update-checking = Kontroluje sa…
settings-update-checking-hint = Kontrolujú sa aktualizácie…
settings-update-check-again = Skontrolovať znova
settings-update-current = Vmux je aktuálny.
settings-update-downloading = Sťahuje sa…
settings-update-downloading-hint = Sťahuje sa Vmux { $version }…
settings-update-installing = Inštaluje sa…
settings-update-installing-hint = Inštaluje sa Vmux { $version }…
settings-update-ready = Aktualizácia pripravená
settings-update-ready-hint = Vmux { $version } je pripravený. Reštartujte na uplatnenie.
settings-update-try-again = Skúsiť znova
settings-update-failed = Nepodarilo sa skontrolovať aktualizácie.
settings-item = Položka
settings-item-number = Položka { $number }
settings-press-key = Stlačte kláves…
settings-saved = Uložené
settings-record-key = Kliknutím zaznamenajte novú kombináciu klávesov

tray-open-window = Otvoriť okno
tray-close-window = Zavrieť okno
tray-pause-recording = Pozastaviť nahrávanie
tray-resume-recording = Obnoviť nahrávanie
tray-finish-recording = Ukončiť nahrávanie
tray-quit = Ukončiť Vmux

composer-attach-files = Priložiť súbory (/upload)
composer-remove-attachment = Odstrániť prílohu

layout-back = Späť
layout-forward = Vpred
layout-reload = Načítať znova
layout-bookmark-page = Pridať záložku na túto stránku
layout-remove-bookmark = Odstrániť záložku
layout-pin-page = Pripnúť túto stránku
layout-unpin-page = Odopnúť túto stránku
layout-manage-extensions = Spravovať rozšírenia
layout-new-stack = Nový zásobník
layout-close-tab = Zavrieť kartu
layout-bookmark = Záložka
layout-pin = Pripnúť
layout-new-tab = Nová karta
layout-team = Tím

command-switch-space = Prepnúť priestor…
command-search-ask = Hľadať alebo sa opýtať…
command-new-tab-placeholder = Hľadajte alebo zadajte URL, alebo vyberte Terminál…
command-placeholder = Zadajte URL, vyhľadajte karty alebo > pre príkazy…
command-composer-placeholder = Zadajte / pre príkazy alebo @ pre médiá
command-send = Odoslať (Enter)
command-terminal = Terminál
command-open-terminal = Otvoriť v termináli
command-stack = Zásobník
command-tabs = { $count ->
    [one] 1 karta
   *[other] { $count } kariet
}
command-prompt = Výzva
command-new-tab = Nová karta
command-search = Hľadať
command-open-value = Otvoriť „{ $value }"
command-search-value = Hľadať „{ $value }"

schema-appearance = Vzhľad
schema-general = Všeobecné
schema-layout = Rozloženie
schema-layout-detail = Okno, podokná, bočný panel a krúžok fokusu.
schema-agent = Agent
schema-agent-detail = Správanie agenta a oprávnenia nástrojov.
schema-shortcuts = Skratky
schema-shortcuts-detail = Zobrazenie len na čítanie. Na zmenu väzieb upravte settings.ron priamo.
schema-terminal = Terminál
schema-browser = Prehliadač
schema-mode = Režim
schema-mode-detail = Farebná schéma pre webové stránky. Zariadenie nasleduje váš systém.
schema-device = Zariadenie
schema-light = Svetlý
schema-dark = Tmavý
schema-language = Jazyk
schema-language-detail = Použite systém, en-US, ja alebo akýkoľvek tag BCP 47 so zodpovedajúcim katalógom ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Automatická aktualizácia
schema-auto-update-detail = Kontrolovať a inštalovať aktualizácie pri spustení a každú hodinu.
schema-startup-url = Spúšťacia URL
schema-startup-url-detail = Prázdne otvorí príkazový riadok.
schema-search-engine = Vyhľadávač
schema-search-engine-detail = Používa sa na webové vyhľadávanie zo Štartu a príkazového riadku.
schema-window = Okno
schema-pane = Podokno
schema-side-sheet = Bočný hárok
schema-focus-ring = Krúžok fokusu
schema-run-placement = Umožniť prepísanie umiestnenia spustenia
schema-run-placement-detail = Umožniť agentom vybrať režim, smer a ukotvenie panela spustenia.
schema-leader = Vedúci kláves
schema-leader-detail = Prefixový kláves pre akordové skratky.
schema-chord-timeout = Časový limit akordu
schema-chord-timeout-detail = Milisekundy pred vypršaním prefixu akordu.
schema-bindings = Väzby
schema-confirm-close = Potvrdiť zatvorenie
schema-confirm-close-detail = Výzva pred zatvorením terminálu s bežiacim procesom.
schema-default-theme = Predvolená téma
schema-default-theme-detail = Názov aktívnej témy zo zoznamu tém.
