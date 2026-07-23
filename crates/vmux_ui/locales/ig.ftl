locale-name = Igbo
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

tools-title = Ngwaọrụ
tools-search = Chọọ ngwugwu, ndị nnọchi anya, MCP, ngwaọrụ asụsụ na faịlụ nhazi…
tools-open = Mepee ngwaọrụ
tools-fold = Kpachie ngwaọrụ
tools-unfold = Gbasaa ngwaọrụ
tools-scanning = Na-enyocha ngwaọrụ mpaghara…
tools-no-installed = Enweghị ngwaọrụ arụnyere
tools-empty = Enweghị ngwaọrụ dabara
tools-empty-detail = Wụnye ngwugwu ma ọ bụ tinye ngwugwu faịlụ nhazi ụdị Stow.
tools-apply = Tinye
tools-homebrew = Homebrew
tools-homebrew-sync = Usoro na ngwa arụnyere na-emekọrịta onwe ha na-akpaghị aka.
tools-open-brewfile = Mepee Brewfile
tools-managed = a na-achịkwa
tools-provider-homebrew-formulae = Usoro Homebrew
tools-provider-homebrew-casks = Ngwa Homebrew
tools-provider-npm = Ngwugwu npm
tools-provider-acp-agents = Ndị nnọchi anya ACP
tools-provider-language-tools = Ngwaọrụ asụsụ
tools-provider-mcp-servers = Sava MCP
tools-provider-dotfiles = Faịlụ nhazi
tools-status-available = Dị
tools-status-missing = Adịghị
tools-status-conflict = Esemokwu
tools-forget = Chefuo
tools-manage = Jikwaa
tools-link = Jikọọ
tools-unlink = Kwụpụ
tools-import = Bubata
tools-update-count = { $count ->
    [one] Mmelite 1
   *[other] Mmelite { $count }
}
tools-conflict-count = { $count ->
    [one] Esemokwu 1
   *[other] Esemokwu { $count }
}
tools-result-applied = Etinyela ngwaọrụ
tools-result-imported = Ebutela ngwaọrụ
tools-result-installed = Awụnyela { $name }
tools-result-updated = Emelitela { $name }
tools-result-uninstalled = Ewepụla { $name }
tools-result-forgotten = Echefuola { $name }
tools-result-managed = A na-achịkwa { $name } ugbu a
tools-result-linked = Ejikọọla { $name }
tools-result-unlinked = Ekwupụla { $name }
vault-title = Vault
vault-open = { common-open } Vault
vault-description = Ntọala mmekọrịta, ngwaọrụ, dotfiles na Ọmụma na Git.
vault-sync = Mmekọrịta
vault-create = Mepụta
vault-connect = Jikọọ
vault-private = Ebe nchekwa nkeonwe
vault-public-warning = Ebe nchekwa ọha na-ekpughe ihe ọmụma na nhazi gị.
vault-choose-repository = Họrọ ebe nchekwa…
vault-empty = efu
vault-clean = Kwalitere ruo ugbu a
vault-not-connected = Ejikọtaghị ya
vault-change-count = Mgbanwe: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

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

settings-empty = (efu)
settings-none = (ọ dịghị)

schema-system = Sistemu
schema-editor = Edezi
schema-recording = Ndekọ
schema-radius = Radiọs
schema-padding = Mgbakwunye oghere
schema-gap = Oghere
schema-width = Obosara
schema-color = Agba
schema-red = Uhie
schema-green = Akwụkwọ ndụ
schema-blue = Anụnụ
schema-follow-files = Soro faịlụ
schema-tidy-files = Hichaa faịlụ
schema-tidy-files-max = Oke nhicha faịlụ
schema-tidy-files-auto = Hichaa faịlụ na-akpaghị aka
schema-app-providers = Ndị na-eweta ngwa
schema-provider = Onye na-eweta
schema-kind = Ụdị
schema-models = Ụdịnlere
schema-acp = Ndị ọrụ ACP
schema-id = ID
schema-name = Aha
schema-command = Iwu
schema-arguments = Arụmụka
schema-environment = Gburugburụ
schema-working-directory = Ndekọ ọrụ
schema-shell = Shei
schema-font-family = Ezinụlọ mkpụrụedemede
schema-startup-directory = Ndekọ mmalite
schema-themes = Akpụkpọ
schema-color-scheme = Atụmatụ agba
schema-font-size = Nha mkpụrụedemede
schema-line-height = Ogologo ahịrị
schema-cursor-style = Ụdị kọsa
schema-cursor-blink = Mịnwụrụ kọsa
schema-custom-themes = Akpụkpọ ahaziri
schema-foreground = Ihu
schema-background = Azụ
schema-cursor = Kọsa
schema-ansi-colors = Agba ANSI
schema-keymap = Maapụ igodo
schema-explorer = Nyocha
schema-visible = A na-ahụ anya
schema-language-servers = Sava asụsụ
schema-servers = Sava
schema-language-id = ID asụsụ
schema-root-markers = Akara mgbọrọgwụ
schema-output-directory = Ndekọ mmepụta

