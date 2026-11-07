mod native_exports;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|_app| {
            std::thread::spawn(|| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(rom_core::start_api());
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, native_exports::save_generated_file])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
