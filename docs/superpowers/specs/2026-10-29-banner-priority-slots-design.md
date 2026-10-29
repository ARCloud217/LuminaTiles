# 顶部横幅：多消息并排与重要级（设计文档）

- 日期：2026-10-29（v2：吸收两轮 builder check）
- 状态：已批准并实施（§16 步骤 1–5 完成，实施记录见文末「实施记录」）
- 相关：[architecture.md](../../architecture.md)、[banner.md](../../features/banner.md)、[messages.md](../../features/messages.md)、[design-language.md](../../design-language.md)、[主题/字体/消息/横幅设计文档](./2026-10-29-theme-font-message-banner-debug-design.md)

## 1. 目的与背景

现在的横幅是严格串行的：[banner/index.js](../../../src/features/banner/index.js) 一次播一条，队列上限 20，播完才播下一条。同时发生的多条消息（保存成功 + 跟随系统提示 + 按键提示不可用）会排队十几秒，重要的那条可能排在后面。

这次让横幅一次并排显示最多 3 条消息，并引入与显示等级（level，决定颜色）**正交**的**重要级**（高 / 中 / 低）：

- 高等级消息优先上屏，能把已在屏上的低等级消息挤掉；
- 同屏消息按重要级分配宽度：等级越高占比越大，同级平分，唯一最高等级居中；
- 最左侧固定一个队列计数，显示还有多少条在等；
- 队列中的消息不计时，按重要级排序；
- 一条横幅内分格并排，**不拆成多个窗口**。

## 2. 目标 / 非目标 / 反目标

目标：

- 最多 3 条同屏并排，宽度按重要级组合分配；
- 高等级可抢占，抢占即销毁被挤者；
- 队列按重要级排序、不计时，空位出现立即补位；
- 每条独立计时、独立点击关闭；
- 左侧队列计数始终可见（0 条时显示 0 并淡化）。

非目标：

- 消息历史、通知中心列表窗口；
- 高等级消息附带提示音；
- 多显示器各一条独立横幅（仍是一条，按设置或光标选屏）；
- 消息分组折叠（×N 之外不再合并）。

反目标（功能可用也算失败）：

- 同一条消息在屏上忽隐忽现（被挤掉后回队列造成的来回弹）；
- 点掉一条顺带清掉另外两条；
- 点错格（命中旧矩形导致清掉旁边那条）；
- 宽度变化时文字跳动、行高不齐、整条上下抖；
- 队列里的消息被计时消耗掉。

## 3. 约束

- 横幅窗口 `set_ignore_cursor_events(true)` + `set_focusable(false)`，页面收不到鼠标事件：悬停与点击只能靠 Rust 的 30ms 光标轮询（[banner.rs](../../../src-tauri/src/features/banner.rs)）。
- 窗口宽度锁显示器宽度，高度由内容撑开（现有 `banner_resize` 夹取 24–240；本次合并进 `banner_report`）。
- 消息文案：页面用 `i18nKey` + `params` 翻译；Rust 不持有文本，只持有翻译键。
- 现有 `message_publish` 调用方（调试页、主题层的系统读取失败提示）不能改签名才可用。
- 视觉令牌与禁用清单以 [design-language.md](../../design-language.md) 为准。
- 工作区无前端构建：`frontendDist` 就是 `src/`，改完刷新窗口即生效。
- 计时只有一份：在 Rust。页面不再本地计时，`defaultDurationMs` 不再下发给页面。

## 4. 职责划分：Rust 管状态与调度，页面管渲染与几何

| 职责 | Rust | 页面 |
| --- | --- | --- |
| 消息池（在屏 3 条）与等待队列 | ✔ | |
| 重要级排序、挤占判定、宽度占比 | ✔ | |
| 每条计时与到期、去重续期 | ✔ | |
| 窗口定位、尺寸、显隐、收窗兜底 | ✔ | |
| 光标轮询、命中整条（悬停）与命中某格（点击） | ✔ | |
| 渲染格子、翻译文案、入场/退场/重排动画 | | ✔ |
| 测量内容高度与每格矩形，回传后端 | | ✔ |

为什么这样切：规则（排序、挤占、占比、计时）是纯逻辑，抽成不碰窗口 API 的纯函数后可用 `cargo test` 覆盖；视觉（动画、省略号、间距）改起来频繁，留页面不必重编译。本文档取代 [主题/字体/消息/横幅设计文档](./2026-10-29-theme-font-message-banner-debug-design.md) §6 的「Rust 只管路由、显示节奏归页面」。

