const SUPPORTED_LOCALES = ["zh-CN", "en-US"];
const FALLBACK_LOCALE = "en-US";

let messages = {};
let activeLocale = FALLBACK_LOCALE;

function pickLocale(preferred) {
  const language = String(preferred ?? "").toLowerCase();
  const exact = SUPPORTED_LOCALES.find((item) => item.toLowerCase() === language);
  if (exact) return exact;
  const base = language.split("-")[0];
  return SUPPORTED_LOCALES.find((item) => item.split("-")[0].toLowerCase() === base) ?? FALLBACK_LOCALE;
}

async function loadMessages(locale) {
  const response = await fetch(new URL(`../locales/${locale}.json`, import.meta.url));
  if (!response.ok) {
    throw new Error(`语言包加载失败：${locale}（HTTP ${response.status}）`);
  }
  return response.json();
}

/** 读取翻译文案；键不存在时返回键名本身，便于开发期发现遗漏。 */
export function t(key, replacements) {
  const text = String(key)
    .split(".")
    .reduce((node, part) => (node && typeof node === "object" ? node[part] : undefined), messages);
  if (typeof text !== "string") return key;
  return text.replace(/\{(\w+)\}/g, (match, name) => replacements?.[name] ?? match);
}

function translateDocument() {
  const missing = [];
  for (const element of document.querySelectorAll("[data-i18n]")) {
    const key = element.dataset.i18n;
    if (t(key) === key) missing.push(key);
    element.textContent = t(key);
  }
  for (const element of document.querySelectorAll("[data-i18n-aria-label]")) {
    const key = element.dataset.i18nAriaLabel;
    if (t(key) === key) missing.push(key);
    element.setAttribute("aria-label", t(key));
  }
  if (missing.length > 0) {
    console.warn("[lumina-tiles] 缺少翻译键：", missing.join(", "));
  }
}

/** 按系统语言加载语言包并翻译当前文档。 */
export async function initI18n() {
  activeLocale = pickLocale(navigator.language);
  messages = await loadMessages(activeLocale);
  document.documentElement.lang = activeLocale;

  const titleKey = document.documentElement.dataset.titleKey;
  if (titleKey) document.title = t(titleKey);

  translateDocument();
}
