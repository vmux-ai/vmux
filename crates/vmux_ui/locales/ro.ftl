common-open = Deschide
common-close = Închide
common-install = Instalează
common-uninstall = Dezinstalează
common-update = Actualizează
common-retry = Reîncearcă
common-refresh = Reîmprospătează
common-remove = Elimină
common-enable = Activează
common-disable = Dezactivează
common-new = Nou
common-active = activ
common-running = în execuție
common-done = finalizat
common-failed = Eșuat
common-installed = Instalat
common-items = { $count ->
    [one] { $count } element
   *[other] { $count } elemente
}
start-title = Start
start-tagline = Un prompt. Orice, gata.

agents-title = Agenți
agents-search = Caută agenți ACP și CLI…
agents-empty = Niciun agent corespunzător
agents-empty-detail = Încearcă un nume, runtime sau ACP/CLI.
agents-install-failed = Instalare eșuată
agents-updating = Se actualizează…
agents-retrying = Se reîncearcă…
agents-preparing = Se pregătește…

extensions-title = Extensii
extensions-search = Caută instalate sau în Chrome Web Store…
extensions-relaunch = Repornește pentru a aplica
extensions-empty = Nicio extensie instalată
extensions-no-match = Nicio extensie corespunzătoare
extensions-empty-detail = Caută în Chrome Web Store de mai sus și apasă Return.
extensions-no-match-detail = Încearcă alt nume sau ID de extensie.
extensions-on = Pornit
extensions-off = Oprit
extensions-enable-confirm = Activezi { $name }?
extensions-enable-permissions = Activează { $name } și permite:

lsp-title = Servere de limbaj
lsp-search = Caută servere de limbaj, lintere, formatoare…
lsp-loading = Se încarcă catalogul…
lsp-empty = Niciun server de limbaj corespunzător
lsp-empty-detail = Încearcă alt limbaj, linter sau formator.
lsp-needs = necesită { $tool }
lsp-status-available = Disponibil
lsp-status-on-path = Pe PATH
lsp-status-installing = Se instalează…
lsp-status-installed = Instalat
lsp-status-outdated = Actualizare disponibilă
lsp-status-running = În execuție
lsp-status-failed = Eșuat

spaces-title = Spații
spaces-new-placeholder = Nume spațiu nou
spaces-empty = Niciun spațiu
spaces-default-name = Spațiu { $number }
spaces-tabs = { $count ->
    [one] 1 filă
   *[other] { $count } file
}
spaces-delete = Șterge spațiu

team-title = Echipă
team-just-you = Doar tu în acest spațiu
team-agents = { $count ->
    [one] Tu și 1 agent
   *[other] Tu și { $count } agenți
}
team-empty = Nimeni aici încă
team-you = Tu
team-agent = Agent

services-title = Servicii de fundal
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procese
}
services-kill-all = Oprește toate
services-not-running = Serviciul nu rulează
services-start-with = Pornește cu:
services-empty = Niciun proces activ
services-filter = Filtrează procese…
services-no-match = Niciun proces corespunzător
services-connected = Conectat
services-disconnected = Deconectat
services-attached = atașat
services-kill = Oprește
services-memory = Memorie
services-size = Dimensiune
services-shell = Shell

error-title = Eroare

history-search = Caută în istoric
history-clear-all = Șterge tot
history-clear-confirm = Ștergi tot istoricul?
history-clear-warning = Această acțiune nu poate fi anulată.
history-cancel = Anulează
history-today = Astăzi
history-yesterday = Ieri
history-days-ago = Acum { $count } zile
history-day-offset = Ziua -{ $count }

