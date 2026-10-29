//! 顶部横幅：多格并排。调度规则（池、队列、占比、挤占、计时）都在 scheduler.rs，
//! 这里只负责窗口、线程与事件：把快照推给页面，按页面上报的矩形做命中。

mod scheduler;

use std::sync::{Mutex, MutexGuard, OnceLock, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use crate::features::messages::Message;
use crate::settings;
use scheduler::{Scheduler, Snapshot};

const WINDOW_LABEL: &str = "banner";
const SETTINGS_SECTION: &str = "banner";
const EVENT_STATE: &str = "banner:state";
const EVENT_HOVER: &str = "banner:hover";

/// 可见时的光标轮询间隔：够跟上手速，又不至于明显吃 CPU。
const VISIBLE_POLL_INTERVAL: Duration = Duration::from_millis(30);
/// 隐藏时的空转间隔，避免窗口没显示时还在高频轮询。
const HIDDEN_POLL_INTERVAL: Duration = Duration::from_millis(150);
/// 调度节拍：到期判断与兜底收窗都按它推进。
const TICK_INTERVAL: Duration = Duration::from_millis(100);
/// 池空后等页面播完退场；等不到就兜底收窗，避免透明窗口一直贴在屏幕上。
const IDLE_TIMEOUT: Duration = Duration::from_millis(1000);
const DEFAULT_DURATION_MS: u64 = 5000;
const MIN_DURATION_MS: u64 = 1000;
const MAX_DURATION_MS: u64 = 30000;
const MIN_HEIGHT: f64 = 24.0;
const MAX_HEIGHT: f64 = 240.0;
const BASE_HEIGHT: f64 = 64.0;

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

/// 页面上报的格子矩形：CSS 逻辑像素，相对窗口内容区左上角。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotReport {
    id: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Debug, Default)]
struct Geometry {
    epoch: u64,
    height: f64,
    slots: Vec<(String, Rect)>,
}

struct Runtime {
    scheduler: Scheduler,
    geometry: Geometry,
    epoch: u64,
    visible: bool,
    ready: bool,
    idle_deadline: Option<Instant>,
}

impl Runtime {
    fn new() -> Self {
        Self {
            scheduler: Scheduler::new(DEFAULT_DURATION_MS),
            geometry: Geometry::default(),
            epoch: 0,
            visible: false,
            ready: false,
            idle_deadline: None,
        }
    }

    fn content_height(&self) -> f64 {
        if self.geometry.height > 0.0 {
            self.geometry.height.clamp(MIN_HEIGHT, MAX_HEIGHT)
        } else {
            BASE_HEIGHT
        }
    }
}

static RUNTIME: OnceLock<Mutex<Runtime>> = OnceLock::new();
static SETTINGS: OnceLock<RwLock<BannerSettings>> = OnceLock::new();

