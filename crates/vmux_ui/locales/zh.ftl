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
start-title = 开始
start-tagline = 一句提示，万事搞定。

agents-title = 智能体
agents-search = 搜索 ACP 和 CLI 智能体…
agents-empty = 没有匹配的智能体
agents-empty-detail = 试试名称、运行时，或 ACP/CLI。
agents-install-failed = 安装失败
agents-updating = 正在更新…
agents-retrying = 正在重试…
agents-preparing = 正在准备…

extensions-title = 扩展
extensions-search = 搜索已安装扩展或 Chrome 网上应用店…
extensions-relaunch = 重新启动以应用
extensions-empty = 未安装扩展
extensions-no-match = 没有匹配的扩展
extensions-empty-detail = 在上方搜索 Chrome 网上应用店，然后按回车键。
extensions-no-match-detail = 试试其他名称或扩展 ID。
extensions-on = 开
extensions-off = 关
extensions-enable-confirm = 要启用 { $name } 吗？
extensions-enable-permissions = 启用 { $name } 并允许：

lsp-title = 语言服务器
lsp-search = 搜索语言服务器、Linter、格式化工具…
lsp-loading = 正在加载目录…
lsp-empty = 没有匹配的语言服务器
lsp-empty-detail = 试试其他语言、Linter 或格式化工具。
lsp-needs = 需要 { $tool }
lsp-status-available = 可用
lsp-status-on-path = 在 PATH 中
lsp-status-installing = 正在安装…
lsp-status-installed = 已安装
lsp-status-outdated = 有可用更新
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
    [one] 你和 1 个智能体
   *[other] 你和 { $count } 个智能体
}
team-empty = 这里还没有人
team-you = 你
team-agent = 智能体

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
history-clear-confirm = 要清除所有历史记录吗？
history-clear-warning = 此操作无法撤销。
history-cancel = 取消
history-today = 今天
history-yesterday = 昨天
history-days-ago = { $count } 天前
history-day-offset = 第 -{ $count } 天

settings-title = 设置
settings-loading = 正在加载设置…
settings-stored = 存储在 ~/.vmux/settings.ron
settings-other = 其他
settings-software-update = 软件更新
settings-check-updates = 检查更新
settings-check-updates-hint = 启用自动更新后，会在启动时和每小时自动检查。
settings-update-unavailable = 不可用
settings-update-unavailable-hint = 此构建不包含更新器。
settings-update-checking = 正在检查…
settings-update-checking-hint = 正在检查更新…
settings-update-check-again = 再次检查
settings-update-current = Vmux 已是最新版本。
settings-update-downloading = 正在下载…
settings-update-downloading-hint = 正在下载 Vmux { $version }…
settings-update-installing = 正在安装…
settings-update-installing-hint = 正在安装 Vmux { $version }…
settings-update-ready = 更新已就绪
settings-update-ready-hint = Vmux { $version } 已准备就绪。重启以应用更新。
settings-update-try-again = 重试
settings-update-failed = 无法检查更新。
settings-item = 项目
settings-item-number = 项目 { $number }
settings-press-key = 请按一个键…
settings-saved = 已保存
settings-record-key = 点击录制新的快捷键组合

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
layout-bookmark-page = 为此页面添加书签
layout-remove-bookmark = 移除书签
layout-pin-page = 固定此页面
layout-unpin-page = 取消固定此页面
layout-manage-extensions = 管理扩展
layout-new-stack = 新建层页
layout-close-tab = 关闭标签页
layout-bookmark = 添加书签
layout-pin = 固定
layout-new-tab = 新建标签页
layout-team = 团队

command-switch-space = 切换工作区…
command-search-ask = 搜索或提问…
command-new-tab-placeholder = 搜索或输入 URL，或选择终端…
command-placeholder = 输入 URL、搜索标签页，或输入 > 运行命令…
command-composer-placeholder = 输入 / 使用命令，或输入 @ 添加媒体
command-send = 发送（Enter）
command-terminal = 终端
command-open-terminal = 在终端中打开
command-stack = 层页
command-tabs = { $count ->
    [one] 1 个标签页
   *[other] { $count } 个标签页
}
command-prompt = 提示词
command-new-tab = 新建标签页
command-search = 搜索
command-open-value = 打开“{ $value }”
command-search-value = 搜索“{ $value }”

schema-appearance = 外观
schema-general = 通用
schema-layout = 布局
schema-layout-detail = 窗口、窗格、侧边栏和焦点环。
schema-agent = 智能体
schema-agent-detail = 智能体行为和工具权限。
schema-shortcuts = 快捷键
schema-shortcuts-detail = 只读视图。要更改绑定，请直接编辑 settings.ron。
schema-terminal = 终端
schema-browser = 浏览器
schema-mode = 模式
schema-mode-detail = 网页的配色方案。“设备”会跟随系统。
schema-device = 设备
schema-light = 浅色
schema-dark = 深色
schema-language = 语言
schema-language-detail = 使用系统语言、en-US、ja，或任何 BCP 47 标签，并提供匹配的 ~/.vmux/locales/<tag>.ftl 目录。
schema-auto-update = 自动更新
schema-auto-update-detail = 启动时和每小时检查并安装更新。
schema-startup-url = 启动 URL
schema-startup-url-detail = 留空则打开命令栏提示。
schema-search-engine = 搜索引擎
schema-search-engine-detail = 用于从“开始”和命令栏进行网页搜索。
schema-window = 窗口
schema-pane = 窗格
schema-side-sheet = 侧边面板
schema-focus-ring = 焦点环
schema-run-placement = 允许运行位置覆盖
schema-run-placement-detail = 允许智能体选择运行窗格模式、方向和锚点。
schema-leader = 前导键
schema-leader-detail = 组合快捷键的前缀键。
schema-chord-timeout = 组合键超时
schema-chord-timeout-detail = 组合键前缀过期前的毫秒数。
schema-bindings = 绑定
schema-confirm-close = 关闭确认
schema-confirm-close-detail = 关闭仍有进程运行的终端前先提示。
schema-default-theme = 默认主题
schema-default-theme-detail = 主题列表中当前启用主题的名称。
