common-open = 打开
common-close = 关闭
common-install = 安装
common-uninstall = 卸载
common-update = 更新
common-retry = 重试
common-refresh = 刷新
common-remove = 删除
common-enable = 启用
common-disable = 禁用
common-new = 新
common-active = 活跃的
common-running = 跑步
common-done = 完成
common-failed = 失败
common-installed = 已安装
common-items = { $count ->
    [one] { $count } 项目
   *[other] { $count } 项
}
start-title = 开始
start-tagline = 一提示。任何事情，完成。

agents-title = 代理商
agents-search = 搜索 ACP 和 CLI 代理...
agents-empty = 没有匹配的代理
agents-empty-detail = 尝试名称、运行时或 ACP/CLI。
agents-install-failed = 安装失败
agents-updating = 正在更新…
agents-retrying = 正在重试...
agents-preparing = 正在准备……

extensions-title = 扩展
extensions-search = 搜索已安装或 Chrome Web Store...
extensions-relaunch = 重新启动即可申请
extensions-empty = 没有安装扩展
extensions-no-match = 没有匹配的扩展名
extensions-empty-detail = 搜索上面的 Chrome Web Store 并按 Return。
extensions-no-match-detail = 尝试其他名称或分机 ID。
extensions-on = 开
extensions-off = 关闭
extensions-enable-confirm = 启用 { $name }？
extensions-enable-permissions = 启用 { $name } 并允许：

lsp-title = 语言服务器
lsp-search = 搜索语言服务器、linter、格式化程序……
lsp-loading = 正在加载目录...
lsp-empty = 没有匹配的语言服务器
lsp-empty-detail = 尝试另一种语言、linter 或格式化程序。
lsp-needs = 需要 { $tool }
lsp-status-available = 可用
lsp-status-on-path = 在 PATH 上
lsp-status-installing = 正在安装...
lsp-status-installed = 已安装
lsp-status-outdated = 可用更新
lsp-status-running = 跑步
lsp-status-failed = 失败

spaces-title = 空间
spaces-new-placeholder = 新空间名称
spaces-empty = 没有空格
spaces-default-name = 空间 { $number }
spaces-tabs = { $count ->
    [one] 1 个选项卡
   *[other] { $count } 选项卡
}
spaces-delete = 删除空格

team-title = 团队
team-just-you = 这个空间里只有你
team-agents = { $count ->
    [one] 您和 1 位代理人
   *[other] 您和 { $count } 代理
}
team-empty = 这里还没有人
team-you = 你
team-agent = 代理

services-title = 后台服务
services-processes = { $count ->
    [one] 1 进程
   *[other] { $count } 进程
}
services-kill-all = 全部杀死
services-not-running = 服务未运行
services-start-with = 从以下开始：
services-empty = 无活动进程
services-filter = 过滤过程...
services-no-match = 没有匹配的进程
services-connected = 已连接
services-disconnected = 已断开连接
services-attached = 附上
services-kill = 杀
services-memory = 内存
services-size = 尺寸
services-shell = 壳牌

error-title = 错误

history-search = 搜索历史
history-clear-all = 全部清除
history-clear-confirm = 清除所有历史记录？
history-clear-warning = 此操作无法撤消。
history-cancel = 取消
history-today = 今天
history-yesterday = 昨天
history-days-ago = { $count } 天前
history-day-offset = 日 -{ $count }

