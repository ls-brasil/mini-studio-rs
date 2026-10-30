mod capture;
mod streaming;

use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Arc::new(streaming::CaptureSessions::default()))
        .invoke_handler(tauri::generate_handler![
            capture::list_capture_sources,
            streaming::start_capture,
            streaming::capture_frame,
            streaming::stop_capture,
            capture::save_snapshot,
        ])
        .build(tauri::generate_context!())
        .expect("Não foi possível iniciar o Mini Studio")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                app.state::<Arc<streaming::CaptureSessions>>().shutdown();
            }
        });
}
