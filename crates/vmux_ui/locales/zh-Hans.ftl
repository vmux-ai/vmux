locale-name = 简体中文
common-open = 打开
common-close = 关闭
common-install = 安装
common-uninstall = 卸载
common-update = 更新
common-retry = 重试
common-refresh = 刷新
common-remove = 移除
common-enable = 启用
common-disable = 停用
common-new = 新建
common-active = 活跃
common-running = 运行中
common-done = 已完成
common-failed = 失败
common-installed = 已安装
common-items = { $count ->
    [one] { $count } 项
   *[other] { $count } 项
}

tools-title = 工具
tools-search = 搜索软件包、代理、MCP、语言工具和配置文件…
tools-open = 打开工具
tools-fold = 收起工具
tools-unfold = 展开工具
tools-scanning = 正在扫描本地工具…
tools-no-installed = 没有已安装的工具
tools-empty = 没有匹配的工具
tools-empty-detail = 安装软件包或添加 Stow 风格的配置文件包。
tools-apply = 应用
tools-homebrew = Homebrew
tools-homebrew-sync = 已安装的配方和应用会自动同步。
tools-open-brewfile = 打开 Brewfile
tools-managed = 已管理
tools-provider-homebrew-formulae = Homebrew 配方
tools-provider-homebrew-casks = Homebrew 应用
tools-provider-npm = npm 软件包
tools-provider-acp-agents = ACP 代理
tools-provider-language-tools = 语言工具
tools-provider-mcp-servers = MCP 服务器
tools-provider-dotfiles = 配置文件
tools-status-available = 可用
tools-status-missing = 缺失
tools-status-conflict = 冲突
tools-forget = 移除记录
tools-manage = 管理
tools-link = 链接
tools-unlink = 取消链接
tools-import = 导入
tools-update-count = { $count ->
    [one] 1 项更新
   *[other] { $count } 项更新
}
tools-conflict-count = { $count ->
    [one] 1 项冲突
   *[other] { $count } 项冲突
}
tools-result-applied = 已应用工具
tools-result-imported = 已导入工具
tools-result-installed = 已安装 { $name }
tools-result-updated = 已更新 { $name }
tools-result-uninstalled = 已卸载 { $name }
tools-result-forgotten = 已移除 { $name } 的记录
tools-result-managed = { $name } 现已纳入管理
tools-result-linked = 已链接 { $name }
tools-result-unlinked = 已取消链接 { $name }
vault-title = Vault
vault-open = { common-open } Vault
vault-description = 使用 Git 同步设置、工具、点文件和知识。
vault-sync = 同步
vault-create = 创造
vault-connect = 连接
vault-private = 私有仓库
vault-public-warning = 公共存储库公开您的知识和配置。
vault-choose-repository = 选择一个存储库...
vault-empty = 空的
vault-clean = 最新
vault-not-connected = 未连接
vault-change-count = 变化: { $count }
vault-result-created = Vault · { common-done }
vault-result-connected = Vault · { common-done }
vault-result-synced = Vault · { common-done }

start-title = 开始
start-tagline = 一句提示，万事搞定。

agents-title = Agent
agents-search = 搜索 ACP 和 CLI Agent…
agents-empty = 没有匹配的 Agent
agents-empty-detail = 试试名称、运行时，或 ACP/CLI。
agents-install-failed = 安装失败
agents-updating = 正在更新…
agents-retrying = 正在重试…
agents-preparing = 正在准备…

extensions-title = 扩展
extensions-search = 搜索已安装扩展或 Chrome Web Store…
extensions-relaunch = 重启以应用
extensions-empty = 未安装扩展
extensions-no-match = 没有匹配的扩展
extensions-empty-detail = 在上方搜索 Chrome Web Store，然后按 Return。
extensions-no-match-detail = 试试其他名称或扩展 ID。
extensions-on = 开
extensions-off = 关
extensions-enable-confirm = 启用 { $name }？
extensions-enable-permissions = 启用 { $name } 并允许：

