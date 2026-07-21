common-open = Öppna
common-close = Stäng
common-install = Installera
common-uninstall = Avinstallera
common-update = Uppdatera
common-retry = Försök igen
common-refresh = Uppdatera
common-remove = Ta bort
common-enable = Aktivera
common-disable = Inaktivera
common-new = Ny
common-active = aktiv
common-running = körs
common-done = klart
common-failed = Misslyckades
common-installed = Installerad
common-items = { $count ->
    [one] { $count } objekt
   *[other] { $count } objekt
}
start-title = Start
start-tagline = En prompt. Allt klart.

agents-title = Agenter
agents-search = Sök ACP- och CLI-agenter…
agents-empty = Inga matchande agenter
agents-empty-detail = Prova ett namn, en runtime eller ACP/CLI.
agents-install-failed = Installationen misslyckades
agents-updating = Uppdaterar…
agents-retrying = Försöker igen…
agents-preparing = Förbereder…

extensions-title = Tillägg
extensions-search = Sök installerade tillägg eller Chrome Web Store…
extensions-relaunch = Starta om för att tillämpa
extensions-empty = Inga tillägg installerade
extensions-no-match = Inga matchande tillägg
extensions-empty-detail = Sök i Chrome Web Store ovan och tryck på Retur.
extensions-no-match-detail = Prova ett annat namn eller tilläggs-ID.
extensions-on = På
extensions-off = Av
extensions-enable-confirm = Aktivera { $name }?
extensions-enable-permissions = Aktivera { $name } och tillåt:

lsp-title = Språkservrar
lsp-search = Sök språkservrar, linters, formaterare…
lsp-loading = Läser in katalog…
lsp-empty = Inga matchande språkservrar
lsp-empty-detail = Prova ett annat språk, en linter eller formaterare.
lsp-needs = kräver { $tool }
lsp-status-available = Tillgänglig
lsp-status-on-path = På PATH
lsp-status-installing = Installerar…
lsp-status-installed = Installerad
lsp-status-outdated = Uppdatering finns
lsp-status-running = Körs
lsp-status-failed = Misslyckades

spaces-title = Ytor
spaces-new-placeholder = Namn på ny yta
spaces-empty = Inga ytor
spaces-default-name = Yta { $number }
spaces-tabs = { $count ->
    [one] 1 flik
   *[other] { $count } flikar
}
spaces-delete = Radera yta

team-title = Team
team-just-you = Bara du i den här ytan
team-agents = { $count ->
    [one] Du och 1 agent
   *[other] Du och { $count } agenter
}
team-empty = Ingen här än
team-you = Du
team-agent = Agent

services-title = Bakgrundstjänster
services-processes = { $count ->
    [one] 1 process
   *[other] { $count } processer
}
services-kill-all = Tvångsavsluta alla
services-not-running = Tjänsten körs inte
services-start-with = Starta med:
services-empty = Inga aktiva processer
services-filter = Filtrera processer…
services-no-match = Inga matchande processer
services-connected = Ansluten
services-disconnected = Frånkopplad
services-attached = ansluten
services-kill = Tvångsavsluta
services-memory = Minne
services-size = Storlek
services-shell = Skal

error-title = Fel

history-search = Sök i historik
history-clear-all = Rensa allt
history-clear-confirm = Rensa all historik?
history-clear-warning = Detta går inte att ångra.
history-cancel = Avbryt
history-today = Idag
history-yesterday = Igår
history-days-ago = { $count } dagar sedan
history-day-offset = Dag -{ $count }

settings-title = Inställningar
settings-loading = Läser in inställningar…
settings-stored = Sparas i ~/.vmux/settings.ron
settings-other = Övrigt
settings-software-update = Programuppdatering
settings-check-updates = Sök efter uppdateringar
settings-check-updates-hint = Söker automatiskt vid start och varje timme när automatisk uppdatering är aktiverad.
settings-update-unavailable = Inte tillgänglig
settings-update-unavailable-hint = Uppdateraren ingår inte i den här versionen.
settings-update-checking = Söker…
settings-update-checking-hint = Söker efter uppdateringar…
settings-update-check-again = Sök igen
settings-update-current = Vmux är uppdaterat.
settings-update-downloading = Hämtar…
settings-update-downloading-hint = Hämtar Vmux { $version }…
settings-update-installing = Installerar…
settings-update-installing-hint = Installerar Vmux { $version }…
settings-update-ready = Uppdatering klar
settings-update-ready-hint = Vmux { $version } är klar. Starta om för att tillämpa den.
settings-update-try-again = Försök igen
settings-update-failed = Kunde inte söka efter uppdateringar.
settings-item = Objekt
settings-item-number = Objekt { $number }
settings-press-key = Tryck på en tangent…
settings-saved = Sparat
settings-record-key = Klicka för att spela in en ny tangentkombination

