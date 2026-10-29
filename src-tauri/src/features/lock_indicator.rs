//! 按键提示：轮询系统开关状态检测 CapsLock / NumLock 的切换，用低级键盘钩子处理 Insert，
//! 按用户设置的位置、大小、色彩与动画弹出提示窗，并在短暂停留后自动隐藏。

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender, SyncSender, TrySendError};
use std::sync::{OnceLock, RwLock};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewWindow};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetCursorPos, GetMessageW, SetWindowsHookExW,
    TranslateMessage, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYUP, WM_SYSKEYUP,
};

/// 虚拟键码：caps lock
const VK_CAPITAL: i32 = 0x14;
/// 虚拟键码：num lock
const VK_NUMLOCK: i32 = 0x90;
/// 虚拟键码：insert
const VK_INSERT: i32 = 0x2D;

const WINDOW_LABEL: &str = "lock-indicator";
const EVENT_STATE: &str = "lock-indicator:state";
const EVENT_HIDE: &str = "lock-indicator:hide";
const EVENT_SETTINGS: &str = "lock-indicator:settings";
/// 提示窗停留时长（不含入场/退场动画）
const VISIBLE_DURATION: Duration = Duration::from_millis(1200);
/// 系统开关状态的轮询间隔；不依赖键盘事件，任何焦点下都能感知切换
const STATE_POLL_INTERVAL: Duration = Duration::from_millis(100);
/// 跟随光标时，提示窗底边与鼠标指针的距离（逻辑像素）
const CURSOR_OFFSET: f64 = 28.0;
/// 九宫格锚点与屏幕边缘的距离（逻辑像素）
const ANCHOR_MARGIN: f64 = 16.0;
/// 提示窗初始尺寸（逻辑像素）；页面加载后由前端测量卡片内容并经 lock_indicator_resize 更新
const BASE_WIDTH: u32 = 236;
const BASE_HEIGHT: u32 = 72;

const SETTINGS_SECTION: &str = "lockIndicator";
const MIN_SCALE: f32 = 0.8;
const MAX_SCALE: f32 = 1.6;
const MIN_OPACITY: f32 = 0.3;
const MIN_DURATION_MS: u32 = 120;
const MAX_DURATION_MS: u32 = 600;
const MAX_OFFSET: i32 = 200;

