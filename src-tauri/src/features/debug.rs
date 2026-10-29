//! 调试页后端：重置本地数据并退出，供危险区一键清空使用。

use tauri::AppHandle;

use crate::settings;

/// 删除整份 settings.json 后立即退出进程；下次启动回到默认主题与设置。
#[tauri::command]
pub fn reset_app_data_and_exit(app: AppHandle) -> Result<(), String> {
    settings::clear(&app)?;
    println!("[lumina-tiles] 已清除本地数据，退出进程");
    app.exit(0);
    Ok(())
}
