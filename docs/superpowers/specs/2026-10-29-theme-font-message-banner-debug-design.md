# 主题、字体、消息中心、顶部横幅与调试页 · 设计文档

- 日期：2026-10-29（v3：按 builder check 与用户三项拍板修订；v2 = git a43ebfc）
- 状态：**待批准**（未获批准前不写任何产品代码）
- 项目：LuminaTiles（Tauri 2 / Windows）
- 关联：`docs/design-language.md`、`docs/architecture.md`

## 0. 一句话

把「一套颜色」变成「四个种子推导出的整套颜色」，换掉界面字体，加一个面向用户的消息中心和它的第一个出口——从屏幕顶部降下的横幅，并把这四样东西挂到主窗口新增的「调试」页上，附一条「退出程序并清除数据」。

## 1. 目标

1. **主要设置**：设置坞新增「主要设置」分组，管理主强调色、副强调色、主背景色、副背景色四个种子；可开关「跟随系统主题」（明暗与强调色一起跟随，跟随期间种子只读）。其余颜色令牌全部由这四个种子推导，功能配色默认跟随。
2. **字体**：界面字体使用 HarmonyOS Sans SC，打包 Regular / Medium / Bold 三档（§5）。
3. **消息中心**：程序内部消息总线，前端与 Rust 都能发布、撤销，画面出口可插拔；本期不做独立 UI。
4. **顶部横幅（通知样式 1）**：屏幕宽度整条、高度随字号变化、从顶部降下的消息横幅；文字渲染与后续特色动画各自独立、可扩展。
5. **调试页**：主窗口设置区新增「调试」选项卡，能试发消息、试主题、看诊断读数，并提供「退出程序，清除并重置数据」。

非目标：主题预设包与导入导出、横幅堆叠/滑动切换、消息历史 UI、托盘通知。

## 2. 现有基础（决定方案边界）

- 颜色：`src/shared/styles/tokens.css` 45 行、13 个颜色令牌；31 处 `var(--color-accent)` / `color-mix()` 走令牌。**但**另有约 14 处写死的 rgba 叠加（白光 wash 与提示卡底色），见 §4.6——它们必须一并改成令牌，否则浅色 tone 会露馅。
- 设置存储：`settings.json` 由 `lock_indicator.rs` 私有结构 `SettingsFile { lock_indicator }` 整份读写（`src-tauri/src/features/lock_indicator.rs:140-145`、`:356-370`），**不认识的新顶层键会被抹掉** → 本次必须抽共享存储层（§9）。
- 前端结构：`src/main.js`（202 行）`FEATURES` 注册表 + `buildLattice` + `initSettingsNav` + `startClock` + `bootstrap`（按 `documentElement.dataset.window` 过滤，`pagehide` 执行 cleanups）。
- API 边界：`src/shared/api/index.js`（82 行）唯一 Tauri 入口。
- 窗口：`main`（880×620，min 640×480）与 `lock-indicator`（transparent / alwaysOnTop / skipTaskbar / decorations false / focus false / resizable false / shadow false / visible false，Rust 侧再 `set_ignore_cursor_events(true)` + `set_focusable(false)`）。
- 运行环境：WebView2 = Chromium 156。已实测通过：`oklch(from var(--seed) …)`、`calc()` 内的 l/c、`color-mix(in oklab, …)`；自定义属性改动会实时重算；把推导后的颜色串交给 1×1 canvas 能取回 8 位 sRGB（对比度体检用这条路）。
- 字体：本机只装了 `HarmonyOS Sans SC Bold`（8,158,996 字节），GDI 家族名 `HarmonyOS Sans SC`。用户已提供完整字体目录 `D:\_File\Font\HarmonyOS Sans\HarmonyOS_Sans_SC`（Thin/Light/Regular/Medium/Bold/Black 各约 8MB，附 `LICENSE.txt`）→ 本次打包其中三档，不再依赖本机安装（§5）。

## 3. 方案总览

