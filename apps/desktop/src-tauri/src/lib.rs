#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = spielorders_core::CRATE_NAME;
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running SpielOrders");
}
