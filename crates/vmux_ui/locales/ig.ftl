common-open = Mepee
common-close = Mechie
common-install = Wụnye
common-uninstall = Wepụ nwụnye
common-update = Melite
common-retry = Nwaa ọzọ
common-refresh = Megharịa
common-remove = Wepụ
common-enable = Gbanwuo
common-disable = Gbanyụọ
common-new = Ọhụrụ
common-active = na-arụ ọrụ
common-running = na-agba
common-done = emechaala
common-failed = Dara
common-installed = Awụnyere
common-items = { $count ->
    [one] { $count } ihe
   *[other] { $count } ihe
}
start-title = Malite
start-tagline = Otu prompt. Ihe ọ bụla, emechaala.

agents-title = Ndị agent
agents-search = Chọọ ndị agent ACP na CLI…
agents-empty = Enweghị agent dabara
agents-empty-detail = Nwaa aha, runtime, ma ọ bụ ACP/CLI.
agents-install-failed = Nwụnye dara
agents-updating = Na-emelite…
agents-retrying = Na-anwa ọzọ…
agents-preparing = Na-akwadebe…

extensions-title = Mgbakwunye
extensions-search = Chọọ ndị awụnyere ma ọ bụ Chrome Web Store…
extensions-relaunch = Malitegharịa ka ọ rụọ ọrụ
extensions-empty = Enweghị mgbakwunye awụnyere
extensions-no-match = Enweghị mgbakwunye dabara
extensions-empty-detail = Chọọ Chrome Web Store n’elu wee pịa Return.
extensions-no-match-detail = Nwaa aha ọzọ ma ọ bụ ID mgbakwunye.
extensions-on = Gbanwuru
extensions-off = Gbanyụrụ
extensions-enable-confirm = Gbanwuo { $name }?
extensions-enable-permissions = Gbanwuo { $name } ma kwe ka:

lsp-title = Sava Asụsụ
lsp-search = Chọọ sava asụsụ, linters, formatters…
lsp-loading = Na-ebunye katalọgụ…
lsp-empty = Enweghị sava asụsụ dabara
lsp-empty-detail = Nwaa asụsụ ọzọ, linter, ma ọ bụ formatter.
lsp-needs = chọrọ { $tool }
lsp-status-available = Dị
lsp-status-on-path = Dị na PATH
lsp-status-installing = Na-awụnye…
lsp-status-installed = Awụnyere
lsp-status-outdated = Mmelite dị
lsp-status-running = Na-agba
lsp-status-failed = Dara

spaces-title = Ebe
spaces-new-placeholder = Aha ebe ọhụrụ
spaces-empty = Enweghị ebe
spaces-default-name = Ebe { $number }
spaces-tabs = { $count ->
    [one] taabụ 1
   *[other] taabụ { $count }
}
spaces-delete = Hichapụ ebe

team-title = Ndị otu
team-just-you = Naanị gị nọ n’ebe a
team-agents = { $count ->
    [one] Gị na agent 1
   *[other] Gị na agent { $count }
}
team-empty = Onweghị onye nọ ebe a
team-you = Gị
team-agent = Agent

services-title = Ọrụ Ndabere
services-processes = { $count ->
    [one] process 1
   *[other] process { $count }
}
services-kill-all = Kwụsị Ha Niile N’ike
services-not-running = Ọrụ a anaghị agba
services-start-with = Malite na:
services-empty = Enweghị process na-agba
services-filter = Nyochaa process…
services-no-match = Enweghị process dabara
services-connected = Ejikọrọ
services-disconnected = Ejikọghị
services-attached = etinyere
services-kill = Kwụsị n’ike
services-memory = Ebe nchekwa
services-size = Nha
services-shell = Shell

error-title = Njehie

history-search = Chọọ akụkọ
history-clear-all = Hichapụ niile
history-clear-confirm = Hichapụ akụkọ niile?
history-clear-warning = A gaghị eweghachi nke a.
history-cancel = Kagbuo
history-today = Taa
history-yesterday = Ụnyaahụ
history-days-ago = ụbọchị { $count } gara aga
history-day-offset = Ụbọchị -{ $count }

settings-title = Ntọala
settings-loading = Na-ebunye ntọala…
settings-stored = Echekwara na ~/.vmux/settings.ron
settings-other = Ndị ọzọ
settings-software-update = Mmelite ngwanrọ
settings-check-updates = Lelee mmelite
settings-check-updates-hint = Na-elele onwe ya mgbe mmalite na kwa elekere mgbe Auto-update gbanyere.
settings-update-unavailable = Adịghị
settings-update-unavailable-hint = Updater adịghị na build a.
settings-update-checking = Na-enyocha…
settings-update-checking-hint = Na-enyocha mmelite…
settings-update-check-again = Lelee ọzọ
settings-update-current = Vmux emeliterela.
settings-update-downloading = Na-ebudata…
settings-update-downloading-hint = Na-ebudata Vmux { $version }…
settings-update-installing = Na-awụnye…
settings-update-installing-hint = Na-awụnye Vmux { $version }…
settings-update-ready = Mmelite adịla njikere
settings-update-ready-hint = Vmux { $version } adịla njikere. Malitegharịa ka ọ rụọ ọrụ.
settings-update-try-again = Nwaa ọzọ
settings-update-failed = Enweghị ike ịlele mmelite.
settings-item = Ihe
settings-item-number = Ihe { $number }
settings-press-key = Pịa igodo…
settings-saved = Echekwara
settings-record-key = Pịa ka ịdekọ ngwakọta igodo ọhụrụ

