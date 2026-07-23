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

tools-title = Nástroje
tools-search = Hledat balíčky, agenty, MCP, jazykové nástroje a konfigurační soubory…
tools-open = Otevřít nástroje
tools-fold = Sbalit nástroje
tools-unfold = Rozbalit nástroje
tools-scanning = Prohledávají se místní nástroje…
tools-no-installed = Nejsou nainstalované žádné nástroje
tools-empty = Žádné odpovídající nástroje
tools-empty-detail = Nainstalujte balíček nebo přidejte balíček konfiguračních souborů ve stylu Stow.
tools-apply = Použít
tools-homebrew = Homebrew
tools-homebrew-sync = Nainstalované formule a aplikace se synchronizují automaticky.
tools-open-brewfile = Otevřít Brewfile
tools-managed = spravováno
tools-provider-homebrew-formulae = Formule Homebrew
tools-provider-homebrew-casks = Aplikace Homebrew
tools-provider-npm = Balíčky npm
tools-provider-acp-agents = Agenti ACP
tools-provider-language-tools = Jazykové nástroje
tools-provider-mcp-servers = Servery MCP
tools-provider-dotfiles = Konfigurační soubory
tools-status-available = Dostupné
tools-status-missing = Chybí
tools-status-conflict = Konflikt
tools-forget = Zapomenout
tools-manage = Spravovat
tools-link = Propojit
tools-unlink = Odpojit
tools-import = Importovat
tools-update-count = { $count ->
    [one] 1 aktualizace
   *[other] { $count } aktualizací
}
tools-conflict-count = { $count ->
    [one] 1 konflikt
   *[other] { $count } konfliktů
}
tools-result-applied = Nástroje byly použity
tools-result-imported = Nástroje byly importovány
tools-result-installed = { $name } byl nainstalován
tools-result-updated = { $name } byl aktualizován
tools-result-uninstalled = { $name } byl odinstalován
tools-result-forgotten = { $name } byl zapomenut
tools-result-managed = { $name } je nyní spravován
tools-result-linked = { $name } byl propojen
tools-result-unlinked = { $name } byl odpojen
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Synchronizujte nastavení, nástroje, dotfiles a znalosti s Git.
vault-sync = Synchronizovat
vault-create = Vytvořit
vault-connect = Připojit
vault-private = Soukromé úložiště
vault-public-warning = Veřejná úložiště odhalují vaše znalosti a konfiguraci.
vault-choose-repository = Vyberte úložiště…
vault-empty = prázdný
vault-clean = Aktuální
vault-not-connected = Nepřipojeno
vault-change-count = Změny: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

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

settings-empty = (prázdné)
settings-none = (žádné)

schema-system = Systém
schema-editor = Editor
schema-recording = Nahrávání
schema-radius = Poloměr
schema-padding = Odsazení
schema-gap = Mezera
schema-width = Šířka
schema-color = Barva
schema-red = Červená
schema-green = Zelená
schema-blue = Modrá
schema-follow-files = Sledovat soubory
schema-tidy-files = Uklízet soubory
schema-tidy-files-max = Práh úklidu souborů
schema-tidy-files-auto = Uklízet soubory automaticky
schema-app-providers = Poskytovatelé aplikací
schema-provider = Poskytovatel
schema-kind = Druh
schema-models = Modely
schema-acp = Agenti ACP
schema-id = ID
schema-name = Název
schema-command = Příkaz
schema-arguments = Argumenty
schema-environment = Prostředí
schema-working-directory = Pracovní adresář
schema-shell = Shell
schema-font-family = Rodina písma
schema-startup-directory = Počáteční adresář
schema-themes = Motivy
schema-color-scheme = Barevné schéma
schema-font-size = Velikost písma
schema-line-height = Výška řádku
schema-cursor-style = Styl kurzoru
schema-cursor-blink = Blikání kurzoru
schema-custom-themes = Vlastní motivy
schema-foreground = Popředí
schema-background = Pozadí
schema-cursor = Kurzor
schema-ansi-colors = Barvy ANSI
schema-keymap = Rozložení kláves
schema-explorer = Průzkumník
schema-visible = Viditelné
schema-language-servers = Jazykové servery
schema-servers = Servery
schema-language-id = ID jazyka
schema-root-markers = Značky kořene
schema-output-directory = Výstupní adresář

