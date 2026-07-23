locale-name = eesti
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

tools-title = Tööriistad
tools-search = Otsi pakette, agente, MCP-d, keeletööriistu ja seadistusfaile…
tools-open = Ava tööriistad
tools-fold = Ahenda tööriistad
tools-unfold = Laienda tööriistad
tools-scanning = Kohalike tööriistade skannimine…
tools-no-installed = Installitud tööriistu pole
tools-empty = Sobivaid tööriistu pole
tools-empty-detail = Installi pakett või lisa Stow-laadis seadistusfailide pakett.
tools-apply = Rakenda
tools-homebrew = Homebrew
tools-homebrew-sync = Installitud valemid ja rakendused sünkroonitakse automaatselt.
tools-open-brewfile = Ava Brewfile
tools-managed = hallatud
tools-provider-homebrew-formulae = Homebrew’ valemid
tools-provider-homebrew-casks = Homebrew’ rakendused
tools-provider-npm = npm-paketid
tools-provider-acp-agents = ACP-agendid
tools-provider-language-tools = Keeletööriistad
tools-provider-mcp-servers = MCP-serverid
tools-provider-dotfiles = Seadistusfailid
tools-status-available = Saadaval
tools-status-missing = Puudub
tools-status-conflict = Konflikt
tools-forget = Unusta
tools-manage = Halda
tools-link = Lingi
tools-unlink = Eemalda link
tools-import = Impordi
tools-update-count = { $count ->
    [one] 1 värskendus
   *[other] { $count } värskendust
}
tools-conflict-count = { $count ->
    [one] 1 konflikt
   *[other] { $count } konflikti
}
tools-result-applied = Tööriistad rakendatud
tools-result-imported = Tööriistad imporditud
tools-result-installed = { $name } installitud
tools-result-updated = { $name } värskendatud
tools-result-uninstalled = { $name } desinstallitud
tools-result-forgotten = { $name } unustatud
tools-result-managed = { $name } on nüüd hallatud
tools-result-linked = { $name } lingitud
tools-result-unlinked = { $name } link eemaldatud

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

settings-empty = (tühi)
settings-none = (puudub)

schema-system = Süsteem
schema-editor = Redaktor
schema-recording = Salvestamine
schema-radius = Raadius
schema-padding = Sisetühik
schema-gap = Vahe
schema-width = Laius
schema-color = Värv
schema-red = Punane
schema-green = Roheline
schema-blue = Sinine
schema-follow-files = Jälgi faile
schema-tidy-files = Korrasta faile
schema-tidy-files-max = Failide korrastamise lävi
schema-tidy-files-auto = Korrasta faile automaatselt
schema-app-providers = Rakendusepakkujad
schema-provider = Pakkuja
schema-kind = Tüüp
schema-models = Mudelid
schema-acp = ACP agendid
schema-id = ID
schema-name = Nimi
schema-command = Käsk
schema-arguments = Argumendid
schema-environment = Keskkond
schema-working-directory = Töökataloog
schema-shell = Kest
schema-font-family = Fondipere
schema-startup-directory = Käivituskataloog
schema-themes = Teemad
schema-color-scheme = Värviskeem
schema-font-size = Fondi suurus
schema-line-height = Reakõrgus
schema-cursor-style = Kursori stiil
schema-cursor-blink = Kursori vilkumine
schema-custom-themes = Kohandatud teemad
schema-foreground = Esiplaan
schema-background = Taust
schema-cursor = Kursor
schema-ansi-colors = ANSI värvid
schema-keymap = Klahvikaart
schema-explorer = Sirvija
schema-visible = Nähtav
schema-language-servers = Keelelserverid
schema-servers = Serverid
schema-language-id = Keele ID
schema-root-markers = Juurmarkerid
schema-output-directory = Väljundkataloog

menu-scene = Stseen
menu-layout = Paigutus
menu-terminal = Terminal
menu-browser = Brauser
menu-service = Teenus
menu-bookmark = Järjehoidja
menu-edit = Redigeerimine

