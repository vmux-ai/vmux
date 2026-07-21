common-open = ເປີດ
common-close = ປິດ
common-install = ຕິດຕັ້ງ
common-uninstall = ຖອນການຕິດຕັ້ງ
common-update = ອັບເດດ
common-retry = ລອງໃໝ່
common-refresh = ໂຫຼດຂໍ້ມູນຄືນໃໝ່
common-remove = ເອົາອອກ
common-enable = ເປີດໃຊ້
common-disable = ປິດໃຊ້ງານ
common-new = ໃໝ່
common-active = ເຄື່ອນໄຫວ
common-running = ແລ່ນ
common-done = ສຳເລັດແລ້ວ
common-failed = ລົ້ມເຫລວ
common-installed = ຕິດຕັ້ງແລ້ວ
common-items = { $count ->
    [one] { $count } ລາຍການ
   *[other] { $count } ລາຍການ
}
start-title = ເລີ່ມ
start-tagline = ການເຕືອນຫນຶ່ງ. ທຸກຢ່າງ, ເຮັດແລ້ວ.

agents-title = ຕົວແທນ
agents-search = ຊອກຫາ ACP ແລະ CLI ຕົວແທນ…
agents-empty = ບໍ່ມີຕົວແທນທີ່ກົງກັນ
agents-empty-detail = ລອງຊື່, runtime, ຫຼື ACP/CLI.
agents-install-failed = ການຕິດຕັ້ງລົ້ມເຫລວ
agents-updating = ກຳລັງອັບເດດ...
agents-retrying = ກຳລັງລອງໃໝ່...
agents-preparing = ກຳລັງກະກຽມ...

extensions-title = ສ່ວນຂະຫຍາຍ
extensions-search = ຊອກຫາທີ່ຕິດຕັ້ງ ຫຼື Chrome Web Store…
extensions-relaunch = ເປີດຄືນໃໝ່ເພື່ອສະໝັກ
extensions-empty = ບໍ່ມີສ່ວນຂະຫຍາຍທີ່ຕິດຕັ້ງ
extensions-no-match = ບໍ່ມີສ່ວນຂະຫຍາຍທີ່ກົງກັນ
extensions-empty-detail = ຄົ້ນຫາ Chrome Web Store ຂ້າງເທິງ ແລະກົດ Return.
extensions-no-match-detail = ລອງຊື່ອື່ນ ຫຼື ID ສ່ວນຂະຫຍາຍ.
extensions-on = ສຸດ
extensions-off = ປິດ
extensions-enable-confirm = ເປີດໃຊ້ { $name } ບໍ?
extensions-enable-permissions = ເປີດໃຊ້ { $name } ແລະອະນຸຍາດໃຫ້:

lsp-title = ເຊີບເວີພາສາ
lsp-search = ຄົ້ນຫາເຊີບເວີພາສາ, ຕົວພິມນ້ອຍ, ຮູບແບບ...
lsp-loading = ກຳລັງໂຫຼດລາຍການ...
lsp-empty = ບໍ່ມີເຊີບເວີພາສາທີ່ກົງກັນ
lsp-empty-detail = ລອງໃຊ້ພາສາອື່ນ, ພາສາຂຽນ, ຫຼືຕົວຈັດຮູບແບບ.
lsp-needs = ຕ້ອງການ { $tool }
lsp-status-available = ມີໃຫ້
lsp-status-on-path = ໃນ PATH
lsp-status-installing = ກຳລັງຕິດຕັ້ງ...
lsp-status-installed = ຕິດຕັ້ງແລ້ວ
lsp-status-outdated = ມີອັບເດດແລ້ວ
lsp-status-running = ແລ່ນ
lsp-status-failed = ລົ້ມເຫລວ

spaces-title = ພື້ນທີ່
spaces-new-placeholder = ຊື່ຊ່ອງໃໝ່
spaces-empty = ບໍ່ມີຊ່ອງຫວ່າງ
spaces-default-name = ພື້ນທີ່ { $number }
spaces-tabs = { $count ->
    [one] 1 ແຖບ
   *[other] { $count } ແຖບ
}
spaces-delete = ລຶບພື້ນທີ່

team-title = ທີມງານ
team-just-you = ພຽງແຕ່ທ່ານຢູ່ໃນຊ່ອງນີ້
team-agents = { $count ->
    [one] ເຈົ້າ ແລະ 1 ຕົວແທນ
   *[other] ທ່ານ ແລະ { $count } ຕົວແທນ
}
team-empty = ບໍ່ມີໃຜຢູ່ທີ່ນີ້ເທື່ອ
team-you = ເຈົ້າ
team-agent = ຕົວແທນ

