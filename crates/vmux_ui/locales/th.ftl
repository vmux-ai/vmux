locale-name = ไทย
common-open = เปิด
common-close = ปิด
common-install = ติดตั้ง
common-uninstall = ถอนการติดตั้ง
common-update = อัปเดต
common-retry = ลองอีกครั้ง
common-refresh = รีเฟรช
common-remove = เอาออก
common-enable = เปิดใช้
common-disable = ปิดใช้
common-new = ใหม่
common-active = ทำงานอยู่
common-running = กำลังทำงาน
common-done = เสร็จแล้ว
common-failed = ล้มเหลว
common-installed = ติดตั้งแล้ว
common-items = { $count ->
    [one] { $count } รายการ
   *[other] { $count } รายการ
}
start-title = เริ่มต้น
start-tagline = พรอมป์เดียว ทำได้ทุกอย่าง

agents-title = เอเจนต์
agents-search = ค้นหาเอเจนต์ ACP และ CLI…
agents-empty = ไม่พบเอเจนต์ที่ตรงกัน
agents-empty-detail = ลองค้นหาชื่อ รันไทม์ หรือ ACP/CLI
agents-install-failed = ติดตั้งไม่สำเร็จ
agents-updating = กำลังอัปเดต…
agents-retrying = กำลังลองอีกครั้ง…
agents-preparing = กำลังเตรียม…

extensions-title = ส่วนขยาย
extensions-search = ค้นหาที่ติดตั้งแล้วหรือใน Chrome Web Store…
extensions-relaunch = เปิดใหม่เพื่อใช้การเปลี่ยนแปลง
extensions-empty = ยังไม่มีส่วนขยายที่ติดตั้ง
extensions-no-match = ไม่พบส่วนขยายที่ตรงกัน
extensions-empty-detail = ค้นหาใน Chrome Web Store ด้านบน แล้วกด Enter
extensions-no-match-detail = ลองใช้ชื่ออื่นหรือ ID ส่วนขยาย
extensions-on = เปิด
extensions-off = ปิด
extensions-enable-confirm = เปิดใช้ { $name }?
extensions-enable-permissions = เปิดใช้ { $name } และอนุญาต:

lsp-title = เซิร์ฟเวอร์ภาษา
lsp-search = ค้นหาเซิร์ฟเวอร์ภาษา ลินเตอร์ ตัวจัดรูปแบบ…
lsp-loading = กำลังโหลดแค็ตตาล็อก…
lsp-empty = ไม่พบเซิร์ฟเวอร์ภาษาที่ตรงกัน
lsp-empty-detail = ลองภาษา ลินเตอร์ หรือตัวจัดรูปแบบอื่น
lsp-needs = ต้องใช้ { $tool }
lsp-status-available = พร้อมใช้งาน
lsp-status-on-path = อยู่ใน PATH
lsp-status-installing = กำลังติดตั้ง…
lsp-status-installed = ติดตั้งแล้ว
lsp-status-outdated = มีอัปเดต
lsp-status-running = กำลังทำงาน
lsp-status-failed = ล้มเหลว

spaces-title = เวิร์กสเปซ
spaces-new-placeholder = ชื่อเวิร์กสเปซใหม่
spaces-empty = ไม่มีเวิร์กสเปซ
spaces-default-name = เวิร์กสเปซ { $number }
spaces-tabs = { $count ->
    [one] 1 แท็บ
   *[other] { $count } แท็บ
}
spaces-delete = ลบเวิร์กสเปซ

team-title = ทีม
team-just-you = มีแค่คุณในเวิร์กสเปซนี้
team-agents = { $count ->
    [one] คุณและเอเจนต์ 1 ตัว
   *[other] คุณและเอเจนต์ { $count } ตัว
}
team-empty = ยังไม่มีใครอยู่ที่นี่
team-you = คุณ
team-agent = เอเจนต์

