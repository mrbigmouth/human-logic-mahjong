//! 對局 session：用測試執行緒扮演人類（照提示打），打完整場並復盤。

use mj_app::{ReviewConfig, ReviewState, Session, SessionConfig};
use std::time::Duration;

/// 扮演人類打完整場：照提示的動作打（沒有提示時選第一個）。回傳收到的幀數。
fn play_out(s: &Session) -> usize {
    let mut since = 0;
    let mut asks = 0;
    loop {
        let u = s.poll(since, Duration::from_secs(10));
        since = u.next;
        if let Some(e) = u.error {
            panic!("{e}");
        }
        if u.game_over.is_some() {
            assert!(asks > 0);
            return since;
        }
        if let Some(ask) = u.ask {
            asks += 1;
            let hint = ask.hint.expect("每次詢問都有提示");
            let i = hint.action.expect("提示的動作是合法動作之一");
            // 過期的詢問編號會被拒絕。
            assert!(s.act(ask.id + 1, i).is_err());
            s.act(ask.id, i).unwrap();
        }
    }
}

#[test]
fn plays_a_full_game_with_hints_and_reviews_it() {
    for seat in [0, 2] {
        let s = Session::start(SessionConfig {
            seed: Some(11 + u64::from(seat)),
            seat,
            opponents: ["greedy".into(), "random".into(), "greedy".into()],
            ..SessionConfig::default()
        })
        .unwrap();
        let frames = play_out(&s);
        assert!(frames > 100);
        let all = s.poll(0, Duration::ZERO);
        assert_eq!(all.frames.len(), frames);
        let last = &all.frames.last().unwrap().table;
        assert_eq!(last.scores.iter().sum::<i32>(), 100_000);
        assert!(s.mjai_log().unwrap().lines().count() > 100);

        s.start_review(&ReviewConfig {
            rollouts: 0,
            ..ReviewConfig::default()
        })
        .unwrap();
        let review = loop {
            match s.review() {
                ReviewState::Running { .. } => std::thread::sleep(Duration::from_millis(50)),
                r => break r,
            }
        };
        match review {
            ReviewState::Done { review, .. } => {
                // 照提示打，所以每個決策都與策略一致。
                assert!(!review.decisions.is_empty());
                assert!(review.decisions.iter().all(|d| d.agrees()));
            }
            r => panic!("復盤失敗：{r:?}"),
        }
    }
}

#[test]
fn single_hand_and_abandon() {
    let s = Session::start(SessionConfig {
        seed: Some(5),
        max_hands: Some(1),
        ..SessionConfig::default()
    })
    .unwrap();
    // 只打一局就結束。
    play_out(&s);
    // 丟棄還在等待中的 session：對局自動打完，不會卡住。
    let s = Session::start(SessionConfig {
        seed: Some(6),
        opponents: ["greedy".into(), "greedy".into(), "greedy".into()],
        ..SessionConfig::default()
    })
    .unwrap();
    let mut since = 0;
    loop {
        let u = s.poll(since, Duration::from_secs(10));
        since = u.next;
        if u.ask.is_some() {
            break;
        }
    }
    drop(s);
}

/// 多場對局檢查牌桌推算（除錯建置時每次詢問都會與引擎的視角比對）。
/// 場數由環境變數 `MJ_APP_GAMES` 指定（預設 3），例如 `MJ_APP_GAMES=50 cargo test --profile fuzz -p mj-app`。
#[test]
fn table_matches_engine_view() {
    let games: u64 = std::env::var("MJ_APP_GAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    for g in 0..games {
        let s = Session::start(SessionConfig {
            seed: Some(1000 + g),
            seat: (g % 4) as u8,
            opponents: [
                mj_app::session::DEFAULT_STRATEGY.into(),
                "greedy".into(),
                mj_app::session::DEFAULT_STRATEGY.into(),
            ],
            ..SessionConfig::default()
        })
        .unwrap();
        play_out(&s);
    }
}