| 块 | 产物 | 关键文件 |
| --- | --- | --- |
| 共享设置层 | 顶层键合并写入，保护未知键 | `src-tauri/src/settings.rs`（新），`lock_indicator.rs` 改用它 |
| 主题 | 种子 → 令牌推导 + 明暗档位 + 系统强调色 | `src/shared/styles/theme.css`、`src/features/theme/`、`src-tauri/src/features/theme.rs` |
| 字体 | `@font-face`（打包三档）+ 字体栈 + 调试读数 | `src/shared/fonts/`、`src/shared/styles/fonts.css` |
| 消息中心 | 单一权威路由 + 前端总线 + Rust 命令 | `src/shared/messages/index.js`、`src-tauri/src/features/messages.rs` |
| 横幅 | 新窗口 + 渲染页 + 页内队列 | `src/features/banner/`、`banner` 窗口、`src-tauri/src/features/banner.rs` |
| 调试页 | 设置坞 03 + 重置命令 | `src/features/debug/`、`src-tauri/src/features/debug.rs` |

原则不变：Tauri 调用只经 `shared/api`；可见文本只经语言包；视觉值只来自令牌。

## 4. 主题：四个种子 → 整套令牌

### 4.1 数据模型

`settings.json` 新增顶层键 `theme`：

```json
{
  "theme": {
    "primaryAccent": "#e9a23b",
    "secondaryAccent": "#3f7bd6",
    "backgroundPrimary": "#06070a",
    "backgroundSecondary": "#0a0d12",
    "tone": "auto",
    "followSystem": false
  }
}
```

- `tone`: `"auto" | "dark" | "light"`；`auto` = 由 `backgroundPrimary` 的相对亮度判定（< 0.18 → dark）。`followSystem=true` 时本字段被忽略，面板上的明暗选择器置灰。
- `followSystem`（用户拍板「一起跟随，但不强制」→ 追问后定为「跟随期间系统值优先」）：开启后四个种子全部来自系统——
  - 明暗 = `AppsUseLightTheme`；
  - 主强调色 = 系统强调色；
  - 主/副背景 = 系统明暗对应的内置背景对：深 `#06070a` / `#0a0d12`，浅 `#f4f5f8` / `#eaecf1`；
  - 副强调色 = `oklch(from <系统强调色> l calc(c * 0.9) calc(h + 150))`，不可编辑。
- 跟随期间面板四个种子输入框**只读**并显示系统值，附说明「跟随系统期间颜色由系统提供，关闭跟随即可自定义」；用户自己的四个种子原样留在 `settings.json`，关闭跟随后立即恢复（不清空、不覆盖）。
- 跟随期间写种子一律拒绝：`update_theme_settings` 收到 `followSystem=true` 且携带种子变更时返回错误码 `theme_locked_by_system`，不静默忽略；「关闭跟随」本身始终允许。
- 四个颜色必须是 6 位十六进制；非法值回退默认并记一次警告（不静默吞掉）。
- 默认值 = 上表（琥珀 + 深色），与现有视觉一致。

### 4.2 令牌推导

`tokens.css` 保留非颜色令牌与颜色兜底值（防止 theme.css 加载失败）；`theme.css` 之后加载，用种子重定义同名变量 → 31 处消费方零改动。

```css
:root {
  --seed-accent: #e9a23b;
  --seed-accent-alt: #3f7bd6;
  --seed-bg: #06070a;
  --seed-surface: #0a0d12;

  --tone-depth: 0.025;          /* bg-deep 相对 bg */
  --tone-lift: 0.0375;          /* surface-raised 相对 surface */
  --tone-ink-l: 0.945;
  --tone-ink-muted-l: 0.598;
  --tone-ink-faint-l: 0.45;
  --tone-line-lift: 0.089;
  --tone-line-strong-lift: 0.166;
  --tone-wash-strength: 0.06;   /* 白光/暗光叠加的强度：深色底用白光，浅色底用暗光 */
  --tone-wash-color: 255 255 255;
}
[data-theme-tone="light"] {
  --tone-depth: 0.020;
  --tone-lift: -0.035;
  --tone-ink-l: 0.235;
  --tone-ink-muted-l: 0.495;
  --tone-ink-faint-l: 0.625;
  --tone-line-lift: -0.06;
  --tone-line-strong-lift: -0.12;
  --tone-wash-strength: 0.05;
  --tone-wash-color: 15 18 24;
}
```

