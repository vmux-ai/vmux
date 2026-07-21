common-open = 開啟
common-close = 關閉
common-install = 安裝
common-uninstall = 解除安裝
common-update = 更新
common-retry = 重試
common-refresh = 重新整理
common-remove = 移除
common-enable = 啟用
common-disable = 停用
common-new = 新增
common-active = 使用中
common-running = 執行中
common-done = 完成
common-failed = 失敗
common-installed = 已安裝
common-items = { $count ->
    [one] { $count } 個項目
   *[other] { $count } 個項目
}
start-title = 開始
start-tagline = 一句提示，搞定所有事。

agents-title = Agent
agents-search = 搜尋 ACP 與 CLI Agent…
agents-empty = 找不到符合的 Agent
agents-empty-detail = 試試名稱、執行環境，或 ACP/CLI。
agents-install-failed = 安裝失敗
agents-updating = 更新中…
agents-retrying = 重試中…
agents-preparing = 準備中…

extensions-title = 擴充功能
extensions-search = 搜尋已安裝項目或 Chrome 線上應用程式商店…
extensions-relaunch = 重新啟動以套用
extensions-empty = 尚未安裝擴充功能
extensions-no-match = 找不到符合的擴充功能
extensions-empty-detail = 在上方搜尋 Chrome 線上應用程式商店，然後按 Return。
extensions-no-match-detail = 試試其他名稱或擴充功能 ID。
extensions-on = 開啟
extensions-off = 關閉
extensions-enable-confirm = 要啟用 { $name } 嗎？
extensions-enable-permissions = 啟用 { $name } 並允許：

lsp-title = 語言伺服器
lsp-search = 搜尋語言伺服器、linter、格式化工具…
lsp-loading = 正在載入目錄…
lsp-empty = 找不到符合的語言伺服器
lsp-empty-detail = 試試其他語言、linter 或格式化工具。
lsp-needs = 需要 { $tool }
lsp-status-available = 可用
lsp-status-on-path = 在 PATH 上
lsp-status-installing = 安裝中…
lsp-status-installed = 已安裝
lsp-status-outdated = 有可用更新
lsp-status-running = 執行中
lsp-status-failed = 失敗

spaces-title = 工作區
spaces-new-placeholder = 新工作區名稱
spaces-empty = 沒有工作區
spaces-default-name = 工作區 { $number }
spaces-tabs = { $count ->
    [one] 1 個分頁
   *[other] { $count } 個分頁
}
spaces-delete = 刪除工作區

team-title = 團隊
team-just-you = 這個工作區只有你
team-agents = { $count ->
    [one] 你和 1 個 Agent
   *[other] 你和 { $count } 個 Agent
}
team-empty = 這裡還沒有人
team-you = 你
team-agent = Agent

services-title = 背景服務
services-processes = { $count ->
    [one] 1 個處理程序
   *[other] { $count } 個處理程序
}
services-kill-all = 全部強制終止
services-not-running = 服務未執行
services-start-with = 啟動方式：
services-empty = 沒有作用中的處理程序
services-filter = 篩選處理程序…
services-no-match = 找不到符合的處理程序
services-connected = 已連線
services-disconnected = 已中斷連線
services-attached = 已附加
services-kill = 強制終止
services-memory = 記憶體
services-size = 大小
services-shell = Shell

error-title = 錯誤

history-search = 搜尋歷史記錄
history-clear-all = 全部清除
history-clear-confirm = 要清除所有歷史記錄嗎？
history-clear-warning = 此操作無法復原。
history-cancel = 取消
history-today = 今天
history-yesterday = 昨天
history-days-ago = { $count } 天前
history-day-offset = 第 -{ $count } 天

