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
common-done = tehty
common-failed = Epäonnistui
common-installed = Asennettu
common-items = { $count ->
    [one] { $count } kohde
   *[other] { $count } kohdetta
}
start-title = Aloita
start-tagline = Yksi kehotus. Mitä tahansa, tehty.

agents-title = Agentit
agents-search = Hae ACP ja CLI agentteja…
agents-empty = Ei vastaavia agentteja
agents-empty-detail = Kokeile nimeä, suoritusaikaa tai ACP/CLI.
agents-install-failed = Asennus epäonnistui
agents-updating = Päivitetään…
agents-retrying = Yritetään uudelleen…
agents-preparing = Valmistellaan…

extensions-title = Laajennukset
extensions-search = Hae asennettuna tai Chrome Web Store…
extensions-relaunch = Käynnistä uudelleen hakeaksesi
extensions-empty = Laajennuksia ei ole asennettu
extensions-no-match = Ei vastaavia laajennuksia
extensions-empty-detail = Hae yllä olevasta Chrome Web Store ja paina Return.
extensions-no-match-detail = Kokeile toista nimeä tai laajennuksen tunnusta.
extensions-on = Päällä
extensions-off = Pois päältä
extensions-enable-confirm = Otetaanko { $name } käyttöön?
extensions-enable-permissions = Ota { $name } käyttöön ja salli:

lsp-title = Kielipalvelimet
lsp-search = Hae kielipalvelimia, linterejä, muotoiluja…
lsp-loading = Ladataan luetteloa…
lsp-empty = Ei vastaavia kielipalvelimia
lsp-empty-detail = Kokeile toista kieltä, linteriä tai muotoilulaitetta.
lsp-needs = tarvitsee { $tool }
lsp-status-available = Saatavilla
lsp-status-on-path = PATH
lsp-status-installing = Asennetaan…
lsp-status-installed = Asennettu
lsp-status-outdated = Päivitys saatavilla
lsp-status-running = Juoksemassa
lsp-status-failed = Epäonnistui

spaces-title = Spaces
spaces-new-placeholder = Uusi tilan nimi
spaces-empty = Ei välilyöntejä
spaces-default-name = Space { $number }
spaces-tabs = { $count ->
    [one] 1 välilehti
   *[other] { $count } välilehtiä
}
spaces-delete = Poista välilyönti

team-title = Joukkue
team-just-you = Vain sinä tässä tilassa
team-agents = { $count ->
    [one] Sinä ja 1 agentti
   *[other] Sinä ja { $count } agentit
}
team-empty = Ei täällä vielä ketään
team-you = sinä
team-agent = Agentti

services-title = Taustapalvelut
services-processes = { $count ->
    [one] 1 prosessi
   *[other] { $count } prosesseja
}
services-kill-all = Tapa kaikki
services-not-running = Palvelu ei ole käynnissä
services-start-with = Aloita:
services-empty = Ei aktiivisia prosesseja
services-filter = Suodata prosessit…
services-no-match = Ei vastaavia prosesseja
services-connected = Yhdistetty
services-disconnected = Yhteys katkaistu
services-attached = liitteenä
services-kill = Tapa
services-memory = Muisti
services-size = Koko
services-shell = Shell

error-title = Virhe

history-search = Hakuhistoria
history-clear-all = Tyhjennä kaikki
history-clear-confirm = Poistetaanko koko historia?
history-clear-warning = Tätä ei voi kumota.
history-cancel = Peruuta
history-today = Tänään
history-yesterday = eilen
history-days-ago = { $count } päivää sitten
history-day-offset = Päivä -{ $count }

settings-title = Asetukset
settings-loading = Ladataan asetuksia…
settings-stored = Tallennettu paikkaan ~/.vmux/settings.ron
settings-other = Muut
settings-software-update = Ohjelmistopäivitys
settings-check-updates = Tarkista päivitykset
settings-check-updates-hint = Tarkistaa automaattisesti käynnistyksen yhteydessä ja tunnin välein, kun automaattinen päivitys on käytössä.
settings-update-unavailable = Ei saatavilla
settings-update-unavailable-hint = Päivitys ei sisälly tähän koontiversioon.
settings-update-checking = Tarkistetaan…
settings-update-checking-hint = Tarkistetaan päivityksiä…
settings-update-check-again = Tarkista uudelleen
settings-update-current = Vmux on ajan tasalla.
settings-update-downloading = Ladataan…
settings-update-downloading-hint = Ladataan Vmux { $version }…
settings-update-installing = Asennetaan…
settings-update-installing-hint = Asennetaan Vmux { $version }…
settings-update-ready = Päivitys valmis
settings-update-ready-hint = Vmux { $version } on valmis. Käynnistä se uudelleen.
settings-update-try-again = Yritä uudelleen
settings-update-failed = Päivityksiä ei voi tarkistaa.
settings-item = Tuote
settings-item-number = Kohde { $number }
settings-press-key = Paina näppäintä…
settings-saved = Tallennettu
settings-record-key = Napsauta tallentaaksesi uuden näppäinyhdistelmän

