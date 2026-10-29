mod features;
mod settings;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            features::lock_indicator::init(handle.clone());
            features::theme::init(&handle);
            features::banner::init(&handle);
            features::lock_indicator::init_window(&handle);
            features::tray::init(&handle)?;
            Ok(())
        })
        // 主窗口关闭即隐藏到托盘，退出只能通过托盘菜单。
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            features::lock_indicator::lock_indicator_status,
            features::lock_indicator::lock_indicator_settings,
            features::lock_indicator::update_lock_indicator_settings,
            features::lock_indicator::reset_lock_indicator_settings,
            features::lock_indicator::preview_lock_indicator,
            features::lock_indicator::lock_indicator_monitors,
            features::lock_indicator::lock_indicator_resize,
            features::theme::theme_settings,
            features::theme::update_theme_settings,
            features::theme::reset_theme_settings,
            features::theme::system_theme,
            features::messages::message_publish,
            features::messages::message_dismiss,
            features::banner::banner_settings,
            features::banner::update_banner_settings,
            features::banner::banner_ready,
            features::banner::banner_report,
            features::banner::banner_idle,
            features::banner::banner_clear,
            features::banner::banner_show,
            features::banner::banner_geometry,
            features::debug::reset_app_data_and_exit
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
