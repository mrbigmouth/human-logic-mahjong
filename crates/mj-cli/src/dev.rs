//! `mj dev`：策略編輯的開發模式（M16 B）。啟動 `mj serve`，監看原始碼；存檔後自動重建、重啟服務，
//! 前端偵測到重啟後自動重新整理並回到原本的畫面（進行中的對局照保存的動作接續）。
//!
//! - 監看 `crates/` 的 `.rs`、`Cargo.toml`、`.md`（策略的 README 會嵌入執行檔）與根目錄的 `Cargo.toml`：
//!   `cargo build --profile <建置設定> -p mj-cli` → 成功就重啟服務，失敗時舊的服務繼續跑，錯誤顯示在 GUI；
//! - 監看 `app/src`、`app/public`、`app/index.html`：`npm run build`（前端不需要重啟服務，只要重新整理）；
//! - 狀態寫在 `<資料目錄>/dev-status.json`，服務的 `dev_status` API 讀它給前端（見 `serve.rs`）。
//!
//! 預設的建置設定 `dev-fast`（Cargo.toml）：不做 LTO、開啟增量編譯，改策略後約 2 秒重建完（release 約 13 秒），
//! 執行速度約慢 15%。

use serde_json::json;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

/// `mj dev` 的設定。
pub struct Options {
    pub port: u16,
    pub host: String,
    pub dir: PathBuf,
    pub data: PathBuf,
    pub profile: String,
    /// 不監看前端（沒有安裝 Node.js 時）
    pub no_frontend: bool,
}

/// 開發狀態（寫給前端看）。
struct Status {
    path: PathBuf,
    /// 每次重建成功（服務或前端）加一；前端看到變了就重新整理
    version: u64,
}

impl Status {
    fn write(&self, state: &str, target: &str, changed: &[String], extra: serde_json::Value) {
        let mut v = json!({
            "state": state,
            "target": target,
            "changed": changed,
            "version": self.version,
            "time": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
        });
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        let tmp = self.path.with_extension("json.tmp");
        if std::fs::write(&tmp, v.to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }
}

/// 監看的檔案與修改時間。
fn scan(roots: &[PathBuf], exts: &[&str], out: &mut BTreeMap<PathBuf, SystemTime>) {
    for root in roots {
        if root.is_file() {
            if let Ok(t) = std::fs::metadata(root).and_then(|m| m.modified()) {
                out.insert(root.clone(), t);
            }
            continue;
        }
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            if p.is_dir() {
                scan(&[p], exts, out);
            } else if exts.is_empty() || exts.iter().any(|x| name.ends_with(x)) {
                if let Ok(t) = e.metadata().and_then(|m| m.modified()) {
                    out.insert(p, t);
                }
            }
        }
    }
}

fn changed(
    old: &BTreeMap<PathBuf, SystemTime>,
    new: &BTreeMap<PathBuf, SystemTime>,
) -> Vec<String> {
    let mut v: Vec<String> = new
        .iter()
        .filter(|(p, t)| old.get(*p) != Some(t))
        .map(|(p, _)| p.display().to_string())
        .collect();
    v.extend(
        old.keys()
            .filter(|p| !new.contains_key(*p))
            .map(|p| format!("{}（刪除）", p.display())),
    );
    v
}

/// 執行一個指令，輸出同時印在終端機，回傳（是否成功, 輸出的最後部分）。
fn run_logged(mut cmd: Command) -> (bool, String) {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return (false, format!("無法執行：{e}")),
    };
    let err = child.stderr.take().expect("piped");
    let out = child.stdout.take().expect("piped");
    // 兩個管道分別讀，避免其中一個塞滿時卡住。
    let t = std::thread::spawn(move || {
        let mut lines = Vec::new();
        for l in BufReader::new(out).lines().map_while(Result::ok) {
            println!("{l}");
            lines.push(l);
        }
        lines
    });
    let mut lines = Vec::new();
    for l in BufReader::new(err).lines().map_while(Result::ok) {
        eprintln!("{l}");
        lines.push(l);
    }
    lines.extend(t.join().unwrap_or_default());
    let ok = child.wait().is_ok_and(|s| s.success());
    // 只留最後 300 行（編譯錯誤通常在最後）。
    let tail = lines.len().saturating_sub(300);
    (ok, lines[tail..].join("\n"))
}

fn cargo_build(profile: &str) -> (bool, String) {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "build",
        "--profile",
        profile,
        "-p",
        "mj-cli",
        "--color",
        "never",
    ]);
    run_logged(cmd)
}

fn npm_build() -> (bool, String) {
    let mut cmd = Command::new("npm");
    cmd.args(["run", "build"]).current_dir("app");
    run_logged(cmd)
}

fn binary(profile: &str) -> PathBuf {
    let dir = if profile == "dev" { "debug" } else { profile };
    Path::new("target")
        .join(dir)
        .join(format!("mj{}", std::env::consts::EXE_SUFFIX))
}