> **实施期标定（2026-10-29）**：本文初稿把旧 `tokens.css` 的 `--color-surface-raised`（`#10151c`）误当成「副背景种子」，而旧 `--color-surface` 实际是 `#0a0d12`；照初稿的 0.30 / 0.32 / 0.55 档位，默认主题的 `surface-raised` 会被推到 `rgb(91,98,107)` 这类中灰，与现有视觉差一个量级。落地做法：副背景种子改回 `#0a0d12`，再用二分法在真实 WebView2 里按默认种子反解各档位，使默认主题逐通道复现旧令牌（实测每通道偏差 ≤6/255）；浅色档位同理标定。调整档位前请先跑 `node scripts/snapshot-theme.mjs <CDP端口>` 复核，不要凭手感调。

推导表（一套表达式，两种 tone 通用）：

| 令牌 | 推导 |
| --- | --- |
| `--color-accent` | `var(--seed-accent)` |
| `--color-accent-soft` / `-glow` | `color-mix(in oklab, var(--seed-accent) 14% / 35%, transparent)` |
| `--color-secondary` / `-soft` / `-glow` | 同法，种子换成 `--seed-accent-alt` |
| `--color-bg` / `--color-surface` | 直接取种子 |
| `--color-bg-deep` | `oklch(from var(--seed-bg) calc(l - var(--tone-depth)) c h)` |
| `--color-surface-raised` | `oklch(from var(--seed-surface) calc(l + var(--tone-lift)) calc(c * 1.05) h)` |
| `--color-ink` / `-muted` / `-faint` | `oklch(from var(--seed-bg) var(--tone-ink-*-l) calc(c * 0.6) h)` |
| `--color-line` / `-strong` | `oklch(from var(--seed-surface) calc(l + var(--tone-line-*-lift)) calc(c * 1.1) h)` |
| `--color-wash` | `rgb(var(--tone-wash-color) / var(--tone-wash-strength))` |
| `--color-wash-soft` | 同色、强度 × 0.5 |

语义色不参与推导（红必须像红），按 tone 取两档明度：`--color-danger`、`--color-success`、`--color-warning`。

`--color-secondary` 定位「第二信号」，**本期只在一处消费**：横幅排队计数文字与调试页试色块。主界面不引入第二个强调色（遵守 design-language 禁令）；「能改但看不见」在文档里明确记录，作为后续功能（图表）的预留。

### 4.3 系统主题

Rust 命令 `system_theme` → `{ accent: "#rrggbb" | null, light: bool | null }`：

- 强调色读 `HKCU\SOFTWARE\Microsoft\Windows\DWM\ColorizationColor`（DWORD ARGB，取低 24 位）；为 0 时回退 `HKCU\SOFTWARE\Microsoft\Windows\DWM\AccentColor`（ABGR，取低 24 位后交换 R/B）。
- 明暗读 `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme`（0 = 深色）。
- 用 `windows` crate 的 `Win32_System_Registry`（Cargo features 补一项，不引新 crate）。单个读取失败该字段返回 `null`；两个都失败则前端保留用户种子并打一条 warning 消息。
- 本机实测：`ColorizationColor = 0xc40078d4`（→ `#0078d4`）、`AccentColor = 0xffd47800`、`AppsUseLightTheme = 0`。开启跟随会立刻从琥珀变系统蓝并保持深色——预期行为，验收以此为准。
- 跟随是实时还是惰性：本期只在**开关切换、应用启动、调试页点「重新读取」**时读一次系统值，不订阅系统主题变更事件（订阅需要窗口消息钩子，收益不足）。

### 4.4 首帧不闪

`index.html` 内联同步脚本给 `<html>` 加 `data-theme-pending`；CSS 里 `html[data-theme-pending] body{opacity:0}`，底由 `html{background:兜底}` 铺深色；主题应用后移除该属性，body 120ms 淡入。

