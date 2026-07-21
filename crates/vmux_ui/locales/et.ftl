common-open = Avatud
common-close = Sule
common-install = Installige
common-uninstall = Desinstalli
common-update = Värskenda
common-retry = Proovi uuesti
common-refresh = Värskenda
common-remove = Eemalda
common-enable = Luba
common-disable = Keela
common-new = Uus
common-active = aktiivne
common-running = jooksmine
common-done = tehtud
common-failed = Ebaõnnestunud
common-installed = Paigaldatud
common-items = { $count ->
    [one] { $count } üksus
   *[other] { $count } üksust
}
start-title = Alusta
start-tagline = Üks käsk. Midagi, tehtud.

agents-title = Agendid
agents-search = Otsi ACP ja CLI agente…
agents-empty = Sobivaid agente pole
agents-empty-detail = Proovige nime, käitusaega või ACP/CLI.
agents-install-failed = Install ebaõnnestus
agents-updating = Värskendamine…
agents-retrying = Uuesti proovimine…
agents-preparing = Ettevalmistus…

extensions-title = Laiendused
extensions-search = Otsige installitud või Chrome Web Store…
extensions-relaunch = Taotlemiseks taaskäivitage
extensions-empty = Laiendusi pole installitud
extensions-no-match = Sobivaid laiendeid pole
extensions-empty-detail = Otsige ülalolevast Chrome Web Store ja vajutage Return.
extensions-no-match-detail = Proovige teist nime või laienduse ID-d.
extensions-on = Sees
extensions-off = Väljas
extensions-enable-confirm = Kas lubada { $name }?
extensions-enable-permissions = Lubage { $name } ja lubage:

lsp-title = Keeleserverid
lsp-search = Otsi keeleserveritest, linteritest, vormindajatest…
lsp-loading = Kataloogi laadimine…
lsp-empty = Sobivaid keeleservereid pole
lsp-empty-detail = Proovige mõnda muud keelt, linterit või vormindajat.
lsp-needs = vajab { $tool }
lsp-status-available = Saadaval
lsp-status-on-path = PATH
lsp-status-installing = Installimine…
lsp-status-installed = Paigaldatud
lsp-status-outdated = Värskendus saadaval
lsp-status-running = Jooksmine
lsp-status-failed = Ebaõnnestunud

spaces-title = Ruumid
spaces-new-placeholder = Uus ruumi nimi
spaces-empty = Tühikuid pole
spaces-default-name = Ruum { $number }
spaces-tabs = { $count ->
    [one] 1 vahekaart
   *[other] { $count } vahekaarte
}
spaces-delete = Kustuta tühik

team-title = Meeskond
team-just-you = Ainult sina selles ruumis
team-agents = { $count ->
    [one] Sina ja 1 agent
   *[other] Teie ja { $count } agendid
}
team-empty = Siin pole veel kedagi
team-you = Sina
team-agent = Agent

services-title = Taustateenused
services-processes = { $count ->
    [one] 1 protsess
   *[other] { $count } protsessid
}
services-kill-all = Tapa kõik
services-not-running = Teenus ei tööta
services-start-with = Alustage:
services-empty = Aktiivseid protsesse pole
services-filter = Filtreeri protsessid…
services-no-match = Ühtegi sobitamisprotsesse pole
services-connected = Ühendatud
services-disconnected = Ühendus katkestatud
services-attached = lisatud
services-kill = Tapa
services-memory = Mälu
services-size = Suurus
services-shell = Kest

error-title = Viga

history-search = Otsinguajalugu
history-clear-all = Tühjenda kõik
history-clear-confirm = Kas kustutada kogu ajalugu?
history-clear-warning = Seda ei saa tagasi võtta.
history-cancel = Tühista
history-today = Täna
history-yesterday = eile
history-days-ago = { $count } päeva tagasi
history-day-offset = Päev -{ $count }

