//! 牌譜重播：從完整（全知視角）的事件紀錄，重建任一時刻各家的手牌。
//! 預測評估用它取得「當時的真相」。

use mj_core::{MeldKind, TileCounts};
use mj_engine::{Event, HandEnd, Seat};

/// 第 `index` 個事件發生「之前」各家的門前手牌（`index` = 已發生的事件數）。
pub fn hands_at(log: &[Event], index: usize) -> [TileCounts; 4] {
    let mut hands: [TileCounts; 4] = Default::default();
    for ev in &log[..index.min(log.len())] {
        match ev {
            Event::StartKyoku { hands: h, .. } => {
                for s in 0..4 {
                    hands[s] = TileCounts::from_tiles(h[s].iter().map(|p| p.tile));
                }
            }
            Event::Draw {
                seat, pai: Some(p), ..
            } => hands[usize::from(*seat)].add(p.tile),
            Event::Discard { seat, pai, .. } => {
                hands[usize::from(*seat)].remove(pai.tile);
            }
            Event::Call { seat, meld } => {
                let h = &mut hands[usize::from(*seat)];
                match meld.kind {
                    // 加槓：手上只少了加上去的那一張。
                    MeldKind::Kakan => {
                        h.remove(meld.base());
                    }
                    // 暗槓：四張都從手上拿出。
                    MeldKind::Ankan => meld.tiles.iter().for_each(|p| {
                        h.remove(p.tile);
                    }),
                    // 吃、碰、明槓：鳴進來的那張不在手上，其餘從手上拿出。
                    _ => {
                        let mut skipped = false;
                        for p in &meld.tiles {
                            if !skipped && Some(*p) == meld.called {
                                skipped = true;
                                continue;
                            }
                            h.remove(p.tile);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    hands
}

/// 第 `index` 個事件之前，`seat` 對聽牌張 `waits` 是否振聽（不能榮和）：
/// - 捨牌振聽：自己打過任一張聽牌張；
/// - 立直振聽：立直後，別人打出過聽牌張（他沒有和）；
/// - 同巡振聽：自己最後一次打牌後，別人打出過聽牌張。
pub fn furiten_at(log: &[Event], index: usize, seat: Seat, waits: &[mj_core::Tile]) -> bool {
    // 只看本局（最後一個 StartKyoku 之後）的事件。
    let events = &log[..index.min(log.len())];
    let start = events
        .iter()
        .rposition(|e| matches!(e, Event::StartKyoku { .. }))
        .unwrap_or(0);
    let mut furiten = false;
    let mut riichi = false;
    let mut passed_since_own_discard = false;
    for ev in &events[start..] {
        match ev {
            Event::Discard {
                seat: s,
                pai,
                riichi: r,
                ..
            } if *s == seat => {
                furiten |= waits.contains(&pai.tile);
                riichi |= *r;
                passed_since_own_discard = false;
            }
            Event::Discard { pai, .. } if waits.contains(&pai.tile) => {
                passed_since_own_discard = true;
                furiten |= riichi;
            }
            _ => {}
        }
    }
    furiten || passed_since_own_discard
}

/// 第 `index` 個事件之後，這一局（到下一個 `StartKyoku` 之前）的和牌者。流局時為空。
pub fn hand_winners_after(log: &[Event], index: usize) -> Vec<Seat> {
    hand_wins_after(log, index)
        .into_iter()
        .map(|(w, _)| w)
        .collect()
}

/// 第 `index` 個事件之後，這一局的和牌：（和牌者, 基本點）。流局時為空。
pub fn hand_wins_after(log: &[Event], index: usize) -> Vec<(Seat, i32)> {
    let mut wins = Vec::new();
    for ev in &log[index.min(log.len())..] {
        match ev {
            Event::StartKyoku { .. } => break,
            Event::HandEnd {
                end: HandEnd::Win { winner, score, .. },
                ..
            } => wins.push((*winner, score.base)),
            _ => {}
        }
    }
    wins
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::GreedyBot;
    use mj_engine::{GameConfig, Player, run_game};
    use mj_rules::riichi::RiichiConfig;

    /// 每次打牌前，重建的手牌必須含有那張牌；每局結束時各家張數必須是 3k+1 或 3k+2。
    #[test]
    fn reconstructed_hands_are_consistent() {
        let mut players: [Box<dyn Player>; 4] =
            std::array::from_fn(|i| Box::new(GreedyBot::new(i as u64, 0.5)) as Box<dyn Player>);
        let r = run_game(
            &GameConfig::new(RiichiConfig::TENHOU, 3),
            &mut players,
            true,
        )
        .unwrap();
        for (i, ev) in r.log.iter().enumerate() {
            let hands = hands_at(&r.log, i);
            if let Event::Discard { seat, pai, .. } = ev {
                assert!(
                    hands[usize::from(*seat)].get(pai.tile) > 0,
                    "event {i}: {pai} not in hand"
                );
            }
            if let Event::EndKyoku { .. } = ev {
                for h in &hands {
                    assert!(
                        matches!(h.total() % 3, 1 | 2),
                        "bad hand size {}",
                        h.total()
                    );
                }
            }
        }
    }

    fn ev_discard(seat: u8, tile: &str, riichi: bool) -> Event {
        Event::Discard {
            seat,
            pai: tile.parse().unwrap(),
            tsumogiri: false,
            riichi,
        }
    }

    fn start() -> Event {
        Event::StartKyoku {
            round_wind: 0,
            kyoku: 0,
            honba: 0,
            riichi_sticks: 0,
            scores: [25_000; 4],
            dora_indicator: "1z".parse().unwrap(),
            hands: Default::default(),
        }
    }

    /// 三種振聽：捨牌振聽、同巡振聽（自己再打牌後解除）、立直振聽（永久）。
    #[test]
    fn furiten_kinds() {
        let w = ["3m".parse().unwrap(), "6m".parse().unwrap()];
        // 自己打過 6m → 捨牌振聽。
        let log = vec![start(), ev_discard(1, "6m", false)];
        assert!(furiten_at(&log, log.len(), 1, &w));
        // 別人打 3m 後 → 同巡振聽；自己再打一張後解除。
        let mut log = vec![
            start(),
            ev_discard(1, "9p", false),
            ev_discard(2, "3m", false),
        ];
        assert!(furiten_at(&log, log.len(), 1, &w));
        log.push(ev_discard(1, "9s", false));
        assert!(!furiten_at(&log, log.len(), 1, &w));
        // 立直後別人打 3m → 之後永遠振聽。
        let mut log = vec![
            start(),
            ev_discard(1, "9p", true),
            ev_discard(2, "3m", false),
        ];
        log.push(ev_discard(1, "9s", false));
        assert!(furiten_at(&log, log.len(), 1, &w));
    }
}
