common-open = เปิด
common-close = ปิด
common-install = ติดตั้ง
common-uninstall = ถอนการติดตั้ง
common-update = อัปเดต
common-retry = ลองใหม่
common-refresh = รีเฟรช
common-remove = ลบ
common-enable = เปิดใช้งาน
common-disable = ปิดใช้งาน
common-new = ใหม่
common-active = ใช้งานอยู่
common-running = กำลังทำงาน
common-done = เสร็จแล้ว
common-failed = ล้มเหลว
common-installed = ติดตั้งแล้ว
common-items = { $count ->
    [one] { $count } รายการ
   *[other] { $count } รายการ
}
start-title = เริ่มต้น
start-tagline = หนึ่งคำสั่ง. ทุกอย่าง, เสร็จสิ้น.

agents-title = ตัวแทน
agents-search = ค้นหาตัวแทน ACP และ CLI…
agents-empty = ไม่พบตัวแทนที่ตรงกัน
agents-empty-detail = ลองค้นด้วยชื่อ, รันไทม์ หรือ ACP/CLI
agents-install-failed = ติดตั้งล้มเหลว
agents-updating = กำลังอัปเดต…
agents-retrying = กำลังลองใหม่…
agents-preparing = กำลังเตรียม…

extensions-title = ส่วนขยาย
extensions-search = ค้นหาที่ติดตั้งหรือ Chrome Web Store…
extensions-relaunch = รีสตาร์ทเพื่อใช้งาน
extensions-empty = ไม่มีส่วนขยายที่ติดตั้ง
extensions-no-match = ไม่พบส่วนขยายที่ตรงกัน
extensions-empty-detail = ค้นหาใน Chrome Web Store ด้านบนแล้วกด Return
extensions-no-match-detail = ลองชื่ออื่นหรือ ID ส่วนขยาย
extensions-on = เปิด
extensions-off = ปิด
extensions-enable-confirm = เปิดใช้งาน { $name }?
extensions-enable-permissions = เปิดใช้งาน { $name } และอนุญาต:

lsp-title = เซิร์ฟเวอร์ภาษา
lsp-search = ค้นหาเซิร์ฟเวอร์ภาษา, ลินเตอร์, ฟอร์แมตเตอร์…
lsp-loading = กำลังโหลดแคตตาล็อก…
lsp-empty = ไม่พบเซิร์ฟเวอร์ภาษาที่ตรงกัน
lsp-empty-detail = ลองภาษา, ลินเตอร์ หรือฟอร์แมตเตอร์อื่น
lsp-needs = ต้องการ { $tool }
lsp-status-available = พร้อมใช้งาน
lsp-status-on-path = อยู่ใน PATH
lsp-status-installing = กำลังติดตั้ง…
lsp-status-installed = ติดตั้งแล้ว
lsp-status-outdated = มีการอัปเดต
lsp-status-running = กำลังทำงาน
lsp-status-failed = ล้มเหลว

spaces-title = พื้นที่
spaces-new-placeholder = ชื่อพื้นที่ใหม่
spaces-empty = ไม่มีพื้นที่
spaces-default-name = พื้นที่ { $number }
spaces-tabs = { $count ->
    [one] 1 แท็บ
   *[other] { $count } แท็บ
}
spaces-delete = ลบพื้นที่

team-title = ทีม
team-just-you = แค่คุณในพื้นที่นี้
team-agents = { $count ->
    [one] คุณและ 1 ตัวแทน
   *[other] คุณและ { $count } ตัวแทน
}
team-empty = ยังไม่มีใครที่นี่
team-you = คุณ
team-agent = ตัวแทน

services-title = บริการพื้นหลัง
services-processes = { $count ->
    [one] 1 กระบวนการ
   *[other] { $count } กระบวนการ
}
services-kill-all = หยุดทั้งหมด
services-not-running = บริการไม่ได้ทำงาน
services-start-with = เริ่มด้วย:
services-empty = ไม่มีกระบวนการที่ใช้งานอยู่
services-filter = กรองกระบวนการ…
services-no-match = ไม่พบกระบวนการที่ตรงกัน
services-connected = เชื่อมต่อแล้ว
services-disconnected = ตัดการเชื่อมต่อ
services-attached = แนบแล้ว
services-kill = หยุด
services-memory = หน่วยความจำ
services-size = ขนาด
services-shell = เชลล์

error-title = ข้อผิดพลาด

history-search = ค้นหาประวัติ
history-clear-all = ล้างทั้งหมด
history-clear-confirm = ล้างประวัติทั้งหมด?
history-clear-warning = การกระทำนี้ไม่สามารถยกเลิกได้
history-cancel = ยกเลิก
history-today = วันนี้
history-yesterday = เมื่อวาน
history-days-ago = { $count } วันที่แล้ว
history-day-offset = วัน -{ $count }

settings-title = การตั้งค่า
settings-loading = กำลังโหลดการตั้งค่า…
settings-stored = จัดเก็บใน ~/.vmux/settings.ron
settings-other = อื่นๆ
settings-software-update = อัปเดตซอฟต์แวร์
settings-check-updates = ตรวจสอบการอัปเดต
settings-check-updates-hint = ตรวจสอบอัตโนมัติเมื่อเปิดใช้งานและทุกชั่วโมงเมื่อเปิดใช้ Auto-update
settings-update-unavailable = ไม่พร้อมใช้งาน
settings-update-unavailable-hint = ตัวอัปเดตไม่รวมอยู่ในบิลด์นี้
settings-update-checking = กำลังตรวจสอบ…
settings-update-checking-hint = กำลังตรวจสอบการอัปเดต…
settings-update-check-again = ตรวจสอบอีกครั้ง
settings-update-current = Vmux เป็นเวอร์ชันล่าสุดแล้ว
settings-update-downloading = กำลังดาวน์โหลด…
settings-update-downloading-hint = กำลังดาวน์โหลด Vmux { $version }…
settings-update-installing = กำลังติดตั้ง…
settings-update-installing-hint = กำลังติดตั้ง Vmux { $version }…
settings-update-ready = พร้อมอัปเดต
settings-update-ready-hint = Vmux { $version } พร้อมแล้ว รีสตาร์ทเพื่อใช้งาน
settings-update-try-again = ลองอีกครั้ง
settings-update-failed = ไม่สามารถตรวจสอบการอัปเดตได้
settings-item = รายการ
settings-item-number = รายการ { $number }
settings-press-key = กดปุ่ม…
settings-saved = บันทึกแล้ว
settings-record-key = คลิกเพื่อบันทึกชุดปุ่มใหม่

