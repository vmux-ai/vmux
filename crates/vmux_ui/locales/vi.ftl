common-open = Mở
common-close = Đóng
common-install = Cài đặt
common-uninstall = Gỡ cài đặt
common-update = Cập nhật
common-retry = Thử lại
common-refresh = Làm mới
common-remove = Xóa
common-enable = Bật
common-disable = Tắt
common-new = Mới
common-active = đang hoạt động
common-running = đang chạy
common-done = xong
common-failed = Thất bại
common-installed = Đã cài đặt
common-items = { $count ->
    [one] { $count } mục
   *[other] { $count } mục
}
start-title = Bắt đầu
start-tagline = Một lệnh. Bất cứ điều gì, xong ngay.

agents-title = Tác nhân
agents-search = Tìm kiếm tác nhân ACP và CLI…
agents-empty = Không tìm thấy tác nhân
agents-empty-detail = Thử tên, runtime, hoặc ACP/CLI.
agents-install-failed = Cài đặt thất bại
agents-updating = Đang cập nhật…
agents-retrying = Đang thử lại…
agents-preparing = Đang chuẩn bị…

extensions-title = Tiện ích mở rộng
extensions-search = Tìm kiếm đã cài đặt hoặc Chrome Web Store…
extensions-relaunch = Khởi động lại để áp dụng
extensions-empty = Chưa cài đặt tiện ích nào
extensions-no-match = Không tìm thấy tiện ích
extensions-empty-detail = Tìm kiếm Chrome Web Store ở trên và nhấn Return.
extensions-no-match-detail = Thử tên khác hoặc ID tiện ích.
extensions-on = Bật
extensions-off = Tắt
extensions-enable-confirm = Bật { $name }?
extensions-enable-permissions = Bật { $name } và cho phép:

lsp-title = Máy chủ ngôn ngữ
lsp-search = Tìm kiếm máy chủ ngôn ngữ, linter, formatter…
lsp-loading = Đang tải danh mục…
lsp-empty = Không tìm thấy máy chủ ngôn ngữ
lsp-empty-detail = Thử ngôn ngữ, linter, hoặc formatter khác.
lsp-needs = cần { $tool }
lsp-status-available = Có sẵn
lsp-status-on-path = Trong PATH
lsp-status-installing = Đang cài đặt…
lsp-status-installed = Đã cài đặt
lsp-status-outdated = Có bản cập nhật
lsp-status-running = Đang chạy
lsp-status-failed = Thất bại

spaces-title = Không gian
spaces-new-placeholder = Tên không gian mới
spaces-empty = Không có không gian
spaces-default-name = Không gian { $number }
spaces-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
spaces-delete = Xóa không gian

team-title = Nhóm
team-just-you = Chỉ có bạn trong không gian này
team-agents = { $count ->
    [one] Bạn và 1 tác nhân
   *[other] Bạn và { $count } tác nhân
}
team-empty = Chưa có ai ở đây
team-you = Bạn
team-agent = Tác nhân

services-title = Dịch vụ nền
services-processes = { $count ->
    [one] 1 tiến trình
   *[other] { $count } tiến trình
}
services-kill-all = Dừng tất cả
services-not-running = Dịch vụ không chạy
services-start-with = Khởi động với:
services-empty = Không có tiến trình nào
services-filter = Lọc tiến trình…
services-no-match = Không tìm thấy tiến trình
services-connected = Đã kết nối
services-disconnected = Đã ngắt kết nối
services-attached = đã gắn
services-kill = Dừng
services-memory = Bộ nhớ
services-size = Kích thước
services-shell = Shell

error-title = Lỗi

history-search = Tìm kiếm lịch sử
history-clear-all = Xóa tất cả
history-clear-confirm = Xóa toàn bộ lịch sử?
history-clear-warning = Không thể hoàn tác.
history-cancel = Hủy
history-today = Hôm nay
history-yesterday = Hôm qua
history-days-ago = { $count } ngày trước
history-day-offset = Ngày -{ $count }

settings-title = Cài đặt
settings-loading = Đang tải cài đặt…
settings-stored = Lưu trong ~/.vmux/settings.ron
settings-other = Khác
settings-software-update = Cập nhật phần mềm
settings-check-updates = Kiểm tra cập nhật
settings-check-updates-hint = Tự động kiểm tra khi khởi động và mỗi giờ khi Tự động cập nhật được bật.
settings-update-unavailable = Không khả dụng
settings-update-unavailable-hint = Trình cập nhật không có trong bản dựng này.
settings-update-checking = Đang kiểm tra…
settings-update-checking-hint = Đang kiểm tra cập nhật…
settings-update-check-again = Kiểm tra lại
settings-update-current = Vmux đã cập nhật.
settings-update-downloading = Đang tải xuống…
settings-update-downloading-hint = Đang tải xuống Vmux { $version }…
settings-update-installing = Đang cài đặt…
settings-update-installing-hint = Đang cài đặt Vmux { $version }…
settings-update-ready = Cập nhật sẵn sàng
settings-update-ready-hint = Vmux { $version } đã sẵn sàng. Khởi động lại để áp dụng.
settings-update-try-again = Thử lại
settings-update-failed = Không thể kiểm tra cập nhật.
settings-item = Mục
settings-item-number = Mục { $number }
settings-press-key = Nhấn một phím…
settings-saved = Đã lưu
settings-record-key = Nhấp để ghi tổ hợp phím mới

