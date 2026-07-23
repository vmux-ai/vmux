locale-name = suomi
common-open = Avaa
common-close = Sulje
common-install = Asenna
common-uninstall = Poista asennus
common-update = Päivitä
common-retry = Yritä uudelleen
common-refresh = Päivitä
common-remove = Poista
common-enable = Ota käyttöön
common-disable = Poista käytöstä
common-new = Uusi
common-active = aktiivinen
common-running = käynnissä
common-done = valmis
common-failed = Epäonnistui
common-installed = Asennettu
common-items = { $count ->
    [one] { $count } kohde
   *[other] { $count } kohdetta
}

tools-title = Työkalut
tools-search = Hae paketteja, agentteja, MCP:tä, kielityökaluja ja määritystiedostoja…
tools-open = Avaa työkalut
tools-fold = Kutista työkalut
tools-unfold = Laajenna työkalut
tools-scanning = Tarkistetaan paikallisia työkaluja…
tools-no-installed = Ei asennettuja työkaluja
tools-empty = Ei vastaavia työkaluja
tools-empty-detail = Asenna paketti tai lisää Stow-tyylinen määritystiedostopaketti.
tools-apply = Käytä
tools-homebrew = Homebrew
tools-homebrew-sync = Asennetut kaavat ja sovellukset synkronoidaan automaattisesti.
tools-open-brewfile = Avaa Brewfile
tools-managed = hallinnoitu
tools-provider-homebrew-formulae = Homebrew-kaavat
tools-provider-homebrew-casks = Homebrew-sovellukset
tools-provider-npm = npm-paketit
tools-provider-acp-agents = ACP-agentit
tools-provider-language-tools = Kielityökalut
tools-provider-mcp-servers = MCP-palvelimet
tools-provider-dotfiles = Määritystiedostot
tools-status-available = Saatavilla
tools-status-missing = Puuttuu
tools-status-conflict = Ristiriita
tools-forget = Unohda
tools-manage = Hallinnoi
tools-link = Linkitä
tools-unlink = Poista linkitys
tools-import = Tuo
tools-update-count = { $count ->
    [one] 1 päivitys
   *[other] { $count } päivitystä
}
tools-conflict-count = { $count ->
    [one] 1 ristiriita
   *[other] { $count } ristiriitaa
}
tools-result-applied = Työkalut otettu käyttöön
tools-result-imported = Työkalut tuotu
tools-result-installed = { $name } asennettu
tools-result-updated = { $name } päivitetty
tools-result-uninstalled = { $name } poistettu
tools-result-forgotten = { $name } unohdettu
tools-result-managed = { $name } on nyt hallinnoitu
tools-result-linked = { $name } linkitetty
tools-result-unlinked = Kohteen { $name } linkitys poistettu

start-title = Aloita
start-tagline = Yksi kehote. Kaikki hoituu.

agents-title = Agentit
agents-search = Hae ACP- ja CLI-agentteja…
agents-empty = Ei vastaavia agentteja
agents-empty-detail = Kokeile nimeä, ajoympäristöä tai ACP/CLI:tä.
agents-install-failed = Asennus epäonnistui
agents-updating = Päivitetään…
agents-retrying = Yritetään uudelleen…
agents-preparing = Valmistellaan…

extensions-title = Laajennukset
extensions-search = Hae asennetuista tai Chrome Web Storesta…
extensions-relaunch = Käynnistä uudelleen, jotta muutokset tulevat voimaan
extensions-empty = Laajennuksia ei ole asennettu
extensions-no-match = Ei vastaavia laajennuksia
extensions-empty-detail = Hae yllä Chrome Web Storesta ja paina rivinvaihtonäppäintä.
extensions-no-match-detail = Kokeile toista nimeä tai laajennuksen tunnusta.
extensions-on = Päällä
extensions-off = Pois
extensions-enable-confirm = Otetaanko { $name } käyttöön?
extensions-enable-permissions = Ota { $name } käyttöön ja salli:

lsp-title = Kielipalvelimet
lsp-search = Hae kielipalvelimia, lintereitä tai muotoilijoita…
lsp-loading = Ladataan luetteloa…
lsp-empty = Ei vastaavia kielipalvelimia
lsp-empty-detail = Kokeile toista kieltä, linteriä tai muotoilijaa.
lsp-needs = vaatii työkalun { $tool }
lsp-status-available = Saatavilla
lsp-status-on-path = PATHissa
lsp-status-installing = Asennetaan…
lsp-status-installed = Asennettu
lsp-status-outdated = Päivitys saatavilla
lsp-status-running = Käynnissä
lsp-status-failed = Epäonnistui

