//! 顶部横幅：整条贴顶显示最新消息。窗口点击穿透、不抢焦点，
//! 悬停淡化与点击消失由 30ms 光标轮询补回来（只在窗口可见时运行）。

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock, RwLock};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use crate::settings;

const WINDOW_LABEL: &str = "banner";
const SETTINGS_SECTION: &str = "banner";
const EVENT_MESSAGE: &str = "banner:message";
const EVENT_CLEAR: &str = "banner:clear";
const EVENT_HOVER: &str = "banner:hover";
const EVENT_DISMISS: &str = "banner:dismiss";

/// 可见时的光标轮询间隔：够跟上手速，又不至于明显吃 CPU。
const VISIBLE_POLL_INTERVAL: Duration = Duration::from_millis(30);
/// 隐藏时的空转间隔，避免窗口没显示时还在高频轮询。
const HIDDEN_POLL_INTERVAL: Duration = Duration::from_millis(150);
const DEFAULT_DURATION_MS: u64 = 5000;
const MIN_DURATION_MS: u64 = 1000;
const MAX_DURATION_MS: u64 = 30000;
const MIN_HEIGHT: f64 = 24.0;
const MAX_HEIGHT: f64 = 240.0;
const BASE_HEIGHT: u32 = 64;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BannerSettings {
    /// 每条消息默认停留时长（毫秒）；消息自带 durationMs 时以消息为准
    pub duration_ms: u64,
    /// 目标显示器名称；空字符串表示光标所在屏幕
    pub monitor: String,
}

impl Default for BannerSettings {
    fn default() -> Self {
        Self {
            duration_ms: DEFAULT_DURATION_MS,
            monitor: String::new(),
        }
    }
}

impl BannerSettings {
    fn normalized(mut self) -> Self {
        self.duration_ms = self.duration_ms.clamp(MIN_DURATION_MS, MAX_DURATION_MS);
        self.monitor = self.monitor.trim().to_string();
        self
    }
}

#[derive(Clone, Copy, Serialize)]
struct HoverPayload {
    inside: bool,
}

static SETTINGS: OnceLock<RwLock<BannerSettings>> = OnceLock::new();
static READY: AtomicBool = AtomicBool::new(false);
static PENDING: OnceLock<Mutex<Option<Value>>> = OnceLock::new();
/// 横幅内容高度（逻辑像素），页面量完内容后同步，定位与尺寸都用它。
static HEIGHT: AtomicU32 = AtomicU32::new(BASE_HEIGHT);

fn settings_lock() -> &'static RwLock<BannerSettings> {
    SETTINGS.get_or_init(|| RwLock::new(BannerSettings::default()))
}

fn pending_lock() -> &'static Mutex<Option<Value>> {
    PENDING.get_or_init(|| Mutex::new(None))
}

/// 当前设置；锁被污染时退回默认值，不让横幅彻底失效。
fn current_settings() -> BannerSettings {
    settings_lock()
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

/// 启动：读设置、把窗口调成穿透，并起光标轮询线程。
pub fn init(app: &AppHandle) {
    let loaded = settings::read_section::<BannerSettings>(app, SETTINGS_SECTION)
        .unwrap_or_default()
        .normalized();
    if let Ok(mut guard) = settings_lock().write() {
        *guard = loaded;
    }
    init_window(app);
    start_polling(app.clone());
}

fn init_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW_LABEL) else {
        eprintln!("[lumina-tiles] 未找到横幅窗口");
        return;
    };
    if let Err(error) = window.set_ignore_cursor_events(true) {
        eprintln!("[lumina-tiles] 横幅窗口穿透设置失败：{error}");
    }
    if let Err(error) = window.set_focusable(false) {
        eprintln!("[lumina-tiles] 横幅窗口焦点设置失败：{error}");
    }
}