services-title = บริการเบื้องหลัง
services-processes = { $count ->
    [one] 1 โปรเซส
   *[other] { $count } โปรเซส
}
services-kill-all = บังคับหยุดทั้งหมด
services-not-running = บริการไม่ได้ทำงาน
services-start-with = เริ่มด้วย:
services-empty = ไม่มีโปรเซสที่ทำงานอยู่
services-filter = กรองโปรเซส…
services-no-match = ไม่พบโปรเซสที่ตรงกัน
services-connected = เชื่อมต่อแล้ว
services-disconnected = ตัดการเชื่อมต่อแล้ว
services-attached = แนบอยู่
services-kill = บังคับหยุด
services-memory = หน่วยความจำ
services-size = ขนาด
services-shell = เชลล์

error-title = ข้อผิดพลาด

history-search = ค้นหาประวัติ
history-clear-all = ล้างทั้งหมด
history-clear-confirm = ล้างประวัติทั้งหมด?
history-clear-warning = การดำเนินการนี้ไม่สามารถเลิกทำได้
history-cancel = ยกเลิก
history-today = วันนี้
history-yesterday = เมื่อวาน
history-days-ago = { $count } วันที่แล้ว
history-day-offset = วันที่ -{ $count }

settings-title = การตั้งค่า
settings-loading = กำลังโหลดการตั้งค่า…
settings-stored = เก็บไว้ใน ~/.vmux/settings.ron
settings-other = อื่นๆ
settings-software-update = อัปเดตซอฟต์แวร์
settings-check-updates = ตรวจหาอัปเดต
settings-check-updates-hint = ตรวจอัตโนมัติเมื่อเปิดแอปและทุกชั่วโมงเมื่อเปิดอัปเดตอัตโนมัติ
settings-update-unavailable = ไม่พร้อมใช้งาน
settings-update-unavailable-hint = บิลด์นี้ไม่มีตัวอัปเดต
settings-update-checking = กำลังตรวจสอบ…
settings-update-checking-hint = กำลังตรวจหาอัปเดต…
settings-update-check-again = ตรวจอีกครั้ง
settings-update-current = Vmux เป็นเวอร์ชันล่าสุดแล้ว
settings-update-downloading = กำลังดาวน์โหลด…
settings-update-downloading-hint = กำลังดาวน์โหลด Vmux { $version }…
settings-update-installing = กำลังติดตั้ง…
settings-update-installing-hint = กำลังติดตั้ง Vmux { $version }…
settings-update-ready = อัปเดตพร้อมติดตั้ง
settings-update-ready-hint = Vmux { $version } พร้อมแล้ว รีสตาร์ทเพื่อใช้งาน
settings-update-try-again = ลองอีกครั้ง
settings-update-failed = ตรวจหาอัปเดตไม่ได้
settings-item = รายการ
settings-item-number = รายการ { $number }
settings-press-key = กดปุ่ม…
settings-saved = บันทึกแล้ว
settings-record-key = คลิกเพื่อบันทึกคีย์ลัดใหม่

tray-open-window = เปิดหน้าต่าง
tray-close-window = ปิดหน้าต่าง
tray-pause-recording = พักการบันทึก
tray-resume-recording = บันทึกต่อ
tray-finish-recording = เสร็จสิ้นการบันทึก
tray-quit = ออกจาก Vmux

composer-attach-files = แนบไฟล์ (/upload)
composer-remove-attachment = เอาไฟล์แนบออก

layout-back = ย้อนกลับ
layout-forward = ไปข้างหน้า
layout-reload = โหลดใหม่
layout-bookmark-page = บุ๊กมาร์กหน้านี้
layout-remove-bookmark = เอาบุ๊กมาร์กออก
layout-pin-page = ปักหมุดหน้านี้
layout-unpin-page = เลิกปักหมุดหน้านี้
layout-manage-extensions = จัดการส่วนขยาย
layout-new-stack = สแต็กใหม่
layout-close-tab = ปิดแท็บ
layout-bookmark = บุ๊กมาร์ก
layout-pin = ปักหมุด
layout-new-tab = แท็บใหม่
layout-team = ทีม