fn runtime() -> MutexGuard<'static, Runtime> {
    RUNTIME
        .get_or_init(|| Mutex::new(Runtime::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn settings_lock() -> &'static RwLock<BannerSettings> {
    SETTINGS.get_or_init(|| RwLock::new(BannerSettings::default()))
}

/// 当前设置；锁被污染时退回默认值，不让横幅彻底失效。
fn current_settings() -> BannerSettings {
    settings_lock()
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

/// 启动：读设置、把窗口调成穿透，并起调度与光标轮询线程。
pub fn init(app: &AppHandle) {
    let loaded = settings::read_section::<BannerSettings>(app, SETTINGS_SECTION)
        .unwrap_or_default()
        .normalized();
    if let Ok(mut guard) = settings_lock().write() {
        *guard = loaded.clone();
    }
    runtime().scheduler.set_default_duration(loaded.duration_ms);
    init_window(app);
    start_ticking(app.clone());
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

/// 消息路由完成后调用：交给调度器，再把结果推给页面。
pub fn dispatch(app: &AppHandle, message: &Message) {
    let now = Instant::now();
    let changed = runtime().scheduler.arrive(message.clone(), now);
    apply(app, changed);
}

/// 撤销一条消息（点击或外部调用）。
pub fn dismiss(app: &AppHandle, id: &str) {
    let now = Instant::now();
    let changed = runtime().scheduler.dismiss(id, now);
    apply(app, changed);
}

/// 推送快照、决定显隐。持锁时间只覆盖状态运算，窗口操作一律放锁后。
fn apply(app: &AppHandle, changed: bool) {
    let now = Instant::now();
    let (payload, need_place, hide_now);
    {
        let mut rt = runtime();
        if changed {
            rt.epoch += 1;
        }
        let snapshot = rt.scheduler.snapshot();
        let empty = snapshot.slots.is_empty() && snapshot.queue_count == 0;
        if empty {
            if rt.visible && rt.idle_deadline.is_none() {
                rt.idle_deadline = Some(now + IDLE_TIMEOUT);
            }
        } else {
            rt.idle_deadline = None;
        }
        payload = if changed {
            Some(state_payload(rt.epoch, &snapshot))
        } else {
            None
        };
        need_place = !snapshot.slots.is_empty() && !rt.visible;
        if need_place {
            rt.visible = true;
            rt.geometry = Geometry {
                epoch: rt.epoch,
                ..Geometry::default()
            };
        }
        hide_now = rt
            .idle_deadline
            .map(|deadline| now >= deadline)
            .unwrap_or(false);
        if hide_now {
            rt.idle_deadline = None;
            rt.visible = false;
            rt.geometry = Geometry::default();
        }
    }
    // 先让页面拿到内容，再把窗口摆上屏，避免闪一帧空白。
    if let Some(payload) = payload {
        if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
            if let Err(error) = window.emit(EVENT_STATE, &payload) {
                eprintln!("[lumina-tiles] 横幅状态发送失败：{error}");
            }
        }
    }
    if need_place {
        place(app);
    }
    if hide_now {
        hide(app);
    }
}

fn state_payload(epoch: u64, snapshot: &Snapshot) -> Value {
    let slots: Vec<Value> = snapshot
        .slots
        .iter()
        .map(|slot| {
            let mut value = serde_json::to_value(&slot.message).unwrap_or(Value::Null);
            if let Some(map) = value.as_object_mut() {
                map.insert("width".to_string(), json!(slot.width));
            }
            value
        })
        .collect();
    json!({
        "epoch": epoch,
        "slots": slots,
        "queueCount": snapshot.queue_count,
    })
}

/// 摆到目标显示器顶部：宽度锁屏宽，高度取页面量出的内容高度。
fn place(app: &AppHandle) {
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
    let height = runtime().content_height() * scale;
    let size = PhysicalSize::new(
        area_width.max(1.0).round() as u32,
        height.max(1.0).round() as u32,
    );
    if let Err(error) = window.set_size(size) {
        eprintln!("[lumina-tiles] 横幅尺寸设置失败：{error}");
    }
    if let Err(error) = window.set_position(PhysicalPosition::new(
        left.round() as i32,
        top.round() as i32,
    )) {
        eprintln!("[lumina-tiles] 横幅定位失败：{error}");
    }
    if let Err(error) = window.show() {
        eprintln!("[lumina-tiles] 横幅显示失败：{error}");
    }
}

fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        if let Err(error) = window.hide() {
            eprintln!("[lumina-tiles] 横幅隐藏失败：{error}");
        }
    }
}

/// 调度线程：到期移除、补位，以及等不到页面收尾时的兜底收窗。
fn start_ticking(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(TICK_INTERVAL);
        let now = Instant::now();
        let (changed, idle_overdue) = {
            let mut rt = runtime();
            let changed = rt.scheduler.tick(now);
            let idle_overdue = rt
                .idle_deadline
                .map(|deadline| now >= deadline)
                .unwrap_or(false);
            (changed, idle_overdue)
        };
        if changed || idle_overdue {
            apply(&app, changed);
        }
    });
}