layout-knowledge = Teadmised
layout-open-knowledge = Ava teadmised
layout-open-welcome-knowledge = Ava „Tere tulemast teadmistesse“
layout-open-path = Ava { $path }
layout-fold-knowledge = Ahenda teadmised
layout-unfold-knowledge = Laienda teadmised
layout-bookmarks = Järjehoidjad
layout-new-folder = Uus kaust
layout-add-to-bookmarks = Lisa järjehoidjatesse
layout-move-to-bookmarks = Teisalda järjehoidjatesse
layout-stack-number = Virn { $number }
layout-fold-stack = Ahenda virn
layout-unfold-stack = Laienda virn
layout-close-stack = Sulge virn
layout-bookmark-in = Lisa järjehoidja kausta { $folder }

common-cancel = Tühista
common-delete = Kustuta
common-save = Salvesta
common-rename = Nimeta ümber
common-expand = Laienda
common-collapse = Ahenda
common-loading = Laadimine…
common-error = Viga
common-output = Väljund
common-pending = Ootel
common-current = praegune
common-stop = Peata
services-command = Vmuxi teenus
services-uptime-seconds = { $seconds } s
services-uptime-minutes = { $minutes } min { $seconds } s
services-uptime-hours = { $hours } h { $minutes } min
services-uptime-days = { $days } p { $hours } h

error-page-failed-load = Lehe laadimine nurjus
error-page-not-found = Lehte ei leitud
error-unknown-host = Tundmatu Vmuxi rakenduse host: { $host }

history-title = Ajalugu

