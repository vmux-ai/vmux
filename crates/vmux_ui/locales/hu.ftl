locale-name = magyar
common-open = Megnyitás
common-close = Bezárás
common-install = Telepítés
common-uninstall = Eltávolítás
common-update = Frissítés
common-retry = Újra
common-refresh = Frissítés
common-remove = Eltávolítás
common-enable = Engedélyezés
common-disable = Letiltás
common-new = Új
common-active = aktív
common-running = fut
common-done = kész
common-failed = Sikertelen
common-installed = Telepítve
common-items = { $count ->
    [one] { $count } elem
   *[other] { $count } elem
}

tools-title = Eszközök
tools-search = Csomagok, ügynökök, MCP, nyelvi eszközök és konfigurációs fájlok keresése…
tools-open = Eszközök megnyitása
tools-fold = Eszközök összecsukása
tools-unfold = Eszközök kibontása
tools-scanning = Helyi eszközök vizsgálata…
tools-no-installed = Nincs telepített eszköz
tools-empty = Nincs egyező eszköz
tools-empty-detail = Telepítsen egy csomagot, vagy adjon hozzá egy Stow-stílusú konfigurációsfájl-csomagot.
tools-apply = Alkalmaz
tools-homebrew = Homebrew
tools-homebrew-sync = A telepített képletek és alkalmazások automatikusan szinkronizálódnak.
tools-open-brewfile = Brewfile megnyitása
tools-managed = kezelt
tools-provider-homebrew-formulae = Homebrew-képletek
tools-provider-homebrew-casks = Homebrew-alkalmazások
tools-provider-npm = npm-csomagok
tools-provider-acp-agents = ACP-ügynökök
tools-provider-language-tools = Nyelvi eszközök
tools-provider-mcp-servers = MCP-kiszolgálók
tools-provider-dotfiles = Konfigurációs fájlok
tools-status-available = Elérhető
tools-status-missing = Hiányzik
tools-status-conflict = Ütközés
tools-forget = Elfelejtés
tools-manage = Kezelés
tools-link = Összekapcsolás
tools-unlink = Szétkapcsolás
tools-import = Importálás
tools-update-count = { $count ->
    [one] 1 frissítés
   *[other] { $count } frissítés
}
tools-conflict-count = { $count ->
    [one] 1 ütközés
   *[other] { $count } ütközés
}
tools-result-applied = Eszközök alkalmazva
tools-result-imported = Eszközök importálva
tools-result-installed = { $name } telepítve
tools-result-updated = { $name } frissítve
tools-result-uninstalled = { $name } eltávolítva
tools-result-forgotten = { $name } elfelejtve
tools-result-managed = { $name } mostantól kezelt
tools-result-linked = { $name } összekapcsolva
tools-result-unlinked = { $name } szétkapcsolva

start-title = Kezdés
start-tagline = Egy prompt. Bármi elkészül.

agents-title = Agentek
agents-search = ACP- és CLI-agentek keresése…
agents-empty = Nincs találó agent
agents-empty-detail = Próbáljon névre, futtatókörnyezetre vagy ACP/CLI-re keresni.
agents-install-failed = A telepítés sikertelen
agents-updating = Frissítés…
agents-retrying = Újrapróbálkozás…
agents-preparing = Előkészítés…

extensions-title = Bővítmények
extensions-search = Keresés a telepítettek között vagy a Chrome Web Store-ban…
extensions-relaunch = Újraindítás az alkalmazáshoz
extensions-empty = Nincs telepített bővítmény
extensions-no-match = Nincs találó bővítmény
extensions-empty-detail = Keressen fent a Chrome Web Store-ban, majd nyomja meg az Entert.
extensions-no-match-detail = Próbáljon másik nevet vagy bővítményazonosítót.
extensions-on = Be
extensions-off = Ki
extensions-enable-confirm = Engedélyezi ezt: { $name }?
extensions-enable-permissions = { $name } engedélyezése és hozzáférés engedélyezése:

lsp-title = Nyelvi kiszolgálók
lsp-search = Nyelvi kiszolgálók, lintelők, formázók keresése…
lsp-loading = Katalógus betöltése…
lsp-empty = Nincs találó nyelvi kiszolgáló
lsp-empty-detail = Próbáljon másik nyelvet, lintelőt vagy formázót.
lsp-needs = szükséges: { $tool }
lsp-status-available = Elérhető
lsp-status-on-path = A PATH-ban
lsp-status-installing = Telepítés…
lsp-status-installed = Telepítve
lsp-status-outdated = Frissítés érhető el
lsp-status-running = Fut
lsp-status-failed = Sikertelen

