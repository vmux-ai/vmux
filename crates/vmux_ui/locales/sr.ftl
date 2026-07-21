common-open = Otvori
common-close = Zatvori
common-install = Instaliraj
common-uninstall = Deinstaliraj
common-update = Ažuriraj
common-retry = Pokušaj ponovo
common-refresh = Osveži
common-remove = Ukloni
common-enable = Omogući
common-disable = Onemogući
common-new = Novo
common-active = aktivan
common-running = u toku
common-done = završeno
common-failed = Neuspelo
common-installed = Instalirano
common-items = { $count ->
    [one] { $count } stavka
   *[other] { $count } stavki
}
start-title = Početak
start-tagline = Jedan upit. Bilo šta, gotovo.

agents-title = Agenti
agents-search = Pretraži ACP i CLI agente…
agents-empty = Nema odgovarajućih agenata
agents-empty-detail = Pokušajte ime, okruženje ili ACP/CLI.
agents-install-failed = Instalacija neuspela
agents-updating = Ažuriranje…
agents-retrying = Ponovni pokušaj…
agents-preparing = Priprema…

extensions-title = Ekstenzije
extensions-search = Pretraži instalirane ili Chrome Web Store…
extensions-relaunch = Ponovo pokrenite da primenite
extensions-empty = Nema instaliranih ekstenzija
extensions-no-match = Nema odgovarajućih ekstenzija
extensions-empty-detail = Pretražite Chrome Web Store gore i pritisnite Return.
extensions-no-match-detail = Pokušajte drugo ime ili ID ekstenzije.
extensions-on = Uključeno
extensions-off = Isključeno
extensions-enable-confirm = Omogućiti { $name }?
extensions-enable-permissions = Omogući { $name } i dozvoli:

lsp-title = Jezički serveri
lsp-search = Pretraži jezičke servere, lintere, formatere…
lsp-loading = Učitavanje kataloga…
lsp-empty = Nema odgovarajućih jezičkih servera
lsp-empty-detail = Pokušajte drugi jezik, linter ili formater.
lsp-needs = zahteva { $tool }
lsp-status-available = Dostupan
lsp-status-on-path = Na PATH
lsp-status-installing = Instaliranje…
lsp-status-installed = Instalirano
lsp-status-outdated = Dostupno ažuriranje
lsp-status-running = Aktivan
lsp-status-failed = Neuspelo

spaces-title = Prostori
spaces-new-placeholder = Naziv novog prostora
spaces-empty = Nema prostora
spaces-default-name = Prostor { $number }
spaces-tabs = { $count ->
    [one] 1 kartica
   *[other] { $count } kartica
}
spaces-delete = Obriši prostor

team-title = Tim
team-just-you = Samo vi u ovom prostoru
team-agents = { $count ->
    [one] Vi i 1 agent
   *[other] Vi i { $count } agenata
}
team-empty = Još niko ovde
team-you = Vi
team-agent = Agent

services-title = Pozadinske usluge
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procesa
}
services-kill-all = Zaustavi sve
services-not-running = Usluga ne radi
services-start-with = Pokreni sa:
services-empty = Nema aktivnih procesa
services-filter = Filtriraj procese…
services-no-match = Nema odgovarajućih procesa
services-connected = Povezano
services-disconnected = Prekinuto
services-attached = priključeno
services-kill = Zaustavi
services-memory = Memorija
services-size = Veličina
services-shell = Shell

error-title = Greška

history-search = Pretraži istoriju
history-clear-all = Obriši sve
history-clear-confirm = Obrisati svu istoriju?
history-clear-warning = Ovo se ne može poništiti.
history-cancel = Otkaži
history-today = Danas
history-yesterday = Juče
history-days-ago = Pre { $count } dana
history-day-offset = Dan -{ $count }

settings-title = Podešavanja
settings-loading = Učitavanje podešavanja…
settings-stored = Sačuvano u ~/.vmux/settings.ron
settings-other = Ostalo
settings-software-update = Ažuriranje softvera
settings-check-updates = Proveri ažuriranja
settings-check-updates-hint = Automatski proverava pri pokretanju i svakog sata kada je automatsko ažuriranje omogućeno.
settings-update-unavailable = Nedostupno
settings-update-unavailable-hint = Alat za ažuriranje nije uključen u ovaj build.
settings-update-checking = Proveravanje…
settings-update-checking-hint = Proveravanje ažuriranja…
settings-update-check-again = Proveri ponovo
settings-update-current = Vmux je ažuran.
settings-update-downloading = Preuzimanje…
settings-update-downloading-hint = Preuzimanje Vmux { $version }…
settings-update-installing = Instaliranje…
settings-update-installing-hint = Instaliranje Vmux { $version }…
settings-update-ready = Ažuriranje spremno
settings-update-ready-hint = Vmux { $version } je spreman. Ponovo pokrenite da primenite.
settings-update-try-again = Pokušaj ponovo
settings-update-failed = Nije moguće proveriti ažuriranja.
settings-item = Stavka
settings-item-number = Stavka { $number }
settings-press-key = Pritisnite taster…
settings-saved = Sačuvano
settings-record-key = Kliknite da snimite novu kombinaciju tastera

