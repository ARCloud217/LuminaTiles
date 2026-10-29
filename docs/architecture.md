# LuminaTiles 架构

## 运行形态

Tauri 2 应用，前端为无构建步骤的原生 ES module，通过 `withGlobalTauri` 使用 `window.__TAURI__`。应用常驻系统托盘：关闭主窗口只是隐藏，退出只能通过托盘菜单。

## 窗口

| 窗口 label | 用途 | 说明 |
| --- | --- | --- |
| `main` | 主窗口 | 顶栏 + 展示区 + 设置坞三段式，880×620；点关闭按钮隐藏到托盘，不退出进程 |
| `banner` | 顶部横幅 | 贴屏顶整条显示消息；透明、无边框、置顶、跳过任务栏、不抢焦点、点击穿透；初始隐藏，显示前按目标显示器重算尺寸与位置 |
| `lock-indicator` | 按键提示 | 基础尺寸 236×72，随设置按比例缩放；透明、无边框、置顶、不抢焦点、点击穿透，启动时隐藏 |

三个窗口共用同一套入口脚本，靠 `<html data-window>` 区分当前窗口，权限在 `src-tauri/capabilities/default.json` 的 `windows` 中一起放开。

## 前端结构

```text
src/
  index.html                     # 主窗口页面
  main.js                        # 入口 + 功能注册表（FEATURES）
  shared/
    api/index.js                 # 唯一的 Tauri 命令 / 事件边界
    i18n/index.js                # 语言包加载与 t()
    locales/zh-CN.json, en-US.json
    messages/index.js            # 消息总线（发布 / 订阅 / 撤销，不持有队列）
    motion/index.js              # 通用动画预设（playMotionPreset / resetMotion）
    fonts/                       # 打包字体 HarmonyOS Sans SC 三个字重 + LICENSE
    styles/fonts.css             # @font-face 声明
    styles/theme.css             # 主题令牌：四个种子推导深浅两档的全部颜色
    styles/tokens.css            # 字体、间距、圆角、阴影等非颜色令牌
    styles/shell.css             # 主窗口布局与组件样式
    styles/motion.css            # 动画预设的 keyframes 与 data 属性驱动
  features/
    theme/
      index.js                   # 种子应用、跟随系统、theme:changed 监听（beforeI18n）
      settings.js                # 设置坞 02 主要设置
      settings.css
    banner/
      index.html                 # 横幅窗页面
      index.js                   # 渲染格子、翻译、动画、回传矩形与高度
      styles.css
    debug/
      index.js                   # 设置坞 03 调试页读数与自检动作
      styles.css
    lock-indicator/
      index.html                 # 提示窗页面
      index.js                   # 提示窗渲染与生命周期
      styles.css                 # 提示窗样式
      settings.js                # 主窗口设置面板
      settings.css               # 设置面板样式
```

`src/main.js` 的 `FEATURES` 数组是功能总开关：每项包含 `id`、`enabled`、`window`（或 `windows` 数组）、`init`，可选 `beforeI18n`。初始化时按当前窗口匹配执行：先跑 `beforeI18n` 的功能（主题必须在文案渲染前落地），再加载语言包，再跑其余功能；单项失败只记日志，不影响其它功能。`init` 返回的清理函数会在 `pagehide` 时调用。

## 共享动画预设

- 预设集合定义在 `src/shared/motion/index.js` 的 `MOTION_PRESETS`：`none`、`fade`、`slideUp`、`slideDown`、`slideLeft`、`slideRight`、`fly`，与后端 `AnimationPreset` 一一对应。
- `playMotionPreset(element, { preset, phase, durationMs })` 只做三件事：写入 `--motion-duration`、设置 `data-motion` / `data-phase`、强制重排后切到 `in` / `out`；具体位移与透明度由 `src/shared/styles/motion.css` 的 keyframes 决定。
- 任何需要入场/退场动画的功能都可以复用这套预设，不需要自己写 keyframes；使用方需在页面引入 `shared/styles/motion.css`。

## 后端结构

