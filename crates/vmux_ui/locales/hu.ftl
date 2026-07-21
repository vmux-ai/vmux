common-open = Nyissa meg
common-close = Bezárás
common-install = Telepítés
common-uninstall = Eltávolítás
common-update = Frissítés
common-retry = Próbálja újra
common-refresh = Frissítés
common-remove = Távolítsa el
common-enable = Engedélyezés
common-disable = Letiltás
common-new = Új
common-active = aktív
common-running = futás
common-done = kész
common-failed = Sikertelen
common-installed = Telepítve
common-items = { $count ->
    [one] { $count } elem
   *[other] { $count } elem
}
start-title = Indítsa el
start-tagline = Egy felszólítás. Bármi, kész.

agents-title = Ügynökök
agents-search = Keresés ACP és CLI ügynökök között…
agents-empty = Nincsenek megfelelő ügynökök
agents-empty-detail = Próbáljon ki egy nevet, futási környezetet vagy ACP/CLI.
agents-install-failed = A telepítés sikertelen
agents-updating = Frissítés…
agents-retrying = Újrapróbálkozás…
agents-preparing = Felkészülés…

extensions-title = Kiterjesztések
extensions-search = Keresés telepítve vagy Chrome Web Store…
extensions-relaunch = A jelentkezéshez indítsa újra
extensions-empty = Nincsenek telepítve bővítmények
extensions-no-match = Nincsenek megfelelő bővítmények
extensions-empty-detail = Keressen a fenti Chrome Web Store között, és nyomja meg a Return gombot.
extensions-no-match-detail = Próbálkozzon másik névvel vagy bővítményazonosítóval.
extensions-on = Be
extensions-off = Ki
extensions-enable-confirm = Engedélyezi a { $name }?
extensions-enable-permissions = Engedélyezze a { $name } és engedélyezze:

lsp-title = Nyelvi szerverek
lsp-search = Nyelvi szerverek, linterek, formázók keresése…
lsp-loading = Katalógus betöltése…
lsp-empty = Nincsenek megfelelő nyelvű szerverek
lsp-empty-detail = Próbálkozzon másik nyelvvel, linterrel vagy formázóval.
lsp-needs = { $tool } kell
lsp-status-available = Elérhető
lsp-status-on-path = PATH
lsp-status-installing = Telepítés…
lsp-status-installed = Telepítve
lsp-status-outdated = Frissítés elérhető
lsp-status-running = Futás
lsp-status-failed = Sikertelen

spaces-title = Spaces
spaces-new-placeholder = Új térnév
spaces-empty = Nincs szóköz
spaces-default-name = Tér { $number }
spaces-tabs = { $count ->
    [one] 1 lap
   *[other] { $count } lapok
}
spaces-delete = Tér törlése

team-title = Csapat
team-just-you = Csak te ezen a téren
team-agents = { $count ->
    [one] Ön és 1 ügynök
   *[other] Ön és { $count } ügynökök
}
team-empty = Még nincs itt senki
team-you = Te
team-agent = ügynök

services-title = Háttérszolgáltatások
services-processes = { $count ->
    [one] 1 folyamat
   *[other] { $count } folyamatok
}
services-kill-all = Ölj meg mindent
services-not-running = A szolgáltatás nem fut
services-start-with = Kezdje ezzel:
services-empty = Nincsenek aktív folyamatok
services-filter = Folyamatok szűrése…
services-no-match = Nincsenek megfelelő folyamatok
services-connected = Csatlakozva
services-disconnected = Lekapcsolva
services-attached = csatolva
services-kill = Ölj meg
services-memory = Memória
services-size = Méret
services-shell = Shell

error-title = Hiba

history-search = Keresési előzmények
history-clear-all = Minden törlése
history-clear-confirm = Törli az összes előzményt?
history-clear-warning = Ezt nem lehet visszavonni.
history-cancel = Mégsem
history-today = Ma
history-yesterday = tegnap
history-days-ago = { $count } napja
history-day-offset = nap -{ $count }

settings-title = Beállítások elemre
settings-loading = Beállítások betöltése…
settings-stored = Tárolva: ~/.vmux/settings.ron
settings-other = Egyéb
settings-software-update = Szoftverfrissítés
settings-check-updates = Ellenőrizze a frissítéseket
settings-check-updates-hint = Automatikusan ellenőrzi indításkor és óránként, ha az automatikus frissítés engedélyezve van.
settings-update-unavailable = Nem elérhető
settings-update-unavailable-hint = Ez a build nem tartalmazza a frissítőt.
settings-update-checking = Ellenőrzés…
settings-update-checking-hint = Frissítések keresése…
settings-update-check-again = Ellenőrizze újra
settings-update-current = A Vmux naprakész.
settings-update-downloading = Letöltés…
settings-update-downloading-hint = Vmux { $version } letöltése…
settings-update-installing = Telepítés…
settings-update-installing-hint = Vmux { $version } telepítése…
settings-update-ready = Frissítés kész
settings-update-ready-hint = Vmux { $version } készen áll. Indítsa újra az alkalmazáshoz.
settings-update-try-again = Próbáld újra
settings-update-failed = Nem lehet frissítéseket keresni.
settings-item = Tétel
settings-item-number = { $number } elem
settings-press-key = Nyomj meg egy gombot…
settings-saved = Mentve
settings-record-key = Kattintson az új kulcskombó rögzítéséhez