spaces-title = Munkaterek
spaces-new-placeholder = Új munkatér neve
spaces-empty = Nincsenek munkaterek
spaces-default-name = Munkatér { $number }
spaces-tabs = { $count ->
    [one] 1 lap
   *[other] { $count } lap
}
spaces-delete = Munkatér törlése

team-title = Csapat
team-just-you = Csak Ön van ebben a munkatérben
team-agents = { $count ->
    [one] Ön és 1 agent
   *[other] Ön és { $count } agent
}
team-empty = Még nincs itt senki
team-you = Ön
team-agent = Agent

services-title = Háttérszolgáltatások
services-processes = { $count ->
    [one] 1 folyamat
   *[other] { $count } folyamat
}
services-kill-all = Összes kilövése
services-not-running = A szolgáltatás nem fut
services-start-with = Indítás ezzel:
services-empty = Nincsenek aktív folyamatok
services-filter = Folyamatok szűrése…
services-no-match = Nincs találó folyamat
services-connected = Csatlakoztatva
services-disconnected = Nincs kapcsolat
services-attached = csatolva
services-kill = Kilövés
services-memory = Memória
services-size = Méret
services-shell = Shell

error-title = Hiba

history-search = Keresés az előzményekben
history-clear-all = Összes törlése
history-clear-confirm = Törli az összes előzményt?
history-clear-warning = Ez nem vonható vissza.
history-cancel = Mégse
history-today = Ma
history-yesterday = Tegnap
history-days-ago = { $count } napja
history-day-offset = Nap -{ $count }

settings-title = Beállítások
settings-loading = Beállítások betöltése…
settings-stored = Tárolási hely: ~/.vmux/settings.ron
settings-other = Egyéb
settings-software-update = Szoftverfrissítés
settings-check-updates = Frissítések keresése
settings-check-updates-hint = Indításkor és óránként automatikusan ellenőrzi, ha az automatikus frissítés be van kapcsolva.
settings-update-unavailable = Nem érhető el
settings-update-unavailable-hint = Ez a build nem tartalmaz frissítőt.
settings-update-checking = Ellenőrzés…
settings-update-checking-hint = Frissítések keresése…
settings-update-check-again = Újraellenőrzés
settings-update-current = A Vmux naprakész.
settings-update-downloading = Letöltés…
settings-update-downloading-hint = Vmux { $version } letöltése…
settings-update-installing = Telepítés…
settings-update-installing-hint = Vmux { $version } telepítése…
settings-update-ready = A frissítés kész
settings-update-ready-hint = A Vmux { $version } készen áll. Indítsa újra az alkalmazáshoz.
settings-update-try-again = Próbálja újra
settings-update-failed = Nem sikerült frissítéseket keresni.
settings-item = Elem
settings-item-number = Elem { $number }
settings-press-key = Nyomjon meg egy billentyűt…
settings-saved = Mentve
settings-record-key = Kattintson új billentyűkombináció rögzítéséhez

tray-open-window = Ablak megnyitása
tray-close-window = Ablak bezárása
tray-pause-recording = Rögzítés szüneteltetése
tray-resume-recording = Rögzítés folytatása
tray-finish-recording = Rögzítés befejezése
tray-quit = Kilépés a Vmuxból

composer-attach-files = Fájlok csatolása (/upload)
composer-remove-attachment = Melléklet eltávolítása

layout-back = Vissza
layout-forward = Előre
layout-reload = Újratöltés
layout-bookmark-page = Oldal könyvjelzőzése
layout-remove-bookmark = Könyvjelző eltávolítása
layout-pin-page = Oldal kitűzése
layout-unpin-page = Oldal kitűzésének megszüntetése
layout-manage-extensions = Bővítmények kezelése
layout-new-stack = Új réteg
layout-close-tab = Lap bezárása
layout-bookmark = Könyvjelző
layout-pin = Kitűzés
layout-new-tab = Új lap
layout-team = Csapat

