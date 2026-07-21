locale-name = română
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
common-running = rulează
common-done = gata
common-failed = Eșuat
common-installed = Instalat
common-items = { $count ->
    [one] { $count } element
   *[other] { $count } elemente
}
start-title = Start
start-tagline = Un singur prompt. Orice, rezolvat.

agents-title = Agenți
agents-search = Caută agenți ACP și CLI…
agents-empty = Nu există agenți potriviți
agents-empty-detail = Încearcă un nume, un runtime sau ACP/CLI.
agents-install-failed = Instalarea a eșuat
agents-updating = Se actualizează…
agents-retrying = Se reîncearcă…
agents-preparing = Se pregătește…

extensions-title = Extensii
extensions-search = Caută extensii instalate sau în Chrome Web Store…
extensions-relaunch = Repornește pentru aplicare
extensions-empty = Nu există extensii instalate
extensions-no-match = Nu există extensii potrivite
extensions-empty-detail = Caută mai sus în Chrome Web Store și apasă Enter.
extensions-no-match-detail = Încearcă alt nume sau alt ID de extensie.
extensions-on = Activat
extensions-off = Dezactivat
extensions-enable-confirm = Activezi { $name }?
extensions-enable-permissions = Activează { $name } și permite:

lsp-title = Servere de limbaj
lsp-search = Caută servere de limbaj, linters, formatatoare…
lsp-loading = Se încarcă catalogul…
lsp-empty = Nu există servere de limbaj potrivite
lsp-empty-detail = Încearcă alt limbaj, linter sau formator.
lsp-needs = necesită { $tool }
lsp-status-available = Disponibil
lsp-status-on-path = În PATH
lsp-status-installing = Se instalează…
lsp-status-installed = Instalat
lsp-status-outdated = Actualizare disponibilă
lsp-status-running = Rulează
lsp-status-failed = Eșuat

spaces-title = Spații
spaces-new-placeholder = Numele noului spațiu
spaces-empty = Nu există spații
spaces-default-name = Spațiu { $number }
spaces-tabs = { $count ->
    [one] 1 filă
   *[other] { $count } file
}
spaces-delete = Șterge spațiul

team-title = Echipă
team-just-you = Doar tu în acest spațiu
team-agents = { $count ->
    [one] Tu și 1 agent
   *[other] Tu și { $count } agenți
}
team-empty = Nu este nimeni aici încă
team-you = Tu
team-agent = Agent

services-title = Servicii în fundal
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procese
}
services-kill-all = Oprește forțat tot
services-not-running = Serviciul nu rulează
services-start-with = Pornește cu:
services-empty = Nu există procese active
services-filter = Filtrează procese…
services-no-match = Nu există procese potrivite
services-connected = Conectat
services-disconnected = Deconectat
services-attached = atașat
services-kill = Oprește forțat
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
history-days-ago = acum { $count } zile
history-day-offset = Ziua -{ $count }

settings-title = Setări
settings-loading = Se încarcă setările…
settings-stored = Stocat în ~/.vmux/settings.ron
settings-other = Altele
settings-software-update = Actualizare software
settings-check-updates = Caută actualizări
settings-check-updates-hint = Verifică automat la pornire și la fiecare oră când actualizarea automată este activată.
settings-update-unavailable = Indisponibil
settings-update-unavailable-hint = Modulul de actualizare nu este inclus în acest build.
settings-update-checking = Se verifică…
settings-update-checking-hint = Se caută actualizări…
settings-update-check-again = Verifică din nou
settings-update-current = Vmux este la zi.
settings-update-downloading = Se descarcă…
settings-update-downloading-hint = Se descarcă Vmux { $version }…
settings-update-installing = Se instalează…
settings-update-installing-hint = Se instalează Vmux { $version }…
settings-update-ready = Actualizare pregătită
settings-update-ready-hint = Vmux { $version } este gata. Repornește pentru aplicare.
settings-update-try-again = Încearcă din nou
settings-update-failed = Nu s-au putut verifica actualizările.
settings-item = Element
settings-item-number = Element { $number }
settings-press-key = Apasă o tastă…
settings-saved = Salvat
settings-record-key = Click pentru a înregistra o nouă combinație de taste

