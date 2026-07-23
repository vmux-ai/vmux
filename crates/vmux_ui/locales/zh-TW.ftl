locale-name = 中文（台灣）
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

tools-title = 工具
tools-search = 搜尋套件、代理、MCP、語言工具和設定檔…
tools-open = 開啟工具
tools-fold = 收合工具
tools-unfold = 展開工具
tools-scanning = 正在掃描本機工具…
tools-no-installed = 沒有已安裝的工具
tools-empty = 沒有相符的工具
tools-empty-detail = 安裝套件或新增 Stow 風格的設定檔套件。
tools-apply = 套用
tools-homebrew = Homebrew
tools-homebrew-sync = 已安裝的配方和應用程式會自動同步。
tools-open-brewfile = 開啟 Brewfile
tools-managed = 已管理
tools-provider-homebrew-formulae = Homebrew 配方
tools-provider-homebrew-casks = Homebrew 應用程式
tools-provider-npm = npm 套件
tools-provider-acp-agents = ACP 代理
tools-provider-language-tools = 語言工具
tools-provider-mcp-servers = MCP 伺服器
tools-provider-dotfiles = 設定檔
tools-status-available = 可用
tools-status-missing = 缺少
tools-status-conflict = 衝突
tools-forget = 移除記錄
tools-manage = 管理
tools-link = 連結
tools-unlink = 取消連結
tools-import = 匯入
tools-update-count = { $count ->
    [one] 1 項更新
   *[other] { $count } 項更新
}
tools-conflict-count = { $count ->
    [one] 1 項衝突
   *[other] { $count } 項衝突
}
tools-result-applied = 已套用工具
tools-result-imported = 已匯入工具
tools-result-installed = 已安裝 { $name }
tools-result-updated = 已更新 { $name }
tools-result-uninstalled = 已解除安裝 { $name }
tools-result-forgotten = 已移除 { $name } 的記錄
tools-result-managed = { $name } 現已納入管理
tools-result-linked = 已連結 { $name }
tools-result-unlinked = 已取消連結 { $name }
vault-title = Vault
vault-open = { common-open } Vault
vault-description = 使用 Git 同步設定、工具、點檔案和知識。
vault-sync = 同步
vault-create = 創造
vault-connect = 連接
vault-private = 私有倉庫
vault-public-warning = 公共儲存庫公開您的知識和配置。
vault-choose-repository = 選擇一個儲存庫...
vault-empty = 空的
vault-clean = 最新
vault-not-connected = 未連接
vault-change-count = 變化: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

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

settings-empty =（空）
settings-none =（無）

schema-system = 系統
schema-editor = 編輯器
schema-recording = 錄製
schema-radius = 圓角
schema-padding = 內距
schema-gap = 間距
schema-width = 寬度
schema-color = 顏色
schema-red = 紅色
schema-green = 綠色
schema-blue = 藍色
schema-follow-files = 跟隨檔案
schema-tidy-files = 整理檔案
schema-tidy-files-max = 檔案整理門檻
schema-tidy-files-auto = 自動整理檔案
schema-app-providers = 應用程式提供者
schema-provider = 提供者
schema-kind = 類型
schema-models = 模型
schema-acp = ACP 代理程式
schema-id = ID
schema-name = 名稱
schema-command = 指令
schema-arguments = 引數
schema-environment = 環境變數
schema-working-directory = 工作目錄
schema-shell = 殼層
schema-font-family = 字型
schema-startup-directory = 啟動目錄
schema-themes = 主題
schema-color-scheme = 配色方案
schema-font-size = 字型大小
schema-line-height = 行高
schema-cursor-style = 游標樣式
schema-cursor-blink = 游標閃爍
schema-custom-themes = 自訂主題
schema-foreground = 前景色
schema-background = 背景色
schema-cursor = 游標
schema-ansi-colors = ANSI 色彩
schema-keymap = 按鍵配置
schema-explorer = 檔案總管
schema-visible = 顯示
schema-language-servers = 語言伺服器
schema-servers = 伺服器
schema-language-id = 語言 ID
schema-root-markers = 根目錄標記檔
schema-output-directory = 輸出目錄

