# 顶部横幅

## 用途

所有消息的显示出口。整条贴顶、跨整个屏幕宽度，不抢焦点、不阻挡点击，鼠标靠近时淡化，点击即消失。

## 窗口

`src-tauri/tauri.conf.json` 的 `banner` 窗口：`features/banner/index.html`，初始 1280×64、`visible: false`、无边框、透明、置顶、跳过任务栏、不聚焦、不可缩放、无阴影。真正的尺寸与位置在每次显示前由 Rust 按目标显示器重算：宽度 = 显示器逻辑宽，高度 = 页面实测高度（24–240，由 `banner_resize` 回传），位置 = 显示器左上角。

Rust 在 `banner::init` 里对窗口调 `set_ignore_cursor_events(true)` 与 `set_focusable(false)`，所以横幅不吃点击、也不会把焦点从别的窗口抢走。

## 显示节奏

横幅页是唯一 display sink，自己持有队列（上限 20，超出丢最旧）：

1. 收到 `banner:message` → 空闲就播，忙就入队；
2. 播放 = `data-state` 从 `idle` 强制重排到 `shown`（入场动画），停留 `durationMs > 0 ? durationMs : 默认时长`；
3. 退场 160ms（`leaving`），`transitionend` 或 400ms 兜底后取下一条；
4. 队列空 → 回调 `banner_done`，Rust 隐藏窗口。

启动竞态：页面先订阅 `banner:message` / `banner:clear` 等事件，再 `invoke("banner_ready")`；Rust 在收到 ready 前把最近一条请求存在 `pending`，ready 到达后立刻显示并补发，避免窗口亮着没内容。

## 光标交互

窗口可见期间，Rust 每 30ms 轮询 `GetCursorPos` 与 `GetAsyncKeyState(VK_LBUTTON)`（隐藏时降到 150ms）：

- 光标进入/离开横幅矩形 → emit `banner:hover` `{ inside }`，页面加 `.is-dimmed`（opacity 0.35，120ms 过渡）；收窗时补发一条 `inside: false` 复位，避免下一条消息带着淡化状态出现。
- 左键由抬起变按下且光标在矩形内 → emit `banner:dismiss`，页立刻走退场。
- 30ms 轮询意味着极快单击（按下并释放 < 30ms）可能漏检，默认时长兜底。

## 设置

`settings.json` 的 `banner` 节：`{ "durationMs": 5000, "monitor": "" }`。`monitor` 为空表示跟随光标所在屏幕，否则是显示器设备名（如 `\\.\DISPLAY1`）。改设置后 Rust 广播 `banner:settings`，已开着的横幅页即时更新默认停留时长。

## 接口

| 命令 | 说明 |
| --- | --- |
| `banner_settings` / `update_banner_settings` | 读 / 写 `{ durationMs, monitor }`，写入后广播 `banner:settings` |
| `banner_ready` | 页面完成订阅后的握手，触发 `pending` 补发 |
| `banner_done` | 队列排空，隐藏窗口 |
| `banner_clear` | 清空 `pending` 并向页面 emit `banner:clear` |
| `banner_resize` | 页面实测高度（夹取 24–240），可见时立即改窗口高度 |
| `banner_show` | 定位 + 显示 + emit `banner:message`（内部缓存 pending） |
| `banner_geometry` | 调试用：`{ visible, rect, cursor }` |

事件（Rust → 横幅页）：`banner:message`、`banner:clear`、`banner:hover`、`banner:dismiss`、`banner:settings`。

前端：`src/features/banner/index.js`（`initBannerFeature`，仅在 `banner` 窗口执行）。API 边界见 `src/shared/api/index.js` 的 `getBannerSettings` / `saveBannerSettings` / `bannerReady` / `bannerDone` / `bannerClear` / `bannerResize` / `getBannerGeometry` / `onBannerMessage` / `onBannerClear` / `onBannerHover` / `onBannerDismiss` / `onBannerSettings`。

## 视觉

`section.banner[data-level]` 一行铺开：左侧 3px 等级信号条（info 用强调色，success/warning/error 用语义色），右侧文本 + 次数元信息（`×N`，用 `--color-secondary`）。底色 `--color-surface-raised`，底边 1px `--color-line`；`prefers-reduced-motion` 下只保留透明度变化。

## 本地化键

文案来自消息自带的 `i18nKey` 或 `text`，横幅自身没有新增文案；调试页的横幅设置文案在 `settings.debug.{banner,bannerVisible,bannerRect,bannerCursor,bannerMissing,bannerMonitor,monitorFollow,monitorPrimary,monitorOption,duration,durationValue,geometryFailed}`。

## 验证

- 发一条消息：窗口出现在目标显示器顶部、整条宽度、文案与等级条正确。
- 同文案连发：显示 `×N` 且只有一条在播。
- 光标移入：opacity 0.35；移出：恢复 1。
- 点击矩形：约 250ms 内窗口隐藏。
- 自定义 `durationMs`（如 1200）到点自动收窗；调试页改默认时长后新消息立即按新值播放。