tray-open-window = Mở cửa sổ
tray-close-window = Đóng cửa sổ
tray-pause-recording = Tạm dừng ghi
tray-resume-recording = Tiếp tục ghi
tray-finish-recording = Kết thúc ghi
tray-quit = Thoát Vmux

composer-attach-files = Đính kèm tệp (/upload)
composer-remove-attachment = Xóa tệp đính kèm

layout-back = Quay lại
layout-forward = Tiếp theo
layout-reload = Tải lại
layout-bookmark-page = Đánh dấu trang này
layout-remove-bookmark = Xóa đánh dấu
layout-pin-page = Ghim trang này
layout-unpin-page = Bỏ ghim trang này
layout-manage-extensions = Quản lý tiện ích
layout-new-stack = Stack mới
layout-close-tab = Đóng tab
layout-bookmark = Đánh dấu
layout-pin = Ghim
layout-new-tab = Tab mới
layout-team = Nhóm

command-switch-space = Chuyển không gian…
command-search-ask = Tìm kiếm hoặc hỏi…
command-new-tab-placeholder = Tìm kiếm hoặc nhập URL, hoặc chọn Terminal…
command-placeholder = Nhập URL, tìm tab, hoặc > cho lệnh…
command-composer-placeholder = Nhập / cho lệnh hoặc @ cho media
command-send = Gửi (Enter)
command-terminal = Terminal
command-open-terminal = Mở trong Terminal
command-stack = Stack
command-tabs = { $count ->
    [one] 1 tab
   *[other] { $count } tab
}
command-prompt = Lệnh nhắc
command-new-tab = Tab mới
command-search = Tìm kiếm
command-open-value = Mở "{ $value }"
command-search-value = Tìm kiếm "{ $value }"

schema-appearance = Giao diện
schema-general = Chung
schema-layout = Bố cục
schema-layout-detail = Cửa sổ, ngăn, thanh bên và vòng lấy nét.
schema-agent = Tác nhân
schema-agent-detail = Hành vi tác nhân và quyền công cụ.
schema-shortcuts = Phím tắt
schema-shortcuts-detail = Chỉ xem. Chỉnh sửa settings.ron trực tiếp để thay đổi phím gán.
schema-terminal = Terminal
schema-browser = Trình duyệt
schema-mode = Chế độ
schema-mode-detail = Bảng màu cho trang web. Thiết bị theo hệ thống của bạn.
schema-device = Thiết bị
schema-light = Sáng
schema-dark = Tối
schema-language = Ngôn ngữ
schema-language-detail = Dùng system, en-US, ja, hoặc bất kỳ thẻ BCP 47 nào có danh mục ~/.vmux/locales/<tag>.ftl tương ứng.
schema-auto-update = Tự động cập nhật
schema-auto-update-detail = Kiểm tra và cài đặt cập nhật khi khởi động và mỗi giờ.
schema-startup-url = URL khởi động
schema-startup-url-detail = Để trống sẽ mở thanh lệnh.
schema-search-engine = Công cụ tìm kiếm
schema-search-engine-detail = Dùng cho tìm kiếm web từ Bắt đầu và thanh lệnh.
schema-window = Cửa sổ
schema-pane = Ngăn
schema-side-sheet = Bảng bên
schema-focus-ring = Vòng lấy nét
schema-run-placement = Cho phép ghi đè vị trí chạy
schema-run-placement-detail = Cho phép tác nhân chọn chế độ ngăn chạy, hướng và điểm neo.
schema-leader = Leader
schema-leader-detail = Phím tiền tố cho phím tắt tổ hợp.
schema-chord-timeout = Thời gian chờ tổ hợp
schema-chord-timeout-detail = Mili giây trước khi tiền tố tổ hợp hết hạn.
schema-bindings = Phím gán
schema-confirm-close = Xác nhận đóng
schema-confirm-close-detail = Hỏi trước khi đóng terminal có tiến trình đang chạy.
schema-default-theme = Chủ đề mặc định
schema-default-theme-detail = Tên chủ đề đang hoạt động trong danh sách chủ đề.
