common-open = Otvori
common-close = Zatvori
common-install = Instaliraj
common-uninstall = Deinstaliraj
common-update = Ažuriraj
common-retry = Pokušaj ponovo
common-refresh = Osvježi
common-remove = Ukloni
common-enable = Omogući
common-disable = Onemogući
common-new = Novo
common-active = aktivno
common-running = pokrenuto
common-done = gotovo
common-failed = Neuspjelo
common-installed = Instalirano
common-items = { $count ->
    [one] { $count } stavka
   *[other] { $count } stavki
}
start-title = Start
start-tagline = Jedna uputa. Sve završeno.

agents-title = Agenti
agents-search = Pretraži ACP i CLI agente…
agents-empty = Nema odgovarajućih agenata
agents-empty-detail = Pokušaj s nazivom, okruženjem ili ACP/CLI.
agents-install-failed = Instalacija nije uspjela
agents-updating = Ažuriranje…
agents-retrying = Ponovni pokušaj…
agents-preparing = Priprema…

extensions-title = Proširenja
extensions-search = Pretraži instalirano ili Chrome Web Store…
extensions-relaunch = Ponovo pokreni za primjenu
extensions-empty = Nema instaliranih proširenja
extensions-no-match = Nema odgovarajućih proširenja
extensions-empty-detail = Pretraži Chrome Web Store iznad i pritisni Enter.
extensions-no-match-detail = Pokušaj s drugim nazivom ili ID-jem proširenja.
extensions-on = Uključeno
extensions-off = Isključeno
extensions-enable-confirm = Omogućiti { $name }?
extensions-enable-permissions = Omogući { $name } i dozvoli:

lsp-title = Jezički serveri
lsp-search = Pretraži jezičke servere, lintere, formatere…
lsp-loading = Učitavanje kataloga…
lsp-empty = Nema odgovarajućih jezičkih servera
lsp-empty-detail = Pokušaj s drugim jezikom, linterom ili formaterom.
lsp-needs = traži { $tool }
lsp-status-available = Dostupno
lsp-status-on-path = Na PATH
lsp-status-installing = Instaliranje…
lsp-status-installed = Instalirano
lsp-status-outdated = Dostupno ažuriranje
lsp-status-running = Pokrenuto
lsp-status-failed = Neuspjelo

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

services-title = Pozadinski servisi
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procesa
}
services-kill-all = Prekini sve
services-not-running = Servis nije pokrenut
services-start-with = Pokreni pomoću:
services-empty = Nema aktivnih procesa
services-filter = Filtriraj procese…
services-no-match = Nema odgovarajućih procesa
services-connected = Povezano
services-disconnected = Prekinuta veza
services-attached = prikačeno
services-kill = Prekini
services-memory = Memorija
services-size = Veličina
services-shell = Shell

error-title = Greška

history-search = Pretraži historiju
history-clear-all = Obriši sve
history-clear-confirm = Obrisati cijelu historiju?
history-clear-warning = Ovo se ne može poništiti.
history-cancel = Otkaži
history-today = Danas
history-yesterday = Jučer
history-days-ago = prije { $count } dana
history-day-offset = Dan -{ $count }

settings-title = Postavke
settings-loading = Učitavanje postavki…
settings-stored = Spremljeno u ~/.vmux/settings.ron
settings-other = Ostalo
settings-software-update = Ažuriranje softvera
settings-check-updates = Provjeri ažuriranja
settings-check-updates-hint = Provjerava automatski pri pokretanju i svakog sata kada je uključeno automatsko ažuriranje.
settings-update-unavailable = Nedostupno
settings-update-unavailable-hint = Ažuriranje nije uključeno u ovu verziju.
settings-update-checking = Provjera…
settings-update-checking-hint = Provjera ažuriranja…
settings-update-check-again = Provjeri ponovo
settings-update-current = Vmux je ažuriran.
settings-update-downloading = Preuzimanje…
settings-update-downloading-hint = Preuzimanje Vmux { $version }…
settings-update-installing = Instaliranje…
settings-update-installing-hint = Instaliranje Vmux { $version }…
settings-update-ready = Ažuriranje spremno
settings-update-ready-hint = Vmux { $version } je spreman. Ponovo pokreni za primjenu.
settings-update-try-again = Pokušaj ponovo
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
composer-remove-attachment = Ukloni prilog