spaces-title = Työtilat
spaces-new-placeholder = Uuden työtilan nimi
spaces-empty = Ei työtiloja
spaces-default-name = Työtila { $number }
spaces-tabs = { $count ->
    [one] 1 välilehti
   *[other] { $count } välilehteä
}
spaces-delete = Poista työtila

team-title = Tiimi
team-just-you = Vain sinä tässä työtilassa
team-agents = { $count ->
    [one] Sinä ja 1 agentti
   *[other] Sinä ja { $count } agenttia
}
team-empty = Täällä ei ole vielä ketään
team-you = Sinä
team-agent = Agentti

services-title = Taustapalvelut
services-processes = { $count ->
    [one] 1 prosessi
   *[other] { $count } prosessia
}
services-kill-all = Lopeta kaikki pakolla
services-not-running = Palvelu ei ole käynnissä
services-start-with = Käynnistystapa:
services-empty = Ei aktiivisia prosesseja
services-filter = Suodata prosesseja…
services-no-match = Ei vastaavia prosesseja
services-connected = Yhdistetty
services-disconnected = Ei yhteyttä
services-attached = liitetty
services-kill = Lopeta pakolla
services-memory = Muisti
services-size = Koko
services-shell = Komentotulkki

error-title = Virhe

history-search = Hae historiasta
history-clear-all = Tyhjennä kaikki
history-clear-confirm = Tyhjennetäänkö koko historia?
history-clear-warning = Tätä ei voi kumota.
history-cancel = Peruuta
history-today = Tänään
history-yesterday = Eilen
history-days-ago = { $count } päivää sitten
history-day-offset = Päivä -{ $count }

settings-title = Asetukset
settings-loading = Ladataan asetuksia…
settings-stored = Tallennettu tiedostoon ~/.vmux/settings.ron
settings-other = Muut
settings-software-update = Ohjelmistopäivitys
settings-check-updates = Tarkista päivitykset
settings-check-updates-hint = Tarkistetaan automaattisesti käynnistyksen yhteydessä ja tunnin välein, kun automaattiset päivitykset ovat käytössä.
settings-update-unavailable = Ei saatavilla
settings-update-unavailable-hint = Päivittäjä ei sisälly tähän koontiversioon.
settings-update-checking = Tarkistetaan…
settings-update-checking-hint = Tarkistetaan päivityksiä…
settings-update-check-again = Tarkista uudelleen
settings-update-current = Vmux on ajan tasalla.
settings-update-downloading = Ladataan…
settings-update-downloading-hint = Ladataan Vmux { $version }…
settings-update-installing = Asennetaan…
settings-update-installing-hint = Asennetaan Vmux { $version }…
settings-update-ready = Päivitys valmis
settings-update-ready-hint = Vmux { $version } on valmis. Ota päivitys käyttöön käynnistämällä uudelleen.
settings-update-try-again = Yritä uudelleen
settings-update-failed = Päivitysten tarkistus ei onnistu.
settings-item = Kohde
settings-item-number = Kohde { $number }
settings-press-key = Paina näppäintä…
settings-saved = Tallennettu
settings-record-key = Tallenna uusi näppäinyhdistelmä napsauttamalla

tray-open-window = Avaa ikkuna
tray-close-window = Sulje ikkuna
tray-pause-recording = Keskeytä tallennus
tray-resume-recording = Jatka tallennusta
tray-finish-recording = Lopeta tallennus
tray-quit = Lopeta Vmux

composer-attach-files = Liitä tiedostoja (/upload)
composer-remove-attachment = Poista liite

layout-back = Takaisin
layout-forward = Eteenpäin
layout-reload = Lataa uudelleen
layout-bookmark-page = Lisää tämä sivu kirjanmerkkeihin
layout-remove-bookmark = Poista kirjanmerkki
layout-pin-page = Kiinnitä tämä sivu
layout-unpin-page = Poista tämän sivun kiinnitys
layout-manage-extensions = Hallitse laajennuksia
layout-new-stack = Uusi pino
layout-close-tab = Sulje välilehti
layout-bookmark = Kirjanmerkki
layout-pin = Kiinnitä
layout-new-tab = Uusi välilehti
layout-team = Tiimi