tray-open-window = Otvori prozor
tray-close-window = Zatvori prozor
tray-pause-recording = Pauziraj snimanje
tray-resume-recording = Nastavi snimanje
tray-finish-recording = Završi snimanje
tray-quit = Zatvori Vmux

composer-attach-files = Priloži datoteke (/upload)
composer-remove-attachment = Ukloni prilog

layout-back = Nazad
layout-forward = Napred
layout-reload = Ponovo učitaj
layout-bookmark-page = Dodaj u oznake
layout-remove-bookmark = Ukloni oznaku
layout-pin-page = Prikvači stranicu
layout-unpin-page = Otkvači stranicu
layout-manage-extensions = Upravljaj ekstenzijama
layout-new-stack = Novi stek
layout-close-tab = Zatvori karticu
layout-bookmark = Oznaka
layout-pin = Prikvači
layout-new-tab = Nova kartica
layout-team = Tim

command-switch-space = Promeni prostor…
command-search-ask = Pretraži ili pitaj…
command-new-tab-placeholder = Pretražite ili unesite URL, ili izaberite Terminal…
command-placeholder = Unesite URL, pretražite kartice ili > za komande…
command-composer-placeholder = Ukucajte / za komande ili @ za medije
command-send = Pošalji (Enter)
command-terminal = Terminal
command-open-terminal = Otvori u terminalu
command-stack = Stek
command-tabs = { $count ->
    [one] 1 kartica
   *[other] { $count } kartica
}
command-prompt = Prompt
command-new-tab = Nova kartica
command-search = Pretraži
command-open-value = Otvori „{ $value }"
command-search-value = Pretraži „{ $value }"

schema-appearance = Izgled
schema-general = Opšte
schema-layout = Raspored
schema-layout-detail = Prozor, paneli, bočna traka i okvir fokusa.
schema-agent = Agent
schema-agent-detail = Ponašanje agenta i dozvole alata.
schema-shortcuts = Prečice
schema-shortcuts-detail = Prikaz samo za čitanje. Uredite settings.ron direktno za promenu vezivanja.
schema-terminal = Terminal
schema-browser = Pretraživač
schema-mode = Režim
schema-mode-detail = Šema boja za veb stranice. Uređaj prati vaš sistem.
schema-device = Uređaj
schema-light = Svetlo
schema-dark = Tamno
schema-language = Jezik
schema-language-detail = Koristite sistem, en-US, ja, ili bilo koji BCP 47 tag sa odgovarajućim ~/.vmux/locales/<tag>.ftl katalogom.
schema-auto-update = Automatsko ažuriranje
schema-auto-update-detail = Proverava i instalira ažuriranja pri pokretanju i svakog sata.
schema-startup-url = URL pri pokretanju
schema-startup-url-detail = Prazno otvara komandnu traku.
schema-search-engine = Motor za pretragu
schema-search-engine-detail = Koristi se za veb pretrage sa Početne stranice i komandne trake.
schema-window = Prozor
schema-pane = Panel
schema-side-sheet = Bočni panel
schema-focus-ring = Okvir fokusa
schema-run-placement = Dozvoli preusmeravanje pozicije pokretanja
schema-run-placement-detail = Dozvolite agentima da biraju režim, smer i sidro pokretanja.
schema-leader = Leader
schema-leader-detail = Prefiks taster za akordne prečice.
schema-chord-timeout = Timeout akorda
schema-chord-timeout-detail = Milisekunde pre isteka prefiksa akorda.
schema-bindings = Vezivanja
schema-confirm-close = Potvrdi zatvaranje
schema-confirm-close-detail = Traži potvrdu pre zatvaranja terminala sa aktivnim procesom.
schema-default-theme = Podrazumevana tema
schema-default-theme-detail = Naziv aktivne teme sa liste tema.