`ponytail:` 不上 `WebviewWindowBuilder::from_config` + `initialization_script`——那要把所有窗口改成运行时代码创建，只为省一次 120ms 淡入。

### 4.5 对比度体检

读 `getComputedStyle` 的 `--color-bg`/`--color-ink`/`--color-ink-muted`/`--color-accent` **原始串**（自定义属性给出的是已替换代 var 的文本），交给 1×1 canvas 取 8 位 sRGB（不自己写色空间转换），算 WCAG 对比度：正文、次要文字、强调色三条读数常驻面板；正文 < 4.5 或强调色 < 3 标红警告，**仍允许保存**。不静默通过。

### 4.6 清掉现有写死的叠加色

新增 `--color-wash` / `--color-wash-soft` 后，替换约 14 处写死 rgba：

- `src/shared/styles/shell.css:65,77,235,296,351,448,452`
- `src/features/lock-indicator/styles.css:26,27,41`
- `src/features/lock-indicator/settings.css:25,39,155,168`

规则：白光叠加 → `--color-wash`；强调色混合 → `color-mix(in oklab, var(--color-accent) N%, transparent)`；提示卡底色 `rgba(8,11,15,.95)` → `color-mix(in oklab, var(--color-surface) 95%, transparent)`。改完 `design-language.md` 的「禁止硬编码色值」才真正成立。

### 4.7 浅色 tone 的其余接线

- `documentElement.style.colorScheme` 随 tone 切（原生控件、滚动条跟随）。
- 主窗口原生标题栏：`theme:changed` 时 Rust 对 `main` 调 `set_theme(Light|Dark)`；横幅与提示窗无装饰，不涉及。
- 面板文案：`settings.theme.toneAuto` 说明「按主背景亮度自动判定」，并显示判定结果。
- 跟随系统时：明暗选择器与四个种子置灰，面板显示「当前由系统提供（深色 / 强调色 #xxxxxx）」，并给出「关闭跟随」按钮；`tone` 字段保持原值不动，供关闭后恢复。


### 4.8 与 `lockIndicator.accent` 的关系

字段保留：`null`/缺失 → 提示窗用 `--color-accent`；非空 → 用自身值（不破坏现有设置）。design-language 补一句：功能级自定义色是例外，默认必须跟随主题。

## 5. 字体

- 打包三档，源目录 `D:\_File\Font\HarmonyOS Sans\HarmonyOS_Sans_SC`：

| 文件 | 字重声明 | 用途 | 体积 |
| --- | --- | --- | --- |
| `HarmonyOS_Sans_SC_Regular.ttf` | 400 | 正文、控件 | 7.88MB |
| `HarmonyOS_Sans_SC_Medium.ttf` | 500 600 | 小标题、现有 5 处 `font-weight: 600` | 7.85MB |
| `HarmonyOS_Sans_SC_Bold.ttf` | 700 | 展示体（LUMINA TILES、页标题） | 7.78MB |

合计约 23.5MB，存放 `src/shared/fonts/`，同目录放源许可 `LICENSE.txt`（Huawei Device Co., Ltd. HarmonyOS Sans Fonts License Agreement，允许随产品分发，需保留声明）。

- `src/shared/styles/fonts.css` 里三个 `@font-face`，家族名统一 `HarmonyOS Sans SC`（与系统登记名一致，打包与已装两条路都能命中），`font-weight: 400 / 500 600 / 700`，`font-display: swap`，`src` 指向 `../fonts/*.ttf`；本项目 `frontendDist` 就是 `src/`（无打包器），相对路径直接生效，`cargo tauri build` 后确认安装包内存在三个 ttf。
- 令牌：`--font-family-base` / `--font-family-display` 首位改为 `"HarmonyOS Sans SC"`，回退链 `"Segoe UI Variable Text"` → `"Segoe UI"` → `"Microsoft YaHei UI"` → `sans-serif`。`--font-family-mono` 保持 Cascadia Mono（时钟与数值需要等宽对齐，功能需求）。
- 命中验证：`document.fonts.ready` 后用 canvas 与「不存在的家族」比宽度差（`document.fonts.check()` 返回 true 不可作为证据）；调试页显示「已命中 / 回退到 X」。
- 不做子集化（本期）。若日后安装包体积成为问题，再用 `pyftsubset` 按实际字符集裁剪。

