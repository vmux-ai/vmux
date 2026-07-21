locale-name = slovenščina
common-open = Odpri
common-close = Zapri
common-install = Namesti
common-uninstall = Odstrani
common-update = Posodobi
common-retry = Poskusi znova
common-refresh = Osveži
common-remove = Odstrani
common-enable = Omogoči
common-disable = Onemogoči
common-new = Novo
common-active = aktivno
common-running = se izvaja
common-done = končano
common-failed = Spodletelo
common-installed = Nameščeno
common-items = { $count ->
    [one] { $count } element
   *[other] { $count } elementov
}
start-title = Začetek
start-tagline = En prompt. Vse opravljeno.

agents-title = Agenti
agents-search = Išči agente ACP in CLI …
agents-empty = Ni ujemajočih se agentov
agents-empty-detail = Poskusite z imenom, izvajalnim okoljem ali ACP/CLI.
agents-install-failed = Namestitev ni uspela
agents-updating = Posodabljanje …
agents-retrying = Ponovni poskus …
agents-preparing = Priprava …

extensions-title = Razširitve
extensions-search = Išči med nameščenimi ali v Chrome Web Store …
extensions-relaunch = Znova zaženi za uveljavitev
extensions-empty = Ni nameščenih razširitev
extensions-no-match = Ni ujemajočih se razširitev
extensions-empty-detail = Zgoraj poiščite v Chrome Web Store in pritisnite Enter.
extensions-no-match-detail = Poskusite z drugim imenom ali ID-jem razširitve.
extensions-on = Vklopljeno
extensions-off = Izklopljeno
extensions-enable-confirm = Omogočim { $name }?
extensions-enable-permissions = Omogoči { $name } in dovoli:

lsp-title = Jezikovni strežniki
lsp-search = Išči jezikovne strežnike, linterje, oblikovalnike …
lsp-loading = Nalaganje kataloga …
lsp-empty = Ni ujemajočih se jezikovnih strežnikov
lsp-empty-detail = Poskusite z drugim jezikom, linterjem ali oblikovalnikom.
lsp-needs = potrebuje { $tool }
lsp-status-available = Na voljo
lsp-status-on-path = Na PATH
lsp-status-installing = Nameščanje …
lsp-status-installed = Nameščeno
lsp-status-outdated = Na voljo je posodobitev
lsp-status-running = Se izvaja
lsp-status-failed = Spodletelo

spaces-title = Prostori
spaces-new-placeholder = Ime novega prostora
spaces-empty = Ni prostorov
spaces-default-name = Prostor { $number }
spaces-tabs = { $count ->
    [one] 1 zavihek
   *[other] { $count } zavihkov
}
spaces-delete = Izbriši prostor

team-title = Ekipa
team-just-you = V tem prostoru ste samo vi
team-agents = { $count ->
    [one] Vi in 1 agent
   *[other] Vi in { $count } agentov
}
team-empty = Tukaj še ni nikogar
team-you = Vi
team-agent = Agent

services-title = Storitve v ozadju
services-processes = { $count ->
    [one] 1 proces
   *[other] { $count } procesov
}
services-kill-all = Prisili končanje vseh
services-not-running = Storitev se ne izvaja
services-start-with = Zaženi z:
services-empty = Ni aktivnih procesov
services-filter = Filtriraj procese …
services-no-match = Ni ujemajočih se procesov
services-connected = Povezano
services-disconnected = Prekinjena povezava
services-attached = pripeto
services-kill = Prisili končanje
services-memory = Pomnilnik
services-size = Velikost
services-shell = Lupina

error-title = Napaka

history-search = Išči po zgodovini
history-clear-all = Počisti vse
history-clear-confirm = Počistim vso zgodovino?
history-clear-warning = Tega ni mogoče razveljaviti.
history-cancel = Prekliči
history-today = Danes
history-yesterday = Včeraj
history-days-ago = pred { $count } dnevi
history-day-offset = Dan -{ $count }