tray-open-window = Öppna fönster
tray-close-window = Stäng fönster
tray-pause-recording = Pausa inspelning
tray-resume-recording = Fortsätt inspelning
tray-finish-recording = Avsluta inspelning
tray-quit = Avsluta Vmux

composer-attach-files = Bifoga filer (/upload)
composer-remove-attachment = Ta bort bilaga

layout-back = Tillbaka
layout-forward = Framåt
layout-reload = Läs om
layout-bookmark-page = Bokmärk den här sidan
layout-remove-bookmark = Ta bort bokmärke
layout-pin-page = Fäst den här sidan
layout-unpin-page = Lossa den här sidan
layout-manage-extensions = Hantera tillägg
layout-new-stack = Ny stack
layout-close-tab = Stäng flik
layout-bookmark = Bokmärke
layout-pin = Fäst
layout-new-tab = Ny flik
layout-team = Team

command-switch-space = Byt yta…
command-search-ask = Sök eller fråga…
command-new-tab-placeholder = Sök eller ange en URL, eller välj Terminal…
command-placeholder = Ange en URL, sök flikar eller skriv > för kommandon…
command-composer-placeholder = Skriv / för kommandon eller @ för media
command-send = Skicka (Retur)
command-terminal = Terminal
command-open-terminal = Öppna i Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] 1 flik
   *[other] { $count } flikar
}
command-prompt = Prompt
command-new-tab = Ny flik
command-search = Sök
command-open-value = Öppna ”{ $value }”
command-search-value = Sök efter ”{ $value }”

schema-appearance = Utseende
schema-general = Allmänt
schema-layout = Layout
schema-layout-detail = Fönster, paneler, sidofält och fokusring.
schema-agent = Agent
schema-agent-detail = Agentens beteende och behörigheter för verktyg.
schema-shortcuts = Kortkommandon
schema-shortcuts-detail = Skrivskyddad vy. Redigera settings.ron direkt för att ändra bindningar.
schema-terminal = Terminal
schema-browser = Webbläsare
schema-mode = Läge
schema-mode-detail = Färgschema för webbsidor. Enhet följer systemet.
schema-device = Enhet
schema-light = Ljust
schema-dark = Mörkt
schema-language = Språk
schema-language-detail = Använd systemet, en-US, ja eller valfri BCP 47-tagg med en matchande ~/.vmux/locales/<tag>.ftl-katalog.
schema-auto-update = Automatisk uppdatering
schema-auto-update-detail = Sök efter och installera uppdateringar vid start och varje timme.
schema-startup-url = Start-URL
schema-startup-url-detail = Tomt öppnar kommandoradens prompt.
schema-search-engine = Sökmotor
schema-search-engine-detail = Används för webbsökningar från Start och kommandoraden.
schema-window = Fönster
schema-pane = Panel
schema-side-sheet = Sidopanel
schema-focus-ring = Fokusring
schema-run-placement = Tillåt åsidosättning av körplacering
schema-run-placement-detail = Låt agenter välja panelläge, riktning och ankare för körning.
schema-leader = Leader
schema-leader-detail = Prefixtangent för chord-kortkommandon.
schema-chord-timeout = Tidsgräns för chord
schema-chord-timeout-detail = Millisekunder innan ett chord-prefix slutar gälla.
schema-bindings = Bindningar
schema-confirm-close = Bekräfta stängning
schema-confirm-close-detail = Fråga innan en terminal med en körande process stängs.
schema-default-theme = Standardtema
schema-default-theme-detail = Namnet på det aktiva temat i temalistan.
