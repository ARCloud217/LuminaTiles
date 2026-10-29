import { initI18n, t } from "./shared/i18n/index.js";
import { initLockIndicatorFeature } from "./features/lock-indicator/index.js";
import { initLockIndicatorSettingsFeature } from "./features/lock-indicator/settings.js";

// 功能注册表：window 指定功能运行在哪个窗口，enabled 用于单独停用与调试。
const FEATURES = [
  { id: "lock-indicator", enabled: true, window: "lock-indicator", init: initLockIndicatorFeature },
  { id: "lock-indicator-settings", enabled: true, window: "main", init: initLockIndicatorSettingsFeature }
];

const cleanups = [];

// 舞台点阵：静态细格 + 一条循环扫过的光带，用 DOM 生成，避免在 HTML 里堆重复节点。
function buildLattice() {
  const lattice = document.querySelector("[data-lattice]");
  if (!lattice) return;

  const columns = 9;
  const rows = 6;
  const grid = document.createElement("div");
  grid.className = "lattice__grid";

  let index = 0;
  for (let row = 0; row < rows; row += 1) {
    for (let column = 0; column < columns; column += 1) {
      const cell = document.createElement("span");
      cell.className = "lattice__cell";
      if (index === 22) cell.dataset.focus = "";
      grid.append(cell);
      index += 1;
    }
  }

  const beam = document.createElement("span");
  beam.className = "lattice__beam";
  grid.append(beam);

  lattice.replaceChildren(grid);
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

async function bootstrap() {
  buildLattice();
  startClock();
  await initI18n();

  const windowId = document.documentElement.dataset.window;
  for (const feature of FEATURES) {
    if (!feature.enabled || feature.window !== windowId) continue;
    try {
      const cleanup = await feature.init();
      if (typeof cleanup === "function") cleanups.push(cleanup);
    } catch (error) {
      reportFeatureFailure(feature, error);
    }
  }

  window.addEventListener("pagehide", () => {
    for (const cleanup of cleanups.splice(0)) cleanup();
  });
}

bootstrap().catch(showInitFailure);

