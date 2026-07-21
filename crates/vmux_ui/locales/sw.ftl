common-open = Fungua
common-close = Funga
common-install = Sakinisha
common-uninstall = Sanidua
common-update = Sasisha
common-retry = Jaribu tena
common-refresh = Onyesha upya
common-remove = Ondoa
common-enable = Washa
common-disable = Zima
common-new = Mpya
common-active = inatumika
common-running = inaendeshwa
common-done = imekamilika
common-failed = Imeshindikana
common-installed = Imesakinishwa
common-items = { $count ->
    [one] kipengee { $count }
   *[other] vipengee { $count }
}
start-title = Anza
start-tagline = Maelekezo moja. Chochote, kimekamilika.

agents-title = Ajenti
agents-search = Tafuta ajenti za ACP na CLI…
agents-empty = Hakuna ajenti zinazolingana
agents-empty-detail = Jaribu jina, runtime, au ACP/CLI.
agents-install-failed = Usakinishaji umeshindikana
agents-updating = Inasasishwa…
agents-retrying = Inajaribu tena…
agents-preparing = Inaandaa…

extensions-title = Viendelezi
extensions-search = Tafuta vilivyosakinishwa au Chrome Web Store…
extensions-relaunch = Zindua upya ili itumike
extensions-empty = Hakuna viendelezi vilivyosakinishwa
extensions-no-match = Hakuna viendelezi vinavyolingana
extensions-empty-detail = Tafuta Chrome Web Store hapo juu kisha ubonyeze Enter.
extensions-no-match-detail = Jaribu jina lingine au kitambulisho cha kiendelezi.
extensions-on = Kimewashwa
extensions-off = Kimezimwa
extensions-enable-confirm = Uwashe { $name }?
extensions-enable-permissions = Washa { $name } na uruhusu:

lsp-title = Seva za Lugha
lsp-search = Tafuta seva za lugha, linter, formatter…
lsp-loading = Inapakia katalogi…
lsp-empty = Hakuna seva za lugha zinazolingana
lsp-empty-detail = Jaribu lugha nyingine, linter, au formatter.
lsp-needs = inahitaji { $tool }
lsp-status-available = Inapatikana
lsp-status-on-path = Iko kwenye PATH
lsp-status-installing = Inasakinishwa…
lsp-status-installed = Imesakinishwa
lsp-status-outdated = Sasisho linapatikana
lsp-status-running = Inaendeshwa
lsp-status-failed = Imeshindikana

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
team-just-you = Ni wewe tu katika nafasi hii
team-agents = { $count ->
    [one] Wewe na ajenti 1
   *[other] Wewe na ajenti { $count }
}
team-empty = Bado hakuna mtu hapa
team-you = Wewe
team-agent = Ajenti

services-title = Huduma za Mandharinyuma
services-processes = { $count ->
    [one] mchakato 1
   *[other] michakato { $count }
}
services-kill-all = Lazimisha zote zisimame
services-not-running = Huduma haiendeshwi
services-start-with = Anza kwa:
services-empty = Hakuna michakato inayotumika
services-filter = Chuja michakato…
services-no-match = Hakuna michakato inayolingana
services-connected = Imeunganishwa
services-disconnected = Imetenganishwa
services-attached = imeambatishwa
services-kill = Lazimisha kusimama
services-memory = Kumbukumbu
services-size = Ukubwa
services-shell = Shell

error-title = Hitilafu

history-search = Tafuta kwenye historia
history-clear-all = Futa yote
history-clear-confirm = Ufute historia yote?
history-clear-warning = Hili haliwezi kutenduliwa.
history-cancel = Ghairi
history-today = Leo
history-yesterday = Jana
history-days-ago = siku { $count } zilizopita
history-day-offset = Siku -{ $count }

settings-title = Mipangilio
settings-loading = Inapakia mipangilio…
settings-stored = Imehifadhiwa katika ~/.vmux/settings.ron
settings-other = Nyingine
settings-software-update = Sasisho la programu
settings-check-updates = Kagua masasisho
settings-check-updates-hint = Hukagua kiotomatiki inapozinduliwa na kila saa wakati Usasishaji otomatiki umewashwa.
settings-update-unavailable = Haipatikani
settings-update-unavailable-hint = Kisasishaji hakijajumuishwa katika toleo hili.
settings-update-checking = Inakagua…
settings-update-checking-hint = Inakagua masasisho…
settings-update-check-again = Kagua tena
settings-update-current = Vmux imesasishwa kikamilifu.
settings-update-downloading = Inapakua…
settings-update-downloading-hint = Inapakua Vmux { $version }…
settings-update-installing = Inasakinisha…
settings-update-installing-hint = Inasakinisha Vmux { $version }…
settings-update-ready = Sasisho liko tayari
settings-update-ready-hint = Vmux { $version } iko tayari. Anzisha upya ili itumike.
settings-update-try-again = Jaribu tena
settings-update-failed = Imeshindikana kukagua masasisho.
settings-item = Kipengee
settings-item-number = Kipengee { $number }
settings-press-key = Bonyeza kitufe…
settings-saved = Imehifadhiwa
settings-record-key = Bofya kurekodi mchanganyiko mpya wa vitufe

