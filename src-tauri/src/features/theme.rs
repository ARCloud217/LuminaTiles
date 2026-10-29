//! 主题：四个颜色种子 + 明暗档位。负责校验、持久化、广播变化，并读取系统主题。
//!
//! 令牌推导本身在 src/shared/styles/theme.css：这里只给出种子，颜色怎么由种子长出来归样式表。

use std::ffi::c_void;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::{PCWSTR, w};

const SECTION: &str = "theme";
const EVENT_THEME_CHANGED: &str = "theme:changed";

const DEFAULT_PRIMARY_ACCENT: &str = "#e9a23b";
const DEFAULT_SECONDARY_ACCENT: &str = "#3f7bd6";
const DEFAULT_BACKGROUND_PRIMARY: &str = "#06070a";
const DEFAULT_BACKGROUND_SECONDARY: &str = "#0a0d12";

const DWM_KEY: PCWSTR = w!("SOFTWARE\\Microsoft\\Windows\\DWM");
const PERSONALIZE_KEY: PCWSTR = w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");

/// 明暗档位：auto 由主背景相对亮度判定；跟随系统时本字段被忽略。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    #[default]
    Auto,
    Dark,
    Light,
}

/// settings.json 的 theme 节。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeSettings {
    pub primary_accent: String,
    pub secondary_accent: String,
    pub background_primary: String,
    pub background_secondary: String,
    pub tone: Tone,
    pub follow_system: bool,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            primary_accent: DEFAULT_PRIMARY_ACCENT.to_string(),
            secondary_accent: DEFAULT_SECONDARY_ACCENT.to_string(),
            background_primary: DEFAULT_BACKGROUND_PRIMARY.to_string(),
            background_secondary: DEFAULT_BACKGROUND_SECONDARY.to_string(),
            tone: Tone::default(),
            follow_system: false,
        }
    }
}

/// 系统主题读数；读不到的字段为 null，由前端决定退回用户种子。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemTheme {
    accent: Option<String>,
    light: Option<bool>,
}

fn is_hex_color(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(u8::is_ascii_hexdigit)
}

/// 非法色值回退默认并留下警告，不静默吞掉。
fn normalized_hex(field: &str, value: String, fallback: &str) -> String {
    if is_hex_color(&value) {
        value.to_ascii_lowercase()
    } else {
        eprintln!("[lumina-tiles] 主题种子 {field} 非法（{value:?}），已回退默认 {fallback}");
        fallback.to_string()
    }
}

fn normalize(settings: ThemeSettings) -> ThemeSettings {
    ThemeSettings {
        primary_accent: normalized_hex("primaryAccent", settings.primary_accent, DEFAULT_PRIMARY_ACCENT),
        secondary_accent: normalized_hex(
            "secondaryAccent",
            settings.secondary_accent,
            DEFAULT_SECONDARY_ACCENT,
        ),
        background_primary: normalized_hex(
            "backgroundPrimary",
            settings.background_primary,
            DEFAULT_BACKGROUND_PRIMARY,
        ),
        background_secondary: normalized_hex(
            "backgroundSecondary",
            settings.background_secondary,
            DEFAULT_BACKGROUND_SECONDARY,
        ),
        tone: settings.tone,
        follow_system: settings.follow_system,
    }
}

fn seeds_changed(left: &ThemeSettings, right: &ThemeSettings) -> bool {
    left.primary_accent != right.primary_accent
        || left.secondary_accent != right.secondary_accent
        || left.background_primary != right.background_primary
        || left.background_secondary != right.background_secondary
}

fn read_dword(subkey: PCWSTR, value: PCWSTR) -> Option<u32> {
    let mut data: u32 = 0;
    let mut size = u32::try_from(std::mem::size_of::<u32>()).ok()?;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            value,
            RRF_RT_REG_DWORD,
            None,
            Some(&raw mut data as *mut c_void),
            Some(&raw mut size),
        )
    };
    (status.0 == 0).then_some(data)
}

