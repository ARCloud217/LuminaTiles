import { initI18n, t } from "./shared/i18n/index.js";
import { initThemeFeature } from "./features/theme/index.js";
import { initBannerFeature } from "./features/banner/index.js";
import { initLockIndicatorFeature } from "./features/lock-indicator/index.js";
import { initLockIndicatorSettingsFeature } from "./features/lock-indicator/settings.js";
import { initThemeSettingsFeature } from "./features/theme/settings.js";
import { initDebugFeature } from "./features/debug/index.js";

// 功能注册表：windows 指定功能运行在哪些窗口，enabled 用于单独停用与调试，
// beforeI18n 表示必须在语言包就绪前完成（主题要先落种子，避免首帧闪默认色）。
const FEATURES = [
  { id: "theme", enabled: true, windows: ["main", "banner", "lock-indicator"], beforeI18n: true, init: initThemeFeature },
  { id: "banner", enabled: true, window: "banner", init: initBannerFeature },
  { id: "lock-indicator", enabled: true, window: "lock-indicator", init: initLockIndicatorFeature },
  { id: "lock-indicator-settings", enabled: true, window: "main", init: initLockIndicatorSettingsFeature },
  { id: "theme-settings", enabled: true, window: "main", init: initThemeSettingsFeature },
  { id: "debug", enabled: true, window: "main", init: initDebugFeature }
];

const cleanups = [];

// 舞台点阵：静态细格 + 光标框选，光标靠近的格子按距离透出余光，用 DOM 生成避免堆重复节点。
function buildLattice() {
  const lattice = document.querySelector("[data-lattice]");
  if (!lattice) return;

  const columns = 9;
  const rows = 6;
  const grid = document.createElement("div");
  grid.className = "lattice__grid";

  const cells = [];
  let index = 0;
  for (let row = 0; row < rows; row += 1) {
    for (let column = 0; column < columns; column += 1) {
      const cell = document.createElement("span");
      cell.className = "lattice__cell";
      if (index === 22) cell.dataset.focus = "";
      grid.append(cell);
      cells.push(cell);
      index += 1;
    }
  }

  const reticle = document.createElement("span");
  reticle.className = "lattice__reticle";
  reticle.setAttribute("aria-hidden", "true");
  grid.append(reticle);

  lattice.replaceChildren(grid);
  cleanups.push(attachLatticeCursor(grid, cells, reticle, columns, rows));
}

// 光标框选：四角括线吸附到光标所在格子，邻格按距离透出余光。
function attachLatticeCursor(grid, cells, reticle, columns, rows) {
  const radius = 130;
  const gap = 6;
  let frame = 0;
  let pointer = null;

  const paint = () => {
    frame = 0;
    if (!pointer) return;
    const rect = grid.getBoundingClientRect();
    if (!rect.width) return;
    const cellWidth = (rect.width - gap * (columns - 1)) / columns;
    const cellHeight = (rect.height - gap * (rows - 1)) / rows;

    for (let i = 0; i < cells.length; i += 1) {
      const centerX = (i % columns) * (cellWidth + gap) + cellWidth / 2;
      const centerY = Math.floor(i / columns) * (cellHeight + gap) + cellHeight / 2;
      const distance = Math.hypot(pointer.x - centerX, pointer.y - centerY);
      const falloff = Math.max(0, 1 - distance / radius);
      cells[i].style.setProperty("--glow", (falloff * falloff).toFixed(3));
    }

    const inside = pointer.x >= 0 && pointer.y >= 0 && pointer.x <= rect.width && pointer.y <= rect.height;
    if (!inside) {
      delete reticle.dataset.visible;
      return;
    }

    // 吸附：按格子中心间距取最近的列与行
    const column = Math.min(columns - 1, Math.max(0, Math.round((pointer.x - cellWidth / 2) / (cellWidth + gap))));
    const row = Math.min(rows - 1, Math.max(0, Math.round((pointer.y - cellHeight / 2) / (cellHeight + gap))));
    // 括线内缩 2px 贴在格子内侧，避免与格子自身的边框叠成双线
    const inset = 2;
    reticle.style.width = `${cellWidth - inset * 2}px`;
    reticle.style.height = `${cellHeight - inset * 2}px`;
    reticle.style.transform = `translate3d(${column * (cellWidth + gap) + inset}px, ${
      row * (cellHeight + gap) + inset
    }px, 0)`;
    reticle.dataset.visible = "";
  };

  const schedule = () => {
    if (!frame) frame = window.requestAnimationFrame(paint);
  };

  const clear = () => {
    pointer = null;
    delete reticle.dataset.visible;
    if (frame) {
      window.cancelAnimationFrame(frame);
      frame = 0;
    }
    for (const cell of cells) cell.style.setProperty("--glow", "0");
  };

  const onPointerMove = (event) => {
    const rect = grid.getBoundingClientRect();
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    if (x < -radius || y < -radius || x > rect.width + radius || y > rect.height + radius) {
      if (pointer) clear();
      return;
    }
    pointer = { x, y };
    schedule();
  };

  window.addEventListener("pointermove", onPointerMove, { passive: true });
  document.addEventListener("pointerleave", clear);
  window.addEventListener("blur", clear);

  return () => {
    window.removeEventListener("pointermove", onPointerMove);
    document.removeEventListener("pointerleave", clear);
    window.removeEventListener("blur", clear);
    clear();
  };
}

