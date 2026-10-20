//! Mortal 回應附帶的 Q 值（`tools/mortal/run.sh <模型> --q-values` 時，每個回應都有 `meta.q_values`）
//! → 各合法動作的機率。給提示與復盤顯示「Mortal 認為每個動作有多好」用。
//!
//! Mortal 的動作空間（libriichi `consts.rs`，46 種）：0–33 打牌（牌種編號與 [`mj_core::Tile`] 相同）、
//! 34–36 打赤五（萬、筒、索）、37 立直、38–40 吃（鳴的牌在順子的低／中／高位置）、41 碰、
//! 42 槓（暗槓或加槓時由 `meta.kan_select` 再選哪一種）、43 和牌、44 九種九牌、45 跳過。
//! `mask_bits` 標出合法的動作，`q_values` 依編號由小到大只列合法的。
//!
//! 機率 = Q 值的 softmax（溫度 1）。Mortal 是 DQN：Q 值是它估計的「選這個動作之後的價值」，
//! 機率只是把相對好壞換成好讀的比例，不是它真的會隨機選的機率（實際上一律選 Q 值最高的）。

use mj_core::{Pai, Tile};
use mj_engine::{Action, PlayerView};
use serde_json::Value;

/// Mortal 的一個動作。立直在 Mortal 是單獨一步（之後才決定打哪張），不能對應到單一個合法動作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Move {
    Act(Action),
    Riichi,
}

/// 由 Mortal 的回應算出各動作的機率（由高到低）。沒有 Q 值（沒加 `--q-values`、或只有一個選擇時）回傳 `None`。
///
/// `after_riichi` = 這是立直宣言後「要打哪張」的回應（`mj_peek` 的回答）：打牌對應到 `Action::Riichi`。
pub fn policy(
    reply: &Value,
    view: &PlayerView,
    legal: &[Action],
    after_riichi: bool,
) -> Option<Vec<(Move, f64)>> {
    let meta = reply.get("meta")?;
    let q: Vec<f64> = meta
        .get("q_values")?
        .as_array()?
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    let mask = meta.get("mask_bits")?.as_u64()?;
    let ids: Vec<usize> = (0..46).filter(|i| mask >> i & 1 == 1).collect();
    if ids.len() != q.len() || q.is_empty() {
        return None;
    }
    let top = q.iter().copied().fold(f64::MIN, f64::max);
    let w: Vec<f64> = q.iter().map(|x| (x - top).exp()).collect();
    let sum: f64 = w.iter().sum();
    let mut out: Vec<(Move, f64)> = ids
        .iter()
        .zip(&w)
        .filter_map(|(&id, &x)| Some((to_move(id, meta, view, legal, after_riichi)?, x / sum)))
        .collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    Some(out)
}

/// 赤五的牌種：34–36 → 5m、5p、5s。
fn red_five(id: usize) -> Pai {
    Pai {
        tile: Tile::new([4, 13, 22][id - 34]).expect("five"),
        red: true,
    }
}