command-new-app-chat = Uus { $provider }/{ $model } vestlus (rakendus)
command-interactive-mode-user = Scene > Interaktiivne režiim > Kasutaja
command-interactive-mode-player = Scene > Interaktiivne režiim > Mängija
command-minimize-window = Layout > Aken > Minimeeri
command-toggle-layout = Layout > Paigutus > Lülita paigutust
command-close-tab = Layout > Vahekaart > Sulge vahekaart
command-new-task = Layout > Vahekaart > Uus ülesanne…
command-next-tab = Layout > Vahekaart > Järgmine vahekaart
command-prev-tab = Layout > Vahekaart > Eelmine vahekaart
command-rename-tab = Layout > Vahekaart > Nimeta vahekaart ümber
command-tab-select-1 = Layout > Vahekaart > Vali vahekaart 1
command-tab-select-2 = Layout > Vahekaart > Vali vahekaart 2
command-tab-select-3 = Layout > Vahekaart > Vali vahekaart 3
command-tab-select-4 = Layout > Vahekaart > Vali vahekaart 4
command-tab-select-5 = Layout > Vahekaart > Vali vahekaart 5
command-tab-select-6 = Layout > Vahekaart > Vali vahekaart 6
command-tab-select-7 = Layout > Vahekaart > Vali vahekaart 7
command-tab-select-8 = Layout > Vahekaart > Vali vahekaart 8
command-tab-select-last = Layout > Vahekaart > Vali viimane vahekaart
command-close-pane = Layout > Paan > Sulge paan
command-select-pane-left = Layout > Paan > Vali vasak paan
command-select-pane-right = Layout > Paan > Vali parem paan
command-select-pane-up = Layout > Paan > Vali ülemine paan
command-select-pane-down = Layout > Paan > Vali alumine paan
command-swap-pane-prev = Layout > Paan > Vaheta eelmise paaniga
command-swap-pane-next = Layout > Paan > Vaheta järgmise paaniga
command-equalize-pane-size = Layout > Paan > Ühtlusta paanide suurus
command-resize-pane-left = Layout > Paan > Muuda paani suurust vasakule
command-resize-pane-right = Layout > Paan > Muuda paani suurust paremale
command-resize-pane-up = Layout > Paan > Muuda paani suurust üles
command-resize-pane-down = Layout > Paan > Muuda paani suurust alla
command-stack-close = Layout > Stack > Sulge stack
command-stack-next = Layout > Stack > Järgmine stack
command-stack-previous = Layout > Stack > Eelmine stack
command-stack-reopen = Layout > Stack > Ava suletud leht uuesti
command-stack-swap-prev = Layout > Stack > Liiguta stack vasakule
command-stack-swap-next = Layout > Stack > Liiguta stack paremale
command-space-open = Layout > Space > Space’id
command-terminal-close = Terminal > Sulge terminal
command-terminal-next = Terminal > Järgmine terminal
command-terminal-prev = Terminal > Eelmine terminal
command-terminal-clear = Terminal > Tühjenda terminal
command-browser-prev-page = Browser > Navigeerimine > Tagasi
command-browser-next-page = Browser > Navigeerimine > Edasi
command-browser-reload = Browser > Navigeerimine > Laadi uuesti
command-browser-hard-reload = Browser > Navigeerimine > Laadi täielikult uuesti
command-open-in-place = Browser > Ava > Ava siin
command-open-in-new-stack = Browser > Ava > Ava uues stack’is
command-open-in-pane-top = Browser > Ava > Ava ülemises paanis
command-open-in-pane-right = Browser > Ava > Ava paremas paanis
command-open-in-pane-bottom = Browser > Ava > Ava alumises paanis
command-open-in-pane-left = Browser > Ava > Ava vasakus paanis
command-open-in-new-tab = Browser > Ava > Ava uuel vahekaardil
command-open-in-new-space = Browser > Ava > Ava uues space’is
command-browser-zoom-in = Browser > Vaade > Suurenda
command-browser-zoom-out = Browser > Vaade > Vähenda
command-browser-zoom-reset = Browser > Vaade > Tegelik suurus
command-browser-dev-tools = Browser > Vaade > Arendaja tööriistad
command-browser-open-command-bar = Browser > Riba > Käsuriba
command-browser-open-page-in-command-bar = Browser > Riba > Muuda lehte
command-browser-open-path-bar = Browser > Riba > Asukohanavigaator
command-browser-open-commands = Browser > Riba > Käsud
command-browser-open-history = Browser > Riba > Ajalugu
command-service-open = Service > Ava teenusemonitor
command-bookmark-toggle-active = Bookmark > Lisa leht järjehoidjatesse
command-bookmark-pin-active = Bookmark > Kinnita leht

layout-tab = Vahekaart
layout-no-stacks = Stack’e pole
layout-loading = Laadimine…
layout-no-markdown-files = Markdowni faile pole
layout-empty-folder = Tühi kaust
layout-worktree = tööpuu
layout-folder-name = Kausta nimi
layout-no-pins-bookmarks = Kinnitusi ega järjehoidjaid pole
layout-move-to = Teisalda kausta { $folder }
layout-bookmark-current-page = Lisa praegune leht järjehoidjatesse
layout-rename-folder = Nimeta kaust ümber
layout-remove-folder = Eemalda kaust
layout-update-downloading = Värskenduse allalaadimine
layout-update-installing = Värskenduse installimine…
layout-update-ready = Uus versioon on saadaval
layout-restart-update = Värskendamiseks taaskäivita