layout-back = Nazad
layout-forward = Naprijed
layout-reload = Ponovo učitaj
layout-bookmark-page = Dodaj ovu stranicu u oznake
layout-remove-bookmark = Ukloni oznaku
layout-pin-page = Prikači ovu stranicu
layout-unpin-page = Otkvači ovu stranicu
layout-manage-extensions = Upravljaj proširenjima
layout-new-stack = Novi sloj
layout-close-tab = Zatvori karticu
layout-bookmark = Oznaka
layout-pin = Prikači
layout-new-tab = Nova kartica
layout-team = Tim

command-switch-space = Promijeni prostor…
command-search-ask = Pretraži ili pitaj…
command-new-tab-placeholder = Pretraži ili unesi URL, ili odaberi Terminal…
command-placeholder = Unesi URL, pretraži kartice ili > za naredbe…
command-composer-placeholder = Unesi / za naredbe ili @ za medije
command-send = Pošalji (Enter)
command-terminal = Terminal
command-open-terminal = Otvori u Terminalu
command-stack = Sloj
command-tabs = { $count ->
    [one] 1 kartica
   *[other] { $count } kartica
}
command-prompt = Uputa
command-new-tab = Nova kartica
command-search = Pretraži
command-open-value = Otvori “{ $value }”
command-search-value = Pretraži “{ $value }”

schema-appearance = Izgled
schema-general = Opšte
schema-layout = Raspored
schema-layout-detail = Prozor, okna, bočna traka i fokusni okvir.
schema-agent = Agent
schema-agent-detail = Ponašanje agenta i dozvole za alate.
schema-shortcuts = Prečice
schema-shortcuts-detail = Samo za pregled. Za promjenu prečica direktno uredi settings.ron.
schema-terminal = Terminal
schema-browser = Preglednik
schema-mode = Način
schema-mode-detail = Shema boja za web stranice. Uređaj prati sistem.
schema-device = Uređaj
schema-light = Svijetlo
schema-dark = Tamno
schema-language = Jezik
schema-language-detail = Koristi sistem, en-US, ja ili bilo koju BCP 47 oznaku s odgovarajućim ~/.vmux/locales/<tag>.ftl katalogom.
schema-auto-update = Automatsko ažuriranje
schema-auto-update-detail = Provjeri i instaliraj ažuriranja pri pokretanju i svakog sata.
schema-startup-url = Početni URL
schema-startup-url-detail = Ako je prazno, otvara se upit komandne trake.
schema-search-engine = Pretraživač
schema-search-engine-detail = Koristi se za web pretrage sa Starta i iz komandne trake.
schema-window = Prozor
schema-pane = Okno
schema-side-sheet = Bočni panel
schema-focus-ring = Fokusni okvir
schema-run-placement = Dozvoli promjenu smještaja pokretanja
schema-run-placement-detail = Dozvoli agentima da biraju način okna za pokretanje, smjer i sidro.
schema-leader = Vodeća tipka
schema-leader-detail = Prefiksna tipka za akordne prečice.
schema-chord-timeout = Istek akorda
schema-chord-timeout-detail = Milisekunde prije isteka prefiksa akorda.
schema-bindings = Veze tipki
schema-confirm-close = Potvrdi zatvaranje
schema-confirm-close-detail = Pitaj prije zatvaranja terminala s pokrenutim procesom.
schema-default-theme = Zadana tema
schema-default-theme-detail = Naziv aktivne teme iz liste tema.