fn to_move(
    id: usize,
    meta: &Value,
    view: &PlayerView,
    legal: &[Action],
    after_riichi: bool,
) -> Option<Move> {
    let find = |ok: &dyn Fn(&Action) -> bool| legal.iter().find(|a| ok(a)).cloned();
    let act = match id {
        0..=36 => {
            let pai = if id < 34 {
                Pai::plain(Tile::new(id as u8)?)
            } else {
                red_five(id)
            };
            // 與 Mortal 相同：打的是剛摸到的那張時算摸切。
            let tg = view.drawn == Some(pai);
            let is = |a: &Action| match a {
                Action::Riichi { pai: p, tsumogiri } if after_riichi => {
                    *p == pai && *tsumogiri == tg
                }
                Action::Discard { pai: p, tsumogiri } if !after_riichi => {
                    *p == pai && *tsumogiri == tg
                }
                _ => false,
            };
            let same_pai = |a: &Action| match a {
                Action::Riichi { pai: p, .. } if after_riichi => *p == pai,
                Action::Discard { pai: p, .. } if !after_riichi => *p == pai,
                _ => false,
            };
            find(&is).or_else(|| find(&same_pai))
        }
        37 => return Some(Move::Riichi),
        38..=40 => {
            let called = view.last_discard?.1.tile.index() as i32;
            let offsets = [[1, 2], [-1, 1], [-2, -1]][id - 38];
            let want: Vec<i32> = offsets.iter().map(|o| called + o).collect();
            let chis: Vec<&Action> = legal
                .iter()
                .filter(|a| match a {
                    Action::Chi { consumed } => {
                        let mut c: Vec<i32> =
                            consumed.iter().map(|p| p.tile.index() as i32).collect();
                        c.sort();
                        c == want
                    }
                    _ => false,
                })
                .collect();
            // 有赤五時 Mortal 用赤五吃。
            chis.iter()
                .find(|a| matches!(a, Action::Chi { consumed } if consumed.iter().any(|p| p.red)))
                .or(chis.first())
                .map(|a| (*a).clone())
        }
        41 => find(&|a| matches!(a, Action::Pon { consumed } if consumed.iter().any(|p| p.red)))
            .or_else(|| find(&|a| matches!(a, Action::Pon { .. }))),
        42 => find(&|a| *a == Action::Daiminkan).or_else(|| {
            // 暗槓、加槓：kan_select 的 Q 值最高的牌種（只有一種槓時沒有 kan_select）。
            let tile = meta.get("kan_select").and_then(|k| {
                let q = k.get("q_values")?.as_array()?;
                let mask = k.get("mask_bits")?.as_u64()?;
                let ids = (0..34u8).filter(|i| mask >> i & 1 == 1);
                ids.zip(q.iter().filter_map(Value::as_f64))
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .and_then(|(i, _)| Tile::new(i))
            });
            find(&|a| match a {
                Action::Ankan { tile: t } => tile.is_none_or(|x| x == *t),
                Action::Kakan { pai } => tile.is_none_or(|x| x == pai.tile),
                _ => false,
            })
        }),
        43 => find(&|a| matches!(a, Action::Tsumo | Action::Ron)),
        44 => find(&|a| *a == Action::KyuushuKyuuhai),
        45 => find(&|a| *a == Action::Pass),
        _ => None,
    };
    act.map(Move::Act)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn view(drawn: Option<&str>) -> PlayerView {
        PlayerView {
            me: 1,
            round_wind: 0,
            kyoku: 0,
            honba: 0,
            riichi_sticks: 0,
            scores: [25_000; 4],
            dora_indicators: vec![],
            hand: mj_core::TileCounts::default(),
            hand_pais: vec![],
            drawn: drawn.map(|d| d.parse().unwrap()),
            melds: Default::default(),
            rivers: Default::default(),
            riichi: [false; 4],
            tiles_left_in_wall: 60,
            total_kyoku: 8,
            last_discard: None,
        }
    }

    /// 合法動作的編號、Q 值的 softmax、摸切的判定與排序。
    #[test]
    fn decodes_discards_and_riichi() {
        let d = |s: &str, tsumogiri| Action::Discard {
            pai: s.parse().unwrap(),
            tsumogiri,
        };
        let legal = vec![
            d("1m", false),
            d("0p", false),
            d("7z", true),
            Action::Riichi {
                pai: "7z".parse().unwrap(),
                tsumogiri: true,
            },
        ];
        // 1m = 0、赤五筒 = 35、中 = 33、立直 = 37
        let mask: u64 = 1 | 1 << 33 | 1 << 35 | 1 << 37;
        let reply = json!({ "type": "dahai", "meta": { "q_values": [0.0, 1.0, -1.0, 2.0], "mask_bits": mask } });
        let p = policy(&reply, &view(Some("7z")), &legal, false).unwrap();
        assert_eq!(p.len(), 4);
        assert_eq!(p[0].0, Move::Riichi);
        assert_eq!(p[1].0, Move::Act(d("7z", true)));
        assert_eq!(p[3].0, Move::Act(d("0p", false)));
        let total: f64 = p.iter().map(|x| x.1).sum();
        assert!((total - 1.0).abs() < 1e-9);
        assert!(p[0].1 > p[1].1 && p[1].1 > p[2].1);
        // 沒有 Q 值
        assert!(policy(&json!({ "type": "none" }), &view(None), &legal, false).is_none());
    }
}
