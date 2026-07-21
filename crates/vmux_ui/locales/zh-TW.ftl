common-open = 打開
common-close = 關閉
common-install = 安裝
common-uninstall = 解除安裝
common-update = 更新
common-retry = 重試
common-refresh = 重新整理
common-remove = 刪除
common-enable = 啟用
common-disable = 停用
common-new = 新
common-active = 活躍的
common-running = 跑步
common-done = 完成
common-failed = 失敗
common-installed = 已安裝
common-items = { $count ->
    [one] { $count } 項目
   *[other] { $count } 項
}
start-title = 開始
start-tagline = 一提示。任何事情，完成。

agents-title = 代理商
agents-search = 搜尋 ACP 和 CLI 代理程式...
agents-empty = 沒有匹配的代理
agents-empty-detail = 嘗試名稱、執行時期或 ACP/CLI。
agents-install-failed = 安裝失敗
agents-updating = 正在更新…
agents-retrying = 正在重試...
agents-preparing = 正在準備…

extensions-title = 擴充
extensions-search = 搜尋已安裝或 Chrome Web Store...
extensions-relaunch = 重新啟動即可申請
extensions-empty = 沒有安裝擴充
extensions-no-match = 沒有匹配的擴展名
extensions-empty-detail = 搜尋上面的 Chrome Web Store 並按 Return。
extensions-no-match-detail = 嘗試其他名稱或分機 ID。
extensions-on = 開
extensions-off = 關閉
extensions-enable-confirm = 啟用 { $name }？
extensions-enable-permissions = 啟用 { $name } 並允許：

lsp-title = 語言伺服器
lsp-search = 搜尋語言伺服器、linter、格式化程式…
lsp-loading = 正在載入目錄...
lsp-empty = 沒有匹配的語言伺服器
lsp-empty-detail = 嘗試另一種語言、linter 或格式化程式。
lsp-needs = 需要 { $tool }
lsp-status-available = 可用
lsp-status-on-path = 在 PATH 上
lsp-status-installing = 正在安裝...
lsp-status-installed = 已安裝
lsp-status-outdated = 可用更新
lsp-status-running = 跑步
lsp-status-failed = 失敗

spaces-title = 空間
spaces-new-placeholder = 新空間名稱
spaces-empty = 沒有空格
spaces-default-name = 空間 { $number }
spaces-tabs = { $count ->
    [one] 1 個選項卡
   *[other] { $count } 選項卡
}
spaces-delete = 刪除空格

team-title = 團隊
team-just-you = 這個空間裡只有你
team-agents = { $count ->
    [one] 您和 1 位代理人
   *[other] 您和 { $count } 代理
}
team-empty = 這裡還沒有人
team-you = 你
team-agent = 代理商

services-title = 後台服務
services-processes = { $count ->
    [one] 1 進程
   *[other] { $count } 進程
}
services-kill-all = 全部殺死
services-not-running = 服務未運行
services-start-with = 從以下開始：
services-empty = 無活動行程
services-filter = 過濾過程...
services-no-match = 沒有符合的進程
services-connected = 已連接
services-disconnected = 已斷開連接
services-attached = 附上
services-kill = 殺
services-memory = 記憶體
services-size = 尺寸
services-shell = 殼牌

error-title = 錯誤

history-search = 搜尋紀錄
history-clear-all = 全部清除
history-clear-confirm = 清除所有歷史記錄？
history-clear-warning = 此操作無法撤銷。
history-cancel = 取消
history-today = 今天
history-yesterday = 昨天
history-days-ago = { $count } 天前
history-day-offset = 日 -{ $count }