## 6. 消息中心

### 6.1 消息模型

```js
{
  id: "msg-7",                     // Rust 分配（全局唯一、单调递增）
  level: "info" | "success" | "warning" | "error",
  i18nKey: "messages.hookFailed",  // 与 text 二选一
  text: "…",
  params: { value: "Caps Lock" },
  origin: "frontend" | "rust",
  durationMs: 0,                   // 0 或缺失 = 跟随横幅设置（默认 5000ms）
  count: 1                         // 去重合并计数
}
```

### 6.2 单一权威路由（谁拥有队列）

**Rust 是唯一权威，前端总线不本地入队。** 一次发布只走一条路：

1. 任意窗口调 `publish(msg)`（`src/shared/messages/index.js`）→ `invoke("message_publish", { message })`；
2. Rust：分配 id、做去重（800ms 内同 `level + i18nKey` 递增 `count` 而非新消息）、向**全部窗口** emit `lumina:message`；
3. 各窗口总线收到 `lumina:message` 后广播给 sink。发布者不会拿到两份，因为发布时本地不入队。
4. `dismiss(id)` → `invoke("message_dismiss", { id })` → Rust 广播 `lumina:message-dismissed`。

前端总线 `src/shared/messages/index.js` 导出：`publish` / `dismiss` / `subscribe` / `registerSink` / `connect()`（订阅 `lumina:message` 与 `lumina:message-dismissed`，返回清理函数）。sink 抛错只记日志，不影响其它 sink。

### 6.3 显示队列归横幅页

横幅页是唯一 display sink，自己持有显示队列（上限 20，超出丢最旧）：空闲立刻播，忙则入队；播完取下一条。

Rust **不**管显示节奏，只管路由与去重，外加一件窗口级的事：路由完成后调 `banner_show`（定位 → 窗口可见 → emit `banner:message`）。这样「窗口何时出现」由 Rust 掌握，「显示多久、下一条什么时候播」由页面掌握，两边不争同一个状态。

调试页的「清空队列」→ `invoke("banner_clear")` → Rust 向横幅页 emit `banner:clear`；横幅页清空队列、播完当前条的退场后隐藏。调试页不显示跨窗口实时队列（跨窗口读要加状态回传，收益不足）；它只累计「本次会话已发送 N 条」。

### 6.4 启动竞态

横幅页在 `bootstrap` 里**先** `listen("banner:message")` + `listen("banner:clear")`，再 `invoke("banner_ready")`。Rust 收到 ready 之前把最近一条 `banner_show` 请求缓存在 `pending`；ready 到达后立刻定位窗口、show、补发。避免「窗口亮着没内容」。

### 6.5 与现有反馈的关系（本期只做加法）

本期**不迁移**现有用户可见反馈：按键提示窗（CapsLock 等三类）与设置页的错误文案保持现状，横幅也不显示它们（否则同一次按键会两条通知）。消息中心是新增通道，第一个真实消费者是横幅；后续若要迁移，逐个功能把 emit 换成 `publish`。

## 7. 顶部横幅（通知样式 1）

### 7.1 窗口

```json
{
  "label": "banner",
  "url": "features/banner/index.html",
  "width": 1280,
  "height": 64,
  "visible": false, "decorations": false, "transparent": true,
  "alwaysOnTop": true, "skipTaskbar": true, "focus": false,
  "resizable": false, "shadow": false, "center": false
}
```

与提示窗一致地在 Rust 侧 `set_ignore_cursor_events(true)` + `set_focusable(false)`：**横幅不接收鼠标与键盘**，也不抢焦点。因此横幅上的悬停与点击只能由 Rust 侧轮询判定，见 §7.2。

### 7.2 点击穿透 + 光标靠近淡化 + 点击消失

整条横幅是屏幕宽、贴顶、置顶的；若接收点击，它会在显示期间吞掉屏幕顶部约 60px 的所有点击（最大化窗口标题栏、浏览器标签栏、上置任务栏）。所以保留点击穿透，再用一条**轮询线程**把交互补回来（用户拍板：「点击穿透，光标靠近降低透明度，点击消失，也可以通过其它事件来控制自身消失，默认 5s 消失」）：