settings-title = Setări
settings-loading = Se încarcă setările…
settings-stored = Stocat în ~/.vmux/settings.ron
settings-other = Altele
settings-software-update = Actualizare software
settings-check-updates = Verifică actualizări
settings-check-updates-hint = Verifică automat la lansare și în fiecare oră când actualizarea automată este activată.
settings-update-unavailable = Indisponibil
settings-update-unavailable-hint = Actualizatorul nu este inclus în această versiune.
settings-update-checking = Se verifică…
settings-update-checking-hint = Se verifică actualizări…
settings-update-check-again = Verifică din nou
settings-update-current = Vmux este actualizat.
settings-update-downloading = Se descarcă…
settings-update-downloading-hint = Se descarcă Vmux { $version }…
settings-update-installing = Se instalează…
settings-update-installing-hint = Se instalează Vmux { $version }…
settings-update-ready = Actualizare pregătită
settings-update-ready-hint = Vmux { $version } este gata. Repornește pentru a aplica.
settings-update-try-again = Încearcă din nou
settings-update-failed = Imposibil de verificat actualizările.
settings-item = Element
settings-item-number = Element { $number }
settings-press-key = Apasă o tastă…
settings-saved = Salvat
settings-record-key = Clic pentru a înregistra o combinație de taste nouă

tray-open-window = Deschide fereastra
tray-close-window = Închide fereastra
tray-pause-recording = Pauză înregistrare
tray-resume-recording = Reia înregistrarea
tray-finish-recording = Finalizează înregistrarea
tray-quit = Ieși din Vmux

composer-attach-files = Atașează fișiere (/upload)
composer-remove-attachment = Elimină atașament

layout-back = Înapoi
layout-forward = Înainte
layout-reload = Reîncarcă
layout-bookmark-page = Marchează această pagină
layout-remove-bookmark = Elimină marcaj
layout-pin-page = Fixează această pagină
layout-unpin-page = Defixează această pagină
layout-manage-extensions = Gestionează extensii
layout-new-stack = Stack nou
layout-close-tab = Închide fila
layout-bookmark = Marcaj
layout-pin = Fixare
layout-new-tab = Filă nouă
layout-team = Echipă

command-switch-space = Schimbă spațiu…
command-search-ask = Caută sau întreabă…
command-new-tab-placeholder = Caută sau tastează o adresă URL ori selectează Terminal…
command-placeholder = Tastează o adresă URL, caută file sau > pentru comenzi…
command-composer-placeholder = Tastează / pentru comenzi sau @ pentru media
command-send = Trimite (Enter)
command-terminal = Terminal
command-open-terminal = Deschide în Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] 1 filă
   *[other] { $count } file
}
command-prompt = Prompt
command-new-tab = Filă nouă
command-search = Caută
command-open-value = Deschide „{ $value }"
command-search-value = Caută „{ $value }"

schema-appearance = Aspect
schema-general = General
schema-layout = Layout
schema-layout-detail = Fereastră, panouri, bară laterală și inel de focalizare.
schema-agent = Agent
schema-agent-detail = Comportamentul agentului și permisiunile instrumentelor.
schema-shortcuts = Scurtături
schema-shortcuts-detail = Vizualizare doar-citire. Editează settings.ron direct pentru a schimba asocierile.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Mod
schema-mode-detail = Schema de culori pentru paginile web. Dispozitivul urmează sistemul tău.
schema-device = Dispozitiv
schema-light = Luminos
schema-dark = Întunecat
schema-language = Limbă
schema-language-detail = Folosește system, en-US, ja sau orice etichetă BCP 47 cu un catalog ~/.vmux/locales/<tag>.ftl corespunzător.
schema-auto-update = Actualizare automată
schema-auto-update-detail = Verifică și instalează actualizări la lansare și în fiecare oră.
schema-startup-url = URL de pornire
schema-startup-url-detail = Gol deschide promptul barei de comenzi.
schema-search-engine = Motor de căutare
schema-search-engine-detail = Folosit pentru căutări web din Start și bara de comenzi.
schema-window = Fereastră
schema-pane = Panou
schema-side-sheet = Panou lateral
schema-focus-ring = Inel de focalizare
schema-run-placement = Permite suprascrierea plasării la rulare
schema-run-placement-detail = Permite agenților să aleagă modul, direcția și ancora panoului de rulare.
schema-leader = Leader
schema-leader-detail = Tastă prefix pentru scurtăturile de acord.
schema-chord-timeout = Timeout acord
schema-chord-timeout-detail = Milisecunde înainte ca un prefix de acord să expire.
schema-bindings = Asocieri
schema-confirm-close = Confirmă închiderea
schema-confirm-close-detail = Solicită confirmare înainte de a închide un terminal cu un proces în execuție.
schema-default-theme = Temă implicită
schema-default-theme-detail = Numele temei active din lista de teme.