tray-open-window = Mepee windo
tray-close-window = Mechie windo
tray-pause-recording = Kwụsị ndekọ nwa oge
tray-resume-recording = Gaa n’ihu ndekọ
tray-finish-recording = Mechaa ndekọ
tray-quit = Kwụsị Vmux

composer-attach-files = Tinye faịlụ (/upload)
composer-remove-attachment = Wepụ mgbakwunye

layout-back = Laghachi
layout-forward = Gaa n’ihu
layout-reload = Bubanye ọzọ
layout-bookmark-page = Tinye ibe a na bookmark
layout-remove-bookmark = Wepụ bookmark
layout-pin-page = Kpọgide ibe a
layout-unpin-page = Wepụ mkpọgide ibe a
layout-manage-extensions = Jikwaa mgbakwunye
layout-new-stack = Stack ọhụrụ
layout-close-tab = Mechie taabụ
layout-bookmark = Bookmark
layout-pin = Kpọgide
layout-new-tab = Taabụ ọhụrụ
layout-team = Ndị otu

command-switch-space = Gbanwee ebe…
command-search-ask = Chọọ ma ọ bụ jụọ…
command-new-tab-placeholder = Chọọ ma ọ bụ pịnye URL, ma ọ bụ họrọ Terminal…
command-placeholder = Pịnye URL, chọọ taabụ, ma ọ bụ > maka iwu…
command-composer-placeholder = Pịnye / maka iwu ma ọ bụ @ maka media
command-send = Zipụ (Enter)
command-terminal = Terminal
command-open-terminal = Mepee na Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] taabụ 1
   *[other] taabụ { $count }
}
command-prompt = Prompt
command-new-tab = Taabụ ọhụrụ
command-search = Chọọ
command-open-value = Mepee “{ $value }”
command-search-value = Chọọ “{ $value }”

schema-appearance = Ọdịdị
schema-general = Izugbe
schema-layout = Nhazi
schema-layout-detail = Windo, pane, sidebar, na focus ring.
schema-agent = Agent
schema-agent-detail = Omume agent na ikike ngwaọrụ.
schema-shortcuts = Ụzọ mkpirisi
schema-shortcuts-detail = Nlele naanị-agụ. Dezie settings.ron ozugbo iji gbanwee bindings.
schema-terminal = Terminal
schema-browser = Ihe nchọgharị
schema-mode = Ọnọdụ
schema-mode-detail = Usoro agba maka ibe weebụ. Device na-eso sistemụ gị.
schema-device = Device
schema-light = Ìhè
schema-dark = Ọchịchịrị
schema-language = Asụsụ
schema-language-detail = Jiri nke sistemụ, en-US, ja, ma ọ bụ tag BCP 47 ọ bụla nwere katalọgụ ~/.vmux/locales/<tag>.ftl kwekọrọ.
schema-auto-update = Auto-update
schema-auto-update-detail = Lelee ma wụnye mmelite mgbe mmalite na kwa elekere.
schema-startup-url = URL mmalite
schema-startup-url-detail = Ọ bụrụ na ọ tọgbọ chakoo, ọ ga-emepe prompt nke command bar.
schema-search-engine = Injin ọchụchọ
schema-search-engine-detail = A na-eji ya maka ọchụchọ weebụ site na Malite na command bar.
schema-window = Windo
schema-pane = Pane
schema-side-sheet = Mpempe akụkụ
schema-focus-ring = Focus ring
schema-run-placement = Kwe ka override nke ebe run
schema-run-placement-detail = Kwe ka ndị agent họrọ mode pane run, ntụziaka, na anchor.
schema-leader = Leader
schema-leader-detail = Igodo mbido maka ụzọ mkpirisi chord.
schema-chord-timeout = Oge ngwụcha chord
schema-chord-timeout-detail = Millisekọnd tupu prefix chord agwụ.
schema-bindings = Bindings
schema-confirm-close = Kwenye tupu imechi
schema-confirm-close-detail = Jụọ tupu imechi terminal nwere process na-agba.
schema-default-theme = Theme ndabara
schema-default-theme-detail = Aha theme na-arụ ọrụ site na ndepụta themes.
