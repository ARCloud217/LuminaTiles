import { getLockIndicatorStatus, onLockIndicatorState } from "../../shared/api/index.js";
import { t } from "../../shared/i18n/index.js";

const KEY_LABEL_KEYS = {
  capsLock: "lockIndicator.capsLock",
  numLock: "lockIndicator.numLock",
  insert: "lockIndicator.insert"
};

async function initToastWindow() {
  const toast = document.querySelector("[data-lock-indicator]");
  const keyNode = toast.querySelector("[data-lock-indicator-key]");
  const stateNode = toast.querySelector("[data-lock-indicator-state]");

  const available = await getLockIndicatorStatus();
  if (!available) {
    throw new Error(t("lockIndicator.unavailable"));
  }

  return onLockIndicatorState(({ key, active }) => {
    keyNode.textContent = t(KEY_LABEL_KEYS[key] ?? key);
    stateNode.textContent = t(active ? "lockIndicator.on" : "lockIndicator.off");
    toast.dataset.active = String(active);
  });
}

async function initShellWindow() {
  const errorNode = document.querySelector("[data-lock-indicator-error]");
  if (!errorNode) return undefined;

  try {
    if (await getLockIndicatorStatus()) return undefined;
    errorNode.textContent = t("lockIndicator.unavailable");
    errorNode.hidden = false;
  } catch (error) {
    console.error("[lumina-tiles] 按键提示状态查询失败", error);
    errorNode.textContent = t("lockIndicator.unavailable");
    errorNode.hidden = false;
  }
  return undefined;
}

export async function initLockIndicatorFeature() {
  return document.querySelector("[data-lock-indicator]") ? initToastWindow() : initShellWindow();
}