command-switch-space = Vaihda työtilaa…
command-search-ask = Hae tai kysy…
command-new-tab-placeholder = Hae, kirjoita URL tai valitse pääte…
command-placeholder = Kirjoita URL, hae välilehtiä tai avaa komennot kirjoittamalla >…
command-composer-placeholder = Kirjoita / komennoille tai @ medialle
command-send = Lähetä (Enter)
command-terminal = Pääte
command-open-terminal = Avaa päätteessä
command-stack = Pino
command-tabs = { $count ->
    [one] 1 välilehti
   *[other] { $count } välilehteä
}
command-prompt = Kehote
command-new-tab = Uusi välilehti
command-search = Hae
command-open-value = Avaa “{ $value }”
command-search-value = Hae “{ $value }”

schema-appearance = Ulkoasu
schema-general = Yleiset
schema-layout = Asettelu
schema-layout-detail = Ikkuna, ruudut, sivupalkki ja kohdistuskehys.
schema-agent = Agentti
schema-agent-detail = Agentin toiminta ja työkalujen käyttöoikeudet.
schema-shortcuts = Pikanäppäimet
schema-shortcuts-detail = Vain luku -näkymä. Muuta näppäinsidontoja muokkaamalla settings.ron-tiedostoa suoraan.
schema-terminal = Pääte
schema-browser = Selain
schema-mode = Tila
schema-mode-detail = Verkkosivujen väriteema. Laite seuraa järjestelmän asetusta.
schema-device = Laite
schema-light = Vaalea
schema-dark = Tumma
schema-language = Kieli
schema-language-detail = Käytä järjestelmää, en-US, ja tai mitä tahansa BCP 47 -tunnistetta, jolle on vastaava ~/.vmux/locales/<tag>.ftl-luettelo.
schema-auto-update = Automaattiset päivitykset
schema-auto-update-detail = Tarkista ja asenna päivitykset käynnistyksen yhteydessä ja tunnin välein.
schema-startup-url = Käynnistyksen URL
schema-startup-url-detail = Tyhjä arvo avaa komentopalkin kehotteen.
schema-search-engine = Hakukone
schema-search-engine-detail = Käytetään verkkohakuihin aloitusnäkymästä ja komentopalkista.
schema-window = Ikkuna
schema-pane = Ruutu
schema-side-sheet = Sivupaneeli
schema-focus-ring = Kohdistuskehys
schema-run-placement = Salli suorituksen sijoittelun ohitus
schema-run-placement-detail = Anna agenttien valita suoritusruudun tila, suunta ja ankkuri.
schema-leader = Etunäppäin
schema-leader-detail = Sointupikanäppäinten etuliitenäppäin.
schema-chord-timeout = Soinnun aikakatkaisu
schema-chord-timeout-detail = Millisekunteina aika, jonka jälkeen soinnun etuliite vanhenee.
schema-bindings = Sidonnat
schema-confirm-close = Vahvista sulkeminen
schema-confirm-close-detail = Kysy ennen päätteen sulkemista, jos siinä on käynnissä prosessi.
schema-default-theme = Oletusteema
schema-default-theme-detail = Aktiivisen teeman nimi teemaluettelosta.

settings-empty = (tyhjä)
settings-none = (ei mitään)

schema-system = Järjestelmä
schema-editor = Editori
schema-recording = Tallennus
schema-radius = Säde
schema-padding = Täyte
schema-gap = Väli
schema-width = Leveys
schema-color = Väri
schema-red = Punainen
schema-green = Vihreä
schema-blue = Sininen
schema-follow-files = Seuraa tiedostoja
schema-tidy-files = Siisti tiedostot
schema-tidy-files-max = Tiedostojen siistimiskynnys
schema-tidy-files-auto = Siisti tiedostot automaattisesti
schema-app-providers = Sovellusten tarjoajat
schema-provider = Tarjoaja
schema-kind = Tyyppi
schema-models = Mallit
schema-acp = ACP-agentit
schema-id = ID
schema-name = Nimi
schema-command = Komento
schema-arguments = Argumentit
schema-environment = Ympäristömuuttujat
schema-working-directory = Työhakemisto
schema-shell = Komentotulkki
schema-font-family = Fonttiperhe
schema-startup-directory = Aloitushakemisto
schema-themes = Teemat
schema-color-scheme = Väriteema
schema-font-size = Fonttikoko
schema-line-height = Rivikorkeus
schema-cursor-style = Kohdistimen tyyli
schema-cursor-blink = Kohdistimen vilkkuminen
schema-custom-themes = Mukautetut teemat
schema-foreground = Edustaväri
schema-background = Tausta
schema-cursor = Kohdistin
schema-ansi-colors = ANSI-värit
schema-keymap = Näppäinkartta
schema-explorer = Resurssienhallinta
schema-visible = Näkyvissä
schema-language-servers = Kielipalvelimet
schema-servers = Palvelimet
schema-language-id = Kielen ID
schema-root-markers = Juurimerkit
schema-output-directory = Tulostehakemisto

