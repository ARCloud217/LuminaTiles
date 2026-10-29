# 主题（配色种子与跟随系统）

## 用途

一套种子决定整个界面的颜色。用户在设置坞「02 主要设置」里改四个种子，所有窗口即时换肤；也可以直接跟随 Windows 强调色与明暗。

## 用户行为

- 四个种子：主强调色、副强调色、主背景色、副背景色。改动后 220ms 防抖写入 `settings.json`，`theme:changed` 广播到全部窗口（主窗口、横幅、按键提示）。
- 明暗基调：`auto`（按主背景相对亮度 ≥ 0.18 判浅色）/ `dark` / `light` 三档，决定 `theme.css` 用哪组档位常量。
- 跟随系统：开启后主/副背景取系统明暗对应的内置背景对（深 `#06070a` / `#0a0d12`，浅 `#f4f5f8` / `#eaecf1`），主强调色取系统强调色，副强调色由系统强调色推导（`oklch(from <系统色> l calc(c * 0.9) calc(h + 150))`）；四个种子输入与基调下拉变只读并显示系统值，用户的种子原样保留在 `settings.json`，关闭跟随立刻恢复。
- 跟随期间带种子的写入被后端拒绝（错误码 `theme_locked_by_system`），前端显示 `settings.theme.saveFailed` 提示。
- 系统强调色读取失败（两项都是 null）时回落用户种子，控制台留 `[lumina-tiles] 系统主题读取失败，暂用用户种子`，并通过消息中心发一条 `messages.systemAccentFailed` 警告（主题层不依赖 i18n，文案由横幅页翻译）。不订阅系统主题变更事件，只在启动、开关跟随、调试页「重新读取系统主题」时重读。
- 首帧：`index.html` 的 head 内联脚本给 `<html>` 加 `data-theme-pending`，`theme.css` 据此把 body 藏起来，种子落到 `:root` 后再移除属性，避免默认琥珀闪一下。

## 令牌模型

`src/shared/styles/theme.css` 是唯一颜色来源，`tokens.css` 不再定义颜色。

- 种子：`--seed-accent`、`--seed-accent-alt`、`--seed-bg`、`--seed-surface`（由 `src/features/theme/index.js` 写到 `:root`；文件内的默认值就是出厂主题）。
- 推导：`--color-bg` / `--color-surface` 直接用种子；`--color-bg-deep`、`--color-surface-raised`、`--color-ink`(-muted/-faint)、`--color-line`(-strong) 用 `oklch(from <种子> calc(l ± 档位) calc(c * 系数) h)` 推导；`--color-secondary` 与 soft/glow 同法；叠加层统一 `--color-wash` / `--color-wash-soft`（深色档白光 6%，浅色档暗光 5%）。
- 档位常量：深色档 `--tone-depth 0.025`、`--tone-lift 0.0375`、`--tone-ink-l 0.945`、`--tone-ink-muted-l 0.598`、`--tone-ink-faint-l 0.45`、`--tone-line-lift 0.089`、`--tone-line-strong-lift 0.166`；浅色档在 `[data-theme-tone="light"]` 覆盖。这些值是在真实 WebView2 里按默认种子二分标定的，改之前先跑 `node scripts/snapshot-theme.mjs <CDP端口>` 复核（输出 14 个令牌的 raw 文本与 canvas 回读的 8 位 sRGB）。
- 语义色（danger / success / warning）不参与推导，只按档位换明度。

## 接口

后端 `src-tauri/src/features/theme.rs`：

| 命令 | 说明 |
| --- | --- |
| `theme_settings` | 返回 `ThemeSettings { primaryAccent, secondaryAccent, backgroundPrimary, backgroundSecondary, tone, followSystem }` |
| `update_theme_settings` | 校验十六进制（非法回退默认并记日志）、跟随期间拒绝种子变更、写盘、同步原生标题栏、广播 `theme:changed` |
| `reset_theme_settings` | 恢复默认种子与 `tone=auto`、关闭跟随 |
| `system_theme` | 返回 `{ accent, light }`：强调色取 `DWM\ColorizationColor`（为 0 回退 `AccentColor` 并交换 R/B），明暗取 `AppsUseLightTheme`；单项失败返回 null |

事件：`theme:changed` 由 Rust 广播，前端 `onThemeChanged` 重新应用。

前端：`src/features/theme/index.js`（`applyTheme` / `initThemeFeature`，在 `beforeI18n` 阶段执行，避免文案先按默认主题渲染）、`src/features/theme/settings.js`（02 面板逻辑）。API 边界见 `src/shared/api/index.js` 的 `getThemeSettings` / `saveThemeSettings` / `resetThemeSettings` / `getSystemTheme` / `onThemeChanged`。

存储：`settings.json` 的 `theme` 节，由 `src-tauri/src/settings.rs` 的 `read_section` / `patch` 读写，写盘保留其它顶层键。

## 字体

`src/shared/fonts/` 打包 HarmonyOS Sans SC 三个字重（Regular / Medium / Bold，合计约 23.5MB，附 LICENSE），`src/shared/styles/fonts.css` 用相对路径声明 `@font-face`（`font-display: swap`），`tokens.css` 的 `--font-family-base` 与 `--font-family-display` 首位指向它，`--font-family-mono` 仍是 Cascadia Mono。

## 本地化键

`settings.theme.{title,description,primaryAccent,secondaryAccent,backgroundPrimary,backgroundSecondary,tone,toneAuto,toneDark,toneLight,followSystem,systemThemeActive,systemThemeDark,systemThemeLight,systemThemeFailed,seedsLocked,reset,saveFailed}`、`messages.systemAccentFailed`。

## 验证

- `cargo check` 零警告；`cargo tauri dev` 起来后改种子，`--color-accent` / `--color-ink` 的 computed 值与 canvas 回读随之变化。
- 跟随系统时 `--seed-accent` 等于系统强调色（本机 `#0078d4`），种子输入 disabled 且有锁定提示。
- 浅色档下正文与次要文字对比度 ≥ 4.5（调试页「主题体检」直接读）。
- 首帧无琥珀闪烁：`data-theme-pending` 在 `applyTheme` 完成后移除。
