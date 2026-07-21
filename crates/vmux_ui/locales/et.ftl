common-open = Ava
common-close = Sulge
common-install = Paigalda
common-uninstall = Eemalda
common-update = Värskenda
common-retry = Proovi uuesti
common-refresh = Värskenda
common-remove = Eemalda
common-enable = Luba
common-disable = Keela
common-new = Uus
common-active = aktiivne
common-running = töötab
common-done = valmis
common-failed = Nurjus
common-installed = Paigaldatud
common-items = { $count ->
    [one] { $count } üksus
   *[other] { $count } üksust
}
start-title = Alusta
start-tagline = Üks prompt. Kõik tehtud.

agents-title = Agendid
agents-search = Otsi ACP ja CLI agente…
agents-empty = Sobivaid agente pole
agents-empty-detail = Proovi nime, käituskeskkonda või ACP/CLI-d.
agents-install-failed = Paigaldamine nurjus
agents-updating = Värskendatakse…
agents-retrying = Proovitakse uuesti…
agents-preparing = Valmistatakse ette…

extensions-title = Laiendused
extensions-search = Otsi paigaldatutest või Chrome Web Store’ist…
extensions-relaunch = Rakendamiseks käivita uuesti
extensions-empty = Laiendusi pole paigaldatud
extensions-no-match = Sobivaid laiendusi pole
extensions-empty-detail = Otsi ülal Chrome Web Store’ist ja vajuta sisestusklahvi.
extensions-no-match-detail = Proovi teist nime või laienduse ID-d.
extensions-on = Sees
extensions-off = Väljas
extensions-enable-confirm = Lubada { $name }?
extensions-enable-permissions = Luba { $name } ja anna õigused:

lsp-title = Keeleserverid
lsp-search = Otsi keeleservereid, lintreid, vormindajaid…
lsp-loading = Kataloogi laadimine…
lsp-empty = Sobivaid keeleservereid pole
lsp-empty-detail = Proovi teist keelt, lintrit või vormindajat.
lsp-needs = vajab tööriista { $tool }
lsp-status-available = Saadaval
lsp-status-on-path = PATH-is
lsp-status-installing = Paigaldatakse…
lsp-status-installed = Paigaldatud
lsp-status-outdated = Värskendus saadaval
lsp-status-running = Töötab
lsp-status-failed = Nurjus

spaces-title = Tööruumid
spaces-new-placeholder = Uue tööruumi nimi
spaces-empty = Tööruume pole
spaces-default-name = Tööruum { $number }
spaces-tabs = { $count ->
    [one] 1 kaart
   *[other] { $count } kaarti
}
spaces-delete = Kustuta tööruum

team-title = Tiim
team-just-you = Selles tööruumis oled ainult sina
team-agents = { $count ->
    [one] Sina ja 1 agent
   *[other] Sina ja { $count } agenti
}
team-empty = Siin pole veel kedagi
team-you = Sina
team-agent = Agent

services-title = Taustateenused
services-processes = { $count ->
    [one] 1 protsess
   *[other] { $count } protsessi
}
services-kill-all = Lõpeta kõik jõuga
services-not-running = Teenus ei tööta
services-start-with = Käivita käsuga:
services-empty = Aktiivseid protsesse pole
services-filter = Filtreeri protsesse…
services-no-match = Sobivaid protsesse pole
services-connected = Ühendatud
services-disconnected = Ühenduseta
services-attached = ühendatud
services-kill = Lõpeta jõuga
services-memory = Mälu
services-size = Suurus
services-shell = Kest

error-title = Viga

history-search = Otsi ajaloost
history-clear-all = Tühjenda kõik
history-clear-confirm = Tühjendada kogu ajalugu?
history-clear-warning = Seda ei saa tagasi võtta.
history-cancel = Loobu
history-today = Täna
history-yesterday = Eile
history-days-ago = { $count } päeva tagasi
history-day-offset = Päev -{ $count }