menu-scene = Ihe ngosi
menu-layout = Nhazi
menu-terminal = Teminalụ
menu-browser = Nchọgharị
menu-service = Ọrụ
menu-bookmark = Edokọbara
menu-edit = Dezie

layout-knowledge = Ọmụma
layout-open-knowledge = Mepee Ọmụma
layout-open-welcome-knowledge = Mepee Nnọọ na Ọmụma
layout-open-path = Mepee { $path }
layout-fold-knowledge = Kpịchie ọmụma
layout-unfold-knowledge = Gbasaa ọmụma
layout-bookmarks = Ibe edokọbara
layout-new-folder = Folda ọhụrụ
layout-add-to-bookmarks = Tinye na ibe edokọbara
layout-move-to-bookmarks = Bugharịa na ibe edokọbara
layout-stack-number = Mkpokọ { $number }
layout-fold-stack = Kpịchie mkpokọ
layout-unfold-stack = Gbasaa mkpokọ
layout-close-stack = Mechie mkpokọ
layout-bookmark-in = Debe edokọbara na { $folder }

common-cancel = Kagbuo
common-delete = Hichapụ
common-save = Chekwaa
common-rename = Nyegharịa aha
common-expand = Gbasaa
common-collapse = Kpukọta
common-loading = Na-ebubata…
common-error = Njehie
common-output = Mmepụta
common-pending = Na-echere
common-current = nke ugbu a
common-stop = Kwụsị
services-command = Ọrụ Vmux
services-uptime-seconds = { $seconds }s
services-uptime-minutes = { $minutes }m { $seconds }s
services-uptime-hours = { $hours }h { $minutes }m
services-uptime-days = { $days }d { $hours }h

error-page-failed-load = Ibe ahụ ebubataghị
error-page-not-found = Ahụghị ibe ahụ
error-unknown-host = A maghị onye nnabata ngwa Vmux: { $host }

history-title = Akụkọ