settings-title = Seaded
settings-loading = Seadete laadimine…
settings-stored = Salvestatud ~/.vmux/settings.ron
settings-other = muud
settings-software-update = Tarkvara värskendus
settings-check-updates = Kontrollige värskendusi
settings-check-updates-hint = Kontrollib automaatselt käivitamisel ja iga tunni järel, kui automaatvärskendus on lubatud.
settings-update-unavailable = Pole saadaval
settings-update-unavailable-hint = Värskendaja ei sisaldu selles järgus.
settings-update-checking = Kontrollimine…
settings-update-checking-hint = Värskenduste otsimine…
settings-update-check-again = Kontrollige uuesti
settings-update-current = Vmux on ajakohane.
settings-update-downloading = Allalaadimine…
settings-update-downloading-hint = Vmux { $version } allalaadimine…
settings-update-installing = Installimine…
settings-update-installing-hint = Vmux { $version } installimine…
settings-update-ready = Värskendus valmis
settings-update-ready-hint = Vmux { $version } on valmis. Selle rakendamiseks taaskäivitage.
settings-update-try-again = Proovige uuesti
settings-update-failed = Värskendusi ei saa kontrollida.
settings-item = Üksus
settings-item-number = Üksus { $number }
settings-press-key = Vajutage klahvi…
settings-saved = Salvestatud
settings-record-key = Klõpsake uue klahvikombo salvestamiseks

tray-open-window = Ava aken
tray-close-window = Sule aken
tray-pause-recording = Peata salvestamine
tray-resume-recording = Jätka salvestamist
tray-finish-recording = Lõpeta salvestamine
tray-quit = Lõpeta Vmux

composer-attach-files = Failide manustamine (/upload)
composer-remove-attachment = Eemalda manus

layout-back = Tagasi
layout-forward = Edasi
layout-reload = Laadi uuesti
layout-bookmark-page = Lisa see leht järjehoidjatesse
layout-remove-bookmark = Eemalda järjehoidja
layout-pin-page = Kinnitage see leht
layout-unpin-page = Vabastage see leht
layout-manage-extensions = Laienduste haldamine
layout-new-stack = Uus virn
layout-close-tab = Sule vahekaart
layout-bookmark = Järjehoidja
layout-pin = Pin
layout-new-tab = Uus vahekaart
layout-team = Meeskond

command-switch-space = Vaheta ruumi…
command-search-ask = Otsi või küsi…
command-new-tab-placeholder = Otsige või tippige URL või valige Terminal…
command-placeholder = Tippige käskude jaoks URL, otsige vahekaarte või >...
command-composer-placeholder = Sisestage käskude jaoks / või meediumide jaoks @
command-send = Saada (Enter)
command-terminal = Terminal
command-open-terminal = Avage terminalis
command-stack = Virna
command-tabs = { $count ->
    [one] 1 vahekaart
   *[other] { $count } vahekaarte
}
command-prompt = Viip
command-new-tab = Uus vahekaart
command-search = Otsi
command-open-value = Ava "{ $value }"
command-search-value = Otsi "{ $value }"

schema-appearance = Välimus
schema-general = Kindral
schema-layout = Paigutus
schema-layout-detail = Aken, paanid, külgriba ja teravustamisrõngas.
schema-agent = Agent
schema-agent-detail = Agendi käitumine ja tööriista load.
schema-shortcuts = Otseteed
schema-shortcuts-detail = Kirjutuskaitstud vaade. Sidemete muutmiseks muutke settings.ron otse.
schema-terminal = Terminal
schema-browser = Brauser
schema-mode = Režiim
schema-mode-detail = Veebilehtede värviskeem. Seade järgib teie süsteemi.
schema-device = Seade
schema-light = Valgus
schema-dark = Tume
schema-language = Keel
schema-language-detail = Kasutage süsteemi, en-US, ja või mis tahes BCP 47 märgendit sobiva ~/.vmux/locales/<tag>.ftl kataloogiga.
schema-auto-update = Automaatne värskendus
schema-auto-update-detail = Kontrollige ja installige värskendusi käivitamisel ja iga tund.
schema-startup-url = Käivitamine URL
schema-startup-url-detail = Tühi avab käsuriba viipa.
schema-search-engine = Otsingumootor
schema-search-engine-detail = Kasutatakse veebiotsinguks Start ja käsuribalt.
schema-window = Aken
schema-pane = Paan
schema-side-sheet = Külgleht
schema-focus-ring = Fookusrõngas
schema-run-placement = Luba käitamise paigutuse alistamine
schema-run-placement-detail = Laske agentidel valida käitamispaani režiim, suund ja ankur.
schema-leader = Juht
schema-leader-detail = Prefiksklahv akordi otseteede jaoks.
schema-chord-timeout = Akordi ajalõpp
schema-chord-timeout-detail = Millisekundeid enne akordi eesliite aegumist.
schema-bindings = Köited
schema-confirm-close = Kinnitage sulgemine
schema-confirm-close-detail = Küsi enne töötava protsessiga terminali sulgemist.
schema-default-theme = Vaiketeema
schema-default-theme-detail = Aktiivse teema nimi teemade loendist.
