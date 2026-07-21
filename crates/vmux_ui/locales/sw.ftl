common-open = Fungua
common-close = Funga
common-install = Sakinisha
common-uninstall = Sakinusha
common-update = Sasisha
common-retry = Jaribu tena
common-refresh = Onyesha upya
common-remove = Ondoa
common-enable = Wezesha
common-disable = Zima
common-new = Mpya
common-active = inayofanya kazi
common-running = inayoendelea
common-done = imekamilika
common-failed = Imeshindwa
common-installed = Imesakinishwa
common-items = { $count ->
    [one] { $count } kipengele
   *[other] { $count } vipengele
}
start-title = Anza
start-tagline = Ombi moja. Chochote, kimekamilika.

agents-title = Mawakala
agents-search = Tafuta mawakala ya ACP na CLI…
agents-empty = Hakuna mawakala yanayolingana
agents-empty-detail = Jaribu jina, muda wa utekelezaji, au ACP/CLI.
agents-install-failed = Usakinishaji umeshindwa
agents-updating = Inasasisha…
agents-retrying = Inajaribu tena…
agents-preparing = Inaandaa…

extensions-title = Nyongeza
extensions-search = Tafuta zilizosakinishwa au Chrome Web Store…
extensions-relaunch = Zindua upya ili kutumia
extensions-empty = Hakuna nyongeza zilizosakinishwa
extensions-no-match = Hakuna nyongeza zinazofanana
extensions-empty-detail = Tafuta Chrome Web Store hapo juu na bonyeza Return.
extensions-no-match-detail = Jaribu jina lingine au kitambulisho cha nyongeza.
extensions-on = Imewashwa
extensions-off = Imezimwa
extensions-enable-confirm = Wezesha { $name }?
extensions-enable-permissions = Wezesha { $name } na ruhusu:

lsp-title = Seva za Lugha
lsp-search = Tafuta seva za lugha, vikaguzi, visanidi…
lsp-loading = Inapakia katalogi…
lsp-empty = Hakuna seva za lugha zinazofanana
lsp-empty-detail = Jaribu lugha, kikaguzi, au kisanidi kingine.
lsp-needs = inahitaji { $tool }
lsp-status-available = Inapatikana
lsp-status-on-path = Iko kwenye PATH
lsp-status-installing = Inasakinisha…
lsp-status-installed = Imesakinishwa
lsp-status-outdated = Sasisha inapatikana
lsp-status-running = Inafanya kazi
lsp-status-failed = Imeshindwa

spaces-title = Nafasi
spaces-new-placeholder = Jina la nafasi mpya
spaces-empty = Hakuna nafasi
spaces-default-name = Nafasi { $number }
spaces-tabs = { $count ->
    [one] kichupo 1
   *[other] vichupo { $count }
}
spaces-delete = Futa nafasi

team-title = Timu
team-just-you = Wewe tu katika nafasi hii
team-agents = { $count ->
    [one] Wewe na wakala 1
   *[other] Wewe na mawakala { $count }
}
team-empty = Bado hakuna mtu hapa
team-you = Wewe
team-agent = Wakala

services-title = Huduma za Nyuma
services-processes = { $count ->
    [one] mchakato 1
   *[other] michakato { $count }
}
services-kill-all = Komesha Zote
services-not-running = Huduma haifanyi kazi
services-start-with = Anza na:
services-empty = Hakuna michakato inayofanya kazi
services-filter = Chuja michakato…
services-no-match = Hakuna michakato inayofanana
services-connected = Imeunganishwa
services-disconnected = Imekatwa
services-attached = imeambatanishwa
services-kill = Komesha
services-memory = Kumbukumbu
services-size = Ukubwa
services-shell = Shell

error-title = Hitilafu

history-search = Tafuta historia
history-clear-all = Futa yote
history-clear-confirm = Futa historia yote?
history-clear-warning = Hii haiwezi kutenduliwa.
history-cancel = Ghairi
history-today = Leo
history-yesterday = Jana
history-days-ago = Siku { $count } zilizopita
history-day-offset = Siku -{ $count }

