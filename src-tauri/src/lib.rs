mod features;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            features::lock_indicator::init_window(&handle);
            features::lock_indicator::init(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            features::lock_indicator::lock_indicator_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
