# 按键提示（lock-indicator）

## 用途

在大写锁定、数字锁定、Insert 状态发生切换时，在指针附近弹出一张小卡片提示当前状态，约 1.2 秒后自动隐藏。位置、大小、颜色、透明度与动画都可在主窗口设置里调整。

## 用户行为

- 按下 CapsLock / NumLock / Insert 并松开后，提示窗按设置的锚点与偏移出现，约 1.2 秒（另加退场动画时长）后消失。
- 提示内容为按键名 + 当前状态；开启状态用设置里的强调色显示。
- 提示窗置顶、点击穿透、不抢焦点，不打断当前输入。
- 主窗口「设置 → 按键提示」可实时调整：显示位置（跟随光标或九宫格锚点）、偏移、目标显示器、大小、强调色、文字颜色、透明度、动画预设与时长；改动即时保存并在下次提示时生效。
- 「预览提示」立即按当前设置弹一次，便于调试；「恢复默认」回落到内置默认值。
- 键盘钩子安装失败时，设置面板显示本地化错误提示，提示窗保持隐藏。

## 实现要点

- CapsLock / NumLock 由独立的轮询线程每 100ms 读取 `GetKeyState` 的开关位，与上次记录比较，变化才提示。状态来自系统真实值，不做「按按键翻转」的推断，因此不依赖按键事件是否送达当前焦点——桌面、其它程序里切换同样会提示。
- Insert 没有系统级查询接口，只能靠 `WH_KEYBOARD_LL` 低级键盘钩子，在 `keyup` 时（`KBDLLHOOKSTRUCT.vkCode`）翻转本地状态，按住不放只提示一次。
- 钩子仅用于 Insert，因此钩子安装失败时只影响 Insert 提示；CapsLock / NumLock 的轮询检测不受影响。
- 钩子回调只做「入队」一件事，窗口显示/定位/设置读取全部交给独立的提示线程；低级钩子回调超过 Windows 的 LowLevelHooksTimeout 会被系统静默摘除，慢操作绝不能放在回调里。轮询线程与钩子共用同一个队列。
- 提示线程 `publish()`：读设置 → 按缩放设置窗口尺寸 → 计算位置并 `show` → `emit` 状态事件 → 等 1.2 秒 → `emit` 隐藏事件（携带动画时长）→ 等退场动画结束后 `hide`。全过程由 `REVISION` 计数保护，连续切换时旧任务自动失效。
- 位置由 Rust 计算：锚点按目标显示器的可用区域计算（边距 16 逻辑像素），「跟随光标」用 `GetCursorPos` 再加 `CURSOR_OFFSET` 与偏移，最后夹取到显示器范围内。窗口尺寸缩放后不读 `outer_size`（刚 `set_size` 时可能仍是旧值），直接用基础尺寸乘缩放系数。
- 前端只负责渲染文案与外观：CSS 变量 `--toast-scale`、`--toast-accent`、`--toast-text`、`--toast-text-muted`，透明度写 `body.style.opacity`。
- 动画使用 `src/shared/motion` 的通用预设，提示窗只是消费方；`AnimationPreset::None` 时后端连退场等待也跳过。

## 接口

| 类型 | 名称 | 说明 |
| --- | --- | --- |
| 命令 | `lock_indicator_status` | 返回 `boolean`：键盘钩子是否可用 |
| 命令 | `lock_indicator_settings` | 返回当前 `LockIndicatorSettings` |
| 命令 | `update_lock_indicator_settings` | 入参整套设置，归一化后保存并广播，返回生效值 |
| 命令 | `reset_lock_indicator_settings` | 恢复默认并返回默认值 |
| 命令 | `preview_lock_indicator` | 按当前设置立即弹一次提示 |
| 命令 | `lock_indicator_monitors` | 返回 `[{ name, width, height, isPrimary }]`，供显示器下拉使用 |
| 事件 | `lock-indicator:state` | payload `{ key: "capsLock" \| "numLock" \| "insert", active: boolean }` |
| 事件 | `lock-indicator:hide` | payload `{ durationMs: number }`，前端据此播退场动画 |
| 事件 | `lock-indicator:settings` | payload 整套设置，设置变更后广播给提示窗 |

