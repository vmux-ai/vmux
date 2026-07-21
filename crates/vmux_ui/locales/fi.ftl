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
