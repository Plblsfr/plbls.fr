#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(estompe_core::commands::OAuth::default())
        .setup(|app| {
            use tauri::Manager as _;
            if let Ok(dir) = app.path().app_log_dir() {
                estompe_core::debuglog::init(dir);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            estompe_core::commands::oauth_start, estompe_core::commands::oauth_wait, estompe_core::commands::native_fetch,
            estompe_core::commands::secret_set, estompe_core::commands::secret_get, estompe_core::commands::secret_delete,
            estompe_core::debuglog::debug_set, estompe_core::debuglog::debug_write, estompe_core::debuglog::debug_read, estompe_core::debuglog::debug_clear])
        .run(tauri::generate_context!())
        .expect("erreur au lancement d'Estompe");
}
