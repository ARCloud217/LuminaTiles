# 顶部横幅

## 用途

所有消息的显示出口。贴屏顶、跨显示器宽度、最多同时显示 3 条，不抢焦点、不阻挡点击，鼠标靠近整条淡化，点击某一格只销毁那一格。

## 职责划分

| 侧 | 负责 |
| --- | --- |
| Rust（`src-tauri/src/features/banner/`） | 消息池与队列、重要级排序、挤占、宽度占比、计时、窗口几何与显隐、光标命中 |
| 横幅页（`src/features/banner/`） | 渲染格子、翻译 `i18nKey`、入场退场与重排动画、测量高度与每格矩形回传 |

页面不再自己排队和计时：所有调度结论由 Rust 放进 `banner:state`，页面只负责把状态画出来并把矩形量回去。

## 窗口

`src-tauri/tauri.conf.json` 的 `banner` 窗口：`features/banner/index.html`，初始 1280×64、`visible: false`、无边框、透明、置顶、跳过任务栏、不聚焦、不可缩放、无阴影。每次显示前由 Rust 按目标显示器重算：宽度 = 显示器逻辑宽，高度 = 页面回传高度（`banner_report`，夹取 24–240），位置 = 显示器左上角。

`banner::init` 对窗口调 `set_ignore_cursor_events(true)` 与 `set_focusable(false)`，横幅不吃点击、也不抢焦点。

## 布局

左侧固定 44px 队列计数槽（三条横线图形 + `tabular-nums` 数字，0 也显示 0 并淡化到 `--color-ink-faint`，超过 9 显示 `9+`，不可点、不参与命中），右侧 1–3 个格子：格间距 12px、右内边距 20px、上下 12px；每格左侧 3px 等级信号条 + 单行省略文本 + `×N`。

宽度由 Rust 算好放进 `slots[].width`，页面只当 `flex-basis`：

| 组合 | 宽度 |
| --- | --- |
| 1 条 | 100 |
| 同级 ×2 / 高+中 / 高+低 / 中+低 | 50/50 · 60/40 · 66/34 · 58/42 |
| 同级 ×3 | 33.3/33.3/33.4 |
| 2 高 1 中 / 2 高 1 低 | 38/38/24 · 40/40/20 |
| 1 高 2 中 / 1 高 1 中 1 低 / 1 高 2 低 | 42/29/29 · 50/30/20 · 50/25/25 |
| 2 中 1 低 / 1 中 2 低 | 38/38/24 · 50/25/25 |

3 格且最高等级唯一时，最高格居中并占最多；其余按重要级降序、同级按入场顺序从左到右。

## 重要级与队列

- 重要级 `importance`：`high` / `normal`（缺省）/ `low`，与 `level`（info/success/warning/error，只管颜色）正交。
- 到达时不足 3 条直接上屏；已满 3 条时，只有严格更高的重要级才能挤掉「在场最低等级中最旧」的那条，否则入队。被挤掉、被点击、到期消失的消息都不回队列。
- 队列上限 20，按「重要级降序 + 入场顺序升序」插入，不计时；上屏后给完整时长。队列满时再到达新消息，先销毁屏上最早入场的那条，再按队列顺序补位。
- 同一 `id` 再次到达（去重合并）：在屏就地更新次数/时长并续期、不重播动画；在队列就地更新并按新重要级重排。
- 计时只对在屏格生效（100ms tick），重排不重置；同一 tick 多条到期时按截止时间升序一次性移除再补位。

## 状态同步

- Rust 每次池或队列变化都把 `epoch` +1，广播 `banner:state` `{ epoch, slots: [Message + width], queueCount }`；首次上屏先 emit state 再 `show()`，避免亮着空白窗。
- 页面渲染后测量：整条高度 + 每格矩形（CSS 逻辑像素，相对窗口左上角）经 `banner_report({ epoch, height, slots }) ` 回传。Rust 只接受 `epoch` 相等的报告；重排动画期间 epoch 不匹配时不做命中，避免用旧坐标点到别的格子。
- 池空后页面等最后一格收完再 `banner_idle`，Rust 隐藏窗口；`banner_idle` 在池非空时被忽略，Rust 另有 1000ms 兜底隐藏。
- 启动竞态：页面先订阅 `banner:state` / `banner:hover`，再 `banner_ready`，Rust 在 ready 之前不显示窗口；ready 到达时若池非空则补发一份 state。

## 光标交互