command-switch-space = Munkatér váltása…
command-search-ask = Keresés vagy kérdés…
command-new-tab-placeholder = Keressen vagy írjon be egy URL-t, vagy válassza a Terminált…
command-placeholder = Írjon be egy URL-t, keressen a lapok között, vagy használja a > jelet parancsokhoz…
command-composer-placeholder = Parancsokhoz írjon / jelet, médiához @ jelet
command-send = Küldés (Enter)
command-terminal = Terminál
command-open-terminal = Megnyitás Terminálban
command-stack = Réteg
command-tabs = { $count ->
    [one] 1 lap
   *[other] { $count } lap
}
command-prompt = Prompt
command-new-tab = Új lap
command-search = Keresés
command-open-value = „{ $value }” megnyitása
command-search-value = „{ $value }” keresése

schema-appearance = Megjelenés
schema-general = Általános
schema-layout = Elrendezés
schema-layout-detail = Ablak, panelek, oldalsáv és fókuszkeret.
schema-agent = Agent
schema-agent-detail = Agent viselkedése és eszközengedélyei.
schema-shortcuts = Billentyűparancsok
schema-shortcuts-detail = Csak olvasható nézet. A kiosztások módosításához szerkessze közvetlenül a settings.ron fájlt.
schema-terminal = Terminál
schema-browser = Böngésző
schema-mode = Mód
schema-mode-detail = Weboldalak színsémája. Az Eszköz beállítás a rendszerét követi.
schema-device = Eszköz
schema-light = Világos
schema-dark = Sötét
schema-language = Nyelv
schema-language-detail = Használja a rendszer nyelvét, az en-US-t, a ja-t vagy bármely BCP 47 címkét egy hozzá illő ~/.vmux/locales/<tag>.ftl katalógussal.
schema-auto-update = Automatikus frissítés
schema-auto-update-detail = Frissítések keresése és telepítése indításkor és óránként.
schema-startup-url = Indítási URL
schema-startup-url-detail = Üresen a parancssáv promptja nyílik meg.
schema-search-engine = Keresőmotor
schema-search-engine-detail = Webes keresésekhez használja a Kezdés képernyőről és a parancssávból.
schema-window = Ablak
schema-pane = Panel
schema-side-sheet = Oldalsó lap
schema-focus-ring = Fókuszkeret
schema-run-placement = Futtatási elhelyezés felülbírálásának engedélyezése
schema-run-placement-detail = Az agentek kiválaszthatják a futtatási panel módját, irányát és rögzítési pontját.
schema-leader = Leader
schema-leader-detail = Előtagbillentyű akkordos billentyűparancsokhoz.
schema-chord-timeout = Akkord időkorlátja
schema-chord-timeout-detail = Ennyi ezredmásodperc után jár le az akkord előtagja.
schema-bindings = Kiosztások
schema-confirm-close = Bezárás megerősítése
schema-confirm-close-detail = Kérjen megerősítést futó folyamattal rendelkező terminál bezárása előtt.
schema-default-theme = Alapértelmezett téma
schema-default-theme-detail = Az aktív téma neve a témalistából.

settings-empty = (üres)
settings-none = (nincs)

schema-system = Rendszer
schema-editor = Szerkesztő
schema-recording = Felvétel
schema-radius = Sugár
schema-padding = Belső margó
schema-gap = Hézag
schema-width = Szélesség
schema-color = Szín
schema-red = Piros
schema-green = Zöld
schema-blue = Kék
schema-follow-files = Fájlok követése
schema-tidy-files = Fájlok rendbetétele
schema-tidy-files-max = Fájl-rendbetételi küszöb
schema-tidy-files-auto = Fájlok automatikus rendbetétele
schema-app-providers = Alkalmazásszolgáltatók
schema-provider = Szolgáltató
schema-kind = Típus
schema-models = Modellek
schema-acp = ACP-ügynökök
schema-id = ID
schema-name = Név
schema-command = Parancs
schema-arguments = Argumentumok
schema-environment = Környezet
schema-working-directory = Munkakönyvtár
schema-shell = Parancsértelmező
schema-font-family = Betűcsalád
schema-startup-directory = Induló könyvtár
schema-themes = Témák
schema-color-scheme = Színséma
schema-font-size = Betűméret
schema-line-height = Sormagasság
schema-cursor-style = Kurzor stílusa
schema-cursor-blink = Kurzor villogása
schema-custom-themes = Egyéni témák
schema-foreground = Előtér
schema-background = Háttér
schema-cursor = Kurzor
schema-ansi-colors = ANSI-színek
schema-keymap = Billentyűtérkép
schema-explorer = Intéző
schema-visible = Látható
schema-language-servers = Nyelvi szerverek
schema-servers = Szerverek
schema-language-id = Nyelv ID
schema-root-markers = Gyökérjelölők
schema-output-directory = Kimeneti könyvtár