menu-scene = 場景
menu-layout = 版面配置
menu-terminal = 終端機
menu-browser = 瀏覽器
menu-service = 服務
menu-bookmark = 書籤
menu-edit = 編輯

layout-knowledge = 知識庫
layout-open-knowledge = 開啟知識庫
layout-open-welcome-knowledge = 開啟歡迎使用知識庫
layout-open-path = 開啟 { $path }
layout-fold-knowledge = 收合知識庫
layout-unfold-knowledge = 展開知識庫
layout-bookmarks = 書籤
layout-new-folder = 新增資料夾
layout-add-to-bookmarks = 加入書籤
layout-move-to-bookmarks = 移至書籤
layout-stack-number = 堆疊 { $number }
layout-fold-stack = 收合堆疊
layout-unfold-stack = 展開堆疊
layout-close-stack = 關閉堆疊
layout-bookmark-in = 加入書籤到 { $folder }

common-cancel = 取消
common-delete = 刪除
common-save = 儲存
common-rename = 重新命名
common-expand = 展開
common-collapse = 收合
common-loading = 載入中…
common-error = 錯誤
common-output = 輸出
common-pending = 待處理
common-current = 目前
common-stop = 停止
services-command = Vmux 服務
services-uptime-seconds = { $seconds } 秒
services-uptime-minutes = { $minutes } 分 { $seconds } 秒
services-uptime-hours = { $hours } 時 { $minutes } 分
services-uptime-days = { $days } 天 { $hours } 時

error-page-failed-load = 頁面載入失敗
error-page-not-found = 找不到頁面
error-unknown-host = 未知的 Vmux app 主機：{ $host }

history-title = 歷史記錄

