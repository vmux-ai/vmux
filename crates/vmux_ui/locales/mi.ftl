common-open = Tuwhera
common-close = Katia
common-install = Tāuta
common-uninstall = Wetetāuta
common-update = Whakahou
common-retry = Ngana ano
common-refresh = Tāmata
common-remove = Tango
common-enable = Whakahohe
common-disable = Monokia
common-new = Hou
common-active = kaha
common-running = rere
common-done = kua oti
common-failed = I rahua
common-installed = Kua whakauruhia
common-items = { $count ->
    [one] { $count } tūemi
   *[other] { $count } tūemi
}
start-title = Tīmata
start-tagline = Kotahi te akiaki. Ko nga mea katoa, kua oti.

agents-title = Nga Kaihoko
agents-search = Rapua ACP me CLI kaihoko…
agents-empty = Karekau he kaihoko taurite
agents-empty-detail = Whakamātauria he ingoa, wā whakahaere, ACP/CLI ranei.
agents-install-failed = I rahua te tāuta
agents-updating = Whakahōu ana…
agents-retrying = Kei te ngana ano…
agents-preparing = Kei te whakareri…

extensions-title = Toronga
extensions-search = Kua whakauruhia te rapu, Chrome Web Store ranei…
extensions-relaunch = Whakarewa ano ki te tono
extensions-empty = Karekau he taapiri i whakauruhia
extensions-no-match = Karekau he taapiri taapiri
extensions-empty-detail = Rapua te Chrome Web Store i runga ake ka pehi Return.
extensions-no-match-detail = Ngana ki tetahi atu ingoa, toronga ID ranei.
extensions-on = Kei runga
extensions-off = Wehe
extensions-enable-confirm = Whakahohe { $name }?
extensions-enable-permissions = Whakahohea { $name } ka whakaaetia:

lsp-title = Tūmau Reo
lsp-search = Rapua nga kaitoro reo, rarangi, whakahōputu...
lsp-loading = Uta ana putumōhio…
lsp-empty = Karekau he tūmau reo ōrite
lsp-empty-detail = Whakamātauria he reo kē, he rārangi, he kaiwhakahōputu rānei.
lsp-needs = hiahia { $tool }
lsp-status-available = Kei te waatea
lsp-status-on-path = Kei PATH
lsp-status-installing = Tāuta ana…
lsp-status-installed = Kua whakauruhia
lsp-status-outdated = Kei te waatea te whakahou
lsp-status-running = Rere ana
lsp-status-failed = I rahua

spaces-title = Nga waahi
spaces-new-placeholder = Ingoa mokowā hou
spaces-empty = Kaore he waahi
spaces-default-name = Mokowā { $number }
spaces-tabs = { $count ->
    [one] 1 ripa
   *[other] { $count } ripa
}
spaces-delete = Mukua te mokowā

team-title = Kapa
team-just-you = Ko koe anake i tenei waahi
team-agents = { $count ->
    [one] Ko koe me te kaihoko kotahi
   *[other] Ko koe me { $count } nga kaihoko
}
team-empty = Kaore ano he tangata i konei
team-you = Ko koe
team-agent = Kaihoko

services-title = Ratonga Papamuri
services-processes = { $count ->
    [one] 1 tukanga
   *[other] { $count } nga tukanga
}
services-kill-all = Patua Katoa
services-not-running = Kaore te ratonga e rere
services-start-with = Tīmata ki:
services-empty = Karekau he tukanga kaha
services-filter = Tātari ngā tukanga…
services-no-match = Karekau he tukanga taurite
services-connected = Kua hono
services-disconnected = Momotuhia
services-attached = piri
services-kill = Whakamatea
services-memory = Maharahara
services-size = Rahi
services-shell = Anga

error-title = Hapa

history-search = Hītori rapu
history-clear-all = Ūkuia katoa
history-clear-confirm = Ūkuia te hītori katoa?
history-clear-warning = Kaore e taea te whakakore i tenei.
history-cancel = Whakakore
history-today = I tenei ra
history-yesterday = Inanahi
history-days-ago = { $count } ra ki muri
history-day-offset = Ra -{ $count }

settings-title = Tautuhinga
settings-loading = Uta ana i nga tautuhinga…
settings-stored = Kua penapena ki ~/.vmux/settings.ron
settings-other = Ētahi atu
settings-software-update = Whakahou Pūmanawa
settings-check-updates = Tirohia mo nga Whakahoutanga
settings-check-updates-hint = Ka taki aunoa i te whakarewatanga me ia haora ka whakahohea te Whakahou Aunoa.
settings-update-unavailable = Kāore i te wātea
settings-update-unavailable-hint = Kaore i whakauruhia te kaiwhakahou ki tenei hanga.
settings-update-checking = Takitaki ana…
settings-update-checking-hint = Kei te tirotiro mo nga whakahōu…
settings-update-check-again = Tirohia Anō
settings-update-current = Vmux he mea hou.
settings-update-downloading = Tikiake ana…
settings-update-downloading-hint = Kei te tikiake Vmux { $version }…
settings-update-installing = Tāuta ana…
settings-update-installing-hint = Tāuta ana Vmux { $version }…
settings-update-ready = Whakahou Kua Reri
settings-update-ready-hint = Vmux { $version } kua reri. Tīmata anō ki te tono.
settings-update-try-again = Ngana ano
settings-update-failed = Kaore e taea te tirotiro mo nga whakahou.
settings-item = Tūemi
settings-item-number = Tūemi { $number }
settings-press-key = Pēhia he kī…
settings-saved = Whakaorangia
settings-record-key = Paatohia ki te tuhi i tetahi paheko matua hou

