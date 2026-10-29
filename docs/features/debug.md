# 设置坞 03：调试页

## 用途

开发期自检面板，只在设置坞第三组出现。用来发测试消息、核对主题令牌与对比度、确认打包字体真的被命中、读系统主题与横幅几何，以及在需要时清库重来。

## 四组内容

1. **消息中心**：四个等级按钮（信息 / 成功 / 警告 / 错误）各发一条本地化样例；重要级下拉（高 / 中 / 低，默认中）决定之后所有发送的 `importance`；自定义文本框 + 发送（沿用最后一次点选的等级）；停留时长滑杆（1000–30000ms，改后写 `settings.json` 的 `banner.durationMs`）；显示器下拉（跟随光标 / 指定显示器）；「清空队列」调用 `banner_clear`；累计显示「本次会话已发送 N 条」。
2. **主题体检**：12 个颜色令牌的色块 + canvas 回读的 8 位 RGB；正文 / 次要文字 / 强调色三条 WCAG 对比度（不足 4.5 / 4.5 / 3 时加警示徽标）；`system_theme` 的系统强调色与明暗读数。
3. **横幅体检**：`banner_geometry` 的可见性、矩形（`x, y · w×h`）、当前光标位置、时间线版本（`epoch`）、队列条数、队首消息 id、屏上格子列表（`id · level · importance · 宽度%`）。面板可见时每秒刷新一次，失败提示单独标记来源，不会覆盖其它操作的错误。
4. **字体状态**：`document.fonts` 中 HarmonyOS Sans SC 各字重状态（先 `await document.fonts.load("700 40px …")` 再读，否则 700 恒为 unloaded）、命中家族实测宽度、与不存在家族的宽度差、当前 `--font-family-base`。

**危险区**：「退出程序，清除并重置数据」需点击两次（首次变「确认清除并退出」，6s 内未确认自动还原）→ `reset_app_data_and_exit` 删除 `settings.json`、记日志、`app.exit(0)`。下次启动重建默认设置。

## 接口

| 命令 | 说明 |
| --- | --- |
| `reset_app_data_and_exit` | 清空 `settings.json` 后退出进程 |
| `banner_geometry` | 横幅可见性、矩形、光标、`epoch`、`queueCount`、`queueHead`、`slots` |
| `system_theme` / `theme_settings` / `banner_settings` | 面板只读区与滑杆初值 |

前端：`src/features/debug/index.js`（`initDebugFeature`）、`src/features/debug/styles.css`（`.debug__*` 系列）。canvas 回读的写法：先把 `fillStyle` 设为哨兵值再赋值，`1×1` 填充后 `getImageData`，这样 `oklch()`/相对色也能拿到 8 位 sRGB。

## 本地化键

`settings.debug.{title,description,messages,theme,banner,fonts,danger,send,customPlaceholder,duration,durationValue,sentCount,clearQueue,levelInfo,levelSuccess,levelWarning,levelError,sampleInfo,sampleSuccess,sampleWarning,sampleError,sendFailed,bannerVisible,bannerRect,bannerCursor,bannerMissing,bannerMonitor,monitorFollow,monitorPrimary,monitorOption,geometryFailed,tokens,contrastBody,contrastMuted,contrastAccent,contrastFail,systemAccent,systemTone,systemUnknown,readSystem,reading,fontState,fontsReady,fontsLoading,fontHit,fontFallback,fontStack,fontFailed,relaunchWipe,confirmWipe,wipeWarn,resetFailed,yes,no,importance,importanceHigh,importanceNormal,importanceLow,bannerEpoch,bannerQueueCount,bannerQueueHead,bannerSlots,bannerSlotEmpty}`。

## 验证

- 面板五组齐全、无滚动遮挡；控制台无报错、无缺键。
- 令牌读数与 `getComputedStyle` 一致；对比度数字与手算一致。
- 字体状态「已就绪 · 400 loaded · 500 600 loaded · 700 loaded」，宽度差 ≠ 0。
- 改停留时长后 `settings.json` 落盘，新消息按新时长播放。
- 切重要级后发送：屏上格子按 `高居中 / 低在两侧` 排布，读数里的 `importance` 与宽度占比同步；发满 3 条再发同级消息只增加队列条数。
- 危险区两次点击后 `settings.json` 消失、进程退出、重启回默认主题。