- 线程：`banner` 窗口创建后起一个约 30ms 的循环（与 `lock_indicator` 的按键轮询同风格），每轮 `GetCursorPos` + `GetAsyncKeyState(VK_LBUTTON)`。**仅横幅可见时运行**，隐藏后暂停（不常驻）。
- 悬停淡化：光标落在横幅矩形内 → emit `banner:hover { inside: true }`，离开 → `false`。横幅页给 `section.banner` 加 `.is-dimmed`（`opacity: 0.35`，120ms 过渡），让下方内容透出来。窗口自身不改透明度（Tauri 2 没有 per-window opacity API）。
- 点击消失：光标在横幅矩形内、且左键由抬起变为按下（记录上一帧状态）→ emit `banner:dismiss`；页立刻播退场并回调 `banner_done`，队列继续下一条。30ms 轮询意味着极快单击（按下并释放 < 30ms）可能漏检——可接受，默认时长仍会兜底。
- 其它事件消失：`banner_clear`、`lumina:message-dismissed` 命中当前条，都走同一条退场路径；退场后队列为空则 `hide`。
- 默认停留 **5000ms**（用户拍板），调试页滑杆可改 1000–30000。
- 本期**不做 `sticky`**：需要久留就把时长调到 30s。以后要常驻，再加托盘菜单「关闭当前消息」这条显式通道。

### 7.3 几何

- 宽度 = 目标显示器逻辑宽度，`x = monitor.position().x`，`y = monitor.position().y`（贴顶）。
- 高度 = 内容驱动：横幅页 `ResizeObserver` 测高 → `banner_resize(height)`（24–240 校验）；宽度锁屏宽，避免横向抖动。
- 默认目标显示器 = 光标所在屏（与提示窗一致）；调试页可切「主显示器」。
- 坐标单位沿用提示窗那套物理像素定位实现（`lock_indicator.rs:522-558`），不新造一套换算。

### 7.4 视觉

- 结构：`section.banner[data-level]` > `span.banner__signal`（左 3px 等级条）+ `div.banner__content` > `p[data-banner-text]` + `span[data-banner-meta]`（排队计数，用 `--color-secondary`）。
- 底 `--color-surface-raised`，底部 1px `--color-line`；等级色 info → accent、success → `--color-success`、warning → `--color-warning`、error → `--color-danger`。
- 字号取自令牌（`--font-size-body`），高度 = 行高 + 上下 12px padding → 字号变则整条高度变。
- 淡化：`section.banner.is-dimmed { opacity: 0.35 }`，120ms 过渡；`prefers-reduced-motion` 下无过渡。
- 文字渲染独立成 `renderMessage(message)`，后续特色动画只包这一层，不动窗口与几何。

### 7.5 时间线

1. Rust：定位 → show → emit `banner:message`（窗口已 ready 才发，否则进 `pending`，§6.4）；
2. 横幅页渲染后入场 `translateY(-100%) → 0`，180ms `--ease-out`；
3. 停留 `durationMs`（默认 5000；调试页可调）；期间光标进入则淡化（§7.2），点击或外部事件可提前退场；
4. 退场 `translateY(-100%)` 160ms，`transitionend` + 定时兜底 → 回调 `banner_done`（Rust 只在队列空时 hide 窗口）；
5. 下一条：同窗口内先渲染再播入场，不闪；淡化状态在每条开始时复位。

`prefers-reduced-motion` 下入场/退场改为纯透明度。

## 8. 调试页（设置坞 03）

1. **消息中心**：四等级发送按钮、自定义文本输入、停留时长滑杆、「清空队列」、「本次会话已发送 N 条」。
2. **主题**：四个种子即时试色（不落盘）、当前 tone 判定结果、三个对比度读数、`system_theme` 返回值（强调色 + 明暗）、「跟随系统」开关、「重新读取系统主题」；跟随时种子输入框 disabled 并显示系统值。
   另加一行**横幅体检**：当前横幅矩形（x/y/w/h）与最近一次轮询读到的光标位置，便于确认悬停/点击判定区域正确。