## 5. 数据模型

消息体新增一个字段，其它不变：

```json
{
  "id": "msg-7",
  "level": "warning",
  "importance": "high",
  "i18nKey": "settings.debug.sampleWarning",
  "params": null,
  "origin": "frontend",
  "durationMs": 0,
  "count": 1
}
```

- `importance` 取值 `high` / `normal` / `low`，缺省 `normal`。现有调用方不改也能跑。
- `level` 仍决定颜色（info / success / warning / error），与重要级互不影响：一条 `success` 也可以是 `high`。
- 队列项 = 消息 + 入队序号（同级先进先出）。

## 6. 同屏布局

### 6.1 结构

整条贴屏顶，从左到右：队列计数槽（固定 44px）+ 1–3 个消息格。格与格之间 12px，右内边距 20px，上下内边距 12px。

- **计数槽**（始终占位，避免有/无队列两种宽度导致整条跳动）：44px 内放一个 16×16 的叠层图形（三条不同长度的横线，CSS 画）+ 右侧 2 位数字，`font-variant-numeric: tabular-nums`。数字 0 时照常显示 `0`，整槽淡化到 `--color-ink-faint`；大于 0 时用 `--color-ink-muted`，超过 9 显示 `9+`。计数槽不可点击、不参与命中。
- **消息格**：左侧 3px 等级信号条（沿用 `--banner-signal`）+ 单行文本 + 可选 `×N`。
- 文本单行、超出省略（`text-overflow: ellipsis`），所有格统一行高，保证 3 格底部对齐。
- 格子的渲染顺序 = `banner:state` 里 `slots` 的数组顺序（左→右），页面不自行排序。

### 6.2 宽度组合表

宽度是「去掉计数槽与间距后的剩余宽度」的百分比，由 Rust 算好塞进 `slots[].width`，页面只用作 `flex-basis`。

| 同屏组合 | 左 → 右 |
| --- | --- |
| 1 条（任意等级） | 100% |
| 2 条 · 同等级 | 50 / 50 |
| 2 条 · 高 + 中 | 60 / 40 |
| 2 条 · 高 + 低 | 66 / 34 |
| 2 条 · 中 + 低 | 58 / 42 |
| 3 条 · 同等级 | 33.3 × 3 |
| 3 条 · 2 高 1 中 | 38 / 38 / 24 |
| 3 条 · 2 高 1 低 | 40 / 40 / 20 |
| 3 条 · 1 高 2 中 | 29 / **42** / 29 |
| 3 条 · 1 高 1 中 1 低 | 30 / **50** / 20 |
| 3 条 · 1 高 2 低 | 25 / **50** / 25 |
| 3 条 · 2 中 1 低 | 38 / 38 / 24 |
| 3 条 · 1 中 2 低 | 25 / **50** / 25 |

规则（表只是规则的穷举）：3 格且最高等级唯一时，最高等级**居中**、占最多；其余情况按「等级降序、同级入场早者靠左」排列，同级平分。

### 6.3 动效与「幽灵格」

- 整条入场：`translateY(-100%) → 0` + 淡入 180ms（沿用现有）；同时新格宽度 0 → 目标 180ms。
- 整条退场：最后一格收窄淡出后，整条上滑 160ms，再调 `banner_idle`。
- 格离场（到期 / 点击 / 被挤 / clear）：该格不立刻从 DOM 移除，而是变成**幽灵格**——`flex-basis` 从旧百分比过渡到 0、淡出 160ms，结束后移除。因为其余格同期线性过渡到新占比，总和始终约 100%，不会溢出或跳动。
- 幽灵格**不上报矩形**、不参与命中（见 §9）。
- 重排中途收到新 state：直接改 `flex-basis` 目标值，CSS 过渡自行打断续接，不排队等过渡结束。
- `prefers-reduced-motion: reduce`：所有宽度/位移过渡取消，只保留 120ms 透明度；整条退场 120ms 后立刻 `banner_idle`。

## 7. 调度规则

### 7.1 到达

1. 同 id 已在池中 → 就地更新（见 §7.5），不参与挤占；
2. 在屏格数 < 3 → 直接上屏，开始计时；
3. 在屏 3 格 → 比较新消息与在屏最低重要级：
   - 新消息等级**严格更高** → 挤掉最低等级中最早入场的那条（销毁，不回队列），新消息上屏；
   - 否则 → 入队，不挤。

