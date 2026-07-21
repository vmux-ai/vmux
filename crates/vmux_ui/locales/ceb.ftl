common-open = Bukas
common-close = Duol
common-install = Pag-instalar
common-uninstall = I-uninstall
common-update = Update
common-retry = Sulayi pag-usab
common-refresh = I-refresh
common-remove = Kuhaa
common-enable = Makapahimo
common-disable = Pag-disable
common-new = Bag-o
common-active = aktibo
common-running = nagdagan
common-done = nahuman
common-failed = Napakyas
common-installed = Gi-install
common-items = { $count ->
    [one] { $count } butang
   *[other] { $count } mga butang
}
start-title = Pagsugod
start-tagline = Usa ka prompt. Bisan unsa, nahuman.

agents-title = Mga ahente
agents-search = Pangitaa ang ACP ug CLI nga mga ahente…
agents-empty = Walay katugbang nga ahente
agents-empty-detail = Sulayi ang usa ka ngalan, runtime, o ACP/CLI.
agents-install-failed = Napakyas ang pag-instalar
agents-updating = Gi-update…
agents-retrying = Gisulayan pag-usab…
agents-preparing = Nangandam…

extensions-title = Mga extension
extensions-search = Na-install ang pagpangita o Chrome Web Store…
extensions-relaunch = Ilunsad pag-usab aron magamit
extensions-empty = Walay mga extension nga na-install
extensions-no-match = Walay katugbang nga mga extension
extensions-empty-detail = Pangitaa ang Chrome Web Store sa ibabaw ug pindota ang Return.
extensions-no-match-detail = Sulayi ang laing ngalan o extension ID.
extensions-on = Sa
extensions-off = Off
extensions-enable-confirm = I-enable ang { $name }?
extensions-enable-permissions = I-enable ang { $name } ug tugoti:

lsp-title = Mga Server sa Pinulongan
lsp-search = Pangitaa ang mga server sa pinulongan, mga linter, mga tig-format…
lsp-loading = Nagkarga sa katalogo…
lsp-empty = Walay katugbang nga mga server sa pinulongan
lsp-empty-detail = Sulayi ang laing pinulongan, linter, o formatter.
lsp-needs = kinahanglan { $tool }
lsp-status-available = Anaa
lsp-status-on-path = Sa PATH
lsp-status-installing = Nag-instalar…
lsp-status-installed = Gi-install
lsp-status-outdated = Magamit ang update
lsp-status-running = Nagdagan
lsp-status-failed = Napakyas

