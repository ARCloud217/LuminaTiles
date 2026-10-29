// 横幅：唯一 display sink。显示队列在这里，窗口何时出现由 Rust 掌握，
// 这里只决定每条播多久、下一条什么时候播。

import { t } from "../../shared/i18n/index.js";
import {
  bannerDone,
  bannerReady,
  bannerResize,
  getBannerSettings,
  onBannerClear,
  onBannerDismiss,
  onBannerHover,
  onBannerMessage,
  onMessageDismissed
} from "../../shared/api/index.js";

const MAX_QUEUE = 20;
const EXIT_FALLBACK_MS = 400;
const DEFAULT_DURATION_MS = 5000;

const section = document.querySelector("[data-banner]");
const textNode = document.querySelector("[data-banner-text]");
const metaNode = document.querySelector("[data-banner-meta]");

const queue = [];
let playing = false;
let current = null;
let exitNow = null;
let defaultDuration = DEFAULT_DURATION_MS;
// 光标是否停在横幅矩形内；后端只在变化时广播，所以本地留一份用于新消息重绘。
let pointerInside = false;

function render(message) {
  textNode.textContent = message.i18nKey
    ? t(message.i18nKey, message.params)
    : (message.text ?? "");
  const merged = Number(message.count) > 1;
  metaNode.hidden = !merged;
  metaNode.textContent = merged ? `×${message.count}` : "";
  section.dataset.level = message.level;
  section.classList.toggle("is-dimmed", pointerInside);
}

function waitForExit() {
  return new Promise((resolve) => {
    let settled = false;
    const finish = () => {
      if (settled) return;
      settled = true;
      section.removeEventListener("transitionend", onEnd);
      clearTimeout(timer);
      resolve();
    };
    const onEnd = (event) => {
      if (event.target === section) finish();
    };
    const timer = setTimeout(finish, EXIT_FALLBACK_MS);
    section.addEventListener("transitionend", onEnd);
  });
}

async function play(message) {
  current = message;
  render(message);
  // 先回到 idle 再强制重排，保证每条都有独立的入场过渡。
  section.dataset.state = "idle";
  void section.offsetHeight;
  section.dataset.state = "shown";

  const duration = Number(message.durationMs) > 0 ? Number(message.durationMs) : defaultDuration;
  await new Promise((resolve) => {
    let settled = false;
    const finish = () => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      exitNow = null;
      resolve();
    };
    const timer = setTimeout(finish, duration);
    exitNow = finish;
  });

  current = null;
  section.dataset.state = "leaving";
  await waitForExit();
}

/** 队列空了就让 Rust 收窗；播放中再有新消息会接着播，不闪。 */
async function drain() {
  if (playing) return;
  playing = true;
  try {
    while (queue.length > 0) {
      await play(queue.shift());
    }
    section.dataset.state = "idle";
    await bannerDone();
  } finally {
    playing = false;
    if (queue.length > 0) drain();
  }
}

function enqueue(message) {
  // 去重合并：同一条消息只更新计数，不排队第二条。
  if (current && message.id === current.id) {
    current.count = message.count;
    render(current);
    return;
  }
  const queued = queue.findIndex((item) => item.id === message.id);
  if (queued >= 0) {
    queue[queued] = message;
    return;
  }
  queue.push(message);
  if (queue.length > MAX_QUEUE) {
    // 上限 20，超出丢最旧，避免一次刷屏把队列越堆越长。
    queue.splice(0, queue.length - MAX_QUEUE);
  }
  drain();
}

function clearQueue() {
  queue.length = 0;
  if (exitNow) exitNow();
}

/** 订阅事件、握手 ready、同步高度；返回清理函数。 */
export async function initBannerFeature() {
  try {
    const settings = await getBannerSettings();
    defaultDuration = Number(settings?.durationMs) || DEFAULT_DURATION_MS;
  } catch (error) {
    console.error("[lumina-tiles] 横幅设置读取失败，用默认时长", error);
  }

  const stops = await Promise.all([
    onBannerMessage((message) => enqueue(message)),
    onBannerClear(() => clearQueue()),
    onBannerHover((payload) => {
      pointerInside = Boolean(payload?.inside);
      section.classList.toggle("is-dimmed", pointerInside);
    }),
    onBannerDismiss(() => {
      if (exitNow) exitNow();
    }),
    onMessageDismissed((payload) => {
      if (current && payload?.id === current.id && exitNow) exitNow();
    })
  ]);

  let frame = 0;
  const observer = new ResizeObserver(() => {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
      const height = Math.ceil(section.getBoundingClientRect().height);
      if (height > 0) bannerResize(height).catch((error) => console.error("[lumina-tiles] 横幅高度同步失败", error));
    });
  });
  observer.observe(section);

  // 事件就绪后再握手，后端会把等待中的那条补发过来。
  await bannerReady();

  return () => {
    cancelAnimationFrame(frame);
    observer.disconnect();
    for (const stop of stops) stop();
  };
}