/// 路由完成后调用：窗口就绪就直接摆上去，否则只留最近一条。
pub fn show(app: &AppHandle, message: &Value) {
    if !READY.load(Ordering::Relaxed) {
        if let Ok(mut pending) = pending_lock().lock() {
            *pending = Some(message.clone());
        }
        return;
    }
    present(app, message);
}

fn present(app: &AppHandle, message: &Value) {
    let Some(window) = app.get_webview_window(WINDOW_LABEL) else {
        eprintln!("[lumina-tiles] 未找到横幅窗口");
        return;
    };
    let settings = current_settings();
    let cursor = cursor_point();
    let Some((left, top, area_width)) = monitor_area(app, &settings, cursor) else {
        eprintln!("[lumina-tiles] 无法确定横幅所在显示器");
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let height = f64::from(HEIGHT.load(Ordering::Relaxed)) * scale;
    let size = PhysicalSize::new(
        area_width.max(1.0).round() as u32,
        height.max(1.0).round() as u32,
    );
    if let Err(error) = window.set_size(size) {
        eprintln!("[lumina-tiles] 横幅尺寸设置失败：{error}");
    }
    if let Err(error) = window.set_position(PhysicalPosition::new(left.round() as i32, top.round() as i32)) {
        eprintln!("[lumina-tiles] 横幅定位失败：{error}");
    }
    if let Err(error) = window.show() {
        eprintln!("[lumina-tiles] 横幅显示失败：{error}");
    }
    if let Err(error) = window.emit(EVENT_MESSAGE, message) {
        eprintln!("[lumina-tiles] 横幅消息发送失败：{error}");
    }
}

fn start_polling(app: AppHandle) {
    thread::spawn(move || {
        let mut inside = false;
        let mut pressed_before = false;
        loop {
            let window = app.get_webview_window(WINDOW_LABEL);
            let visible = window
                .as_ref()
                .map(|window| window.is_visible().unwrap_or(false))
                .unwrap_or(false);
            if !visible {
                // 收窗期间光标会离开，但页面收不到轮询；这里补一条复位，
                // 否则下一条消息会带着上一次的淡化状态出现。
                if inside {
                    inside = false;
                    if let Some(window) = window.as_ref() {
                        if let Err(error) = window.emit(EVENT_HOVER, HoverPayload { inside: false }) {
                            eprintln!("[lumina-tiles] 横幅悬停事件发送失败：{error}");
                        }
                    }
                }
                pressed_before = false;
                thread::sleep(HIDDEN_POLL_INTERVAL);
                continue;
            }
            let Some(window) = window else {
                thread::sleep(HIDDEN_POLL_INTERVAL);
                continue;
            };

            let now_inside = window_rect(&window)
                .zip(cursor_point())
                .map(|((left, top, width, height), point)| {
                    let x = f64::from(point.x);
                    let y = f64::from(point.y);
                    x >= left && x < left + width && y >= top && y < top + height
                })
                .unwrap_or(false);

            if now_inside != inside {
                inside = now_inside;
                if let Err(error) = window.emit(EVENT_HOVER, HoverPayload { inside }) {
                    eprintln!("[lumina-tiles] 横幅悬停事件发送失败：{error}");
                }
            }

            // SAFETY: GetAsyncKeyState 只读按键状态，不要求窗口句柄。
            let pressed = unsafe { GetAsyncKeyState(i32::from(VK_LBUTTON.0)) } as u16 & 0x8000 != 0;
            if inside && pressed && !pressed_before {
                if let Err(error) = window.emit(EVENT_DISMISS, ()) {
                    eprintln!("[lumina-tiles] 横幅点击事件发送失败：{error}");
                }
            }
            pressed_before = pressed;

            thread::sleep(VISIBLE_POLL_INTERVAL);
        }
    });
}

fn window_rect(window: &WebviewWindow) -> Option<(f64, f64, f64, f64)> {
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    Some((
        f64::from(position.x),
        f64::from(position.y),
        f64::from(size.width),
        f64::from(size.height),
    ))
}

fn cursor_point() -> Option<POINT> {
    let mut point = POINT::default();
    // SAFETY: GetCursorPos 只写入传入的 POINT。
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return None;
    }
    Some(point)
}

