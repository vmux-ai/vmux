common-open = Otvoreno
common-close = Zatvori
common-install = Instalirajte
common-uninstall = Deinstaliraj
common-update = Ažuriranje
common-retry = Pokušaj ponovo
common-refresh = Osvježi
common-remove = Ukloniti
common-enable = Omogući
common-disable = Onemogući
common-new = Novo
common-active = aktivan
common-running = trčanje
common-done = učinjeno
common-failed = neuspješno
common-installed = instalirano
common-items = { $count ->
    [one] { $count } stavka
   *[other] { $count } stavke
}
start-title = Start
start-tagline = Jedan upit. Bilo što, gotovo.

agents-title = Agenti
agents-search = Pretraži ACP i CLI agente…
agents-empty = Nema odgovarajućih agenata
agents-empty-detail = Pokušajte s nazivom, vremenom izvođenja ili ACP/CLI.
agents-install-failed = Instalacija nije uspjela
agents-updating = Ažuriranje...
agents-retrying = Ponovni pokušaj...
agents-preparing = Priprema…

extensions-title = Ekstenzije
extensions-search = Pretraživanje instalirano ili Chrome Web Store…
extensions-relaunch = Ponovno pokrenite za primjenu
extensions-empty = Nema instaliranih proširenja
extensions-no-match = Nema odgovarajućih proširenja
extensions-empty-detail = Pretražite Chrome Web Store iznad i pritisnite Return.
extensions-no-match-detail = Pokušajte s drugim imenom ili ID-om proširenja.
extensions-on = Uključeno
extensions-off = Isključeno
extensions-enable-confirm = Omogućiti { $name }?
extensions-enable-permissions = Omogućite { $name } i dopustite:

lsp-title = Jezični poslužitelji
lsp-search = Pretražite jezične poslužitelje, lintere, formatere…
lsp-loading = Učitavanje kataloga…
lsp-empty = Nema odgovarajućih jezičnih poslužitelja
lsp-empty-detail = Pokušajte s drugim jezikom, linterom ili formatterom.
lsp-needs = treba { $tool }
lsp-status-available = na raspolaganju
lsp-status-on-path = Na PATH
lsp-status-installing = Instaliranje...
lsp-status-installed = instalirano
lsp-status-outdated = Ažuriranje dostupno
lsp-status-running = Trčanje
lsp-status-failed = neuspješno

spaces-title = Razmaci
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
    [one] Vi i 1 agent
   *[other] Vi i { $count } agenti
}
team-empty = Ovdje još nema nikoga
team-you = ti
team-agent = Agent

services-title = Pozadinske usluge
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procesa
}
services-kill-all = Ubiti sve
services-not-running = Usluga ne radi
services-start-with = Počnite s:
services-empty = Nema aktivnih procesa
services-filter = Filtrirajte procese...
services-no-match = Nema odgovarajućih procesa
services-connected = Povezan
services-disconnected = Isključeno
services-attached = u prilogu
services-kill = ubiti
services-memory = Memorija
services-size = Veličina
services-shell = školjka

error-title = Greška

history-search = Povijest pretraživanja
history-clear-all = Obriši sve
history-clear-confirm = Obrisati svu povijest?
history-clear-warning = Ovo se ne može poništiti.
history-cancel = Odustani
history-today = Danas
history-yesterday = jučer
history-days-ago = Prije { $count } dana
history-day-offset = Dan -{ $count }

settings-title = postavke
settings-loading = Učitavanje postavki…
settings-stored = Pohranjeno u ~/.vmux/settings.ron
settings-other = ostalo
settings-software-update = Ažuriranje softvera
settings-check-updates = Provjerite ima li ažuriranja
settings-check-updates-hint = Provjerava automatski pri pokretanju i svaki sat kada je omogućeno automatsko ažuriranje.
settings-update-unavailable = Nedostupan
settings-update-unavailable-hint = Ažuriranje nije uključeno u ovu verziju.
settings-update-checking = Provjera...
settings-update-checking-hint = Provjera ažuriranja...
settings-update-check-again = Provjerite ponovno
settings-update-current = Vmux je ažuriran.
settings-update-downloading = Preuzimanje...
settings-update-downloading-hint = Preuzimanje Vmux { $version }…
settings-update-installing = Instaliranje...
settings-update-installing-hint = Instaliranje Vmux { $version }…
settings-update-ready = Ažuriranje spremno
settings-update-ready-hint = Vmux { $version } je spreman. Ponovno pokrenite da biste ga primijenili.
settings-update-try-again = Pokušajte ponovno
settings-update-failed = Nije moguće provjeriti ima li ažuriranja.
settings-item = Stavka
settings-item-number = Stavka { $number }
settings-press-key = Pritisnite tipku...
settings-saved = Spremljeno
settings-record-key = Kliknite za snimanje nove kombinacije tipki