tray-open-window = Nyissa meg az ablakot
tray-close-window = Zárja be az ablakot
tray-pause-recording = Felvétel szüneteltetése
tray-resume-recording = Felvétel folytatása
tray-finish-recording = Felvétel befejezése
tray-quit = Kilépés a Vmux alkalmazásból

composer-attach-files = Fájlok csatolása (/upload)
composer-remove-attachment = Távolítsa el a mellékletet

layout-back = Vissza
layout-forward = Előre
layout-reload = Újratöltés
layout-bookmark-page = Könyvjelzők közé ezt az oldalt
layout-remove-bookmark = Könyvjelző eltávolítása
layout-pin-page = Rögzítse ezt az oldalt
layout-unpin-page = Oldja fel az oldal rögzítését
layout-manage-extensions = Bővítmények kezelése
layout-new-stack = Új Stack
layout-close-tab = Lap bezárása
layout-bookmark = Könyvjelző
layout-pin = Pin
layout-new-tab = Új lap
layout-team = Csapat

command-switch-space = Helyváltás…
command-search-ask = Keress vagy kérdezz…
command-new-tab-placeholder = Keressen vagy írjon be egy URL, vagy válassza a Terminál…
command-placeholder = Írjon be egy URL, keressen tabulátorokat vagy > parancsokat…
command-composer-placeholder = Írja be a / parancsokat, vagy a @-t a média használatához
command-send = Küldés (Enter)
command-terminal = Terminál
command-open-terminal = Megnyitás a terminálban
command-stack = Verem
command-tabs = { $count ->
    [one] 1 lap
   *[other] { $count } lapok
}
command-prompt = Prompt
command-new-tab = Új lap
command-search = Keresés
command-open-value = Nyissa meg a következőt: „{ $value }”
command-search-value = Keresés „{ $value }”

schema-appearance = Megjelenés
schema-general = tábornok
schema-layout = Elrendezés
schema-layout-detail = Ablak, ablaktáblák, oldalsáv és fókuszgyűrű.
schema-agent = ügynök
schema-agent-detail = Ügynök viselkedése és eszközengedélyei.
schema-shortcuts = Parancsikonok
schema-shortcuts-detail = Csak olvasható nézet. Szerkessze közvetlenül a settings.ron fájlt a kötések módosításához.
schema-terminal = Terminál
schema-browser = Böngésző
schema-mode = mód
schema-mode-detail = Weboldalak színséma. Az eszköz követi a rendszert.
schema-device = Eszköz
schema-light = Fény
schema-dark = Sötét
schema-language = Nyelv
schema-language-detail = Használjon rendszert, en-US, ja-t vagy bármely BCP 47 címkét a megfelelő ~/.vmux/locales/<tag>.ftl katalógussal.
schema-auto-update = Automatikus frissítés
schema-auto-update-detail = Ellenőrizze és telepítse a frissítéseket indításkor és óránként.
schema-startup-url = Indítás URL
schema-startup-url-detail = Az üres megnyitja a parancssort.
schema-search-engine = Keresőmotor
schema-search-engine-detail = Internetes keresésekhez használatos a Startból és a parancssorból.
schema-window = Ablak
schema-pane = Panel
schema-side-sheet = Oldallap
schema-focus-ring = Fókusz gyűrű
schema-run-placement = Futtatási elhelyezés felülbírálásának engedélyezése
schema-run-placement-detail = Hagyja, hogy az ügynökök válasszák ki a futáspanel módot, irányt és horgonyzást.
schema-leader = Vezető
schema-leader-detail = Előtag gomb az akkord gyorsbillentyűkhöz.
schema-chord-timeout = Akkord időtúllépés
schema-chord-timeout-detail = Ezredmásodperccel az akkord előtag lejárta előtt.
schema-bindings = Kötések
schema-confirm-close = Erősítse meg a bezárást
schema-confirm-close-detail = Kérdezzen egy terminál bezárása előtt egy futó folyamattal.
schema-default-theme = Alapértelmezett téma
schema-default-theme-detail = Az aktív téma neve a témalistából.
