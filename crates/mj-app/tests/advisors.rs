//! 顧問（M14.5）：提示同時列出多個策略／AI 的選擇；復盤以第一個顧問為比較基準。
//! Mortal 只在已安裝時（`tools/mortal/setup.sh`）測試。

use mj_app::App;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

const HR: &str = "tenhou-normal-4p-hanchan/hand-reading";
const PF: &str = "tenhou-normal-4p-hanchan/push-fold";

/// Mortal 的參賽者名稱（未安裝時為 `None`）。
fn mortal() -> Option<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let installed = root.join("tools/mortal/.venv").is_dir()
        && root
            .join("data/mortal/s42/weights/s42-inference.pth")
            .is_file();
    installed.then(|| {
        let run = root.join("tools/mortal/run.sh").canonicalize().unwrap();
        format!("mjai:{} s42", run.display())
    })
}

/// 打完一局（照第一個顧問的建議），檢查每次詢問的提示，回傳詢問次數。
fn play(app: &App, advisors: &[String]) -> usize {
    app.call(
        "new_game",
        json!({ "seed": 77, "seat": 1, "opponents": ["greedy", "greedy", "greedy"],
                "advisors": advisors, "max_hands": 1 }),
    )
    .unwrap();
    let mut since = 0;
    let mut asks = 0;
    loop {
        let u = app
            .call("poll", json!({ "since": since, "wait_ms": 10000 }))
            .unwrap();
        since = u["next"].as_u64().unwrap();
        assert!(u["error"].is_null(), "{}", u["error"]);
        if !u["game_over"].is_null() {
            return asks;
        }
        let Some(ask) = u["ask"].as_object() else {
            continue;
        };
        asks += 1;
        let hint = &ask["hint"];
        let list = hint["advisors"].as_array().unwrap();
        assert_eq!(list.len(), advisors.len());
        assert_eq!(hint["action"], list[0]["action"]);
        let n_actions = ask["actions"].as_array().unwrap().len() as u64;
        for (k, a) in list.iter().enumerate() {
            assert_eq!(a["name"], advisors[k].as_str());
            let i = a["action"].as_u64().expect("每個顧問都有選擇");
            assert!(i < n_actions);
            for d in a["decisions"].as_array().unwrap() {
                let picks = d["picks"].as_array().unwrap();
                assert_eq!(picks.len(), advisors.len());
                // 自己在自己的決策的選擇就是 chosen。
                assert_eq!(picks[k], d["chosen"]);
            }
            if a["ai"] == true {
                // AI：只有機率，沒有理由。
                let p = &a["policy"];
                if n_actions > 1 {
                    let probs = p["probs"].as_array().expect("有 Q 值時有機率");
                    let total: f64 = probs.iter().map(|x| x["prob"].as_f64().unwrap()).sum();
                    assert!((total - 1.0).abs() < 1e-6, "{total}");
                }
                for d in a["decisions"].as_array().unwrap() {
                    assert!(d["notes"].as_array().unwrap().is_empty());
                }
            }
        }
        let i = hint["action"].as_u64().unwrap();
        app.call("act", json!({ "ask": ask["id"], "index": i }))
            .unwrap();
    }
}

/// 剛打完的對局：讀入後復盤（牌譜記載的顧問）。
fn review(app: &App) -> Value {
    let text = app.call("log", json!(null)).unwrap();
    let info = app
        .call("load_log", json!({ "text": text, "name": "s.mjson" }))
        .unwrap();
    assert!(!info["meta"]["advisors"].as_array().unwrap().is_empty());
    app.call("start_log_review", json!({ "seat": 1, "rollouts": 0 }))
        .unwrap();
    loop {
        let r = app.call("log_review", json!({ "seat": 1 })).unwrap();
        if r["state"] == "running" {
            std::thread::sleep(Duration::from_millis(50));
        } else {
            assert_eq!(r["state"], "done", "{r}");
            return r;
        }
    }
}

#[test]
fn two_strategies() {
    let app = App::new();
    let advisors = [PF.to_string(), HR.to_string()];
    assert!(play(&app, &advisors) > 0);
    let r = review(&app);
    assert_eq!(r["advisors"].as_array().unwrap().len(), 2);
    let ds = r["review"]["decisions"].as_array().unwrap();
    for d in ds {
        // 第一個顧問是比較基準。
        assert_eq!(d["picks"][0], d["strategy"]);
    }
    // 基礎策略（hand-reading）在顧問中：有理由。
    assert!(
        ds.iter()
            .any(|d| !d["notes"].as_array().unwrap().is_empty())
    );
}

#[test]
fn with_mortal() {
    let Some(m) = mortal() else {
        eprintln!("Mortal 未安裝，略過");
        return;
    };
    let app = App::new();
    // Mortal 是第一個：提示標示 Mortal 的選擇，復盤以 Mortal 為基準。
    let advisors = [m.clone(), HR.to_string()];
    assert!(play(&app, &advisors) > 0);
    let r = review(&app);
    let ds = r["review"]["decisions"].as_array().unwrap();
    assert!(!ds.is_empty());
    for d in ds {
        assert_eq!(d["picks"][0], d["strategy"]);
        assert!(!d["policies"][0].is_null() || d["options"].as_array().unwrap().len() < 2);
    }
    // 只有 Mortal：決策結構與重打用預設策略，但不顯示它的理由。
    let only = [m];
    assert!(play(&app, &only) > 0);
    let r = review(&app);
    for d in r["review"]["decisions"].as_array().unwrap() {
        assert!(d["notes"].as_array().unwrap().is_empty());
        assert_eq!(d["picks"][0], d["strategy"]);
    }
}

#[test]
fn rejects_bad_advisor_lists() {
    let app = App::new();
    for bad in [
        json!([]),
        json!([HR, HR]),
        json!(["greedy"]),
        json!([HR, PF, HR, PF]),
    ] {
        let r = app.call("new_game", json!({ "advisors": bad }));
        // 空清單 = 使用 strategy（預設策略），其他都是錯誤。
        if bad.as_array().unwrap().is_empty() {
            assert!(r.is_ok());
        } else {
            assert!(r.is_err(), "{bad}");
        }
    }
}
