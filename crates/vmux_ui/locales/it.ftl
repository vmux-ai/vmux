common-open = Aperto
common-close = Chiudi
common-install = Installa
common-uninstall = Disinstallare
common-update = Aggiorna
common-retry = Riprova
common-refresh = Aggiorna
common-remove = Rimuovi
common-enable = Abilita
common-disable = Disabilita
common-new = Nuovo
common-active = attivo
common-running = correre
common-done = fatto
common-failed = Fallito
common-installed = Installato
common-items = { $count ->
    [one] { $count } elemento
   *[other] { $count } elementi
}
start-title = Inizia
start-tagline = Un suggerimento. Qualunque cosa, fatta.

agents-title = Agenti
agents-search = Cerca agenti ACP e CLI…
agents-empty = Nessun agente corrispondente
agents-empty-detail = Prova un nome, un runtime o ACP/CLI.
agents-install-failed = Installazione non riuscita
agents-updating = Aggiornamento…
agents-retrying = Nuovo tentativo…
agents-preparing = Preparazione…

extensions-title = Estensioni
extensions-search = Cerca installato o Chrome Web Store...
extensions-relaunch = Riavviare per candidarsi
extensions-empty = Nessuna estensione installata
extensions-no-match = Nessuna estensione corrispondente
extensions-empty-detail = Cerca Chrome Web Store sopra e premi Return.
extensions-no-match-detail = Prova un altro nome o ID estensione.
extensions-on = Su
extensions-off = Spento
extensions-enable-confirm = Abilitare { $name }?
extensions-enable-permissions = Abilita { $name } e consenti:

lsp-title = Server linguistici
lsp-search = Cerca server linguistici, linter, formattatori...
lsp-loading = Caricamento catalogo...
lsp-empty = Nessun server della lingua corrispondente
lsp-empty-detail = Prova un'altra lingua, linter o formattatore.
lsp-needs = necessita di { $tool }
lsp-status-available = Disponibile
lsp-status-on-path = Il PATH
lsp-status-installing = Installazione…
lsp-status-installed = Installato
lsp-status-outdated = Aggiornamento disponibile
lsp-status-running = Correre
lsp-status-failed = Fallito

spaces-title = Spazi
spaces-new-placeholder = Nuovo nome dello spazio
spaces-empty = Nessuno spazio
spaces-default-name = Spazio { $number }
spaces-tabs = { $count ->
    [one] 1 etichetta
   *[other] { $count } schede
}
spaces-delete = Elimina spazio

team-title = Squadra
team-just-you = Solo tu in questo spazio
team-agents = { $count ->
    [one] Tu e 1 agente
   *[other] Tu e gli agenti { $count }
}
team-empty = Nessuno qui ancora
team-you = Tu
team-agent = Agente

services-title = Servizi in background
services-processes = { $count ->
    [one] 1 processo
   *[other] { $count } processi
}
services-kill-all = Uccidi tutti
services-not-running = Il servizio non è in esecuzione
services-start-with = Inizia con:
services-empty = Nessun processo attivo
services-filter = Filtra processi...
services-no-match = Nessun processo corrispondente
services-connected = Connesso
services-disconnected = Disconnesso
services-attached = allegato
services-kill = Uccidi
services-memory = Memoria
services-size = Dimensioni
services-shell = Conchiglia

error-title = Errore

history-search = Cronologia delle ricerche
history-clear-all = Cancella tutto
history-clear-confirm = Cancellare tutta la cronologia?
history-clear-warning = Questa operazione non può essere annullata.
history-cancel = Annulla
history-today = Oggi
history-yesterday = Ieri
history-days-ago = { $count } giorni fa
history-day-offset = Giorno -{ $count }

settings-title = Impostazioni
settings-loading = Caricamento impostazioni…
settings-stored = Memorizzato in ~/.vmux/settings.ron
settings-other = Altro
settings-software-update = Aggiornamento del software
settings-check-updates = Controlla gli aggiornamenti
settings-check-updates-hint = Controlla automaticamente all'avvio e ogni ora quando l'aggiornamento automatico è abilitato.
settings-update-unavailable = Non disponibile
settings-update-unavailable-hint = Il programma di aggiornamento non è incluso in questa build.
settings-update-checking = Controllo…
settings-update-checking-hint = Controllo aggiornamenti…
settings-update-check-again = Controlla di nuovo
settings-update-current = Vmux è aggiornato.
settings-update-downloading = Download in corso...
settings-update-downloading-hint = Download di Vmux { $version }…
settings-update-installing = Installazione…
settings-update-installing-hint = Installazione di Vmux { $version }…
settings-update-ready = Aggiornamento pronto
settings-update-ready-hint = Vmux { $version } è pronto. Riavvia per applicarlo.
settings-update-try-again = Riprova
settings-update-failed = Impossibile controllare gli aggiornamenti.
settings-item = Articolo
settings-item-number = Articolo { $number }
settings-press-key = Premi un tasto...
settings-saved = Salvato
settings-record-key = Fare clic per registrare una nuova combinazione di tasti

