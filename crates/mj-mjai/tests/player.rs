//! mjai 介接的測試：內建機器人經由 `MjaiPlayer` 坐上引擎的牌桌。

use mj_engine::{GameConfig, Player, run_game, run_game_with};
use mj_mjai::bots::{Kind, MjaiBot};
use mj_mjai::{Bot, MjaiPlayer};
use mj_rules::riichi::RiichiConfig;
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// 記下收到的所有事件，再交給內建機器人回應。
struct Recording {
    inner: MjaiBot,
    seen: Arc<Mutex<Vec<Value>>>,
}

impl Bot for Recording {
    fn react(&mut self, events: &[Value]) -> Result<Value, String> {
        self.seen.lock().unwrap().extend(events.iter().cloned());
        self.inner.react(events)
    }
}

fn names() -> [String; 4] {
    std::array::from_fn(|i| format!("p{i}"))
}

/// 兩個 mjai 機器人和兩個策略打完整場：全部合法、點數守恆；
/// 機器人收到的事件中看不到別人的配牌與摸牌（不能作弊）。
#[test]
fn bots_play_full_games_without_seeing_hidden_tiles() {
    let st = mj_strategy::find_strategy("tenhou-normal-4p-hanchan/hand-reading").unwrap();
    for seed in 0..4 {
        let seen: [Arc<Mutex<Vec<Value>>>; 2] = Default::default();
        let bot = |kind, i: usize| Recording {
            inner: MjaiBot::new(kind),
            seen: seen[i].clone(),
        };
        let mut players: [Box<dyn Player>; 4] = [
            Box::new(MjaiPlayer::new(bot(Kind::Shanten, 0), names())),
            st.player(false, None),
            Box::new(MjaiPlayer::new(bot(Kind::Tsumogiri, 1), names())),
            st.player(false, None),
        ];
        let r = run_game(
            &GameConfig::new(RiichiConfig::TENHOU, seed),
            &mut players,
            false,
        )
        .expect("all actions legal");
        assert_eq!(r.scores.iter().sum::<i32>(), 100_000);
        for (log, me) in seen.iter().zip([0u64, 2]) {
            let log = log.lock().unwrap();
            assert!(
                log.iter()
                    .any(|e| e["type"] == "hora" || e["type"] == "ryukyoku")
            );
            assert_eq!(log[0]["type"], "start_game");
            assert_eq!(log[0]["id"].as_u64(), Some(me));
            for e in log.iter() {
                match e["type"].as_str().unwrap() {
                    "start_kyoku" => {
                        for (s, h) in e["tehais"].as_array().unwrap().iter().enumerate() {
                            let hidden = h.as_array().unwrap().iter().all(|t| t == "?");
                            assert_eq!(hidden, s as u64 != me, "配牌遮蔽錯誤：{e}");
                        }
                    }
                    "tsumo" => {
                        let hidden = e["pai"] == "?";
                        assert_eq!(hidden, e["actor"].as_u64() != Some(me), "摸牌遮蔽錯誤：{e}");
                    }
                    _ => {}
                }
            }
        }
    }
}

/// 不合法的回應：對局停止，錯誤原因可以取出。
#[test]
fn illegal_reply_stops_the_game() {
    struct Bad;
    impl Bot for Bad {
        fn react(&mut self, _: &[Value]) -> Result<Value, String> {
            Ok(serde_json::json!({ "type": "nonsense" }))
        }
    }
    let st = mj_strategy::find_strategy("tenhou-normal-4p-hanchan/hand-reading").unwrap();
    let mut bad = MjaiPlayer::new(Bad, names());
    let mut others: Vec<_> = (0..3).map(|_| st.player(false, None)).collect();
    let [a, b, c] = &mut others[..] else {
        unreachable!()
    };
    let r = run_game_with(
        &GameConfig::new(RiichiConfig::TENHOU, 3),
        [&mut bad, a.as_mut(), b.as_mut(), c.as_mut()],
        false,
    );
    assert!(r.is_err());
    let e = bad.take_error().expect("error recorded");
    assert!(e.contains("回應不合法"), "{e}");
}