### 7.2 离场

计时到期 / 本格被点击 / `message_dismiss(id)` / `banner_clear()`。离场后立刻从队首补位（不等页面确认）；没有补位时进入收窗流程。

### 7.3 队列

- 容量 20；插入位置按「重要级降序、同级入场序列升序」；
- 队列不计时；上屏时给完整时长（`durationMs > 0` 用消息自带值，否则用横幅设置的默认值）；
- 溢出（>20，即队列已满又来新消息）时：**销毁屏上最早入场的那一条**（按入场序号，不看等级），再按队列优先级补位；被销毁的那条不回队列；
- 因此队列长度始终 ≤ 20，屏上始终是「最新到达 + 队列里等级最高的那几条」；
- `message_dismiss(id)` 命中队列项时直接移除。

### 7.4 计时

- 只对在屏格计时，每格记 `deadline: Instant`；重排（宽度变化）不重置；
- 调度 tick 100ms，最坏晚 100ms 收起；
- 同一 tick 内多条到期：按 `deadline` 升序（相同则入场序号升序）一次性全部移除，再按队列优先级补位（不做连锁挤占）。

### 7.5 去重到达（同 id 第二次到达）

`messages.rs` 在 800ms 内对同 `level + i18nKey + text` 复用同一个 id 并 `count += 1`，所以同一条消息会以同 id 再次进来。规则：

- 在池中 → 就地更新 `count` 与 `duration_ms`，并把 `deadline` 续期到 `now + duration`（连续刷屏时不至于中途消失）；**不重播入场动画**，页面只更新 `×N`；
- 在队列中 → 就地更新，并按新 `importance` 重新排序；
- 若这条消息随去重带上了新的 `importance`，以新值生效（去重键仍是 level + i18nKey + text）；
- 去重到达不触发 §7.1 的挤占判定，也不会出现「自己挤自己」。

## 8. 交互

- **悬停**：光标进入整条窗口矩形 → 整条淡化到 0.35（120ms），让下面的内容露出来（这是原行为，保留）；离开恢复。不按格淡化——格本体没有独立背景，只淡化一格既露不出桌面，也会让字变灰更难读。
- **点击**：命中某一格 → 只销毁该格（由 Rust 直接改池）；命中计数槽或空白 → 什么都不做。
- 点击只在「当前 state 已有匹配 epoch 的几何报告」时才判定，动画进行中不判（宁可这 180ms 点不到，也不点错格）。
- 窗口整条点击穿透、不抢焦点；横幅不可聚焦、不参与 Tab 序列；文本区 `role="status" aria-live="polite"`。

## 9. 命令与事件

| 方向 | 名称 | 说明 |
| --- | --- | --- |
| 前端 → Rust | `message_publish` | 新增可选 `importance`（缺省 normal） |
| 前端 → Rust | `message_dismiss(id)` | 外部撤销：池/队列命中即移除 |
| 前端 → Rust | `banner_ready` | 页面订阅完成：Rust 立刻补发当前 state |
| 前端 → Rust | `banner_report` | `{ epoch, height, slots: [{ id, x, y, width, height }] }`，CSS 逻辑像素、相对窗口左上角，**只含当前 state 里的格** |
| 前端 → Rust | `banner_idle` | 退场动画结束；池为空时才生效 |
| 前端 → Rust | `banner_clear` | 清空池与队列并直接收窗 |
| Rust → 页面 | `banner:state` | `{ epoch, slots: [Message + width], queueCount }` |
| Rust → 页面 | `banner:hover` | `{ inside: bool }`（整条） |
| 调试 | `banner_geometry` | `{ visible, rect, cursor, slots, queueCount, epoch }` |
| 调试 | `banner_show` | 保留，但改为走 `message_publish` 同一条路径 |

下线：`banner:message`（由 `banner:state` 取代）、`banner:dismiss`（点击由 Rust 直接改池，不再回传页面）、`banner_done`（由 `banner_idle` 取代）、`banner_resize`（并入 `banner_report`）、`banner:clear` 事件、`banner:settings`（计时归 Rust，页面不再需要默认时长）。`banner_settings` / `update_banner_settings` 命令保留，调试页照用。

`banner:state` 示例：

