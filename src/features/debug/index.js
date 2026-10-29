// 调试页：只给开发期用。四组分别是消息发送、主题/对比度体检、横幅体检与字体命中，
// 外加危险区。所有读数都从真实运行环境取，不写死期望值。

import {
  bannerClear,
  getBannerGeometry,
  getBannerSettings,
  getLockIndicatorMonitors,
  getSystemTheme,
  messagePublish,
  onThemeChanged,
  resetAppDataAndExit,
  saveBannerSettings
} from "../../shared/api/index.js";
import { t } from "../../shared/i18n/index.js";

const TOKENS = [
  "--color-bg",
  "--color-bg-deep",
  "--color-surface",
  "--color-surface-raised",
  "--color-ink",
  "--color-ink-muted",
  "--color-ink-faint",
  "--color-line",
  "--color-line-strong",
  "--color-accent",
  "--color-secondary",
  "--color-wash"
];

const CONTRASTS = [
  { key: "contrastBody", token: "--color-ink", min: 4.5 },
  { key: "contrastMuted", token: "--color-ink-muted", min: 4.5 },
  { key: "contrastAccent", token: "--color-accent", min: 3 }
];

const FONT_FAMILY = "HarmonyOS Sans SC";
const MISSING_FAMILY = "NoSuchFontXYZ123";
const FONT_SAMPLE = "LuminaTiles Wg 大写锁定 已开启 0123456789";
const SAVE_DELAY = 220;
const WIPE_CONFIRM_MS = 6000;

const probe = document.createElement("canvas");
probe.width = 1;
probe.height = 1;
const probeContext = probe.getContext("2d", { willReadFrequently: true });
const measureCanvas = document.createElement("canvas");
const measureContext = measureCanvas.getContext("2d");

/** 令牌文本可能是 oklch 相对色，交给 canvas 落成 8 位 sRGB 再读数。 */
function toRgba(value) {
  probeContext.fillStyle = "#010203";
  probeContext.fillStyle = value;
  probeContext.clearRect(0, 0, 1, 1);
  probeContext.fillRect(0, 0, 1, 1);
  const data = probeContext.getImageData(0, 0, 1, 1).data;
  return { r: data[0], g: data[1], b: data[2], a: data[3] };
}