lsp-title = Language Servers
lsp-search = 搜索语言服务器、Linter、格式化工具…
lsp-loading = 正在加载目录…
lsp-empty = 没有匹配的语言服务器
lsp-empty-detail = 试试其他语言、Linter 或格式化工具。
lsp-needs = 需要 { $tool }
lsp-status-available = 可用
lsp-status-on-path = 在 PATH 中
lsp-status-installing = 正在安装…
lsp-status-installed = 已安装
lsp-status-outdated = 有更新可用
lsp-status-running = 运行中
lsp-status-failed = 失败

spaces-title = 工作区
spaces-new-placeholder = 新工作区名称
spaces-empty = 没有工作区
spaces-default-name = 工作区 { $number }
spaces-tabs = { $count ->
    [one] 1 个标签页
   *[other] { $count } 个标签页
}
spaces-delete = 删除工作区

team-title = 团队
team-just-you = 此工作区中只有你
team-agents = { $count ->
    [one] 你和 1 个 Agent
   *[other] 你和 { $count } 个 Agent
}
team-empty = 暂无成员
team-you = 你
team-agent = Agent

services-title = 后台服务
services-processes = { $count ->
    [one] 1 个进程
   *[other] { $count } 个进程
}
services-kill-all = 全部强制结束
services-not-running = 服务未运行
services-start-with = 启动命令：
services-empty = 没有活动进程
services-filter = 筛选进程…
services-no-match = 没有匹配的进程
services-connected = 已连接
services-disconnected = 未连接
services-attached = 已附加
services-kill = 强制结束
services-memory = 内存
services-size = 大小
services-shell = Shell

error-title = 错误

history-search = 搜索历史记录
history-clear-all = 全部清除
history-clear-confirm = 清除所有历史记录？
history-clear-warning = 此操作无法撤销。
history-cancel = 取消
history-today = 今天
history-yesterday = 昨天
history-days-ago = { $count } 天前
history-day-offset = 第 -{ $count } 天

settings-title = 设置
settings-loading = 正在加载设置…
settings-stored = 存储于 ~/.vmux/settings.ron
settings-other = 其他
settings-software-update = 软件更新
settings-check-updates = 检查更新
settings-check-updates-hint = 启用自动更新后，启动时及每小时自动检查一次。
settings-update-unavailable = 不可用
settings-update-unavailable-hint = 此构建未包含更新器。
settings-update-checking = 正在检查…
settings-update-checking-hint = 正在检查更新…
settings-update-check-again = 再次检查
settings-update-current = Vmux 已是最新版本。
settings-update-downloading = 正在下载…
settings-update-downloading-hint = 正在下载 Vmux { $version }…
settings-update-installing = 正在安装…
settings-update-installing-hint = 正在安装 Vmux { $version }…
settings-update-ready = 更新已就绪
settings-update-ready-hint = Vmux { $version } 已准备就绪。重启以应用。
settings-update-try-again = 重试
settings-update-failed = 无法检查更新。
settings-item = 项目
settings-item-number = 项目 { $number }
settings-press-key = 请按键…
settings-saved = 已保存
settings-record-key = 点击录制新的按键组合

tray-open-window = 打开窗口
tray-close-window = 关闭窗口
tray-pause-recording = 暂停录制
tray-resume-recording = 继续录制
tray-finish-recording = 完成录制
tray-quit = 退出 Vmux

composer-attach-files = 附加文件 (/upload)
composer-remove-attachment = 移除附件

layout-back = 后退
layout-forward = 前进
layout-reload = 重新加载
layout-bookmark-page = 收藏此页
layout-remove-bookmark = 移除书签
layout-pin-page = 固定此页
layout-unpin-page = 取消固定此页
layout-manage-extensions = 管理扩展
layout-new-stack = 新建层页
layout-close-tab = 关闭标签页
layout-bookmark = 收藏
layout-pin = 固定
layout-new-tab = 新建标签页
layout-team = 团队