tray-open-window = Apri finestra
tray-close-window = Chiudi finestra
tray-pause-recording = Pausa registrazione
tray-resume-recording = Riprendi la registrazione
tray-finish-recording = Termina la registrazione
tray-quit = Esci da Vmux

composer-attach-files = Allega file (/upload)
composer-remove-attachment = Rimuovi l'allegato

layout-back = Indietro
layout-forward = Avanti
layout-reload = Ricarica
layout-bookmark-page = Aggiungi questa pagina ai segnalibri
layout-remove-bookmark = Rimuovi segnalibro
layout-pin-page = Appunta questa pagina
layout-unpin-page = Sblocca questa pagina
layout-manage-extensions = Gestisci estensioni
layout-new-stack = Nuova pila
layout-close-tab = Chiudi scheda
layout-bookmark = Segnalibro
layout-pin = Perno
layout-new-tab = Nuova scheda
layout-team = Squadra

command-switch-space = Cambia spazio...
command-search-ask = Cerca o chiedi...
command-new-tab-placeholder = Cerca o digita URL oppure seleziona Terminale…
command-placeholder = Digita URL, schede di ricerca o > per i comandi...
command-composer-placeholder = Digitare / per i comandi o @ per i contenuti multimediali
command-send = Invia (Enter)
command-terminal = Terminale
command-open-terminal = Apri nel terminale
command-stack = Pila
command-tabs = { $count ->
    [one] 1 etichetta
   *[other] { $count } schede
}
command-prompt = Richiedi
command-new-tab = Nuova scheda
command-search = Cerca
command-open-value = Apri "{ $value }"
command-search-value = Cerca "{ $value }"

schema-appearance = Aspetto
schema-general = Generale
schema-layout = Disposizione
schema-layout-detail = Finestra, riquadri, barra laterale e anello di messa a fuoco.
schema-agent = Agente
schema-agent-detail = Comportamento dell'agente e autorizzazioni dello strumento.
schema-shortcuts = Scorciatoie
schema-shortcuts-detail = Visualizzazione di sola lettura. Modifica settings.ron direttamente per cambiare le associazioni.
schema-terminal = Terminale
schema-browser = Navigatore
schema-mode = Modalità
schema-mode-detail = Combinazione di colori per le pagine web. Il dispositivo segue il tuo sistema.
schema-device = Dispositivo
schema-light = Luce
schema-dark = Buio
schema-language = Lingua
schema-language-detail = Utilizza system, en-US, ja o qualsiasi tag BCP 47 con un catalogo ~/.vmux/locales/<tag>.ftl corrispondente.
schema-auto-update = Aggiornamento automatico
schema-auto-update-detail = Controlla e installa gli aggiornamenti all'avvio e ogni ora.
schema-startup-url = Avvio URL
schema-startup-url-detail = Vuoto apre il prompt della barra dei comandi.
schema-search-engine = Motore di ricerca
schema-search-engine-detail = Utilizzato per le ricerche Web da Start e dalla barra dei comandi.
schema-window = Finestra
schema-pane = Riquadro
schema-side-sheet = Foglio laterale
schema-focus-ring = Anello di messa a fuoco
schema-run-placement = Consenti l'override del posizionamento della corsa
schema-run-placement-detail = Consenti agli agenti di scegliere la modalità, la direzione e l'ancoraggio del riquadro di esecuzione.
schema-leader = Capo
schema-leader-detail = Tasto prefisso per scorciatoie di accordi.
schema-chord-timeout = Timeout dell'accordo
schema-chord-timeout-detail = Millisecondi prima della scadenza del prefisso di un accordo.
schema-bindings = Legami
schema-confirm-close = Conferma chiusura
schema-confirm-close-detail = Richiedi conferma prima di chiudere un terminale con un processo in esecuzione.
schema-default-theme = Tema predefinito
schema-default-theme-detail = Nome del tema attivo dall'elenco dei temi.
