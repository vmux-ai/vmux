common-open = Fungura
common-close = Funga
common-install = Shyiraho
common-uninstall = Kuraho
common-update = Vugurura
common-retry = Ongera ugerageze
common-refresh = Subiramo
common-remove = Kura
common-enable = Injiza
common-disable = Hagarika
common-new = Nshya
common-active = ikora
common-running = irimo gukorwa
common-done = byarangiye
common-failed = Byanze
common-installed = Yashyizweho
common-items = { $count ->
    [one] { $count } kintu
   *[other] { $count } ibintu
}
start-title = Tangira
start-tagline = Ikibazo kimwe. Ikintu cyose, cyakozwe.

agents-title = Abakozi
agents-search = Shakisha ACP na CLI abakozi…
agents-empty = Nta mukozi uhuye
agents-empty-detail = Gerageza izina, runtime, cyangwa ACP/CLI.
agents-install-failed = Gushyiraho byanze
agents-updating = Birimo kuvugururwa…
agents-retrying = Birimo kongera gerageza…
agents-preparing = Bitegurwa…

extensions-title = Ibongerwa
extensions-search = Shakisha ibyashyizweho cyangwa Chrome Web Store…
extensions-relaunch = Tangira nanone kugirango bikorwe
extensions-empty = Nta bongerwa bushyizweho
extensions-no-match = Nta bongerwa buhuye
extensions-empty-detail = Shakisha Chrome Web Store hejuru hanyuma usinde Return.
extensions-no-match-detail = Gerageza izina rindi cyangwa ID y'ibongerwa.
extensions-on = Injijwe
extensions-off = Hagaritswe
extensions-enable-confirm = Injiza { $name }?
extensions-enable-permissions = Injiza { $name } hanyuma uturutse:

lsp-title = Seriveri z'Ururimi
lsp-search = Shakisha seriveri z'ururimi, linters, formatters…
lsp-loading = Gutegura catalogue…
lsp-empty = Nta seriveri y'ururimi ihuye
lsp-empty-detail = Gerageza ururimi rundi, linter, cyangwa formatter.
lsp-needs = bisaba { $tool }
lsp-status-available = Iboneka
lsp-status-on-path = Kuri PATH
lsp-status-installing = Birimo gushyirwa…
lsp-status-installed = Yashyizweho
lsp-status-outdated = Vugurura iboneka
lsp-status-running = Irimo gukorwa
lsp-status-failed = Byanze

spaces-title = Ibibanza
spaces-new-placeholder = Izina ry'ikibianza gishya
spaces-empty = Nta bibianza
spaces-default-name = Ikibianza { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tabs
}
spaces-delete = Siba ikibianza

team-title = Itsinda
team-just-you = Wewe wenyine muri iki kibianza
team-agents = { $count ->
    [one] Wewe n'umukozi 1
   *[other] Wewe n'abakozi { $count }
}
team-empty = Nta muntu hano ubu
team-you = Wewe
team-agent = Umukozi

services-title = Serivisi z'Inyuma
services-processes = { $count ->
    [one] igikorwa 1
   *[other] ibikorwa { $count }
}
services-kill-all = Hagarika Byose
services-not-running = Serivisi ntikora
services-start-with = Tangira hamwe na:
services-empty = Nta bikorwa bikora
services-filter = Shungura ibikorwa…
services-no-match = Nta bikorwa bihuye
services-connected = Yundaganye
services-disconnected = Yatandukanye
services-attached = yashyizweho
services-kill = Hagarika
services-memory = Ububiko
services-size = Ingano
services-shell = Shell

error-title = Ikosa

history-search = Shakisha amateka
history-clear-all = Siba byose
history-clear-confirm = Siba amateka yose?
history-clear-warning = Ibi ntibishobora gusubizwaho.
history-cancel = Reka
history-today = Uyu munsi
history-yesterday = Ejo hashize
history-days-ago = Hashize { $count } iminsi
history-day-offset = Umunsi -{ $count }

settings-title = Igenamiterere
settings-loading = Gutegura igenamiterere…
settings-stored = Bitswe muri ~/.vmux/settings.ron
settings-other = Ibindi
settings-software-update = Kuvugurura Porogaramu
settings-check-updates = Reba Ivugururwa
settings-check-updates-hint = Ireba bwite iyo tangiye kandi buri saa imwe iyo Auto-update yinjijwe.
settings-update-unavailable = Ntiboneka
settings-update-unavailable-hint = Umuvugururwa ntibashyizwe muri iki gishushanyo.
settings-update-checking = Birimo kureba…
settings-update-checking-hint = Birimo kureba ivugururwa…
settings-update-check-again = Reba Nanone
settings-update-current = Vmux iri ku kigero cy'ubu.
settings-update-downloading = Birimo gukurura…
settings-update-downloading-hint = Birimo gukurura Vmux { $version }…
settings-update-installing = Birimo gushyirwa…
settings-update-installing-hint = Birimo gushyira Vmux { $version }…
settings-update-ready = Vugurura Yiteguye
settings-update-ready-hint = Vmux { $version } yiteguye. Tangira nanone kuyishyira mu bikorwa.
settings-update-try-again = Ongera Ugerageze
settings-update-failed = Ntabwo bishobora kureba ivugururwa.
settings-item = Ikintu
settings-item-number = Ikintu { $number }
settings-press-key = Kanda urufunguzo…
settings-saved = Byabitswe
settings-record-key = Kanda kugirango wandike igice gishya cy'urufunguzo