command-switch-space = 切换工作区…
command-search-ask = 搜索或提问…
command-new-tab-placeholder = 搜索或输入 URL，或选择终端…
command-placeholder = 输入 URL、搜索标签页，或输入 > 打开命令…
command-composer-placeholder = 输入 / 使用命令，或输入 @ 添加媒体
command-send = 发送 (Enter)
command-terminal = 终端
command-open-terminal = 在终端中打开
command-stack = 层页
command-tabs = { $count ->
    [one] 1 个标签页
   *[other] { $count } 个标签页
}
command-prompt = 提示
command-new-tab = 新建标签页
command-search = 搜索
command-open-value = 打开“{ $value }”
command-search-value = 搜索“{ $value }”

schema-appearance = 外观
schema-general = 通用
schema-layout = 布局
schema-layout-detail = 窗口、窗格、侧边栏和焦点环。
schema-agent = Agent
schema-agent-detail = Agent 行为和工具权限。
schema-shortcuts = 快捷键
schema-shortcuts-detail = 只读视图。如需更改按键绑定，请直接编辑 settings.ron。
schema-terminal = 终端
schema-browser = 浏览器
schema-mode = 模式
schema-mode-detail = 网页配色方案。“设备”会跟随系统。
schema-device = 设备
schema-light = 浅色
schema-dark = 深色
schema-language = 语言
schema-language-detail = 使用系统语言、en-US、ja，或任意 BCP 47 标签，并提供匹配的 ~/.vmux/locales/<tag>.ftl 目录。
schema-auto-update = 自动更新
schema-auto-update-detail = 启动时及每小时检查并安装更新。
schema-startup-url = 启动 URL
schema-startup-url-detail = 留空则打开命令栏提示。
schema-search-engine = 搜索引擎
schema-search-engine-detail = 用于从“开始”和命令栏发起网页搜索。
schema-window = 窗口
schema-pane = 窗格
schema-side-sheet = 侧边面板
schema-focus-ring = 焦点环
schema-run-placement = 允许覆盖运行位置
schema-run-placement-detail = 允许 Agent 选择运行窗格模式、方向和锚点。
schema-leader = 前导键
schema-leader-detail = 和弦快捷键的前缀键。
schema-chord-timeout = 和弦超时
schema-chord-timeout-detail = 和弦前缀过期前的毫秒数。
schema-bindings = 按键绑定
schema-confirm-close = 关闭前确认
schema-confirm-close-detail = 关闭含有运行中进程的终端前进行提示。
schema-default-theme = 默认主题
schema-default-theme-detail = 主题列表中当前主题的名称。

settings-empty = (空)
settings-none = (无)

schema-system = 系统
schema-editor = 编辑器
schema-recording = 录制
schema-radius = 圆角
schema-padding = 内边距
schema-gap = 间距
schema-width = 宽度
schema-color = 颜色
schema-red = 红色
schema-green = 绿色
schema-blue = 蓝色
schema-follow-files = 跟随文件
schema-tidy-files = 整理文件
schema-tidy-files-max = 文件整理阈值
schema-tidy-files-auto = 自动整理文件
schema-app-providers = 应用提供商
schema-provider = 提供商
schema-kind = 类型
schema-models = 模型
schema-acp = ACP 代理
schema-id = ID
schema-name = 名称
schema-command = 命令
schema-arguments = 参数
schema-environment = 环境变量
schema-working-directory = 工作目录
schema-shell = Shell
schema-font-family = 字体
schema-startup-directory = 启动目录
schema-themes = 主题
schema-color-scheme = 配色方案
schema-font-size = 字号
schema-line-height = 行高
schema-cursor-style = 光标样式
schema-cursor-blink = 光标闪烁
schema-custom-themes = 自定义主题
schema-foreground = 前景色
schema-background = 背景色
schema-cursor = 光标
schema-ansi-colors = ANSI 颜色
schema-keymap = 键位映射
schema-explorer = 资源管理器
schema-visible = 可见
schema-language-servers = 语言服务器
schema-servers = 服务器
schema-language-id = 语言 ID
schema-root-markers = 根目录标记
schema-output-directory = 输出目录