窗口可见期间，Rust 每 30ms 轮询 `GetCursorPos` 与 `GetAsyncKeyState(VK_LBUTTON)`（隐藏时降到 150ms）：

- 光标进出整条矩形 → emit `banner:hover` `{ inside }`，页面给整条加 `.is-dimmed`（opacity 0.35，120ms）。格子没有独立背景，所以淡化的是整条，不是单格。收窗时补发一条 `inside: false` 复位，避免下一条消息带着淡化状态出现。
- 左键由抬起变按下时按 `banner_report` 的矩形命中格子 → 只销毁那一格并补位；命中空白或计数槽则不响应。点击权威归 Rust，页面不判断。
- 30ms 轮询意味着按下并释放都短于 30ms 的极快单击可能漏检，默认时长兜底。

## 动效

整条 `translateY(-100%)` → 就位 180ms、离场 160ms；格宽度变化 180ms 过渡；被移除的格先变「幽灵格」（脱离文档流、绝对定位在旧位置、宽度收 0 + 淡出 160ms），不上报矩形、不参与命中；`prefers-reduced-motion` 下只保留 120ms 透明度。

## 设置

`settings.json` 的 `banner` 节：`{ "durationMs": 5000, "monitor": "" }`。`monitor` 为空表示跟随光标所在屏幕，否则是显示器设备名（如 `\\.\DISPLAY1`）。设置只影响 Rust 调度（默认停留时长、目标显示器），不再向页面广播事件。

## 接口

| 命令 | 说明 |
| --- | --- |
| `message_publish` / `message_dismiss` | 发消息（可带 `importance`）/ 按 id 销毁 |
| `banner_settings` / `update_banner_settings` | 读 / 写 `{ durationMs, monitor }` |
| `banner_ready` | 页面完成订阅后的握手，池非空则补发 state |
| `banner_report` | 页面回传 `{ epoch, height, slots }`，Rust 据此调窗口高度并做命中 |
| `banner_idle` | 页面确认全部退场，Rust 隐藏窗口（池非空时忽略） |
| `banner_clear` | 清空池与队列 |
| `banner_show` | 调试/兼容用：直接投递一条消息 |
| `banner_geometry` | 调试用：`{ visible, rect, cursor, epoch, queueCount, queueHead, slots }` |

事件（Rust → 横幅页）：`banner:state`、`banner:hover`。消息总线事件 `lumina:message` / `lumina:message-dismissed` 仍广播给所有窗口，横幅页不再直接消费。

前端：`src/features/banner/index.js`（`initBannerFeature`，仅在 `banner` 窗口执行）。API 边界见 `src/shared/api/index.js` 的 `bannerReady` / `bannerReport` / `bannerIdle` / `bannerClear` / `getBannerGeometry` / `getBannerSettings` / `saveBannerSettings` / `onBannerState` / `onBannerHover`。

## 视觉

`section.banner` 内是 `.banner__queue`（计数槽）与 `.banner__slots > article.cell[data-level][data-importance]`。每格：3px 信号条（info 用强调色，success/warning/error 用语义色）+ 单行省略文本 + `×N`（`--color-secondary`）。整条底色 `--color-surface-raised`、底边 1px `--color-line`。格子无独立底色，只靠 12px 间隙与 1px 分隔线区分，因此淡化与命中都作用在整条上。

## 本地化键

文案来自消息自带的 `i18nKey` 或 `text`；计数槽的读屏标签用 `banner.queueCount`（"还有 {count} 条消息排队"）。调试页的横幅设置与读数文案在 `settings.debug.{banner,bannerVisible,bannerRect,bannerCursor,bannerMissing,bannerMonitor,monitorFollow,monitorPrimary,monitorOption,duration,durationValue,geometryFailed,bannerEpoch,bannerQueueCount,bannerQueueHead,bannerSlots,bannerSlotEmpty,importance,importanceHigh,importanceNormal,importanceLow}`。

## 验证

- 三条不同重要级：视觉左中右与宽度比例符合上表，最高等级居中。
- 高等级到达、屏上已有低等级：低等级中被挤掉的是最旧那条；同等级到达只入队、屏上不变。
- 队列满 20 后再来一条：屏上最早入场的被销毁，队首补位。
- 光标移入整条：opacity 0.35；移出恢复 1；点击某格只消失该格，点击计数槽与空白无反应。
- `durationMs` 到点自动收格并补位；删空后约 200ms 内窗口隐藏（`banner_geometry.visible` 为 false）。
- 调试页每秒刷新读数：epoch、队列条数、队首、屏上格子（id · level · importance · 宽度）。
