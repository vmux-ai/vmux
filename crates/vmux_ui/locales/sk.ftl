locale-name = slovenčina
common-open = Otvoriť
common-close = Zavrieť
common-install = Nainštalovať
common-uninstall = Odinštalovať
common-update = Aktualizovať
common-retry = Skúsiť znova
common-refresh = Obnoviť
common-remove = Odstrániť
common-enable = Zapnúť
common-disable = Vypnúť
common-new = Nové
common-active = aktívne
common-running = spustené
common-done = hotovo
common-failed = Zlyhalo
common-installed = Nainštalované
common-items = { $count ->
    [one] { $count } položka
   *[other] { $count } položiek
}
start-title = Štart
start-tagline = Jeden prompt. Hotové čokoľvek.

agents-title = Agenti
agents-search = Hľadať agentov ACP a CLI…
agents-empty = Žiadni zodpovedajúci agenti
agents-empty-detail = Skúste názov, runtime alebo ACP/CLI.
agents-install-failed = Inštalácia zlyhala
agents-updating = Aktualizuje sa…
agents-retrying = Skúša sa znova…
agents-preparing = Pripravuje sa…

extensions-title = Rozšírenia
extensions-search = Hľadať nainštalované alebo v Chrome Web Store…
extensions-relaunch = Reštartujte na použitie zmien
extensions-empty = Nie sú nainštalované žiadne rozšírenia
extensions-no-match = Žiadne zodpovedajúce rozšírenia
extensions-empty-detail = Vyhľadajte vyššie v Chrome Web Store a stlačte Enter.
extensions-no-match-detail = Skúste iný názov alebo ID rozšírenia.
extensions-on = Zapnuté
extensions-off = Vypnuté
extensions-enable-confirm = Zapnúť { $name }?
extensions-enable-permissions = Zapnúť { $name } a povoliť:

lsp-title = Jazykové servery
lsp-search = Hľadať jazykové servery, lintery, formátovače…
lsp-loading = Načítava sa katalóg…
lsp-empty = Žiadne zodpovedajúce jazykové servery
lsp-empty-detail = Skúste iný jazyk, linter alebo formátovač.
lsp-needs = vyžaduje { $tool }
lsp-status-available = Dostupné
lsp-status-on-path = V PATH
lsp-status-installing = Inštaluje sa…
lsp-status-installed = Nainštalované
lsp-status-outdated = Dostupná aktualizácia
lsp-status-running = Spustené
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
team-just-you = V tomto priestore ste len vy
team-agents = { $count ->
    [one] Vy a 1 agent
   *[other] Vy a { $count } agentov
}
team-empty = Zatiaľ tu nikto nie je
team-you = Vy
team-agent = Agent

services-title = Služby na pozadí
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procesov
}
services-kill-all = Vynútiť ukončenie všetkých
services-not-running = Služba nie je spustená
services-start-with = Spustiť pomocou:
services-empty = Žiadne aktívne procesy
services-filter = Filtrovať procesy…
services-no-match = Žiadne zodpovedajúce procesy
services-connected = Pripojené
services-disconnected = Odpojené
services-attached = pripojené
services-kill = Vynútiť ukončenie
services-memory = Pamäť
services-size = Veľkosť
services-shell = Shell

error-title = Chyba

history-search = Hľadať v histórii
history-clear-all = Vymazať všetko
history-clear-confirm = Vymazať celú históriu?
history-clear-warning = Túto akciu nemožno vrátiť späť.
history-cancel = Zrušiť
history-today = Dnes
history-yesterday = Včera
history-days-ago = pred { $count } dňami
history-day-offset = Deň -{ $count }

settings-title = Nastavenia
settings-loading = Načítavajú sa nastavenia…
settings-stored = Uložené v ~/.vmux/settings.ron
settings-other = Ostatné
settings-software-update = Aktualizácia softvéru
settings-check-updates = Vyhľadať aktualizácie
settings-check-updates-hint = Pri zapnutej automatickej aktualizácii sa kontrolujú pri spustení a každú hodinu.
settings-update-unavailable = Nedostupné
settings-update-unavailable-hint = Aktualizátor nie je súčasťou tohto zostavenia.
settings-update-checking = Kontroluje sa…
settings-update-checking-hint = Kontrolujú sa aktualizácie…
settings-update-check-again = Skontrolovať znova
settings-update-current = Vmux je aktuálny.
settings-update-downloading = Sťahuje sa…
settings-update-downloading-hint = Sťahuje sa Vmux { $version }…
settings-update-installing = Inštaluje sa…
settings-update-installing-hint = Inštaluje sa Vmux { $version }…
settings-update-ready = Aktualizácia je pripravená
settings-update-ready-hint = Vmux { $version } je pripravený. Reštartujte aplikáciu na použitie aktualizácie.
settings-update-try-again = Skúsiť znova
settings-update-failed = Nepodarilo sa vyhľadať aktualizácie.
settings-item = Položka
settings-item-number = Položka { $number }
settings-press-key = Stlačte kláves…
settings-saved = Uložené
settings-record-key = Kliknutím nahrajte novú klávesovú skratku