settings-title = Mipangilio
settings-loading = Inapakia mipangilio…
settings-stored = Imehifadhiwa katika ~/.vmux/settings.ron
settings-other = Nyingine
settings-software-update = Sasisho la Programu
settings-check-updates = Angalia Masasisho
settings-check-updates-hint = Inakagua kiotomatiki wakati wa uzinduzi na kila saa wakati Sasisha-kiotomatiki imewezeshwa.
settings-update-unavailable = Haipatikani
settings-update-unavailable-hint = Kisasishaji hakijajumuishwa katika toleo hili.
settings-update-checking = Inakagua…
settings-update-checking-hint = Inakagua masasisho…
settings-update-check-again = Kagua Tena
settings-update-current = Vmux iko katika toleo jipya zaidi.
settings-update-downloading = Inapakua…
settings-update-downloading-hint = Inapakua Vmux { $version }…
settings-update-installing = Inasakinisha…
settings-update-installing-hint = Inasakinisha Vmux { $version }…
settings-update-ready = Sasisho Liko Tayari
settings-update-ready-hint = Vmux { $version } iko tayari. Anzisha upya ili kutumia.
settings-update-try-again = Jaribu Tena
settings-update-failed = Imeshindwa kukagua masasisho.
settings-item = Kipengele
settings-item-number = Kipengele { $number }
settings-press-key = Bonyeza kitufe…
settings-saved = Imehifadhiwa
settings-record-key = Bonyeza kurekodi mchanganyiko mpya wa vitufe

tray-open-window = Fungua Dirisha
tray-close-window = Funga Dirisha
tray-pause-recording = Simamisha Urekodi
tray-resume-recording = Endelea na Urekodi
tray-finish-recording = Maliza Urekodi
tray-quit = Toka Vmux

composer-attach-files = Ambatanisha faili (/upload)
composer-remove-attachment = Ondoa kiambatisho

layout-back = Nyuma
layout-forward = Mbele
layout-reload = Pakia upya
layout-bookmark-page = Weka alama ukurasa huu
layout-remove-bookmark = Ondoa alama
layout-pin-page = Piga pini ukurasa huu
layout-unpin-page = Ondoa pini ukurasa huu
layout-manage-extensions = Simamia nyongeza
layout-new-stack = Stack Mpya
layout-close-tab = Funga kichupo
layout-bookmark = Alama
layout-pin = Pini
layout-new-tab = Kichupo kipya
layout-team = Timu

command-switch-space = Badili nafasi…
command-search-ask = Tafuta au uliza…
command-new-tab-placeholder = Tafuta au andika URL, au chagua Terminal…
command-placeholder = Andika URL, tafuta vichupo, au > kwa amri…
command-composer-placeholder = Andika / kwa amri au @ kwa media
command-send = Tuma (Enter)
command-terminal = Terminal
command-open-terminal = Fungua katika Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] kichupo 1
   *[other] vichupo { $count }
}
command-prompt = Ombi
command-new-tab = Kichupo kipya
command-search = Tafuta
command-open-value = Fungua "{ $value }"
command-search-value = Tafuta "{ $value }"

schema-appearance = Mwonekano
schema-general = Jumla
schema-layout = Mpangilio
schema-layout-detail = Dirisha, sehemu, upau wa pembeni, na pete ya makini.
schema-agent = Wakala
schema-agent-detail = Tabia ya wakala na ruhusa za zana.
schema-shortcuts = Njia za mkato
schema-shortcuts-detail = Muundo wa kusoma tu. Hariri settings.ron moja kwa moja kubadilisha vifupisho.
schema-terminal = Terminal
schema-browser = Kivinjari
schema-mode = Hali
schema-mode-detail = Mpango wa rangi kwa kurasa za wavuti. Kifaa hufuata mfumo wako.
schema-device = Kifaa
schema-light = Mwanga
schema-dark = Giza
schema-language = Lugha
schema-language-detail = Tumia mfumo, en-US, ja, au lolote la BCP 47 lenye katalogi inayolingana ya ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Sasisha-kiotomatiki
schema-auto-update-detail = Kagua na sakinisha masasisho wakati wa uzinduzi na kila saa.
schema-startup-url = URL ya Uzinduzi
schema-startup-url-detail = Tupu inafungua kisanduku cha amri.
schema-search-engine = Injini ya utafutaji
schema-search-engine-detail = Inatumika kwa utafutaji wa wavuti kutoka Anza na upau wa amri.
schema-window = Dirisha
schema-pane = Sehemu
schema-side-sheet = Karatasi ya pembeni
schema-focus-ring = Pete ya makini
schema-run-placement = Ruhusu uamuzi wa uwekaji wa utekelezaji
schema-run-placement-detail = Ruhusu mawakala kuchagua hali ya sehemu ya utekelezaji, mwelekeo, na nanga.
schema-leader = Kiongozi
schema-leader-detail = Kitufe cha awali kwa njia za mkato za chord.
schema-chord-timeout = Muda wa chord kumalizika
schema-chord-timeout-detail = Milisekunde kabla kitufe cha awali cha chord kumalizika.
schema-bindings = Vifupisho
schema-confirm-close = Thibitisha kufunga
schema-confirm-close-detail = Uliza kabla ya kufunga terminal iliyo na mchakato unaoendelea.
schema-default-theme = Mandhari ya msingi
schema-default-theme-detail = Jina la mandhari inayotumika kutoka kwenye orodha ya mandhari.