// 设置分组：切换逻辑与面板标记已就位，新增功能补一个 tab 与一个面板即可。
function initSettingsNav() {
  const tabs = Array.from(document.querySelectorAll("[data-settings-tab]"));
  const panels = Array.from(document.querySelectorAll("[data-settings-panel]"));
  if (tabs.length === 0 || panels.length === 0) return undefined;

  const activate = (id) => {
    for (const tab of tabs) tab.setAttribute("aria-current", String(tab.dataset.settingsTab === id));
    for (const panel of panels) panel.hidden = panel.dataset.settingsPanel !== id;
  };

  const listeners = tabs.map((tab) => {
    const onClick = () => activate(tab.dataset.settingsTab);
    tab.addEventListener("click", onClick);
    return () => tab.removeEventListener("click", onClick);
  });

  activate(tabs[0].dataset.settingsTab);
  return () => {
    for (const remove of listeners) remove();
  };
}

// 顶栏时钟：证明应用仍在实时运行，也承担动态设计的一角。
function startClock() {
  const node = document.querySelector("[data-console-clock]");
  if (!node) return;

  const paint = () => {
    const now = new Date();
    const pad = (value) => String(value).padStart(2, "0");
    node.textContent = pad(now.getHours()) + ":" + pad(now.getMinutes()) + ":" + pad(now.getSeconds());
    node.dateTime = now.toISOString();
  };
  paint();
  const timer = window.setInterval(paint, 1000);
  cleanups.push(() => window.clearInterval(timer));
}

function showInitFailure(error) {
  console.error("[lumina-tiles] 界面初始化失败", error);
  const slot = document.querySelector("[data-init-error]");
  if (slot) {
    slot.textContent = t("app.initFailed");
    slot.hidden = false;
  }
}

function reportFeatureFailure(feature, error) {
  console.error(`[lumina-tiles] 功能 ${feature.id} 初始化失败`, error);
}

async function runFeature(feature) {
  try {
    const cleanup = await feature.init();
    if (typeof cleanup === "function") cleanups.push(cleanup);
  } catch (error) {
    reportFeatureFailure(feature, error);
  }
}

async function bootstrap() {
  buildLattice();
  startClock();
  const settingsNavCleanup = initSettingsNav();
  if (typeof settingsNavCleanup === "function") cleanups.push(settingsNavCleanup);

  const windowId = document.documentElement.dataset.window;
  const active = FEATURES.filter(
    (feature) => feature.enabled && (feature.windows ?? [feature.window]).includes(windowId)
  );

  for (const feature of active.filter((item) => item.beforeI18n)) await runFeature(feature);
  await initI18n();
  for (const feature of active.filter((item) => !item.beforeI18n)) await runFeature(feature);

  window.addEventListener("pagehide", () => {
    for (const cleanup of cleanups.splice(0)) cleanup();
  });
}

bootstrap().catch(showInitFailure);

