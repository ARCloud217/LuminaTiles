# 视觉语言与美术风格

LuminaTiles 的界面是「新极简 + 科幻控制台」：近黑画布、单一信号色、冷灰细线、硬边几何。所有图形由 CSS/JS 绘制，不使用位图与外部图标资源。

本文件是新增或修改界面的硬约束。写界面之前先读一遍，改完按文末检查表自检；与本文冲突的实现需要先改本文，再改代码。

## 色彩

| 令牌 | 值 | 用途 |
| --- | --- | --- |
| `--color-bg` | `#06070a` | 主画布 |
| `--color-bg-deep` | `#030407` | 凹陷分区、提示窗底、按钮文字 |
| `--color-surface` | `#0a0d12` | 卡片 / 面板 |
| `--color-surface-raised` | `#10151c` | 控件、悬停层 |
| `--color-ink` | `#e8edf3` | 主文字 |
| `--color-ink-muted` | `#78838f` | 说明文字 |
| `--color-ink-faint` | `#4b5561` | 元信息、占位 |
| `--color-line` | `#1a2129` | 分隔线、网格 |
| `--color-line-strong` | `#2b343f` | 控件描边、悬停描边 |
| `--color-accent` | `#e9a23b` | 唯一信号色：选中、焦点、数值强调 |
| `--color-accent-soft` | `rgba(233,162,59,.14)` | 选中底纹 |
| `--color-accent-glow` | `rgba(233,162,59,.35)` | 光晕 |
| `--color-danger` | `#ff6b57` | 仅错误状态 |

- 一屏只允许一个强调色。功能色（danger）只表达状态，不做装饰。
- 需要某色的半透明版本时用 `color-mix(in srgb, var(--color-accent) 16%, transparent)` 现算，不新增写死 alpha 的彩色常量。
- 层次靠 1px 线与明度差，不靠投影。投影只允许 `--shadow-panel` / `--shadow-toast`。
- 提示窗等独立窗口的主题色可由用户设置（`--toast-accent`），但默认值必须来自 `--color-accent`。

## 字体

| 角色 | 令牌 | 规则 |
| --- | --- | --- |
| 展示体 | `--font-family-display`（Bahnschrift SemiCondensed） | 品牌名、窗口标题、提示牌按键名 |
| 等宽 | `--font-family-mono`（Cascadia Mono） | 时钟、数值、字段标签、分组序号 |
| 正文 | `--font-family-base`（Segoe UI Variable） | 说明文字、按钮、表单 |

- 元信息统一「等宽 + 大写 + 字距 `.24em`–`.32em` + `--color-ink-muted`」，字号 10–11px（`--font-size-eyebrow`）。
- 数值与时钟必须 `font-variant-numeric: tabular-nums`，避免跳动。
- 不引入网络字体、字体文件或图标字体；只用系统已装字体。

## 布局与尺度

- 主窗口三段式：顶栏（46px）/ 展示区（`minmax(0,1fr)`）/ 设置坞。任何新窗口沿用「顶栏 + 内容」的骨架。
- 间距只用 `--space-1`…`--space-7`；圆角只用 `--radius-sm/md/lg`（2/3/4px），超过 4px 的圆角视为风格违规。
- 设置坞是「左分组导航 156px + 右面板」：新增功能在 `.settings__nav` 里加一个 `button.settings__tab`（含 `settings__tab-index` 序号与 `settings__tab-label` 文案），并在 `.settings` 里加对应的 `.settings__panel[data-settings-panel]`；切换逻辑由 `initSettingsNav()` 统一处理，不要在功能模块里自己写 tab 逻辑。
- 面板内表单统一 12 列网格（`.settings__grid`），字段按内容用 `--span-3/4/6` 占位，间距 16px。
- 窗口最小尺寸按 880×620 校验，不出现滚动条；更窄时用媒体查询转单列（现有断点 820px）。

## 平面构成

