import {
  getLockIndicatorSettings,
  getLockIndicatorStatus,
  onLockIndicatorHide,
  onLockIndicatorSettings,
  onLockIndicatorState
} from "../../shared/api/index.js";
import { t } from "../../shared/i18n/index.js";
import { playMotionPreset, resetMotion } from "../../shared/motion/index.js";

const KEY_LABEL_KEYS = {
  capsLock: "lockIndicator.capsLock",
  numLock: "lockIndicator.numLock",
  insert: "lockIndicator.insert"
};

function applyAppearance(toast, settings) {
  toast.style.setProperty("--toast-scale", String(settings.scale));
  document.body.style.opacity = String(settings.opacity);

  if (settings.accent) {
    toast.style.setProperty("--toast-accent", settings.accent);
  } else {
    toast.style.removeProperty("--toast-accent");
  }

  if (settings.text) {
    toast.style.setProperty("--toast-text", settings.text);
    toast.style.setProperty("--toast-text-muted", `color-mix(in srgb, ${settings.text} 72%, transparent)`);
  } else {
    toast.style.removeProperty("--toast-text");
    toast.style.removeProperty("--toast-text-muted");
  }
}

async function initToastWindow() {
  const toast = document.querySelector("[data-lock-indicator]");
  const keyNode = toast.querySelector("[data-lock-indicator-key]");
  const stateNode = toast.querySelector("[data-lock-indicator-state]");

  const available = await getLockIndicatorStatus();
  if (!available) {
    throw new Error(t("lockIndicator.unavailable"));
  }

  let settings = await getLockIndicatorSettings();
  applyAppearance(toast, settings);
  resetMotion(toast);

  const unlistenState = await onLockIndicatorState(({ key, active }) => {
    keyNode.textContent = t(KEY_LABEL_KEYS[key] ?? key);
    stateNode.textContent = t(active ? "lockIndicator.on" : "lockIndicator.off");
    toast.dataset.active = String(active);
    playMotionPreset(toast, {
      preset: settings.animation,
      phase: "in",
      durationMs: settings.durationMs
    });
  });

  // 后端在真正隐藏窗口前发出该事件，前端用同一时长播放退场动画。
  const unlistenHide = await onLockIndicatorHide(({ durationMs } = {}) => {
    playMotionPreset(toast, {
      preset: settings.animation,
      phase: "out",
      durationMs: durationMs ?? settings.durationMs
    });
  });

  const unlistenSettings = await onLockIndicatorSettings((next) => {
    settings = next;
    applyAppearance(toast, settings);
  });

  return () => {
    unlistenState();
    unlistenHide();
    unlistenSettings();
  };
}

export async function initLockIndicatorFeature() {
  return initToastWindow();
}