/// 目标显示器的物理区域：命中设置里的屏幕名，否则光标所在屏，最后退回主屏。
fn monitor_area(app: &AppHandle, settings: &BannerSettings, cursor: Option<POINT>) -> Option<(f64, f64, f64)> {
    let wanted = settings.monitor.trim();
    if !wanted.is_empty() {
        if let Ok(monitors) = app.available_monitors() {
            for monitor in &monitors {
                if monitor.name().map(|name| name.as_str() == wanted).unwrap_or(false) {
                    return Some(area_of(&monitor));
                }
            }
        }
    }
    if let Some(point) = cursor {
        if let Ok(Some(monitor)) = app.monitor_from_point(f64::from(point.x), f64::from(point.y)) {
            return Some(area_of(&monitor));
        }
    }
    app.primary_monitor().ok().flatten().map(|monitor| area_of(&monitor))
}

fn area_of(monitor: &tauri::Monitor) -> (f64, f64, f64) {
    let origin = monitor.position();
    let size = monitor.size();
    (
        f64::from(origin.x),
        f64::from(origin.y),
        f64::from(size.width),
    )
}

/// 读取设置并校验。
#[tauri::command]
pub fn banner_settings(app: AppHandle) -> Result<BannerSettings, String> {
    let settings = settings::read_section::<BannerSettings>(&app, SETTINGS_SECTION)?.normalized();
    if let Ok(mut guard) = settings_lock().write() {
        *guard = settings.clone();
    }
    Ok(settings)
}

/// 保存设置：同时更新内存里的当前值，下一条消息立即生效。
#[tauri::command]
pub fn update_banner_settings(app: AppHandle, settings: BannerSettings) -> Result<BannerSettings, String> {
    let settings = settings.normalized();
    settings::patch(&app, SETTINGS_SECTION, &settings)?;
    if let Ok(mut guard) = settings_lock().write() {
        *guard = settings.clone();
    }
    Ok(settings)
}

/// 横幅页订阅完事件后的握手；有缓存的消息立刻补发。
#[tauri::command]
pub fn banner_ready(app: AppHandle) {
    READY.store(true, Ordering::Relaxed);
    let pending = pending_lock().lock().ok().and_then(|mut guard| guard.take());
    if let Some(message) = pending {
        present(&app, &message);
    }
}

/// 页面确认队列已空：收起窗口。
#[tauri::command]
pub fn banner_done(app: AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        if let Err(error) = window.hide() {
            eprintln!("[lumina-tiles] 横幅隐藏失败：{error}");
        }
    }
}

/// 清空横幅：丢弃缓存并向页面下达清空指令。
#[tauri::command]
pub fn banner_clear(app: AppHandle) {
    if let Ok(mut pending) = pending_lock().lock() {
        *pending = None;
    }
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        if let Err(error) = window.emit(EVENT_CLEAR, ()) {
            eprintln!("[lumina-tiles] 横幅清空事件发送失败：{error}");
        }
    }
}

/// 页面测量内容后同步高度；宽度始终锁屏宽，避免横向抖动。
#[tauri::command]
pub fn banner_resize(app: AppHandle, height: f64) {
    let height = height.clamp(MIN_HEIGHT, MAX_HEIGHT).round();
    HEIGHT.store(height as u32, Ordering::Relaxed);
    let Some(window) = app.get_webview_window(WINDOW_LABEL) else {
        return;
    };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let Ok(current) = window.outer_size() else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let size = PhysicalSize::new(current.width, (height * scale).max(1.0).round() as u32);
    if let Err(error) = window.set_size(size) {
        eprintln!("[lumina-tiles] 横幅高度调整失败：{error}");
    }
}

/// 外部显式显示（调试用）：与消息路由走同一条路径。
#[tauri::command]
pub fn banner_show(app: AppHandle, message: Value) {
    show(&app, &message);
}