tray-open-window = Deschide fereastra
tray-close-window = Închide fereastra
tray-pause-recording = Pune înregistrarea pe pauză
tray-resume-recording = Reia înregistrarea
tray-finish-recording = Finalizează înregistrarea
tray-quit = Închide Vmux

composer-attach-files = Atașează fișiere (/upload)
composer-remove-attachment = Elimină atașamentul

layout-back = Înapoi
layout-forward = Înainte
layout-reload = Reîncarcă
layout-bookmark-page = Adaugă pagina la favorite
layout-remove-bookmark = Elimină favoritul
layout-pin-page = Fixează pagina
layout-unpin-page = Anulează fixarea paginii
layout-manage-extensions = Gestionează extensiile
layout-new-stack = Stivă nouă
layout-close-tab = Închide fila
layout-bookmark = Favorit
layout-pin = Fixează
layout-new-tab = Filă nouă
layout-team = Echipă

command-switch-space = Schimbă spațiul…
command-search-ask = Caută sau întreabă…
command-new-tab-placeholder = Caută, introdu un URL sau selectează Terminal…
command-placeholder = Introdu un URL, caută file sau folosește > pentru comenzi…
command-composer-placeholder = Tastează / pentru comenzi sau @ pentru media
command-send = Trimite (Enter)
command-terminal = Terminal
command-open-terminal = Deschide în Terminal
command-stack = Stivă
command-tabs = { $count ->
    [one] 1 filă
   *[other] { $count } file
}
command-prompt = Prompt
command-new-tab = Filă nouă
command-search = Caută
command-open-value = Deschide „{ $value }”
command-search-value = Caută „{ $value }”

schema-appearance = Aspect
schema-general = General
schema-layout = Aranjare
schema-layout-detail = Fereastră, panouri, bară laterală și contur de focalizare.
schema-agent = Agent
schema-agent-detail = Comportamentul agentului și permisiunile pentru unelte.
schema-shortcuts = Scurtături
schema-shortcuts-detail = Vizualizare doar în citire. Editează direct settings.ron pentru a schimba asocierile.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Mod
schema-mode-detail = Schema de culori pentru pagini web. Dispozitiv folosește setarea sistemului.
schema-device = Dispozitiv
schema-light = Luminos
schema-dark = Întunecat
schema-language = Limbă
schema-language-detail = Folosește sistemul, en-US, ja sau orice etichetă BCP 47 cu un catalog ~/.vmux/locales/<tag>.ftl corespunzător.
schema-auto-update = Actualizare automată
schema-auto-update-detail = Caută și instalează actualizări la pornire și la fiecare oră.
schema-startup-url = URL la pornire
schema-startup-url-detail = Dacă este gol, se deschide promptul barei de comenzi.
schema-search-engine = Motor de căutare
schema-search-engine-detail = Folosit pentru căutări web din Start și din bara de comenzi.
schema-window = Fereastră
schema-pane = Panou
schema-side-sheet = Panou lateral
schema-focus-ring = Contur de focalizare
schema-run-placement = Permite suprascrierea plasării rulării
schema-run-placement-detail = Permite agenților să aleagă modul panoului de rulare, direcția și ancora.
schema-leader = Leader
schema-leader-detail = Tastă prefix pentru scurtături chord.
schema-chord-timeout = Timeout chord
schema-chord-timeout-detail = Milisecunde înainte ca un prefix chord să expire.
schema-bindings = Asocieri
schema-confirm-close = Confirmă închiderea
schema-confirm-close-detail = Cere confirmare înainte de a închide un terminal cu un proces în rulare.
schema-default-theme = Temă implicită
schema-default-theme-detail = Numele temei active din lista de teme.
