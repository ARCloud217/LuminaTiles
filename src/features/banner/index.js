// 横幅页：只负责把 Rust 下发的快照画出来（渲染、翻译、动画、量矩形回传）。
// 谁上屏、谁排队、每格多宽、停多久，全部由 Rust 的调度器决定。

import { t } from "../../shared/i18n/index.js";
import {
  bannerIdle,
  bannerReady,
  bannerReport,
  onBannerHover,
  onBannerState
} from "../../shared/api/index.js";

/** 幽灵格收窄淡出的时长，和 styles.css 里的过渡保持一致。 */
const GHOST_MS = 200;
/** 整条上滑退场的兜底时长。 */
const EXIT_FALLBACK_MS = 220;

const strip = document.querySelector("[data-banner]");
const slotsNode = document.querySelector("[data-banner-slots]");
const queueNode = document.querySelector("[data-banner-queue]");
const queueCountNode = document.querySelector("[data-banner-queue-count]");

let state = { epoch: 0, slots: [], queueCount: 0 };
let pointerInside = false;
let reportFrame = 0;
let settleTimer = 0;
let idleTimer = 0;

function label(message) {
  return message.i18nKey ? t(message.i18nKey, message.params) : (message.text ?? "");
}

function createCell(message) {
  const cell = document.createElement("article");
  cell.className = "cell";
  cell.dataset.cell = "";
  cell.dataset.id = message.id;

  const signal = document.createElement("span");
  signal.className = "cell__signal";
  signal.setAttribute("aria-hidden", "true");

  const content = document.createElement("div");
  content.className = "cell__content";

  const text = document.createElement("p");
  text.className = "cell__text";
  // 同屏可能有多条，统一用 polite 播报，避免打断正在朗读的内容。
  text.setAttribute("role", "status");
  text.setAttribute("aria-live", "polite");

  const meta = document.createElement("span");
  meta.className = "cell__meta";
  meta.hidden = true;

  content.append(text, meta);
  cell.append(signal, content);
  return cell;
}

/** 写内容、等级与宽度；顺序用 flex order 控制，避免移动节点打断宽度过渡。 */
function paint(cell, message, index) {
  cell.dataset.level = message.level;
  cell.dataset.importance = message.importance;
  cell.style.order = String(index);
  const width = Number(message.width);
  cell.style.flexBasis = Number.isFinite(width) ? `${width}%` : "0%";

  const text = cell.querySelector(".cell__text");
  const next = label(message);
  if (text.textContent !== next) text.textContent = next;

  const meta = cell.querySelector(".cell__meta");
  const merged = Number(message.count) > 1;
  meta.hidden = !merged;
  meta.textContent = merged ? `×${message.count}` : "";
}

/** 离场：钉在原位收窄淡出，不参与命中也不再上报矩形。 */
function ghostify(cell) {
  const box = slotsNode.getBoundingClientRect();
  const rect = cell.getBoundingClientRect();
  const width = rect.width;
  cell.classList.add("cell--ghost");
  cell.style.order = "";
  cell.style.flexBasis = "";
  cell.style.left = `${rect.left - box.left}px`;
  cell.style.top = `${rect.top - box.top}px`;
  cell.style.width = `${width}px`;
  // 先落定起始宽度，再触发过渡，否则浏览器会把两步合成一跳。
  void cell.offsetWidth;
  requestAnimationFrame(() => {
    cell.style.width = "0px";
    cell.style.opacity = "0";
  });
  setTimeout(() => cell.remove(), GHOST_MS);
}

function apply(next) {
  state = next;
  const existing = new Map(
    [...slotsNode.querySelectorAll("[data-cell]")].map((node) => [node.dataset.id, node])
  );
  const ids = new Set(next.slots.map((slot) => slot.id));
  for (const [id, node] of existing) {
    if (!ids.has(id)) ghostify(node);
  }
  next.slots.forEach((message, index) => {
    const cell = existing.get(message.id) ?? createCell(message);
    if (!cell.isConnected) slotsNode.append(cell);
    paint(cell, message, index);
  });

  const count = Number(next.queueCount) || 0;
  queueCountNode.textContent = count > 9 ? "9+" : String(count);
  queueNode.dataset.empty = count > 0 ? "false" : "true";
  // 单看数字读不出含义，给读屏补一句完整描述。
  queueNode.setAttribute("aria-label", t("banner.queueCount", { count }));
  strip.classList.toggle("is-dimmed", pointerInside);
  if (next.slots.length > 0) strip.dataset.state = "shown";
}

/** 量内容高度与每格矩形回传；Rust 只接受当前 epoch 的报告。 */
function report() {
  const height = Math.ceil(strip.getBoundingClientRect().height);
  if (!height) return;
  const slots = [...slotsNode.querySelectorAll("[data-cell]:not(.cell--ghost)")].map((node) => {
    const rect = node.getBoundingClientRect();
    return {
      id: node.dataset.id,
      x: rect.left,
      y: rect.top,
      width: rect.width,
      height: rect.height
    };
  });
  bannerReport(state.epoch, height, slots).catch((error) =>
    console.error("[lumina-tiles] 横幅几何同步失败", error)
  );
}

function scheduleReport() {
  cancelAnimationFrame(reportFrame);
  reportFrame = requestAnimationFrame(report);
  // 宽度过渡结束后再量一次，命中矩形才和最终布局一致。
  clearTimeout(settleTimer);
  settleTimer = setTimeout(report, 240);
}

/** 池与队列都空：等幽灵格收完，整条上滑，再让后端收窗。 */
function maybeIdle() {
  clearTimeout(idleTimer);
  if (state.slots.length > 0 || state.queueCount > 0) return;
  idleTimer = setTimeout(async () => {
    if (state.slots.length > 0 || state.queueCount > 0) return;
    strip.dataset.state = "leaving";
    await new Promise((resolve) => setTimeout(resolve, EXIT_FALLBACK_MS));
    if (state.slots.length > 0 || state.queueCount > 0) return;
    strip.dataset.state = "idle";
    try {
      await bannerIdle();
    } catch (error) {
      console.error("[lumina-tiles] 横幅收窗失败", error);
    }
  }, GHOST_MS);
}

export async function initBannerFeature() {
  const stops = await Promise.all([
    onBannerState((payload) => {
      if (!payload) return;
      apply(payload);
      scheduleReport();
      maybeIdle();
    }),
    onBannerHover((payload) => {
      pointerInside = Boolean(payload?.inside);
      strip.classList.toggle("is-dimmed", pointerInside);
    })
  ]);

  const observer = new ResizeObserver(() => scheduleReport());
  observer.observe(strip);
  strip.addEventListener("transitionend", scheduleReport);

  // 事件就绪后再握手，后端会把当前快照补发过来。
  await bannerReady();

  return () => {
    observer.disconnect();
    strip.removeEventListener("transitionend", scheduleReport);
    cancelAnimationFrame(reportFrame);
    clearTimeout(settleTimer);
    clearTimeout(idleTimer);
    for (const stop of stops) stop();
  };
}
