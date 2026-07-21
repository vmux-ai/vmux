common-open = Atidaryti
common-close = Uždaryti
common-install = Įdiegti
common-uninstall = Pašalinkite
common-update = Atnaujinti
common-retry = Bandykite dar kartą
common-refresh = Atnaujinti
common-remove = Pašalinti
common-enable = Įgalinti
common-disable = Išjungti
common-new = Nauja
common-active = aktyvus
common-running = bėgimas
common-done = padaryta
common-failed = Nepavyko
common-installed = Įdiegta
common-items = { $count ->
    [one] { $count } elementas
   *[other] { $count } elementai
}
start-title = Pradėti
start-tagline = Vienas raginimas. Viskas, padaryta.

agents-title = Agentai
agents-search = Ieškoti ACP ir CLI agentų…
agents-empty = Nėra atitinkamų agentų
agents-empty-detail = Išbandykite pavadinimą, vykdymo laiką arba ACP/CLI.
agents-install-failed = Diegimas nepavyko
agents-updating = Atnaujinama…
agents-retrying = Bandoma dar kartą…
agents-preparing = Ruošiamasi…

extensions-title = Plėtiniai
extensions-search = Ieškoti įdiegtų arba Chrome Web Store…
extensions-relaunch = Norėdami taikyti, paleiskite iš naujo
extensions-empty = Nėra įdiegtų plėtinių
extensions-no-match = Nėra atitinkančių plėtinių
extensions-empty-detail = Ieškokite Chrome Web Store aukščiau ir paspauskite Return.
extensions-no-match-detail = Išbandykite kitą pavadinimą arba plėtinio ID.
extensions-on = Įjungta
extensions-off = Išjungta
extensions-enable-confirm = Įjungti { $name }?
extensions-enable-permissions = Įgalinti { $name } ir leisti:

lsp-title = Kalbos serveriai
lsp-search = Ieškokite kalbų serverių, linijų, formatuotojų...
lsp-loading = Įkeliamas katalogas…
lsp-empty = Nėra atitinkamos kalbos serverių
lsp-empty-detail = Išbandykite kitą kalbą, linterį arba formatavimo priemonę.
lsp-needs = reikia { $tool }
lsp-status-available = Galima
lsp-status-on-path = PATH
lsp-status-installing = Diegiama…
lsp-status-installed = Įdiegta
lsp-status-outdated = Galimas atnaujinimas
lsp-status-running = Bėgimas
lsp-status-failed = Nepavyko

spaces-title = Erdvės
spaces-new-placeholder = Naujas erdvės pavadinimas
spaces-empty = Jokių tarpų
spaces-default-name = Erdvė { $number }
spaces-tabs = { $count ->
    [one] 1 skirtukas
   *[other] { $count } skirtukai
}
spaces-delete = Ištrinti erdvę

team-title = Komanda
team-just-you = Tik tu šioje erdvėje
team-agents = { $count ->
    [one] Jūs ir 1 agentas
   *[other] Jūs ir { $count } agentai
}
team-empty = Čia dar niekas
team-you = Jūs
team-agent = Agentas

services-title = Pagrindinės paslaugos
services-processes = { $count ->
    [one] 1 procesas
   *[other] { $count } procesus
}
services-kill-all = Nužudyk visus
services-not-running = Paslauga neveikia
services-start-with = Pradėkite nuo:
services-empty = Nėra aktyvių procesų
services-filter = Filtruoti procesus…
services-no-match = Nėra suderinimo procesų
services-connected = Prisijungta
services-disconnected = Atjungtas
services-attached = pridedamas
services-kill = Nužudyti
services-memory = Atmintis
services-size = Dydis
services-shell = Lukštas

error-title = Klaida

history-search = Paieškos istorija
history-clear-all = Išvalyti viską
history-clear-confirm = Išvalyti visą istoriją?
history-clear-warning = To negalima anuliuoti.
history-cancel = Atšaukti
history-today = Šiandien
history-yesterday = vakar
history-days-ago = Prieš { $count } dienas
history-day-offset = Diena -{ $count }

settings-title = Nustatymai
settings-loading = Įkeliami nustatymai…
settings-stored = Saugoma ~/.vmux/settings.ron
settings-other = Kita
settings-software-update = Programinės įrangos atnaujinimas
settings-check-updates = Patikrinkite, ar nėra atnaujinimų
settings-check-updates-hint = Tikrinama automatiškai paleidžiant ir kas valandą, kai įjungtas automatinis atnaujinimas.
settings-update-unavailable = Nepasiekiamas
settings-update-unavailable-hint = Atnaujinimo priemonė neįtraukta į šią versiją.
settings-update-checking = Tikrinama…
settings-update-checking-hint = Tikrinama, ar yra naujinių…
settings-update-check-again = Patikrinkite dar kartą
settings-update-current = Vmux yra atnaujinta.
settings-update-downloading = Atsisiunčiama…
settings-update-downloading-hint = Atsisiunčiama Vmux { $version }…
settings-update-installing = Diegiama…
settings-update-installing-hint = Diegiama Vmux { $version }…
settings-update-ready = Atnaujinimas paruoštas
settings-update-ready-hint = Vmux { $version } paruoštas. Paleiskite iš naujo, kad pritaikytumėte.
settings-update-try-again = Bandyk dar kartą
settings-update-failed = Nepavyko patikrinti, ar nėra naujinimų.
settings-item = Prekė
settings-item-number = Prekė { $number }
settings-press-key = Paspauskite klavišą…
settings-saved = Išsaugota
settings-record-key = Spustelėkite norėdami įrašyti naują klavišų kombinaciją