settings-title = Nastavitve
settings-loading = Nalaganje nastavitev …
settings-stored = Shranjeno v ~/.vmux/settings.ron
settings-other = Drugo
settings-software-update = Posodobitev programske opreme
settings-check-updates = Preveri posodobitve
settings-check-updates-hint = Ob omogočenih samodejnih posodobitvah preveri ob zagonu in vsako uro.
settings-update-unavailable = Ni na voljo
settings-update-unavailable-hint = Posodabljalnik ni vključen v to gradnjo.
settings-update-checking = Preverjanje …
settings-update-checking-hint = Preverjanje posodobitev …
settings-update-check-again = Preveri znova
settings-update-current = Vmux je posodobljen.
settings-update-downloading = Prenašanje …
settings-update-downloading-hint = Prenašanje Vmux { $version } …
settings-update-installing = Nameščanje …
settings-update-installing-hint = Nameščanje Vmux { $version } …
settings-update-ready = Posodobitev je pripravljena
settings-update-ready-hint = Vmux { $version } je pripravljen. Znova zaženite za uveljavitev.
settings-update-try-again = Poskusi znova
settings-update-failed = Posodobitev ni bilo mogoče preveriti.
settings-item = Element
settings-item-number = Element { $number }
settings-press-key = Pritisnite tipko …
settings-saved = Shranjeno
settings-record-key = Kliknite za snemanje nove kombinacije tipk

tray-open-window = Odpri okno
tray-close-window = Zapri okno
tray-pause-recording = Začasno ustavi snemanje
tray-resume-recording = Nadaljuj snemanje
tray-finish-recording = Končaj snemanje
tray-quit = Zapri Vmux

composer-attach-files = Priloži datoteke (/upload)
composer-remove-attachment = Odstrani prilogo

layout-back = Nazaj
layout-forward = Naprej
layout-reload = Znova naloži
layout-bookmark-page = Dodaj stran med zaznamke
layout-remove-bookmark = Odstrani zaznamek
layout-pin-page = Pripni to stran
layout-unpin-page = Odpni to stran
layout-manage-extensions = Upravljaj razširitve
layout-new-stack = Nov sklad
layout-close-tab = Zapri zavihek
layout-bookmark = Zaznamek
layout-pin = Pripni
layout-new-tab = Nov zavihek
layout-team = Ekipa

command-switch-space = Preklopi prostor …
command-search-ask = Išči ali vprašaj …
command-new-tab-placeholder = Iščite, vnesite URL ali izberite Terminal …
command-placeholder = Vnesite URL, poiščite zavihke ali > za ukaze …
command-composer-placeholder = Vnesite / za ukaze ali @ za predstavnost
command-send = Pošlji (Enter)
command-terminal = Terminal
command-open-terminal = Odpri v Terminalu
command-stack = Sklad
command-tabs = { $count ->
    [one] 1 zavihek
   *[other] { $count } zavihkov
}
command-prompt = Prompt
command-new-tab = Nov zavihek
command-search = Išči
command-open-value = Odpri »{ $value }«
command-search-value = Išči »{ $value }«

schema-appearance = Videz
schema-general = Splošno
schema-layout = Postavitev
schema-layout-detail = Okno, podokna, stranska vrstica in obroč fokusa.
schema-agent = Agent
schema-agent-detail = Vedenje agenta in dovoljenja za orodja.
schema-shortcuts = Bližnjice
schema-shortcuts-detail = Pogled samo za branje. Za spremembo vezav neposredno uredite settings.ron.
schema-terminal = Terminal
schema-browser = Brskalnik
schema-mode = Način
schema-mode-detail = Barvna shema za spletne strani. Naprava sledi sistemu.
schema-device = Naprava
schema-light = Svetlo
schema-dark = Temno
schema-language = Jezik
schema-language-detail = Uporabite sistem, en-US, ja ali katero koli oznako BCP 47 z ustreznim katalogom ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Samodejno posodabljanje
schema-auto-update-detail = Preveri in namesti posodobitve ob zagonu in vsako uro.
schema-startup-url = Začetni URL
schema-startup-url-detail = Prazno odpre prompt ukazne vrstice.
schema-search-engine = Iskalnik
schema-search-engine-detail = Uporablja se za spletna iskanja iz Začetka in ukazne vrstice.
schema-window = Okno
schema-pane = Podokno
schema-side-sheet = Stranski list
schema-focus-ring = Obroč fokusa
schema-run-placement = Dovoli preglasitev postavitve izvajanja
schema-run-placement-detail = Dovoli agentom izbrati način, smer in sidro podokna za izvajanje.
schema-leader = Vodilna tipka
schema-leader-detail = Predponska tipka za akordne bližnjice.
schema-chord-timeout = Časovna omejitev akorda
schema-chord-timeout-detail = Milisekunde, preden predpona akorda poteče.
schema-bindings = Vezave
schema-confirm-close = Potrdi zapiranje
schema-confirm-close-detail = Pred zapiranjem terminala s procesom v teku prikaži poziv.
schema-default-theme = Privzeta tema
schema-default-theme-detail = Ime aktivne teme s seznama tem.