command-new-app-chat = 新增 { $provider }/{ $model } 聊天（App）
command-interactive-mode-user = 場景 > 互動模式 > 使用者
command-interactive-mode-player = 場景 > 互動模式 > 玩家
command-minimize-window = 佈局 > 視窗 > 最小化
command-toggle-layout = 佈局 > 佈局 > 切換佈局
command-close-tab = 佈局 > 分頁 > 關閉分頁
command-new-task = 佈局 > 分頁 > 新增任務…
command-next-tab = 佈局 > 分頁 > 下一個分頁
command-prev-tab = 佈局 > 分頁 > 上一個分頁
command-rename-tab = 佈局 > 分頁 > 重新命名分頁
command-tab-select-1 = 佈局 > 分頁 > 選取分頁 1
command-tab-select-2 = 佈局 > 分頁 > 選取分頁 2
command-tab-select-3 = 佈局 > 分頁 > 選取分頁 3
command-tab-select-4 = 佈局 > 分頁 > 選取分頁 4
command-tab-select-5 = 佈局 > 分頁 > 選取分頁 5
command-tab-select-6 = 佈局 > 分頁 > 選取分頁 6
command-tab-select-7 = 佈局 > 分頁 > 選取分頁 7
command-tab-select-8 = 佈局 > 分頁 > 選取分頁 8
command-tab-select-last = 佈局 > 分頁 > 選取最後一個分頁
command-close-pane = 佈局 > 窗格 > 關閉窗格
command-select-pane-left = 佈局 > 窗格 > 選取左側窗格
command-select-pane-right = 佈局 > 窗格 > 選取右側窗格
command-select-pane-up = 佈局 > 窗格 > 選取上方窗格
command-select-pane-down = 佈局 > 窗格 > 選取下方窗格
command-swap-pane-prev = 佈局 > 窗格 > 與上一個窗格交換
command-swap-pane-next = 佈局 > 窗格 > 與下一個窗格交換
command-equalize-pane-size = 佈局 > 窗格 > 平均窗格大小
command-resize-pane-left = 佈局 > 窗格 > 向左調整窗格
command-resize-pane-right = 佈局 > 窗格 > 向右調整窗格
command-resize-pane-up = 佈局 > 窗格 > 向上調整窗格
command-resize-pane-down = 佈局 > 窗格 > 向下調整窗格
command-stack-close = 佈局 > 堆疊 > 關閉堆疊
command-stack-next = 佈局 > 堆疊 > 下一個堆疊
command-stack-previous = 佈局 > 堆疊 > 上一個堆疊
command-stack-reopen = 佈局 > 堆疊 > 重新開啟已關閉頁面
command-stack-swap-prev = 佈局 > 堆疊 > 將堆疊左移
command-stack-swap-next = 佈局 > 堆疊 > 將堆疊右移
command-space-open = 佈局 > 空間 > 空間
command-terminal-close = 終端機 > 關閉終端機
command-terminal-next = 終端機 > 下一個終端機
command-terminal-prev = 終端機 > 上一個終端機
command-terminal-clear = 終端機 > 清除終端機
command-browser-prev-page = 瀏覽器 > 導覽 > 返回
command-browser-next-page = 瀏覽器 > 導覽 > 前進
command-browser-reload = 瀏覽器 > 導覽 > 重新載入
command-browser-hard-reload = 瀏覽器 > 導覽 > 強制重新載入
command-open-in-place = 瀏覽器 > 開啟 > 在此開啟
command-open-in-new-stack = 瀏覽器 > 開啟 > 在新堆疊中開啟
command-open-in-pane-top = 瀏覽器 > 開啟 > 在上方窗格開啟
command-open-in-pane-right = 瀏覽器 > 開啟 > 在右側窗格開啟
command-open-in-pane-bottom = 瀏覽器 > 開啟 > 在下方窗格開啟
command-open-in-pane-left = 瀏覽器 > 開啟 > 在左側窗格開啟
command-open-in-new-tab = 瀏覽器 > 開啟 > 在新分頁開啟
command-open-in-new-space = 瀏覽器 > 開啟 > 在新空間開啟
command-browser-zoom-in = 瀏覽器 > 檢視 > 放大
command-browser-zoom-out = 瀏覽器 > 檢視 > 縮小
command-browser-zoom-reset = 瀏覽器 > 檢視 > 實際大小
command-browser-dev-tools = 瀏覽器 > 檢視 > 開發者工具
command-browser-open-command-bar = 瀏覽器 > 列 > 指令列
command-browser-open-page-in-command-bar = 瀏覽器 > 列 > 編輯頁面
command-browser-open-path-bar = 瀏覽器 > 列 > 路徑導覽器
command-browser-open-commands = 瀏覽器 > 列 > 指令
command-browser-open-history = 瀏覽器 > 列 > 歷史記錄
command-service-open = 服務 > 開啟服務監視器
command-bookmark-toggle-active = 書籤 > 將頁面加入書籤
command-bookmark-pin-active = 書籤 > 釘選頁面

layout-tab = 分頁
layout-no-stacks = 沒有堆疊
layout-loading = 載入中…
layout-no-markdown-files = 沒有 Markdown 檔案
layout-empty-folder = 空資料夾
layout-worktree = 工作樹
layout-folder-name = 資料夾名稱
layout-no-pins-bookmarks = 沒有釘選或書籤
layout-move-to = 移至 { $folder }
layout-bookmark-current-page = 將目前頁面加入書籤
layout-rename-folder = 重新命名資料夾
layout-remove-folder = 移除資料夾
layout-update-downloading = 正在下載更新
layout-update-installing = 正在安裝更新…
layout-update-ready = 有新版本可用
layout-restart-update = 重新啟動以更新