menu-scene = Näkymä
menu-layout = Asettelu
menu-terminal = Pääte
menu-browser = Selain
menu-service = Palvelu
menu-bookmark = Kirjanmerkki
menu-edit = Muokkaa

layout-knowledge = Tietämys
layout-open-knowledge = Avaa tietämys
layout-open-welcome-knowledge = Avaa Tervetuloa tietämykseen
layout-open-path = Avaa { $path }
layout-fold-knowledge = Kutista tietämys
layout-unfold-knowledge = Laajenna tietämys
layout-bookmarks = Kirjanmerkit
layout-new-folder = Uusi kansio
layout-add-to-bookmarks = Lisää kirjanmerkkeihin
layout-move-to-bookmarks = Siirrä kirjanmerkkeihin
layout-stack-number = Pino { $number }
layout-fold-stack = Kutista pino
layout-unfold-stack = Laajenna pino
layout-close-stack = Sulje pino
layout-bookmark-in = Lisää kirjanmerkki kansioon { $folder }

common-cancel = Peruuta
common-delete = Poista
common-save = Tallenna
common-rename = Nimeä uudelleen
common-expand = Laajenna
common-collapse = Supista
common-loading = Ladataan…
common-error = Virhe
common-output = Tuloste
common-pending = Odottaa
common-current = nykyinen
common-stop = Pysäytä
services-command = Vmux-palvelu
services-uptime-seconds = { $seconds } s
services-uptime-minutes = { $minutes } min { $seconds } s
services-uptime-hours = { $hours } h { $minutes } min
services-uptime-days = { $days } pv { $hours } h

error-page-failed-load = Sivun lataus epäonnistui
error-page-not-found = Sivua ei löydy
error-unknown-host = Tuntematon Vmux-sovellusisäntä: { $host }

history-title = Historia

