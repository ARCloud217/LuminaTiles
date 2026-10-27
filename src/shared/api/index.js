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

/** 订阅按键状态变化；返回取消订阅函数。 */
export async function onLockIndicatorState(handler) {
  return events().listen(LOCK_INDICATOR_EVENT, (event) => handler(event.payload));
}