menu-scene = Jelenet
menu-layout = Elrendezés
menu-terminal = Terminál
menu-browser = Böngésző
menu-service = Szolgáltatás
menu-bookmark = Könyvjelző
menu-edit = Szerkesztés

layout-knowledge = Tudástár
layout-open-knowledge = Tudástár megnyitása
layout-open-welcome-knowledge = Tudástár üdvözlőlapjának megnyitása
layout-open-path = { $path } megnyitása
layout-fold-knowledge = Tudástár összecsukása
layout-unfold-knowledge = Tudástár kinyitása
layout-bookmarks = Könyvjelzők
layout-new-folder = Új mappa
layout-add-to-bookmarks = Hozzáadás a könyvjelzőkhöz
layout-move-to-bookmarks = Áthelyezés a könyvjelzőkhöz
layout-stack-number = Köteg { $number }
layout-fold-stack = Köteg összecsukása
layout-unfold-stack = Köteg kinyitása
layout-close-stack = Köteg bezárása
layout-bookmark-in = Könyvjelző ide: { $folder }

common-cancel = Mégse
common-delete = Törlés
common-save = Mentés
common-rename = Átnevezés
common-expand = Kibontás
common-collapse = Összecsukás
common-loading = Betöltés…
common-error = Hiba
common-output = Kimenet
common-pending = Függőben
common-current = aktuális
common-stop = Leállítás
services-command = Vmux-szolgáltatás
services-uptime-seconds = { $seconds } mp
services-uptime-minutes = { $minutes } p { $seconds } mp
services-uptime-hours = { $hours } ó { $minutes } p
services-uptime-days = { $days } n { $hours } ó

error-page-failed-load = Az oldal betöltése sikertelen
error-page-not-found = Az oldal nem található
error-unknown-host = Ismeretlen Vmux-alkalmazásgazda: { $host }

history-title = Előzmények

