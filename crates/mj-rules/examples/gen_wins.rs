//! 隨機產生和牌，連同本專案算出的分數以 JSON lines 輸出，
//! 供差異測試與獨立的計分程式比對（tools/oracle/check_scoring.py）。
//!
//! 用法：cargo run --release -p mj-rules --example gen_wins -- <手數> <seed>
//!
//! 為了讓各種役都有足夠的出現次數，產生方式刻意偏向特定牌型：
//! 單一花色（清一色／混一色）、字牌多（役牌、字一色）、順子多（平和、三色、一通）、
//! 只有老頭牌（清老頭）、九蓮、國士、七對子，以及隨機的副露、槓、立直、寶牌、赤五等狀況。

use mj_core::{Meld, MeldKind, Pai, Tile, TileCounts};
use mj_rules::riichi::{RiichiConfig, RiichiState, WinContext, score};
use rand::seq::IndexedRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde_json::json;

/// 產生時的面子形狀（數字為牌的編號）。
#[derive(Clone, Copy)]
enum Shape {
    /// 順子，從此編號開始
    Seq(u8),
    /// 刻子
    Trip(u8),
}

/// 從允許的花色（與字牌）中隨機挑一種牌。
fn pick_kind(rng: &mut ChaCha8Rng, suits: &[u8], honors: bool) -> u8 {
    if honors && rng.random_bool(0.3) {
        return 27 + rng.random_range(0..7);
    }
    suits.choose(rng).unwrap() * 9 + rng.random_range(0..9)
}

