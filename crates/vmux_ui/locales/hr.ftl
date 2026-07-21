common-open = Otvori
common-close = Zatvori
common-install = Instaliraj
common-uninstall = Deinstaliraj
common-update = Ažuriraj
common-retry = Pokušaj ponovno
common-refresh = Osvježi
common-remove = Ukloni
common-enable = Omogući
common-disable = Onemogući
common-new = Novo
common-active = aktivno
common-running = pokrenuto
common-done = gotovo
common-failed = Nije uspjelo
common-installed = Instalirano
common-items = { $count ->
    [one] { $count } stavka
   *[other] { $count } stavki
}
start-title = Početak
start-tagline = Jedan prompt. Sve riješeno.

agents-title = Agenti
agents-search = Pretraži ACP i CLI agente…
agents-empty = Nema odgovarajućih agenata
agents-empty-detail = Pokušaj s nazivom, runtimeom ili ACP/CLI.
agents-install-failed = Instalacija nije uspjela
agents-updating = Ažuriranje…
agents-retrying = Ponovni pokušaj…
agents-preparing = Priprema…

extensions-title = Proširenja
extensions-search = Pretraži instalirana ili Chrome Web Store…
extensions-relaunch = Ponovno pokreni za primjenu
extensions-empty = Nema instaliranih proširenja
extensions-no-match = Nema odgovarajućih proširenja
extensions-empty-detail = Pretraži Chrome Web Store iznad i pritisni Enter.
extensions-no-match-detail = Pokušaj s drugim nazivom ili ID-jem proširenja.
extensions-on = Uključeno
extensions-off = Isključeno
extensions-enable-confirm = Omogućiti { $name }?
extensions-enable-permissions = Omogući { $name } i dopusti:

lsp-title = Jezični poslužitelji
lsp-search = Pretraži jezične poslužitelje, lintere, formattere…
lsp-loading = Učitavanje kataloga…
lsp-empty = Nema odgovarajućih jezičnih poslužitelja
lsp-empty-detail = Pokušaj s drugim jezikom, linterom ili formatterom.
lsp-needs = treba { $tool }
lsp-status-available = Dostupno
lsp-status-on-path = Na PATH
lsp-status-installing = Instaliranje…
lsp-status-installed = Instalirano
lsp-status-outdated = Dostupno ažuriranje
lsp-status-running = Pokrenuto
lsp-status-failed = Nije uspjelo

spaces-title = Prostori
spaces-new-placeholder = Naziv novog prostora
spaces-empty = Nema prostora
spaces-default-name = Prostor { $number }
spaces-tabs = { $count ->
    [one] 1 kartica
   *[other] { $count } kartica
}
spaces-delete = Izbriši prostor

team-title = Tim
team-just-you = Samo ti u ovom prostoru
team-agents = { $count ->
    [one] Ti i 1 agent
   *[other] Ti i { $count } agenata
}
team-empty = Ovdje još nema nikoga
team-you = Ti
team-agent = Agent

services-title = Pozadinske usluge
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procesa
}
services-kill-all = Prisili završetak svih
services-not-running = Usluga nije pokrenuta
services-start-with = Pokreni s:
services-empty = Nema aktivnih procesa
services-filter = Filtriraj procese…
services-no-match = Nema odgovarajućih procesa
services-connected = Povezano
services-disconnected = Prekinuto
services-attached = priključeno
services-kill = Prisili završetak
services-memory = Memorija
services-size = Veličina
services-shell = Ljuska

error-title = Pogreška

history-search = Pretraži povijest
history-clear-all = Očisti sve
history-clear-confirm = Očistiti cijelu povijest?
history-clear-warning = Ovo se ne može poništiti.
history-cancel = Odustani
history-today = Danas
history-yesterday = Jučer
history-days-ago = Prije { $count } dana
history-day-offset = Dan -{ $count }