command-new-app-chat = Mkparịta ụka { $provider }/{ $model } ọhụrụ (Ngwa)
command-interactive-mode-user = Scene > Ọnọdụ mmekọrịta > Onye ọrụ
command-interactive-mode-player = Scene > Ọnọdụ mmekọrịta > Onye ọkpụkpọ
command-minimize-window = Layout > Windo > Belata
command-toggle-layout = Layout > Nhazi > Gbanye/gbanyụọ nhazi
command-close-tab = Layout > Taabụ > Mechie taabụ
command-new-task = Layout > Taabụ > Ọrụ ọhụrụ…
command-next-tab = Layout > Taabụ > Taabụ na-esote
command-prev-tab = Layout > Taabụ > Taabụ gara aga
command-rename-tab = Layout > Taabụ > Nyegharịa taabụ aha
command-tab-select-1 = Layout > Taabụ > Họrọ taabụ 1
command-tab-select-2 = Layout > Taabụ > Họrọ taabụ 2
command-tab-select-3 = Layout > Taabụ > Họrọ taabụ 3
command-tab-select-4 = Layout > Taabụ > Họrọ taabụ 4
command-tab-select-5 = Layout > Taabụ > Họrọ taabụ 5
command-tab-select-6 = Layout > Taabụ > Họrọ taabụ 6
command-tab-select-7 = Layout > Taabụ > Họrọ taabụ 7
command-tab-select-8 = Layout > Taabụ > Họrọ taabụ 8
command-tab-select-last = Layout > Taabụ > Họrọ taabụ ikpeazụ
command-close-pane = Layout > Pane > Mechie pane
command-select-pane-left = Layout > Pane > Họrọ pane aka ekpe
command-select-pane-right = Layout > Pane > Họrọ pane aka nri
command-select-pane-up = Layout > Pane > Họrọ pane dị n’elu
command-select-pane-down = Layout > Pane > Họrọ pane dị n’okpuru
command-swap-pane-prev = Layout > Pane > Gbanwee pane na nke gara aga
command-swap-pane-next = Layout > Pane > Gbanwee pane na nke na-esote
command-equalize-pane-size = Layout > Pane > Mee nha pane hà
command-resize-pane-left = Layout > Pane > Gbanwee nha pane n’aka ekpe
command-resize-pane-right = Layout > Pane > Gbanwee nha pane n’aka nri
command-resize-pane-up = Layout > Pane > Gbanwee nha pane n’elu
command-resize-pane-down = Layout > Pane > Gbanwee nha pane n’okpuru
command-stack-close = Layout > Stack > Mechie stack
command-stack-next = Layout > Stack > Stack na-esote
command-stack-previous = Layout > Stack > Stack gara aga
command-stack-reopen = Layout > Stack > Mepee ibe emechiri ọzọ
command-stack-swap-prev = Layout > Stack > Bugharịa stack aka ekpe
command-stack-swap-next = Layout > Stack > Bugharịa stack aka nri
command-space-open = Layout > Space > Spaces
command-terminal-close = Terminal > Mechie terminal
command-terminal-next = Terminal > Terminal na-esote
command-terminal-prev = Terminal > Terminal gara aga
command-terminal-clear = Terminal > Kpochapụ terminal
command-browser-prev-page = Browser > Njegharị > Laghachi
command-browser-next-page = Browser > Njegharị > Gaa n’ihu
command-browser-reload = Browser > Njegharị > Bubata ọzọ
command-browser-hard-reload = Browser > Njegharị > Bubata ọzọ n’ike
command-open-in-place = Browser > Mepee > Mepee ebe a
command-open-in-new-stack = Browser > Mepee > Mepee na stack ọhụrụ
command-open-in-pane-top = Browser > Mepee > Mepee na pane dị n’elu
command-open-in-pane-right = Browser > Mepee > Mepee na pane aka nri
command-open-in-pane-bottom = Browser > Mepee > Mepee na pane dị n’okpuru
command-open-in-pane-left = Browser > Mepee > Mepee na pane aka ekpe
command-open-in-new-tab = Browser > Mepee > Mepee na taabụ ọhụrụ
command-open-in-new-space = Browser > Mepee > Mepee na Space ọhụrụ
command-browser-zoom-in = Browser > Nlele > Bawanye
command-browser-zoom-out = Browser > Nlele > Belata
command-browser-zoom-reset = Browser > Nlele > Nha nkịtị
command-browser-dev-tools = Browser > Nlele > Ngwa ndị mmepe
command-browser-open-command-bar = Browser > Ogwe > Ogwe iwu
command-browser-open-page-in-command-bar = Browser > Ogwe > Dezie ibe
command-browser-open-path-bar = Browser > Ogwe > Njegharị ụzọ
command-browser-open-commands = Browser > Ogwe > Iwu
command-browser-open-history = Browser > Ogwe > Akụkọ
command-service-open = Service > Mepee nyocha ọrụ
command-bookmark-toggle-active = Bookmark > Debe ibe a
command-bookmark-pin-active = Bookmark > Kụchie ibe a

layout-tab = Taabụ
layout-no-stacks = Enweghị stack
layout-loading = Na-ebubata…
layout-no-markdown-files = Enweghị faịlụ Markdown
layout-empty-folder = Folda efu
layout-worktree = worktree
layout-folder-name = Aha folda
layout-no-pins-bookmarks = Enweghị pin ma ọ bụ bookmark
layout-move-to = Bugharịa gaa { $folder }
layout-bookmark-current-page = Debe ibe nke ugbu a
layout-rename-folder = Nyegharịa folda aha
layout-remove-folder = Wepụ folda
layout-update-downloading = Na-ebudata mmelite
layout-update-installing = Na-etinye mmelite…
layout-update-ready = Ụdị ọhụrụ dị
layout-restart-update = Malitegharịa iji melite