tray-open-window = Avaa ikkuna
tray-close-window = Sulje ikkuna
tray-pause-recording = Keskeytä tallennus
tray-resume-recording = Jatka tallennusta
tray-finish-recording = Lopeta tallennus
tray-quit = Lopeta Vmux

composer-attach-files = Liitä tiedostot (/upload)
composer-remove-attachment = Poista liite

layout-back = Takaisin
layout-forward = Eteenpäin
layout-reload = Lataa uudelleen
layout-bookmark-page = Merkitse tämä sivu kirjanmerkkeihin
layout-remove-bookmark = Poista kirjanmerkki
layout-pin-page = Kiinnitä tämä sivu
layout-unpin-page = Irrota tämä sivu
layout-manage-extensions = Hallinnoi laajennuksia
layout-new-stack = Uusi pino
layout-close-tab = Sulje välilehti
layout-bookmark = Kirjanmerkki
layout-pin = Pin
layout-new-tab = Uusi välilehti
layout-team = Joukkue

command-switch-space = Vaihda tilaa…
command-search-ask = Hae tai kysy…
command-new-tab-placeholder = Hae tai kirjoita URL tai valitse Pääte…
command-placeholder = Kirjoita URL, hae välilehtiä tai > komentoja varten…
command-composer-placeholder = Kirjoita / komentoja varten tai @ mediaa varten
command-send = Lähetä (Enter)
command-terminal = Terminaali
command-open-terminal = Avaa terminaalissa
command-stack = Pinoa
command-tabs = { $count ->
    [one] 1 välilehti
   *[other] { $count } välilehtiä
}
command-prompt = Kehote
command-new-tab = Uusi välilehti
command-search = Etsi
command-open-value = Avaa "{ $value }"
command-search-value = Hae "{ $value }"

schema-appearance = Ulkonäkö
schema-general = Kenraali
schema-layout = Asettelu
schema-layout-detail = Ikkuna, ruudut, sivupalkki ja tarkennusrengas.
schema-agent = Agentti
schema-agent-detail = Edustajan käyttäytyminen ja työkalujen käyttöoikeudet.
schema-shortcuts = Pikanäppäimet
schema-shortcuts-detail = Vain luku -näkymä. Muokkaa settings.ron suoraan muuttaaksesi sidoksia.
schema-terminal = Terminaali
schema-browser = Selain
schema-mode = tila
schema-mode-detail = Verkkosivujen värimaailma. Laite seuraa järjestelmääsi.
schema-device = Laite
schema-light = Kevyt
schema-dark = Tumma
schema-language = Kieli
schema-language-detail = Käytä järjestelmä-, en-US-, ja-tunnistetta tai mitä tahansa BCP 47-tunnistetta vastaavan ~/.vmux/locales/<tag>.ftl-luettelon kanssa.
schema-auto-update = Automaattinen päivitys
schema-auto-update-detail = Tarkista ja asenna päivitykset käynnistyksen yhteydessä ja tunnin välein.
schema-startup-url = Käynnistys URL
schema-startup-url-detail = Tyhjä avaa komentorivin kehotteen.
schema-search-engine = Hakukone
schema-search-engine-detail = Käytetään verkkohakuihin Käynnistä-kohdasta ja komentopalkista.
schema-window = Ikkuna
schema-pane = Pane
schema-side-sheet = Sivulevy
schema-focus-ring = Tarkennusrengas
schema-run-placement = Salli suorituksen sijoittelun ohitus
schema-run-placement-detail = Anna agenttien valita suoritusruudun tila, suunta ja ankkuri.
schema-leader = Johtaja
schema-leader-detail = Etuliitenäppäin sointujen pikanäppäimiä varten.
schema-chord-timeout = Sointu aikakatkaisu
schema-chord-timeout-detail = Millisekuntia ennen kuin sointuetuliite vanhenee.
schema-bindings = Sidokset
schema-confirm-close = Vahvista sulkeminen
schema-confirm-close-detail = Kysy ennen päätteen sulkemista käynnissä olevan prosessin kanssa.
schema-default-theme = Oletusteema
schema-default-theme-detail = Aktiivisen teeman nimi teemaluettelosta.
