//! 系统托盘：常驻图标、左键/右键菜单、打开主窗口与退出。
//!
//! 托盘文案编译期嵌入前端语言包，按系统 UI 语言选择，
//! 与 WebView 的 navigator.language 来自同一系统设置。

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};

const LOCALE_ZH_CN: &str = include_str!("../../../src/shared/locales/zh-CN.json");
const LOCALE_EN_US: &str = include_str!("../../../src/shared/locales/en-US.json");

const TRAY_ID: &str = "main";
const MENU_OPEN_MAIN: &str = "tray-open-main";
const MENU_QUIT: &str = "tray-quit";
const MAIN_WINDOW: &str = "main";

#[derive(serde::Deserialize)]
struct TrayStrings {
    tooltip: String,
    #[serde(rename = "openMain")]
    open_main: String,
    quit: String,
}

#[derive(serde::Deserialize)]
struct LocaleFile {
    tray: TrayStrings,
}

fn load_strings() -> TrayStrings {
    // LANGID 低 10 位是主语言；0x04 为中文，其余语言回落到 en-US，与前端 pickLocale 的行为一致。
    let language_id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() } & 0x03ff;
    let raw = if language_id == 0x0004 { LOCALE_ZH_CN } else { LOCALE_EN_US };
    serde_json::from_str::<LocaleFile>(raw)
        .expect("语言包缺少 tray 文案，检查 src/shared/locales/*.json")
        .tray
}

pub fn init<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let strings = load_strings();

    let open_main = MenuItem::with_id(app, MENU_OPEN_MAIN, strings.open_main, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, strings.quit, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_main, &quit])?;

    let icon = app
        .default_window_icon()
        .ok_or_else(|| tauri::Error::AssetNotFound("icons/icon.ico".to_string()))?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon.clone())
        .tooltip(strings.tooltip)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_OPEN_MAIN => show_main_window(app),
            MENU_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