agent-preparing = Na-akwadebe agent…
agent-send-all-queued = Zipụ prompt niile nọ n’ahịrị ugbu a (Esc)
agent-send = Zipụ (Enter)
agent-ready = Dị njikere mgbe ị dị.
agent-loading-older = Na-ebubata ozi ochie…
agent-load-older = Bubata ozi ochie
agent-continued-from = Gara n’ihu site na { $source }
agent-older-context-omitted = ewepụrụ ọnọdụ ochie
agent-interrupted = kwụsịrị n’etiti
agent-allow-tool = Kwe ka { $tool }?
agent-deny = Jụ
agent-allow-always = Kwe mgbe niile
agent-allow = Kwe
agent-loading-sessions = Na-ebubata sessions…
agent-no-resumable-sessions = Ahụghị session a ga-aga n’ihu
agent-no-matching-sessions = Enweghị session dabara
agent-no-matching-models = Enweghị model dabara
agent-choice-help = ↑/↓ ma ọ bụ Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = Họrọ folda repository
agent-choose-repository-detail = Họrọ repository Git dị na kọmputa nke agent ga-eji.
agent-choosing = Na-ahọrọ…
agent-choose-folder = Họrọ folda
agent-queued = nọ n’ahịrị
agent-attached = Ejikọtara:
agent-cancel-queued = Kagbuo prompt nọ n’ahịrị
agent-resume-queued = Gaa n’ihu na prompt nọ n’ahịrị
agent-clear-queue = Kpochapụ ahịrị
agent-send-all-now = zipụ niile ugbu a
agent-choose-option = Họrọ nhọrọ dị n’elu
agent-loading-media = Na-ebubata media…
agent-no-matching-media = Enweghị media dabara
agent-prompt-context = Ọnọdụ prompt
agent-details = Nkọwa
agent-path = Ụzọ
agent-tool = Ngwaọrụ
agent-server = Sava
agent-bytes = { $count } bytes
agent-worked-for = Rụrụ ọrụ ruo { $duration }
agent-worked-for-steps = { $count ->
    [one] Rụrụ ọrụ ruo { $duration } · nzọụkwụ 1
   *[other] Rụrụ ọrụ ruo { $duration } · nzọụkwụ { $count }
}
agent-tool-guardian-review = Nyocha nche
agent-tool-read-files = Gụrụ faịlụ
agent-tool-viewed-image = Lere onyonyo
agent-tool-used-browser = Jiri browser
agent-tool-searched-files = Chọgharịrị faịlụ
agent-tool-ran-commands = Gbalịrị iwu
agent-thinking = Na-eche
agent-subagent = Subagent
agent-prompt = Prompt
agent-thread = Eriri
agent-parent = Nne
agent-children = Ụmụ
agent-call = Oku
agent-raw-event = Ihe omume raw
agent-plan = Atụmatụ
agent-tasks = { $count ->
    [one] ọrụ 1
   *[other] ọrụ { $count }
}
agent-edited = Edeziri
agent-reconnecting = Na-ejikọ ọzọ { $attempt }/{ $total }
agent-status-running = Na-agba
agent-status-done = Emechala
agent-status-failed = Dara
agent-status-pending = Na-echere
agent-slash-attach-files = Jikọọ faịlụ
agent-slash-resume-session = Gaa n’ihu na session gara aga
agent-slash-select-model = Họrọ model
agent-slash-continue-cli = Gaa n’ihu na session a na CLI
agent-session-just-now = ugbu a
agent-session-minutes-ago = nkeji { $count } gara aga
agent-session-hours-ago = awa { $count } gara aga
agent-session-days-ago = ụbọchị { $count } gara aga
agent-working-working = Na-arụ ọrụ
agent-working-thinking = Na-eche
agent-working-pondering = Na-atụgharị uche
agent-working-noodling = Na-enyocha echiche
agent-working-percolating = Na-esiwanye ike
agent-working-conjuring = Na-akpọpụta echiche
agent-working-cooking = Na-esi nri
agent-working-brewing = Na-amịpụta
agent-working-musing = Na-atụgharị uche
agent-working-ruminating = Na-atụgharị n’uche
agent-working-scheming = Na-akpa atụmatụ
agent-working-synthesizing = Na-ejikọta echiche
agent-working-tinkering = Na-emegharị obere ihe
agent-working-churning = Na-agbagharị
agent-working-vibing = Na-anata vibe
agent-working-simmering = Na-esi nwayọ
agent-working-crafting = Na-akpụpụta
agent-working-divining = Na-achọpụta
agent-working-mulling = Na-atụle
agent-working-spelunking = Na-egwu n’ime omimi

