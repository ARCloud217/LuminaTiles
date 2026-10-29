# 主题、字体、消息中心、顶部横幅与调试页 · 设计文档

- 日期：2026-10-29
- 状态：**待批准**（未获批准前不写任何产品代码）
- 项目：LuminaTiles（Tauri 2 / Windows）
- 关联：\`docs/design-language.md\`（视觉契约）、\`docs/architecture.md\`

## 0. 一句话

把「一套颜色」变成「四个种子推导出的整套颜色」，换掉界面字体，加一个面向用户的消息中心和它的第一个出口——从屏幕顶部降下的横幅，并把这三样东西挂到主窗口新增的「调试」页上，附一条「退出程序并清除数据」。

## 1. 目标

1. **主要设置**：设置坞新增「主要设置」分组，管理主强调色、副强调色、主背景色、副背景色四个种子；可开关「使用系统主题色」（Windows 强调色 → 主强调色，深/浅模式 → 背景档）。其余全部颜色令牌由这四个种子推导，功能配色默认跟随。
2. **字体**：界面字体使用 HarmonyOS Sans SC Bold（按第 5 节的方式打包与回退）。
3. **消息中心**：程序内部的消息总线，开放给前端与 Rust 发布、订阅、撤销，管理一切面向用户的通信；本期不做独立 UI。
4. **顶部横幅（通知样式 1）**：屏幕宽度整条、高度由字体大小决定、从顶部降下的消息横幅；文字渲染与后续特色动画要有独立、可扩展的结构。
5. **调试页**：主窗口设置区新增「调试」选项卡，能试发消息、试主题、试字体、看诊断读数，并提供「退出程序，清除并重置数据」。

非目标（本期不做）：主题预设包/导入导出、横幅堆叠与滑动切换、消息历史 UI、通知中心托盘图标。

## 2. 现有基础（勘察结论，决定方案边界）

- 颜色：\`src/shared/styles/tokens.css\` 45 行，13 个颜色令牌；全项目 31 处 \`var(--color-accent)\` / \`color-mix()\` 全部走令牌，无硬编码色值 → **只改令牌层即可换肤**。
- 前端结构：\`src/main.js\`（202 行）——\`FEATURES\` 注册表（id / enabled / window / init）+ \`buildLattice\` + \`initSettingsNav\` + \`startClock\` + \`bootstrap\`（按 \`document.documentElement.dataset.window\` 过滤窗口、\`pagehide\` 执行 cleanups）。
- API 边界：\`src/shared/api/index.js\`（82 行）是唯一 Tauri 入口，\`core()\` / \`events()\` 两个内部取用函数 + 若干具名导出。
- 窗口：\`src-tauri/tauri.conf.json\` 声明 \`main\`（880×620，min 640×480）与 \`lock-indicator\`（transparent / alwaysOnTop / skipTaskbar / decorations false / focus false / resizable false / shadow false / center false / visible false）。
- 后端：\`src-tauri/src/lib.rs\` 里 \`mod features\`，setup 中调用 \`lock_indicator::init\`、\`lock_indicator::init_window\`、\`tray::init\`；\`invoke_handler\` 集中注册 7 个命令；\`CloseRequested\` 拦截 \`main\` 为 hide。
- 存储：\`%APPDATA%/com.otaz.luminatiles/settings.json\`，顶层 \`{ lockIndicator: {...} }\`，tmp + rename 写入。
- 语言包：顶层键 \`app / brand / settings / lockIndicator / tray\`；\`t()\` 支持 \`{占位}\`。
- 运行环境：WebView2 = Chromium 156，\`oklch(from …)\` 相对色、\`calc()\` 内的 l/c、\`color-mix(in oklab, …)\` 全部 \`CSS.supports() === true\`（已实测）。

## 3. 方案总览

| 块 | 新增/改动 | 关键文件 |
| --- | --- | --- |
| 主题 | 新 \`theme.css\`（种子 → 令牌推导 + 明暗档位）、新 \`features/theme.rs\`、设置面板 | \`src/shared/styles/theme.css\`、\`src-tauri/src/features/theme.rs\` |
| 字体 | 打包字体 + \`@font-face\` + 令牌改字体栈 | \`src/shared/fonts/\`、\`src/shared/styles/fonts.css\` |
| 消息中心 | 前端总线 + Rust 桥 | \`src/shared/messages/index.js\`、\`src-tauri/src/features/messages.rs\` |
| 横幅 | 新窗口 + 渲染页 + 队列 | \`src/features/banner/\`、conf 新增 \`banner\` 窗口、\`features/banner.rs\` |
| 调试页 | 设置坞新 tab + 面板 + 重置命令 | \`src/features/debug/\`、\`src-tauri/src/features/debug.rs\` |

四块共用一条原则：**前端所有 Tauri 调用只经 \`src/shared/api/index.js\`**；**所有可见文本只经语言包**；**所有视觉值只来自令牌**。

## 4. 主题：四个种子 → 整套令牌

### 4.1 数据模型

\`settings.json\` 新增顶层键：

\`\`\`json
{
  "theme": {
    "primaryAccent": "#e9a23b",
    "secondaryAccent": "#3f7bd6",
    "backgroundPrimary": "#06070a",
    "backgroundSecondary": "#10151c",
    "tone": "auto",
    "followSystem": false
  }
}
\`\`\`

- \`tone\`: \`"auto" | "dark" | "light"\`。\`auto\` 表示由 \`backgroundPrimary\` 的相对亮度自动判定（ luminance < 0.18 → dark）。
- \`followSystem\`: true 时启动与切回前台各读一次系统色，覆盖 \`primaryAccent\` 与 \`tone\`（不落盘，只影响运行时；用户四种子原值保留）。
- 校验：四个颜色必须是 6 位十六进制；非法值回退到默认并记录一次警告（不静默吞掉）。
- 默认值即上表（琥珀 + 深色），与现在视觉一致。

### 4.2 令牌推导

\`theme.css\` 在 \`tokens.css\` 之后加载。\`tokens.css\` 只保留非颜色令牌（字体、间距、圆角、阴影、时长）与**颜色令牌的兜底值**（防止 theme.css 加载失败导致无样式）；\`theme.css\` 用种子重定义同名变量，**31 处消费方零改动**。

\`\`\`css
:root {
  /* 种子：由 JS 写入，缺省时用兜底值 */
  --seed-accent: var(--color-accent-fallback, #e9a23b);
  --seed-accent-alt: #3f7bd6;
  --seed-bg: #06070a;
  --seed-surface: #10151c;

  /* 明暗档位：浅色主题只覆盖这一组，表达式不变 */
  --tone-depth: 0.022;   /* bg-deep 比 bg 暗多少 */
  --tone-lift: 0.30;     /* surface-raised 比 surface 亮多少 */
  --tone-ink-l: 0.945;
  --tone-ink-muted-l: 0.635;
  --tone-ink-faint-l: 0.475;
  --tone-line-lift: 0.32;
  --tone-line-strong-lift: 0.55;
}
[data-theme-tone="light"] {
  --tone-depth: 0.020;
  --tone-lift: -0.28;
  --tone-ink-l: 0.235;
  --tone-ink-muted-l: 0.495;
  --tone-ink-faint-l: 0.625;
  --tone-line-lift: -0.14;
  --tone-line-strong-lift: -0.26;
}
\`\`\`

每个颜色令牌的推导（一套表达式，两种 tone 通用）：

| 令牌 | 推导 |
| --- | --- |
| \`--color-accent\` | \`var(--seed-accent)\` |
| \`--color-accent-soft\` | \`color-mix(in oklab, var(--seed-accent) 14%, transparent)\` |
| \`--color-accent-glow\` | \`color-mix(in oklab, var(--seed-accent) 35%, transparent)\` |
| \`--color-secondary\` | \`var(--seed-accent-alt)\` |
| \`--color-secondary-soft\` / \`-glow\` | 同上，14% / 35% |
| \`--color-bg\` | \`var(--seed-bg)\` |
| \`--color-bg-deep\` | \`oklch(from var(--seed-bg) calc(l - var(--tone-depth)) c h)\` |
| \`--color-surface\` | \`var(--seed-surface)\` |
| \`--color-surface-raised\` | \`oklch(from var(--seed-surface) calc(l + var(--tone-lift)) calc(c * 1.05) h)\` |
| \`--color-ink\` | \`oklch(from var(--seed-bg) var(--tone-ink-l) calc(c * 0.6) h)\` |
| \`--color-ink-muted\` | \`oklch(from var(--seed-bg) var(--tone-ink-muted-l) calc(c * 0.6) h)\` |
| \`--color-ink-faint\` | \`oklch(from var(--seed-bg) var(--tone-ink-faint-l) calc(c * 0.6) h)\` |
| \`--color-line\` | \`oklch(from var(--seed-surface) calc(l + var(--tone-line-lift)) calc(c * 1.1) h)\` |
| \`--color-line-strong\` | \`oklch(from var(--seed-surface) calc(l + var(--tone-line-strong-lift)) calc(c * 1.1) h)\` |

语义色不参与推导（红色必须像红色），但按 tone 调整明度：\`--color-danger\`、\`--color-success\`、\`--color-warning\` 三者在浅色 tone 下取更暗的一档。\`--color-secondary\` 定位为「第二信号」：图表第二系列、状态区分、hover 第二层反馈，**不参与主按钮**。

### 4.3 系统主题色

Rust 命令 \`system_theme\` → \`{ accent: "#rrggbb", tone: "dark"|"light" }\`：

- 强调色：\`HKCU\SOFTWARE\Microsoft\Windows\DWM\ColorizationColor\`（DWORD ARGB，取低 24 位）。
- 明暗：\`HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme\`（0/1）。
- 只用 \`windows\` crate 已有的 \`Win32_System_Registry\` 能力（需在 \`Cargo.toml\` 的 features 里补一项），不引入新 crate。
- 读失败时返回 \`{ accent: null, tone: null }\`，前端保持用户种子并打一条 warn 级消息（不伪装成功）。

### 4.4 首帧不闪

主题是异步读的，直接等 \`invoke\` 会先渲染一帧默认琥珀再跳变。做法：\`index.html\` 内联一段同步脚本给 \`<html>\` 加 \`data-theme-pending\`，CSS 里 \`html[data-theme-pending] body { opacity: 0 }\`，背景仍由 \`html { background: 兜底 bg }\` 铺满（深色底，不是白屏）；主题应用到 \`documentElement\` 后移除该属性，\`body\` 用 120ms 淡入。

\`ponytail:\` 不引入 \`WebviewWindowBuilder::from_config\` + \`initialization_script\` 的复杂方案——那需要把所有窗口改成运行时代码创建，收益只是省掉一次 120ms 淡入。

### 4.5 对比度体检

保存主题时（以及调试页每次试色时）前端做一次读数：

1. \`getComputedStyle(document.documentElement)\` 读 \`--color-bg\` / \`--color-ink\` / \`--color-ink-muted\` / \`--color-accent\`；
2. 颜色串解析为 sRGB（浏览器已把 \`oklch()\` 计算成 \`color(...)\` 或 \`oklch()\` 原始值时，用一个 1×1 canvas 填色再 \`getImageData\` 取回 8 位 sRGB，避免自己实现色空间转换）；
3. 算 WCAG 对比度，面板上显示三个读数：正文 \`ink/bg\`、次要文字 \`ink-muted/bg\`、强调色 \`accent/bg\`；
4. 阈值：正文 < 4.5 显示警告文案并**允许保存**（用户自选配色是权利），但面板与调试页都标红；accent < 3 时同样警告。不静默通过。

### 4.6 设置界面

设置坞导航新增一项「主要设置」（序号 02，「调试」为 03）：

- 四个颜色种子：\`<input type="color">\` + 十六进制文本输入（双向下标同步），每行右侧显示「重置为默认」。
- 明暗：三段选择 \`自动 / 深色 / 浅色\`（自动 = 由主背景亮度判定，面板上显示判定结果）。
- 跟随系统主题色：开关。打开时四个种子输入框保持可见但标注「当前由系统覆盖」，并显示读到的系统色。
- 读数区：三个对比度数值 + 状态条。
- 「恢复默认」与「预览」沿用现有 \`.settings__actions\` 按钮样式与 \`button\` 切角。

### 4.7 与 \`lockIndicator.accent\` 的关系

现有 \`lockIndicator.accent\`（用户当前自选 #6563e3）保留字段：当它为 \`null\`/\缺失时提示窗用 \`--color-accent\`；非空时仍用自身值（不破坏现有设置）。设计语言文档补一句：功能级自定义色是例外，默认必须跟随主题。

## 5. 字体

- \`@font-face\` 于 \`src/shared/styles/fonts.css\`：

\`\`\`css
@font-face {
  font-family: "HarmonyOS Sans SC";
  src: url("../fonts/harmonyos-sans-sc-bold.ttf") format("truetype");
  font-weight: 700;
  font-style: normal;
  font-display: swap;
}
\`\`\`

- 令牌：\`--font-family-base\` 与 \`--font-family-display\` 的首位改为 \`"HarmonyOS Sans SC"\`，回退链保留 \`"HarmonyOS Sans"\` → \`Segoe UI Variable Text\` → \`Segoe UI\` → \`Microsoft YaHei UI\` → \`sans-serif\`（用户本机已装同族字体，打包与已装两条路都成立）。
- \`--font-family-mono\` 保持不变（Cascadia Mono）：顶栏时钟、数值、元信息需要等宽对齐，这是功能需求不是风格偏好。
- 字体文件来源：用户级安装的 \`C:\Users\ARClo\AppData\Local\Microsoft\Windows\Fonts\HarmonyOS_Sans_SC_Bold_0.ttf\`（7.78 MB），复制为 \`src/shared/fonts/harmonyos-sans-sc-bold.ttf\`，同目录附 \`LICENSE-HarmonyOS-Sans.txt\`（HarmonyOS Sans 允许免费商用，需随包附带许可说明）。
- 命中验证：\`document.fonts.ready\` 后用 canvas \`measureText\` 与一个不存在的字体族对比宽度差（此前 \`document.fonts.check()\` 返回 true 但宽度只差 9px，不可作为证据）；调试页显示「字体已命中 / 回退到 X」。

## 6. 消息中心

### 6.1 消息模型

\`\`\`js
{
  id: "m-1729…",          // 前端生成
  level: "info" | "success" | "warning" | "error",
  i18nKey: "messages.hookFailed",  // 二选一：本地化键
  text: "…",                        // 或直出文本（Rust 侧无法本地化时）
  params: { value: "Caps Lock" },    // 可选占位参数
  source: "frontend" | "rust",
  durationMs: 3200,        // 0 / null = 由等级默认值决定
  sticky: false            // true = 不自动消失，需显式 dismiss
}
\`\`\`

### 6.2 前端总线（\`src/shared/messages/index.js\`）

| 导出 | 说明 |
| --- | --- |
| \`publish(message)\` | 规范化（补 id、等级默认时长、校验必填）后入队并广播，返回 id |
| \`subscribe(handler)\` | 订阅消息事件，返回取消函数；handler 收到规范化后的消息 |
| \`dismiss(id)\` | 撤销一条（从队列移除并广播 \`dismissed\`） |
| \`getQueue()\` | 只读快照，供调试页展示 |
| \`registerSink(fn)\` | 注册出口（横幅、控制台日志等）；sink 抛错只记日志不影响其它 sink |

- 单例，无状态容器，不碰 DOM；\`main.js\` 在 \`bootstrap\` 里注册 \`console\` sink（开发期）与横幅 sink。
- 去重：同 \`level + i18nKey\` 在 800ms 内重复发布则合并计数（横幅显示 \`×N\`），避免连按 CapsLock 刷屏。

### 6.3 Rust 桥（\`src-tauri/src/features/messages.rs\`）

- 命令 \`message_publish\`：前端发布用（调试页试发、未来任何功能都可调），Rust 侧转发到所有窗口的事件 \`lumina:message\`，保证多窗口一致。
- Rust 内部要发消息时调用同一个 \`publish\` 函数（内部直接 emit），不绕过总线。
- 事件：\`lumina:message\`（新消息）、\`lumina:message-dismissed\`（撤销）。

### 6.4 生命周期

\`publish\` → 队列 → sink 消费；横幅 sink 一次只显示一条，其余排队；\`dismiss\` 或超时后出队。队列上限 20 条，超出丢弃最旧的并在控制台留痕。

## 7. 顶部横幅（通知样式 1）

### 7.1 窗口

\`tauri.conf.json\` 新增第三个窗口：

\`\`\`json
{
  "label": "banner",
  "url": "features/banner/index.html",
  "width": 1280,
  "height": 64,
  "visible": false,
  "decorations": false,
  "transparent": true,
  "alwaysOnTop": true,
  "skipTaskbar": true,
  "focus": false,
  "resizable": false,
  "shadow": false,
  "center": false
}
\`\`\`

### 7.2 几何

- 宽度：目标显示器的**逻辑宽度**，\`x = monitor.position().x\`，\`y = monitor.position().y\`（贴屏幕顶边）。
- 高度：由内容决定——横幅页用 \`ResizeObserver\` 测内容高，调 \`banner_resize(height)\` 回写；宽度始终锁屏宽，避免「宽度也由内容决定」导致的横向抖动。
- 目标显示器：默认跟随光标所在显示器（与提示窗一致），调试页可切换为「主显示器」。
- DPI：Rust 侧用 \`LogicalSize/LogicalPosition\`，与现有提示窗一致。

### 7.3 视觉

- 结构：\`section.banner[data-level]\` > \`span.banner__signal\`（左侧 3px 等级色条）+ \`div.banner__content\` > \`p[data-banner-text]\` + \`span[data-banner-meta]\`（可选计时/计数）。
- 背景 \`--color-surface-raised\` + 底部 1px \`--color-line\`；等级色：info → \`--color-accent\`，success → \`--color-success\`，warning → \`--color-warning\`，error → \`--color-danger\`。
- 字号来自令牌（\`--font-size-body\`），高度 = 字号 × 行高 + 上下 padding（12px）→ 满足「宽度由字体大小决定」的诉求：**文字缩放即整条横幅高度变化**。
- 「文字渲染」独立成 \`renderMessage(message)\` 一个函数，后续特色动画只包这一层，不改窗口与几何逻辑。

### 7.4 时间线

1. \`banner_show\`（Rust）：定位 → 显示窗口 → emit \`banner:message\`；
2. 横幅页收到后渲染，入场：\`transform: translateY(-100%) → 0\`，180ms \`--ease-out\`；
3. 停留：\`durationMs\`（默认 info/success 3200，warning 5000，error 6000；\`sticky\` 则不动）；
4. 退场：\`translateY(-100%)\`，160ms；\`transitionend\` 或定时兜底后 \`invoke("banner_hide")\`；
5. 队列下一条：横幅页先渲染新内容再播入场（同一窗口，不闪）。

\`prefers-reduced-motion\` 下入场/退场改为纯透明度过渡，不位移。

### 7.5 多消息

队列在横幅页内（窗口常驻、\`visible: false\` 时 JS 仍在）；新消息到达时若正在显示则入队。点击横幅 = 立刻 dismiss 当前条。

## 8. 调试页（主窗口设置区 03）

面板内容分四组：

1. **消息中心**：四个等级各一个「发送」按钮、自定义文本输入、「粘性」开关、停留时长滑杆、「清空队列」、当前队列快照（条数 + 最近 3 条 \`level / id\`）。
2. **主题**：四个种子即时试色（不落盘，仅改 \`documentElement\`）、当前 tone 判定结果、三个对比度读数、\`system_theme\` 当前返回值、「重新读取系统色」。
3. **字体**：\`@font-face\` 是否就绪、实际命中的字体族、与回退字体的宽度差、\`--font-family-base\` 当前值。
4. **危险区**：「退出程序，清除并重置数据」——二次确认（原生 \`<dialog>\`？沿用现有按钮 + 文字确认行）→ \`invoke("reset_app_data_and_exit")\`：删除 \`settings.json\` → 记录日志 → \`app.exit(0)\`。横幅/提示窗随之终止。

调试页常驻（用户要求「开发期」可见，后续可用 \`FEATURES\` 开关关闭）；面板顶部加一行说明「仅开发期使用」。

## 9. 新增命令与事件

| 类型 | 名称 | 说明 |
| --- | --- | --- |
| 命令 | \`theme_settings\` | 读主题种子与 tone/followSystem |
| 命令 | \`update_theme_settings\` | 校验并保存（tmp+rename） |
| 命令 | \`reset_theme_settings\` | 恢复默认并返回 |
| 命令 | \`system_theme\` | 读系统强调色与明暗 |
| 命令 | \`message_publish\` | 前端发布消息（Rust 转发事件） |
| 命令 | \`banner_show\` | 定位并显示横幅窗口，参数：等级、文本、时长、显示器 |
| 命令 | \`banner_hide\` | 隐藏横幅窗口 |
| 命令 | \`banner_resize\` | 回写内容高度（逻辑像素，24–240 校验） |
| 命令 | \`debug_reset_app_data\` | 删除设置文件并退出 |
| 事件 | \`lumina:message\` / \`lumina:message-dismissed\` | 消息总线 |
| 事件 | \`theme:changed\` | 种子变化，各窗口即时换肤 |
| 事件 | \`banner:message\` | Rust → 横幅页 |

## 10. 语言包新增键

\`settings.theme.{title,description,primaryAccent,secondaryAccent,backgroundPrimary,backgroundSecondary,tone,toneAuto,toneDark,toneLight,followSystem,systemActive,contrastBody,contrastMuted,contrastAccent,reset,readingFailed}\`、\`settings.debug.{title,description,messages,theme,fonts,danger,send,sendCustom,sticky,duration,clearQueue,queueEmpty,relaunchWipe,confirmWipe,fontHit,fontFallback}\`、\`banner.{dismiss,count}\`、\`messages.{systemThemeFailed,resetDone}\`。中英双语同步，键顺序一致。

## 11. 文档同步

- \`docs/design-language.md\`：颜色令牌表改为「种子 + 推导」，新增等级语义色、横幅视觉规则（整条、贴顶、左侧信号条、无霓虹描边）、字体三分工更新（HarmonyOS Sans SC 承担展示与正文）。
- \`docs/architecture.md\`：窗口表加 \`banner\`，前后端结构加 \`messages\` / \`theme\` / \`debug\`，事件与命令清单更新。
- 新增 \`docs/features/theme.md\`、\`docs/features/messages.md\`、\`docs/features/banner.md\`、\`docs/features/debug.md\`。
- \`AGENTS.md\` 美术风格节补一句：功能配色默认跟随主题，禁止另起色值。

## 12. 验证计划

- \`cargo build\` 无警告；\`cargo tauri dev\` 手动走查。
- CDP 断言（\`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>\` + \`/json/list\`）：
  1. 三个种子换色后，\`--color-ink\` / \`--color-line\` 的 \`getComputedStyle\` 值随之变化且仍满足对比度阈值；
  2. 浅色 tone 下正文对比度 ≥ 4.5；
  3. \`document.fonts\` 就绪后宽度差 ≠ 0（字体真命中）；
  4. 横幅窗口 \`outerWidth == monitor 逻辑宽\`、\`outerY == monitor 顶边\`，内容高度变化时窗口高度随之变化；
  5. 连发 3 条消息：第 2、3 条排队，横幅逐条播完，无重叠；
  6. 调试页试发四个等级，横幅等级色分别为 accent/success/warning/danger；
  7. 重置数据后 \`settings.json\` 消失、进程退出、重启后回到默认主题。
- 主窗 880×620 与 640×480 下无横向滚动；i18n 0 漏翻。

## 13. 风险与需要你拍板

**风险**

1. 字体文件 7.78 MB 会增大安装包；若后续要缩小体积需做子集化（本期不做）。
2. 横幅 \`alwaysOnTop\` 会盖在全屏应用上（与提示窗一致的行为）；如需在游戏全屏时静默，需要额外检测前台全屏状态（本期不做）。
3. \`oklch(from …)\` 依赖 WebView2 ≥ Chromium 119；本项目实测 156 可用，但若用户机器 WebView2 过旧会退化成兜底色（theme.css 里的兜底值保证仍可读）。
4. 主题在 \`main.js\` 之前就要应用，因此读取主题的代码不能依赖 \`initI18n\`（否则错误文案缺翻译）。

**需要你决定的三件事**

1. **字体打包范围**：只打包 Bold（+7.78 MB，正文也用 Bold）/ 同时打包 Regular（再 +约 8 MB，正文 400、标题 700）/ 不打包（依赖本机安装）。我推荐第二项：设计语言要求「重标题 + 常规正文」，只有 Bold 会让正文偏重。
2. **横幅默认显示器**：光标所在屏（我推荐，与提示窗一致）/ 主屏。
3. **等宽字体**：保留 Cascadia Mono 用于时间与数值（我推荐）/ 也换成 HarmonyOS Sans SC。

## 14. 实施顺序

1. 主题令牌层（\`theme.css\` + 种子存储 + \`system_theme\`）→ 现有界面不改结构就能换肤。
2. 字体（打包 + 令牌 + 调试页字体读数）。
3. 消息中心总线 + Rust 桥（无 UI，先用控制台 sink 验证）。
4. 横幅窗口 + 渲染页 + 队列。
5. 调试页四组面板（把前三块挂上去，同时落地重置数据）。
6. 文档同步 + 全量验证 + 截图归档。