fn hex_from_rgb(red: u32, green: u32, blue: u32) -> String {
    format!("#{red:02x}{green:02x}{blue:02x}")
}

/// 系统强调色：ColorizationColor 为 ARGB，为 0 时回退 AccentColor（ABGR）。
fn system_accent() -> Option<String> {
    if let Some(color) = read_dword(DWM_KEY, w!("ColorizationColor")) {
        if color & 0x00ff_ffff != 0 {
            return Some(hex_from_rgb(
                (color >> 16) & 0xff,
                (color >> 8) & 0xff,
                color & 0xff,
            ));
        }
    }
    let accent = read_dword(DWM_KEY, w!("AccentColor"))?;
    let (red, green, blue) = (accent & 0xff, (accent >> 8) & 0xff, (accent >> 16) & 0xff);
    (accent & 0x00ff_ffff != 0).then(|| hex_from_rgb(red, green, blue))
}

fn system_light() -> Option<bool> {
    read_dword(PERSONALIZE_KEY, w!("AppsUseLightTheme")).map(|value| value != 0)
}

/// 当前生效的明暗，用于原生标题栏；跟随系统时以系统为准。
pub fn effective_light(settings: &ThemeSettings) -> bool {
    if settings.follow_system {
        if let Some(light) = system_light() {
            return light;
        }
    }
    match settings.tone {
        Tone::Dark => false,
        Tone::Light => true,
        Tone::Auto => {
            let (red, green, blue) = parse_hex(&settings.background_primary);
            relative_luminance(red, green, blue) >= 0.18
        }
    }
}

fn parse_hex(value: &str) -> (u32, u32, u32) {
    let channel = |start: usize| u32::from_str_radix(&value[start..start + 2], 16).unwrap_or(0);
    (channel(1), channel(3), channel(5))
}

fn relative_luminance(red: u32, green: u32, blue: u32) -> f64 {
    let linear = |value: u32| {
        let channel = f64::from(value) / 255.0;
        if channel <= 0.040_45 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
}

/// 把当前明暗同步到主窗口原生标题栏；提示窗与横幅无装饰，不涉及。
fn sync_window_theme(app: &AppHandle) {
    let settings = crate::settings::read_section::<ThemeSettings>(app, SECTION)
        .map(normalize)
        .unwrap_or_default();
    let theme = if effective_light(&settings) {
        tauri::Theme::Light
    } else {
        tauri::Theme::Dark
    };
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_theme(Some(theme));
    }
}

/// 启动时同步一次标题栏，并让前端拿到变化事件。
pub fn init(app: &AppHandle) {
    sync_window_theme(app);
}

#[tauri::command]
pub fn theme_settings(app: AppHandle) -> Result<ThemeSettings, String> {
    let stored: ThemeSettings = crate::settings::read_section(&app, SECTION)?;
    Ok(normalize(stored))
}

#[tauri::command]
pub fn update_theme_settings(
    app: AppHandle,
    settings: ThemeSettings,
) -> Result<ThemeSettings, String> {
    let stored: ThemeSettings = crate::settings::read_section(&app, SECTION)?;
    let stored = normalize(stored);
    let next = normalize(settings);
    // 跟随系统期间种子由系统提供：拒绝写入而不是静默忽略。
    if next.follow_system && seeds_changed(&stored, &next) {
        return Err("theme_locked_by_system".to_string());
    }
    crate::settings::patch(&app, SECTION, &next)?;
    sync_window_theme(&app);
    let _ = app.emit(EVENT_THEME_CHANGED, &next);
    Ok(next)
}

#[tauri::command]
pub fn reset_theme_settings(app: AppHandle) -> Result<ThemeSettings, String> {
    let next = ThemeSettings::default();
    crate::settings::patch(&app, SECTION, &next)?;
    sync_window_theme(&app);
    let _ = app.emit(EVENT_THEME_CHANGED, &next);
    Ok(next)
}

#[tauri::command]
pub fn system_theme() -> SystemTheme {
    SystemTheme {
        accent: system_accent(),
        light: system_light(),
    }
}