services-title = ບໍລິການພື້ນຫຼັງ
services-processes = { $count ->
    [one] 1 ຂະບວນການ
   *[other] { $count } ຂະບວນການ
}
services-kill-all = ຂ້າທັງໝົດ
services-not-running = ບໍລິການບໍ່ເຮັດວຽກ
services-start-with = ເລີ່ມຕົ້ນດ້ວຍ:
services-empty = ບໍ່ມີຂະບວນການເຄື່ອນໄຫວ
services-filter = ຂະບວນການກັ່ນຕອງ...
services-no-match = ບໍ່ມີຂະບວນການທີ່ກົງກັນ
services-connected = ເຊື່ອມຕໍ່
services-disconnected = ຕັດການເຊື່ອມຕໍ່
services-attached = ຕິດ
services-kill = ຂ້າ
services-memory = ຄວາມຊົງຈໍາ
services-size = ຂະໜາດ
services-shell = ແກະ

error-title = ຜິດພາດ

history-search = ປະຫວັດການຄົ້ນຫາ
history-clear-all = ລຶບລ້າງທັງໝົດ
history-clear-confirm = ລຶບລ້າງປະຫວັດທັງໝົດບໍ?
history-clear-warning = ອັນນີ້ບໍ່ສາມາດຍົກເລີກໄດ້.
history-cancel = ຍົກເລີກ
history-today = ມື້ນີ້
history-yesterday = ມື້ວານນີ້
history-days-ago = { $count } ມື້ກ່ອນ
history-day-offset = ມື້ -{ $count }

settings-title = ການຕັ້ງຄ່າ
settings-loading = ກຳລັງໂຫຼດການຕັ້ງຄ່າ...
settings-stored = ເກັບໄວ້ໃນ ~/.vmux/settings.ron
settings-other = ອື່ນໆ
settings-software-update = ອັບເດດຊອບແວ
settings-check-updates = ກວດສອບການອັບເດດ
settings-check-updates-hint = ກວດສອບອັດຕະໂນມັດເມື່ອເປີດຕົວ ແລະທຸກໆຊົ່ວໂມງເມື່ອເປີດໃຊ້ການອັບເດດອັດຕະໂນມັດ.
settings-update-unavailable = ບໍ່ສາມາດໃຊ້ໄດ້
settings-update-unavailable-hint = ຕົວອັບເດດບໍ່ໄດ້ລວມຢູ່ໃນການກໍ່ສ້າງນີ້.
settings-update-checking = ກຳລັງກວດສອບ...
settings-update-checking-hint = ກຳລັງກວດສອບການອັບເດດ...
settings-update-check-again = ກວດເບິ່ງອີກຄັ້ງ
settings-update-current = Vmux ອັບເດດແລ້ວ.
settings-update-downloading = ກຳລັງດາວໂຫຼດ...
settings-update-downloading-hint = ກຳລັງດາວໂຫຼດ Vmux { $version }…
settings-update-installing = ກຳລັງຕິດຕັ້ງ...
settings-update-installing-hint = ກຳລັງຕິດຕັ້ງ Vmux { $version }…
settings-update-ready = ອັບເດດພ້ອມແລ້ວ
settings-update-ready-hint = Vmux { $version } ພ້ອມແລ້ວ. ຣີສະຕາດເພື່ອນຳໃຊ້ມັນ.
settings-update-try-again = ລອງອີກຄັ້ງ
settings-update-failed = ບໍ່ສາມາດກວດສອບການອັບເດດໄດ້.
settings-item = ລາຍການ
settings-item-number = ລາຍການ { $number }
settings-press-key = ກົດປຸ່ມ…
settings-saved = ບັນທຶກແລ້ວ
settings-record-key = ຄລິກເພື່ອບັນທຶກປຸ່ມຄອມໂບໃໝ່

tray-open-window = ເປີດປ່ອງຢ້ຽມ
tray-close-window = ປິດໜ້າຈໍ
tray-pause-recording = ຢຸດການບັນທຶກຊົ່ວຄາວ
tray-resume-recording = ສືບຕໍ່ການບັນທຶກ
tray-finish-recording = ສຳເລັດການບັນທຶກ
tray-quit = ອອກຈາກ Vmux

composer-attach-files = ແນບໄຟລ໌ (/upload)
composer-remove-attachment = ເອົາໄຟລ໌ແນບ

layout-back = ກັບຄືນໄປບ່ອນ
layout-forward = ສົ່ງຕໍ່
layout-reload = ໂຫຼດໃໝ່
layout-bookmark-page = ບຸກມາກໜ້ານີ້
layout-remove-bookmark = ເອົາບຸກມາກອອກ
layout-pin-page = ປັກໝຸດໜ້ານີ້
layout-unpin-page = ຖອນປັກໝຸດໜ້ານີ້
layout-manage-extensions = ຈັດການສ່ວນຂະຫຍາຍ
layout-new-stack = Stack ໃໝ່
layout-close-tab = ປິດແຖບ
layout-bookmark = ບຸກມາກ
layout-pin = ປັກໝຸດ
layout-new-tab = ແຖບໃໝ່
layout-team = ທີມງານ