menu-scene = Scéna
menu-layout = Rozvržení
menu-terminal = Terminál
menu-browser = Prohlížeč
menu-service = Služba
menu-bookmark = Záložka
menu-edit = Úpravy

layout-knowledge = Znalosti
layout-open-knowledge = Otevřít znalosti
layout-open-welcome-knowledge = Otevřít Vítejte ve znalostech
layout-open-path = Otevřít { $path }
layout-fold-knowledge = Sbalit znalosti
layout-unfold-knowledge = Rozbalit znalosti
layout-bookmarks = Záložky
layout-new-folder = Nová složka
layout-add-to-bookmarks = Přidat do záložek
layout-move-to-bookmarks = Přesunout do záložek
layout-stack-number = Stack { $number }
layout-fold-stack = Sbalit stack
layout-unfold-stack = Rozbalit stack
layout-close-stack = Zavřít stack
layout-bookmark-in = Záložka ve složce { $folder }

common-cancel = Zrušit
common-delete = Smazat
common-save = Uložit
common-rename = Přejmenovat
common-expand = Rozbalit
common-collapse = Sbalit
common-loading = Načítání…
common-error = Chyba
common-output = Výstup
common-pending = Čeká
common-current = aktuální
common-stop = Zastavit
services-command = Služba Vmux
services-uptime-seconds = { $seconds } s
services-uptime-minutes = { $minutes } m { $seconds } s
services-uptime-hours = { $hours } h { $minutes } m
services-uptime-days = { $days } d { $hours } h

error-page-failed-load = Stránku se nepodařilo načíst
error-page-not-found = Stránka nenalezena
error-unknown-host = Neznámý hostitel aplikace Vmux: { $host }

history-title = Historie

