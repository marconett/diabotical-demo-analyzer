mod commands;
mod state;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let cache_dir = app.path().app_data_dir()?.join("extract-cache");
            app.manage(state::AppState::new(cache_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_demos,
            commands::discover_players,
            commands::run_scene_finder,
            commands::cancel,
            commands::cache_stats,
            commands::clear_cache,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
