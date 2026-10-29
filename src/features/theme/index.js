// 主题：把 settings.json 里的四个种子（或系统的强调色与明暗）写进 :root，
// 具体颜色由 shared/styles/theme.css 从种子推导，这里不出现任何色值。

import { getSystemTheme, getThemeSettings, onThemeChanged } from "../../shared/api/index.js";

// 跟随系统期间使用的内置背景对：系统只给明暗，取哪一组背景由这里决定。
const SYSTEM_BACKGROUNDS = {
  dark: { primary: "#06070a", secondary: "#0a0d12" },
  light: { primary: "#f4f5f8", secondary: "#eaecf1" }
};

function channelToLinear(value) {
  const channel = value / 255;
  return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
}

function relativeLuminance(hex) {
  const red = parseInt(hex.slice(1, 3), 16);
  const green = parseInt(hex.slice(3, 5), 16);
  const blue = parseInt(hex.slice(5, 7), 16);
  return 0.2126 * channelToLinear(red) + 0.7152 * channelToLinear(green) + 0.0722 * channelToLinear(blue);
}

/** 明暗档位：跟随时以系统为准，否则 auto 按主背景亮度判定（< 0.18 为深色）。 */
function resolveTone(settings, system) {
  if (settings.followSystem && typeof system?.light === "boolean") {
    return system.light ? "light" : "dark";
  }
  if (settings.tone === "dark" || settings.tone === "light") return settings.tone;
  return relativeLuminance(settings.backgroundPrimary) < 0.18 ? "dark" : "light";
}

/** 实际生效的四个种子：跟随时系统值优先，读不到的字段退回用户种子。 */
function resolveSeeds(settings, system, tone) {
  if (!settings.followSystem) {
    return {
      accent: settings.primaryAccent,
      accentAlt: settings.secondaryAccent,
      background: settings.backgroundPrimary,
      surface: settings.backgroundSecondary
    };
  }
  const backgrounds = SYSTEM_BACKGROUNDS[tone];
  const accent = system?.accent ?? settings.primaryAccent;
  return {
    accent,
    // 副强调色由主强调色转色相推出，跟随时不可自定义。
    accentAlt: system?.accent
      ? `oklch(from ${system.accent} l calc(c * 0.9) calc(h + 150))`
      : settings.secondaryAccent,
    background: backgrounds.primary,
    surface: backgrounds.secondary
  };
}

function paint(seeds, tone) {
  const root = document.documentElement;
  root.style.setProperty("--seed-accent", seeds.accent);
  root.style.setProperty("--seed-accent-alt", seeds.accentAlt);
  root.style.setProperty("--seed-bg", seeds.background);
  root.style.setProperty("--seed-surface", seeds.surface);
  root.dataset.themeTone = tone;
  // 原生控件与滚动条跟随明暗。
  root.style.colorScheme = tone;
  // 种子已落地，撤掉首帧遮罩。
  root.removeAttribute("data-theme-pending");
}

/** 读设置并应用一次，返回实际生效的值供面板显示。 */
export async function applyTheme() {
  const settings = await getThemeSettings();
  const system = settings.followSystem ? await getSystemTheme() : null;
  if (settings.followSystem && system && system.accent === null && system.light === null) {
    console.warn("[lumina-tiles] 系统主题读取失败，暂用用户种子");
  }
  const tone = resolveTone(settings, system);
  const seeds = resolveSeeds(settings, system, tone);
  paint(seeds, tone);
  return { settings, system, seeds, tone };
}

/** 启动时先应用主题（早于 i18n），随后订阅变化。 */
export async function initThemeFeature() {
  try {
    await applyTheme();
  } finally {
    // 失败也要撤掉首帧遮罩，否则界面一直是黑的。
    document.documentElement.removeAttribute("data-theme-pending");
  }

  const unlisten = await onThemeChanged(() => {
    applyTheme().catch((error) => {
      console.error("[lumina-tiles] 主题更新失败", error);
    });
  });
  return () => unlisten();
}