function relativeLuminance({ r, g, b }) {
  const linear = (channel) => {
    const value = channel / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}

function contrastRatio(first, second) {
  const a = relativeLuminance(first);
  const b = relativeLuminance(second);
  const lighter = Math.max(a, b);
  const darker = Math.min(a, b);
  return (lighter + 0.05) / (darker + 0.05);
}

function readTokens() {
  const style = getComputedStyle(document.documentElement);
  const values = new Map();
  for (const token of TOKENS) {
    values.set(token, style.getPropertyValue(token).trim() || "transparent");
  }
  return values;
}

function rgbaText(color) {
  if (color.a === 255) return `rgb(${color.r} ${color.g} ${color.b})`;
  return `rgba(${color.r} ${color.g} ${color.b} / ${(color.a / 255).toFixed(2)})`;
}

function paintTokens(container, values) {
  const items = [];
  for (const [token, raw] of values) {
    const item = document.createElement("div");
    item.className = "debug__token";

    const swatch = document.createElement("span");
    swatch.className = "debug__swatch";
    swatch.style.background = raw;

    const name = document.createElement("span");
    name.className = "debug__token-name";
    name.textContent = token.replace("--color-", "");

    const value = document.createElement("span");
    value.className = "debug__token-rgb";
    value.textContent = rgbaText(toRgba(raw));

    item.append(swatch, name, value);
    items.push(item);
  }
  container.replaceChildren(...items);
}

function paintContrast(container, values) {
  const background = toRgba(values.get("--color-bg"));
  const rows = [];
  for (const item of CONTRASTS) {
    const ratio = contrastRatio(toRgba(values.get(item.token)), background);
    const row = document.createElement("div");
    row.className = "debug__row";

    const label = document.createElement("span");
    label.className = "debug__label";
    label.textContent = t(`settings.debug.${item.key}`);

    const value = document.createElement("span");
    value.className = "debug__value";
    value.textContent = `${ratio.toFixed(2)}:1`;

    row.append(label, value);
    if (ratio < item.min) {
      const warn = document.createElement("span");
      warn.className = "debug__warn";
      warn.textContent = t("settings.debug.contrastFail");
      row.append(warn);
    }
    rows.push(row);
  }
  container.replaceChildren(...rows);
}

async function paintFonts(nodes) {
  // 字体按需加载：不先请求 700 字重时会读到 unloaded，测宽也会落到回退字体上。
  try {
    await document.fonts.load(`700 40px "${FONT_FAMILY}"`);
  } catch (error) {
    console.error("[lumina-tiles] 打包字体加载失败", error);
  }

  const faces = [];
  document.fonts.forEach((face) => {
    if (face.family.replace(/["']/g, "") === FONT_FAMILY) faces.push(face);
  });

  if (nodes.status) {
    if (faces.length === 0) {
      nodes.status.textContent = t("settings.debug.fontFailed");
    } else {
      const ready = faces.every((face) => face.status === "loaded");
      const detail = faces.map((face) => `${face.weight} ${face.status}`).join(" · ");
      nodes.status.textContent = `${ready ? t("settings.debug.fontsReady") : t("settings.debug.fontsLoading")} · ${detail}`;
    }
  }

  if (measureContext) {
    measureContext.font = `700 40px "${FONT_FAMILY}"`;
    const hit = measureContext.measureText(FONT_SAMPLE).width;
    measureContext.font = `700 40px "${MISSING_FAMILY}"`;
    const missing = measureContext.measureText(FONT_SAMPLE).width;
    if (nodes.hit) nodes.hit.textContent = `${FONT_FAMILY} · ${hit.toFixed(1)}px`;
    if (nodes.gap) {
      // 命中家族与回退宽度相同说明实际没命中打包字体。
      nodes.gap.textContent = `${Math.abs(hit - missing).toFixed(1)}px`;
    }
  }

  if (nodes.stack) {
    nodes.stack.textContent = getComputedStyle(document.body).fontFamily;
  }
}

export async function initDebugFeature() {
  const panel = document.querySelector('[data-settings-panel="debug"]');
  if (!panel) return undefined;

  const tokenContainer = panel.querySelector("[data-debug-tokens]");
  const contrastContainer = panel.querySelector("[data-debug-contrast]");
  const countNode = panel.querySelector("[data-debug-count]");
  const errorNode = panel.querySelector("[data-debug-error]");
  const textInput = panel.querySelector("[data-debug-text]");
  const durationInput = panel.querySelector('[data-debug-banner="durationMs"]');
  const durationOutput = panel.querySelector('[data-debug-value="durationMs"]');
  const monitorSelect = panel.querySelector('[data-debug-banner="monitor"]');
  const systemAccentNode = panel.querySelector("[data-debug-system-accent]");
  const systemToneNode = panel.querySelector("[data-debug-system-tone]");
  const geometryNodes = {};
  for (const node of panel.querySelectorAll("[data-debug-geometry]")) {
    geometryNodes[node.dataset.debugGeometry] = node;
  }
  const fontNodes = {};
  for (const node of panel.querySelectorAll("[data-debug-font]")) {
    fontNodes[node.dataset.debugFont] = node;
  }

  if (textInput) textInput.placeholder = t("settings.debug.customPlaceholder");

  let sentCount = 0;
  let lastLevel = "info";
  let saveTimer = 0;
  let pendingSave = false;
  let confirming = false;
  let confirmTimer = 0;

  function showError(message) {
    if (!errorNode) return;
    errorNode.hidden = false;
    errorNode.textContent = message;
  }

  function clearError() {
    if (errorNode) errorNode.hidden = true;
  }

  function repaintTheme() {
    const values = readTokens();
    if (tokenContainer) paintTokens(tokenContainer, values);
    if (contrastContainer) paintContrast(contrastContainer, values);
  }

  function repaintSystem(system) {
    if (systemAccentNode) systemAccentNode.textContent = system?.accent ?? t("settings.debug.systemUnknown");
    if (systemToneNode) {
      if (typeof system?.light !== "boolean") {
        systemToneNode.textContent = t("settings.debug.systemUnknown");
      } else {
        systemToneNode.textContent = system.light
          ? t("settings.theme.systemThemeLight")
          : t("settings.theme.systemThemeDark");
      }
    }
  }

  async function readSystem() {
    try {
      const system = await getSystemTheme();
      repaintSystem(system);
      return system;
    } catch (error) {
      console.error("[lumina-tiles] 系统主题读取失败", error);
      repaintSystem(null);
      return null;
    }
  }

  async function refreshGeometry() {
    try {
      const geometry = await getBannerGeometry();
      if (geometryNodes.visible) {
        geometryNodes.visible.textContent = geometry.visible ? t("settings.debug.yes") : t("settings.debug.no");
      }
      if (geometryNodes.rect) {
        const rect = geometry.rect;
        geometryNodes.rect.textContent = rect
          ? `${Math.round(rect.x)}, ${Math.round(rect.y)} · ${Math.round(rect.width)}×${Math.round(rect.height)}`
          : t("settings.debug.bannerMissing");
      }
      if (geometryNodes.cursor) {
        const cursor = geometry.cursor;
        geometryNodes.cursor.textContent = cursor
          ? `${Math.round(cursor.x)}, ${Math.round(cursor.y)}`
          : t("settings.debug.bannerMissing");
      }
      clearError();
    } catch (error) {
      console.error("[lumina-tiles] 横幅体检失败", error);
      showError(t("settings.debug.geometryFailed"));
    }
  }

  function readBanner() {
    return {
      durationMs: Number(durationInput?.value) || 5000,
      monitor: monitorSelect?.value ?? ""
    };
  }

  function paintDuration(value) {
    if (durationOutput) durationOutput.textContent = t("settings.debug.durationValue", { value });
  }

  async function persistBanner() {
    pendingSave = false;
    try {
      const saved = await saveBannerSettings(readBanner());
      clearError();
      if (durationInput) durationInput.value = String(saved.durationMs);
      paintDuration(saved.durationMs);
      if (monitorSelect) monitorSelect.value = saved.monitor ?? "";
    } catch (error) {
      console.error("[lumina-tiles] 横幅设置保存失败", error);
      showError(t("settings.debug.sendFailed"));
    }
  }

  function scheduleBannerSave() {
    pendingSave = true;
    clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => {
      void persistBanner();
    }, SAVE_DELAY);
  }

  async function fillMonitors(selected) {
    if (!monitorSelect) return;
    const monitors = await getLockIndicatorMonitors();
    monitorSelect.replaceChildren(new Option(t("settings.debug.monitorFollow"), ""));
    for (const monitor of monitors) {
      const label = t("settings.debug.monitorOption", {
        name: monitor.name,
        width: monitor.width,
        height: monitor.height
      });
      const text = monitor.isPrimary ? `${label} · ${t("settings.debug.monitorPrimary")}` : label;
      monitorSelect.append(new Option(text, monitor.name));
    }
    monitorSelect.value = monitors.some((monitor) => monitor.name === selected) ? selected : "";
  }

  async function publish(message) {
    try {
      await messagePublish(message);
      sentCount += 1;
      clearError();
      if (countNode) countNode.textContent = t("settings.debug.sentCount", { count: sentCount });
    } catch (error) {
      console.error("[lumina-tiles] 调试消息发送失败", error);
      showError(t("settings.debug.sendFailed"));
    }
  }

  for (const button of panel.querySelectorAll("[data-debug-level]")) {
    button.addEventListener("click", () => {
      const level = button.dataset.debugLevel;
      lastLevel = level;
      const suffix = level.charAt(0).toUpperCase() + level.slice(1);
      void publish({ level, i18nKey: `settings.debug.sample${suffix}`, origin: "frontend" });
    });
  }

  panel.querySelector('[data-debug-action="sendCustom"]')?.addEventListener("click", () => {
    const text = textInput?.value.trim() ?? "";
    if (!text) {
      textInput?.focus();
      return;
    }
    void publish({ level: lastLevel, text, origin: "frontend" });
    if (textInput) textInput.value = "";
  });

  panel.querySelector('[data-debug-action="clearQueue"]')?.addEventListener("click", async () => {
    try {
      await bannerClear();
      clearError();
    } catch (error) {
      console.error("[lumina-tiles] 清空横幅队列失败", error);
      showError(t("settings.debug.sendFailed"));
    }
  });

  panel.querySelector('[data-debug-action="refresh"]')?.addEventListener("click", async () => {
    repaintTheme();
    await paintFonts(fontNodes);
    await readSystem();
    await refreshGeometry();
  });

  durationInput?.addEventListener("input", () => {
    paintDuration(Number(durationInput.value) || 5000);
    scheduleBannerSave();
  });

  monitorSelect?.addEventListener("change", () => {
    scheduleBannerSave();
  });

  const wipeButton = panel.querySelector('[data-debug-action="wipe"]');

  function resetWipeLabel() {
    confirming = false;
    window.clearTimeout(confirmTimer);
    if (!wipeButton) return;
    delete wipeButton.dataset.confirming;
    wipeButton.textContent = t("settings.debug.relaunchWipe");
  }

  wipeButton?.addEventListener("click", async () => {
    if (!confirming) {
      confirming = true;
      wipeButton.dataset.confirming = "";
      wipeButton.textContent = t("settings.debug.confirmWipe");
      confirmTimer = window.setTimeout(resetWipeLabel, WIPE_CONFIRM_MS);
      return;
    }
    resetWipeLabel();
    try {
      await resetAppDataAndExit();
    } catch (error) {
      console.error("[lumina-tiles] 清除本地数据失败", error);
      showError(t("settings.debug.resetFailed"));
    }
  });

  repaintTheme();
  await paintFonts(fontNodes);
  await readSystem();
  await refreshGeometry();

  try {
    const banner = await getBannerSettings();
    if (durationInput) durationInput.value = String(banner.durationMs);
    paintDuration(banner.durationMs);
    await fillMonitors(banner.monitor ?? "");
  } catch (error) {
    console.error("[lumina-tiles] 横幅设置读取失败", error);
    showError(t("settings.debug.sendFailed"));
  }

  let unlisten = null;
  try {
    unlisten = await onThemeChanged(() => {
      repaintTheme();
    });
  } catch (error) {
    console.error("[lumina-tiles] 主题变更订阅失败", error);
  }

  return () => {
    clearTimeout(saveTimer);
    window.clearTimeout(confirmTimer);
    if (typeof unlisten === "function") unlisten();
  };
}