command-switch-space = สลับเวิร์กสเปซ…
command-search-ask = ค้นหาหรือถาม…
command-new-tab-placeholder = ค้นหาหรือพิมพ์ URL หรือเลือกเทอร์มินัล…
command-placeholder = พิมพ์ URL ค้นหาแท็บ หรือพิมพ์ > เพื่อใช้คำสั่ง…
command-composer-placeholder = พิมพ์ / เพื่อใช้คำสั่ง หรือ @ เพื่อเพิ่มสื่อ
command-send = ส่ง (Enter)
command-terminal = เทอร์มินัล
command-open-terminal = เปิดในเทอร์มินัล
command-stack = สแต็ก
command-tabs = { $count ->
    [one] 1 แท็บ
   *[other] { $count } แท็บ
}
command-prompt = พรอมป์
command-new-tab = แท็บใหม่
command-search = ค้นหา
command-open-value = เปิด “{ $value }”
command-search-value = ค้นหา “{ $value }”

schema-appearance = รูปลักษณ์
schema-general = ทั่วไป
schema-layout = เลย์เอาต์
schema-layout-detail = หน้าต่าง พื้นที่แบ่ง แถบด้านข้าง และเส้นโฟกัส
schema-agent = เอเจนต์
schema-agent-detail = พฤติกรรมของเอเจนต์และสิทธิ์ใช้เครื่องมือ
schema-shortcuts = คีย์ลัด
schema-shortcuts-detail = มุมมองอ่านอย่างเดียว แก้การผูกคีย์โดยตรงใน settings.ron
schema-terminal = เทอร์มินัล
schema-browser = เบราว์เซอร์
schema-mode = โหมด
schema-mode-detail = ชุดสีสำหรับหน้าเว็บ อุปกรณ์จะใช้ตามระบบของคุณ
schema-device = อุปกรณ์
schema-light = สว่าง
schema-dark = มืด
schema-language = ภาษา
schema-language-detail = ใช้ภาษาระบบ, en-US, ja หรือแท็ก BCP 47 ใดก็ได้ที่มีแค็ตตาล็อก ~/.vmux/locales/<tag>.ftl ตรงกัน
schema-auto-update = อัปเดตอัตโนมัติ
schema-auto-update-detail = ตรวจหาและติดตั้งอัปเดตเมื่อเปิดแอปและทุกชั่วโมง
schema-startup-url = URL เริ่มต้น
schema-startup-url-detail = หากเว้นว่าง จะเปิดพรอมป์แถบคำสั่ง
schema-search-engine = เครื่องมือค้นหา
schema-search-engine-detail = ใช้สำหรับค้นหาเว็บจากหน้าเริ่มต้นและแถบคำสั่ง
schema-window = หน้าต่าง
schema-pane = พื้นที่แบ่ง
schema-side-sheet = แผงด้านข้าง
schema-focus-ring = เส้นโฟกัส
schema-run-placement = อนุญาตให้กำหนดตำแหน่งรันทับค่าเดิม
schema-run-placement-detail = ให้เอเจนต์เลือกโหมดพื้นที่รัน ทิศทาง และจุดยึดได้
schema-leader = คีย์นำ
schema-leader-detail = คีย์นำหน้าสำหรับคีย์ลัดแบบคอร์ด
schema-chord-timeout = เวลาหมดอายุคอร์ด
schema-chord-timeout-detail = จำนวนมิลลิวินาทีก่อนคีย์นำหน้าของคอร์ดหมดอายุ
schema-bindings = การผูกคีย์
schema-confirm-close = ยืนยันก่อนปิด
schema-confirm-close-detail = ถามก่อนปิดเทอร์มินัลที่มีโปรเซสกำลังทำงาน
schema-default-theme = ธีมเริ่มต้น
schema-default-theme-detail = ชื่อธีมที่ใช้งานอยู่จากรายการธีม