static APP: OnceLock<AppHandle> = OnceLock::new();
static AVAILABLE: AtomicBool = AtomicBool::new(false);
static CAPS_LOCK_ON: AtomicBool = AtomicBool::new(false);
static NUM_LOCK_ON: AtomicBool = AtomicBool::new(false);
static INSERT_ON: AtomicBool = AtomicBool::new(false);
/// 每次提示自增，用来让旧的隐藏任务失效
static REVISION: AtomicU64 = AtomicU64::new(0);
/// 钩子回调到提示线程的队列；容量有限，避免异常情况下堆积。
static QUEUE: OnceLock<SyncSender<(&'static str, bool)>> = OnceLock::new();
/// 提示窗当前逻辑尺寸，前端测量卡片内容后同步，定位夹取也用它。
static TOAST_WIDTH: AtomicU32 = AtomicU32::new(BASE_WIDTH);
static TOAST_HEIGHT: AtomicU32 = AtomicU32::new(BASE_HEIGHT);
/// 当前生效的设置；主窗口与提示窗共用同一份。
static SETTINGS: OnceLock<RwLock<LockIndicatorSettings>> = OnceLock::new();

/// 提示窗显示位置：跟随光标，或屏幕九宫格锚点。
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PositionPreset {
    #[default]
    Cursor,
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    Center,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// 动画预设；id 与前端 src/shared/motion 保持一致，后续功能可复用。
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnimationPreset {
    None,
    #[default]
    Fade,
    SlideUp,
    SlideDown,
    SlideLeft,
    SlideRight,
    Fly,
}

/// 按键提示的外观设置，整体序列化成 settings.json 的 lockIndicator 字段。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LockIndicatorSettings {
    /// 显示位置：跟随光标或九宫格锚点
    pub position: PositionPreset,
    /// 指定显示器名称；空字符串表示跟随光标所在屏幕
    pub monitor: String,
    /// 相对基础位置的偏移（逻辑像素）
    pub offset_x: i32,
    pub offset_y: i32,
    /// 大小缩放比例（0.8 ~ 1.6）
    pub scale: f32,
    /// 强调色；None 表示跟随主题
    pub accent: Option<String>,
    /// 按键文字颜色；None 表示跟随主题
    pub text: Option<String>,
    /// 整张卡片不透明度（0.3 ~ 1.0）
    pub opacity: f32,
    /// 入场/退场动画预设
    pub animation: AnimationPreset,
    /// 动画时长（毫秒）
    pub duration_ms: u32,
}

impl Default for LockIndicatorSettings {
    fn default() -> Self {
        Self {
            position: PositionPreset::default(),
            monitor: String::new(),
            offset_x: 0,
            offset_y: 0,
            scale: 1.0,
            accent: None,
            text: None,
            opacity: 1.0,
            animation: AnimationPreset::default(),
            duration_ms: 180,
        }
    }
}

#[derive(Clone, Serialize)]
struct LockStatePayload {
    key: &'static str,
    active: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HidePayload {
    duration_ms: u32,
}

/// 显示器选项，供设置面板选择提示窗显示在哪块屏幕上。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorOption {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

/// 启动钩子线程，并等到钩子安装结果后返回。
pub fn init(app: AppHandle) {
    match load_settings(&app) {
        Ok(settings) => {
            if let Ok(mut guard) = settings_lock().write() {
                *guard = settings;
            }
        }
        Err(error) => eprintln!("[lumina-tiles] {error}"),
    }
    let _ = APP.set(app);

    let (queue, tasks) = mpsc::sync_channel::<(&'static str, bool)>(16);
    let _ = QUEUE.set(queue);
    thread::spawn(move || run_publish_loop(tasks));

    start_state_watcher();

    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || run_hook_loop(sender));

    // 必须等钩子线程回报结果，不能用短超时竞速：
    // 机器繁忙时线程尚未被调度，2 秒超时会先返回，出现"钩子实际安装成功却报告不可用"。
    // 兜底上限只防钩子线程被卡死导致启动挂起，正常运行不会触发。
    let installed = match receiver.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(())) => true,
        Ok(Err(error)) => {
            eprintln!("[lumina-tiles] 键盘钩子安装失败：{error}");
            false
        }
        Err(error) => {
            eprintln!("[lumina-tiles] 键盘钩子线程未回报结果：{error}");
            false
        }
    };
    AVAILABLE.store(installed, Ordering::Relaxed);
}

/// 前端按内容测量卡片后同步提示窗尺寸（逻辑像素），同时供定位夹取使用。
#[tauri::command]
pub fn lock_indicator_resize(app: AppHandle, width: u32, height: u32) -> Result<(), String> {
    if !(40..=1200).contains(&width) || !(24..=400).contains(&height) {
        return Err(format!("提示窗尺寸超出合理范围：{width}x{height}"));
    }
    TOAST_WIDTH.store(width, Ordering::Relaxed);
    TOAST_HEIGHT.store(height, Ordering::Relaxed);
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window
            .set_size(LogicalSize::new(f64::from(width), f64::from(height)))
            .map_err(|error| format!("提示窗尺寸设置失败：{error}"))?;
        // 正在显示时尺寸变了（如开/关文案宽度不同），重新按锚点夹取位置。
        if window.is_visible().unwrap_or(false) {
            position_window(&app, &window, &current_settings());
        }
    }
    Ok(())
}

/// 供前端查询按键提示是否可用。
#[tauri::command]
pub fn lock_indicator_status() -> bool {
    AVAILABLE.load(Ordering::Relaxed)
}

/// 读取当前设置；设置面板与提示窗都用它初始化。
#[tauri::command]
pub fn lock_indicator_settings() -> LockIndicatorSettings {
    current_settings()
}

/// 保存设置：校验范围、写入配置文件，并同步到提示窗。
#[tauri::command]
pub fn update_lock_indicator_settings(
    app: AppHandle,
    settings: LockIndicatorSettings,
) -> Result<LockIndicatorSettings, String> {
    apply_and_store(&app, settings)
}

/// 恢复默认设置。
#[tauri::command]
pub fn reset_lock_indicator_settings(app: AppHandle) -> Result<LockIndicatorSettings, String> {
    apply_and_store(&app, LockIndicatorSettings::default())
}

/// 立刻弹一次提示，方便在设置面板里调参。
#[tauri::command]
pub fn preview_lock_indicator() {
    publish("capsLock", true);
}