3. **字体**：`@font-face` 就绪状态、实际命中家族、与回退家族宽度差、当前 `--font-family-base`。
4. **危险区**：「退出程序，清除并重置数据」→ 行内二次确认 → `invoke("reset_app_data_and_exit")`：删除 `settings.json` → 记录日志 → `app.exit(0)`。

顶部一行说明「仅开发期使用」；后续可用 `FEATURES` 开关关掉。

## 9. 共享设置层（必须动现有后端）

新增 `src-tauri/src/settings.rs`：

- `read_root() -> serde_json::Value`：整份读入，缺失文件返回 `{}`；
- `patch(section: &str, value: Value)`：读 → 覆盖该顶层键 → tmp + rename 写回，**保留其它顶层键**；
- `reset()`：删除文件。

`lock_indicator.rs` 的 `SettingsFile` 读写改为走该模块（`load_settings` / `save_settings` 语义不变），`theme.rs` 同样用它。这是本期唯一必须修改的现有可用功能；回归范围仅「按键提示设置保存/读取/恢复默认」。

## 10. 新增命令与事件

| 类型 | 名称 | 说明 |
| --- | --- | --- |
| 命令 | `theme_settings` / `update_theme_settings` / `reset_theme_settings` | 主题种子的读 / 校验保存 / 恢复默认（跟随系统期间带种子的写入被拒，错误码 `theme_locked_by_system`） |
| 命令 | `system_theme` | 读系统强调色与明暗（字段级失败返回 null） |
| 命令 | `message_publish` / `message_dismiss` | 唯一发布与撤销入口 |
| 命令 | `banner_ready` / `banner_done` / `banner_clear` / `banner_resize` | 横幅页生命周期与尺寸 |
| 命令 | `banner_settings` / `update_banner_settings` | 横幅默认停留时长与目标显示器（设置坞 03 需要持久化；写入后广播 `banner:settings`） |
| 命令 | `banner_geometry` | 调试页体检：`{ visible, rect, cursor }` |
| 命令 | `banner_show` | 定位 + 显示 + emit（内部缓存 pending） |
| 命令 | `reset_app_data_and_exit` | 清数据并退出 |
| 事件 | `lumina:message` / `lumina:message-dismissed` | 消息总线（全部窗口） |
| 事件 | `theme:changed` | 种子或 tone 变化，各窗口即时换肤 |
| 事件 | `banner:message` / `banner:clear` / `banner:hover` / `banner:dismiss` / `banner:settings` | Rust → 横幅页（含轮询出的悬停与点击、设置变更） |

## 11. 语言包新增键

实施后的键清单以语言包与功能文档为准（本节初稿只列了估算集合，落地时按面板实际文案补齐）：`settings.theme.*`（含 `systemThemeReading` / `seedsLocked` / `saveFailed`）、`settings.debug.*`（含 `fontState` / `yes` / `no` / `sentCount` / `monitorFollow` / `bannerMissing` 等）、`messages.systemAccentFailed`。中英同步、键序一致；逐项见 [features/theme.md](../../features/theme.md)、[features/debug.md](../../features/debug.md)。

## 12. 文档同步

- `docs/design-language.md`：颜色令牌表改为「种子 + 推导 + wash」，补等级语义色与横幅规则（整条、贴顶、左信号条、点击穿透、光标靠近淡化、点击即消失、默认 5s）、字体三分工更新（HarmonyOS Sans SC 承担展示与正文）；删除「禁止引入字体文件」与「不做第二套配色」两句——前者已被打包取代，后者改为「浅色 tone 是同构推导，不是另做一套」。
- `docs/architecture.md`：窗口表加 `banner`；结构加 `settings.rs`、`messages`、`theme`、`debug`；命令与事件表更新。
- 新增 `docs/features/theme.md`、`messages.md`、`banner.md`、`debug.md`。
- `AGENTS.md`：美术风格节补「功能配色默认跟随主题，禁止另起色值；新增叠加色必须走 wash 令牌」。