command-new-app-chat = Új { $provider }/{ $model } csevegés (Alkalmazás)
command-interactive-mode-user = Jelenet > Interaktív mód > Felhasználó
command-interactive-mode-player = Jelenet > Interaktív mód > Lejátszó
command-minimize-window = Elrendezés > Ablak > Kis méret
command-toggle-layout = Elrendezés > Elrendezés > Elrendezés váltása
command-close-tab = Elrendezés > Lap > Lap bezárása
command-new-task = Elrendezés > Lap > Új feladat…
command-next-tab = Elrendezés > Lap > Következő lap
command-prev-tab = Elrendezés > Lap > Előző lap
command-rename-tab = Elrendezés > Lap > Lap átnevezése
command-tab-select-1 = Elrendezés > Lap > 1. lap kiválasztása
command-tab-select-2 = Elrendezés > Lap > 2. lap kiválasztása
command-tab-select-3 = Elrendezés > Lap > 3. lap kiválasztása
command-tab-select-4 = Elrendezés > Lap > 4. lap kiválasztása
command-tab-select-5 = Elrendezés > Lap > 5. lap kiválasztása
command-tab-select-6 = Elrendezés > Lap > 6. lap kiválasztása
command-tab-select-7 = Elrendezés > Lap > 7. lap kiválasztása
command-tab-select-8 = Elrendezés > Lap > 8. lap kiválasztása
command-tab-select-last = Elrendezés > Lap > Utolsó lap kiválasztása
command-close-pane = Elrendezés > Panel > Panel bezárása
command-select-pane-left = Elrendezés > Panel > Bal oldali panel kiválasztása
command-select-pane-right = Elrendezés > Panel > Jobb oldali panel kiválasztása
command-select-pane-up = Elrendezés > Panel > Felső panel kiválasztása
command-select-pane-down = Elrendezés > Panel > Alsó panel kiválasztása
command-swap-pane-prev = Elrendezés > Panel > Panel csere az előzővel
command-swap-pane-next = Elrendezés > Panel > Panel csere a következővel
command-equalize-pane-size = Elrendezés > Panel > Panelek méretének kiegyenlítése
command-resize-pane-left = Elrendezés > Panel > Panel méretezése balra
command-resize-pane-right = Elrendezés > Panel > Panel méretezése jobbra
command-resize-pane-up = Elrendezés > Panel > Panel méretezése felfelé
command-resize-pane-down = Elrendezés > Panel > Panel méretezése lefelé
command-stack-close = Elrendezés > Verem > Verem bezárása
command-stack-next = Elrendezés > Verem > Következő verem
command-stack-previous = Elrendezés > Verem > Előző verem
command-stack-reopen = Elrendezés > Verem > Bezárt oldal újranyitása
command-stack-swap-prev = Elrendezés > Verem > Verem mozgatása balra
command-stack-swap-next = Elrendezés > Verem > Verem mozgatása jobbra
command-space-open = Elrendezés > Tér > Terek
command-terminal-close = Terminál > Terminál bezárása
command-terminal-next = Terminál > Következő terminál
command-terminal-prev = Terminál > Előző terminál
command-terminal-clear = Terminál > Terminál törlése
command-browser-prev-page = Böngésző > Navigáció > Vissza
command-browser-next-page = Böngésző > Navigáció > Előre
command-browser-reload = Böngésző > Navigáció > Újratöltés
command-browser-hard-reload = Böngésző > Navigáció > Teljes újratöltés
command-open-in-place = Böngésző > Megnyitás > Megnyitás itt
command-open-in-new-stack = Böngésző > Megnyitás > Megnyitás új veremben
command-open-in-pane-top = Böngésző > Megnyitás > Megnyitás a felső panelen
command-open-in-pane-right = Böngésző > Megnyitás > Megnyitás a jobb oldali panelen
command-open-in-pane-bottom = Böngésző > Megnyitás > Megnyitás az alsó panelen
command-open-in-pane-left = Böngésző > Megnyitás > Megnyitás a bal oldali panelen
command-open-in-new-tab = Böngésző > Megnyitás > Megnyitás új lapon
command-open-in-new-space = Böngésző > Megnyitás > Megnyitás új térben
command-browser-zoom-in = Böngésző > Nézet > Nagyítás
command-browser-zoom-out = Böngésző > Nézet > Kicsinyítés
command-browser-zoom-reset = Böngésző > Nézet > Tényleges méret
command-browser-dev-tools = Böngésző > Nézet > Fejlesztői eszközök
command-browser-open-command-bar = Böngésző > Sáv > Parancssáv
command-browser-open-page-in-command-bar = Böngésző > Sáv > Oldal szerkesztése
command-browser-open-path-bar = Böngésző > Sáv > Útvonal-navigátor
command-browser-open-commands = Böngésző > Sáv > Parancsok
command-browser-open-history = Böngésző > Sáv > Előzmények
command-service-open = Szolgáltatás > Szolgáltatásfigyelő megnyitása
command-bookmark-toggle-active = Könyvjelző > Oldal könyvjelzőzése
command-bookmark-pin-active = Könyvjelző > Oldal rögzítése

layout-tab = Lap
layout-no-stacks = Nincsenek vermek
layout-loading = Betöltés…
layout-no-markdown-files = Nincsenek Markdown-fájlok
layout-empty-folder = Üres mappa
layout-worktree = munkafa
layout-folder-name = Mappanév
layout-no-pins-bookmarks = Nincsenek rögzítések vagy könyvjelzők
layout-move-to = Áthelyezés ide: { $folder }
layout-bookmark-current-page = Aktuális oldal könyvjelzőzése
layout-rename-folder = Mappa átnevezése
layout-remove-folder = Mappa eltávolítása
layout-update-downloading = Frissítés letöltése
layout-update-installing = Frissítés telepítése…
layout-update-ready = Új verzió érhető el
layout-restart-update = Újraindítás a frissítéshez

