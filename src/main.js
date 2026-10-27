import { initI18n, t } from "./shared/i18n/index.js";
import { initLockIndicatorFeature } from "./features/lock-indicator/index.js";

// 功能注册表：window 指定功能运行在哪个窗口，enabled 用于单独停用与调试。
const FEATURES = [
  { id: "lock-indicator", enabled: true, window: "lock-indicator", init: initLockIndicatorFeature }
];

const cleanups = [];

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