tray-open-window = Atidaryti langą
tray-close-window = Uždaryti langą
tray-pause-recording = Pristabdyti įrašymą
tray-resume-recording = Tęsti įrašymą
tray-finish-recording = Baigti įrašymą
tray-quit = Išeiti iš Vmux

composer-attach-files = Pridėti failus (/upload)
composer-remove-attachment = Pašalinti priedą

layout-back = Atgal
layout-forward = Pirmyn
layout-reload = Įkelti iš naujo
layout-bookmark-page = Pažymėkite šį puslapį
layout-remove-bookmark = Pašalinti žymę
layout-pin-page = Prisekite šį puslapį
layout-unpin-page = Atsegti šį puslapį
layout-manage-extensions = Tvarkyti plėtinius
layout-new-stack = Naujas kaminas
layout-close-tab = Uždaryti skirtuką
layout-bookmark = Žymė
layout-pin = Smeigtukas
layout-new-tab = Naujas skirtukas
layout-team = Komanda

command-switch-space = Perjungti erdvę…
command-search-ask = Ieškok arba klausk…
command-new-tab-placeholder = Ieškokite arba įveskite URL arba pasirinkite terminalą…
command-placeholder = Įveskite URL, ieškokite skirtukų arba > komandų...
command-composer-placeholder = Įveskite / komandoms arba @, jei norite medijos
command-send = Siųsti (Enter)
command-terminal = Terminalas
command-open-terminal = Atidaryti terminale
command-stack = Stack
command-tabs = { $count ->
    [one] 1 skirtukas
   *[other] { $count } skirtukai
}
command-prompt = Raginimas
command-new-tab = Naujas skirtukas
command-search = Ieškoti
command-open-value = Atidaryti „{ $value }“
command-search-value = Ieškoti „{ $value }“

schema-appearance = Išvaizda
schema-general = Generolas
schema-layout = Išdėstymas
schema-layout-detail = Langas, stiklai, šoninė juosta ir fokusavimo žiedas.
schema-agent = Agentas
schema-agent-detail = Agento elgsena ir įrankių leidimai.
schema-shortcuts = Spartieji klavišai
schema-shortcuts-detail = Tik skaitomas vaizdas. Redaguokite settings.ron tiesiogiai, kad pakeistumėte surišimus.
schema-terminal = Terminalas
schema-browser = Naršyklė
schema-mode = Režimas
schema-mode-detail = Tinklalapių spalvų schema. Įrenginys seka jūsų sistemą.
schema-device = Įrenginys
schema-light = Šviesa
schema-dark = Tamsus
schema-language = Kalba
schema-language-detail = Naudokite sistemą, en-US, ja arba bet kurią BCP 47 žymą su atitinkamu ~/.vmux/locales/<tag>.ftl katalogu.
schema-auto-update = Automatinis atnaujinimas
schema-auto-update-detail = Patikrinkite ir įdiekite naujinimus paleidimo metu ir kas valandą.
schema-startup-url = Paleidimas URL
schema-startup-url-detail = Empty atidaro komandų juostos eilutę.
schema-search-engine = Paieškos variklis
schema-search-engine-detail = Naudojamas žiniatinklio paieškoms iš pradžios ir komandų juostos.
schema-window = Langas
schema-pane = Skydas
schema-side-sheet = Šoninis lapas
schema-focus-ring = Fokusavimo žiedas
schema-run-placement = Leisti vykdymo vietos nepaisymą
schema-run-placement-detail = Leiskite agentams pasirinkti vykdymo srities režimą, kryptį ir inkarą.
schema-leader = Lyderis
schema-leader-detail = Akordo nuorodų priešdėlio klavišas.
schema-chord-timeout = Akordo skirtasis laikas
schema-chord-timeout-detail = Milisekundės iki akordo priešdėlio galiojimo pabaigos.
schema-bindings = Apkaustai
schema-confirm-close = Patvirtinkite uždarymą
schema-confirm-close-detail = Paraginti prieš uždarant terminalą su vykdomu procesu.
schema-default-theme = Numatytoji tema
schema-default-theme-detail = Aktyvios temos pavadinimas iš temų sąrašo.