agent-preparing = Ügynök előkészítése…
agent-send-all-queued = Összes sorban álló prompt küldése most (Esc)
agent-send = Küldés (Enter)
agent-ready = Készen áll, amikor te is.
agent-loading-older = Korábbi üzenetek betöltése…
agent-load-older = Korábbi üzenetek betöltése
agent-continued-from = Folytatás innen: { $source }
agent-older-context-omitted = korábbi kontextus kihagyva
agent-interrupted = megszakítva
agent-allow-tool = Engedélyezi ezt: { $tool }?
agent-deny = Elutasítás
agent-allow-always = Mindig engedélyezés
agent-allow = Engedélyezés
agent-loading-sessions = Munkamenetek betöltése…
agent-no-resumable-sessions = Nem találhatók folytatható munkamenetek
agent-no-matching-sessions = Nincs egyező munkamenet
agent-no-matching-models = Nincs egyező modell
agent-choice-help = ↑/↓ vagy Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Válassz adattármappát
agent-choose-repository-detail = Válaszd ki azt a helyi Git-adattárat, amelyet az ügynök használjon.
agent-choosing = Kiválasztás…
agent-choose-folder = Mappa kiválasztása
agent-queued = sorban
agent-attached = Csatolva:
agent-cancel-queued = Sorban álló prompt megszakítása
agent-resume-queued = Sorban álló promptok folytatása
agent-clear-queue = Sor törlése
agent-send-all-now = összes küldése most
agent-choose-option = Válassz egy lehetőséget fent
agent-loading-media = Média betöltése…
agent-no-matching-media = Nincs egyező média
agent-prompt-context = Promptkörnyezet
agent-details = Részletek
agent-path = Útvonal
agent-tool = Eszköz
agent-server = Kiszolgáló
agent-bytes = { $count } bájt
agent-worked-for = Munkaidő: { $duration }
agent-worked-for-steps = { $count ->
    [one] Munkaidő: { $duration } · 1 lépés
   *[other] Munkaidő: { $duration } · { $count } lépés
}
agent-tool-guardian-review = Őrzői felülvizsgálat
agent-tool-read-files = Fájlokat olvasott
agent-tool-viewed-image = Képet nézett meg
agent-tool-used-browser = Böngészőt használt
agent-tool-searched-files = Fájlokban keresett
agent-tool-ran-commands = Parancsokat futtatott
agent-thinking = Gondolkodik
agent-subagent = Alügynök
agent-prompt = Prompt
agent-thread = Szál
agent-parent = Szülő
agent-children = Gyermekek
agent-call = Hívás
agent-raw-event = Nyers esemény
agent-plan = Terv
agent-tasks = { $count ->
    [one] 1 feladat
   *[other] { $count } feladat
}
agent-edited = Szerkesztve
agent-reconnecting = Újracsatlakozás { $attempt }/{ $total }
agent-status-running = Fut
agent-status-done = Kész
agent-status-failed = Sikertelen
agent-status-pending = Függőben
agent-slash-attach-files = Fájlok csatolása
agent-slash-resume-session = Korábbi munkamenet folytatása
agent-slash-select-model = Modell kiválasztása
agent-slash-continue-cli = Munkamenet folytatása a CLI-ben
agent-session-just-now = épp most
agent-session-minutes-ago = { $count } p ezelőtt
agent-session-hours-ago = { $count } ó ezelőtt
agent-session-days-ago = { $count } n ezelőtt
agent-working-working = Dolgozik
agent-working-thinking = Gondolkodik
agent-working-pondering = Tűnődik
agent-working-noodling = Ötletel
agent-working-percolating = Érik a gondolat
agent-working-conjuring = Varázsol
agent-working-cooking = Főzi a megoldást
agent-working-brewing = Kotyvaszt
agent-working-musing = Mereng
agent-working-ruminating = Rágódik
agent-working-scheming = Tervet sző
agent-working-synthesizing = Szintetizál
agent-working-tinkering = Bütyköl
agent-working-churning = Dolgozza fel
agent-working-vibing = Ráhangolódik
agent-working-simmering = Lassú tűzön főz
agent-working-crafting = Formálja
agent-working-divining = Fürkészi
agent-working-mulling = Mérlegel
agent-working-spelunking = Mélyre ás

