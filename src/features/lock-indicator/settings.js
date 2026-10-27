import {
  getLockIndicatorMonitors,
  getLockIndicatorSettings,
  getLockIndicatorStatus,
  previewLockIndicator,
  resetLockIndicatorSettings,
  saveLockIndicatorSettings
} from "../../shared/api/index.js";
import { t } from "../../shared/i18n/index.js";

const SAVE_DELAY = 200;
const COLOR_FALLBACKS = { accent: "#e4b363", text: "#f1efea" };

const VALUE_FORMATTERS = {
  offsetX: (value) => t("settings.lockIndicator.offsetValue", { value }),
  offsetY: (value) => t("settings.lockIndicator.offsetValue", { value }),
  scale: (value) => t("settings.lockIndicator.sizeValue", { value }),
  opacity: (value) => t("settings.lockIndicator.opacityValue", { value }),
  durationMs: (value) => t("settings.lockIndicator.durationValue", { value })
};

function themeColor(token, fallback) {
  const value = getComputedStyle(document.documentElement).getPropertyValue(token).trim();
  return value || fallback;
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function showError(statusNode, error) {
  console.error("[lumina-tiles] 设置保存失败", error);
  if (!statusNode) return;
  statusNode.hidden = false;
  statusNode.textContent = t("settings.lockIndicator.saveFailed");
}

export async function initLockIndicatorSettingsFeature() {
  const form = document.querySelector("[data-settings-form]");
  if (!form) return undefined;

  const statusNode = document.querySelector("[data-settings-status]");
  const errorNode = document.querySelector("[data-lock-indicator-error]");
  const controls = {};
  for (const node of form.querySelectorAll("[data-setting]")) {
    controls[node.dataset.setting] = node;
  }
  const outputs = {};
  for (const node of form.querySelectorAll("[data-setting-value]")) {
    outputs[node.dataset.settingValue] = node;
  }

  const themeDefaults = {
    accent: themeColor("--color-accent", COLOR_FALLBACKS.accent),
    text: themeColor("--color-ink", COLOR_FALLBACKS.text)
  };

  let saveTimer = 0;
  let pendingSave = false;

  function paintOutputs() {
    for (const [name, formatter] of Object.entries(VALUE_FORMATTERS)) {
      const output = outputs[name];
      if (!output) continue;
      output.textContent = formatter(Number(controls[name].value));
    }
  }

  function paintColor(name, value) {
    const input = controls[name];
    const follow = controls[`${name}FollowTheme`];
    if (!input || !follow) return;
    follow.checked = value == null;
    input.value = value ?? themeDefaults[name];
    input.disabled = follow.checked;
  }

  function render(settings) {
    controls.position.value = settings.position;
    controls.monitor.value = settings.monitor ?? "";
    controls.offsetX.value = String(settings.offsetX);
    controls.offsetY.value = String(settings.offsetY);
    controls.scale.value = String(Math.round(settings.scale * 100));
    controls.opacity.value = String(Math.round(settings.opacity * 100));
    controls.animation.value = settings.animation;
    controls.durationMs.value = String(settings.durationMs);
    paintColor("accent", settings.accent);
    paintColor("text", settings.text);
    paintOutputs();
  }

  function readSettings() {
    return {
      position: controls.position.value,
      monitor: controls.monitor.value,
      offsetX: clamp(Number(controls.offsetX.value) || 0, -200, 200),
      offsetY: clamp(Number(controls.offsetY.value) || 0, -200, 200),
      scale: clamp(Number(controls.scale.value) / 100 || 1, 0.8, 1.6),
      accent: controls.accentFollowTheme.checked ? null : controls.accent.value,
      text: controls.textFollowTheme.checked ? null : controls.text.value,
      opacity: clamp(Number(controls.opacity.value) / 100 || 1, 0.3, 1),
      animation: controls.animation.value,
      durationMs: clamp(Number(controls.durationMs.value) || 180, 120, 600)
    };
  }

  async function persist() {
    pendingSave = false;
    try {
      const saved = await saveLockIndicatorSettings(readSettings());
      render(saved);
      if (statusNode) statusNode.hidden = true;
    } catch (error) {
      showError(statusNode, error);
    }
  }

  function scheduleSave() {
    pendingSave = true;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      void persist();
    }, SAVE_DELAY);
  }

  async function flushSave() {
    if (!pendingSave) return;
    clearTimeout(saveTimer);
    await persist();
  }

  async function fillMonitors(selected) {
    const monitors = await getLockIndicatorMonitors();
    controls.monitor.replaceChildren(
      new Option(t("settings.lockIndicator.monitorFollow"), "")
    );
    for (const monitor of monitors) {
      const label = t("settings.lockIndicator.monitorOption", {
        name: monitor.name,
        width: monitor.width,
        height: monitor.height
      });
      const text = monitor.isPrimary
        ? `${label} · ${t("settings.lockIndicator.monitorPrimary")}`
        : label;
      controls.monitor.append(new Option(text, monitor.name));
    }
    const found = monitors.some((monitor) => monitor.name === selected);
    controls.monitor.value = selected && found ? selected : "";
  }

  form.addEventListener("input", (event) => {
    const name = event.target?.dataset?.setting;
    if (!name) return;
    paintOutputs();
    scheduleSave();
  });

  form.addEventListener("change", (event) => {
    const name = event.target?.dataset?.setting;
    if (!name) return;
    if (name === "accentFollowTheme") controls.accent.disabled = event.target.checked;
    if (name === "textFollowTheme") controls.text.disabled = event.target.checked;
    paintOutputs();
    scheduleSave();
  });

  document
    .querySelector('[data-settings-action="preview"]')
    ?.addEventListener("click", async () => {
      await flushSave();
      try {
        await previewLockIndicator();
      } catch (error) {
        showError(statusNode, error);
      }
    });

  document
    .querySelector('[data-settings-action="reset"]')
    ?.addEventListener("click", async () => {
      try {
        render(await resetLockIndicatorSettings());
        if (statusNode) statusNode.hidden = true;
      } catch (error) {
        showError(statusNode, error);
      }
    });

  try {
    if (!(await getLockIndicatorStatus()) && errorNode) {
      errorNode.textContent = t("lockIndicator.unavailable");
      errorNode.hidden = false;
    }
  } catch (error) {
    console.error("[lumina-tiles] 按键提示状态查询失败", error);
  }

  try {
    const initial = await getLockIndicatorSettings();
    render(initial);
    try {
      await fillMonitors(initial.monitor);
    } catch (error) {
      console.error("[lumina-tiles] 显示器列表读取失败", error);
      if (statusNode) {
        statusNode.hidden = false;
        statusNode.textContent = t("settings.lockIndicator.monitorsFailed");
      }
    }
  } catch (error) {
    console.error("[lumina-tiles] 设置读取失败", error);
    if (statusNode) {
      statusNode.hidden = false;
      statusNode.textContent = t("settings.lockIndicator.loadFailed");
    }
  }

  return () => clearTimeout(saveTimer);
}
