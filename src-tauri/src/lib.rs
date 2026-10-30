mod capture;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            capture::list_capture_sources,
            capture::capture_frame,
            capture::save_snapshot,
        ])
        .run(tauri::generate_context!())
        .expect("Não foi possível iniciar o Mini Studio");
}