menu-scene = 场景
menu-layout = 布局
menu-terminal = 终端
menu-browser = 浏览器
menu-service = 服务
menu-bookmark = 书签
menu-edit = 编辑

layout-knowledge = 知识
layout-open-knowledge = 打开知识
layout-open-welcome-knowledge = 打开“欢迎使用知识”
layout-open-path = 打开 { $path }
layout-fold-knowledge = 折叠知识
layout-unfold-knowledge = 展开知识
layout-bookmarks = 书签
layout-new-folder = 新建文件夹
layout-add-to-bookmarks = 添加到书签
layout-move-to-bookmarks = 移到书签
layout-stack-number = 堆栈 { $number }
layout-fold-stack = 折叠堆栈
layout-unfold-stack = 展开堆栈
layout-close-stack = 关闭堆栈
layout-bookmark-in = 加入 { $folder } 书签

common-cancel = 取消
common-delete = 删除
common-save = 保存
common-rename = 重命名
common-expand = 展开
common-collapse = 折叠
common-loading = 正在加载…
common-error = 错误
common-output = 输出
common-pending = 待处理
common-current = 当前
common-stop = 停止
services-command = Vmux 服务
services-uptime-seconds = { $seconds } 秒
services-uptime-minutes = { $minutes } 分 { $seconds } 秒
services-uptime-hours = { $hours } 时 { $minutes } 分
services-uptime-days = { $days } 天 { $hours } 时

error-page-failed-load = 页面加载失败
error-page-not-found = 找不到页面
error-unknown-host = 未知的 Vmux 应用主机：{ $host }

history-title = 历史记录

