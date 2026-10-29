# 消息中心

## 用途

统一的消息发布与去重入口。前端任何功能、Rust 任何模块都通过它发一条用户可见通知，由顶部横幅负责显示；前端不持有队列，避免同一事件被两个窗口各显示一次。

## 数据模型

```json
{
  "id": "msg-7",
  "level": "info | success | warning | error",
  "importance": "high | normal | low",
  "i18nKey": "messages.themeLocked",
  "text": "纯文本（与 i18nKey 二选一）",
  "params": { "count": 3 },
  "origin": "frontend | rust",
  "durationMs": 0,
  "count": 1
}
```

- `id` 由 Rust 分配（`msg-<n>` 单调递增），前端不要自己造。
- `importance` 缺省为 `normal`；它决定排序、挤占与宽度占比，`level` 只管颜色，两者正交（见 [banner.md](banner.md)）。
- `durationMs`：`0` 或缺省表示跟随横幅设置（默认 5000ms）；显式值夹取到 500–60000。
- `level` 白名单校验，未知等级报「未知消息等级：…」；`i18nKey` 与 `text` 都为空报「消息必须提供 i18nKey 或 text」。反序列化失败报「消息格式错误：…」。

## 路由

1. `publish(msg)`（`src/shared/messages/index.js`）→ `invoke("message_publish", { message })`。
2. Rust（`src-tauri/src/features/messages.rs`）分配 id、去重、向**全部窗口** emit `lumina:message`，然后调用 `banner::dispatch` 交给横幅调度器（在屏/队列由 Rust 决定，页面只负责画）。
3. 各窗口的 `connect()` 订阅 `lumina:message` / `lumina:message-dismissed` 并广播给 sink；发布者本地不入队，所以不会出现两份。
4. `dismiss(id)` → `invoke("message_dismiss", { id })` → Rust 广播 `lumina:message-dismissed`，并从横幅调度器移除该 id（在屏则销毁该格、补位；在队列则移出）。

去重：800ms 内同 `level` + `i18nKey` + `text` 视为同一条，递增 `count` 并复用 id（横幅显示 `×N`）。文本消息没有 `i18nKey`，所以必须把 `text` 一起比较，否则不同文案会被误合并。重复到达时 `durationMs` 会更新；只有请求显式带 `importance` 才覆盖原值，随后横幅调度器就地续期/重排，不重播动画、不参与挤占。

## 接口

| 命令 | 说明 |
| --- | --- |
| `message_publish` | 唯一发布入口，`{ message: { level, importance?, i18nKey?, text?, params?, durationMs? } }`，返回分配后的 `Message` |
| `message_dismiss` | 撤销指定 id，广播 `lumina:message-dismissed` |

事件：`lumina:message`（`Message`）、`lumina:message-dismissed`（`{ id }`）。

前端：`src/shared/messages/index.js` 导出 `publish` / `dismiss` / `subscribe`（别名 `registerSink`）/ `subscribeDismissed` / `connect()`。API 边界见 `src/shared/api/index.js` 的 `messagePublish` / `messageDismiss` / `onMessage` / `onMessageDismissed`。

## 本地化键

消息按 `i18nKey` 取文案，键放在语言包顶层 `messages` 节；当前有 `messages.systemAccentFailed`（跟随系统时读不到系统主题，主题层发出 warning）。新增消息同时补 `zh-CN` 与 `en-US`，并在这里登记。

## 验证

- 同文案连发两次：同 id、`count=2`，横幅显示 `×2` 且不重播入场动画；不同文案分别占用横幅格子或排队。
- 带 `importance` 发布：横幅按重要级排布宽度（高等级居中占最多），同等级消息只入队不挤占。
- 发一条空 `i18nKey` 与空 `text` 的消息：返回错误「消息必须提供 i18nKey 或 text」。
- 两个窗口都收到 `lumina:message`，但横幅只出现一次（调度器只有一份池）。
- `message_dismiss(id)` 能立刻销毁屏上那一格或把排队中的那条移出队列。