command-new-app-chat = Nový chat { $provider }/{ $model } (aplikace)
command-interactive-mode-user = Scéna > Interaktivní režim > Uživatel
command-interactive-mode-player = Scéna > Interaktivní režim > Hráč
command-minimize-window = Rozvržení > Okno > Minimalizovat
command-toggle-layout = Rozvržení > Rozvržení > Přepnout rozvržení
command-close-tab = Rozvržení > Panel > Zavřít panel
command-new-task = Rozvržení > Panel > Nový úkol…
command-next-tab = Rozvržení > Panel > Další panel
command-prev-tab = Rozvržení > Panel > Předchozí panel
command-rename-tab = Rozvržení > Panel > Přejmenovat panel
command-tab-select-1 = Rozvržení > Panel > Vybrat panel 1
command-tab-select-2 = Rozvržení > Panel > Vybrat panel 2
command-tab-select-3 = Rozvržení > Panel > Vybrat panel 3
command-tab-select-4 = Rozvržení > Panel > Vybrat panel 4
command-tab-select-5 = Rozvržení > Panel > Vybrat panel 5
command-tab-select-6 = Rozvržení > Panel > Vybrat panel 6
command-tab-select-7 = Rozvržení > Panel > Vybrat panel 7
command-tab-select-8 = Rozvržení > Panel > Vybrat panel 8
command-tab-select-last = Rozvržení > Panel > Vybrat poslední panel
command-close-pane = Rozvržení > Podokno > Zavřít podokno
command-select-pane-left = Rozvržení > Podokno > Vybrat podokno vlevo
command-select-pane-right = Rozvržení > Podokno > Vybrat podokno vpravo
command-select-pane-up = Rozvržení > Podokno > Vybrat podokno nahoře
command-select-pane-down = Rozvržení > Podokno > Vybrat podokno dole
command-swap-pane-prev = Rozvržení > Podokno > Prohodit s předchozím podoknem
command-swap-pane-next = Rozvržení > Podokno > Prohodit s dalším podoknem
command-equalize-pane-size = Rozvržení > Podokno > Sjednotit velikost podoken
command-resize-pane-left = Rozvržení > Podokno > Změnit velikost doleva
command-resize-pane-right = Rozvržení > Podokno > Změnit velikost doprava
command-resize-pane-up = Rozvržení > Podokno > Změnit velikost nahoru
command-resize-pane-down = Rozvržení > Podokno > Změnit velikost dolů
command-stack-close = Rozvržení > Vrstva > Zavřít vrstvu
command-stack-next = Rozvržení > Vrstva > Další vrstva
command-stack-previous = Rozvržení > Vrstva > Předchozí vrstva
command-stack-reopen = Rozvržení > Vrstva > Znovu otevřít zavřenou stránku
command-stack-swap-prev = Rozvržení > Vrstva > Přesunout vrstvu doleva
command-stack-swap-next = Rozvržení > Vrstva > Přesunout vrstvu doprava
command-space-open = Rozvržení > Prostor > Prostory
command-terminal-close = Terminál > Zavřít terminál
command-terminal-next = Terminál > Další terminál
command-terminal-prev = Terminál > Předchozí terminál
command-terminal-clear = Terminál > Vymazat terminál
command-browser-prev-page = Prohlížeč > Navigace > Zpět
command-browser-next-page = Prohlížeč > Navigace > Vpřed
command-browser-reload = Prohlížeč > Navigace > Načíst znovu
command-browser-hard-reload = Prohlížeč > Navigace > Tvrdé obnovení
command-open-in-place = Prohlížeč > Otevřít > Otevřít zde
command-open-in-new-stack = Prohlížeč > Otevřít > Otevřít v nové vrstvě
command-open-in-pane-top = Prohlížeč > Otevřít > Otevřít v podokně nahoře
command-open-in-pane-right = Prohlížeč > Otevřít > Otevřít v podokně vpravo
command-open-in-pane-bottom = Prohlížeč > Otevřít > Otevřít v podokně dole
command-open-in-pane-left = Prohlížeč > Otevřít > Otevřít v podokně vlevo
command-open-in-new-tab = Prohlížeč > Otevřít > Otevřít v novém panelu
command-open-in-new-space = Prohlížeč > Otevřít > Otevřít v novém prostoru
command-browser-zoom-in = Prohlížeč > Zobrazení > Přiblížit
command-browser-zoom-out = Prohlížeč > Zobrazení > Oddálit
command-browser-zoom-reset = Prohlížeč > Zobrazení > Skutečná velikost
command-browser-dev-tools = Prohlížeč > Zobrazení > Vývojářské nástroje
command-browser-open-command-bar = Prohlížeč > Lišta > Panel příkazů
command-browser-open-page-in-command-bar = Prohlížeč > Lišta > Upravit stránku
command-browser-open-path-bar = Prohlížeč > Lišta > Navigátor cest
command-browser-open-commands = Prohlížeč > Lišta > Příkazy
command-browser-open-history = Prohlížeč > Lišta > Historie
command-service-open = Služba > Otevřít monitor služeb
command-bookmark-toggle-active = Záložka > Přidat stránku do záložek
command-bookmark-pin-active = Záložka > Připnout stránku

layout-tab = Panel
layout-no-stacks = Žádné vrstvy
layout-loading = Načítání…
layout-no-markdown-files = Žádné soubory Markdown
layout-empty-folder = Prázdná složka
layout-worktree = pracovní strom
layout-folder-name = Název složky
layout-no-pins-bookmarks = Žádná připnutí ani záložky
layout-move-to = Přesunout do { $folder }
layout-bookmark-current-page = Přidat aktuální stránku do záložek
layout-rename-folder = Přejmenovat složku
layout-remove-folder = Odebrat složku
layout-update-downloading = Stahování aktualizace
layout-update-installing = Instalace aktualizace…
layout-update-ready = Je dostupná nová verze
layout-restart-update = Aktualizujte restartováním

