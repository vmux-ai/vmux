common-open = Otvori
common-close = Zatvori
common-install = Instaliraj
common-uninstall = Deinstaliraj
common-update = Ažuriraj
common-retry = Pokušajte ponovo
common-refresh = Osvježi
common-remove = Ukloni
common-enable = Omogući
common-disable = Onemogući
common-new = Novo
common-active = aktivan
common-running = trčanje
common-done = urađeno
common-failed = Nije uspjelo
common-installed = Instalirano
common-items = { $count ->
    [one] { $count } stavka
   *[other] { $count } stavki
}
start-title = Počni
start-tagline = Jedan upit. Bilo šta, gotovo.

agents-title = Agenti
agents-search = Pretraži ACP i CLI agente…
agents-empty = Nema odgovarajućih agenata
agents-empty-detail = Pokušajte s imenom, vremenom izvođenja ili ACP/CLI.
agents-install-failed = Instalacija nije uspjela
agents-updating = Ažuriranje…
agents-retrying = Ponovni pokušaj…
agents-preparing = Priprema…

extensions-title = Ekstenzije
extensions-search = Traži instaliran ili Chrome Web Store…
extensions-relaunch = Ponovo pokrenite da biste se prijavili
extensions-empty = Nema instaliranih ekstenzija
extensions-no-match = Nema odgovarajućih ekstenzija
extensions-empty-detail = Pretražite Chrome Web Store iznad i pritisnite Return.
extensions-no-match-detail = Pokušajte s drugim imenom ili ID-om ekstenzije.
extensions-on = On
extensions-off = Isključeno
extensions-enable-confirm = Omogućiti { $name }?
extensions-enable-permissions = Omogućite { $name } i dozvolite:

lsp-title = Jezički serveri
lsp-search = Pretraži jezičke servere, lintere, formatere…
lsp-loading = Učitavanje kataloga…
lsp-empty = Nema odgovarajućih jezičkih servera
lsp-empty-detail = Pokušajte s drugim jezikom, linterom ili formaterom.
lsp-needs = treba { $tool }
lsp-status-available = Dostupan
lsp-status-on-path = Na PATH
lsp-status-installing = Instaliranje…
lsp-status-installed = Instalirano
lsp-status-outdated = Dostupno je ažuriranje
lsp-status-running = Trčanje
lsp-status-failed = Nije uspjelo

spaces-title = Prostori
spaces-new-placeholder = Novo ime prostora
spaces-empty = Nema razmaka
spaces-default-name = Prostor { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } kartica
}
spaces-delete = Izbriši prostor

team-title = Tim
team-just-you = Samo ti u ovom prostoru
team-agents = { $count ->
    [one] Ti i 1 agent
   *[other] Vi i { $count } agenti
}
team-empty = Još nikog ovde
team-you = Vi
team-agent = Agent

services-title = Pozadinske usluge
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procesa
}
services-kill-all = Kill All
services-not-running = Usluga ne radi
services-start-with = Počnite sa:
services-empty = Nema aktivnih procesa
services-filter = Filtrirajte procese…
services-no-match = Nema odgovarajućih procesa
services-connected = Povezano
services-disconnected = Disconnected
services-attached = u prilogu
services-kill = Ubij
services-memory = Memorija
services-size = Veličina
services-shell = Shell

error-title = Greška

history-search = Historija pretraživanja
history-clear-all = Obriši sve
history-clear-confirm = Obrisati svu historiju?
history-clear-warning = Ovo se ne može poništiti.
history-cancel = Otkaži
history-today = Danas
history-yesterday = Jučer
history-days-ago = prije { $count } dana
history-day-offset = Dan -{ $count }