fn main() {
    let args: Vec<u64> = std::env::args()
        .skip(1)
        .map(|a| a.parse().unwrap())
        .collect();
    let (count, seed) = (
        args.first().copied().unwrap_or(1000),
        args.get(1).copied().unwrap_or(0),
    );
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let cfg = RiichiConfig::TENHOU;
    let mut emitted = 0;
    while emitted < count {
        // 偏向少數花色，讓清一色、混一色、字一色、役滿經常出現。
        let suits: Vec<u8> = match rng.random_range(0..10) {
            0..=1 => vec![rng.random_range(0..3)],
            2 => vec![],
            _ => vec![0, 1, 2],
        };
        // 順子多的手牌用來測平和、三色同順、一氣通貫、純全帶么九。
        let seq_heavy = rng.random_bool(0.4);
        let honors = suits.is_empty() || rng.random_bool(if seq_heavy { 0.15 } else { 0.6 });
        let seq_p = if seq_heavy { 0.9 } else { 0.55 };
        let terminals_only = rng.random_bool(0.03);
        let chuuren = rng.random_bool(0.01);
        let chiitoi = !chuuren && !terminals_only && rng.random_bool(0.08);
        let kokushi = !chuuren && !chiitoi && rng.random_bool(0.01);

        let mut counts = [0u8; 34];
        let mut shapes: Vec<Shape> = Vec::new();
        let mut ok = true;
        let pick = |rng: &mut ChaCha8Rng| {
            if terminals_only {
                let s = rng.random_range(0..3u8);
                return s * 9 + if rng.random_bool(0.5) { 0 } else { 8 };
            }
            if suits.is_empty() {
                27 + rng.random_range(0..7)
            } else {
                pick_kind(rng, &suits, honors)
            }
        };
        // 依模式組出 14 張的和牌（尚未決定副露）。
        if chuuren {
            let s = rng.random_range(0..3usize);
            for (i, c) in [3, 1, 1, 1, 1, 1, 1, 1, 3].into_iter().enumerate() {
                counts[s * 9 + i] = c;
            }
            counts[s * 9 + rng.random_range(0..9)] += 1;
        } else if kokushi {
            for t in Tile::all().filter(|t| t.is_yaochu()) {
                counts[t.index()] += 1;
            }
            let extra = *Tile::all()
                .filter(|t| t.is_yaochu())
                .collect::<Vec<_>>()
                .choose(&mut rng)
                .unwrap();
            counts[extra.index()] += 1;
        } else if chiitoi {
            let mut kinds = Vec::new();
            while kinds.len() < 7 {
                let k = pick(&mut rng);
                if !kinds.contains(&k) {
                    kinds.push(k);
                }
            }
            for k in kinds {
                counts[usize::from(k)] += 2;
            }
        } else {
            for _ in 0..4 {
                let k = pick(&mut rng);
                let t = Tile::from_index(k);
                if !terminals_only && !t.is_honor() && t.number() <= 7 && rng.random_bool(seq_p) {
                    shapes.push(Shape::Seq(k));
                    (0..3).for_each(|i| counts[usize::from(k + i)] += 1);
                } else {
                    shapes.push(Shape::Trip(k));
                    counts[usize::from(k)] += 3;
                }
            }
            let pair = pick(&mut rng);
            counts[usize::from(pair)] += 2;
            ok = counts.iter().all(|&c| c <= 4);
        }
        if !ok {
            continue;
        }

        // 隨機把部分面子改成副露（吃、碰、各種槓）。
        let mut melds: Vec<Meld> = Vec::new();
        let mut concealed = TileCounts(counts);
        if !chiitoi && !kokushi && !chuuren {
            for s in &shapes {
                if !rng.random_bool(0.3) {
                    continue;
                }
                let from = Some(rng.random_range(1..4));
                let meld = match *s {
                    Shape::Seq(k) => {
                        let tiles: Vec<Pai> = (0..3)
                            .map(|i| Pai::plain(Tile::from_index(k + i)))
                            .collect();
                        Meld {
                            kind: MeldKind::Chi,
                            called: Some(tiles[0]),
                            tiles,
                            from,
                        }
                    }
                    Shape::Trip(k) => {
                        let p = Pai::plain(Tile::from_index(k));
                        // 5 的槓子必然包含赤五，為了簡化不產生。
                        let five = !p.tile.is_honor() && p.tile.number() == 5;
                        let kan = counts[usize::from(k)] == 3 && !five && rng.random_bool(0.3);
                        if kan {
                            counts[usize::from(k)] += 1;
                            let kind = *[MeldKind::Ankan, MeldKind::Daiminkan, MeldKind::Kakan]
                                .choose(&mut rng)
                                .unwrap();
                            let open = kind != MeldKind::Ankan;
                            Meld {
                                kind,
                                tiles: vec![p; 4],
                                called: open.then_some(p),
                                from: if open { from } else { None },
                            }
                        } else {
                            Meld {
                                kind: MeldKind::Pon,
                                tiles: vec![p; 3],
                                called: Some(p),
                                from,
                            }
                        }
                    }
                };
                let base = match *s {
                    Shape::Seq(k) => (0..3).map(|i| Tile::from_index(k + i)).collect::<Vec<_>>(),
                    Shape::Trip(k) => vec![Tile::from_index(k); 3],
                };
                base.iter().for_each(|&t| {
                    concealed.remove(t);
                });
                melds.push(meld);
            }
        }
        // 和牌張從門前部分隨機挑一張；接著隨機產生和牌狀況（自摸／榮和、莊家、風位、立直……）。
        let kinds_in_hand: Vec<Tile> = concealed.tiles().collect();
        let win = *kinds_in_hand.choose(&mut rng).unwrap();
        let closed = melds.iter().all(|m| !m.is_open());
        let has_kan = melds.iter().any(|m| m.is_kan());

        let mut ctx = WinContext::new(win, rng.random_bool(0.5));
        ctx.dealer = rng.random_bool(0.25);
        ctx.seat_wind = if ctx.dealer {
            0
        } else {
            rng.random_range(1..4)
        };
        ctx.round_wind = rng.random_range(0..2);
        if closed && rng.random_bool(0.5) {
            ctx.riichi = if rng.random_bool(0.1) {
                RiichiState::Double
            } else {
                RiichiState::Riichi
            };
            ctx.ippatsu = rng.random_bool(0.2);
        }
        let last = rng.random_bool(0.05);
        ctx.haitei = last && ctx.tsumo;
        ctx.houtei = last && !ctx.tsumo;
        ctx.rinshan = has_kan && ctx.tsumo && !ctx.haitei && rng.random_bool(0.3);
        if ctx.rinshan {
            ctx.ippatsu = false;
        }
        // 搶槓：榮和的是別人加槓的牌，所以這張牌不能是我們手上已有多張的牌。
        ctx.chankan = !ctx.tsumo && !last && counts[win.index()] == 1 && rng.random_bool(0.05);
        ctx.first_draw = melds.is_empty()
            && ctx.tsumo
            && ctx.riichi == RiichiState::None
            && rng.random_bool(0.03);
        // 寶牌指示牌數 = 1 + 槓的數量。
        let n_ind = 1 + melds.iter().filter(|m| m.is_kan()).count();
        ctx.dora_indicators = (0..n_ind)
            .map(|_| Tile::from_index(rng.random_range(0..34)))
            .collect();
        if ctx.riichi != RiichiState::None {
            ctx.ura_indicators = (0..n_ind)
                .map(|_| Tile::from_index(rng.random_range(0..34)))
                .collect();
        }
        // 赤五：每種花色最多一張，只放在門前部分（簡化副露）。
        // 若某種 5 四張都在手上，其中一張必然是赤五。
        let mut reds = Vec::new();
        let mut impossible = false;
        for s in 0..3u8 {
            let five = Tile::from_index(s * 9 + 4);
            let all_four = counts[five.index()] == 4;
            if concealed.get(five) > 0 && (all_four || rng.random_bool(0.4)) {
                reds.push(five.to_string());
            } else if all_four {
                impossible = true;
            }
        }
        if impossible {
            continue;
        }
        ctx.red_fives = reds.len() as u8;

        // 用本專案的計分算出結果，連同手牌與狀況一起輸出（無役時 ours 為 null）。
        let result = score(&concealed, &melds, &ctx, &cfg);
        let melds_json: Vec<_> = melds
            .iter()
            .map(|m| json!({"kind": format!("{:?}", m.kind), "tiles": m.tiles.iter().map(|p| p.to_string()).collect::<Vec<_>>()}))
            .collect();
        let ours = result.as_ref().map(|s| {
            let (td, tn) = s.tsumo_points(ctx.dealer, 0);
            json!({
                "han": s.han, "fu": s.fu, "yakuman": matches!(s.limit, mj_rules::riichi::Limit::Yakuman(_)),
                "yaku": s.yaku.iter().map(|(y, h)| format!("{y:?}:{h}")).collect::<Vec<_>>(),
                "total": if ctx.tsumo { if ctx.dealer { td * 3 } else { td + 2 * tn } } else { s.ron_points(ctx.dealer, 0) },
            })
        });
        println!(
            "{}",
            json!({
                "concealed": concealed.to_string(), "melds": melds_json, "win": win.to_string(),
                "tsumo": ctx.tsumo, "riichi": format!("{:?}", ctx.riichi), "ippatsu": ctx.ippatsu,
                "haitei": ctx.haitei, "houtei": ctx.houtei, "chankan": ctx.chankan, "rinshan": ctx.rinshan, "first_draw": ctx.first_draw,
                "dealer": ctx.dealer, "seat_wind": ctx.seat_wind, "round_wind": ctx.round_wind,
                "dora": ctx.dora_indicators.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
                "ura": ctx.ura_indicators.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
                "reds": reds, "ours": ours,
            })
        );
        emitted += 1;
    }
}