agent-preparing = Příprava agenta…
agent-send-all-queued = Odeslat všechny výzvy ve frontě teď (Esc)
agent-send = Odeslat (Enter)
agent-ready = Připraven, až budete vy.
agent-loading-older = Načítání starších zpráv…
agent-load-older = Načíst starší zprávy
agent-continued-from = Pokračování z { $source }
agent-older-context-omitted = starší kontext vynechán
agent-interrupted = přerušeno
agent-allow-tool = Povolit { $tool }?
agent-deny = Zamítnout
agent-allow-always = Vždy povolit
agent-allow = Povolit
agent-loading-sessions = Načítání relací…
agent-no-resumable-sessions = Nebyly nalezeny žádné relace k obnovení
agent-no-matching-sessions = Žádné odpovídající relace
agent-no-matching-models = Žádné odpovídající modely
agent-choice-help = ↑/↓ nebo Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Vyberte složku repozitáře
agent-choose-repository-detail = Vyberte místní Git repozitář, který má agent používat.
agent-choosing = Výběr…
agent-choose-folder = Vybrat složku
agent-queued = ve frontě
agent-attached = Připojeno:
agent-cancel-queued = Zrušit výzvu ve frontě
agent-resume-queued = Obnovit výzvy ve frontě
agent-clear-queue = Vymazat frontu
agent-send-all-now = odeslat vše teď
agent-choose-option = Vyberte možnost výše
agent-loading-media = Načítání médií…
agent-no-matching-media = Žádná odpovídající média
agent-prompt-context = Kontext výzvy
agent-details = Podrobnosti
agent-path = Cesta
agent-tool = Nástroj
agent-server = Server
agent-bytes = { $count } bajtů
agent-worked-for = Pracoval { $duration }
agent-worked-for-steps = { $count ->
    [one] Pracoval { $duration } · 1 krok
   *[other] Pracoval { $duration } · { $count } kroků
}
agent-tool-guardian-review = Kontrola Guardian
agent-tool-read-files = Četl soubory
agent-tool-viewed-image = Zobrazil obrázek
agent-tool-used-browser = Použil prohlížeč
agent-tool-searched-files = Prohledával soubory
agent-tool-ran-commands = Spustil příkazy
agent-thinking = Přemýšlí
agent-subagent = Dílčí agent
agent-prompt = Výzva
agent-thread = Vlákno
agent-parent = Nadřazené
agent-children = Podřízené
agent-call = Volání
agent-raw-event = Nezpracovaná událost
agent-plan = Plán
agent-tasks = { $count ->
    [one] 1 úkol
   *[other] { $count } úkolů
}
agent-edited = Upraveno
agent-reconnecting = Opětovné připojování { $attempt }/{ $total }
agent-status-running = Běží
agent-status-done = Hotovo
agent-status-failed = Selhalo
agent-status-pending = Čeká
agent-slash-attach-files = Připojit soubory
agent-slash-resume-session = Obnovit předchozí relaci
agent-slash-select-model = Vybrat model
agent-slash-continue-cli = Pokračovat v této relaci v CLI
agent-session-just-now = právě teď
agent-session-minutes-ago = před { $count } min
agent-session-hours-ago = před { $count } h
agent-session-days-ago = před { $count } d
agent-working-working = Pracuje
agent-working-thinking = Přemýšlí
agent-working-pondering = Zvažuje
agent-working-noodling = Hloubá
agent-working-percolating = Probublává
agent-working-conjuring = Kouzlí
agent-working-cooking = Vaří
agent-working-brewing = Louhuje
agent-working-musing = Rozjímá
agent-working-ruminating = Přemítá
agent-working-scheming = Plánuje
agent-working-synthesizing = Syntetizuje
agent-working-tinkering = Ladí
agent-working-churning = Zpracovává
agent-working-vibing = Ladí se
agent-working-simmering = Probublává
agent-working-crafting = Tvoří
agent-working-divining = Věští
agent-working-mulling = Promýšlí
agent-working-spelunking = Prozkoumává