editor-toggle-explorer = Gbanye/gbanyụọ Explorer (Cmd+B)
editor-unsaved = echekwabeghị
editor-rendered-markdown = Markdown egosiri na ndezi ndụ
editor-note = Ndetu
editor-source-editor = Editor isi iyi
editor-editor = Editor
editor-git-diff = Git diff
editor-diff = Diff
editor-tidy = Kpochapụ
editor-always = Mgbe niile
editor-unchanged-previews = { $count ->
    [one] ✦ preview 1 agbanweghị
   *[other] ✦ preview { $count } agbanweghị
}
editor-open-externally = Mepee n’èzí
editor-changed-line = Ahịrị gbanwere
editor-go-to-definition = Gaa na Definition
editor-find-references = Chọta References
editor-references = { $count ->
    [one] reference 1
   *[other] references { $count }
}
editor-lsp-starting = { $server } na-amalite…
editor-lsp-not-installed = { $server } — etinyebeghị
editor-explorer = Explorer
editor-open-editors = Editors mepere emepe
editor-outline = Outline
editor-new-file = Faịlụ ọhụrụ
editor-new-folder = Folda ọhụrụ
editor-delete-confirm = Hichapụ “{ $name }”? A gaghị eweghachi nke a.
editor-created-folder = Emepụtara folda { $name }
editor-created-file = Emepụtara faịlụ { $name }
editor-renamed-to = E nyere aha ọhụrụ { $name }
editor-deleted = Ehichapụrụ { $name }
editor-failed-decode-image = Enweghị ike ịdecode onyonyo
editor-preview-large-image = onyonyo (ọ buru oke ibu maka preview)
editor-preview-binary = binary
editor-preview-file = faịlụ

git-status-clean = dị ọcha
git-status-modified = gbanwere
git-status-staged = staged
git-status-staged-modified = staged*
git-status-untracked = anaghị eso
git-status-deleted = ehichapụrụ
git-status-conflict = esemokwu
git-accept-all = ✓ nabata niile
git-unstage = Wepụ na stage
git-confirm-deny-all = Kwenye ịjụ niile
git-deny-all = ✗ jụ niile
git-commit-message = ozi commit
git-commit = Commit ({ $count })
git-push = ↑ Push
git-loading-diff = Na-ebubata diff…
git-no-changes = Enweghị mgbanwe igosi
git-accept = ✓ nabata
git-deny = ✗ jụ
git-show-unchanged-lines = Gosi ahịrị { $count } agbanweghị

terminal-loading = Na-ebubata…
terminal-runs-when-ready = na-agba mgbe ọ dị njikere · Ctrl+C na-akpochapụ · Esc na-awụpụ
terminal-booting = na-amalite
terminal-type-command = pịnye iwu · na-agba mgbe ọ dị njikere · Esc na-awụpụ

setup-tagline-claude = Agent koodu Anthropic, na Vmux
setup-tagline-codex = Agent koodu OpenAI, na Vmux
setup-tagline-vibe = Agent koodu Mistral, na Vmux
setup-install-title = Tinye { $name } CLI
setup-homebrew-required = A chọrọ Homebrew iji tinye { $command }, ma edozibeghị ya. Vmux ga-ebu ụzọ tinye Homebrew, mgbe ahụ { $name }.
setup-terminal-instructions = Na terminal, pịa Return iji malite, tinye paswọọdụ Mac gị mgbe a jụrụ gị.
setup-command-missing = Vmux mepere ibe a n’ihi na iwu { $command } adịghị n’ime kọmputa a. Gbaa iwu dị n’okpuru iji nweta ya.
setup-install-failed = Ntinye emechaghị. Lelee terminal maka nkọwa, wee nwaa ọzọ.
setup-installing = Na-etinye…
setup-install-homebrew = Tinye Homebrew + { $name }
setup-run-install = Gbaa iwu ntinye
setup-auto-reload = Vmux na-agba ya na terminal ma na-ebubata ọzọ mgbe { $command } dị njikere.

debug-title = Debug
debug-auto-update = Mmelite akpaka
debug-simulate-update = Mee ka mmelite dị ka ọ dị
debug-simulate-download = Mee ka nbudata dị ka ọ na-eme
debug-clear-update = Kpochapụ mmelite
debug-trigger-restart = Kpalite mmalitegharị

command-manage-spaces = Jikwaa oghere…
command-pane-stack-location = pane { $pane } / stack { $stack }
command-space-pane-stack-location = { $space } / pane { $pane } / stack { $stack }
command-terminal-path = Terminal ({ $path })
command-group-interactive-mode = Ọnọdụ mmekọrịta
command-group-window = Windo
command-group-tab = Taabụ
command-group-pane = Pane
command-group-stack = Stack
command-group-space = Oghere
command-group-navigation = Njegharị
command-group-open = Mepee
command-group-view = Nlele
command-group-bar = Ogwe

menu-close-vmux = Mechie Vmux

agents-terminal-coding-agent = Agen koodu na-arụ na Terminal