/// 啟動服務。執行的是建置結果的複本（`<資料目錄>/dev-bin/`）：Windows 不能覆寫執行中的檔案，
/// 直接執行建置結果的話，下一次建置會失敗。
fn spawn_server(o: &Options, status: &Path, version: u64) -> Result<Child, String> {
    let built = binary(&o.profile);
    let dir = o.data.join("dev-bin");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let bin = dir.join(format!("mj-{version}{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(&built, &bin).map_err(|e| format!("無法複製 {}：{e}", built.display()))?;
    Command::new(&bin)
        .args(["serve", "--port", &o.port.to_string(), "--host", &o.host])
        .arg("--dir")
        .arg(&o.dir)
        .arg("--data")
        .arg(&o.data)
        .env("MJ_DEV_STATUS", status)
        .spawn()
        .map_err(|e| format!("無法啟動 {}：{e}", bin.display()))
}

/// 等服務開始接受連線（最多幾秒）。
fn wait_ready(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn run(o: &Options) -> Result<(), String> {
    if !Path::new("crates/mj-strategy/src/strategy-list").is_dir() {
        return Err(
            "mj dev 要在專案根目錄執行（找不到 crates/mj-strategy/src/strategy-list）".into(),
        );
    }
    std::fs::create_dir_all(&o.data).map_err(|e| e.to_string())?;
    let status_path = o.data.join("dev-status.json");
    let mut status = Status {
        path: status_path.clone(),
        version: 0,
    };
    let frontend = !o.no_frontend && Path::new("app/node_modules").is_dir();
    let rust_roots = vec![PathBuf::from("crates"), PathBuf::from("Cargo.toml")];
    let rust_exts = [".rs", "Cargo.toml", ".md"];
    let web_roots = vec![
        PathBuf::from("app/src"),
        PathBuf::from("app/public"),
        PathBuf::from("app/index.html"),
    ];

    println!("mj dev：建置中（{}）…", o.profile);
    status.write("building", "rust", &[], json!({}));
    let t = Instant::now();
    let (ok, log) = cargo_build(&o.profile);
    if !ok && !binary(&o.profile).is_file() {
        return Err(format!("第一次建置失敗：\n{log}"));
    }
    if frontend && !o.dir.join("index.html").is_file() {
        npm_build();
    }
    let mut child = spawn_server(o, &status_path, 0)?;
    wait_ready(o.port);
    if ok {
        status.write(
            "ready",
            "rust",
            &[],
            json!({ "build_secs": t.elapsed().as_secs_f64() }),
        );
    } else {
        status.write("failed", "rust", &[], json!({ "error": log }));
    }
    println!(
        "mj dev：http://localhost:{}　監看 crates/{}；存檔後自動重建（Ctrl-C 結束）",
        o.port,
        if frontend { " 與 app/" } else { "" }
    );

    let mut rust_files = BTreeMap::new();
    scan(&rust_roots, &rust_exts, &mut rust_files);
    let mut web_files = BTreeMap::new();
    if frontend {
        scan(&web_roots, &[], &mut web_files);
    }
    loop {
        std::thread::sleep(Duration::from_millis(400));
        if let Ok(Some(s)) = child.try_wait() {
            // 服務自己結束了（例如 port 被占用）：重新啟動。
            eprintln!("mj dev：服務結束（{s}），重新啟動");
            child = spawn_server(o, &status_path, status.version)?;
            wait_ready(o.port);
        }
        let mut now = BTreeMap::new();
        scan(&rust_roots, &rust_exts, &mut now);
        let mut rust_changed = changed(&rust_files, &now);
        let mut web_now = BTreeMap::new();
        if frontend {
            scan(&web_roots, &[], &mut web_now);
        }
        let web_changed = changed(&web_files, &web_now);
        if rust_changed.is_empty() && web_changed.is_empty() {
            continue;
        }
        // 等存檔告一段落（編輯器可能分幾次寫入）。
        std::thread::sleep(Duration::from_millis(300));
        let mut again = BTreeMap::new();
        scan(&rust_roots, &rust_exts, &mut again);
        rust_changed.extend(changed(&now, &again));
        rust_changed.sort();
        rust_changed.dedup();
        rust_files = again;
        web_files = web_now;

        if !rust_changed.is_empty() {
            println!("mj dev：變更 {}，重建中…", rust_changed.join("、"));
            status.write(
                "building",
                "rust",
                &rust_changed,
                json!({ "started": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs()) }),
            );
            let t = Instant::now();
            let (ok, log) = cargo_build(&o.profile);
            let secs = t.elapsed().as_secs_f64();
            if !ok {
                println!("mj dev：建置失敗（{secs:.1} 秒），服務維持舊版");
                status.write(
                    "failed",
                    "rust",
                    &rust_changed,
                    json!({ "error": log, "build_secs": secs }),
                );
                continue;
            }
            println!("mj dev：建置完成（{secs:.1} 秒），重新啟動服務");
            let _ = child.kill();
            let _ = child.wait();
            status.version += 1;
            child = spawn_server(o, &status_path, status.version)?;
            wait_ready(o.port);
            status.write(
                "ready",
                "rust",
                &rust_changed,
                json!({ "build_secs": secs }),
            );
        }
        if !web_changed.is_empty() {
            println!("mj dev：前端變更，npm run build…");
            status.write("building", "frontend", &web_changed, json!({}));
            let t = Instant::now();
            let (ok, log) = npm_build();
            let secs = t.elapsed().as_secs_f64();
            if ok {
                status.version += 1;
                status.write(
                    "ready",
                    "frontend",
                    &web_changed,
                    json!({ "build_secs": secs }),
                );
            } else {
                status.write(
                    "failed",
                    "frontend",
                    &web_changed,
                    json!({ "error": log, "build_secs": secs }),
                );
            }
        }
    }
}
