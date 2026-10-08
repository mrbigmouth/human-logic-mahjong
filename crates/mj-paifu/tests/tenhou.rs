//! 以天鳳牌譜驗證匯入：`tests/data/` 的匿名化牌譜（雙響、加槓、暗槓、大明槓、流局滿貫、九種九牌、
//! 東風戰、西入；以及曾經與引擎不一致的兩場：南四局莊家第一名時九種九牌、立直者的聽牌張被暗槓），加上 `data/tenhou/` 中下載的牌譜（由 `tools/fetch-tenhou.sh` 下載，沒有時略過）。

use mj_paifu::{load, replay::replay};
use std::path::PathBuf;

fn files(ext: &str) -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut v: Vec<PathBuf> = [root.join("tests/data"), root.join("../../data/tenhou")]
        .iter()
        .flat_map(|dir| {
            std::fs::read_dir(dir)
                .map(|d| d.flatten().map(|e| e.path()).collect::<Vec<_>>())
                .unwrap_or_default()
        })
        .collect();
    v.retain(|p| p.extension().is_some_and(|e| e == ext));
    v.sort();
    assert!(v.len() >= 5, "測試牌譜不足");
    v
}

/// 每個牌譜都能重播，且局數、點數與原牌譜完全一致。
#[test]
fn downloaded_logs_replay_exactly() {
    let mut failures = Vec::new();
    let mut n = 0;
    for ext in ["xml", "json"] {
        for f in files(ext) {
            n += 1;
            let r = load(&f).and_then(|rec| replay(&rec, [None, None, None, None], false));
            if let Err(e) = r {
                failures.push(format!("{}: {e}", f.file_name().unwrap().to_string_lossy()));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{}/{n} 失敗：\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// 同一場對局的 XML 與 JSON 解析出相同的內容。
#[test]
fn xml_and_json_agree() {
    for x in files("xml") {
        let j = x.with_extension("json");
        if !j.exists() {
            continue;
        }
        let (a, b) = (load(&x).unwrap(), load(&j).unwrap());
        assert_eq!(a.players, b.players, "{}", x.display());
        assert_eq!(a.final_scores, b.final_scores, "{}", x.display());
        assert_eq!(a.hands.len(), b.hands.len(), "{}", x.display());
        for (i, (ha, hb)) in a.hands.iter().zip(&b.hands).enumerate() {
            // JSON 的配牌已排序，XML 依發牌順序；只比較內容。
            let sorted = |h: &[Vec<mj_core::Pai>; 4]| {
                h.clone().map(|mut v| {
                    v.sort();
                    v
                })
            };
            assert_eq!(
                sorted(&ha.haipai),
                sorted(&hb.haipai),
                "{} 第 {i} 局",
                x.display()
            );
            assert_eq!(
                ha.dora_indicators,
                hb.dora_indicators,
                "{} 第 {i} 局",
                x.display()
            );
            assert_eq!(ha.steps, hb.steps, "{} 第 {i} 局", x.display());
        }
    }
}

/// 每個決策點重建的視角都合理：選擇在合法動作中、自己回合的手牌張數正確、
/// 回應時看得到被回應的牌，而且決策點數 = 引擎詢問次數。
#[test]
fn decision_points_rebuild_views() {
    use mj_engine::Action;
    for f in files("xml").into_iter().take(10) {
        let rec = load(&f).unwrap();
        let r = replay(&rec, [None, None, None, None], true).unwrap();
        assert!(!r.points.is_empty());
        let mut asks = [0u32; 4];
        for p in &r.points {
            assert!(p.legal.contains(&p.chosen), "{}: {p:?}", f.display());
            assert_eq!(p.ask_index, asks[usize::from(p.seat)]);
            asks[usize::from(p.seat)] += 1;
            let v = &p.view;
            assert_eq!(v.me, p.seat);
            let melds = v.melds[usize::from(p.seat)].len() as u32;
            match v.last_discard {
                None => assert_eq!(v.hand.total() + 3 * melds, 14, "{}: {v:?}", f.display()),
                Some(_) => {
                    assert_eq!(v.hand.total() + 3 * melds, 13);
                    assert!(p.legal.contains(&Action::Pass) || p.legal.contains(&Action::Ron));
                }
            }
        }
    }
}