editor-toggle-explorer = Intéző ki/be (Cmd+B)
editor-unsaved = nincs mentve
editor-rendered-markdown = Renderelt Markdown élő szerkesztéssel
editor-note = Jegyzet
editor-source-editor = Forrásszerkesztő
editor-editor = Szerkesztő
editor-git-diff = Git diff
editor-diff = Diff
editor-tidy = Rendrakás
editor-always = Mindig
editor-unchanged-previews = { $count ->
    [one] ✦ 1 változatlan előnézet
   *[other] ✦ { $count } változatlan előnézet
}
editor-open-externally = Megnyitás külső alkalmazásban
editor-changed-line = Módosított sor
editor-go-to-definition = Ugrás a definícióhoz
editor-find-references = Hivatkozások keresése
editor-references = { $count ->
    [one] 1 hivatkozás
   *[other] { $count } hivatkozás
}
editor-lsp-starting = { $server } indul…
editor-lsp-not-installed = { $server } — nincs telepítve
editor-explorer = Intéző
editor-open-editors = Megnyitott szerkesztők
editor-outline = Vázlat
editor-new-file = Új fájl
editor-new-folder = Új mappa
editor-delete-confirm = Törlöd ezt: „{ $name }”? Ez nem vonható vissza.
editor-created-folder = Mappa létrehozva: { $name }
editor-created-file = Fájl létrehozva: { $name }
editor-renamed-to = Átnevezve erre: { $name }
editor-deleted = Törölve: { $name }
editor-failed-decode-image = A kép dekódolása sikertelen
editor-preview-large-image = kép (túl nagy az előnézethez)
editor-preview-binary = bináris
editor-preview-file = fájl

git-status-clean = tiszta
git-status-modified = módosítva
git-status-staged = előkészítve
git-status-staged-modified = előkészítve*
git-status-untracked = nem követett
git-status-deleted = törölve
git-status-conflict = ütközés
git-accept-all = ✓ összes elfogadása
git-unstage = Előkészítés visszavonása
git-confirm-deny-all = Összes elutasításának megerősítése
git-deny-all = ✗ összes elutasítása
git-commit-message = commitüzenet
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Diff betöltése…
git-no-changes = Nincs megjeleníthető módosítás
git-accept = ✓ elfogadás
git-deny = ✗ elutasítás
git-show-unchanged-lines = { $count } változatlan sor megjelenítése

terminal-loading = Betöltés…
terminal-runs-when-ready = fut, ha kész · Ctrl+C töröl · Esc kihagy
terminal-booting = indul
terminal-type-command = írj be egy parancsot · fut, ha kész · Esc kihagy

setup-tagline-claude = Az Anthropic kódolóügynöke a Vmuxban
setup-tagline-codex = Az OpenAI kódolóügynöke a Vmuxban
setup-tagline-vibe = A Mistral kódolóügynöke a Vmuxban
setup-install-title = { $name } CLI telepítése
setup-homebrew-required = A { $command } telepítéséhez Homebrew szükséges, de még nincs beállítva. A Vmux először a Homebrew-t telepíti, majd ezt: { $name }.
setup-terminal-instructions = A terminálban nyomd meg a Return billentyűt az indításhoz, majd add meg a Mac jelszavad, amikor kéri.
setup-command-missing = A Vmux azért nyitotta meg ezt az oldalt, mert a helyi { $command } parancs még nincs telepítve. A beszerzéséhez futtasd az alábbi parancsot.
setup-install-failed = A telepítés nem fejeződött be. Nézd meg a részleteket a terminálban, majd próbáld újra.
setup-installing = Telepítés…
setup-install-homebrew = Homebrew + { $name } telepítése
setup-run-install = Telepítőparancs futtatása
setup-auto-reload = A Vmux terminálban futtatja, és újratölt, amikor a { $command } készen áll.

debug-title = Hibakeresés
debug-auto-update = Automatikus frissítés
debug-simulate-update = Elérhető frissítés szimulálása
debug-simulate-download = Letöltés szimulálása
debug-clear-update = Frissítés törlése
debug-trigger-restart = Újraindítás indítása

command-manage-spaces = Terek kezelése…
command-pane-stack-location = panel { $pane } / verem { $stack }
command-space-pane-stack-location = { $space } / panel { $pane } / verem { $stack }
command-terminal-path = Terminál ({ $path })
command-group-interactive-mode = Interaktív mód
command-group-window = Ablak
command-group-tab = Lap
command-group-pane = Panel
command-group-stack = Verem
command-group-space = Tér
command-group-navigation = Navigáció
command-group-open = Megnyitás
command-group-view = Nézet
command-group-bar = Sáv

menu-close-vmux = Vmux bezárása

agents-terminal-coding-agent = Terminálalapú kódoló ügynök
