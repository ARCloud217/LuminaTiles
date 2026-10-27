// 通用动画预设：提示窗等浮层共用同一套 id，后续功能可直接复用。
export const MOTION_PRESETS = ["none", "fade", "slideUp", "slideDown", "slideLeft", "slideRight", "fly"];

const DEFAULT_PRESET = "fade";
const DEFAULT_DURATION = 180;

function resolvePreset(preset) {
  return MOTION_PRESETS.includes(preset) ? preset : DEFAULT_PRESET;
}

/**
 * 播放一次动画预设。
 * @param {HTMLElement | null} element 带 data-motion / data-phase 的元素
 * @param {{ preset?: string, phase?: "in" | "out", durationMs?: number }} options
 */
export function playMotionPreset(element, { preset, phase = "in", durationMs } = {}) {
  if (!element) return;
  const duration = Number.isFinite(Number(durationMs)) ? Math.max(0, Number(durationMs)) : DEFAULT_DURATION;
  element.dataset.motion = resolvePreset(preset);
  element.style.setProperty("--motion-duration", `${duration}ms`);
  // 先清空 phase 并强制重排，让同一个动画可以连续重播。
  element.dataset.phase = "";
  void element.offsetWidth;
  element.dataset.phase = phase === "out" ? "out" : "in";
}

/** 把元素恢复到起始态（不可见），用于下一次入场前等待。 */
export function resetMotion(element) {
  if (element) element.dataset.phase = "";
}
