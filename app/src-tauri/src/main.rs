//! 桌面版練習 App（M9）：前端與 `mj serve` 相同，只是 API 改走 Tauri command。
//! 唯一的 command `api(name, args)` 轉給 [`mj_app::App::call`]（`poll` 是長輪詢，所以在背景執行緒執行）。

// Windows 的 release 版不開主控台視窗。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use mj_app::App;
use serde_json::Value;
use std::sync::Arc;

#[tauri::command]
async fn api(
    state: tauri::State<'_, Arc<App>>,
    name: String,
    args: Value,
) -> Result<Value, String> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || app.call(&name, args))
        .await
        .map_err(|e| e.to_string())?
}

fn main() {
    tauri::Builder::default()
        .manage(Arc::new(App::new()))
        .invoke_handler(tauri::generate_handler![api])
        .run(tauri::generate_context!())
        .expect("無法啟動 App");
}