agent-preparing = Agendi ettevalmistamine…
agent-send-all-queued = Saada kõik järjekorras viibad kohe (Esc)
agent-send = Saada (Enter)
agent-ready = Valmis, kui sina oled.
agent-loading-older = Vanemate sõnumite laadimine…
agent-load-older = Laadi vanemad sõnumid
agent-continued-from = Jätkatud allikast { $source }
agent-older-context-omitted = vanem kontekst välja jäetud
agent-interrupted = katkestatud
agent-allow-tool = Lubada tööriist { $tool }?
agent-deny = Keela
agent-allow-always = Luba alati
agent-allow = Luba
agent-loading-sessions = Seansside laadimine…
agent-no-resumable-sessions = Jätkatavaid seansse ei leitud
agent-no-matching-sessions = Sobivaid seansse pole
agent-no-matching-models = Sobivaid mudeleid pole
agent-choice-help = ↑/↓ või Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Vali repositooriumi kaust
agent-choose-repository-detail = Vali kohalik Giti repositoorium, mida agent peaks kasutama.
agent-choosing = Valimine…
agent-choose-folder = Vali kaust
agent-queued = järjekorras
agent-attached = Manustatud:
agent-cancel-queued = Tühista järjekorras viip
agent-resume-queued = Jätka järjekorras viipasid
agent-clear-queue = Tühjenda järjekord
agent-send-all-now = saada kõik kohe
agent-choose-option = Vali ülalt üks suvand
agent-loading-media = Meedia laadimine…
agent-no-matching-media = Sobivat meediat pole
agent-prompt-context = Viiba kontekst
agent-details = Üksikasjad
agent-path = Tee
agent-tool = Tööriist
agent-server = Server
agent-bytes = { $count } baiti
agent-worked-for = Töötas { $duration }
agent-worked-for-steps = { $count ->
    [one] Töötas { $duration } · 1 samm
   *[other] Töötas { $duration } · { $count } sammu
}
agent-tool-guardian-review = Guardiani ülevaatus
agent-tool-read-files = Luges faile
agent-tool-viewed-image = Vaatas pilti
agent-tool-used-browser = Kasutas brauserit
agent-tool-searched-files = Otsis faile
agent-tool-ran-commands = Käivitas käske
agent-thinking = Mõtleb
agent-subagent = Alamagent
agent-prompt = Viip
agent-thread = Lõim
agent-parent = Ülem
agent-children = Alamad
agent-call = Kutse
agent-raw-event = Töötlemata sündmus
agent-plan = Plaan
agent-tasks = { $count ->
    [one] 1 ülesanne
   *[other] { $count } ülesannet
}
agent-edited = Muudetud
agent-reconnecting = Ühenduse taastamine { $attempt }/{ $total }
agent-status-running = Töötab
agent-status-done = Valmis
agent-status-failed = Nurjus
agent-status-pending = Ootel
agent-slash-attach-files = Manusta faile
agent-slash-resume-session = Jätka varasemat seanssi
agent-slash-select-model = Vali mudel
agent-slash-continue-cli = Jätka seda seanssi CLI-s
agent-session-just-now = just praegu
agent-session-minutes-ago = { $count } min tagasi
agent-session-hours-ago = { $count } h tagasi
agent-session-days-ago = { $count } p tagasi
agent-working-working = Töötab
agent-working-thinking = Mõtleb
agent-working-pondering = Kaalub
agent-working-noodling = Nuputab
agent-working-percolating = Laagerdub
agent-working-conjuring = Loob
agent-working-cooking = Keedab plaani
agent-working-brewing = Hautab
agent-working-musing = Mõtiskleb
agent-working-ruminating = Juurdleb
agent-working-scheming = Sepitseb
agent-working-synthesizing = Sünteesib
agent-working-tinkering = Nokitseb
agent-working-churning = Töötleb
agent-working-vibing = Häälestub
agent-working-simmering = Podiseb
agent-working-crafting = Meisterdab
agent-working-divining = Ennustab
agent-working-mulling = Vaeb
agent-working-spelunking = Kaevub

