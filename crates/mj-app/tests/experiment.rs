//! 反事實實驗（M14）：對局中、讀入的牌譜、取消。

use mj_app::App;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

fn wait(app: &App) -> Value {
    loop {
        let s = app.call("experiment", json!(null)).unwrap();
        if s["state"] == "running" {
            std::thread::sleep(Duration::from_millis(50));
        } else {
            return s;
        }
    }
}

/// 檢查結果：每個要求的選項都有一筆；基準選項的差值為 0；比率在 0–1。
fn check(done: &Value, options: usize, reference: usize) {
    assert_eq!(done["state"], "done", "{done}");
    let results = done["results"].as_array().unwrap();
    assert_eq!(results.len(), options);
    for r in results {
        let rate = |k: &str| r[k].as_f64().unwrap();
        assert!((0.0..=1.0).contains(&rate("win_rate")));
        assert!((0.0..=1.0).contains(&rate("deal_in_rate")));
        if r["option"] == reference {
            assert_eq!(rate("diff"), 0.0);
        }
    }
}

/// 對局中：等到要何切，對提示的何切決策做實驗（照動作紀錄重播到這一刻）。
#[test]
fn experiment_in_a_live_game() {
    let app = App::new();
    app.call(
        "new_game",
        json!({ "seed": 31, "seat": 2, "opponents": ["greedy", "random", "greedy"] }),
    )
    .unwrap();
    let mut since = 0;
    let mut tested = 0;
    while tested < 2 {
        let u = app
            .call("poll", json!({ "since": since, "wait_ms": 5000 }))
            .unwrap();
        since = u["next"].as_u64().unwrap();
        let Some(ask) = u["ask"].as_object() else {
            assert!(u["game_over"].is_null(), "對局結束前應該有可以實驗的何切");
            continue;
        };
        let hint = &ask["hint"];
        let id = ask["id"].as_u64().unwrap();
        // 第 3 次以後的何切（有一些歷史）才做實驗；做兩次。
        if let Some(d) = hint["advisors"][0]["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["kind_key"] == "Discard")
            && id > 3
        {
            let n = d["options"].as_array().unwrap().len();
            let chosen = d["chosen"].as_u64().unwrap() as usize;
            let other = (chosen + 1) % n;
            app.call(
                "start_experiment",
                json!({
                    "source": "game", "ask": id, "kind": "Discard", "option_count": n,
                    "options": [other], "reference": chosen, "worlds": 6, "metric": "points",
                }),
            )
            .unwrap();
            check(&wait(&app), 2, chosen);
            tested += 1;
        }
        let i = hint["action"].as_u64().unwrap_or(0);
        app.call("act", json!({ "ask": id, "index": i })).unwrap();
    }
}

/// 讀入的牌譜：對復盤列出的第一個何切做實驗（名次點，打到整場結束）；然後測試取消。
#[test]
fn experiment_on_a_loaded_log() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mj-paifu/tests/data");
    let file = mj_paifu::find_files(&[dir]).remove(0);
    let app = App::new();
    app.call(
        "load_log",
        json!({ "text": std::fs::read_to_string(file).unwrap(), "name": "x" }),
    )
    .unwrap();
    app.call("start_log_review", json!({ "seat": 1, "rollouts": 0 }))
        .unwrap();
    let review = loop {
        let r = app.call("log_review", json!({ "seat": 1 })).unwrap();
        if r["state"] != "running" {
            break r;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let d = review["review"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["kind"] == "Discard" && d["options"].as_array().unwrap().len() > 2)
        .unwrap()
        .clone();
    let ask = d["act_index"].as_u64().unwrap() + 1;
    let n = d["options"].as_array().unwrap().len();
    let reference = d["strategy"].as_u64().unwrap() as usize;
    let req = json!({
        "source": "log", "seat": 1, "ask": ask, "kind": "Discard", "option_count": n,
        "reference": reference, "worlds": 4, "metric": "placement",
    });
    app.call("start_experiment", req.clone()).unwrap();
    let done = wait(&app);
    check(&done, n, reference);
    assert!(done["results"][0]["avg_rank"].as_f64().is_some());

    // 取消：大量的世界，馬上取消 → 很快結束（取消的結果或「沒有完成任何世界」）。
    let mut big = req;
    big["worlds"] = json!(1000);
    big["metric"] = json!("points");
    app.call("start_experiment", big).unwrap();
    app.call("cancel_experiment", json!(null)).unwrap();
    let s = wait(&app);
    match s["state"].as_str().unwrap() {
        "done" => {
            assert_eq!(s["cancelled"], true);
            assert!(s["worlds"].as_u64().unwrap() < 1000);
        }
        "failed" => assert!(s["error"].as_str().unwrap().contains("沒有完成")),
        other => panic!("{other}"),
    }
}