## 13. 验证计划

- `cargo build` 无警告；`cargo tauri dev` 手动走查。
- CDP（`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>` + `/json/list`）：
  1. 换种子后 `--color-ink` / `--color-line` 的 computed 值随之变化，正文对比度 ≥ 4.5；
  2. `tone=light` 时 `color-scheme`、wash、提示卡与主窗文字全部可用（无深底浅字残留）；
  3. `document.fonts` 就绪后与不存在家族宽度差 ≠ 0；
  4. 横幅 `outerWidth == 显示器逻辑宽`、`outerY == 显示器顶边`，改字号后窗口高度跟随；
  5. 连发 3 条：逐条播完、无重叠、计数正确；
  6. 先发消息再让横幅页冷启动：ready 后补发，无空窗；
  7. 保存按键提示设置后 `theme` 节仍在（回归 §9）；
  8. 重置数据后 `settings.json` 消失、进程退出、重启回默认主题。
- 字体三档：400 / 600 / 700 三段文本的 canvas 宽度互不相同（不是同一份字形被合成加粗）。
- 跟随系统：开启后生效值为系统强调色 + 系统明暗对应背景，四个种子输入框 disabled；关闭后恢复用户原种子；开启状态下调 `update_theme_settings` 改种子返回 `theme_locked_by_system`。
- 横幅交互：光标移入横幅矩形 → `section.banner` 带 `.is-dimmed`（computed opacity ≈ 0.35）；在矩形内按下左键 → 收到 `banner:dismiss` 且 160ms 内退场；横幅隐藏后轮询线程停止（CPU 回到基线）。
- 主窗 880×620 与 640×480 无横向滚动；i18n 0 漏翻。

## 14. 风险

1. 字体体积：打包三档约 23.5MB，安装包显著变大（接受；日后可子集化）。
2. 横幅 `alwaysOnTop` 会压在全屏应用上（与提示窗同）；若要在游戏全屏时静默，需前台全屏检测，本期不做。
3. `oklch(from …)` 依赖 WebView2 ≥ Chromium 119（本机 156）；旧环境退化为 tokens.css 兜底值，仍可读。
4. 主题必须在 `initI18n` 之前应用，因此主题代码不得依赖 `t()`，错误文案延后到 i18n 就绪再展示。
5. 光标轮询线程：仅在横幅可见时以 30ms 频率运行（每秒两次 Win32 调用），隐藏后暂停；若发现空转占 CPU，退到 60ms。

## 15. 实施顺序（v3）

1. 字体资产：三档 ttf + `LICENSE.txt` 复制进 `src/shared/fonts/`，`fonts.css` 三档 `@font-face`，栈首位换掉（一次性体积代价，先落地再谈子集化）。
2. `settings.rs` 共享层 + `lock_indicator.rs` 迁移（回归验证：保存按键提示后 `theme` 节仍在）。
3. 主题令牌层（`theme.css` + 种子读写 + `system_theme` + 跟随系统 + wash 替换 14 处）。
4. 消息总线 + Rust 路由（先用控制台 sink 验证）。
5. 横幅窗口 + 渲染页 + 队列 + ready 握手 + 光标轮询（悬停淡化 / 点击消失）。
6. 调试页四组（把前三块挂上，落地重置）。
7. 文档同步 + 全量验证 + 截图归档。

> **实施状态（2026-10-29 收尾）**：以上七步全部落地，对应提交 `6567cb9`（字体）、`4984dda`（settings 共享层）、`da6e775`（主题令牌层）、`d6a3efb`（消息总线与横幅）、`195c3dd`（设置坞 02/03）。实施期与文档的偏差只有两处，已同步：`backgroundSecondary` 默认值 `#0a0d12`（§4.1 标定说明）、消息 `durationMs` 缺省/0 表示跟随横幅设置（§6.1/§10）；另因调试页需要持久化横幅参数，补了 `banner_settings` / `update_banner_settings` / `banner_geometry` 三个命令与 `banner:settings` 事件。功能说明见 `docs/features/{theme,messages,banner,debug}.md`。
