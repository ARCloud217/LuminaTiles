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

/** 查询按键提示是否可用（键盘钩子是否安装成功）。 */
export async function getLockIndicatorStatus() {
  return Boolean(await core().invoke("lock_indicator_status"));
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