/// 光标轮询：只做两件页面做不到的事——悬停淡化与按格命中。
/// 窗口是点击穿透的，WndProc 收不到鼠标事件，只能自己读光标与按键状态。
fn start_polling(app: AppHandle) {
    thread::spawn(move || {
        let mut inside = false;
        let mut pressed_before = false;
        loop {
            let Some(window) = app.get_webview_window(WINDOW_LABEL) else {
                thread::sleep(HIDDEN_POLL_INTERVAL);
                continue;
            };
            let visible = {
                let rt = runtime();
                rt.visible
            };
            if !visible {
                // 收窗期间光标会离开，但页面收不到轮询；补一条复位，
                // 否则下一条消息会带着上一次的淡化状态出现。
                if inside {
                    inside = false;
                    if let Err(error) = window.emit(EVENT_HOVER, json!({ "inside": false })) {
                        eprintln!("[lumina-tiles] 横幅悬停事件发送失败：{error}");
                    }
                }
                pressed_before = false;
                thread::sleep(HIDDEN_POLL_INTERVAL);
                continue;
            }

            let surface = window_rect(&window);
            let point = cursor_point();
            let inside_window = surface
                .zip(point)
                .map(|((x, y, width, height), point)| {
                    let px = f64::from(point.x);
                    let py = f64::from(point.y);
                    px >= x && px < x + width && py >= y && py < y + height
                })
                .unwrap_or(false);
            let hit = if inside_window {
                surface
                    .zip(point)
                    .and_then(|(surface, point)| {
                        let scale = window.scale_factor().unwrap_or(1.0);
                        let rt = runtime();
                        hit_test(&rt, surface, scale, point)
                    })
            } else {
                None
            };

            if inside_window != inside {
                inside = inside_window;
                if let Err(error) = window.emit(EVENT_HOVER, json!({ "inside": inside })) {
                    eprintln!("[lumina-tiles] 横幅悬停事件发送失败：{error}");
                }
            }

            // SAFETY: GetAsyncKeyState 只读按键状态，不要求窗口句柄。
            let pressed = unsafe { GetAsyncKeyState(i32::from(VK_LBUTTON.0)) } as u16 & 0x8000 != 0;
            if pressed && !pressed_before && inside {
                if let Some(id) = hit {
                    dismiss(&app, &id);
                }
            }
            pressed_before = pressed;

            thread::sleep(VISIBLE_POLL_INTERVAL);
        }
    });
}

