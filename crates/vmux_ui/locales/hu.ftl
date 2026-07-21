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
