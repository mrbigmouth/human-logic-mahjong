//! 讀牌資料匯出：在牌譜（或自我對戰）的每個「自己要打牌」的時點，對三家對手各輸出一列
//! 「看得到的特徵＋全知視角的真相」，供分析與校準讀牌預測（聽牌、聽牌張、打點）。
//!
//! 特徵只用 `mj-tools` 的事實工具從決策者的 `PlayerView` 計算（與策略看得到的相同），
//! 真相用完整事件紀錄重建。輸出兩個 CSV：
//! - `tenpai.csv`：每個時點 × 每家對手一列（聽牌與否、本局是否和牌與打點）；
//! - `waits.csv`：每個時點 × 每家**已聽牌**的對手 × 決策者手上的每種牌一列（是否為聽牌張、振聽）。

use crate::replay::{furiten_at, hands_at};
use mj_core::{Tile, TileCounts};
use mj_engine::{Action, Event, HandEnd, Player, PlayerView, Seat};
use mj_paifu::GameRecord;
use mj_paifu::replay::replay;
use mj_tools::{river, safety, table, visible};
use rayon::prelude::*;
use std::fmt::Write;

/// 旁觀者：記錄每次「自己要打牌」時的視角與事件位置。
#[derive(Default)]
struct Observer {
    events_seen: usize,
    points: Vec<(usize, PlayerView)>,
}

impl Player for Observer {
    fn on_event(&mut self, _me: Seat, _event: &Event) {
        self.events_seen += 1;
    }

    fn act(&mut self, view: &PlayerView, legal: &[Action]) -> Action {
        // 只記錄自己的回合（沒有「跳過」選項），而且這一次是要打牌（手牌 3k+2 張）。
        if !legal.contains(&Action::Pass) && view.hand.total() % 3 == 2 {
            self.points.push((self.events_seen, view.clone()));
        }
        // 旁觀者的動作不會被採用（照牌譜打），回傳任一合法動作即可。
        legal[0].clone()
    }
}

/// `tenpai.csv` 的欄位。
pub const TENPAI_HEADER: &str = "game,kyoku,at,me,seat,rel,dealer,turn,left,melds,riichi,\
trail_tsumo,tedashi3,tedashi6,mid_tedashi6,last_tedashi_turn,last_tedashi_mid,last_tedashi_honor,\
yakuhai_meld,meld_dora,meld_suits,honor_disc,others_riichi,dora_unseen,pons,riichi_turn,tenpai,shanten,n_waits,wins,win_base,win_han";

/// `waits.csv` 的欄位。
pub const WAITS_HEADER: &str = "game,at,seat,turn,riichi,melds,tile,num,honor,genbutsu,suji,\
ryanmen,one_chance,visible,dora,dora_dist,riichi_tile_dist,last_tedashi_dist,suit_disc,suit_frac,\
meld_suit_match,yakuhai_for_target,wait,furiten,n_waits";

/// 一場的輸出：（tenpai 列, waits 列）。
fn rows_for_game(game: &str, log: &[Event], points: &[(usize, PlayerView)]) -> (String, String) {
    let mut tenpai = String::new();
    let mut waits = String::new();
    for (at, v) in points {
        let hands = hands_at(log, *at);
        let outcome = hand_outcome(log, *at);
        for seat in table::opponents(v) {
            let s = usize::from(seat);
            let h = hands[s];
            let w = mj_calc::waits(&h);
            let sh = mj_calc::shanten(&h);
            let turn = river::turn(v, seat);
            let last = river::last_tedashi(v, seat);
            let honor_disc = river::suit_counts(v, seat)[3];
            let others_riichi = table::opponents(v)
                .into_iter()
                .filter(|&o| o != seat && v.riichi[usize::from(o)])
                .count()
                + usize::from(v.riichi[usize::from(v.me)]);
            let dora_unseen = mj_tools::dora::unseen_dora(v);
            let pons = v.melds[s].iter().filter(|m| !m.is_sequence()).count();
            let riichi_turn = v.rivers[s]
                .iter()
                .position(|d| d.riichi_declared)
                .map_or(-1, |i| i as i64);
            let (wins, base, han) = match outcome {
                Some((winner, base, han)) if winner == seat => (1, base, han),
                _ => (0, 0, 0),
            };
            writeln!(
                tenpai,
                "{game},{},{at},{},{seat},{},{},{turn},{},{},{},{},{},{},{},{},{},{},{},{},{},{honor_disc},{others_riichi},{dora_unseen},{pons},{riichi_turn},{},{sh},{},{wins},{base},{han}",
                v.current_kyoku_index(),
                v.me,
                table::relative(v, seat),
                u8::from(v.dealer() == seat),
                v.tiles_left_in_wall,
                river::open_melds(v, seat),
                u8::from(v.riichi[s]),
                river::trailing_tsumogiri(v, seat),
                river::recent_tedashi(v, seat, 3),
                river::recent_tedashi(v, seat, 6),
                river::middle_tedashi_since(v, seat, 6),
                last.map_or(-1, |(i, _)| i as i64),
                u8::from(last.is_some_and(|(_, p)| river::is_middle(p.tile))),
                u8::from(last.is_some_and(|(_, p)| p.tile.is_honor())),
                u8::from(river::has_yakuhai_meld(v, seat)),
                river::meld_dora(v, seat),
                river::meld_suits(v, seat).count_ones(),
                u8::from(!w.is_empty()),
                w.len(),
            )
            .unwrap();
            if w.is_empty() {
                continue;
            }
            let furiten = furiten_at(log, *at, seat, &w);
            wait_rows(&mut waits, game, *at, v, seat, &w, furiten);
        }
    }
    (tenpai, waits)
}

