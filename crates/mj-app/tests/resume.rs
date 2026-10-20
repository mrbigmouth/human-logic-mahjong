//! 接續對局與參數版本（M16）：服務重啟後照保存的動作回到同一個詢問；參數版本列在參賽者中、可以上桌。

use mj_app::App;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn temp_dir(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// 等到下一個詢問，回傳（詢問, 到目前為止的全部幀）。
fn next_ask(app: &App, frames: &mut Vec<Value>) -> Value {
    loop {
        let u = app
            .call("poll", json!({ "since": frames.len(), "wait_ms": 10000 }))
            .unwrap();
        frames.extend(u["frames"].as_array().unwrap().iter().cloned());
        assert!(u["game_over"].is_null(), "對局不應該這麼快結束");
        if !u["ask"].is_null() {
            return u["ask"].clone();
        }
    }
}

#[test]
fn resume_after_restart() {
    let dir = temp_dir("resume");
    let cfg = json!({ "seed": 123, "seat": 2, "opponents": ["tenhou-normal-4p-hanchan/hand-reading", "greedy", "random"] });
    let (ask, frames) = {
        let app = App::with_data_dir(Some(dir.clone()));
        app.call("new_game", cfg).unwrap();
        let mut frames = Vec::new();
        // 照提示回答 12 次，停在第 13 次詢問（不回答）。
        for _ in 0..12 {
            let ask = next_ask(&app, &mut frames);
            app.call(
                "act",
                json!({ "ask": ask["id"], "index": ask["hint"]["action"] }),
            )
            .unwrap();
        }
        let ask = next_ask(&app, &mut frames);
        (ask, frames)
        // App 在這裡被丟棄（模擬服務重啟）
    };
    let app = App::with_data_dir(Some(dir));
    let saved = app.call("saved_game", json!(null)).unwrap();
    assert_eq!(saved["seed"], 123);
    assert_eq!(saved["asks"], 12);
    let g = app.call("resume_game", json!(null)).unwrap();
    assert_eq!(g["resumed"], true);
    let mut again = Vec::new();
    let ask2 = next_ask(&app, &mut again);
    // 同一個詢問：編號、幀數、牌桌、合法動作都相同。
    assert_eq!(ask2["id"], ask["id"]);
    assert_eq!(again.len(), frames.len());
    assert_eq!(again.last(), frames.last());
    assert_eq!(ask2["actions"], ask["actions"]);
    // 接續的對局也能做實驗（重播的動作紀錄接得上）。
    let d = ask2["hint"]["advisors"][0]["decisions"][0].clone();
    app.call(
        "start_experiment",
        json!({ "source": "game", "ask": ask2["id"], "kind": d["kind_key"],
                "option_count": d["options"].as_array().unwrap().len(),
                "reference": d["chosen"], "worlds": 4 }),
    )
    .unwrap();
    let s = loop {
        let s = app.call("experiment", json!(null)).unwrap();
        if s["state"] != "running" {
            break s;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(s["state"], "done", "{s}");
}

#[test]
fn param_variants_are_entrants() {
    let app = App::with_data_dir(Some(temp_dir("variants")));
    let hr = "tenhou-normal-4p-hanchan/hand-reading";
    let p = app.call("params", json!({ "strategy": hr })).unwrap();
    let first = &p["params"][0];
    let name = first["name"].as_str().unwrap();
    let v = app
        .call(
            "save_params",
            json!({ "strategy": hr, "variant": "快速", "values": { name: first["max"] } }),
        )
        .unwrap();
    let spec = v["spec"].as_str().unwrap().to_string();
    let entrants = app.call("entrants", json!(null)).unwrap();
    let e = entrants
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == spec.as_str())
        .expect("參數版本列在參賽者中");
    assert_eq!(e["variant"], "快速");
    assert!(e["title"].as_str().unwrap().ends_with("（@快速）"));
    // 可以當對手、顧問與測試的參賽者。
    app.call(
        "new_game",
        json!({ "seed": 1, "seat": 0, "opponents": [spec, "greedy", "greedy"], "advisors": [spec] }),
    )
    .unwrap();
    app.call(
        "start_test",
        json!({ "mode": "duplicate", "entrants": [spec, hr], "count": 1, "seed": 2 }),
    )
    .unwrap();
    let s = loop {
        let s = app.call("test_status", json!(null)).unwrap();
        if s["state"] != "running" {
            break s;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(s["state"], "done", "{s}");
    app.call(
        "delete_params",
        json!({ "strategy": hr, "variant": "快速" }),
    )
    .unwrap();
    assert!(
        app.call("params", json!({ "strategy": hr })).unwrap()["variants"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