command-new-app-chat = Uusi { $provider }/{ $model } -keskustelu (sovellus)
command-interactive-mode-user = Scene > Vuorovaikutustila > Käyttäjä
command-interactive-mode-player = Scene > Vuorovaikutustila > Pelaaja
command-minimize-window = Layout > Ikkuna > Pienennä
command-toggle-layout = Layout > Asettelu > Vaihda asettelua
command-close-tab = Layout > Välilehti > Sulje välilehti
command-new-task = Layout > Välilehti > Uusi tehtävä…
command-next-tab = Layout > Välilehti > Seuraava välilehti
command-prev-tab = Layout > Välilehti > Edellinen välilehti
command-rename-tab = Layout > Välilehti > Nimeä välilehti uudelleen
command-tab-select-1 = Layout > Välilehti > Valitse välilehti 1
command-tab-select-2 = Layout > Välilehti > Valitse välilehti 2
command-tab-select-3 = Layout > Välilehti > Valitse välilehti 3
command-tab-select-4 = Layout > Välilehti > Valitse välilehti 4
command-tab-select-5 = Layout > Välilehti > Valitse välilehti 5
command-tab-select-6 = Layout > Välilehti > Valitse välilehti 6
command-tab-select-7 = Layout > Välilehti > Valitse välilehti 7
command-tab-select-8 = Layout > Välilehti > Valitse välilehti 8
command-tab-select-last = Layout > Välilehti > Valitse viimeinen välilehti
command-close-pane = Layout > Ruutu > Sulje ruutu
command-select-pane-left = Layout > Ruutu > Valitse vasen ruutu
command-select-pane-right = Layout > Ruutu > Valitse oikea ruutu
command-select-pane-up = Layout > Ruutu > Valitse ylempi ruutu
command-select-pane-down = Layout > Ruutu > Valitse alempi ruutu
command-swap-pane-prev = Layout > Ruutu > Vaihda edelliseen ruutuun
command-swap-pane-next = Layout > Ruutu > Vaihda seuraavaan ruutuun
command-equalize-pane-size = Layout > Ruutu > Tasaa ruutujen koot
command-resize-pane-left = Layout > Ruutu > Muuta ruudun kokoa vasemmalle
command-resize-pane-right = Layout > Ruutu > Muuta ruudun kokoa oikealle
command-resize-pane-up = Layout > Ruutu > Muuta ruudun kokoa ylös
command-resize-pane-down = Layout > Ruutu > Muuta ruudun kokoa alas
command-stack-close = Layout > Pino > Sulje pino
command-stack-next = Layout > Pino > Seuraava pino
command-stack-previous = Layout > Pino > Edellinen pino
command-stack-reopen = Layout > Pino > Avaa suljettu sivu uudelleen
command-stack-swap-prev = Layout > Pino > Siirrä pino vasemmalle
command-stack-swap-next = Layout > Pino > Siirrä pino oikealle
command-space-open = Layout > Tila > Tilat
command-terminal-close = Terminal > Sulje pääte
command-terminal-next = Terminal > Seuraava pääte
command-terminal-prev = Terminal > Edellinen pääte
command-terminal-clear = Terminal > Tyhjennä pääte
command-browser-prev-page = Browser > Navigointi > Takaisin
command-browser-next-page = Browser > Navigointi > Eteenpäin
command-browser-reload = Browser > Navigointi > Lataa uudelleen
command-browser-hard-reload = Browser > Navigointi > Pakotettu uudelleenlataus
command-open-in-place = Browser > Avaa > Avaa tähän
command-open-in-new-stack = Browser > Avaa > Avaa uudessa pinossa
command-open-in-pane-top = Browser > Avaa > Avaa yläpuoliseen ruutuun
command-open-in-pane-right = Browser > Avaa > Avaa oikeanpuoleiseen ruutuun
command-open-in-pane-bottom = Browser > Avaa > Avaa alapuoliseen ruutuun
command-open-in-pane-left = Browser > Avaa > Avaa vasemmanpuoleiseen ruutuun
command-open-in-new-tab = Browser > Avaa > Avaa uudessa välilehdessä
command-open-in-new-space = Browser > Avaa > Avaa uudessa tilassa
command-browser-zoom-in = Browser > Näytä > Lähennä
command-browser-zoom-out = Browser > Näytä > Loitonna
command-browser-zoom-reset = Browser > Näytä > Todellinen koko
command-browser-dev-tools = Browser > Näytä > Kehittäjätyökalut
command-browser-open-command-bar = Browser > Palkki > Komentopalkki
command-browser-open-page-in-command-bar = Browser > Palkki > Muokkaa sivua
command-browser-open-path-bar = Browser > Palkki > Polkunavigaattori
command-browser-open-commands = Browser > Palkki > Komennot
command-browser-open-history = Browser > Palkki > Historia
command-service-open = Service > Avaa palveluvalvonta
command-bookmark-toggle-active = Bookmark > Lisää sivu kirjanmerkkeihin
command-bookmark-pin-active = Bookmark > Kiinnitä sivu

layout-tab = Välilehti
layout-no-stacks = Ei pinoja
layout-loading = Ladataan…
layout-no-markdown-files = Ei Markdown-tiedostoja
layout-empty-folder = Tyhjä kansio
layout-worktree = työhakemisto
layout-folder-name = Kansion nimi
layout-no-pins-bookmarks = Ei kiinnityksiä tai kirjanmerkkejä
layout-move-to = Siirrä kansioon { $folder }
layout-bookmark-current-page = Lisää nykyinen sivu kirjanmerkkeihin
layout-rename-folder = Nimeä kansio uudelleen
layout-remove-folder = Poista kansio
layout-update-downloading = Ladataan päivitystä
layout-update-installing = Asennetaan päivitystä…
layout-update-ready = Uusi versio saatavilla
layout-restart-update = Käynnistä uudelleen päivittääksesi