tray-open-window = Fungura Idirishya
tray-close-window = Funga Idirishya
tray-pause-recording = Hagarika Kwandika
tray-resume-recording = Subiramo Kwandika
tray-finish-recording = Rangiza Kwandika
tray-quit = Sohoka kuri Vmux

composer-attach-files = Shyiraho dosiye (/upload)
composer-remove-attachment = Kura ikimanurwa

layout-back = Subira inyuma
layout-forward = Subira imbere
layout-reload = Subiramo
layout-bookmark-page = Shyira urupapuro mu makipe
layout-remove-bookmark = Kura ikipe
layout-pin-page = Shyira urupapuro mu kibanza
layout-unpin-page = Kura urupapuro mu kibanza
layout-manage-extensions = Gucunga ibongerwa
layout-new-stack = Indetso Nshya
layout-close-tab = Funga tab
layout-bookmark = Ikipe
layout-pin = Shyira
layout-new-tab = Tab nshya
layout-team = Itsinda

command-switch-space = Hindura ikibianza…
command-search-ask = Shakisha cyangwa ubaze…
command-new-tab-placeholder = Shakisha cyangwa andika URL, cyangwa hitamo Terminal…
command-placeholder = Andika URL, shakisha amakawa, cyangwa > ku mabwiriza…
command-composer-placeholder = Andika / ku mabwiriza cyangwa @ ku makurushwa
command-send = Ohereza (Enter)
command-terminal = Terminal
command-open-terminal = Fungura muri Terminal
command-stack = Indetso
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tabs
}
command-prompt = Ikibazo
command-new-tab = Tab nshya
command-search = Shakisha
command-open-value = Fungura "{ $value }"
command-search-value = Shakisha "{ $value }"

schema-appearance = Imiterere
schema-general = Rusange
schema-layout = Igenamiterere
schema-layout-detail = Idirishya, ibice, inzira y'uruhande, no guhuza.
schema-agent = Umukozi
schema-agent-detail = Imyitwarire y'umukozi n'uburenganzira bw'ibikoresho.
schema-shortcuts = Inzira Ngufi
schema-shortcuts-detail = Kureba gusa. Hindura settings.ron kugirango uhindure imirongo.
schema-terminal = Terminal
schema-browser = Guca kuri interineti
schema-mode = Uburyo
schema-mode-detail = Ibara ry'urupapuro rwa interineti. Igikoresho gikurikira sisitemu yawe.
schema-device = Igikoresho
schema-light = Urumuri
schema-dark = Umwiza
schema-language = Ururumi
schema-language-detail = Koresha sisitemu, en-US, ja, cyangwa tag iyo ari yo yose ya BCP 47 ifite ~/.vmux/locales/<tag>.ftl catalogue ihuye.
schema-auto-update = Kuvugurura bwite
schema-auto-update-detail = Reba no gushyira ivugururwa iyo tangiye kandi buri saa imwe.
schema-startup-url = URL yo Gutangira
schema-startup-url-detail = Ubusa bifungura ikibazo cy'inzira y'amabwiriza.
schema-search-engine = Injini yo gushakisha
schema-search-engine-detail = Ikoreshwa mu gushakisha kuri interineti uhereye ku Tangira no ku nzira y'amabwiriza.
schema-window = Idirishya
schema-pane = Igice
schema-side-sheet = Urupapuro rw'uruhande
schema-focus-ring = Uruziga rwo guhuza
schema-run-placement = Reka gushyira mu bikorwa bivugwa
schema-run-placement-detail = Reka abakozi bashyire uburyo bw'inzira yo gukoramo, inshinga, n'incike.
schema-leader = Umuyobozi
schema-leader-detail = Urufunguzo rw'inzira ku nzira ngufi z'indirimbo.
schema-chord-timeout = Igihe cy'indirimbo
schema-chord-timeout-detail = Milliseconds mbere y'aho inzira y'indirimbo irangirira.
schema-bindings = Imirongo
schema-confirm-close = Emeza gufunga
schema-confirm-close-detail = Baza mbere yo gufunga terminal ifite igikorwa gikora.
schema-default-theme = Imbonerahamwe Isanzwe
schema-default-theme-detail = Izina ry'imbonerahamwe ikora muri lisiti y'imbonerahamwe.