tray-open-window = เปิดหน้าต่าง
tray-close-window = ปิดหน้าต่าง
tray-pause-recording = หยุดการบันทึกชั่วคราว
tray-resume-recording = ดำเนินการบันทึกต่อ
tray-finish-recording = สิ้นสุดการบันทึก
tray-quit = ออกจาก Vmux

composer-attach-files = แนบไฟล์ (/upload)
composer-remove-attachment = ลบไฟล์แนบ

layout-back = ย้อนกลับ
layout-forward = ไปข้างหน้า
layout-reload = โหลดใหม่
layout-bookmark-page = บุ๊กมาร์กหน้านี้
layout-remove-bookmark = ลบบุ๊กมาร์ก
layout-pin-page = ปักหมุดหน้านี้
layout-unpin-page = เลิกปักหมุดหน้านี้
layout-manage-extensions = จัดการส่วนขยาย
layout-new-stack = สแตกใหม่
layout-close-tab = ปิดแท็บ
layout-bookmark = บุ๊กมาร์ก
layout-pin = ปักหมุด
layout-new-tab = แท็บใหม่
layout-team = ทีม

command-switch-space = สลับพื้นที่…
command-search-ask = ค้นหาหรือถาม…
command-new-tab-placeholder = ค้นหาหรือพิมพ์ URL หรือเลือก Terminal…
command-placeholder = พิมพ์ URL ค้นหาแท็บ หรือ > สำหรับคำสั่ง…
command-composer-placeholder = พิมพ์ / สำหรับคำสั่งหรือ @ สำหรับมีเดีย
command-send = ส่ง (Enter)
command-terminal = Terminal
command-open-terminal = เปิดใน Terminal
command-stack = สแตก
command-tabs = { $count ->
    [one] 1 แท็บ
   *[other] { $count } แท็บ
}
command-prompt = พรอมต์
command-new-tab = แท็บใหม่
command-search = ค้นหา
command-open-value = เปิด "{ $value }"
command-search-value = ค้นหา "{ $value }"

schema-appearance = รูปลักษณ์
schema-general = ทั่วไป
schema-layout = เลย์เอาต์
schema-layout-detail = หน้าต่าง, บานหน้าต่าง, แถบด้านข้าง และวงโฟกัส
schema-agent = ตัวแทน
schema-agent-detail = พฤติกรรมตัวแทนและสิทธิ์เครื่องมือ
schema-shortcuts = ทางลัด
schema-shortcuts-detail = มุมมองอ่านอย่างเดียว แก้ไข settings.ron โดยตรงเพื่อเปลี่ยนการผูกปุ่ม
schema-terminal = Terminal
schema-browser = เบราว์เซอร์
schema-mode = โหมด
schema-mode-detail = ชุดสีสำหรับหน้าเว็บ อุปกรณ์ตามระบบของคุณ
schema-device = อุปกรณ์
schema-light = สว่าง
schema-dark = มืด
schema-language = ภาษา
schema-language-detail = ใช้ระบบ, en-US, ja หรือแท็ก BCP 47 ใดก็ได้พร้อมแคตตาล็อก ~/.vmux/locales/<tag>.ftl ที่ตรงกัน
schema-auto-update = อัปเดตอัตโนมัติ
schema-auto-update-detail = ตรวจสอบและติดตั้งการอัปเดตเมื่อเปิดใช้งานและทุกชั่วโมง
schema-startup-url = URL เริ่มต้น
schema-startup-url-detail = เว้นว่างเพื่อเปิดพรอมต์แถบคำสั่ง
schema-search-engine = เครื่องมือค้นหา
schema-search-engine-detail = ใช้สำหรับการค้นหาเว็บจาก Start และแถบคำสั่ง
schema-window = หน้าต่าง
schema-pane = บานหน้าต่าง
schema-side-sheet = แผ่นด้านข้าง
schema-focus-ring = วงโฟกัส
schema-run-placement = อนุญาตให้แทนที่การวางตำแหน่งรัน
schema-run-placement-detail = ให้ตัวแทนเลือกโหมดบานหน้าต่างรัน, ทิศทาง และจุดยึด
schema-leader = ปุ่มนำ
schema-leader-detail = ปุ่มนำหน้าสำหรับทางลัดแบบคอร์ด
schema-chord-timeout = หมดเวลาคอร์ด
schema-chord-timeout-detail = มิลลิวินาทีก่อนที่คำนำหน้าคอร์ดจะหมดอายุ
schema-bindings = การผูกปุ่ม
schema-confirm-close = ยืนยันการปิด
schema-confirm-close-detail = แจ้งเตือนก่อนปิด Terminal ที่มีกระบวนการทำงานอยู่
schema-default-theme = ธีมเริ่มต้น
schema-default-theme-detail = ชื่อธีมที่ใช้งานอยู่จากรายการธีม