/// 列出可用于定位的显示器。
#[tauri::command]
pub fn lock_indicator_monitors(app: AppHandle) -> Vec<MonitorOption> {
    let primary = app
        .primary_monitor()
        .ok()
        .flatten()
        .and_then(|monitor| monitor.name().cloned());
    let mut options = Vec::new();
    if let Ok(monitors) = app.available_monitors() {
        for monitor in monitors {
            let Some(name) = monitor.name().cloned() else {
                continue;
            };
            let size = monitor.size();
            options.push(MonitorOption {
                is_primary: primary.as_deref() == Some(name.as_str()),
                name,
                width: size.width,
                height: size.height,
            });
        }
    }
    options
}

fn apply_and_store(
    app: &AppHandle,
    settings: LockIndicatorSettings,
) -> Result<LockIndicatorSettings, String> {
    let normalized = normalize(settings);
    save_settings(app, &normalized)?;
    if let Ok(mut guard) = settings_lock().write() {
        *guard = normalized.clone();
    }
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        if let Err(error) = window.emit(EVENT_SETTINGS, normalized.clone()) {
            eprintln!("[lumina-tiles] 设置事件发送失败：{error}");
        }
    }
    Ok(normalized)
}

fn settings_lock() -> &'static RwLock<LockIndicatorSettings> {
    SETTINGS.get_or_init(|| RwLock::new(LockIndicatorSettings::default()))
}

