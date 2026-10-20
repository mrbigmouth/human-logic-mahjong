//! 對戰測試平台（M15）：自我對戰、duplicate、場地對戰、預測評估；進度、取消、存檔、對局清單、重打檢視。

use mj_app::App;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

const HR: &str = "tenhou-normal-4p-hanchan/hand-reading";
const PF: &str = "tenhou-normal-4p-hanchan/push-fold";

/// 等測試結束，回傳最後的狀態。
fn wait(app: &App) -> Value {
    loop {
        let s = app.call("test_status", json!(null)).unwrap();
        if s["state"] == "running" {
            std::thread::sleep(Duration::from_millis(50));
        } else {
            return s;
        }
    }
}

fn run(app: &App, req: Value) -> String {
    let id = app.call("start_test", req).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let s = wait(app);
    assert_eq!(s["state"], "done", "{s}");
    assert_eq!(s["id"], id.as_str());
    id
}

/// 測試用的暫存資料目錄（每次清空）。
fn temp_dir(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn selfplay_list_open_and_persist() {
    let dir = temp_dir("testbed-selfplay");
    let app = App::with_data_dir(Some(dir.clone()));
    let id = run(
        &app,
        json!({ "mode": "selfplay", "entrants": ["greedy", "random", "greedy", "random"],
                "count": 8, "seed": 3 }),
    );
    let r = app.call("test_result", json!({ "id": id })).unwrap();
    assert_eq!(r["games"], 8);
    let names = r["names"].as_array().unwrap();
    assert_eq!(names.len(), 2, "同名的座位合併");
    let entrants = r["report"]["entrants"].as_array().unwrap();
    let seats: u64 = entrants.iter().map(|e| e["games"].as_u64().unwrap()).sum();
    assert_eq!(seats, 32);
    for e in entrants {
        let dist: u64 = e["ranks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap())
            .sum();
        assert_eq!(dist, e["games"].as_u64().unwrap());
    }
    // 對局清單：依 greedy 的名次點由高到低。
    let page = app
        .call(
            "test_games",
            json!({ "id": id, "sort": "item:0", "desc": true, "limit": 5 }),
        )
        .unwrap();
    assert_eq!(page["total"], 8);
    let rows = page["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 5);
    let avg = |row: &Value| {
        let v: Vec<f64> = row["seats"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["item"] == 0)
            .map(|s| s["points"].as_f64().unwrap())
            .collect();
        v.iter().sum::<f64>() / v.len() as f64
    };
    assert!(rows.windows(2).all(|w| avg(&w[0]) >= avg(&w[1])));
    // 打開一場：重打並讀入，之後就是一般的牌譜（檢視、復盤）。
    let index = rows[0]["index"].as_u64().unwrap();
    let info = app
        .call("open_test_game", json!({ "id": id, "index": index }))
        .unwrap();
    let scores: Vec<i64> = rows[0]["seats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["score"].as_i64().unwrap())
        .collect();
    let finals: Vec<i64> = info["final_scores"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_i64().unwrap())
        .collect();
    assert_eq!(finals, scores);
    let frames = app
        .call("log_frames", json!({ "seat": 0, "open": true }))
        .unwrap();
    assert!(!frames["frames"].as_array().unwrap().is_empty());

    // 存檔：另一個 App（重開）看得到，也能刪除。
    let again = App::with_data_dir(Some(dir));
    let list = again.call("tests", json!(null)).unwrap();
    assert_eq!(list[0]["id"], id.as_str());
    assert!(again.call("test_result", json!({ "id": id })).is_ok());
    again.call("delete_test", json!({ "id": id })).unwrap();
    assert!(
        again
            .call("tests", json!(null))
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn duplicate_field_and_cancel() {
    let app = App::new();
    // 兩方 duplicate：成對比較。
    let id = run(
        &app,
        json!({ "mode": "duplicate", "entrants": [HR, PF], "count": 2, "seed": 11 }),
    );
    let r = app.call("test_result", json!({ "id": id })).unwrap();
    assert_eq!(r["games"], 8);
    assert_eq!(r["report"]["comparison"]["blocks"], 2);
    // 場地對戰：A、B 與 field 同桌各 4 場。
    let id = run(
        &app,
        json!({ "mode": "field", "entrants": [HR, PF], "field": "greedy", "count": 1, "seed": 4 }),
    );
    let r = app.call("test_result", json!({ "id": id })).unwrap();
    assert_eq!(r["games"], 8);
    assert_eq!(r["names"].as_array().unwrap().len(), 4);
    let open = app.call("open_test_game", json!({ "id": id, "index": 5 }));
    assert!(open.is_ok(), "{open:?}");
    // 取消：以完成的對局計算。
    app.call(
        "start_test",
        json!({ "mode": "selfplay", "entrants": [HR, HR, HR, HR], "count": 5000 }),
    )
    .unwrap();
    // 同時只能有一個。
    assert!(
        app.call(
            "start_test",
            json!({ "mode": "selfplay", "entrants": ["greedy", "greedy", "greedy", "greedy"], "count": 1 })
        )
        .is_err()
    );
    std::thread::sleep(Duration::from_millis(300));
    app.call("cancel_test", json!(null)).unwrap();
    let s = wait(&app);
    assert_eq!(s["state"], "done", "{s}");
    let r = app.call("test_result", json!({ "id": s["id"] })).unwrap();
    assert_eq!(r["summary"]["cancelled"], true);
    let done = r["summary"]["games_done"].as_u64().unwrap();
    assert!(done < 5000);
    assert_eq!(r["games"].as_u64().unwrap(), done);
}

#[test]
fn predict_eval_rows() {
    let app = App::new();
    let id = run(
        &app,
        json!({ "mode": "predict", "entrants": [HR, PF, HR, PF], "count": 2, "seed": 8 }),
    );
    let r = app.call("test_result", json!({ "id": id })).unwrap();
    let rows = r["predict"].as_array().unwrap();
    assert!(!rows.is_empty());
    // 每組預測都有「全部」與兩個被預測者。
    assert!(rows.iter().any(|x| x["target"] == "全部"));
    assert!(rows.iter().any(|x| x["target"] == HR));
    assert!(rows.iter().all(|x| x["metrics"]["n"].as_u64().unwrap() > 0));
    // 預測評估只能用策略。
    assert!(
        app.call(
            "start_test",
            json!({ "mode": "predict", "entrants": [HR, "greedy", HR, HR], "count": 1 })
        )
        .is_err()
    );
}

/// 有 Mortal 的對局：保存牌譜，打開時讀保存的牌譜（Mortal 未安裝時略過）。
#[test]
fn mortal_games_store_logs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if !root.join("tools/mortal/.venv").is_dir() {
        eprintln!("Mortal 未安裝，略過");
        return;
    }
    let run_sh = root.join("tools/mortal/run.sh").canonicalize().unwrap();
    let mortal = format!("mjai:{} s42", run_sh.display());
    let app = App::with_data_dir(Some(temp_dir("testbed-mortal")));
    let id = run(
        &app,
        json!({ "mode": "duplicate", "entrants": [HR, mortal], "count": 1, "seed": 5 }),
    );
    let page = app
        .call("test_games", json!({ "id": id, "limit": 10 }))
        .unwrap();
    let rows = page["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|r| r["stored"] == true));
    let info = app
        .call("open_test_game", json!({ "id": id, "index": 2 }))
        .unwrap();
    assert!(
        info["names"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n == mortal.as_str())
    );
}