```json
{
  "epoch": 12,
  "slots": [
    { "id": "msg-3", "level": "info", "importance": "low", "i18nKey": "queue.a", "width": 25, "count": 1 },
    { "id": "msg-5", "level": "error", "importance": "high", "text": "保存失败", "width": 50, "count": 1 },
    { "id": "msg-2", "level": "success", "importance": "low", "text": "已保存", "width": 25, "count": 2 }
  ],
  "queueCount": 2
}
```

`epoch` 每次池/队列变化 +1。页面每次渲染 state 时记下它，`banner_report` 原样带回；Rust 只接受 `epoch` 等于当前值的报告，旧报告丢弃，且**没有当前 epoch 的报告时不做点击命中**。

## 10. 线程、锁与状态

- **纯核心**：`banner/scheduler.rs` 里的 `Scheduler`（池 + 队列 + settings）只有纯函数：`arrive(msg, now)`、`tick(now)`、`dismiss(id)`、`clear()`、`snapshot(epoch)`，返回「移除/补位」结果；不碰窗口 API、不 emit，`now: Instant` 由调用方传入，因此可以在单测里任意拨时钟。
- **线程壳**（现有 `banner.rs`）：`Mutex<Runtime>`，`Runtime = { scheduler, geometry: Option<Geometry>, epoch, visible }`；调度线程 100ms tick，光标轮询线程 30ms（仅可见时）。
- **锁纪律**：两个线程都只在「改状态 + 取快照」时短持锁；窗口 API（`set_size` / `show` / `hide`）与 `emit` 一律在放锁之后做。`banner_report` 只写 `geometry`，不阻塞调度。
- **显示顺序**：首次上屏时先 `emit(banner:state)` 再 `show()`，避免亮起来是空白帧；池变空时不立刻 `hide()`，先等 `banner_idle`，超过 1000ms 没等到就兜底隐藏。池非空时到达的 `banner_idle` 直接忽略（防「退场动画撞上新消息」把有内容的窗口收掉）。
- **重放**：页面刷新/重载后调 `banner_ready`，Rust 重发当前 state（epoch 不变），页面按入场动画重新落位；Rust 始终是唯一真源。

## 11. 本地化

新增键（中英同步）：

- `settings.debug.importance`：重要级
- `settings.debug.importanceHigh` / `importanceNormal` / `importanceLow`：高 / 中 / 低
- `settings.debug.bannerQueueCount`：队列条数读数标签
- `settings.debug.bannerQueueHead`：队首消息读数标签
- `settings.debug.bannerSlots` / `bannerSlotEmpty`：屏上格子列表与空值
- `settings.debug.bannerEpoch`：时间线版本读数标签
- `banner.queueCount`：还有 {count} 条待显示（计数槽的 `aria-label` 用）

原有消息键不变。

## 12. 调试页

- 发送区增加重要级选择（高 / 中 / 低，默认中），四个样例按钮与自定义文本都带上它；
- 新增读数：屏上条数、队列条数、队首 id、每格 `{id, level, importance, width}`、当前 epoch；
- `banner_geometry` 的 `slots` 直接用来核对点击命中区域。

## 13. 视觉契约更新

[design-language.md](../../design-language.md) 的「横幅」节改写：多格并排、唯一最高等级居中、单行省略、整条悬停淡化 0.35、按格点击、左侧计数槽、每格一个信号条、格子统一行高。

## 14. 验证

Rust 单元测试（`scheduler.rs` 的 `#[cfg(test)]`，注入 `Instant`）：

- 宽度表 13 行逐一比对（含「1 高 2 中」高居中 42）；
- 挤占：严格更高才挤、同级不挤、挤最低等级中最旧；
- 队列：按等级+序号插入、溢出去最低等级最旧、补位取队首、去重就地更新并重排；
- 计时：重排不重置、队列不计时、到期移除、同 tick 多到期一次性移除、去重续期；
- 快照：`epoch` 递增、`slots[].width` 和为 100、queueCount 正确。

端到端（CDP + 真实窗口，沿用 `.tmp/*.mjs` 脚手架）：

- 3 条并排宽度与表一致、计数槽显示队列数；
- 高等级到达挤掉低等级并上屏；同级到达不挤、进队列；
- 悬停整条淡化到 0.35、离开恢复；
- 点击某一格只清该格、其余补位且不闪；点击空白无反应；
- 幽灵格期间点击不会误伤（命中被屏蔽）；
- 队列补位后宽度重新分配；
- `prefers-reduced-motion` 下无宽度过渡；
- 页面刷新后状态由 Rust 重放。

