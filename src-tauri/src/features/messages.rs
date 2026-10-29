//! 消息中心：唯一权威路由。发布、去重、广播都只发生在这里，前端不本地入队，
//! 因此一次发布不会出现两份。显示由横幅调度器负责，Rust 同时是唯一真源。

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

/// 重要级：决定谁先上屏、占多宽，与决定颜色的 level 正交。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Importance {
    High,
    #[default]
    Normal,
    Low,
}

impl Importance {
    /// 越大越重要，用于挤占与队列排序。
    pub fn rank(self) -> u8 {
        match self {
            Importance::High => 2,
            Importance::Normal => 1,
            Importance::Low => 0,
        }
    }
}

impl std::fmt::Display for Importance {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Importance::High => "high",
            Importance::Normal => "normal",
            Importance::Low => "low",
        };
        formatter.write_str(text)
    }
}

/// 广播给所有窗口的消息体。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// 全局唯一、单调递增
    pub id: String,
    /// 显示等级：决定颜色
    pub level: String,
    /// 重要级：决定顺序与占比
    #[serde(default)]
    pub importance: Importance,
    /// 与 text 二选一：翻译键或已翻译文本
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub i18n_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(default = "default_origin")]
    pub origin: String,
    /// 停留毫秒；0 表示跟随横幅设置
    #[serde(default)]
    pub duration_ms: u64,
    /// 去重合并次数
    #[serde(default = "default_count")]
    pub count: u32,
}

fn default_origin() -> String {
    "frontend".to_string()
}

fn default_count() -> u32 {
    1
}

/// 发布请求：id、计数由后端分配。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishRequest {
    pub level: String,
    #[serde(default)]
    pub importance: Option<Importance>,
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

/// 发布一条消息：校验 → 去重 → 广播 → 交给横幅调度。
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
    // 0 与缺失表示跟随横幅设置，由调度器补默认停留时长。
    let duration_ms = match request.duration_ms {
        None | Some(0) => 0,
        Some(value) => value.clamp(MIN_DURATION_MS, MAX_DURATION_MS),
    };
    let origin = request.origin.unwrap_or_else(|| "frontend".to_string());
    let importance = request.importance.unwrap_or_default();

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
            // 去重期间带了新的重要级就采用新值，否则沿用原值。
            if request.importance.is_some() {
                previous.message.importance = importance;
            }
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
        importance,
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

/// 撤销一条消息：池与队列都会移除，页面靠状态差集播退场。
pub fn dismiss(app: &AppHandle, id: &str) -> Result<(), String> {
    app.emit(EVENT_DISMISSED, serde_json::json!({ "id": id }))
        .map_err(|error| format!("消息撤销广播失败：{error}"))?;
    banner::dismiss(app, id);
    Ok(())
}

fn broadcast(app: &AppHandle, message: &Message) -> Result<(), String> {
    app.emit(EVENT_MESSAGE, message)
        .map_err(|error| format!("消息广播失败：{error}"))?;
    banner::dispatch(app, message);
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