settings-title = Seaded
settings-loading = Seadete laadimine…
settings-stored = Salvestatud faili ~/.vmux/settings.ron
settings-other = Muu
settings-software-update = Tarkvaravärskendus
settings-check-updates = Kontrolli värskendusi
settings-check-updates-hint = Kui automaatvärskendus on lubatud, kontrollitakse käivitamisel ja iga tunni järel.
settings-update-unavailable = Pole saadaval
settings-update-unavailable-hint = See järk ei sisalda värskendajat.
settings-update-checking = Kontrollitakse…
settings-update-checking-hint = Värskenduste kontrollimine…
settings-update-check-again = Kontrolli uuesti
settings-update-current = Vmux on ajakohane.
settings-update-downloading = Laaditakse alla…
settings-update-downloading-hint = Vmux { $version } allalaadimine…
settings-update-installing = Paigaldatakse…
settings-update-installing-hint = Vmux { $version } paigaldamine…
settings-update-ready = Värskendus on valmis
settings-update-ready-hint = Vmux { $version } on valmis. Rakendamiseks taaskäivita.
settings-update-try-again = Proovi uuesti
settings-update-failed = Värskendusi ei õnnestunud kontrollida.
settings-item = Üksus
settings-item-number = Üksus { $number }
settings-press-key = Vajuta klahvi…
settings-saved = Salvestatud
settings-record-key = Klõpsa uue klahvikombinatsiooni salvestamiseks

tray-open-window = Ava aken
tray-close-window = Sulge aken
tray-pause-recording = Peata salvestamine
tray-resume-recording = Jätka salvestamist
tray-finish-recording = Lõpeta salvestamine
tray-quit = Välju Vmuxist

composer-attach-files = Lisa failid (/upload)
composer-remove-attachment = Eemalda manus

layout-back = Tagasi
layout-forward = Edasi
layout-reload = Laadi uuesti
layout-bookmark-page = Lisa see leht järjehoidjatesse
layout-remove-bookmark = Eemalda järjehoidja
layout-pin-page = Kinnita see leht
layout-unpin-page = Eemalda selle lehe kinnitus
layout-manage-extensions = Halda laiendusi
layout-new-stack = Uus virn
layout-close-tab = Sulge kaart
layout-bookmark = Järjehoidja
layout-pin = Kinnita
layout-new-tab = Uus kaart
layout-team = Tiim

command-switch-space = Vaheta tööruumi…
command-search-ask = Otsi või küsi…
command-new-tab-placeholder = Otsi, sisesta URL või vali Terminal…
command-placeholder = Sisesta URL, otsi kaartidelt või kasuta käskudeks märki >…
command-composer-placeholder = Käskudeks sisesta /, meedia jaoks @
command-send = Saada (Enter)
command-terminal = Terminal
command-open-terminal = Ava terminalis
command-stack = Virn
command-tabs = { $count ->
    [one] 1 kaart
   *[other] { $count } kaarti
}
command-prompt = Prompt
command-new-tab = Uus kaart
command-search = Otsi
command-open-value = Ava „{ $value }”
command-search-value = Otsi „{ $value }”

schema-appearance = Välimus
schema-general = Üldine
schema-layout = Paigutus
schema-layout-detail = Aken, paanid, külgriba ja fookuseraam.
schema-agent = Agent
schema-agent-detail = Agendi käitumine ja tööriistade õigused.
schema-shortcuts = Kiirklahvid
schema-shortcuts-detail = Ainult lugemiseks. Seoste muutmiseks muuda otse faili settings.ron.
schema-terminal = Terminal
schema-browser = Brauser
schema-mode = Režiim
schema-mode-detail = Veebilehtede värviskeem. Seade järgib süsteemi.
schema-device = Seade
schema-light = Hele
schema-dark = Tume
schema-language = Keel
schema-language-detail = Kasuta süsteemi keelt, en-US, ja või mis tahes BCP 47 märgendit, millele vastab ~/.vmux/locales/<tag>.ftl kataloog.
schema-auto-update = Automaatvärskendus
schema-auto-update-detail = Kontrolli ja paigalda värskendusi käivitamisel ning iga tunni järel.
schema-startup-url = Käivitus-URL
schema-startup-url-detail = Tühja väärtuse korral avaneb käsuriba prompt.
schema-search-engine = Otsingumootor
schema-search-engine-detail = Kasutatakse veebiotsinguteks avalehelt ja käsuribalt.
schema-window = Aken
schema-pane = Paan
schema-side-sheet = Külgpaneel
schema-focus-ring = Fookuseraam
schema-run-placement = Luba käivituse paigutuse ülekirjutamine
schema-run-placement-detail = Luba agentidel valida käivituspaani režiim, suund ja ankur.
schema-leader = Juhtklahv
schema-leader-detail = Eesliide akordkiirklahvide jaoks.
schema-chord-timeout = Akordi aegumine
schema-chord-timeout-detail = Millisekundid, mille järel akordi eesliide aegub.
schema-bindings = Seosed
schema-confirm-close = Sulgemise kinnitus
schema-confirm-close-detail = Küsi kinnitust enne töötava protsessiga terminali sulgemist.
schema-default-theme = Vaiketeema
schema-default-theme-detail = Aktiivse teema nimi teemade loendist.