fn current_settings() -> LockIndicatorSettings {
    settings_lock()
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

fn normalize(settings: LockIndicatorSettings) -> LockIndicatorSettings {
    LockIndicatorSettings {
        position: settings.position,
        monitor: settings.monitor.trim().to_string(),
        offset_x: settings.offset_x.clamp(-MAX_OFFSET, MAX_OFFSET),
        offset_y: settings.offset_y.clamp(-MAX_OFFSET, MAX_OFFSET),
        scale: settings.scale.clamp(MIN_SCALE, MAX_SCALE),
        accent: settings.accent.filter(|color| is_hex_color(color)),
        text: settings.text.filter(|color| is_hex_color(color)),
        opacity: settings.opacity.clamp(MIN_OPACITY, 1.0),
        animation: settings.animation,
        duration_ms: settings.duration_ms.clamp(MIN_DURATION_MS, MAX_DURATION_MS),
    }
}

fn is_hex_color(color: &str) -> bool {
    let bytes = color.as_bytes();
    bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(u8::is_ascii_hexdigit)
}

fn load_settings(app: &AppHandle) -> Result<LockIndicatorSettings, String> {
    let stored: LockIndicatorSettings =
        crate::settings::read_section(app, SETTINGS_SECTION)?;
    Ok(normalize(stored))
}

fn save_settings(app: &AppHandle, settings: &LockIndicatorSettings) -> Result<(), String> {
    // 只覆盖自己那节，保留 theme 等其它功能的配置。
    crate::settings::patch(app, SETTINGS_SECTION, settings)
}

fn read_toggle_bit(vk: i32) -> bool {
    // SAFETY: GetKeyState 只读系统键盘状态，无副作用；轮询线程不处理输入消息也能读到最新值。
    (unsafe { GetKeyState(vk) } & 0x0001) != 0
}

/// 状态轮询线程：每 100ms 读取一次真实开关位，变化时提示最新状态。
/// 这样不依赖键盘事件是否送达当前焦点，桌面上、别的程序里切换同样会提示。
fn start_state_watcher() {
    CAPS_LOCK_ON.store(read_toggle_bit(VK_CAPITAL), Ordering::Relaxed);
    NUM_LOCK_ON.store(read_toggle_bit(VK_NUMLOCK), Ordering::Relaxed);
    thread::spawn(|| loop {
        thread::sleep(STATE_POLL_INTERVAL);
        watch_toggle(&CAPS_LOCK_ON, VK_CAPITAL, "capsLock");
        watch_toggle(&NUM_LOCK_ON, VK_NUMLOCK, "numLock");
    });
}

fn watch_toggle(known: &AtomicBool, vk: i32, key: &'static str) {
    let current = read_toggle_bit(vk);
    if known.swap(current, Ordering::Relaxed) != current {
        enqueue(key, current);
    }
}

/// 提示线程：窗口操作都放在这里，钩子回调必须尽快返回。
fn run_publish_loop(tasks: mpsc::Receiver<(&'static str, bool)>) {
    for (key, active) in tasks {
        publish(key, active);
    }
}

fn run_hook_loop(result: Sender<Result<(), String>>) {
    // SAFETY: 只读取模块句柄并安装钩子；句柄随线程与进程存活，无需手动释放。
    let installed = unsafe {
        match GetModuleHandleW(None) {
            Ok(instance) => {
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(HINSTANCE(instance.0)), 0)
            }
            Err(error) => {
                let _ = result.send(Err(error.to_string()));
                return;
            }
        }
    };

    match installed {
        Ok(_hook) => {
            let _ = result.send(Ok(()));
            // SAFETY: 低级键盘钩子要求安装线程持续抽取消息。
            unsafe { run_message_loop() };
        }
        Err(error) => {
            let _ = result.send(Err(error.to_string()));
        }
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: code 非负时 lparam 指向系统填好的 KBDLLHOOKSTRUCT。
    unsafe {
        if code >= 0 {
            let message = wparam.0 as u32;
            let event = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            if (message == WM_KEYUP || message == WM_SYSKEYUP) && event.vkCode as i32 == VK_INSERT {
                enqueue("insert", flip_insert());
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }
}

unsafe fn run_message_loop() {
    let mut message = MSG::default();
    // SAFETY: 低级键盘钩子要求安装线程持续抽取消息。
    unsafe {
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

/// 把一次状态变化交给提示线程；钩子回调与轮询线程共用。
fn enqueue(key: &'static str, active: bool) {
    let Some(queue) = QUEUE.get() else { return };
    match queue.try_send((key, active)) {
        Err(TrySendError::Full(_)) => eprintln!("[lumina-tiles] 按键提示队列已满，丢弃一次提示"),
        Err(TrySendError::Disconnected(_)) => {}
        Ok(()) => {}
    }
}

/// Insert 没有系统级开关状态可查，只能按钩子事件翻转本地状态。
fn flip_insert() -> bool {
    let active = !INSERT_ON.load(Ordering::Relaxed);
    INSERT_ON.store(active, Ordering::Relaxed);
    active
}

fn publish(key: &'static str, active: bool) {
    let Some(app) = APP.get() else { return };
    let Some(window) = app.get_webview_window(WINDOW_LABEL) else {
        return;
    };

    let settings = current_settings();

    // 先送文案让前端重新测量尺寸，再定位显示；顺序颠倒会让窗口先按旧尺寸出现再收缩跳动。
    if let Err(error) = window.emit(EVENT_STATE, LockStatePayload { key, active }) {
        eprintln!("[lumina-tiles] 按键提示事件发送失败：{error}");
    }
    // 提示线程允许短暂等待；给前端一个测量 + resize 往返的时间。
    thread::sleep(Duration::from_millis(16));
    position_window(app, &window, &settings);

    if let Err(error) = window.show() {
        eprintln!("[lumina-tiles] 按键提示窗口显示失败：{error}");
        return;
    }

    // 停留结束后先让前端播退场动画，动画播完再隐藏窗口。
    let revision = REVISION.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    // 无动画时不必等退场，直接隐藏；有动画则留出动画时长。
    let exit_ms = if settings.animation == AnimationPreset::None {
        0
    } else {
        settings.duration_ms
    };
    thread::spawn(move || {
        thread::sleep(VISIBLE_DURATION);
        if REVISION.load(Ordering::Relaxed) != revision {
            return;
        }
        if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
            if let Err(error) = window.emit(EVENT_HIDE, HidePayload { duration_ms: exit_ms }) {
                eprintln!("[lumina-tiles] 按键提示隐藏事件发送失败：{error}");
            }
        }
        thread::sleep(Duration::from_millis(u64::from(exit_ms)));
        if REVISION.load(Ordering::Relaxed) != revision {
            return;
        }
        if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
            let _ = window.hide();
        }
    });
}

/// 按设置把提示窗摆到目标显示器的指定位置，并限制在该显示器范围内。
fn position_window(app: &AppHandle, window: &WebviewWindow, settings: &LockIndicatorSettings) {
    let scale = window.scale_factor().unwrap_or(1.0);
    // 用已同步的逻辑尺寸换算物理尺寸：刚 set_size 后 outer_size 可能仍是旧值。
    let width = f64::from(TOAST_WIDTH.load(Ordering::Relaxed)) * scale;
    let height = f64::from(TOAST_HEIGHT.load(Ordering::Relaxed)) * scale;
    let cursor = cursor_point();
    let Some((left, top, area_width, area_height)) = monitor_area(app, settings, cursor) else {
        return;
    };

    let margin = ANCHOR_MARGIN * scale;
    let (mut x, mut y) = match settings.position {
        PositionPreset::Cursor => {
            let point = cursor.unwrap_or(POINT {
                x: (left + area_width / 2.0) as i32,
                y: (top + area_height / 2.0) as i32,
            });
            (
                f64::from(point.x) - width / 2.0,
                f64::from(point.y) - height - CURSOR_OFFSET * scale,
            )
        }
        preset => anchor_position(preset, left, top, area_width, area_height, width, height, margin),
    };

    x += f64::from(settings.offset_x) * scale;
    y += f64::from(settings.offset_y) * scale;

    let max_x = (left + area_width - width).max(left);
    let max_y = (top + area_height - height).max(top);
    let x = x.clamp(left, max_x).round() as i32;
    let y = y.clamp(top, max_y).round() as i32;

    if let Err(error) = window.set_position(PhysicalPosition::new(x, y)) {
        eprintln!("[lumina-tiles] 按键提示窗口定位失败：{error}");
    }
}

fn anchor_position(
    preset: PositionPreset,
    left: f64,
    top: f64,
    area_width: f64,
    area_height: f64,
    width: f64,
    height: f64,
    margin: f64,
) -> (f64, f64) {
    let center_x = left + (area_width - width) / 2.0;
    let center_y = top + (area_height - height) / 2.0;
    let right = left + area_width - width - margin;
    let bottom = top + area_height - height - margin;
    match preset {
        PositionPreset::TopLeft => (left + margin, top + margin),
        PositionPreset::TopCenter => (center_x, top + margin),
        PositionPreset::TopRight => (right, top + margin),
        PositionPreset::MiddleLeft => (left + margin, center_y),
        PositionPreset::Center => (center_x, center_y),
        PositionPreset::MiddleRight => (right, center_y),
        PositionPreset::BottomLeft => (left + margin, bottom),
        PositionPreset::BottomCenter => (center_x, bottom),
        PositionPreset::BottomRight => (right, bottom),
        PositionPreset::Cursor => (center_x, center_y),
    }
}

fn cursor_point() -> Option<POINT> {
    let mut point = POINT::default();
    // SAFETY: GetCursorPos 只写入传入的 POINT。
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return None;
    }
    Some(point)
}

fn monitor_area(
    app: &AppHandle,
    settings: &LockIndicatorSettings,
    cursor: Option<POINT>,
) -> Option<(f64, f64, f64, f64)> {
    let wanted = settings.monitor.trim();
    if !wanted.is_empty() {
        if let Ok(monitors) = app.available_monitors() {
            for monitor in &monitors {
                if monitor.name().map(|name| name.as_str() == wanted).unwrap_or(false) {
                    return Some(monitor_area_of(monitor));
                }
            }
        }
    }
    if let Some(point) = cursor {
        if let Ok(Some(monitor)) = app.monitor_from_point(f64::from(point.x), f64::from(point.y)) {
            return Some(monitor_area_of(&monitor));
        }
    }
    app.primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| monitor_area_of(&monitor))
}

fn monitor_area_of(monitor: &tauri::Monitor) -> (f64, f64, f64, f64) {
    let origin = monitor.position();
    let size = monitor.size();
    (
        f64::from(origin.x),
        f64::from(origin.y),
        f64::from(size.width),
        f64::from(size.height),
    )
}

/// 让提示窗不抢焦点、不接收鼠标事件；窗口尺寸由前端测量内容后同步。
pub fn init_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW_LABEL) else {
        eprintln!("[lumina-tiles] 未找到按键提示窗口");
        return;
    };
    if let Err(error) = window.set_ignore_cursor_events(true) {
        eprintln!("[lumina-tiles] 按键提示窗口穿透设置失败：{error}");
    }
    if let Err(error) = window.set_focusable(false) {
        eprintln!("[lumina-tiles] 按键提示窗口焦点设置失败：{error}");
    }
    if let Err(error) = window.set_size(LogicalSize::new(f64::from(BASE_WIDTH), f64::from(BASE_HEIGHT))) {
        eprintln!("[lumina-tiles] 按键提示窗口初始尺寸设置失败：{error}");
    }
}
