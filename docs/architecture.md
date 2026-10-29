# LuminaTiles 架构

## 运行形态

Tauri 2 应用，前端为无构建步骤的原生 ES module，通过 `withGlobalTauri` 使用 `window.__TAURI__`。应用常驻系统托盘：关闭主窗口只是隐藏，退出只能通过托盘菜单。

## 窗口

| 窗口 label | 用途 | 说明 |
| --- | --- | --- |
| `main` | 主窗口 | 顶栏 + 展示区 + 设置坞三段式，880×620；点关闭按钮隐藏到托盘，不退出进程 |
| `lock-indicator` | 按键提示 | 基础尺寸 236×72，随设置按比例缩放；透明、无边框、置顶、不抢焦点、点击穿透，启动时隐藏 |

两个窗口共用同一套入口脚本，靠 `<html data-window>` 区分当前窗口。

## 前端结构

```text
src/
  index.html                     # 主窗口页面
  main.js                        # 入口 + 功能注册表（FEATURES）
  shared/
    api/index.js                 # 唯一的 Tauri 命令 / 事件边界
    i18n/index.js                # 语言包加载与 t()
    locales/zh-CN.json, en-US.json
    motion/index.js              # 通用动画预设（playMotionPreset / resetMotion）
    styles/tokens.css            # 设计令牌（强制深色，单一信号色）
    styles/shell.css             # 主窗口布局与组件样式
    styles/motion.css            # 动画预设的 keyframes 与 data 属性驱动
  features/
    lock-indicator/
      index.html                 # 提示窗页面
      index.js                   # 提示窗渲染与生命周期
      styles.css                 # 提示窗样式
      settings.js                # 主窗口设置面板
      settings.css               # 设置面板样式
```

`src/main.js` 的 `FEATURES` 数组是功能总开关：每项包含 `id`、`enabled`、`window`、`init`。初始化时按当前窗口匹配执行，`init` 返回的清理函数会在 `pagehide` 时调用。

## 共享动画预设

- 预设集合定义在 `src/shared/motion/index.js` 的 `MOTION_PRESETS`：`none`、`fade`、`slideUp`、`slideDown`、`slideLeft`、`slideRight`、`fly`，与后端 `AnimationPreset` 一一对应。
- `playMotionPreset(element, { preset, phase, durationMs })` 只做三件事：写入 `--motion-duration`、设置 `data-motion` / `data-phase`、强制重排后切到 `in` / `out`；具体位移与透明度由 `src/shared/styles/motion.css` 的 keyframes 决定。
- 任何需要入场/退场动画的功能都可以复用这套预设，不需要自己写 keyframes；使用方需在页面引入 `shared/styles/motion.css`。

## 后端结构

```text
src-tauri/src/
  lib.rs                         # Builder、窗口初始化、命令注册
  features/lock_indicator.rs     # 按键提示：开关状态轮询、Insert 键盘钩子、设置持久化、窗口定位与显隐
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

界面走「新极简 + 科幻控制台」：近黑画布、单一信号琥珀（`--color-accent`）、冷灰细线划分层次，不使用图片资源，所有图形都由 CSS/JS 绘制。完整约束见 [design-language.md](design-language.md)，新增界面必须遵守。

- 平面构成：主窗口为「顶栏 / 展示区 / 设置坞」三段式；展示区左列排版（eyebrow + 巨型展示体 + 说明），右列是 9×6 的 `lattice` 点阵：光标进入时用四角括线（`.lattice__reticle`）框选所指格子，邻格按距离透出余光（`--glow`），第 23 格常亮为焦点；没有循环扫光。
- 设置坞：左侧是分组导航（`.settings__rail`，当前只有「按键提示」一组），右侧是按组切换的面板（`.settings__panel[data-settings-panel]`）；新增功能只需追加一组 tab 与面板，切换由 `src/main.js` 的 `initSettingsNav()` 统一驱动。
- 动态设计：顶栏时钟每秒走字、状态点脉动、点阵光标框选与余光、三段式入场位移；位移类动效收敛在 `shell.css` 的 keyframes 与过渡里，`prefers-reduced-motion` 时停用。
- 交互设计：按钮切角（`clip-path`）、`focus-visible` 统一 1px 信号色描边、滑杆为细轨 + 硬边方形拇指、输入控件硬边 2px 圆角。
- 字体分工：`--font-family-display`（Bahnschrift）承担品牌与按钮，`--font-family-mono`（Cascadia Mono）承担时钟、数值与字段标签，正文用 Segoe UI Variable。

## 设计令牌

颜色、字体、间距、圆角、阴影全部定义在 `src/shared/styles/tokens.css`，强制深色（`color-scheme: dark`），不再随系统切换明暗。新增样式复用令牌，不写孤立视觉值。

## 开发调试

- 命令：在 `src-tauri` 目录执行 `cargo tauri dev`（项目未装 node 侧 `@tauri-apps/cli`，用 cargo 子命令 `tauri-cli 2.12.4`）。
- 未配置 `build.devUrl`，tauri-cli 用内置静态 dev server 托管 `frontendDist = ../src`，默认监听 `127.0.0.1:1430`（`--port` 可改）；窗口加载的是 `http://127.0.0.1:1430`，不是 `tauri://` 自定义协议。
- 前端改动（`src/**`）会被监听并自动刷新 WebView，实测写文件后约 2 秒生效，不需要手动刷新。
- 后端改动（`src-tauri/**`）触发增量重编译并重启应用，实测 6–7 秒；`--no-watch` 可关闭 watcher，`--additional-watch-folders` 可补充监视目录。
- 两端都不是模块级 HMR：前端刷新会重置页面状态，后端重编译会重启进程状态。

## 用户数据

- 用户设置以 JSON 保存在应用配置目录：`%APPDATA%\<identifier>\settings.json`，当前 identifier 为 `com.otaz.luminatiles`。
- 文件顶层按功能分节（当前只有 `lockIndicator`），两个窗口通过后端命令共享同一份配置。
- 写入采用「临时文件 + rename」，读取失败时输出诊断信息并回落到默认值。目前未实现导入与导出。