tray-open-window = Otvoreni prozor
tray-close-window = Zatvori prozor
tray-pause-recording = Pauziraj snimanje
tray-resume-recording = Nastavi snimanje
tray-finish-recording = Završi snimanje
tray-quit = Zatvori Vmux

composer-attach-files = Priloži datoteke (/upload)
composer-remove-attachment = Ukloni privitak

layout-back = natrag
layout-forward = Naprijed
layout-reload = Ponovno učitaj
layout-bookmark-page = Označite ovu stranicu
layout-remove-bookmark = Ukloni oznaku
layout-pin-page = Prikvači ovu stranicu
layout-unpin-page = Otkvači ovu stranicu
layout-manage-extensions = Upravljanje proširenjima
layout-new-stack = Novi stog
layout-close-tab = Zatvori karticu
layout-bookmark = Knjižna oznaka
layout-pin = Pin
layout-new-tab = Nova kartica
layout-team = Tim

command-switch-space = Promijeni prostor…
command-search-ask = Traži ili pitaj…
command-new-tab-placeholder = Pretražite ili upišite URL ili odaberite Terminal…
command-placeholder = Upišite URL, pretražite kartice ili > za naredbe…
command-composer-placeholder = Upišite / za naredbe ili @ za medije
command-send = Pošalji (Enter)
command-terminal = Terminal
command-open-terminal = Otvorite u terminalu
command-stack = Stog
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } kartica
}
command-prompt = Brz
command-new-tab = Nova kartica
command-search = Traži
command-open-value = Otvori “{ $value }”
command-search-value = Traži “{ $value }”

schema-appearance = Izgled
schema-general = generalno
schema-layout = Izgled
schema-layout-detail = Prozor, okna, bočna traka i prsten za fokus.
schema-agent = Agent
schema-agent-detail = Ponašanje agenta i dopuštenja alata.
schema-shortcuts = Prečaci
schema-shortcuts-detail = Prikaz samo za čitanje. Uredite settings.ron izravno za promjenu vezanja.
schema-terminal = Terminal
schema-browser = preglednik
schema-mode = Način rada
schema-mode-detail = Shema boja za web stranice. Uređaj prati vaš sustav.
schema-device = Uređaj
schema-light = svjetlo
schema-dark = tamno
schema-language = Jezik
schema-language-detail = Koristite sustav, en-US, ja ili bilo koju BCP 47 oznaku s odgovarajućim ~/.vmux/locales/<tag>.ftl katalogom.
schema-auto-update = Automatsko ažuriranje
schema-auto-update-detail = Provjerite i instalirajte ažuriranja pri pokretanju i svaki sat.
schema-startup-url = Pokretanje URL
schema-startup-url-detail = Empty otvara prompt naredbene trake.
schema-search-engine = tražilica
schema-search-engine-detail = Koristi se za web pretraživanja iz Starta i naredbene trake.
schema-window = Prozor
schema-pane = Okno
schema-side-sheet = Bočni list
schema-focus-ring = Prsten za fokus
schema-run-placement = Dopusti nadjačavanje položaja izvođenja
schema-run-placement-detail = Dopustite agentima da odaberu način rada, smjer i sidro.
schema-leader = Vođa
schema-leader-detail = Tipka prefiksa za prečace akorda.
schema-chord-timeout = Istek akorda
schema-chord-timeout-detail = Milisekunde prije isteka prefiksa akorda.
schema-bindings = Vezovi
schema-confirm-close = Potvrdite zatvaranje
schema-confirm-close-detail = Pitaj prije zatvaranja terminala s pokrenutim procesom.
schema-default-theme = Zadana tema
schema-default-theme-detail = Naziv aktivne teme s popisa tema.