command-new-app-chat = 新建 { $provider }/{ $model } 聊天（应用）
command-interactive-mode-user = 场景 > 交互模式 > 用户
command-interactive-mode-player = 场景 > 交互模式 > 播放者
command-minimize-window = 布局 > 窗口 > 最小化
command-toggle-layout = 布局 > 布局 > 切换布局
command-close-tab = 布局 > 标签页 > 关闭标签页
command-new-task = 布局 > 标签页 > 新建任务…
command-next-tab = 布局 > 标签页 > 下一个标签页
command-prev-tab = 布局 > 标签页 > 上一个标签页
command-rename-tab = 布局 > 标签页 > 重命名标签页
command-tab-select-1 = 布局 > 标签页 > 选择标签页 1
command-tab-select-2 = 布局 > 标签页 > 选择标签页 2
command-tab-select-3 = 布局 > 标签页 > 选择标签页 3
command-tab-select-4 = 布局 > 标签页 > 选择标签页 4
command-tab-select-5 = 布局 > 标签页 > 选择标签页 5
command-tab-select-6 = 布局 > 标签页 > 选择标签页 6
command-tab-select-7 = 布局 > 标签页 > 选择标签页 7
command-tab-select-8 = 布局 > 标签页 > 选择标签页 8
command-tab-select-last = 布局 > 标签页 > 选择最后一个标签页
command-close-pane = 布局 > 窗格 > 关闭窗格
command-select-pane-left = 布局 > 窗格 > 选择左侧窗格
command-select-pane-right = 布局 > 窗格 > 选择右侧窗格
command-select-pane-up = 布局 > 窗格 > 选择上方窗格
command-select-pane-down = 布局 > 窗格 > 选择下方窗格
command-swap-pane-prev = 布局 > 窗格 > 与上一个窗格交换
command-swap-pane-next = 布局 > 窗格 > 与下一个窗格交换
command-equalize-pane-size = 布局 > 窗格 > 平均窗格大小
command-resize-pane-left = 布局 > 窗格 > 向左调整窗格
command-resize-pane-right = 布局 > 窗格 > 向右调整窗格
command-resize-pane-up = 布局 > 窗格 > 向上调整窗格
command-resize-pane-down = 布局 > 窗格 > 向下调整窗格
command-stack-close = 布局 > 堆栈 > 关闭堆栈
command-stack-next = 布局 > 堆栈 > 下一个堆栈
command-stack-previous = 布局 > 堆栈 > 上一个堆栈
command-stack-reopen = 布局 > 堆栈 > 重新打开已关闭页面
command-stack-swap-prev = 布局 > 堆栈 > 向左移动堆栈
command-stack-swap-next = 布局 > 堆栈 > 向右移动堆栈
command-space-open = 布局 > 空间 > 空间
command-terminal-close = 终端 > 关闭终端
command-terminal-next = 终端 > 下一个终端
command-terminal-prev = 终端 > 上一个终端
command-terminal-clear = 终端 > 清空终端
command-browser-prev-page = 浏览器 > 导航 > 后退
command-browser-next-page = 浏览器 > 导航 > 前进
command-browser-reload = 浏览器 > 导航 > 重新加载
command-browser-hard-reload = 浏览器 > 导航 > 强制重新加载
command-open-in-place = 浏览器 > 打开 > 在此处打开
command-open-in-new-stack = 浏览器 > 打开 > 在新堆栈中打开
command-open-in-pane-top = 浏览器 > 打开 > 在上方窗格中打开
command-open-in-pane-right = 浏览器 > 打开 > 在右侧窗格中打开
command-open-in-pane-bottom = 浏览器 > 打开 > 在下方窗格中打开
command-open-in-pane-left = 浏览器 > 打开 > 在左侧窗格中打开
command-open-in-new-tab = 浏览器 > 打开 > 在新标签页中打开
command-open-in-new-space = 浏览器 > 打开 > 在新空间中打开
command-browser-zoom-in = 浏览器 > 视图 > 放大
command-browser-zoom-out = 浏览器 > 视图 > 缩小
command-browser-zoom-reset = 浏览器 > 视图 > 实际大小
command-browser-dev-tools = 浏览器 > 视图 > 开发者工具
command-browser-open-command-bar = 浏览器 > 栏 > 命令栏
command-browser-open-page-in-command-bar = 浏览器 > 栏 > 编辑页面
command-browser-open-path-bar = 浏览器 > 栏 > 路径导航器
command-browser-open-commands = 浏览器 > 栏 > 命令
command-browser-open-history = 浏览器 > 栏 > 历史记录
command-service-open = 服务 > 打开服务监视器
command-bookmark-toggle-active = 书签 > 将页面加入书签
command-bookmark-pin-active = 书签 > 置顶页面

layout-tab = 标签页
layout-no-stacks = 没有堆栈
layout-loading = 正在加载…
layout-no-markdown-files = 没有 Markdown 文件
layout-empty-folder = 空文件夹
layout-worktree = 工作树
layout-folder-name = 文件夹名称
layout-no-pins-bookmarks = 没有置顶项或书签
layout-move-to = 移动到 { $folder }
layout-bookmark-current-page = 将当前页面加入书签
layout-rename-folder = 重命名文件夹
layout-remove-folder = 移除文件夹
layout-update-downloading = 正在下载更新
layout-update-installing = 正在安装更新…
layout-update-ready = 有新版本可用
layout-restart-update = 重启以更新