/// 命中哪一格。矩形是上一次上报的布局，重排期间（epoch 不符）一律不响应，
/// 否则会用旧坐标点掉相邻的格子。
fn hit_test(
    rt: &Runtime,
    surface: (f64, f64, f64, f64),
    scale: f64,
    point: POINT,
) -> Option<String> {
    if rt.geometry.epoch != rt.epoch {
        return None;
    }
    let (left, top, _, _) = surface;
    let px = f64::from(point.x);
    let py = f64::from(point.y);
    for (id, rect) in &rt.geometry.slots {
        let x = left + rect.x * scale;
        let y = top + rect.y * scale;
        if px >= x && px < x + rect.width * scale && py >= y && py < y + rect.height * scale {
            return Some(id.clone());
        }
    }
    None
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
fn monitor_area(
    app: &AppHandle,
    settings: &BannerSettings,
    cursor: Option<POINT>,
) -> Option<(f64, f64, f64)> {
    let wanted = settings.monitor.trim();
    if !wanted.is_empty() {
        if let Ok(monitors) = app.available_monitors() {
            for monitor in &monitors {
                if monitor
                    .name()
                    .map(|name| name.as_str() == wanted)
                    .unwrap_or(false)
                {
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
    app.primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| area_of(&monitor))
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
    runtime()
        .scheduler
        .set_default_duration(settings.duration_ms);
    Ok(settings)
}

/// 保存设置：默认停留时长只影响之后上屏的消息，队列里的不受影响。
#[tauri::command]
pub fn update_banner_settings(
    app: AppHandle,
    settings: BannerSettings,
) -> Result<BannerSettings, String> {
    let settings = settings.normalized();
    settings::patch(&app, SETTINGS_SECTION, &settings)?;
    if let Ok(mut guard) = settings_lock().write() {
        *guard = settings.clone();
    }
    runtime()
        .scheduler
        .set_default_duration(settings.duration_ms);
    Ok(settings)
}

/// 横幅页订阅完事件后的握手：把当前快照补给它。
#[tauri::command]
pub fn banner_ready(app: AppHandle) {
    let payload = {
        let mut rt = runtime();
        rt.ready = true;
        if rt.scheduler.is_empty() {
            None
        } else {
            Some(state_payload(rt.epoch, &rt.scheduler.snapshot()))
        }
    };
    if let Some(payload) = payload {
        if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
            if let Err(error) = window.emit(EVENT_STATE, &payload) {
                eprintln!("[lumina-tiles] 横幅状态发送失败：{error}");
            }
        }
    }
}

/// 页面上报几何：只接受当前 epoch，旧布局的矩形不能用来命中。
#[tauri::command]
pub fn banner_report(app: AppHandle, epoch: u64, height: f64, slots: Vec<SlotReport>) {
    let height = height.clamp(MIN_HEIGHT, MAX_HEIGHT);
    {
        let mut rt = runtime();
        if !rt.visible || rt.epoch != epoch {
            return;
        }
        rt.geometry = Geometry {
            epoch,
            height,
            slots: slots
                .into_iter()
                .map(|slot| {
                    (
                        slot.id,
                        Rect {
                            x: slot.x,
                            y: slot.y,
                            width: slot.width,
                            height: slot.height,
                        },
                    )
                })
                .collect(),
        };
    }
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        let scale = window.scale_factor().unwrap_or(1.0);
        let Ok(current) = window.outer_size() else {
            return;
        };
        let target = (height * scale).max(1.0).round() as u32;
        if target != current.height {
            let size = PhysicalSize::new(current.width, target);
            if let Err(error) = window.set_size(size) {
                eprintln!("[lumina-tiles] 横幅高度调整失败：{error}");
            }
        }
    }
}

/// 页面播完退场：池空才收窗，池非空说明还有消息要上屏，忽略这次收尾。
#[tauri::command]
pub fn banner_idle(app: AppHandle) {
    let empty = {
        let mut rt = runtime();
        if !rt.scheduler.is_empty() {
            return;
        }
        rt.idle_deadline = None;
        rt.visible = false;
        rt.geometry = Geometry::default();
        true
    };
    if empty {
        hide(&app);
    }
}

/// 清空横幅：池与队列一起丢，页面按新快照收尾。
#[tauri::command]
pub fn banner_clear(app: AppHandle) {
    {
        let mut rt = runtime();
        rt.scheduler.clear();
    }
    apply(&app, true);
}

/// 外部显式显示（调试用）：与消息路由走同一条路径。
#[tauri::command]
pub fn banner_show(app: AppHandle, message: Value) {
    match serde_json::from_value::<Message>(message) {
        Ok(message) => dispatch(&app, &message),
        Err(error) => eprintln!("[lumina-tiles] 横幅消息解析失败：{error}"),
    }
}

/// 调试页体检：窗口矩形、光标位置、屏上格子与队列读数。
#[tauri::command]
pub fn banner_geometry(app: AppHandle) -> Value {
    let window = app.get_webview_window(WINDOW_LABEL);
    let rect = window.as_ref().and_then(|window| {
        window_rect(window)
            .map(|(x, y, width, height)| json!({ "x": x, "y": y, "width": width, "height": height }))
    });
    let cursor = cursor_point().map(|point| json!({ "x": point.x, "y": point.y }));
    let (snapshot, epoch, visible) = {
        let rt = runtime();
        (rt.scheduler.snapshot(), rt.epoch, rt.visible)
    };
    let slots: Vec<Value> = snapshot
        .slots
        .iter()
        .map(|slot| {
            json!({
                "id": slot.message.id,
                "level": slot.message.level,
                "importance": slot.message.importance,
                "width": slot.width,
            })
        })
        .collect();
    json!({
        "visible": visible,
        "rect": rect,
        "cursor": cursor,
        "epoch": epoch,
        "queueCount": snapshot.queue_count,
        "queueHead": snapshot.queue_head,
        "slots": slots,
    })
}
