locale-name = Ikinyarwanda
common-open = Fungura
common-close = Funga
common-install = Shyiramo
common-uninstall = Kuramo
common-update = Vugurura
common-retry = Ongera ugerageze
common-refresh = Ongera uvugurure
common-remove = Vanaho
common-enable = Koresha
common-disable = Hagarika
common-new = Gishya
common-active = irakora
common-running = irimo gukora
common-done = birangiye
common-failed = Byanze
common-installed = Byashyizwemo
common-items = { $count ->
    [one] ikintu { $count }
   *[other] ibintu { $count }
}
start-title = Tangira
start-tagline = Prompt imwe. Ibyo ushaka, birakorwa.

agents-title = Abajenti
agents-search = Shakisha abajenti ba ACP na CLI…
agents-empty = Nta bajenti bihuye
agents-empty-detail = Gerageza izina, runtime, cyangwa ACP/CLI.
agents-install-failed = Gushyiramo byanze
agents-updating = Biravugururwa…
agents-retrying = Birongera kugeragezwa…
agents-preparing = Birategurwa…

extensions-title = Imigereka
extensions-search = Shakisha iyashyizwemo cyangwa Chrome Web Store…
extensions-relaunch = Ongera utangize ngo bikurikizwe
extensions-empty = Nta migereka yashyizwemo
extensions-no-match = Nta migereka ihuye
extensions-empty-detail = Shakisha muri Chrome Web Store hejuru, hanyuma ukande Enter.
extensions-no-match-detail = Gerageza irindi zina cyangwa ID y’umugereka.
extensions-on = Ifunguye
extensions-off = Ifunze
extensions-enable-confirm = Gukoresha { $name }?
extensions-enable-permissions = Koresha { $name } kandi wemere:

lsp-title = Seriveri z’indimi
lsp-search = Shakisha seriveri z’indimi, linters, formatters…
lsp-loading = Katalo irimo gufungurwa…
lsp-empty = Nta seriveri z’indimi zihuye
lsp-empty-detail = Gerageza urundi rurimi, linter, cyangwa formatter.
lsp-needs = ikeneye { $tool }
lsp-status-available = Iraboneka
lsp-status-on-path = Iri kuri PATH
lsp-status-installing = Irimo gushyirwamo…
lsp-status-installed = Yashyizwemo
lsp-status-outdated = Ivugurura riraboneka
lsp-status-running = Irimo gukora
lsp-status-failed = Byanze

spaces-title = Imyanya
spaces-new-placeholder = Izina ry’umwanya mushya
spaces-empty = Nta myanya
spaces-default-name = Umwanya { $number }
spaces-tabs = { $count ->
    [one] tabu 1
   *[other] tabu { $count }
}
spaces-delete = Siba umwanya

team-title = Ikipe
team-just-you = Ni wowe wenyine muri uyu mwanya
team-agents = { $count ->
    [one] Wowe n’ajenti 1
   *[other] Wowe n’abajenti { $count }
}
team-empty = Nta wundi urahagera
team-you = Wowe
team-agent = Ajenti

services-title = Serivisi z’inyuma
services-processes = { $count ->
    [one] porosesi 1
   *[other] porosesi { $count }
}
services-kill-all = Hagarika zose ku ngufu
services-not-running = Serivisi ntiri gukora
services-start-with = Tangiza ukoresheje:
services-empty = Nta porosesi zikora
services-filter = Shungura porosesi…
services-no-match = Nta porosesi zihuye
services-connected = Byahujwe
services-disconnected = Byatandukanye
services-attached = yometse
services-kill = Hagarika ku ngufu
services-memory = Ububiko
services-size = Ingano
services-shell = Shell

error-title = Ikosa

history-search = Shakisha mu mateka
history-clear-all = Siba byose
history-clear-confirm = Gusiba amateka yose?
history-clear-warning = Ntibishobora gusubizwa inyuma.
history-cancel = Hagarika
history-today = Uyu munsi
history-yesterday = Ejo hashize
history-days-ago = hashize iminsi { $count }
history-day-offset = Umunsi -{ $count }

settings-title = Igenamiterere
settings-loading = Igenamiterere ririmo gufungurwa…
settings-stored = Bibitswe muri ~/.vmux/settings.ron
settings-other = Ibindi
settings-software-update = Ivugurura rya porogaramu
settings-check-updates = Reba ivugurura
settings-check-updates-hint = Igenzura mu kwitangiza no buri saha iyo Auto-update ikora.
settings-update-unavailable = Ntibiboneka
settings-update-unavailable-hint = Uvugurura ntarimo muri iyi build.
settings-update-checking = Biragenzurwa…
settings-update-checking-hint = Harimo kugenzurwa ivugurura…
settings-update-check-again = Ongera ugenzure
settings-update-current = Vmux iri ku gihe.
settings-update-downloading = Birakururwa…
settings-update-downloading-hint = Harakururwa Vmux { $version }…
settings-update-installing = Birashyirwamo…
settings-update-installing-hint = Harashyirwamo Vmux { $version }…
settings-update-ready = Ivugurura ryiteguye
settings-update-ready-hint = Vmux { $version } yiteguye. Ongera utangize ngo rikurikizwe.
settings-update-try-again = Ongera ugerageze
settings-update-failed = Ntibyashobotse kugenzura ivugurura.
settings-item = Ikintu
settings-item-number = Ikintu { $number }
settings-press-key = Kanda urufunguzo…
settings-saved = Byabitswe
settings-record-key = Kanda hano wandike ikomatanya rishya ry’imfunguzo