agent-preparing = 正在準備 Agent…
agent-send-all-queued = 立即送出所有佇列中的提示（Esc）
agent-send = 送出（Enter）
agent-ready = 準備好了，隨時可以開始。
agent-loading-older = 正在載入較舊訊息…
agent-load-older = 載入較舊訊息
agent-continued-from = 接續自 { $source }
agent-older-context-omitted = 已省略較舊脈絡
agent-interrupted = 已中斷
agent-allow-tool = 允許 { $tool }？
agent-deny = 拒絕
agent-allow-always = 一律允許
agent-allow = 允許
agent-loading-sessions = 正在載入工作階段…
agent-no-resumable-sessions = 找不到可繼續的工作階段
agent-no-matching-sessions = 沒有相符的工作階段
agent-no-matching-models = 沒有相符的模型
agent-choice-help = ↑/↓ 或 Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = 選擇儲存庫資料夾
agent-choose-repository-detail = 選取 Agent 應使用的本機 Git 儲存庫。
agent-choosing = 選擇中…
agent-choose-folder = 選擇資料夾
agent-queued = 已排入佇列
agent-attached = 已附加：
agent-cancel-queued = 取消佇列中的提示
agent-resume-queued = 繼續佇列中的提示
agent-clear-queue = 清除佇列
agent-send-all-now = 立即全部送出
agent-choose-option = 請選擇上方選項
agent-loading-media = 正在載入媒體…
agent-no-matching-media = 沒有相符媒體
agent-prompt-context = 提示脈絡
agent-details = 詳細資訊
agent-path = 路徑
agent-tool = 工具
agent-server = 伺服器
agent-bytes = { $count } 位元組
agent-worked-for = 已工作 { $duration }
agent-worked-for-steps = { $count ->
    [one] 已工作 { $duration } · 1 個步驟
   *[other] 已工作 { $duration } · { $count } 個步驟
}
agent-tool-guardian-review = Guardian 審查
agent-tool-read-files = 已讀取檔案
agent-tool-viewed-image = 已檢視圖片
agent-tool-used-browser = 已使用瀏覽器
agent-tool-searched-files = 已搜尋檔案
agent-tool-ran-commands = 已執行指令
agent-thinking = 思考中
agent-subagent = 子 Agent
agent-prompt = 提示
agent-thread = 對話串
agent-parent = 父項
agent-children = 子項
agent-call = 呼叫
agent-raw-event = 原始事件
agent-plan = 計畫
agent-tasks = { $count ->
    [one] 1 個任務
   *[other] { $count } 個任務
}
agent-edited = 已編輯
agent-reconnecting = 正在重新連線 { $attempt }/{ $total }
agent-status-running = 執行中
agent-status-done = 完成
agent-status-failed = 失敗
agent-status-pending = 待處理
agent-slash-attach-files = 附加檔案
agent-slash-resume-session = 繼續過去的工作階段
agent-slash-select-model = 選擇模型
agent-slash-continue-cli = 在 CLI 中繼續此工作階段
agent-session-just-now = 剛剛
agent-session-minutes-ago = { $count } 分鐘前
agent-session-hours-ago = { $count } 小時前
agent-session-days-ago = { $count } 天前
agent-working-working = 工作中
agent-working-thinking = 思考中
agent-working-pondering = 沉思中
agent-working-noodling = 琢磨中
agent-working-percolating = 醞釀中
agent-working-conjuring = 施展中
agent-working-cooking = 烹調中
agent-working-brewing = 釀造中
agent-working-musing = 冥想中
agent-working-ruminating = 反覆思索中
agent-working-scheming = 規劃中
agent-working-synthesizing = 整合中
agent-working-tinkering = 調整中
agent-working-churning = 運轉中
agent-working-vibing = 找感覺中
agent-working-simmering = 慢燉中
agent-working-crafting = 打磨中
agent-working-divining = 探索中
agent-working-mulling = 斟酌中
agent-working-spelunking = 深入挖掘中