settings-title = 设置
settings-loading = 正在加载设置...
settings-stored = 存储在 ~/.vmux/settings.ron 中
settings-other = 其他
settings-software-update = 软件更新
settings-check-updates = 检查更新
settings-check-updates-hint = 启动时自动检查，启用自动更新时每小时自动检查一次。
settings-update-unavailable = 不可用
settings-update-unavailable-hint = 此版本中不包含更新程序。
settings-update-checking = 正在检查...
settings-update-checking-hint = 正在检查更新...
settings-update-check-again = 再次检查
settings-update-current = Vmux 是最新的。
settings-update-downloading = 正在下载...
settings-update-downloading-hint = 正在下载 Vmux { $version }...
settings-update-installing = 正在安装...
settings-update-installing-hint = 正在安装 Vmux { $version }...
settings-update-ready = 更新就绪
settings-update-ready-hint = Vmux { $version } 已准备就绪。重新启动即可应用它。
settings-update-try-again = 再试一次
settings-update-failed = 无法检查更新。
settings-item = 项目
settings-item-number = 项目 { $number }
settings-press-key = 按一个键...
settings-saved = 已保存
settings-record-key = 单击以记录新的组合键

tray-open-window = 开窗
tray-close-window = 关闭窗口
tray-pause-recording = 暂停录音
tray-resume-recording = 恢复录音
tray-finish-recording = 完成录音
tray-quit = 退出 Vmux

composer-attach-files = 附加文件 (/upload)
composer-remove-attachment = 删除附件

layout-back = 返回
layout-forward = 前进
layout-reload = 重新加载
layout-bookmark-page = 将此页添加为书签
layout-remove-bookmark = 删除书签
layout-pin-page = 固定此页面
layout-unpin-page = 取消固定此页面
layout-manage-extensions = 管理扩展
layout-new-stack = 新堆栈
layout-close-tab = 关闭选项卡
layout-bookmark = 书签
layout-pin = 销
layout-new-tab = 新标签页
layout-team = 团队

command-switch-space = 切换空间...
command-search-ask = 搜索或询问...
command-new-tab-placeholder = 搜索或输入 URL，或选择终端...
command-placeholder = 输入 URL、搜索选项卡或 > 来获取命令...
command-composer-placeholder = 输入 / 表示命令，输入 @ 表示媒体
command-send = 发送 (Enter)
command-terminal = 终端
command-open-terminal = 在终端中打开
command-stack = 堆栈
command-tabs = { $count ->
    [one] 1 个选项卡
   *[other] { $count } 选项卡
}
command-prompt = 提示
command-new-tab = 新标签页
command-search = 搜索
command-open-value = 打开“{ $value }”
command-search-value = 搜索“{ $value }”

schema-appearance = 外观
schema-general = 一般
schema-layout = 布局
schema-layout-detail = 窗口、窗格、侧边栏和聚焦环。
schema-agent = 代理
schema-agent-detail = 代理行为和工具权限。
schema-shortcuts = 快捷方式
schema-shortcuts-detail = 只读视图。直接编辑 settings.ron 以更改绑定。
schema-terminal = 终端
schema-browser = 浏览器
schema-mode = 模式
schema-mode-detail = 网页的配色方案。设备跟随您的系统。
schema-device = 设备
schema-light = 光
schema-dark = 黑暗
schema-language = 语言
schema-language-detail = 使用 system、en-US、ja 或任何 BCP 47 标记以及匹配的 ~/.vmux/locales/<tag>.ftl 目录。
schema-auto-update = 自动更新
schema-auto-update-detail = 在启动时和每小时检查并安装更新。
schema-startup-url = 启动 URL
schema-startup-url-detail = 清空打开命令栏提示符。
schema-search-engine = 搜索引擎
schema-search-engine-detail = 用于从“开始”和命令栏进行网络搜索。
schema-window = 窗户
schema-pane = 窗格
schema-side-sheet = 侧板
schema-focus-ring = 对焦环
schema-run-placement = 允许运行布局覆盖
schema-run-placement-detail = 让代理选择运行窗格模式、方向和锚点。
schema-leader = 领导者
schema-leader-detail = 和弦快捷键的前缀键。
schema-chord-timeout = 和弦超时
schema-chord-timeout-detail = 和弦前缀到期前的毫秒数。
schema-bindings = 绑定
schema-confirm-close = 确认关闭
schema-confirm-close-detail = 在关闭正在运行的进程的终端之前进行提示。
schema-default-theme = 默认主题
schema-default-theme-detail = 主题列表中活动主题的名称。
