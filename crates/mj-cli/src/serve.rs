//! `mj serve`：練習 App 的網頁版（M9）。`POST /api/<名稱>`（JSON 參數）轉給 [`mj_app::App::call`]，
//! 其他路徑提供前端檔案（`app/` 中 `npm run build` 的輸出）。每個請求一個執行緒（`poll` 是長輪詢）。
//!
//! `dev_status`（M16）只在這裡處理：`boot` = 服務啟動的時間（前端看到變了就知道服務重啟過）；
//! 由 `mj dev` 啟動時（環境變數 `MJ_DEV_STATUS`）附上 `mj dev` 寫的重建狀態。

use mj_app::App;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, Server};

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

/// 依副檔名決定 Content-Type。
fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
    {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// `dev_status`：服務的啟動時間，與 `mj dev` 的重建狀態（有的話）。
fn dev_status(boot: u128) -> serde_json::Value {
    let status = std::env::var_os("MJ_DEV_STATUS").map(|p| {
        std::fs::read_to_string(p)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .unwrap_or(serde_json::Value::Null)
    });
    serde_json::json!({ "dev": status.is_some(), "boot": boot.to_string(), "status": status })
}

/// 處理一個 API 請求：回應 `{ "ok": 結果 }` 或 `{ "error": 訊息 }`。
fn api(app: &App, name: &str, req: &mut Request, boot: u128) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut body = String::new();
    let params = match req.as_reader().read_to_string(&mut body) {
        Ok(_) if body.trim().is_empty() => Ok(serde_json::Value::Null),
        Ok(_) => serde_json::from_str(&body).map_err(|e| format!("JSON 格式錯誤：{e}")),
        Err(e) => Err(e.to_string()),
    };
    let result = if name == "dev_status" {
        Ok(dev_status(boot))
    } else {
        params.and_then(|p| app.call(name, p))
    };
    let (status, out) = match result {
        Ok(v) => (200, serde_json::json!({ "ok": v })),
        Err(e) => (400, serde_json::json!({ "error": e })),
    };
    Response::from_string(out.to_string())
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
}

/// 前端檔案。路徑不能跑出目錄外；找不到的路徑回傳 index.html（單頁應用），
/// 但有副檔名的（牌圖、JS 等）回傳 404，免得瀏覽器把 HTML 當成圖片而顯示成空白。
fn static_file(dir: &Path, url: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let rel = url
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_start_matches('/');
    let rel = Path::new(rel);
    let safe = rel.components().all(|c| matches!(c, Component::Normal(_)));
    let mut path = if safe && !rel.as_os_str().is_empty() {
        dir.join(rel)
    } else {
        dir.join("index.html")
    };
    if !path.is_file() {
        if rel.extension().is_some() {
            return Response::from_string(format!("找不到 {}", rel.display()))
                .with_status_code(404)
                .with_header(header("Content-Type", "text/plain; charset=utf-8"));
        }
        path = dir.join("index.html");
    }
    match std::fs::read(&path) {
        // no-cache：每次都向伺服器確認，前端重新建置後不會用到舊檔或快取的失敗結果。
        Ok(bytes) => Response::from_data(bytes)
            .with_header(header("Content-Type", content_type(&path)))
            .with_header(header("Cache-Control", "no-cache")),
        Err(_) => Response::from_string(format!(
            "找不到前端檔案 {}。先建置前端：cd app && npm install && npm run build（或用 npm run dev 開發）",
            path.display()
        ))
        .with_status_code(404)
        .with_header(header("Content-Type", "text/plain; charset=utf-8")),
    }
}

pub fn run(host: &str, port: u16, dir: &Path, data: &Path) -> Result<(), String> {
    let server = Server::http((host, port)).map_err(|e| format!("無法監聽 {host}:{port}：{e}"))?;
    let app = Arc::new(App::with_data_dir(Some(data.to_path_buf())));
    let boot = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let dir: PathBuf = dir.to_path_buf();
    println!(
        "練習 App：http://localhost:{port}（API：POST /api/<名稱>；前端檔案：{}）",
        dir.display()
    );
    for mut req in server.incoming_requests() {
        let app = app.clone();
        let dir = dir.clone();
        std::thread::spawn(move || {
            let url = req.url().to_string();
            let resp = match (req.method(), url.strip_prefix("/api/")) {
                (Method::Post, Some(name)) => {
                    let name = name.split('?').next().unwrap_or_default().to_string();
                    api(&app, &name, &mut req, boot)
                }
                (_, Some(_)) => Response::from_string("API 只接受 POST")
                    .with_status_code(405)
                    .with_header(header("Content-Type", "text/plain; charset=utf-8")),
                _ => static_file(&dir, &url),
            };
            let _ = req.respond(resp);
        });
    }
    Ok(())
}