command-switch-space = ສະຫຼັບພື້ນທີ່…
command-search-ask = ຊອກຫາ ຫຼືຖາມ...
command-new-tab-placeholder = ຊອກຫາ ຫຼືພິມ URL, ຫຼືເລືອກ Terminal...
command-placeholder = ພິມ URL, ແຖບຊອກຫາ, ຫຼື > ສໍາລັບຄໍາສັ່ງ...
command-composer-placeholder = ພິມ / ສໍາລັບຄໍາສັ່ງຫຼື @ ສໍາລັບສື່
command-send = ສົ່ງ (Enter)
command-terminal = ສະຖານີ
command-open-terminal = ເປີດໃນ Terminal
command-stack = stack
command-tabs = { $count ->
    [one] 1 ແຖບ
   *[other] { $count } ແຖບ
}
command-prompt = ເຕືອນ
command-new-tab = ແຖບໃໝ່
command-search = ຊອກຫາ
command-open-value = ເປີດ “{ $value }”
command-search-value = ຊອກຫາ “{ $value }”

schema-appearance = ຮູບລັກສະນະ
schema-general = ທົ່ວໄປ
schema-layout = ແຜນຜັງ
schema-layout-detail = ປ່ອງຢ້ຽມ, ແຖບ, ແຖບດ້ານຂ້າງ, ແລະວົງການໂຟກັສ.
schema-agent = ຕົວແທນ
schema-agent-detail = ພຶດຕິກໍາຂອງຕົວແທນແລະການອະນຸຍາດເຄື່ອງມື.
schema-shortcuts = ທາງລັດ
schema-shortcuts-detail = ມຸມມອງແບບອ່ານເທົ່ານັ້ນ. ແກ້ໄຂ settings.ron ໂດຍກົງເພື່ອປ່ຽນການຜູກມັດ.
schema-terminal = ສະຖານີ
schema-browser = ຕົວທ່ອງເວັບ
schema-mode = ໂໝດ
schema-mode-detail = ໂຄງການສີສໍາລັບຫນ້າເວັບ. ອຸປະກອນປະຕິບັດຕາມລະບົບຂອງທ່ານ.
schema-device = ອຸປະກອນ
schema-light = ແສງສະຫວ່າງ
schema-dark = ມືດ
schema-language = ພາສາ
schema-language-detail = ໃຊ້ລະບົບ, en-US, ja, ຫຼືແທັກ BCP 47 ໃດໆກໍຕາມທີ່ມີແຄັດຕາລັອກ ~/.vmux/locales/<tag>.ftl ທີ່ກົງກັນ.
schema-auto-update = ອັບເດດອັດຕະໂນມັດ
schema-auto-update-detail = ກວດເບິ່ງແລະຕິດຕັ້ງການອັບເດດກ່ຽວກັບການເປີດຕົວແລະທຸກໆຊົ່ວໂມງ.
schema-startup-url = ການເລີ່ມຕົ້ນ URL
schema-startup-url-detail = ຫວ່າງເປົ່າເປີດແຖບຄໍາສັ່ງ prompt.
schema-search-engine = ເຄື່ອງຈັກຊອກຫາ
schema-search-engine-detail = ໃຊ້ສໍາລັບການຄົ້ນຫາເວັບໄຊຕ໌ຈາກ Start ແລະແຖບຄໍາສັ່ງ.
schema-window = ປ່ອງຢ້ຽມ
schema-pane = ແຖບ
schema-side-sheet = ແຜ່ນຂ້າງ
schema-focus-ring = ວົງການສຸມໃສ່
schema-run-placement = ອະນຸຍາດໃຫ້ມີການລົບລ້າງການຈັດວາງ
schema-run-placement-detail = ໃຫ້ຕົວແທນເລືອກຮູບແບບການແລ່ນ, ທິດທາງ, ແລະສະມໍ.
schema-leader = ຜູ້ນໍາ
schema-leader-detail = ຄຳນຳໜ້າສຳລັບທາງລັດ chord.
schema-chord-timeout = ໝົດເວລາຂອງ Chord
schema-chord-timeout-detail = ມິນລິວິນາທີກ່ອນທີ່ຄຳນຳໜ້າ chord ຈະໝົດອາຍຸ.
schema-bindings = ຜູກມັດ
schema-confirm-close = ຢືນຢັນປິດ
schema-confirm-close-detail = ເຕືອນກ່ອນທີ່ຈະປິດ terminal ທີ່ມີຂະບວນການແລ່ນ.
schema-default-theme = ຮູບແບບສີສັນເລີ່ມຕົ້ນ
schema-default-theme-detail = ຊື່ຂອງຮູບແບບສີສັນທີ່ໃຊ້ໄດ້ຈາກລາຍຊື່ຫົວຂໍ້.