settings-title = 設定
settings-loading = 正在載入設定…
settings-stored = 儲存在 ~/.vmux/settings.ron
settings-other = 其他
settings-software-update = 軟體更新
settings-check-updates = 檢查更新
settings-check-updates-hint = 啟用自動更新時，會在啟動時及每小時自動檢查。
settings-update-unavailable = 無法使用
settings-update-unavailable-hint = 此組建未包含更新程式。
settings-update-checking = 檢查中…
settings-update-checking-hint = 正在檢查更新…
settings-update-check-again = 再次檢查
settings-update-current = Vmux 已是最新版本。
settings-update-downloading = 下載中…
settings-update-downloading-hint = 正在下載 Vmux { $version }…
settings-update-installing = 安裝中…
settings-update-installing-hint = 正在安裝 Vmux { $version }…
settings-update-ready = 更新已就緒
settings-update-ready-hint = Vmux { $version } 已就緒。重新啟動即可套用。
settings-update-try-again = 再試一次
settings-update-failed = 無法檢查更新。
settings-item = 項目
settings-item-number = 項目 { $number }
settings-press-key = 請按一個按鍵…
settings-saved = 已儲存
settings-record-key = 按一下以錄製新的按鍵組合

tray-open-window = 開啟視窗
tray-close-window = 關閉視窗
tray-pause-recording = 暫停錄製
tray-resume-recording = 繼續錄製
tray-finish-recording = 完成錄製
tray-quit = 結束 Vmux

composer-attach-files = 附加檔案 (/upload)
composer-remove-attachment = 移除附件

layout-back = 返回
layout-forward = 前進
layout-reload = 重新載入
layout-bookmark-page = 將此頁加入書籤
layout-remove-bookmark = 移除書籤
layout-pin-page = 釘選此頁
layout-unpin-page = 取消釘選此頁
layout-manage-extensions = 管理擴充功能
layout-new-stack = 新增堆疊
layout-close-tab = 關閉分頁
layout-bookmark = 書籤
layout-pin = 釘選
layout-new-tab = 新增分頁
layout-team = 團隊

command-switch-space = 切換工作區…
command-search-ask = 搜尋或提問…
command-new-tab-placeholder = 搜尋、輸入 URL，或選擇 Terminal…
command-placeholder = 輸入 URL、搜尋分頁，或輸入 > 執行指令…
command-composer-placeholder = 輸入 / 使用指令，或輸入 @ 加入媒體
command-send = 傳送 (Enter)
command-terminal = 終端機
command-open-terminal = 在終端機中開啟
command-stack = 堆疊
command-tabs = { $count ->
    [one] 1 個分頁
   *[other] { $count } 個分頁
}
command-prompt = 提示
command-new-tab = 新增分頁
command-search = 搜尋
command-open-value = 開啟「{ $value }」
command-search-value = 搜尋「{ $value }」

schema-appearance = 外觀
schema-general = 一般
schema-layout = 版面配置
schema-layout-detail = 視窗、窗格、側邊欄與焦點框。
schema-agent = Agent
schema-agent-detail = Agent 行為與工具權限。
schema-shortcuts = 快捷鍵
schema-shortcuts-detail = 唯讀檢視。若要變更按鍵綁定，請直接編輯 settings.ron。
schema-terminal = 終端機
schema-browser = 瀏覽器
schema-mode = 模式
schema-mode-detail = 網頁的色彩配置。Device 會跟隨系統設定。
schema-device = Device
schema-light = 淺色
schema-dark = 深色
schema-language = 語言
schema-language-detail = 使用系統、en-US、ja，或任何符合 ~/.vmux/locales/<tag>.ftl 目錄的 BCP 47 標籤。
schema-auto-update = 自動更新
schema-auto-update-detail = 啟動時及每小時檢查並安裝更新。
schema-startup-url = 啟動 URL
schema-startup-url-detail = 留空會開啟指令列提示。
schema-search-engine = 搜尋引擎
schema-search-engine-detail = 用於從「開始」與指令列進行網頁搜尋。
schema-window = 視窗
schema-pane = 窗格
schema-side-sheet = 側邊面板
schema-focus-ring = 焦點框
schema-run-placement = 允許覆寫執行位置
schema-run-placement-detail = 讓 Agent 選擇執行窗格模式、方向與錨點。
schema-leader = Leader
schema-leader-detail = 和弦快捷鍵的前置鍵。
schema-chord-timeout = 和弦逾時
schema-chord-timeout-detail = 和弦前置鍵失效前的毫秒數。
schema-bindings = 按鍵綁定
schema-confirm-close = 關閉前確認
schema-confirm-close-detail = 關閉仍有處理程序執行中的終端機前先提示。
schema-default-theme = 預設主題
schema-default-theme-detail = 主題清單中目前使用的主題名稱。
