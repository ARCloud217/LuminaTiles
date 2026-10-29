import {
  getLockIndicatorSettings,
  getLockIndicatorStatus,
  onLockIndicatorHide,
  onLockIndicatorSettings,
  onLockIndicatorState,
  resizeLockIndicatorWindow
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

/** 测量卡片自然尺寸 + 阴影留白，同步给后端调整窗口。 */
function syncWindowSize(toast) {
  const rect = toast.getBoundingClientRect();
  const pad = parseFloat(getComputedStyle(document.body).paddingLeft) || 0;
  const width = Math.ceil(rect.width + pad * 2);
  const height = Math.ceil(rect.height + pad * 2);
  const current = { w: window.innerWidth, h: window.innerHeight };
  if (Math.abs(current.w - width) <= 1 && Math.abs(current.h - height) <= 1) return;
  resizeLockIndicatorWindow(width, height).catch((error) => {
    console.error("[lumina-tiles] 提示窗尺寸同步失败", error);
  });
}

async function initToastWindow() {
  const toast = document.querySelector("[data-lock-indicator]");
  const keyNode = toast.querySelector("[data-lock-indicator-key]");
  const stateNode = toast.querySelector("[data-lock-indicator-state]");

  if (!(await getLockIndicatorStatus())) {
    // 钩子只承载 Insert；大写/数字锁定由轮询检测，提示窗继续工作。
    console.warn("[lumina-tiles]", t("lockIndicator.unavailable"));
  }

  let settings = await getLockIndicatorSettings();
  applyAppearance(toast, settings);
  resetMotion(toast);

  // 文案或缩放变化后重新测量；observe 的是卡片本身，窗口缩放不会形成回环。
  const observer = new ResizeObserver(() => syncWindowSize(toast));
  observer.observe(toast);
  syncWindowSize(toast);

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
    observer.disconnect();
    unlistenState();
    unlistenHide();
    unlistenSettings();
  };
}

export async function initLockIndicatorFeature() {
  return initToastWindow();
}