editor-toggle-explorer = 切換檔案總管（Cmd+B）
editor-unsaved = 未儲存
editor-rendered-markdown = 已轉譯 Markdown，可即時編輯
editor-note = 備註
editor-source-editor = 原始碼編輯器
editor-editor = 編輯器
editor-git-diff = Git 差異
editor-diff = 差異
editor-tidy = 整理
editor-always = 一律
editor-unchanged-previews = { $count ->
    [one] ✦ 1 個未變更預覽
   *[other] ✦ { $count } 個未變更預覽
}
editor-open-externally = 使用外部程式開啟
editor-changed-line = 已變更行
editor-go-to-definition = 前往定義
editor-find-references = 尋找參照
editor-references = { $count ->
    [one] 1 個參照
   *[other] { $count } 個參照
}
editor-lsp-starting = { $server } 啟動中…
editor-lsp-not-installed = { $server } — 尚未安裝
editor-explorer = 檔案總管
editor-open-editors = 已開啟的編輯器
editor-outline = 大綱
editor-new-file = 新增檔案
editor-new-folder = 新增資料夾
editor-delete-confirm = 要刪除「{ $name }」嗎？此動作無法復原。
editor-created-folder = 已建立資料夾 { $name }
editor-created-file = 已建立檔案 { $name }
editor-renamed-to = 已重新命名為 { $name }
editor-deleted = 已刪除 { $name }
editor-failed-decode-image = 圖片解碼失敗
editor-preview-large-image = 圖片（太大，無法預覽）
editor-preview-binary = 二進位
editor-preview-file = 檔案

git-status-clean = 乾淨
git-status-modified = 已修改
git-status-staged = 已暫存
git-status-staged-modified = 已暫存*
git-status-untracked = 未追蹤
git-status-deleted = 已刪除
git-status-conflict = 衝突
git-accept-all = ✓ 全部接受
git-unstage = 取消暫存
git-confirm-deny-all = 確認全部拒絕
git-deny-all = ✗ 全部拒絕
git-commit-message = 提交訊息
git-commit = 提交（{ $count }）
git-push = ↑ 推送
git-loading-diff = 正在載入差異…
git-no-changes = 沒有可顯示的變更
git-accept = ✓ 接受
git-deny = ✗ 拒絕
git-show-unchanged-lines = 顯示 { $count } 行未變更內容

terminal-loading = 載入中…
terminal-runs-when-ready = 就緒後執行 · Ctrl+C 清除 · Esc 跳過
terminal-booting = 啟動中
terminal-type-command = 輸入指令 · 就緒後執行 · Esc 跳過

setup-tagline-claude = Anthropic 的 coding agent，就在 Vmux
setup-tagline-codex = OpenAI 的 coding agent，就在 Vmux
setup-tagline-vibe = Mistral 的 coding agent，就在 Vmux
setup-install-title = 安裝 { $name } CLI
setup-homebrew-required = 需要 Homebrew 才能安裝 { $command }，但尚未設定。Vmux 會先安裝 Homebrew，接著安裝 { $name }。
setup-terminal-instructions = 在終端機中，按 Return 開始，接著依提示輸入 Mac 密碼。
setup-command-missing = Vmux 開啟此頁面，是因為本機尚未安裝 { $command } 指令。執行下方指令即可取得。
setup-install-failed = 安裝未完成。請查看終端機取得詳細資訊，然後重試。
setup-installing = 安裝中…
setup-install-homebrew = 安裝 Homebrew + { $name }
setup-run-install = 執行安裝指令
setup-auto-reload = Vmux 會在終端機中執行，並在 { $command } 就緒後重新載入。

debug-title = 偵錯
debug-auto-update = 自動更新
debug-simulate-update = 模擬有可用更新
debug-simulate-download = 模擬下載
debug-clear-update = 清除更新
debug-trigger-restart = 觸發重新啟動

command-manage-spaces = 管理工作區…
command-pane-stack-location = 窗格 { $pane } / 堆疊 { $stack }
command-space-pane-stack-location = { $space } / 窗格 { $pane } / 堆疊 { $stack }
command-terminal-path = 終端機（{ $path }）
command-group-interactive-mode = 互動模式
command-group-window = 視窗
command-group-tab = 分頁
command-group-pane = 窗格
command-group-stack = 堆疊
command-group-space = 工作區
command-group-navigation = 導覽
command-group-open = 開啟
command-group-view = 檢視
command-group-bar = 列

menu-close-vmux = 關閉 Vmux

agents-terminal-coding-agent = 終端機型 coding agent
