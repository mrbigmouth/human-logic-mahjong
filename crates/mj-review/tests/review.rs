//! 復盤的整合測試：用 mj-paifu 附的匿名天鳳牌譜。

use mj_review::{Options, review_game, summarize};
use mj_sim::counterfactual::{Metric, Options as CfOptions};
use mj_strategy::find_strategy;
use std::path::PathBuf;
use std::sync::Arc;

fn record() -> mj_paifu::GameRecord {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../mj-paifu/tests/data");
    let (records, errors) = mj_paifu::load_all(&[dir]);
    assert!(errors.is_empty(), "{errors:?}");
    records.into_iter().next().expect("fixture games")
}

/// 復盤一個座位：每個決策的選項編號都有效、相同的決策沒有差異類型、
/// 不同的決策都做了評估（少量世界數）、彙整與逐手結果一致。
#[test]
fn reviews_one_seat() {
    let st: Arc<dyn mj_strategy::framework::Strategy> =
        Arc::from(find_strategy("tenhou-normal-4p-hanchan/hand-reading").unwrap());
    let rec = record();
    let opts = Options {
        seats: vec![1],
        all: false,
        eval: CfOptions {
            samples: 0,
            rollouts: 2,
            max_rollouts: 2,
            metric: Metric::Points,
            determinize: true,
            kinds: Vec::new(),
        },
        evaluate: true,
        mjai: None,
    };
    let r = review_game(&st, &rec, &opts).unwrap();
    assert!(r.decisions.len() > 50, "{}", r.decisions.len());
    let mut differences = 0;
    for d in &r.decisions {
        assert_eq!(d.seat, 1);
        assert!(d.ai.is_none());
        assert!(d.player < d.options.len() && d.strategy < d.options.len());
        assert_eq!(d.difference.is_some(), !d.agrees());
        if !d.agrees() {
            differences += 1;
            if let Some(e) = &d.eval {
                assert_eq!(e.option_ev.len(), d.options.len());
                assert_eq!(e.rollouts, 2);
            }
        } else {
            assert!(d.eval.is_none());
        }
    }
    assert!(differences > 0);
    assert_eq!(
        r.decisions.iter().filter(|d| d.eval.is_some()).count() + r.skipped,
        differences
    );
    let s = summarize(std::slice::from_ref(&r));
    let total: usize = s.by_kind.iter().map(|k| k.decisions).sum();
    assert_eq!(total, r.decisions.len());
    assert_eq!(s.overall.decisions, r.decisions.len());
    assert_eq!(
        s.overall.evaluated,
        r.decisions.iter().filter(|d| d.eval.is_some()).count()
    );
    let typed: usize = s.by_type.iter().map(|t| t.decisions).sum();
    assert_eq!(typed, differences);
}