前端只能通过 `src/shared/api/index.js` 访问以上接口。

## 设置项与取值范围

| 字段 | 类型 | 取值 |
| --- | --- | --- |
| `position` | 枚举 | `cursor`（默认）、`topLeft`、`topCenter`、`topRight`、`middleLeft`、`center`、`middleRight`、`bottomLeft`、`bottomCenter`、`bottomRight` |
| `monitor` | 字符串 | 显示器名，空串表示跟随光标所在屏幕 |
| `offsetX` / `offsetY` | 整数 | −200～200 逻辑像素 |
| `scale` | 浮点 | 0.8～1.6，提示窗尺寸按此等比缩放（基础 236×72） |
| `accent` / `text` | 字符串或 `null` | `#RRGGBB`；`null` 表示跟随主题 |
| `opacity` | 浮点 | 0.3～1.0 |
| `animation` | 枚举 | `none`、`fade`（默认）、`slideUp`、`slideDown`、`slideLeft`、`slideRight`、`fly` |
| `durationMs` | 整数 | 120～600 毫秒 |

非法值在保存时被 `normalize()` 夹取或丢弃（颜色非 `#RRGGBB` 时置空回落到主题色）。

## 配置与存储

- 配置文件：`%APPDATA%\com.otaz.luminatiles\settings.json`，顶层为 `{ "lockIndicator": { ... } }`，给后续功能留分节。
- 写入先写 `settings.json.tmp` 再 rename；文件不存在时用默认值，解析失败时输出诊断并回落到默认值。
- 前端开关：`src/main.js` 的 `FEATURES` 中 `id: "lock-indicator"`（提示窗）与 `id: "lock-indicator-settings"`（主窗口设置面板）。
- 窗口配置：`src-tauri/tauri.conf.json` 中 label 为 `lock-indicator` 的窗口；窗口不存在时后端只打印诊断信息，不影响主窗口。
- 时长相关常量：`VISIBLE_DURATION`（1200ms）、`CURSOR_OFFSET`（28）、`ANCHOR_MARGIN`（16）、`BASE_WIDTH`/`BASE_HEIGHT`（236/72）都在 `src-tauri/src/features/lock_indicator.rs`。

## 本地化键

提示窗：`lockIndicator.capsLock`、`lockIndicator.numLock`、`lockIndicator.insert`、`lockIndicator.on`、`lockIndicator.off`、`lockIndicator.unavailable`。

设置面板：`settings.lockIndicator.*`（`title`、`description`、`position` 及各预设名、`monitor` 与 `monitorFollow`/`monitorPrimary`/`monitorOption`、`offsetX`/`offsetY`/`offsetValue`、`size`/`sizeValue`、`accent`/`text`/`followTheme`、`opacity`/`opacityValue`、`animation` 与 `motion*`、`duration`/`durationValue`、`preview`、`reset`、`saveFailed`、`loadFailed`、`monitorsFailed`）。

## 验证

1. `cargo build` 通过且无警告。
2. 启动应用后切换 CapsLock / NumLock / Insert，提示窗应出现在设置的位置并在约 1.2 秒后消失（轮询间隔 100ms，提示最多晚约 0.1 秒）。
3. 把焦点切到别的程序或桌面再切换 CapsLock / NumLock，提示窗仍应按真实状态弹出。
4. 连续快速切换时，提示窗文案同步更新且不会提前隐藏。
5. 主窗口修改位置、大小、颜色、透明度、动画后点「预览提示」，提示窗应立即按新设置出现。
6. 重启应用后设置仍然保留（读取同一份 `settings.json`）。
7. 已实测（截图验证时临时把 `VISIBLE_DURATION` 调到 8 秒，已还原）：
   - 焦点在 VS Code 时切换 CapsLock，提示窗按真实状态显示「大写锁定 / 已关闭」。
   - 自定义 `topLeft` + 偏移 (24, 24) + 缩放 1.4 + 强调色 `#ff5a5f` + 文字 `#ffffff` + 透明度 0.9 时，提示窗为 330×101、位于 (40, 40)，文字为白色、「大写锁定 / 已开启」的状态文字为红色。
   - 默认设置下窗口为 236×72 并跟随光标。
