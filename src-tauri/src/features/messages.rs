//! 消息中心：唯一权威路由。发布、去重、广播都只发生在这里，前端不本地入队，
//! 因此一次发布不会出现两份。显示节奏归横幅页，Rust 只管路由与去重。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::features::banner;

const EVENT_MESSAGE: &str = "lumina:message";
const EVENT_DISMISSED: &str = "lumina:message-dismissed";
/// 同 level + i18nKey 在这个窗口内重复发布视为同一条，只累加计数。
const DEDUPE_WINDOW: Duration = Duration::from_millis(800);
const MIN_DURATION_MS: u64 = 500;
const MAX_DURATION_MS: u64 = 60000;

/// 广播给所有窗口的消息体。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// 全局唯一、单调递增
    pub id: String,
    pub level: String,
    /// 与 text 二选一：翻译键或已翻译文本
    #[serde(skip_serializing_if = "Option::is_none")]
    pub i18n_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    pub origin: String,
    /// 停留毫秒；0 表示跟随横幅设置
    pub duration_ms: u64,
    /// 去重合并次数
    pub count: u32,
}

/// 发布请求：id、计数由后端分配。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishRequest {
    pub level: String,
    #[serde(default)]
    pub i18n_key: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub params: Option<Value>,
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

struct LastPublish {
    message: Message,
    at: Instant,
}

static COUNTER: AtomicU64 = AtomicU64::new(0);
static LAST: OnceLock<Mutex<Option<LastPublish>>> = OnceLock::new();

fn last_publish() -> &'static Mutex<Option<LastPublish>> {
    LAST.get_or_init(|| Mutex::new(None))
}

/// 发布一条消息：校验 → 去重 → 广播 → 交给横幅显示。
pub fn publish(app: &AppHandle, request: PublishRequest) -> Result<Message, String> {
    let level = request.level.trim().to_lowercase();
    if !matches!(level.as_str(), "info" | "success" | "warning" | "error") {
        return Err(format!("未知消息等级：{}", request.level));
    }
    let i18n_key = request.i18n_key.filter(|key| !key.trim().is_empty());
    let text = request.text.filter(|value| !value.trim().is_empty());
    if i18n_key.is_none() && text.is_none() {
        return Err("消息必须提供 i18nKey 或 text".to_string());
    }
    // 0 与缺失表示跟随横幅设置，由横幅页补默认停留时长。
    let duration_ms = match request.duration_ms {
        None | Some(0) => 0,
        Some(value) => value.clamp(MIN_DURATION_MS, MAX_DURATION_MS),
    };
    let origin = request.origin.unwrap_or_else(|| "frontend".to_string());

    let mut guard = last_publish()
        .lock()
        .map_err(|_| "消息状态锁已损坏".to_string())?;
    if let Some(previous) = guard.as_mut() {
        // 文本消息没有 i18nKey，必须把 text 一起比较，否则不同文案会被误合并。
        let same_heading = previous.message.level == level
            && previous.message.i18n_key == i18n_key
            && previous.message.text == text;
        if same_heading && previous.at.elapsed() <= DEDUPE_WINDOW {
            previous.at = Instant::now();
            previous.message.count += 1;
            previous.message.duration_ms = duration_ms;
            let message = previous.message.clone();
            drop(guard);
            broadcast(app, &message)?;
            return Ok(message);
        }
    }

    let id = format!("msg-{}", COUNTER.fetch_add(1, Ordering::Relaxed) + 1);
    let message = Message {
        id,
        level,
        i18n_key,
        text,
        params: request.params,
        origin,
        duration_ms,
        count: 1,
    };
    *guard = Some(LastPublish {
        message: message.clone(),
        at: Instant::now(),
    });
    drop(guard);
    broadcast(app, &message)?;
    Ok(message)
}

/// 撤销一条消息：只广播，不关心它此刻是否在显示。
pub fn dismiss(app: &AppHandle, id: &str) -> Result<(), String> {
    app.emit(EVENT_DISMISSED, serde_json::json!({ "id": id }))
        .map_err(|error| format!("消息撤销广播失败：{error}"))
}

fn broadcast(app: &AppHandle, message: &Message) -> Result<(), String> {
    app.emit(EVENT_MESSAGE, message)
        .map_err(|error| format!("消息广播失败：{error}"))?;
    let payload = serde_json::to_value(message).map_err(|error| format!("消息序列化失败：{error}"))?;
    banner::show(app, &payload);
    Ok(())
}

#[tauri::command]
pub fn message_publish(app: AppHandle, message: PublishRequest) -> Result<Message, String> {
    publish(&app, message)
}

#[tauri::command]
pub fn message_dismiss(app: AppHandle, id: String) -> Result<(), String> {
    dismiss(&app, &id)
}