editor-toggle-explorer = Lülita Explorer sisse/välja (Cmd+B)
editor-unsaved = salvestamata
editor-rendered-markdown = Renderdatud Markdown reaalajas muutmisega
editor-note = Märkus
editor-source-editor = Lähtekoodi redaktor
editor-editor = Redaktor
editor-git-diff = Giti diff
editor-diff = Diff
editor-tidy = Korrasta
editor-always = Alati
editor-unchanged-previews = { $count ->
    [one] ✦ 1 muutmata eelvaade
   *[other] ✦ { $count } muutmata eelvaadet
}
editor-open-externally = Ava välises rakenduses
editor-changed-line = Muudetud rida
editor-go-to-definition = Mine definitsiooni juurde
editor-find-references = Otsi viiteid
editor-references = { $count ->
    [one] 1 viide
   *[other] { $count } viidet
}
editor-lsp-starting = { $server } käivitub…
editor-lsp-not-installed = { $server } — pole installitud
editor-explorer = Explorer
editor-open-editors = Avatud redaktorid
editor-outline = Struktuur
editor-new-file = Uus fail
editor-new-folder = Uus kaust
editor-delete-confirm = Kas kustutada „{ $name }”? Seda ei saa tagasi võtta.
editor-created-folder = Kaust { $name } loodud
editor-created-file = Fail { $name } loodud
editor-renamed-to = Uus nimi: { $name }
editor-deleted = Kustutatud: { $name }
editor-failed-decode-image = Pildi dekodeerimine nurjus
editor-preview-large-image = pilt (eelvaateks liiga suur)
editor-preview-binary = binaarfail
editor-preview-file = fail

git-status-clean = puhas
git-status-modified = muudetud
git-status-staged = lavastatud
git-status-staged-modified = lavastatud*
git-status-untracked = jälgimata
git-status-deleted = kustutatud
git-status-conflict = konflikt
git-accept-all = ✓ nõustu kõigiga
git-unstage = Eemalda lavastusest
git-confirm-deny-all = Kinnita kõigist keeldumine
git-deny-all = ✗ keeldu kõigist
git-commit-message = commiti sõnum
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Diffi laadimine…
git-no-changes = Muudatusi pole näidata
git-accept = ✓ nõustu
git-deny = ✗ keeldu
git-show-unchanged-lines = Kuva { $count } muutmata rida

terminal-loading = Laadimine…
terminal-runs-when-ready = käivitub, kui valmis · Ctrl+C tühjendab · Esc jätab vahele
terminal-booting = käivitub
terminal-type-command = sisesta käsk · käivitub, kui valmis · Esc jätab vahele

setup-tagline-claude = Anthropicu koodiagent Vmuxis
setup-tagline-codex = OpenAI koodiagent Vmuxis
setup-tagline-vibe = Mistrali koodiagent Vmuxis
setup-install-title = Installi { $name } CLI
setup-homebrew-required = { $command } installimiseks on vaja Homebrew’d, kuid see pole veel seadistatud. Vmux installib esmalt Homebrew ja seejärel { $name }.
setup-terminal-instructions = Vajuta terminalis alustamiseks Return ja sisesta küsimisel oma Maci parool.
setup-command-missing = Vmux avas selle lehe, sest kohalik käsk { $command } pole veel installitud. Selle hankimiseks käivita allolev käsk.
setup-install-failed = Installimine ei lõppenud. Vaata üksikasju terminalist ja proovi uuesti.
setup-installing = Installimine…
setup-install-homebrew = Installi Homebrew + { $name }
setup-run-install = Käivita installikäsk
setup-auto-reload = Vmux käivitab selle terminalis ja laadib uuesti, kui { $command } on valmis.

debug-title = Silumine
debug-auto-update = Automaatne värskendamine
debug-simulate-update = Simuleeri saadaolevat värskendust
debug-simulate-download = Simuleeri allalaadimist
debug-clear-update = Tühjenda värskendus
debug-trigger-restart = Käivita taaskäivitus

command-manage-spaces = Halda ruume…
command-pane-stack-location = paan { $pane } / virn { $stack }
command-space-pane-stack-location = { $space } / paan { $pane } / virn { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Interaktiivne režiim
command-group-window = Aken
command-group-tab = Vahekaart
command-group-pane = Paan
command-group-stack = Virn
command-group-space = Ruum
command-group-navigation = Navigeerimine
command-group-open = Ava
command-group-view = Vaade
command-group-bar = Riba

menu-close-vmux = Sulge Vmux

agents-terminal-coding-agent = Terminalipõhine koodiagent
