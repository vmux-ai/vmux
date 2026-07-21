common-open = Apri
common-close = Chiudi
common-install = Installa
common-uninstall = Disinstalla
common-update = Aggiorna
common-retry = Riprova
common-refresh = Ricarica
common-remove = Rimuovi
common-enable = Abilita
common-disable = Disabilita
common-new = Nuovo
common-active = attivo
common-running = in esecuzione
common-done = completato
common-failed = Non riuscito
common-installed = Installato
common-items = { $count ->
    [one] { $count } elemento
   *[other] { $count } elementi
}
start-title = Inizia
start-tagline = Un solo prompt. Tutto fatto.

agents-title = Agenti
agents-search = Cerca agenti ACP e CLI…
agents-empty = Nessun agente corrispondente
agents-empty-detail = Prova con un nome, un runtime o ACP/CLI.
agents-install-failed = Installazione non riuscita
agents-updating = Aggiornamento…
agents-retrying = Nuovo tentativo…
agents-preparing = Preparazione…

extensions-title = Estensioni
extensions-search = Cerca tra le installate o nel Chrome Web Store…
extensions-relaunch = Riavvia per applicare
extensions-empty = Nessuna estensione installata
extensions-no-match = Nessuna estensione corrispondente
extensions-empty-detail = Cerca nel Chrome Web Store qui sopra e premi Invio.
extensions-no-match-detail = Prova con un altro nome o ID estensione.
extensions-on = Attive
extensions-off = Disattive
extensions-enable-confirm = Abilitare { $name }?
extensions-enable-permissions = Abilitare { $name } e consentire:

lsp-title = Server di linguaggio
lsp-search = Cerca server di linguaggio, linter, formatter…
lsp-loading = Caricamento catalogo…
lsp-empty = Nessun server di linguaggio corrispondente
lsp-empty-detail = Prova con un altro linguaggio, linter o formatter.
lsp-needs = richiede { $tool }
lsp-status-available = Disponibile
lsp-status-on-path = Nel PATH
lsp-status-installing = Installazione…
lsp-status-installed = Installato
lsp-status-outdated = Aggiornamento disponibile
lsp-status-running = In esecuzione
lsp-status-failed = Non riuscito

spaces-title = Spazi
spaces-new-placeholder = Nome del nuovo spazio
spaces-empty = Nessuno spazio
spaces-default-name = Spazio { $number }
spaces-tabs = { $count ->
    [one] 1 scheda
   *[other] { $count } schede
}
spaces-delete = Elimina spazio

team-title = Team
team-just-you = Solo tu in questo spazio
team-agents = { $count ->
    [one] Tu e 1 agente
   *[other] Tu e { $count } agenti
}
team-empty = Non c’è ancora nessuno
team-you = Tu
team-agent = Agente

services-title = Servizi in background
services-processes = { $count ->
    [one] 1 processo
   *[other] { $count } processi
}
services-kill-all = Termina tutti
services-not-running = Il servizio non è in esecuzione
services-start-with = Avvia con:
services-empty = Nessun processo attivo
services-filter = Filtra processi…
services-no-match = Nessun processo corrispondente
services-connected = Connesso
services-disconnected = Disconnesso
services-attached = collegato
services-kill = Termina
services-memory = Memoria
services-size = Dimensione
services-shell = Shell

error-title = Errore

history-search = Cerca nella cronologia
history-clear-all = Cancella tutto
history-clear-confirm = Cancellare tutta la cronologia?
history-clear-warning = L’azione non può essere annullata.
history-cancel = Annulla
history-today = Oggi
history-yesterday = Ieri
history-days-ago = { $count } giorni fa
history-day-offset = Giorno -{ $count }

settings-title = Impostazioni
settings-loading = Caricamento impostazioni…
settings-stored = Salvate in ~/.vmux/settings.ron
settings-other = Altro
settings-software-update = Aggiornamento software
settings-check-updates = Cerca aggiornamenti
settings-check-updates-hint = Controlla automaticamente all’avvio e ogni ora quando l’aggiornamento automatico è attivo.
settings-update-unavailable = Non disponibile
settings-update-unavailable-hint = Il sistema di aggiornamento non è incluso in questa build.
settings-update-checking = Controllo…
settings-update-checking-hint = Controllo aggiornamenti…
settings-update-check-again = Controlla di nuovo
settings-update-current = Vmux è aggiornato.
settings-update-downloading = Download…
settings-update-downloading-hint = Download di Vmux { $version }…
settings-update-installing = Installazione…
settings-update-installing-hint = Installazione di Vmux { $version }…
settings-update-ready = Aggiornamento pronto
settings-update-ready-hint = Vmux { $version } è pronto. Riavvia per applicarlo.
settings-update-try-again = Riprova
settings-update-failed = Impossibile controllare gli aggiornamenti.
settings-item = Elemento
settings-item-number = Elemento { $number }
settings-press-key = Premi un tasto…
settings-saved = Salvato
settings-record-key = Fai clic per registrare una nuova combinazione di tasti