```text
src-tauri/src/
  lib.rs                         # Builder、窗口初始化、命令注册
  settings.rs                    # settings.json 共享读写：read_section / patch，保留其它顶层键
  features/lock_indicator.rs     # 按键提示：开关状态轮询、Insert 键盘钩子、设置持久化、窗口定位与显隐
  features/theme.rs              # 主题种子校验与持久化、系统强调色 / 明暗读取、原生标题栏同步
  features/messages.rs           # 消息路由：id 分配、800ms 去重合并、重要级、广播 lumina:message
  features/banner/mod.rs         # 横幅窗口：调度线程、定位显隐、光标轮询与命中、命令注册
  features/banner/scheduler.rs   # 横幅纯调度核心：池 / 队列 / 重要级排序 / 宽度占比 / 计时（含单测）
  features/debug.rs              # 调试页危险区：清库并退出
  features/tray.rs               # 系统托盘：图标、左右键菜单、打开主窗口、退出；文案按系统 UI 语言选取
```

## 托盘与退出

- 托盘在 `lib.rs` 的 `setup` 里通过 `features::tray::init()` 构建，属于应用级能力，不进入前端 `FEATURES` 注册表，也没有对应的 Tauri 命令。
- 左右键都弹菜单（`show_menu_on_left_click(true)`），双击显示主窗口；菜单当前为「打开主窗口」+「退出 LuminaTiles」，后续托盘能力按序追加。
- 关窗隐藏由 `.on_window_event` 的 `CloseRequested` 分支拦截（仅 `main` 窗口），`prevent_close()` + `hide()`；退出走 `app.exit(0)`。
- 托盘文案编译期嵌入 `src/shared/locales/*.json` 的 `tray` 节，用 `GetUserDefaultUILanguage() & 0x03ff` 判断中文（`0x0004`），其余回落 en-US。
- 细节见 [features/tray.md](features/tray.md)。

后端命令必须在 `lib.rs` 的 `invoke_handler` 中集中注册，实现放在对应 feature 模块。

## 本地化

所有用户可见文本来自 `src/shared/locales/*.json`：静态文本用 `data-i18n`，动态文本用 `t()`。`app.title` 同时作为两个窗口的文档标题。

## 视觉语言

界面走「新极简 + 科幻控制台」：近黑画布、单一信号琥珀（`--color-accent`）、冷灰细线划分层次，不使用图片资源，所有图形都由 CSS/JS 绘制。强调色与整套中性色由四个种子推导（见 [features/theme.md](features/theme.md)），可跟随 Windows 强调色与明暗。完整约束见 [design-language.md](design-language.md)，新增界面必须遵守。

- 平面构成：主窗口为「顶栏 / 展示区 / 设置坞」三段式；展示区左列排版（eyebrow + 巨型展示体 + 说明），右列是 9×6 的 `lattice` 点阵：光标进入时用四角括线（`.lattice__reticle`）框选所指格子，邻格按距离透出余光（`--glow`），第 23 格常亮为焦点；没有循环扫光。
- 设置坞：左侧是分组导航（`.settings__rail`，当前是「按键提示」「主要设置」「调试」三组），右侧是按组切换的面板（`.settings__panel[data-settings-panel]`）；新增功能只需追加一组 tab 与面板，切换由 `src/main.js` 的 `initSettingsNav()` 统一驱动。
- 动态设计：顶栏时钟每秒走字、状态点脉动、点阵光标框选与余光、三段式入场位移；位移类动效收敛在 `shell.css` 的 keyframes 与过渡里，`prefers-reduced-motion` 时停用。
- 交互设计：按钮切角（`clip-path`）、`focus-visible` 统一 1px 信号色描边、滑杆为细轨 + 硬边方形拇指、输入控件硬边 2px 圆角。
- 横幅：贴屏顶的整条，左侧 44px 队列计数槽 + 最多 3 个格子；宽度占比与挤占由 Rust 调度器决定（重要级高者居中且最宽），页面只负责渲染与测量，详见 [features/banner.md](features/banner.md)。
- 字体分工：`--font-family-base` 与 `--font-family-display` 首位是打包的 HarmonyOS Sans SC（回退 Bahnschrift SemiCondensed / Segoe UI Variable），`--font-family-mono`（Cascadia Mono）承担时钟、数值与字段标签。