agent-preparing = Valmistellaan agenttia…
agent-send-all-queued = Lähetä kaikki jonossa olevat kehotteet nyt (Esc)
agent-send = Lähetä (Enter)
agent-ready = Valmis, kun sinä olet.
agent-loading-older = Ladataan vanhempia viestejä…
agent-load-older = Lataa vanhemmat viestit
agent-continued-from = Jatkettu lähteestä { $source }
agent-older-context-omitted = vanhempi konteksti jätetty pois
agent-interrupted = keskeytetty
agent-allow-tool = Sallitaanko { $tool }?
agent-deny = Estä
agent-allow-always = Salli aina
agent-allow = Salli
agent-loading-sessions = Ladataan istuntoja…
agent-no-resumable-sessions = Jatkettavia istuntoja ei löytynyt
agent-no-matching-sessions = Ei vastaavia istuntoja
agent-no-matching-models = Ei vastaavia malleja
agent-choice-help = ↑/↓ tai Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Valitse repositoriokansio
agent-choose-repository-detail = Valitse paikallinen Git-repositorio, jota agentin tulee käyttää.
agent-choosing = Valitaan…
agent-choose-folder = Valitse kansio
agent-queued = jonossa
agent-attached = Liitetty:
agent-cancel-queued = Peruuta jonossa oleva kehote
agent-resume-queued = Jatka jonossa olevia kehotteita
agent-clear-queue = Tyhjennä jono
agent-send-all-now = lähetä kaikki nyt
agent-choose-option = Valitse vaihtoehto yllä
agent-loading-media = Ladataan mediaa…
agent-no-matching-media = Ei vastaavaa mediaa
agent-prompt-context = Kehotteen konteksti
agent-details = Tiedot
agent-path = Polku
agent-tool = Työkalu
agent-server = Palvelin
agent-bytes = { $count } tavua
agent-worked-for = Työskenteli { $duration }
agent-worked-for-steps = { $count ->
    [one] Työskenteli { $duration } · 1 vaihe
   *[other] Työskenteli { $duration } · { $count } vaihetta
}
agent-tool-guardian-review = Guardian-tarkistus
agent-tool-read-files = Luki tiedostoja
agent-tool-viewed-image = Katseli kuvaa
agent-tool-used-browser = Käytti selainta
agent-tool-searched-files = Haki tiedostoista
agent-tool-ran-commands = Suoritti komentoja
agent-thinking = Ajattelee
agent-subagent = Aliagentti
agent-prompt = Kehote
agent-thread = Säie
agent-parent = Ylätaso
agent-children = Alatasot
agent-call = Kutsu
agent-raw-event = Raakatapahtuma
agent-plan = Suunnitelma
agent-tasks = { $count ->
    [one] 1 tehtävä
   *[other] { $count } tehtävää
}
agent-edited = Muokattu
agent-reconnecting = Yhdistetään uudelleen { $attempt }/{ $total }
agent-status-running = Käynnissä
agent-status-done = Valmis
agent-status-failed = Epäonnistui
agent-status-pending = Odottaa
agent-slash-attach-files = Liitä tiedostoja
agent-slash-resume-session = Jatka aiempaa istuntoa
agent-slash-select-model = Valitse malli
agent-slash-continue-cli = Jatka tätä istuntoa CLI:ssä
agent-session-just-now = juuri nyt
agent-session-minutes-ago = { $count } min sitten
agent-session-hours-ago = { $count } h sitten
agent-session-days-ago = { $count } pv sitten
agent-working-working = Työskentelee
agent-working-thinking = Ajattelee
agent-working-pondering = Pohdiskelee
agent-working-noodling = Hahmottelee
agent-working-percolating = Kypsyttelee
agent-working-conjuring = Loihtii
agent-working-cooking = Kokkaa
agent-working-brewing = Hauduttelee
agent-working-musing = Mietiskelee
agent-working-ruminating = Märehtii
agent-working-scheming = Juonii
agent-working-synthesizing = Koostaa
agent-working-tinkering = Säätää
agent-working-churning = Työstää
agent-working-vibing = Fiilistelee
agent-working-simmering = Muhittelee
agent-working-crafting = Rakentaa
agent-working-divining = Tulkkailee
agent-working-mulling = Punnitsee
agent-working-spelunking = Sukeltaa syvälle