截图留档：1 格 / 2 格（高+低）/ 3 格（1 高 2 低）/ 队列计数 / 悬停淡化。

## 15. 风险

- **点击命中窗口很短**：动画期间屏蔽命中，用户恰在此刻点击会没反应。接受：宁可不响应，也不点错格。
- **计数槽占位**导致单条消息不再严格居中于屏幕。接受：宽度稳定优先。
- **20% 窄格**只剩省略号。接受：不做 tooltip（穿透窗口里也弹不出来）。
- **页面卡顿**会让动画落后于 Rust：Rust 仍按时改池并重发 state，页面按新 state 追赶。可接受：最终一致。
- **幽灵格**让 DOM 短暂有第 4 个节点：已限定它不计入矩形与命中，只影响 160ms 的绘制。

## 16. 实施顺序

1. `scheduler.rs` 纯核心 + 单元测试（宽度表、挤占、队列、计时、快照）；
2. `banner.rs` 线程壳改造：`Mutex<Runtime>`、state 广播、`banner_report`、命中、显隐兜底、命令下线；
3. `messages.rs` 增 `importance` 与去重续期；
4. 横幅页渲染改造（格子、计数槽、幽灵格、动画、state 镜像）；
5. 调试页（重要级选择 + 池/队列读数）；
6. 文档（architecture、design-language、banner.md、messages.md）与全量验证、截图。

## 17. 已定取舍（我替你拍的，可推翻）

1. `importance` 缺省 `normal`，现有调用方无需改动；
2. 队列溢出销毁屏上最早入场的那条（按你的原话「挤掉最先显示的」），而不是丢弃队内消息；
3. 3 格且最高等级唯一 → 居中占最多；其余按等级降序左→右；
4. 悬停淡化整条（0.35），不按格淡化；点击仍按格；
5. 计数槽固定 44px、始终显示数字（0 也显示）、不可点击；
6. 被挤掉即销毁，不回队列（按你确认的「消失」）；
7. 去重到达续期但不重播动画；
8. 点击由 Rust 直接改池，页面只负责动画（不再有 `banner:dismiss` 事件）；
9. 动画期间屏蔽点击命中，避免点错格；
10. 下线 `banner:message` / `banner:done` / `banner_resize` / `banner:clear` / `banner:settings`，改由带 `epoch` 的 `banner:state` 驱动；
11. 收窗兜底 1000ms（页面不发 `banner_idle` 也不会卡住）；
12. 长文案只做单行省略，不做 tooltip、不做多行展开。

## 18. 实施记录（2026-10-29）

- `src-tauri/src/features/banner/scheduler.rs`：纯调度核心，`cargo test --lib banner::scheduler` 17 个用例通过（宽度表 13 种组合、唯一最高居中、严格更高才挤占、同级入队、队列按等级排序、队列满销毁屏上最早、tick 到期补位、去重续期、队列/在屏 dismiss）。
- `src-tauri/src/features/banner/mod.rs`：调度线程（100ms tick）、光标轮询（可见 30ms / 隐藏 150ms）、`Runtime` 一把 `Mutex`、`epoch` 版本号、`banner_report` 几何回传与 `hit_test` 命中、`banner_idle` 与 1000ms 兜底。
- `src/features/banner/`：计数槽 + 格子渲染、`order` + `flex-basis` 控制重排、幽灵格退场、`banner:state` / `banner:hover` 订阅、`aria-label` 队列读屏。
- 调试页：重要级下拉与 epoch / 队列条数 / 队首 / 屏上格子读数，面板可见时每秒刷新。
- 端到端实测（CDP + 真实光标 / 点击）：三格宽度 N30/H50/L20 且高居中；高等级到达只挤掉最低等级中最旧的那条；同等级只入队（`queueCount` 增加）；队列 20 满后新消息销毁屏上最早入场者并补位；点击命中单格销毁（58/42 复排）、点计数槽无反应；光标进出整条淡化与恢复；默认 5000ms 到期自动收格、删空后窗口隐藏。
- 与 §9 的差异：调试页键名统一为 `settings.debug.banner*` 前缀（见 §11 修订）；新增 `banner.queueCount` 供计数槽的 `aria-label` 使用。