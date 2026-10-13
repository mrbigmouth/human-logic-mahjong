//! 讀入的牌譜（M12）：匯出 → 讀入 → 重播；各視角的幀與引擎視角一致；公開手牌；復盤。

use mj_app::{App, LoadedLog, Session, SessionConfig, log_frames, table_mismatch};
use serde_json::json;
use std::path::Path;
use std::time::Duration;

/// 照提示打完整場，回傳 mjai 牌譜。
fn play(seat: u8, seed: u64) -> String {
    let s = Session::start(SessionConfig {
        seed: Some(seed),
        seat,
        opponents: ["greedy".into(), "random".into(), "greedy".into()],
        ..SessionConfig::default()
    })
    .unwrap();
    let mut since = 0;
    loop {
        let u = s.poll(since, Duration::from_secs(10));
        since = u.next;
        if u.game_over.is_some() {
            return s.mjai_log().unwrap();
        }
        if let Some(ask) = u.ask {
            s.act(ask.id, ask.hint.and_then(|h| h.action).unwrap_or(0))
                .unwrap();
        }
    }
}

/// 每個座位、每次詢問時的幀都和引擎視角一致；公開手牌時他家的手牌就是他自己視角的手牌。
fn check_frames(text: &str) {
    let rec = mj_paifu::parse(text, "t").unwrap();
    let r = mj_paifu::replay::replay(&rec, [None, None, None, None], true).unwrap();
    let all: Vec<_> = (0..4)
        .map(|s| log_frames(&r.result.log, s, false))
        .collect();
    for p in &r.points {
        let f = &all[usize::from(p.seat)][p.event_index - 1];
        if let Some(e) = table_mismatch(&f.table, &p.view) {
            panic!("座位 {} 第 {} 次詢問：{e}", p.seat, p.ask_index);
        }
    }
    let open = log_frames(&r.result.log, 0, true);
    for (i, f) in open.iter().enumerate() {
        for (s, frames) in all.iter().enumerate().skip(1) {
            let own = &frames[i].table.seats[s];
            let shown = &f.table.seats[s];
            if !shown.revealed {
                assert_eq!(shown.hand, own.hand, "第 {i} 幀座位 {s} 的手牌");
                assert_eq!(shown.drawn, own.drawn);
            }
        }
    }
}

#[test]
fn exported_log_round_trips() {
    let text = play(1, 21);
    check_frames(&text);
    let log = LoadedLog::parse(&text, "x").unwrap();
    let meta = log.meta.clone().expect("匯出的牌譜附 mj_meta");
    assert_eq!(meta.seed, Some(21));
    assert_eq!(meta.seat, Some(1));
    // 詢問的幀數與對局中相同：每個詢問都在幀的範圍內、依序遞增。
    let f = log.frames(1, false).unwrap();
    assert!(!f.asks.is_empty());
    assert!(
        f.asks
            .windows(2)
            .all(|w| w[0].0 + 1 == w[1].0 && w[0].1 <= w[1].1)
    );
    assert!(f.asks.iter().all(|&(_, e)| e <= f.frames.len()));
}

#[test]
fn tenhou_logs_load_view_and_review() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mj-paifu/tests/data");
    let mut files = mj_paifu::find_files(&[dir]);
    files.truncate(3);
    assert!(!files.is_empty());
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap();
        check_frames(&text);
    }
    // 經由 API：讀入、取幀、復盤（不做反事實評估，只比對選擇）。
    let app = App::new();
    let text = std::fs::read_to_string(&files[0]).unwrap();
    let info = app
        .call("load_log", json!({ "text": text, "name": "dir/abc.xml" }))
        .unwrap();
    // 天鳳牌譜用牌譜本身記載的 ID（沒有時才用檔名）。
    assert!(!info["id"].as_str().unwrap().is_empty());
    assert!(info["hands"].as_u64().unwrap() > 0);
    let frames = app
        .call("log_frames", json!({ "seat": 2, "open": true }))
        .unwrap();
    assert_eq!(
        frames["frames"].as_array().unwrap().len(),
        info["events"].as_u64().unwrap() as usize
    );
    app.call("start_log_review", json!({ "seat": 2, "rollouts": 0 }))
        .unwrap();
    let review = loop {
        let r: serde_json::Value = app.call("log_review", json!({ "seat": 2 })).unwrap();
        if r["state"] == "running" {
            std::thread::sleep(Duration::from_millis(50));
        } else {
            break r;
        }
    };
    assert_eq!(review["state"], "done", "{review}");
    assert!(!review["review"]["decisions"].as_array().unwrap().is_empty());
    // 其他座位還沒復盤。
    assert_eq!(
        app.call("log_review", json!({ "seat": 0 })).unwrap()["state"],
        "idle"
    );
}