settings-title = 設定
settings-loading = 正在加載設定...
settings-stored = 儲存在 ~/.vmux/settings.ron 中
settings-other = 其他
settings-software-update = 軟體更新
settings-check-updates = 檢查更新
settings-check-updates-hint = 啟動時自動檢查，啟用自動更新時每小時自動檢查一次。
settings-update-unavailable = 不可用
settings-update-unavailable-hint = 此版本中不包含更新程式。
settings-update-checking = 正在檢查...
settings-update-checking-hint = 正在檢查更新...
settings-update-check-again = 再次檢查
settings-update-current = Vmux 是最新的。
settings-update-downloading = 正在下載...
settings-update-downloading-hint = 正在下載 Vmux { $version }...
settings-update-installing = 正在安裝...
settings-update-installing-hint = 正在安裝 Vmux { $version }...
settings-update-ready = 更新就緒
settings-update-ready-hint = Vmux { $version } 已準備就緒。重新啟動即可套用它。
settings-update-try-again = 再試一次
settings-update-failed = 無法檢查更新。
settings-item = 專案
settings-item-number = 項目 { $number }
settings-press-key = 按一個鍵...
settings-saved = 已儲存
settings-record-key = 按一下以記錄新的組合鍵

tray-open-window = 開窗
tray-close-window = 關閉視窗
tray-pause-recording = 暫停錄音
tray-resume-recording = 恢復錄音
tray-finish-recording = 完成錄音
tray-quit = 退出 Vmux

composer-attach-files = 附加文件 (/upload)
composer-remove-attachment = 刪除附件

layout-back = 返回
layout-forward = 前進
layout-reload = 重新載入
layout-bookmark-page = 將此頁加入書籤
layout-remove-bookmark = 刪除書籤
layout-pin-page = 固定此頁面
layout-unpin-page = 取消固定此頁面
layout-manage-extensions = 管理擴充
layout-new-stack = 新堆疊
layout-close-tab = 關閉選項卡
layout-bookmark = 書籤
layout-pin = 插銷
layout-new-tab = 新分頁
layout-team = 團隊

command-switch-space = 切換空間...
command-search-ask = 搜尋或詢問...
command-new-tab-placeholder = 搜尋或輸入 URL，或選擇終端機...
command-placeholder = 輸入 URL、搜尋標籤或 > 來取得指令...
command-composer-placeholder = 輸入 / 表示指令，輸入 @ 表示媒體
command-send = 發送 (Enter)
command-terminal = 終端
command-open-terminal = 在終端機中打開
command-stack = 堆疊
command-tabs = { $count ->
    [one] 1 個選項卡
   *[other] { $count } 選項卡
}
command-prompt = 提示
command-new-tab = 新分頁
command-search = 搜尋
command-open-value = 開啟“{ $value }”
command-search-value = 搜尋“{ $value }”

schema-appearance = 外觀
schema-general = 一般
schema-layout = 佈局
schema-layout-detail = 視窗、窗格、側邊欄和對焦環。
schema-agent = 代理商
schema-agent-detail = 代理行為和工具權限。
schema-shortcuts = 快速方式
schema-shortcuts-detail = 只讀視圖。直接編輯 settings.ron 以變更綁定。
schema-terminal = 終端
schema-browser = 瀏覽器
schema-mode = 模式
schema-mode-detail = 網頁的配色方案。設備跟隨您的系統。
schema-device = 裝置
schema-light = 光
schema-dark = 黑暗
schema-language = 語言
schema-language-detail = 使用 system、en-US、ja 或任何 BCP 47 標記以及符合的 ~/.vmux/locales/<tag>.ftl 目錄。
schema-auto-update = 自動更新
schema-auto-update-detail = 啟動時和每小時檢查並安裝更新。
schema-startup-url = 啟動 URL
schema-startup-url-detail = 清空打開命令列提示字元。
schema-search-engine = 搜尋引擎
schema-search-engine-detail = 用於從「開始」和命令列進行網路搜尋。
schema-window = 窗戶
schema-pane = 窗格
schema-side-sheet = 側板
schema-focus-ring = 對焦環
schema-run-placement = 允許運行佈局覆蓋
schema-run-placement-detail = 讓代理程式選擇運行窗格模式、方向和錨點。
schema-leader = 領導者
schema-leader-detail = 和弦快捷鍵的前綴鍵。
schema-chord-timeout = 和弦超時
schema-chord-timeout-detail = 和弦前綴到期前的毫秒數。
schema-bindings = 綁定
schema-confirm-close = 確認關閉
schema-confirm-close-detail = 在關閉正在運行的進程的終端之前進行提示。
schema-default-theme = 預設主題
schema-default-theme-detail = 主題清單中活動主題的名稱。