## 命令与事件

所有命令在 `src-tauri/src/lib.rs` 的 `invoke_handler` 集中注册，实现归属对应 feature；前端只通过 `src/shared/api/index.js` 调用。

| 领域 | 命令 |
| --- | --- |
| 按键提示 | `lock_indicator_status` / `lock_indicator_settings` / `update_lock_indicator_settings` / `reset_lock_indicator_settings` / `preview_lock_indicator` / `lock_indicator_monitors` / `lock_indicator_resize` |
| 主题 | `theme_settings` / `update_theme_settings` / `reset_theme_settings` / `system_theme` |
| 消息 | `message_publish`（可带 `importance`）/ `message_dismiss` |
| 横幅 | `banner_settings` / `update_banner_settings` / `banner_ready` / `banner_report` / `banner_idle` / `banner_clear` / `banner_show` / `banner_geometry` |
| 调试 | `reset_app_data_and_exit` |

| 事件 | 方向 | 说明 |
| --- | --- | --- |
| `lumina:message` / `lumina:message-dismissed` | Rust → 全部窗口 | 消息总线，横幅页是唯一显示出口 |
| `theme:changed` | Rust → 全部窗口 | 种子或明暗基调变化，各窗口即时换肤 |
| `banner:state` | Rust → 横幅页 | 唯一的显示真相：`epoch`、`slots`（消息 + 宽度）、`queueCount` |
| `banner:hover` | Rust → 横幅页 | 光标进出整条，页面据此淡化 |

## 设计令牌

颜色由 `src/shared/styles/theme.css` 从四个种子（主/副强调色、主/副背景色）推导：中性色用 `oklch(from <种子> calc(l ± 档位) …)`，叠加层统一走 `--color-wash` / `--color-wash-soft`，深色与浅色两档共用同一套表达式、只换档位常量；`color-scheme` 随档位切换。字体、间距、圆角、阴影等非颜色令牌在 `src/shared/styles/tokens.css`。

新增样式复用令牌，不写孤立视觉值；新增叠加色必须用 wash 令牌，不要写死 `rgba(255,255,255,.05)` 这类值（浅色档会失效）。

## 开发调试

- 命令：在 `src-tauri` 目录执行 `cargo tauri dev`（项目未装 node 侧 `@tauri-apps/cli`，用 cargo 子命令 `tauri-cli 2.12.4`）。
- 未配置 `build.devUrl`，tauri-cli 用内置静态 dev server 托管 `frontendDist = ../src`，默认监听 `127.0.0.1:1430`（`--port` 可改）；窗口加载的是 `http://127.0.0.1:1430`，不是 `tauri://` 自定义协议。
- 前端改动（`src/**`）会被监听并自动刷新 WebView，实测写文件后约 2 秒生效，不需要手动刷新。
- 后端改动（`src-tauri/**`）触发增量重编译并重启应用，实测 6–7 秒；`--no-watch` 可关闭 watcher，`--additional-watch-folders` 可补充监视目录。
- 两端都不是模块级 HMR：前端刷新会重置页面状态，后端重编译会重启进程状态。

## 用户数据

- 用户设置以 JSON 保存在应用配置目录：`%APPDATA%\<identifier>\settings.json`，当前 identifier 为 `com.otaz.luminatiles`。
- 文件顶层按功能分节：`theme`（四个种子、明暗基调、是否跟随系统）、`lockIndicator`（按键提示）、`banner`（默认停留时长、显示器）。所有窗口通过后端命令共享同一份配置，读写统一走 `src-tauri/src/settings.rs` 的 `read_section` / `patch`，写某一节时保留其它顶层键。
- 写入采用「临时文件 + rename」，读取失败时输出诊断信息并回落到默认值。目前未实现导入与导出。
- 调试页危险区会删除整份 `settings.json` 并退出，下次启动重建默认值。