agent-preparing = 正在准备智能体…
agent-send-all-queued = 立即发送所有排队提示（Esc）
agent-send = 发送（Enter）
agent-ready = 准备好了，随时可以开始。
agent-loading-older = 正在加载更早消息…
agent-load-older = 加载更早消息
agent-continued-from = 继续自 { $source }
agent-older-context-omitted = 已省略更早上下文
agent-interrupted = 已中断
agent-allow-tool = 允许 { $tool }？
agent-deny = 拒绝
agent-allow-always = 始终允许
agent-allow = 允许
agent-loading-sessions = 正在加载会话…
agent-no-resumable-sessions = 未找到可恢复的会话
agent-no-matching-sessions = 没有匹配的会话
agent-no-matching-models = 没有匹配的模型
agent-choice-help = ↑/↓ 或 Ctrl+N/Ctrl+P · 1–9 · Enter
agent-choose-repository = 选择仓库文件夹
agent-choose-repository-detail = 选择智能体要使用的本地 Git 仓库。
agent-choosing = 正在选择…
agent-choose-folder = 选择文件夹
agent-queued = 已排队
agent-attached = 已附加：
agent-cancel-queued = 取消排队提示
agent-resume-queued = 恢复排队提示
agent-clear-queue = 清空队列
agent-send-all-now = 立即全部发送
agent-choose-option = 在上方选择一个选项
agent-loading-media = 正在加载媒体…
agent-no-matching-media = 没有匹配的媒体
agent-prompt-context = 提示上下文
agent-details = 详情
agent-path = 路径
agent-tool = 工具
agent-server = 服务器
agent-bytes = { $count } 字节
agent-worked-for = 工作了 { $duration }
agent-worked-for-steps = { $count ->
    [one] 工作了 { $duration } · 1 步
   *[other] 工作了 { $duration } · { $count } 步
}
agent-tool-guardian-review = Guardian 审查
agent-tool-read-files = 读取了文件
agent-tool-viewed-image = 查看了图片
agent-tool-used-browser = 使用了浏览器
agent-tool-searched-files = 搜索了文件
agent-tool-ran-commands = 运行了命令
agent-thinking = 正在思考
agent-subagent = 子智能体
agent-prompt = 提示
agent-thread = 线程
agent-parent = 父级
agent-children = 子级
agent-call = 调用
agent-raw-event = 原始事件
agent-plan = 计划
agent-tasks = { $count ->
    [one] 1 个任务
   *[other] { $count } 个任务
}
agent-edited = 已编辑
agent-reconnecting = 正在重新连接 { $attempt }/{ $total }
agent-status-running = 运行中
agent-status-done = 已完成
agent-status-failed = 已失败
agent-status-pending = 待处理
agent-slash-attach-files = 附加文件
agent-slash-resume-session = 恢复过去的会话
agent-slash-select-model = 选择模型
agent-slash-continue-cli = 在 CLI 中继续此会话
agent-session-just-now = 刚刚
agent-session-minutes-ago = { $count } 分钟前
agent-session-hours-ago = { $count } 小时前
agent-session-days-ago = { $count } 天前
agent-working-working = 正在工作
agent-working-thinking = 正在思考
agent-working-pondering = 正在斟酌
agent-working-noodling = 正在琢磨
agent-working-percolating = 正在酝酿
agent-working-conjuring = 正在构思
agent-working-cooking = 正在处理
agent-working-brewing = 正在酝酿
agent-working-musing = 正在沉思
agent-working-ruminating = 正在反复推敲
agent-working-scheming = 正在规划
agent-working-synthesizing = 正在整合
agent-working-tinkering = 正在调试
agent-working-churning = 正在推进
agent-working-vibing = 正在找感觉
agent-working-simmering = 正在慢慢打磨
agent-working-crafting = 正在打磨
agent-working-divining = 正在探查
agent-working-mulling = 正在权衡
agent-working-spelunking = 正在深入探索