tray-open-window = Fungua dirisha
tray-close-window = Funga dirisha
tray-pause-recording = Sitisha kurekodi
tray-resume-recording = Endelea kurekodi
tray-finish-recording = Maliza kurekodi
tray-quit = Ondoka Vmux

composer-attach-files = Ambatisha faili (/upload)
composer-remove-attachment = Ondoa kiambatisho

layout-back = Rudi
layout-forward = Mbele
layout-reload = Pakia upya
layout-bookmark-page = Weka alamisho ukurasa huu
layout-remove-bookmark = Ondoa alamisho
layout-pin-page = Bandika ukurasa huu
layout-unpin-page = Bandua ukurasa huu
layout-manage-extensions = Dhibiti viendelezi
layout-new-stack = Tabaka jipya
layout-close-tab = Funga kichupo
layout-bookmark = Alamisho
layout-pin = Bandika
layout-new-tab = Kichupo kipya
layout-team = Timu

command-switch-space = Badilisha nafasi…
command-search-ask = Tafuta au uliza…
command-new-tab-placeholder = Tafuta au andika URL, au chagua Terminal…
command-placeholder = Andika URL, tafuta vichupo, au > kwa amri…
command-composer-placeholder = Andika / kwa amri au @ kwa midia
command-send = Tuma (Enter)
command-terminal = Terminal
command-open-terminal = Fungua kwenye Terminal
command-stack = Tabaka
command-tabs = { $count ->
    [one] kichupo 1
   *[other] vichupo { $count }
}
command-prompt = Maelekezo
command-new-tab = Kichupo kipya
command-search = Tafuta
command-open-value = Fungua “{ $value }”
command-search-value = Tafuta “{ $value }”

schema-appearance = Mwonekano
schema-general = Jumla
schema-layout = Mpangilio
schema-layout-detail = Dirisha, vidirisha, upau wa pembeni, na pete ya fokasi.
schema-agent = Ajenti
schema-agent-detail = Tabia ya ajenti na ruhusa za zana.
schema-shortcuts = Njia za mkato
schema-shortcuts-detail = Mwonekano wa kusoma pekee. Hariri settings.ron moja kwa moja kubadilisha vitufe.
schema-terminal = Terminal
schema-browser = Kivinjari
schema-mode = Hali
schema-mode-detail = Mpangilio wa rangi wa kurasa za wavuti. Kifaa hufuata mfumo wako.
schema-device = Kifaa
schema-light = Angavu
schema-dark = Giza
schema-language = Lugha
schema-language-detail = Tumia lugha ya mfumo, en-US, ja, au tagi yoyote ya BCP 47 yenye katalogi inayolingana ya ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Usasishaji otomatiki
schema-auto-update-detail = Kagua na usakinishe masasisho unapozindua na kila saa.
schema-startup-url = URL ya kuanzia
schema-startup-url-detail = Ikiwa tupu, hufungua sehemu ya maelekezo kwenye upau wa amri.
schema-search-engine = Injini ya utafutaji
schema-search-engine-detail = Hutumika kwa utafutaji wa wavuti kutoka Anza na upau wa amri.
schema-window = Dirisha
schema-pane = Kidirisha
schema-side-sheet = Laha ya pembeni
schema-focus-ring = Pete ya fokasi
schema-run-placement = Ruhusu kubadilisha mahali pa kuendesha
schema-run-placement-detail = Waruhusu ajenti wachague hali ya kidirisha cha kuendesha, mwelekeo, na nanga.
schema-leader = Kitufe kiongozi
schema-leader-detail = Kitufe cha kuanzia kwa njia za mkato za chord.
schema-chord-timeout = Muda wa chord kuisha
schema-chord-timeout-detail = Milisekunde kabla ya kiambishi cha chord kuisha.
schema-bindings = Viungo vya vitufe
schema-confirm-close = Thibitisha kufunga
schema-confirm-close-detail = Uliza kabla ya kufunga terminal yenye mchakato unaoendelea.
schema-default-theme = Mandhari chaguomsingi
schema-default-theme-detail = Jina la mandhari inayotumika kutoka kwenye orodha ya mandhari.