tray-open-window = Whakatuwhera Matapihi
tray-close-window = Katia te Matapihi
tray-pause-recording = Whakaorangia te Hopu
tray-resume-recording = Whakahokia te Hopu
tray-finish-recording = Whakaoti Rekoata
tray-quit = Whakamutua Vmux

composer-attach-files = Āpiti kōnae (/upload)
composer-remove-attachment = Tangohia te taapiri

layout-back = Whakamuri
layout-forward = Whakamua
layout-reload = Utaina ano
layout-bookmark-page = Tohua tenei wharangi
layout-remove-bookmark = Tango tohuwāhi
layout-pin-page = Pini tenei wharangi
layout-unpin-page = Wewetehia tenei wharangi
layout-manage-extensions = Whakahaere toronga
layout-new-stack = Tāpae Hou
layout-close-tab = Katia te ripa
layout-bookmark = Tohuwāhi
layout-pin = Pin
layout-new-tab = Ripa hou
layout-team = Kapa

command-switch-space = Huri mokowā…
command-search-ask = Rapua, patai ranei…
command-new-tab-placeholder = Rapua, patohia ranei he URL, tīpakohia te Kāpeka…
command-placeholder = Patohia he URL, ripa rapu, > ranei mo nga tono...
command-composer-placeholder = Patohia / mo nga tono, @ ranei mo te pāpāho
command-send = Tukuna (Enter)
command-terminal = Kapekapeka
command-open-terminal = Tuwhera ki te Terminal
command-stack = Tāpae
command-tabs = { $count ->
    [one] 1 ripa
   *[other] { $count } ripa
}
command-prompt = Whakatairanga
command-new-tab = Ripa hou
command-search = Rapu
command-open-value = Tuwhera "{ $value }"
command-search-value = Rapua "{ $value }"

schema-appearance = Te ahua
schema-general = Whānui
schema-layout = Tahora
schema-layout-detail = Matapihi, pihanga, paetaha, me te mowhiti arotahi.
schema-agent = Kaihoko
schema-agent-detail = Te whanonga kaihoko me nga whakaaetanga taputapu.
schema-shortcuts = Pokatata
schema-shortcuts-detail = Tirohanga panui-anake. Whakatika tika settings.ron ki te huri i nga here.
schema-terminal = Kapekapeka
schema-browser = Pūtirotiro
schema-mode = Aratau
schema-mode-detail = Kaupapa tae mo nga wharangi paetukutuku. Ka whai te taputapu i to punaha.
schema-device = Pūrere
schema-light = Maama
schema-dark = pouri
schema-language = Reo
schema-language-detail = Whakamahia te punaha, en-US, ja, tetahi tohu BCP 47 ranei me te putumōhio ~/.vmux/locales/<tag>.ftl.
schema-auto-update = Whakahou-aunoa
schema-auto-update-detail = Tirohia me te whakauru i nga whakahou mo te whakarewatanga me ia haora.
schema-startup-url = Whakaoho URL
schema-startup-url-detail = Ka whakatuwherahia e te Putua te wawe pae whakahau.
schema-search-engine = Pukaha rapu
schema-search-engine-detail = Ka whakamahia mo nga rapunga paetukutuku mai i te Timata me te pae whakahau.
schema-window = Matapihi
schema-pane = Pihanga
schema-side-sheet = Pepa taha
schema-focus-ring = Ringa arotahi
schema-run-placement = Whakaaetia te whakakore i te tuunga whakahaere
schema-run-placement-detail = Tukua nga kaihoko ki te whiriwhiri i te aratau oma, te ahunga, me te punga.
schema-leader = Rangatira
schema-leader-detail = Kī mua mo nga pokatata chord.
schema-chord-timeout = Te wa mutunga
schema-chord-timeout-detail = Mirihakona i mua i te paunga o te chord prefix.
schema-bindings = Nga here
schema-confirm-close = Whakaū kati
schema-confirm-close-detail = Tonoa i mua i te kati i te tauranga me te tukanga e rere ana.
schema-default-theme = Kaupapa taunoa
schema-default-theme-detail = Ingoa o te kaupapa hohe mai i te rarangi kaupapa.