editor-toggle-explorer = 切换资源管理器（Cmd+B）
editor-unsaved = 未保存
editor-rendered-markdown = 已渲染 Markdown，可实时编辑
editor-note = 备注
editor-source-editor = 源码编辑器
editor-editor = 编辑器
editor-git-diff = Git 差异
editor-diff = 差异
editor-tidy = 整理
editor-always = 始终
editor-unchanged-previews = { $count ->
    [one] ✦ 1 个未更改预览
   *[other] ✦ { $count } 个未更改预览
}
editor-open-externally = 在外部打开
editor-changed-line = 已更改行
editor-go-to-definition = 转到定义
editor-find-references = 查找引用
editor-references = { $count ->
    [one] 1 处引用
   *[other] { $count } 处引用
}
editor-lsp-starting = { $server } 正在启动…
editor-lsp-not-installed = { $server } — 未安装
editor-explorer = 资源管理器
editor-open-editors = 打开的编辑器
editor-outline = 大纲
editor-new-file = 新建文件
editor-new-folder = 新建文件夹
editor-delete-confirm = 删除“{ $name }”？此操作无法撤销。
editor-created-folder = 已创建文件夹 { $name }
editor-created-file = 已创建文件 { $name }
editor-renamed-to = 已重命名为 { $name }
editor-deleted = 已删除 { $name }
editor-failed-decode-image = 图片解码失败
editor-preview-large-image = 图片（太大，无法预览）
editor-preview-binary = 二进制
editor-preview-file = 文件

git-status-clean = 干净
git-status-modified = 已修改
git-status-staged = 已暂存
git-status-staged-modified = 已暂存*
git-status-untracked = 未跟踪
git-status-deleted = 已删除
git-status-conflict = 冲突
git-accept-all = ✓ 全部接受
git-unstage = 取消暂存
git-confirm-deny-all = 确认全部拒绝
git-deny-all = ✗ 全部拒绝
git-commit-message = 提交信息
git-commit = 提交（{ $count }）
git-push = ↑ 推送
git-loading-diff = 正在加载差异…
git-no-changes = 没有可显示的更改
git-accept = ✓ 接受
git-deny = ✗ 拒绝
git-show-unchanged-lines = 显示 { $count } 行未更改内容

terminal-loading = 正在加载…
terminal-runs-when-ready = 就绪后运行 · Ctrl+C 清除 · Esc 跳过
terminal-booting = 正在启动
terminal-type-command = 输入命令 · 就绪后运行 · Esc 跳过

setup-tagline-claude = Anthropic 的编码智能体，现已接入 Vmux
setup-tagline-codex = OpenAI 的编码智能体，现已接入 Vmux
setup-tagline-vibe = Mistral 的编码智能体，现已接入 Vmux
setup-install-title = 安装 { $name } CLI
setup-homebrew-required = 安装 { $command } 需要 Homebrew，但尚未设置。Vmux 会先安装 Homebrew，然后安装 { $name }。
setup-terminal-instructions = 在终端中，按 Return 开始，然后在提示时输入你的 Mac 密码。
setup-command-missing = Vmux 打开此页面，是因为本地尚未安装 { $command } 命令。运行下面的命令即可获取。
setup-install-failed = 安装未完成。请查看终端了解详情，然后重试。
setup-installing = 正在安装…
setup-install-homebrew = 安装 Homebrew + { $name }
setup-run-install = 运行安装命令
setup-auto-reload = Vmux 会在终端中运行它，并在 { $command } 就绪后重新加载。

debug-title = 调试
debug-auto-update = 自动更新
debug-simulate-update = 模拟有可用更新
debug-simulate-download = 模拟下载
debug-clear-update = 清除更新
debug-trigger-restart = 触发重启

command-manage-spaces = 管理空间…
command-pane-stack-location = 窗格 { $pane } / 堆栈 { $stack }
command-space-pane-stack-location = { $space } / 窗格 { $pane } / 堆栈 { $stack }
command-terminal-path = 终端 ({ $path })
command-group-interactive-mode = 交互模式
command-group-window = 窗口
command-group-tab = 标签页
command-group-pane = 窗格
command-group-stack = 堆栈
command-group-space = 空间
command-group-navigation = 导航
command-group-open = 打开
command-group-view = 查看
command-group-bar = 栏

menu-close-vmux = 关闭 Vmux

agents-terminal-coding-agent = 基于终端的编码代理
