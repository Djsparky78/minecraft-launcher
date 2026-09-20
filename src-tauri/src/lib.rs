#[tauri::command]
fn launcher_status() -> &'static str {
    "Launcher services ready"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![launcher_status])
        .run(tauri::generate_context!())
        .expect("error while running the launcher");
}