spaces-title = Mga wanang
spaces-new-placeholder = Bag-ong ngalan sa wanang
spaces-empty = Walay mga luna
spaces-default-name = Luna { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
spaces-delete = Pagtangtang sa luna

team-title = Team
team-just-you = Ikaw ra dinhi sa wanang
team-agents = { $count ->
    [one] Ikaw ug 1 ka ahente
   *[other] Ikaw ug { $count } mga ahente
}
team-empty = Wala pa dinhi
team-you = Ikaw
team-agent = Ahente

services-title = Mga Serbisyo sa Background
services-processes = { $count ->
    [one] 1 proseso
   *[other] { $count } mga proseso
}
services-kill-all = Patya Tanan
services-not-running = Wala nagdagan ang serbisyo
services-start-with = Magsugod sa:
services-empty = Walay aktibo nga mga proseso
services-filter = Mga proseso sa pagsala…
services-no-match = Walay mga proseso nga magkaparehas
services-connected = Konektado
services-disconnected = Nadiskonekta
services-attached = gilakip
services-kill = Pagpatay
services-memory = Memorya
services-size = Gidak-on
services-shell = Shell

error-title = Sayop

history-search = Kasaysayan sa pagpangita
history-clear-all = Klaro tanan
history-clear-confirm = Hawani ang tanang kasaysayan?
history-clear-warning = Kini dili na mabawi.
history-cancel = Pagkanselar
history-today = Karon
history-yesterday = Kagahapon
history-days-ago = { $count } ka adlaw ang milabay
history-day-offset = Adlaw -{ $count }

settings-title = Mga setting
settings-loading = Nag-load sa mga setting…
settings-stored = Gitipigan sa ~/.vmux/settings.ron
settings-other = Ang uban
settings-software-update = Update sa Software
settings-check-updates = Susiha ang mga Update
settings-check-updates-hint = Awtomatikong nagsusi sa paglansad ug matag oras kung gi-enable ang Auto-update.
settings-update-unavailable = Dili magamit
settings-update-unavailable-hint = Ang Updater wala gilakip sa kini nga pagtukod.
settings-update-checking = Gisusi…
settings-update-checking-hint = Pagsusi alang sa mga update…
settings-update-check-again = Susiha Pag-usab
settings-update-current = Vmux kay pinakabag-o.
settings-update-downloading = Nag-download…
settings-update-downloading-hint = Nag-download sa Vmux { $version }…
settings-update-installing = Nag-instalar…
settings-update-installing-hint = Nag-instalar sa Vmux { $version }…
settings-update-ready = Andam na ang pag-update
settings-update-ready-hint = Vmux { $version } andam na. I-restart aron magamit kini.
settings-update-try-again = Sulayi Pag-usab
settings-update-failed = Dili masusi ang mga update.
settings-item = butang
settings-item-number = Butang { $number }
settings-press-key = Pindota ang usa ka yawe…
settings-saved = Naluwas
settings-record-key = I-klik aron magrekord og bag-ong key combo

tray-open-window = Bukas nga Bintana
tray-close-window = Isira ang Bintana
tray-pause-recording = Ihunong ang Pagrekord
tray-resume-recording = Ipadayon ang Pagrekord
tray-finish-recording = Tapuson ang Pagrekord
tray-quit = Hunong Vmux

composer-attach-files = Ilakip ang mga file (/upload)
composer-remove-attachment = Kuhaa ang attachment

layout-back = Balik
layout-forward = Sa unahan
layout-reload = I-reload
layout-bookmark-page = I-bookmark kini nga panid
layout-remove-bookmark = Kuhaa ang bookmark
layout-pin-page = I-pin kini nga panid
layout-unpin-page = Unpin kini nga panid
layout-manage-extensions = Pagdumala sa mga extension
layout-new-stack = Bag-ong Stack
layout-close-tab = Isira ang tab
layout-bookmark = Bookmark
layout-pin = Pin
layout-new-tab = Bag-ong tab
layout-team = Team

command-switch-space = Pagbalhin og luna…
command-search-ask = Pangitaa o pangutana…
command-new-tab-placeholder = Pangitaa o i-type ang URL, o pilia ang Terminal…
command-placeholder = Pag-type og URL, pangitaa ang mga tab, o > alang sa mga sugo...
command-composer-placeholder = Type / para sa mga command o @ para sa media
command-send = Ipadala (Enter)
command-terminal = Terminal
command-open-terminal = Bukas sa Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
command-prompt = Giaghat
command-new-tab = Bag-ong tab
command-search = Pangitaa
command-open-value = Ablihi ang "{ $value }"
command-search-value = Pangitaa ang "{ $value }"

schema-appearance = Panagway
schema-general = Heneral
schema-layout = Layout
schema-layout-detail = Window, mga pane, sidebar, ug focus ring.
schema-agent = Ahente
schema-agent-detail = Kinaiya sa ahente ug mga pagtugot sa himan.
schema-shortcuts = Mga shortcut
schema-shortcuts-detail = Pagtan-aw lamang sa pagbasa. I-edit ang settings.ron direkta aron usbon ang mga binding.
schema-terminal = Terminal
schema-browser = Browser
schema-mode = Mode
schema-mode-detail = Ang laraw sa kolor alang sa mga panid sa web. Ang device nagsunod sa imong sistema.
schema-device = Device
schema-light = Kahayag
schema-dark = Ngitngit
schema-language = Pinulongan
schema-language-detail = Gamita ang sistema, en-US, ja, o bisan unsang BCP 47 tag nga adunay katugbang nga ~/.vmux/locales/<tag>.ftl catalog.
schema-auto-update = Awtomatikong pag-update
schema-auto-update-detail = Susiha ug i-install ang mga update sa paglansad ug matag oras.
schema-startup-url = Pagsugod URL
schema-startup-url-detail = Ang Empty nagbukas sa command bar prompt.
schema-search-engine = Search engine
schema-search-engine-detail = Gigamit alang sa pagpangita sa web gikan sa Start ug sa command bar.
schema-window = Bintana
schema-pane = Pane
schema-side-sheet = Side sheet
schema-focus-ring = Focus singsing
schema-run-placement = Tugoti ang pag-override sa pagbutang sa dagan
schema-run-placement-detail = Papilia ang mga ahente sa run pane mode, direksyon, ug angkla.
schema-leader = Lider
schema-leader-detail = Prefix key para sa mga shortcut sa chord.
schema-chord-timeout = Ang oras sa chord
schema-chord-timeout-detail = Milliseconds sa dili pa matapos ang usa ka chord prefix.
schema-bindings = Mga pagbugkos
schema-confirm-close = Kumpirma nga hapit
schema-confirm-close-detail = Pag-aghat sa dili pa isira ang usa ka terminal nga adunay proseso nga nagdagan.
schema-default-theme = Default nga tema
schema-default-theme-detail = Ngalan sa aktibong tema gikan sa listahan sa mga tema.