tray-open-window = Fungura idirishya
tray-close-window = Funga idirishya
tray-pause-recording = Hagarika gufata by’agateganyo
tray-resume-recording = Komeza gufata
tray-finish-recording = Rangiza gufata
tray-quit = Sohoka muri Vmux

composer-attach-files = Ongeraho dosiye (/upload)
composer-remove-attachment = Vanaho umugereka

layout-back = Subira inyuma
layout-forward = Jya imbere
layout-reload = Ongera ufungure
layout-bookmark-page = Bika uru rupapuro
layout-remove-bookmark = Vanaho ikimenyetso
layout-pin-page = Pinning’a uru rupapuro
layout-unpin-page = Kuraho pin y’uru rupapuro
layout-manage-extensions = Gucunga imigereka
layout-new-stack = Stack nshya
layout-close-tab = Funga tabu
layout-bookmark = Ikimenyetso
layout-pin = Pinning’a
layout-new-tab = Tabu nshya
layout-team = Ikipe

command-switch-space = Hindura umwanya…
command-search-ask = Shakisha cyangwa ubaze…
command-new-tab-placeholder = Shakisha cyangwa wandike URL, cyangwa uhitemo Terminal…
command-placeholder = Andika URL, shakisha tabu, cyangwa > ku mategeko…
command-composer-placeholder = Andika / ku mategeko cyangwa @ ku bitangazamakuru
command-send = Ohereza (Enter)
command-terminal = Terminal
command-open-terminal = Fungurira muri Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] tabu 1
   *[other] tabu { $count }
}
command-prompt = Prompt
command-new-tab = Tabu nshya
command-search = Shakisha
command-open-value = Fungura “{ $value }”
command-search-value = Shakisha “{ $value }”

schema-appearance = Imigaragarire
schema-general = Rusange
schema-layout = Imiterere
schema-layout-detail = Idirishya, ibice, akabari ko ku ruhande, n’uruziga rwo kwibanda.
schema-agent = Ajenti
schema-agent-detail = Imyitwarire y’ajenti n’impushya z’ibikoresho.
schema-shortcuts = Amagufi
schema-shortcuts-detail = Kureba gusa. Hindura settings.ron ako kanya kugira ngo uhindure bindings.
schema-terminal = Terminal
schema-browser = Mucukumbuzi
schema-mode = Uburyo
schema-mode-detail = Ibara ry’amapaji y’urubuga. Device ikurikiza sisitemu yawe.
schema-device = Device
schema-light = Urumuri
schema-dark = Umwijima
schema-language = Ururimi
schema-language-detail = Koresha sisitemu, en-US, ja, cyangwa tag iyo ari yo yose ya BCP 47 ifite katalo ya ~/.vmux/locales/<tag>.ftl bihuye.
schema-auto-update = Auto-update
schema-auto-update-detail = Genzura kandi ushyiremo ivugurura mu kwitangiza no buri saha.
schema-startup-url = URL yo gutangira
schema-startup-url-detail = Iyo irimo ubusa, ifungura prompt y’akabari k’amategeko.
schema-search-engine = Moteri y’ishakisha
schema-search-engine-detail = Ikoreshwa mu gushakisha ku rubuga uhereye kuri Tangira no ku kabari k’amategeko.
schema-window = Idirishya
schema-pane = Igice
schema-side-sheet = Urupapuro rwo ku ruhande
schema-focus-ring = Uruziga rwo kwibanda
schema-run-placement = Emera guhindura aho run ishyirwa
schema-run-placement-detail = Reka abajenti bahitemo uburyo bw’igice cya run, icyerekezo, n’aho bishingira.
schema-leader = Leader
schema-leader-detail = Urufunguzo rubanziriza amagufi ya chord.
schema-chord-timeout = Igihe chord imara
schema-chord-timeout-detail = Milisekonda mbere y’uko prefix ya chord irangira.
schema-bindings = Bindings
schema-confirm-close = Emeza gufunga
schema-confirm-close-detail = Baza mbere yo gufunga terminal ifite porosesi ikiri gukora.
schema-default-theme = Insanganyamatsiko isanzwe
schema-default-theme-detail = Izina ry’insanganyamatsiko ikora iri ku rutonde rw’insanganyamatsiko.