settings-title = Postavke
settings-loading = Učitavanje postavki…
settings-stored = Pohranjeno u ~/.vmux/settings.ron
settings-other = Ostalo
settings-software-update = Ažuriranje softvera
settings-check-updates = Provjerite ima li ažuriranja
settings-check-updates-hint = Provjerava automatski pri pokretanju i svakih sat vremena kada je omogućeno automatsko ažuriranje.
settings-update-unavailable = Nedostupno
settings-update-unavailable-hint = Program za ažuriranje nije uključen u ovu verziju.
settings-update-checking = Provjera…
settings-update-checking-hint = Provjera ažuriranja…
settings-update-check-again = Provjerite ponovo
settings-update-current = Vmux je ažuriran.
settings-update-downloading = Preuzimanje…
settings-update-downloading-hint = Preuzimanje Vmux { $version }…
settings-update-installing = Instaliranje…
settings-update-installing-hint = Instaliranje Vmux { $version }…
settings-update-ready = Spreman za ažuriranje
settings-update-ready-hint = Vmux { $version } je spreman. Ponovo pokrenite da ga primijenite.
settings-update-try-again = Pokušajte ponovo
settings-update-failed = Nije moguće provjeriti ažuriranja.
settings-item = Stavka
settings-item-number = Stavka { $number }
settings-press-key = Pritisnite tipku…
settings-saved = Sačuvano
settings-record-key = Kliknite da snimite novu kombinaciju tipki

tray-open-window = Otvorite prozor
tray-close-window = Zatvori prozor
tray-pause-recording = Pauzirajte snimanje
tray-resume-recording = Nastavi snimanje
tray-finish-recording = Završi snimanje
tray-quit = Napusti Vmux

composer-attach-files = Priložite fajlove (/upload)
composer-remove-attachment = Uklonite prilog

layout-back = Nazad
layout-forward = Naprijed
layout-reload = Ponovo učitaj
layout-bookmark-page = Označite ovu stranicu
layout-remove-bookmark = Ukloni oznaku
layout-pin-page = Zakačite ovu stranicu
layout-unpin-page = Otkačite ovu stranicu
layout-manage-extensions = Upravljajte ekstenzijama
layout-new-stack = New Stack
layout-close-tab = Zatvori karticu
layout-bookmark = Bookmark
layout-pin = Pin
layout-new-tab = Nova kartica
layout-team = Tim

command-switch-space = Promijeni prostor…
command-search-ask = Potražite ili pitajte…
command-new-tab-placeholder = Pretražite ili upišite URL, ili odaberite Terminal…
command-placeholder = Upišite URL, tabove za pretragu ili > za komande…
command-composer-placeholder = Unesite / za komande ili @ za medije
command-send = Pošalji (Enter)
command-terminal = Terminal
command-open-terminal = Otvorite u terminalu
command-stack = Stack
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } kartica
}
command-prompt = Prompt
command-new-tab = Nova kartica
command-search = Traži
command-open-value = Otvori “{ $value }”
command-search-value = Pretraži “{ $value }”

schema-appearance = Izgled
schema-general = Generale
schema-layout = Layout
schema-layout-detail = Prozor, okna, bočna traka i prsten za fokusiranje.
schema-agent = Agent
schema-agent-detail = Ponašanje agenta i dozvole alata.
schema-shortcuts = Prečice
schema-shortcuts-detail = Prikaz samo za čitanje. Uredite settings.ron direktno da promijenite veze.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Mode
schema-mode-detail = Šema boja za web stranice. Uređaj prati vaš sistem.
schema-device = Uređaj
schema-light = Light
schema-dark = Dark
schema-language = Jezik
schema-language-detail = Koristite sistemsku, en-US, ja, ili bilo koju BCP 47 oznaku sa odgovarajućim ~/.vmux/locales/<tag>.ftl katalogom.
schema-auto-update = Automatsko ažuriranje
schema-auto-update-detail = Provjerite i instalirajte ažuriranja pri pokretanju i svaki sat.
schema-startup-url = Pokretanje URL
schema-startup-url-detail = Empty otvara prompt komandne trake.
schema-search-engine = Tražilica
schema-search-engine-detail = Koristi se za web pretraživanja sa Starta i komandne trake.
schema-window = Prozor
schema-pane = Okno
schema-side-sheet = Bočni list
schema-focus-ring = Prsten za fokus
schema-run-placement = Dozvoli nadjačavanje položaja pokretanja
schema-run-placement-detail = Neka agenti odaberu način rada, smjer i sidro.
schema-leader = Vođa
schema-leader-detail = Prefiks taster za prečice akorda.
schema-chord-timeout = Istek akorda
schema-chord-timeout-detail = Milisekunde prije isteka prefiksa akorda.
schema-bindings = Vezi
schema-confirm-close = Potvrdite zatvaranje
schema-confirm-close-detail = Pitajte prije zatvaranja terminala s pokrenutim procesom.
schema-default-theme = Zadana tema
schema-default-theme-detail = Naziv aktivne teme sa liste tema.
