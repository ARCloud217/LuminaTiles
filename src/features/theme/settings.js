// 主要设置：四个种子 + 明暗档位 + 跟随系统。
// 跟随期间颜色由系统提供，种子输入保持只读但不改值，关闭跟随后原样恢复。

import { getSystemTheme, getThemeSettings, resetThemeSettings, saveThemeSettings } from "../../shared/api/index.js";
import { t } from "../../shared/i18n/index.js";

const SAVE_DELAY = 220;
const COLOR_FIELDS = ["primaryAccent", "secondaryAccent", "backgroundPrimary", "backgroundSecondary"];

function toneText(tone) {
  if (tone === "light") return t("settings.theme.toneLight");
  if (tone === "dark") return t("settings.theme.toneDark");
  return t("settings.theme.toneAuto");
}

export async function initThemeSettingsFeature() {
  const panel = document.querySelector('[data-settings-panel="theme"]');
  if (!panel) return undefined;

  const controls = {};
  for (const node of panel.querySelectorAll("[data-theme-setting]")) {
    controls[node.dataset.themeSetting] = node;
  }
  const outputs = {};
  for (const node of panel.querySelectorAll("[data-theme-value]")) {
    outputs[node.dataset.themeValue] = node;
  }

  const systemNode = panel.querySelector("[data-theme-system]");
  const lockedNode = panel.querySelector("[data-theme-locked]");
  const errorNode = panel.querySelector("[data-theme-error]");

  let settings = null;
  let system = null;
  let saveTimer = 0;
  let pendingSave = false;

  function paintOutputs() {
    for (const name of COLOR_FIELDS) {
      const output = outputs[name];
      if (output) output.textContent = String(controls[name].value).toUpperCase();
    }
  }

  function paintSystem() {
    if (!systemNode) return;
    if (!settings?.followSystem) {
      systemNode.hidden = true;
      return;
    }
    systemNode.hidden = false;
    if (!system || (system.accent === null && system.light === null)) {
      systemNode.textContent = t("settings.theme.systemThemeFailed");
      return;
    }
    const tone = typeof system.light === "boolean" ? (system.light ? "light" : "dark") : "auto";
    systemNode.textContent = t("settings.theme.systemThemeActive", {
      tone: toneText(tone),
      accent: system.accent ?? t("settings.debug.systemUnknown")
    });
  }

  function render(next) {
    settings = next;
    const locked = Boolean(next.followSystem);
    for (const name of COLOR_FIELDS) {
      controls[name].value = next[name];
      controls[name].disabled = locked;
    }
    controls.tone.value = next.tone;
    controls.tone.disabled = locked;
    controls.followSystem.checked = locked;
    if (lockedNode) lockedNode.hidden = !locked;
    paintOutputs();
    paintSystem();
  }

  function readSettings() {
    return {
      primaryAccent: controls.primaryAccent.value,
      secondaryAccent: controls.secondaryAccent.value,
      backgroundPrimary: controls.backgroundPrimary.value,
      backgroundSecondary: controls.backgroundSecondary.value,
      tone: controls.tone.value,
      followSystem: controls.followSystem.checked
    };
  }

  function showError(error) {
    console.error("[lumina-tiles] 主题保存失败", error);
    if (!errorNode) return;
    errorNode.hidden = false;
    errorNode.textContent = t("settings.theme.saveFailed");
  }

  async function readSystem(following) {
    if (!following) return null;
    try {
      return await getSystemTheme();
    } catch (error) {
      console.error("[lumina-tiles] 系统主题读取失败", error);
      return { accent: null, light: null };
    }
  }

  async function persist() {
    pendingSave = false;
    try {
      const saved = await saveThemeSettings(readSettings());
      if (errorNode) errorNode.hidden = true;
      system = await readSystem(saved.followSystem);
      render(saved);
    } catch (error) {
      showError(error);
    }
  }

  function scheduleSave() {
    pendingSave = true;
    clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => {
      void persist();
    }, SAVE_DELAY);
  }

  async function flushSave() {
    if (!pendingSave) return;
    clearTimeout(saveTimer);
    await persist();
  }

  const form = panel.querySelector("[data-theme-form]");

  form?.addEventListener("input", (event) => {
    if (!event.target?.dataset?.themeSetting) return;
    paintOutputs();
    scheduleSave();
  });

  form?.addEventListener("change", (event) => {
    if (!event.target?.dataset?.themeSetting) return;
    paintOutputs();
    // 跟随开关立即反映禁用态，不等保存往返。
    if (event.target.dataset.themeSetting === "followSystem") render(readSettings());
    scheduleSave();
  });

  panel
    .querySelector('[data-theme-action="reset"]')
    ?.addEventListener("click", async () => {
      clearTimeout(saveTimer);
      pendingSave = false;
      try {
        const saved = await resetThemeSettings();
        if (errorNode) errorNode.hidden = true;
        system = await readSystem(saved.followSystem);
        render(saved);
      } catch (error) {
        showError(error);
      }
    });

  try {
    const initial = await getThemeSettings();
    system = await readSystem(initial.followSystem);
    render(initial);
  } catch (error) {
    console.error("[lumina-tiles] 主题设置读取失败", error);
    if (errorNode) {
      errorNode.hidden = false;
      errorNode.textContent = t("settings.theme.saveFailed");
    }
  }

  return () => clearTimeout(saveTimer);
}
