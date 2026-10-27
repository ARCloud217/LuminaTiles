//! 按键提示：用低级键盘钩子捕获 CapsLock / NumLock / Insert 的切换，
//! 在鼠标上方弹出提示窗，并在短暂停留后自动隐藏。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender, SyncSender, TrySendError};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};
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
const EVENT_NAME: &str = "lock-indicator:state";
/// 提示窗停留时长
const VISIBLE_DURATION: Duration = Duration::from_millis(1200);
/// 提示窗底边与鼠标指针的距离（逻辑像素）
const CURSOR_OFFSET: f64 = 28.0;

static APP: OnceLock<AppHandle> = OnceLock::new();
static AVAILABLE: AtomicBool = AtomicBool::new(false);
static CAPS_LOCK_ON: AtomicBool = AtomicBool::new(false);
static NUM_LOCK_ON: AtomicBool = AtomicBool::new(false);
static INSERT_ON: AtomicBool = AtomicBool::new(false);
/// 每次提示自增，用来让旧的隐藏任务失效
static REVISION: AtomicU64 = AtomicU64::new(0);
/// 钩子回调到提示线程的队列；容量有限，避免异常情况下堆积。
static QUEUE: OnceLock<SyncSender<(&'static str, bool)>> = OnceLock::new();

#[derive(Clone, Serialize)]
struct LockStatePayload {
    key: &'static str,
    active: bool,
}

/// 启动钩子线程，并等到钩子安装结果后返回。
pub fn init(app: AppHandle) {
    let _ = APP.set(app);

    CAPS_LOCK_ON.store(read_toggle_bit(VK_CAPITAL), Ordering::Relaxed);
    NUM_LOCK_ON.store(read_toggle_bit(VK_NUMLOCK), Ordering::Relaxed);

    let (queue, tasks) = mpsc::sync_channel::<(&'static str, bool)>(16);
    let _ = QUEUE.set(queue);
    thread::spawn(move || run_publish_loop(tasks));

    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || run_hook_loop(sender));

    let installed = matches!(receiver.recv_timeout(Duration::from_secs(2)), Ok(Ok(())));
    AVAILABLE.store(installed, Ordering::Relaxed);
    if !installed {
        eprintln!("[lumina-tiles] 键盘钩子安装失败，按键提示不可用");
    }
}

/// 供前端查询按键提示是否可用。
#[tauri::command]
pub fn lock_indicator_status() -> bool {
    AVAILABLE.load(Ordering::Relaxed)
}

fn read_toggle_bit(vk: i32) -> bool {
    // SAFETY: GetKeyState 只读当前线程的键盘状态，无副作用。
    (unsafe { GetKeyState(vk) } & 0x0001) != 0
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
            if message == WM_KEYUP || message == WM_SYSKEYUP {
                enqueue_toggle(event.vkCode as i32);
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

/// 钩子回调内的快路径：只读状态并入队，喂给提示线程。
fn enqueue_toggle(vk: i32) {
    let (key, active) = match vk {
        VK_CAPITAL => ("capsLock", refresh_toggle(&CAPS_LOCK_ON, VK_CAPITAL)),
        VK_NUMLOCK => ("numLock", refresh_toggle(&NUM_LOCK_ON, VK_NUMLOCK)),
        VK_INSERT => ("insert", flip(&INSERT_ON)),
        _ => return,
    };
    let Some(queue) = QUEUE.get() else { return };
    match queue.try_send((key, active)) {
        Err(TrySendError::Full(_)) => eprintln!("[lumina-tiles] 按键提示队列已满，丢弃一次提示"),
        Err(TrySendError::Disconnected(_)) => {}
        Ok(()) => {}
    }
}

/// 以系统开关状态为准；若系统尚未更新（和已知状态相同），则按翻转处理。
fn refresh_toggle(known: &AtomicBool, vk: i32) -> bool {
    let previous = known.load(Ordering::Relaxed);
    let system = read_toggle_bit(vk);
    let active = if system == previous { !previous } else { system };
    known.store(active, Ordering::Relaxed);
    active
}

fn flip(known: &AtomicBool) -> bool {
    let active = !known.load(Ordering::Relaxed);
    known.store(active, Ordering::Relaxed);
    active
}

fn publish(key: &'static str, active: bool) {
    let Some(app) = APP.get() else { return };
    let Some(window) = app.get_webview_window(WINDOW_LABEL) else {
        return;
    };

    if let Err(error) = window.emit(EVENT_NAME, LockStatePayload { key, active }) {
        eprintln!("[lumina-tiles] 按键提示事件发送失败：{error}");
    }
    position_above_cursor(app, &window);
    if let Err(error) = window.show() {
        eprintln!("[lumina-tiles] 按键提示窗口显示失败：{error}");
        return;
    }

    let revision = REVISION.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    thread::spawn(move || {
        let deadline = Instant::now() + VISIBLE_DURATION;
        while Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        if REVISION.load(Ordering::Relaxed) == revision {
            if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
                let _ = window.hide();
            }
        }
    });
}

/// 把提示窗摆在鼠标指针上方，并限制在当前显示器范围内。
fn position_above_cursor(app: &AppHandle, window: &tauri::WebviewWindow) {
    let mut point = POINT::default();
    // SAFETY: GetCursorPos 只写入传入的 POINT。
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return;
    }
    let Ok(size) = window.outer_size() else {
        return;
    };

    let scale = window.scale_factor().unwrap_or(1.0);
    let width = f64::from(size.width);
    let height = f64::from(size.height);
    let mut x = f64::from(point.x) - width / 2.0;
    let mut y = f64::from(point.y) - height - CURSOR_OFFSET * scale;

    if let Ok(Some(monitor)) = app.monitor_from_point(f64::from(point.x), f64::from(point.y)) {
        let origin = monitor.position();
        let area = monitor.size();
        let min_x = f64::from(origin.x);
        let min_y = f64::from(origin.y);
        let max_x = (min_x + f64::from(area.width) - width).max(min_x);
        let max_y = (min_y + f64::from(area.height) - height).max(min_y);
        x = x.clamp(min_x, max_x);
        y = y.clamp(min_y, max_y);
    }

    if let Err(error) = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32)) {
        eprintln!("[lumina-tiles] 按键提示窗口定位失败：{error}");
    }
}

/// 让提示窗不抢焦点、不接收鼠标事件。
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
}