tray-open-window = Otvoriť okno
tray-close-window = Zavrieť okno
tray-pause-recording = Pozastaviť nahrávanie
tray-resume-recording = Pokračovať v nahrávaní
tray-finish-recording = Dokončiť nahrávanie
tray-quit = Ukončiť Vmux

composer-attach-files = Priložiť súbory (/upload)
composer-remove-attachment = Odstrániť prílohu

layout-back = Späť
layout-forward = Dopredu
layout-reload = Obnoviť
layout-bookmark-page = Pridať stránku medzi záložky
layout-remove-bookmark = Odstrániť záložku
layout-pin-page = Pripnúť stránku
layout-unpin-page = Odopnúť stránku
layout-manage-extensions = Spravovať rozšírenia
layout-new-stack = Nová vrstva
layout-close-tab = Zavrieť kartu
layout-bookmark = Záložka
layout-pin = Pripnúť
layout-new-tab = Nová karta
layout-team = Tím

command-switch-space = Prepnúť priestor…
command-search-ask = Hľadať alebo sa opýtať…
command-new-tab-placeholder = Hľadajte, zadajte URL alebo vyberte Terminál…
command-placeholder = Zadajte URL, hľadajte karty alebo použite > na príkazy…
command-composer-placeholder = Zadajte / pre príkazy alebo @ pre médiá
command-send = Odoslať (Enter)
command-terminal = Terminál
command-open-terminal = Otvoriť v Termináli
command-stack = Vrstva
command-tabs = { $count ->
    [one] 1 karta
   *[other] { $count } kariet
}
command-prompt = Prompt
command-new-tab = Nová karta
command-search = Hľadať
command-open-value = Otvoriť „{ $value }“
command-search-value = Hľadať „{ $value }“

schema-appearance = Vzhľad
schema-general = Všeobecné
schema-layout = Rozloženie
schema-layout-detail = Okno, panely, bočný panel a zvýraznenie fokusu.
schema-agent = Agent
schema-agent-detail = Správanie agenta a povolenia nástrojov.
schema-shortcuts = Skratky
schema-shortcuts-detail = Iba na čítanie. Väzby zmeníte priamo v settings.ron.
schema-terminal = Terminál
schema-browser = Prehliadač
schema-mode = Režim
schema-mode-detail = Farebná schéma webových stránok. Zariadenie nasleduje systém.
schema-device = Zariadenie
schema-light = Svetlý
schema-dark = Tmavý
schema-language = Jazyk
schema-language-detail = Použite systém, en-US, ja alebo ľubovoľný tag BCP 47 so zodpovedajúcim katalógom ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Automatické aktualizácie
schema-auto-update-detail = Vyhľadávať a inštalovať aktualizácie pri spustení a každú hodinu.
schema-startup-url = Úvodná URL
schema-startup-url-detail = Ak je prázdna, otvorí sa výzva príkazového panela.
schema-search-engine = Vyhľadávač
schema-search-engine-detail = Používa sa na webové vyhľadávanie zo Štartu a príkazového panela.
schema-window = Okno
schema-pane = Panel
schema-side-sheet = Bočný panel
schema-focus-ring = Zvýraznenie fokusu
schema-run-placement = Povoliť prepísanie umiestnenia spustenia
schema-run-placement-detail = Umožniť agentom zvoliť režim panelu spustenia, smer a ukotvenie.
schema-leader = Leader
schema-leader-detail = Prefixová klávesa pre chord skratky.
schema-chord-timeout = Časový limit chordu
schema-chord-timeout-detail = Počet milisekúnd, po ktorých prefix chordu vyprší.
schema-bindings = Väzby
schema-confirm-close = Potvrdzovať zatvorenie
schema-confirm-close-detail = Pred zatvorením terminálu so spusteným procesom zobraziť výzvu.
schema-default-theme = Predvolená téma
schema-default-theme-detail = Názov aktívnej témy zo zoznamu tém.