editor-toggle-explorer = Přepnout Průzkumník (Cmd+B)
editor-unsaved = neuloženo
editor-rendered-markdown = Vykreslený Markdown s živými úpravami
editor-note = Poznámka
editor-source-editor = Editor zdrojového kódu
editor-editor = Editor
editor-git-diff = Git diff
editor-diff = Diff
editor-tidy = Uklidit
editor-always = Vždy
editor-unchanged-previews = { $count ->
    [one] ✦ 1 nezměněný náhled
   *[other] ✦ { $count } nezměněných náhledů
}
editor-open-externally = Otevřít externě
editor-changed-line = Změněný řádek
editor-go-to-definition = Přejít na definici
editor-find-references = Najít reference
editor-references = { $count ->
    [one] 1 reference
   *[other] { $count } referencí
}
editor-lsp-starting = { $server } se spouští…
editor-lsp-not-installed = { $server } — není nainstalován
editor-explorer = Průzkumník
editor-open-editors = Otevřené editory
editor-outline = Osnova
editor-new-file = Nový soubor
editor-new-folder = Nová složka
editor-delete-confirm = Smazat „{ $name }“? Tuto akci nelze vrátit zpět.
editor-created-folder = Vytvořena složka { $name }
editor-created-file = Vytvořen soubor { $name }
editor-renamed-to = Přejmenováno na { $name }
editor-deleted = Smazáno { $name }
editor-failed-decode-image = Obrázek se nepodařilo dekódovat
editor-preview-large-image = obrázek (příliš velký pro náhled)
editor-preview-binary = binární
editor-preview-file = soubor

git-status-clean = čisté
git-status-modified = změněno
git-status-staged = připraveno
git-status-staged-modified = připraveno*
git-status-untracked = nesledováno
git-status-deleted = smazáno
git-status-conflict = konflikt
git-accept-all = ✓ přijmout vše
git-unstage = Odebrat z přípravy
git-confirm-deny-all = Potvrdit zamítnutí všeho
git-deny-all = ✗ zamítnout vše
git-commit-message = zpráva commitu
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Načítání diffu…
git-no-changes = Žádné změny k zobrazení
git-accept = ✓ přijmout
git-deny = ✗ zamítnout
git-show-unchanged-lines = Zobrazit { $count } nezměněných řádků

terminal-loading = Načítání…
terminal-runs-when-ready = spustí se, až bude připraven · Ctrl+C vymaže · Esc přeskočí
terminal-booting = spouštění
terminal-type-command = zadejte příkaz · spustí se, až bude připraven · Esc přeskočí

setup-tagline-claude = Kódovací agent Anthropic ve Vmux
setup-tagline-codex = Kódovací agent OpenAI ve Vmux
setup-tagline-vibe = Kódovací agent Mistral ve Vmux
setup-install-title = Instalace CLI { $name }
setup-homebrew-required = K instalaci { $command } je potřeba Homebrew a zatím není nastavený. Vmux nejdřív nainstaluje Homebrew a potom { $name }.
setup-terminal-instructions = V terminálu spusťte instalaci stisknutím Return a po výzvě zadejte heslo k Macu.
setup-command-missing = Vmux otevřel tuto stránku, protože místní příkaz { $command } zatím není nainstalovaný. Získáte ho spuštěním příkazu níže.
setup-install-failed = Instalace se nedokončila. Podrobnosti najdete v terminálu, potom to zkuste znovu.
setup-installing = Instaluje se…
setup-install-homebrew = Nainstalovat Homebrew + { $name }
setup-run-install = Spustit instalační příkaz
setup-auto-reload = Vmux ho spustí v terminálu a po připravení { $command } stránku znovu načte.

debug-title = Ladění
debug-auto-update = Automatické aktualizace
debug-simulate-update = Simulovat dostupnou aktualizaci
debug-simulate-download = Simulovat stažení
debug-clear-update = Vymazat aktualizaci
debug-trigger-restart = Spustit restart

command-manage-spaces = Spravovat prostory…
command-pane-stack-location = panel { $pane } / vrstva { $stack }
command-space-pane-stack-location = { $space } / panel { $pane } / vrstva { $stack }
command-terminal-path = Terminál ({ $path })
command-group-interactive-mode = Interaktivní režim
command-group-window = Okno
command-group-tab = Karta
command-group-pane = Panel
command-group-stack = Vrstva
command-group-space = Prostor
command-group-navigation = Navigace
command-group-open = Otevřít
command-group-view = Zobrazení
command-group-bar = Lišta

menu-close-vmux = Zavřít Vmux

agents-terminal-coding-agent = Kódovací agent v terminálu