editor-toggle-explorer = Näytä/piilota Explorer (Cmd+B)
editor-unsaved = tallentamaton
editor-rendered-markdown = Renderöity Markdown reaaliaikaisella muokkauksella
editor-note = Muistiinpano
editor-source-editor = Lähdekoodieditori
editor-editor = Editori
editor-git-diff = Git-diffi
editor-diff = Diffi
editor-tidy = Siivoa
editor-always = Aina
editor-unchanged-previews = { $count ->
    [one] ✦ 1 muuttumaton esikatselu
   *[other] ✦ { $count } muuttumatonta esikatselua
}
editor-open-externally = Avaa ulkoisesti
editor-changed-line = Muutettu rivi
editor-go-to-definition = Siirry määritelmään
editor-find-references = Etsi viittaukset
editor-references = { $count ->
    [one] 1 viittaus
   *[other] { $count } viittausta
}
editor-lsp-starting = { $server } käynnistyy…
editor-lsp-not-installed = { $server } — ei asennettu
editor-explorer = Explorer
editor-open-editors = Avoimet editorit
editor-outline = Jäsennys
editor-new-file = Uusi tiedosto
editor-new-folder = Uusi kansio
editor-delete-confirm = Poistetaanko “{ $name }”? Tätä ei voi kumota.
editor-created-folder = Kansio { $name } luotu
editor-created-file = Tiedosto { $name } luotu
editor-renamed-to = Nimetty uudelleen: { $name }
editor-deleted = Poistettu { $name }
editor-failed-decode-image = Kuvan purku epäonnistui
editor-preview-large-image = kuva (liian suuri esikatseltavaksi)
editor-preview-binary = binääri
editor-preview-file = tiedosto

git-status-clean = puhdas
git-status-modified = muokattu
git-status-staged = valmisteltu
git-status-staged-modified = valmisteltu*
git-status-untracked = seuraamaton
git-status-deleted = poistettu
git-status-conflict = ristiriita
git-accept-all = ✓ hyväksy kaikki
git-unstage = Poista valmistelusta
git-confirm-deny-all = Vahvista kaikkien hylkäys
git-deny-all = ✗ hylkää kaikki
git-commit-message = commit-viesti
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Ladataan diffiä…
git-no-changes = Ei näytettäviä muutoksia
git-accept = ✓ hyväksy
git-deny = ✗ hylkää
git-show-unchanged-lines = Näytä { $count } muuttumatonta riviä

terminal-loading = Ladataan…
terminal-runs-when-ready = suoritetaan, kun valmis · Ctrl+C tyhjentää · Esc ohittaa
terminal-booting = käynnistyy
terminal-type-command = kirjoita komento · suoritetaan, kun valmis · Esc ohittaa

setup-tagline-claude = Anthropicin koodausagentti Vmuxissa
setup-tagline-codex = OpenAI:n koodausagentti Vmuxissa
setup-tagline-vibe = Mistralin koodausagentti Vmuxissa
setup-install-title = Asenna { $name } CLI
setup-homebrew-required = Homebrew tarvitaan { $command }-komennon asentamiseen, eikä sitä ole vielä määritetty. Vmux asentaa ensin Homebrew’n ja sitten { $name }.
setup-terminal-instructions = Aloita painamalla päätteessä Return ja anna sitten Macin salasana pyydettäessä.
setup-command-missing = Vmux avasi tämän sivun, koska paikallista { $command }-komentoa ei ole vielä asennettu. Hanki se suorittamalla alla oleva komento.
setup-install-failed = Asennus ei valmistunut. Tarkista lisätiedot päätteestä ja yritä uudelleen.
setup-installing = Asennetaan…
setup-install-homebrew = Asenna Homebrew + { $name }
setup-run-install = Suorita asennuskomento
setup-auto-reload = Vmux suorittaa sen päätteessä ja lataa uudelleen, kun { $command } on valmis.

debug-title = Vianmääritys
debug-auto-update = Automaattinen päivitys
debug-simulate-update = Simuloi saatavilla oleva päivitys
debug-simulate-download = Simuloi lataus
debug-clear-update = Tyhjennä päivitys
debug-trigger-restart = Käynnistä uudelleen

command-manage-spaces = Hallitse tiloja…
command-pane-stack-location = paneeli { $pane } / pino { $stack }
command-space-pane-stack-location = { $space } / paneeli { $pane } / pino { $stack }
command-terminal-path = Pääte ({ $path })
command-group-interactive-mode = Vuorovaikutteinen tila
command-group-window = Ikkuna
command-group-tab = Välilehti
command-group-pane = Paneeli
command-group-stack = Pino
command-group-space = Tila
command-group-navigation = Navigointi
command-group-open = Avaa
command-group-view = Näytä
command-group-bar = Palkki

menu-close-vmux = Sulje Vmux

agents-terminal-coding-agent = Päätteessä toimiva koodausagentti