settings-title = Postavke
settings-loading = Učitavanje postavki…
settings-stored = Spremljeno u ~/.vmux/settings.ron
settings-other = Ostalo
settings-software-update = Ažuriranje softvera
settings-check-updates = Provjeri ažuriranja
settings-check-updates-hint = Provjerava se automatski pri pokretanju i svaki sat kad je automatsko ažuriranje uključeno.
settings-update-unavailable = Nedostupno
settings-update-unavailable-hint = Ažuriranje nije uključeno u ovu verziju.
settings-update-checking = Provjera…
settings-update-checking-hint = Provjera ažuriranja…
settings-update-check-again = Provjeri ponovno
settings-update-current = Vmux je ažuran.
settings-update-downloading = Preuzimanje…
settings-update-downloading-hint = Preuzimanje Vmux { $version }…
settings-update-installing = Instaliranje…
settings-update-installing-hint = Instaliranje Vmux { $version }…
settings-update-ready = Ažuriranje je spremno
settings-update-ready-hint = Vmux { $version } je spreman. Ponovno pokreni za primjenu.
settings-update-try-again = Pokušaj ponovno
settings-update-failed = Nije moguće provjeriti ažuriranja.
settings-item = Stavka
settings-item-number = Stavka { $number }
settings-press-key = Pritisni tipku…
settings-saved = Spremljeno
settings-record-key = Klikni za snimanje nove kombinacije tipki

tray-open-window = Otvori prozor
tray-close-window = Zatvori prozor
tray-pause-recording = Pauziraj snimanje
tray-resume-recording = Nastavi snimanje
tray-finish-recording = Završi snimanje
tray-quit = Zatvori Vmux

composer-attach-files = Priloži datoteke (/upload)
composer-remove-attachment = Ukloni privitak

layout-back = Natrag
layout-forward = Naprijed
layout-reload = Ponovno učitaj
layout-bookmark-page = Dodaj ovu stranicu u oznake
layout-remove-bookmark = Ukloni oznaku
layout-pin-page = Prikvači ovu stranicu
layout-unpin-page = Otkvači ovu stranicu
layout-manage-extensions = Upravljaj proširenjima
layout-new-stack = Novi sloj
layout-close-tab = Zatvori karticu
layout-bookmark = Oznaka
layout-pin = Prikvači
layout-new-tab = Nova kartica
layout-team = Tim

command-switch-space = Promijeni prostor…
command-search-ask = Pretraži ili pitaj…
command-new-tab-placeholder = Pretraži ili upiši URL, ili odaberi Terminal…
command-placeholder = Upiši URL, pretraži kartice ili > za naredbe…
command-composer-placeholder = Upiši / za naredbe ili @ za medije
command-send = Pošalji (Enter)
command-terminal = Terminal
command-open-terminal = Otvori u Terminalu
command-stack = Sloj
command-tabs = { $count ->
    [one] 1 kartica
   *[other] { $count } kartica
}
command-prompt = Prompt
command-new-tab = Nova kartica
command-search = Pretraži
command-open-value = Otvori “{ $value }”
command-search-value = Pretraži “{ $value }”

schema-appearance = Izgled
schema-general = Općenito
schema-layout = Raspored
schema-layout-detail = Prozor, okna, bočna traka i obrub fokusa.
schema-agent = Agent
schema-agent-detail = Ponašanje agenata i dopuštenja za alate.
schema-shortcuts = Prečaci
schema-shortcuts-detail = Prikaz samo za čitanje. Za promjenu veza uredi settings.ron izravno.
schema-terminal = Terminal
schema-browser = Preglednik
schema-mode = Način
schema-mode-detail = Shema boja za web-stranice. Uređaj prati sustav.
schema-device = Uređaj
schema-light = Svijetlo
schema-dark = Tamno
schema-language = Jezik
schema-language-detail = Koristi sustav, en-US, ja ili bilo koju BCP 47 oznaku s odgovarajućim ~/.vmux/locales/<tag>.ftl katalogom.
schema-auto-update = Automatsko ažuriranje
schema-auto-update-detail = Provjeri i instaliraj ažuriranja pri pokretanju i svaki sat.
schema-startup-url = Početni URL
schema-startup-url-detail = Ako je prazno, otvara se prompt naredbene trake.
schema-search-engine = Tražilica
schema-search-engine-detail = Koristi se za web-pretraživanja iz Početka i naredbene trake.
schema-window = Prozor
schema-pane = Okno
schema-side-sheet = Bočni list
schema-focus-ring = Obrub fokusa
schema-run-placement = Dopusti nadjačavanje položaja izvođenja
schema-run-placement-detail = Dopusti agentima da odaberu način, smjer i sidro okna za izvođenje.
schema-leader = Leader
schema-leader-detail = Prefiksna tipka za chord prečace.
schema-chord-timeout = Istek chorda
schema-chord-timeout-detail = Milisekunde prije isteka chord prefiksa.
schema-bindings = Veze
schema-confirm-close = Potvrdi zatvaranje
schema-confirm-close-detail = Pitaj prije zatvaranja terminala s pokrenutim procesom.
schema-default-theme = Zadana tema
schema-default-theme-detail = Naziv aktivne teme s popisa tema.