- 版面用「左排版 + 右图形」的双列关系：左侧 eyebrow / 巨型展示体 / 说明，右侧一块几何图形区。
- 图形语言三件套：细网格（点阵、栅格）、括线（四角 L 形标记，用于框选/标注）、切角（`clip-path`）。不画圆角胶囊、不画拟物。
- 框选标记贴合被指对象本身，不画霓虹描边、不画扫描线；「当前指向」只允许用括线 + 余光表达。
- 大面积留白是构图的一部分，不要用渐变或纹理填满空区。
- 强调块面允许「实心小方块 + 外发光」，尺寸控制在 20–80px；不要用大面积实心色块。

## 动态设计

| 动效 | 参数 | 用途 |
| --- | --- | --- |
| `rise-in` | 520ms，`--ease-out`，延迟 60/130ms | 分段入场 |
| `status-pulse` | 2.6s 循环 | 顶栏状态点 |
| 光标框选 | 括线即时吸附（不加位移过渡），透明度过渡 140ms | 点阵指示光标所指格子 |
| 光标余光 | 半径 130px，二次衰减写入 `--glow` 0–1，峰值 ≤ .3 | 邻格透出余光 |
| 时钟 | 1s | 顶栏时间 |
| 控件反馈 | 140–160ms（`--duration-fade`） | hover / focus / 展开 |

- 反馈类动效 ≤ 200ms；只有「环境呼吸」类允许 2s 以上的循环，且每屏最多一条。
- 循环动画必须有 `prefers-reduced-motion: reduce` 的停用或固定态；位移与光晕也要在该模式下关掉过渡。
- 动画只表达状态变化或空间关系，不做无意义的旋转、弹跳、抖动。

## 交互

- 主按钮：琥珀底 + 深色文字 + 右上切角（`clip-path`）；次按钮：透明底 + 1px 描边。等价操作并排，主操作在右。
- `:focus-visible` 统一 1px `--color-accent` 内描边（`box-shadow: inset`），不移除焦点环。
- 选中态用 `aria-current`/`aria-pressed` 等语义属性驱动样式，不用纯 class 切换。
- 所有图标按钮必须有本地化 `aria-label`（静态用 `data-i18n-aria-label`，动态用 `t()`）。
- 滑杆、开关等自定义控件必须可用键盘操作，并在文案中给出当前值（不依赖颜色单独传达状态）。

## 纯代码图形的边界

- 允许：CSS 边框/渐变/`clip-path`/伪元素、DOM 生成的网格、内联 SVG 的 `path`（仅几何）。
- 不允许：位图、emoji 当图标、图标库、外链字体、WebGL 大场景、需要下载的主题包。
- 装饰图形必须 `aria-hidden="true"` 且 `pointer-events: none`。

## 禁用清单

- 彩色渐变背景、玻璃拟态模糊卡片、大圆角容器、悬浮投影堆叠。
- 同一屏出现两个强调色；用强调色表达「未选中」。
- 硬编码文案、硬编码颜色值、魔法间距。
- 无降级的无限循环闪烁、跟随系统明暗切换的第二套配色。

## 新功能检查表

1. 颜色/字体/间距/圆角全部来自 `tokens.css`，没有新增孤立视觉值。
2. 所有用户可见文本走 `data-i18n` / `t()`，`zh-CN` 与 `en-US` 键一致。
3. 每个可交互元素有 hover、focus-visible、选中/禁用态，键盘可到达。
4. 动效在 `prefers-reduced-motion` 下有静态替代。
5. 880×620 与 1280×800 两个尺寸下不溢出、不遮挡、无滚动条。
6. 装饰元素 `aria-hidden`，图形无外部资源。
7. 设置类功能已被收纳进设置坞的分组导航，而不是在页面上另起一块。
8. 已在 `docs/features/<feature>.md` 记录行为、接口与本地化键，并在 `docs/architecture.md` 更新结构变化。
