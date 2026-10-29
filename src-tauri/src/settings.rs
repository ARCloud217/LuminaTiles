//! settings.json 的共享读写层。
//!
//! 每个功能只负责自己的顶层键：读取时整份解析后取自己那节，写入时按顶层键合并，
//! 这样某个功能保存设置不会把其它功能的数据整份覆盖掉。

use std::fs;
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager};

const SETTINGS_FILE: &str = "settings.json";

/// 配置文件路径（应用配置目录下的 settings.json）。
pub fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("无法获取配置目录：{error}"))?;
    Ok(dir.join(SETTINGS_FILE))
}

/// 整份读取顶层对象；文件不存在视为空配置。
pub fn read_root(app: &AppHandle) -> Result<Map<String, Value>, String> {
    let path = path(app)?;
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(error) => return Err(format!("设置读取失败（{}）：{error}", path.display())),
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(format!("设置格式错误（{}）：顶层必须是对象", path.display())),
        Err(error) => Err(format!("设置解析失败：{error}")),
    }
}

/// 读某一节；该节缺失时返回该类型的默认值。
pub fn read_section<T>(app: &AppHandle, section: &str) -> Result<T, String>
where
    T: DeserializeOwned + Default,
{
    let root = read_root(app)?;
    match root.get(section) {
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|error| format!("设置项 {section} 解析失败：{error}")),
        None => Ok(T::default()),
    }
}

/// 覆盖写入某一节，保留其它顶层键。
pub fn patch<T: Serialize>(app: &AppHandle, section: &str, value: &T) -> Result<(), String> {
    let mut root = read_root(app)?;
    let encoded =
        serde_json::to_value(value).map_err(|error| format!("设置序列化失败：{error}"))?;
    root.insert(section.to_string(), encoded);
    write_root(app, &root)
}

/// 删除整份配置文件（重置全部数据时使用）。
#[allow(dead_code)] // 调试页的重置命令接入后启用
pub fn clear(app: &AppHandle) -> Result<(), String> {
    let path = path(app)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("设置删除失败（{}）：{error}", path.display())),
    }
}

fn write_root(app: &AppHandle, root: &Map<String, Value>) -> Result<(), String> {
    let path = path(app)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|error| format!("无法创建配置目录：{error}"))?;
    }
    let text = serde_json::to_string_pretty(&Value::Object(root.clone()))
        .map_err(|error| format!("设置序列化失败：{error}"))?;
    // 先写临时文件再替换，避免写入中断留下半份配置。
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, text).map_err(|error| format!("设置写入失败：{error}"))?;
    fs::rename(&temp, &path).map_err(|error| format!("设置保存失败：{error}"))
}
