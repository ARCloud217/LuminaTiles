# 系统托盘（tray）

## 用途

应用启动即在通知区域常驻托盘图标，作为窗口关闭后的入口与唯一的退出通道。左键单击或右键都弹出菜单，双击直接显示主窗口。

## 用户行为

- 主窗口点关闭（×）不退出程序，只隐藏窗口，托盘图标继续存在。
- 单击 / 右键托盘图标弹出菜单，包含「打开主窗口」和「退出 LuminaTiles」两项；未来新增的托盘能力按序追加到同一菜单。
- 双击托盘图标直接显示主窗口（最小化时恢复并置前）。
- 选择「打开主窗口」后窗口恢复显示并获得焦点；选择「退出 LuminaTiles」结束整个进程。
- 托盘 tooltip 为应用名 `LuminaTiles`；菜单文案随系统 UI 语言（中文 / 其它语言回落英文）。

## 实现要点

- 托盘在 `src-tauri/src/features/tray.rs` 的 `init()` 中构建，由 `lib.rs` 的 `setup` 调用；托盘是应用级能力，不进入前端 `FEATURES` 注册表。
- `TrayIconBuilder::with_id("main")`：图标取 `app.default_window_icon()`（即 `tauri.conf.json` bundle 里的应用图标），缺失时返回 `tauri::Error::AssetNotFound("icons/icon.ico")`；`show_menu_on_left_click(true)` 让左右键都弹菜单（`menu_on_left_click` 已弃用）。
- 菜单项 id 为 `tray-open-main` / `tray-quit`，在 `on_menu_event` 中分发；`on_tray_icon_event` 只处理 `TrayIconEvent::DoubleClick { button: MouseButton::Left }`（Windows 才有双击事件）。
- `show_main_window()` 依次 `unminimize` → `show` → `set_focus`，最小化状态下也能正确恢复。
- 关闭到托盘由 `lib.rs` 的 `.on_window_event` 实现：`WindowEvent::CloseRequested` 且 `window.label() == "main"` 时 `api.prevent_close()` + `window.hide()`；其它窗口（如 `lock-indicator`）不受影响。
- 托盘文案在编译期嵌入前端语言包（`include_str!("../../../src/shared/locales/zh-CN.json")` 与 `en-US.json`），不在 Rust 里另存一份文案。
- 语言选择用 `GetUserDefaultUILanguage() & 0x03ff`：等于 `0x0004`（中文）取 zh-CN，其余回落 en-US，与前端 `pickLocale` 的规则一致；解析失败直接 `expect("语言包缺少 tray 文案，检查 src/shared/locales/*.json")`，不静默吞掉。
- 退出走 `app.exit(0)`，不做「隐藏到托盘」以外的拦截，因此不会留下后台进程。

## 接口

托盘功能不新增 Tauri 命令与事件，也不经过 `src/shared/api`：全部交互在原生托盘层完成。

| 类型 | 名称 | 说明 |
| --- | --- | --- |
| 常量 | `TRAY_ID = "main"` | 托盘实例 id |
| 常量 | `MENU_OPEN_MAIN = "tray-open-main"` | 菜单项 id：打开主窗口 |
| 常量 | `MENU_QUIT = "tray-quit"` | 菜单项 id：退出 |
| 常量 | `MAIN_WINDOW = "main"` | 主窗口 label |

## 配置与开关

- Cargo feature：`src-tauri/Cargo.toml` 中 `tauri` 的 `features = ["tray-icon"]`（关闭后 `features/tray.rs` 不参与构建，需同时移除 `lib.rs` 的 `setup` 调用）。
- 单功能关闭：删掉 `lib.rs` `setup` 里的 `features::tray::init(&handle)?;`（同时去掉「关闭即隐藏」的 `on_window_event` 分支，否则关掉窗口后无法再唤回）。
- 应用图标：沿用 `tauri.conf.json` 的 `bundle.icon`，托盘图标与任务栏图标同源，不额外放资源文件。
- 语言：无独立开关，跟随系统 UI 语言；新增语言需在 `load_strings()` 的主语言判断里加入对应分支。

## 本地化键

`tray.tooltip`、`tray.openMain`、`tray.quit`，两个语言包必须同时存在，否则启动时 `expect` 直接报错（属于有意失败）。

## 验证

1. `cargo build` 通过且无警告。
2. 启动后托盘区出现 LuminaTiles 图标（Windows 11 默认收进溢出面板，需展开 ^ 查看），tooltip 为 `LuminaTiles`。
3. 点主窗口关闭按钮：窗口消失、进程仍在（任务管理器可见 `luminatiles.exe`）。
4. 打开溢出面板点图标：弹出菜单含「打开主窗口」「退出 LuminaTiles」；点第一项后主窗口恢复显示并置前。
5. 再次打开菜单点「退出 LuminaTiles」：进程结束，托盘图标消失。
6. 双击托盘图标：主窗口直接显示。
7. 已实测（UIA + 截图，2026-10-27）：
   - 注册表 `HKCU\Control Panel\NotifyIconSettings` 出现 `ExecutablePath = src-tauri\target\debug\luminatiles.exe`、`InitialTooltip = LuminaTiles` 的项。
   - 溢出面板中按钮 `Name = "LuminaTiles"`、`AutomationId = "NotifyItemIcon"`，rect `1419,855 40x40`。
   - 菜单为 win32 `#32768` 窗口，rect `1439,825,1632,875`；点 25% 高度处主窗口 `visible` 由 `False` 变 `True`，点 75% 高度处进程消失。
8. 验证结束清理临时脚本与截图（`.tmp/`），并确认没有残留 `luminatiles.exe` 进程。