tray-open-window = Apri finestra
tray-close-window = Chiudi finestra
tray-pause-recording = Metti in pausa la registrazione
tray-resume-recording = Riprendi registrazione
tray-finish-recording = Termina registrazione
tray-quit = Esci da Vmux

composer-attach-files = Allega file (/upload)
composer-remove-attachment = Rimuovi allegato

layout-back = Indietro
layout-forward = Avanti
layout-reload = Ricarica
layout-bookmark-page = Aggiungi questa pagina ai segnalibri
layout-remove-bookmark = Rimuovi segnalibro
layout-pin-page = Fissa questa pagina
layout-unpin-page = Sblocca questa pagina
layout-manage-extensions = Gestisci estensioni
layout-new-stack = Nuovo stack
layout-close-tab = Chiudi scheda
layout-bookmark = Segnalibro
layout-pin = Fissa
layout-new-tab = Nuova scheda
layout-team = Team

command-switch-space = Cambia spazio…
command-search-ask = Cerca o chiedi…
command-new-tab-placeholder = Cerca o inserisci un URL, oppure seleziona Terminale…
command-placeholder = Inserisci un URL, cerca tra le schede o usa > per i comandi…
command-composer-placeholder = Digita / per i comandi o @ per i media
command-send = Invia (Invio)
command-terminal = Terminale
command-open-terminal = Apri nel Terminale
command-stack = Stack
command-tabs = { $count ->
    [one] 1 scheda
   *[other] { $count } schede
}
command-prompt = Prompt
command-new-tab = Nuova scheda
command-search = Cerca
command-open-value = Apri “{ $value }”
command-search-value = Cerca “{ $value }”

schema-appearance = Aspetto
schema-general = Generali
schema-layout = Layout
schema-layout-detail = Finestra, pannelli, barra laterale e indicatore di focus.
schema-agent = Agente
schema-agent-detail = Comportamento dell’agente e permessi degli strumenti.
schema-shortcuts = Scorciatoie
schema-shortcuts-detail = Vista in sola lettura. Modifica direttamente settings.ron per cambiare le associazioni.
schema-terminal = Terminale
schema-browser = Browser
schema-mode = Modalità
schema-mode-detail = Schema colori per le pagine web. Dispositivo segue il sistema.
schema-device = Dispositivo
schema-light = Chiaro
schema-dark = Scuro
schema-language = Lingua
schema-language-detail = Usa il sistema, en-US, ja o qualsiasi tag BCP 47 con un catalogo ~/.vmux/locales/<tag>.ftl corrispondente.
schema-auto-update = Aggiornamento automatico
schema-auto-update-detail = Cerca e installa aggiornamenti all’avvio e ogni ora.
schema-startup-url = URL di avvio
schema-startup-url-detail = Se vuoto, apre il prompt della barra comandi.
schema-search-engine = Motore di ricerca
schema-search-engine-detail = Usato per le ricerche web da Inizia e dalla barra comandi.
schema-window = Finestra
schema-pane = Pannello
schema-side-sheet = Pannello laterale
schema-focus-ring = Indicatore di focus
schema-run-placement = Consenti override del posizionamento di esecuzione
schema-run-placement-detail = Consenti agli agenti di scegliere modalità, direzione e ancoraggio del pannello di esecuzione.
schema-leader = Leader
schema-leader-detail = Tasto prefisso per scorciatoie a sequenza.
schema-chord-timeout = Timeout sequenza
schema-chord-timeout-detail = Millisecondi prima della scadenza del prefisso di sequenza.
schema-bindings = Associazioni
schema-confirm-close = Conferma chiusura
schema-confirm-close-detail = Chiedi conferma prima di chiudere un terminale con un processo in esecuzione.
schema-default-theme = Tema predefinito
schema-default-theme-detail = Nome del tema attivo dall’elenco dei temi.