/// 已聽牌的 `seat` 對決策者手上每種牌的特徵與真相。
fn wait_rows(
    out: &mut String,
    game: &str,
    at: usize,
    v: &PlayerView,
    seat: Seat,
    w: &[Tile],
    furiten: bool,
) {
    let s = usize::from(seat);
    let seen = visible::visible_counts(v);
    let dora = mj_tools::dora::dora_tiles(v);
    let suits = river::suit_counts(v, seat);
    let total_disc = suits.iter().map(|&c| u32::from(c)).sum::<u32>().max(1);
    let riichi_tile = safety::riichi_declaration_tile(v, seat);
    let last = river::last_tedashi(v, seat).map(|(_, p)| p.tile);
    let meld_suits = river::meld_suits(v, seat);
    let mine: TileCounts = v.hand;
    for tile in Tile::all().filter(|&t| mine.get(t) > 0) {
        let dist = |other: Option<Tile>| -> i64 {
            match other {
                Some(o) if o.suit() == tile.suit() && !tile.is_honor() => {
                    (i64::from(o.number()) - i64::from(tile.number())).abs()
                }
                _ => -1,
            }
        };
        let dora_dist = dora
            .iter()
            .map(|&d| dist(Some(d)))
            .filter(|&d| d >= 0)
            .min()
            .unwrap_or(-1);
        let si = river::suit_index(tile.suit());
        let meld_suit_match = if meld_suits == 0 || tile.is_honor() {
            -1
        } else {
            i64::from(meld_suits == 1 << si)
        };
        let suji = match safety::suji(v, seat, tile) {
            safety::Suji::None => 0,
            safety::Suji::Half => 1,
            safety::Suji::Full => 2,
        };
        writeln!(
            out,
            "{game},{at},{seat},{},{},{},{tile},{},{},{},{suji},{},{},{},{},{dora_dist},{},{},{},{:.3},{meld_suit_match},{},{},{},{}",
            river::turn(v, seat),
            u8::from(v.riichi[s]),
            river::open_melds(v, seat),
            tile.number(),
            u8::from(tile.is_honor()),
            u8::from(safety::is_genbutsu(v, seat, tile)),
            safety::ryanmen_chances(v, tile),
            u8::from(safety::is_one_chance(v, tile)),
            seen.get(tile),
            u8::from(dora.contains(&tile)),
            dist(riichi_tile),
            dist(last),
            suits[si],
            f64::from(suits[si]) / f64::from(total_disc),
            u8::from(tile.is_honor() && table::is_yakuhai_for(v, seat, tile)),
            u8::from(w.contains(&tile)),
            u8::from(furiten),
            w.len(),
        )
        .unwrap();
    }
}

/// 第 `at` 個事件之後，本局的（和牌者, 基本點, 番數）；流局為 `None`（雙響時取第一位）。
fn hand_outcome(log: &[Event], at: usize) -> Option<(Seat, i32, u8)> {
    for ev in &log[at.min(log.len())..] {
        match ev {
            Event::StartKyoku { .. } => return None,
            Event::HandEnd {
                end: HandEnd::Win { winner, score, .. },
                ..
            } => return Some((*winner, score.base, score.han)),
            _ => {}
        }
    }
    None
}

/// 在牌譜上匯出讀牌資料：回傳（tenpai.csv, waits.csv 的內容, 無法重播的牌譜與原因）。
pub fn collect_records(records: &[GameRecord]) -> (String, String, Vec<String>) {
    let per_game: Vec<Result<(String, String), String>> = records
        .par_iter()
        .map(|rec| {
            let mut obs: [Observer; 4] = Default::default();
            let shadows: [Option<&mut dyn Player>; 4] = {
                let mut it = obs.iter_mut().map(|o| Some(o as &mut dyn Player));
                std::array::from_fn(|_| it.next().unwrap())
            };
            let r = replay(rec, shadows, false).map_err(|e| format!("{}: {e}", rec.id))?;
            let mut t = String::new();
            let mut w = String::new();
            for o in &obs {
                let (a, b) = rows_for_game(&rec.id, &r.result.log, &o.points);
                t.push_str(&a);
                w.push_str(&b);
            }
            Ok((t, w))
        })
        .collect();
    let mut tenpai = format!("{TENPAI_HEADER}\n");
    let mut waits = format!("{WAITS_HEADER}\n");
    let mut errs = Vec::new();
    for r in per_game {
        match r {
            Ok((t, w)) => {
                tenpai.push_str(&t);
                waits.push_str(&w);
            }
            Err(e) => errs.push(e),
        }
    }
    (tenpai, waits, errs)
}
