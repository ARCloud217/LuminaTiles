// 前端访问 Tauri 的唯一入口：命令与事件都集中在这里，便于统一处理失败。
const LOCK_INDICATOR_EVENT = "lock-indicator:state";

function core() {
  const api = window.__TAURI__?.core;
  if (!api?.invoke) {
    throw new Error("Tauri 命令接口不可用");
  }
  return api;
}

function events() {
  const api = window.__TAURI__?.event;
  if (!api?.listen) {
    throw new Error("Tauri 事件接口不可用");
  }
  return api;
}

// 启动期 WebView 可能在钩子线程回报结果前就发起查询，false 需带重试再下结论。
const STATUS_RETRY_DELAY_MS = 400;
const STATUS_MAX_ATTEMPTS = 5;

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** 查询按键提示是否可用（键盘钩子是否安装成功）；含启动竞态重试。 */
export async function getLockIndicatorStatus() {
  for (let attempt = 0; attempt < STATUS_MAX_ATTEMPTS; attempt += 1) {
    const ready = await core().invoke("lock_indicator_status");
    if (ready) return true;
    if (attempt < STATUS_MAX_ATTEMPTS - 1) await delay(STATUS_RETRY_DELAY_MS);
  }
  return false;
}

/** 读取按键提示设置。 */
export async function getLockIndicatorSettings() {
  return core().invoke("lock_indicator_settings");
}

/** 保存按键提示设置，返回后端校验后的设置。 */
export async function saveLockIndicatorSettings(settings) {
  return core().invoke("update_lock_indicator_settings", { settings });
}

/** 恢复按键提示默认设置，返回恢复后的设置。 */
export async function resetLockIndicatorSettings() {
  return core().invoke("reset_lock_indicator_settings");
}

/** 立刻弹一次提示，用于在设置面板里预览。 */
export async function previewLockIndicator() {
  return core().invoke("preview_lock_indicator");
}

/** 按内容测量结果同步提示窗尺寸（逻辑像素）。 */
export async function resizeLockIndicatorWindow(width, height) {
  return core().invoke("lock_indicator_resize", { width, height });
}

/** 列出可用于定位的显示器。 */
export async function getLockIndicatorMonitors() {
  return core().invoke("lock_indicator_monitors");
}

/** 订阅按键状态变化；返回取消订阅函数。 */
export async function onLockIndicatorState(handler) {
  return events().listen(LOCK_INDICATOR_EVENT, (event) => handler(event.payload));
}

/** 订阅「准备隐藏」事件，提示窗据此播放退场动画；返回取消订阅函数。 */
export async function onLockIndicatorHide(handler) {
  return events().listen("lock-indicator:hide", (event) => handler(event.payload));
}

/** 订阅设置变化，提示窗据此实时更新外观；返回取消订阅函数。 */
export async function onLockIndicatorSettings(handler) {
  return events().listen("lock-indicator:settings", (event) => handler(event.payload));
}
