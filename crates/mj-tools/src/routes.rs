//! 和牌路線：離目前手牌最近的和牌形——還要摸到哪些牌、要打掉哪些牌、和了時的役與點數。
//!
//! 做法：從目前的手牌出發，每一步「摸一張讓向聽數前進的牌，再打一張維持前進後向聽數的牌」，
//! 直到和牌（最短的路線，摸牌數 = 向聽數 + 1）。同樣的局面（手牌＋已摸的牌）只展開一次，
//! 並設搜尋上限（[`MAX_NODES`]）；向聽數超過 [`MAX_SHANTEN`] 時不算（路線太多，意義也不大）。
//!
//! 點數：完成形以每一張「要摸的牌」當和牌張各算一次榮和（門前時假設立直），取最高者。
//! 赤寶牌依目前手上持有的計算（被打掉的赤五仍會算進去，屬於近似）。
//! 機會：每種要摸的牌的剩餘張數（扣掉自己看得到的）取組合數相乘，只用來比較路線之間的難易。

use crate::table::i_am_closed;
use crate::value::{Score, ron_points, win_value};
use crate::visible::remaining;
use mj_calc::shanten;
use mj_core::{Tile, TileCounts};
use mj_engine::PlayerView;
use std::collections::{HashMap, HashSet};

/// 只計算向聽數不超過這個值的手牌。
pub const MAX_SHANTEN: i8 = 2;
/// 搜尋的節點上限（避免手牌很散時太慢）。
pub const MAX_NODES: usize = 60_000;

/// 一條和牌路線。
#[derive(Debug, Clone, PartialEq)]
pub struct WinRoute {
    /// 要打掉的牌（摸牌後 3k+2 張時，第一張是現在要打的）
    pub discards: Vec<Tile>,
    /// 還要摸到的牌（已排序；順序不限）
    pub draws: Vec<Tile>,
    /// 和了時的門前手牌（已排序，含和牌張）
    pub hand: Vec<Tile>,
    /// 點數最高的和牌張（`draws` 之一）
    pub win_tile: Tile,
    /// 以 `win_tile` 榮和的計分；無役（例如副露沒有役）時為 `None`
    pub score: Option<Score>,
    /// 榮和點數（不含本場、供託；無役為 0）
    pub points: i32,
    /// 點數是假設立直計算的（門前、還沒立直）
    pub assumes_riichi: bool,
    /// 相對機會：各種要摸的牌的剩餘張數取組合數相乘
    pub chance: f64,
}

/// 目前手牌（3k+1 張或摸牌後的 3k+2 張）的和牌路線，依機會由大到小排序，最多 `limit` 條；
/// 若點數最高的路線不在其中，另外附在最後。向聽數超過 [`MAX_SHANTEN`] 時回傳空的。
pub fn win_routes(view: &PlayerView, hand: &TileCounts, limit: usize) -> Vec<WinRoute> {
    let rem = remaining(view);
    let mut search = Search {
        view,
        rem: &rem,
        nodes: 0,
        seen: HashSet::new(),
        found: HashMap::new(),
    };
    let starts: Vec<(TileCounts, Vec<Tile>)> = if hand.total() % 3 == 2 {
        // 摸牌後：先打一張（只考慮打完向聽數最低的打法）。
        let mut options = Vec::new();
        for t in Tile::all().filter(|&t| hand.get(t) > 0) {
            let mut h = *hand;
            h.remove(t);
            options.push((shanten(&h), h, vec![t]));
        }
        let best = options.iter().map(|o| o.0).min().unwrap_or(i8::MAX);
        options
            .into_iter()
            .filter(|o| o.0 == best)
            .map(|(_, h, d)| (h, d))
            .collect()
    } else {
        vec![(*hand, Vec::new())]
    };
    for (h, discards) in starts {
        let s = shanten(&h);
        if !(0..=MAX_SHANTEN).contains(&s) {
            continue;
        }
        search.walk(h, s, discards, Vec::new());
    }

    let mut routes: Vec<WinRoute> = search.found.into_values().collect();
    // 同樣要摸的牌、同樣點數的路線只留一條（例如打掉不同的孤張，結果一樣）。
    routes.sort_by(|a, b| {
        b.chance
            .total_cmp(&a.chance)
            .then(b.points.cmp(&a.points))
            .then(a.draws.cmp(&b.draws))
            .then(a.discards.cmp(&b.discards))
    });
    let mut kept: Vec<WinRoute> = Vec::new();
    for r in routes {
        if !kept
            .iter()
            .any(|k| k.draws == r.draws && k.points == r.points && same_yaku(k, &r))
        {
            kept.push(r);
        }
    }
    let best = kept.iter().max_by_key(|r| r.points).cloned();
    kept.truncate(limit);
    if let Some(b) = best
        && b.points > 0
        && !kept.iter().any(|r| r.points >= b.points)
    {
        kept.push(b);
    }
    kept
}

fn same_yaku(a: &WinRoute, b: &WinRoute) -> bool {
    let names = |r: &WinRoute| r.score.as_ref().map(|s| s.yaku.clone());
    names(a) == names(b)
}

struct Search<'a> {
    view: &'a PlayerView,
    rem: &'a mj_calc::RemainingEstimate,
    nodes: usize,
    /// 已展開的局面：（3k+1 張的手牌, 已摸的牌）
    seen: HashSet<(TileCounts, Vec<Tile>)>,
    /// 找到的路線（依要摸的牌與要打的牌去重）
    found: HashMap<(Vec<Tile>, Vec<Tile>), WinRoute>,
}

impl Search<'_> {
    /// 3k+1 張、向聽數 `s` 的手牌 `h`；已打掉 `discards`、已摸 `draws`（排序）。
    fn walk(&mut self, h: TileCounts, s: i8, discards: Vec<Tile>, draws: Vec<Tile>) {
        if self.nodes >= MAX_NODES || !self.seen.insert((h, draws.clone())) {
            return;
        }
        self.nodes += 1;
        for t in Tile::all() {
            let used = draws.iter().filter(|&&d| d == t).count() as f32;
            if h.get(t) >= 4 || self.rem[t.index()] - used < 1.0 {
                continue;
            }
            let mut h14 = h;
            h14.add(t);
            let mut drawn = draws.clone();
            drawn.push(t);
            drawn.sort();
            if s == 0 {
                if shanten(&h14) == -1 {
                    self.record(h14, &discards, drawn);
                }
                continue;
            }
            // 摸了 t 之後，打一張讓向聽數變成 s − 1 的牌（不打剛摸的牌：那等於沒摸）。
            for d in Tile::all().filter(|&d| d != t && h14.get(d) > 0) {
                let mut next = h14;
                next.remove(d);
                if shanten(&next) == s - 1 {
                    let mut ds = discards.clone();
                    ds.push(d);
                    self.walk(next, s - 1, ds, drawn.clone());
                }
            }
        }
    }

    /// 記下一個完成形：每張要摸的牌都試當和牌張，取點數最高的。
    fn record(&mut self, done: TileCounts, discards: &[Tile], draws: Vec<Tile>) {
        let mut sorted_discards = discards.to_vec();
        if sorted_discards.len() > 1 {
            // 第一張（現在要打的）保持在最前面，其餘排序。
            sorted_discards[1..].sort();
        }
        let key = (draws.clone(), sorted_discards.clone());
        if self.found.contains_key(&key) {
            return;
        }
        let riichi = i_am_closed(self.view) && !self.view.riichi[usize::from(self.view.me)];
        let mut best: Option<(Tile, Option<Score>, i32)> = None;
        let mut tried: Vec<Tile> = draws.clone();
        tried.dedup();
        for w in tried {
            let mut before = done;
            before.remove(w);
            let score = win_value(self.view, &before, w, false, riichi);
            let points = score.as_ref().map_or(0, |s| ron_points(self.view, s));
            if best.as_ref().is_none_or(|b| points > b.2) {
                best = Some((w, score, points));
            }
        }
        let Some((win_tile, score, points)) = best else {
            return;
        };
        let mut chance = 1.0;
        let mut i = 0;
        while i < draws.len() {
            let t = draws[i];
            let k = draws[i..].iter().take_while(|&&d| d == t).count();
            chance *= combinations(self.rem[t.index()].floor() as u32, k as u32);
            i += k;
        }
        self.found.insert(
            key,
            WinRoute {
                discards: sorted_discards,
                draws,
                hand: done.tiles().collect(),
                win_tile,
                score,
                points,
                assumes_riichi: riichi,
                chance,
            },
        );
    }
}

/// 組合數 C(n, k)。
fn combinations(n: u32, k: u32) -> f64 {
    if k > n {
        return 0.0;
    }
    (0..k).fold(1.0, |acc, i| acc * f64::from(n - i) / f64::from(i + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mj_core::{Meld, MeldKind, Pai, parse_pais};

    fn view(hand: &str) -> PlayerView {
        let pais = parse_pais(hand).unwrap();
        PlayerView {
            me: 1,
            round_wind: 0,
            kyoku: 0,
            honba: 0,
            riichi_sticks: 0,
            scores: [25_000; 4],
            dora_indicators: vec!["1z".parse().unwrap()],
            hand: TileCounts::from_tiles(pais.iter().map(|p| p.tile)),
            drawn: None,
            hand_pais: pais,
            melds: Default::default(),
            rivers: Default::default(),
            riichi: [false; 4],
            tiles_left_in_wall: 60,
            total_kyoku: 8,
            last_discard: None,
        }
    }

    fn t(s: &str) -> Tile {
        s.parse().unwrap()
    }

    /// 聽牌（兩面 5s・8s）：兩條路線，都是立直・斷么九・平和 3 番 30 符 = 3900；
    /// 寶牌指示牌是 8s（看得到一張，剩 3 張），所以 5s 的機會較大、排前面。
    #[test]
    fn tenpai_routes() {
        let mut v = view("234m456p345m22p67s");
        v.dora_indicators = vec![t("8s")];
        let r = win_routes(&v, &v.hand, 10);
        assert_eq!(r.len(), 2, "{r:?}");
        assert_eq!(r[0].draws, vec![t("5s")]);
        assert_eq!(r[1].draws, vec![t("8s")]);
        assert_eq!(r[0].chance, 4.0);
        assert_eq!(r[1].chance, 3.0);
        for x in &r {
            assert_eq!(x.points, 3900);
            assert!(x.assumes_riichi && x.discards.is_empty());
            let s = x.score.as_ref().unwrap();
            assert_eq!((s.han, s.fu), (3, 30));
        }
    }

    /// 一向聽：每條路線摸 2 張、打 1 張，完成形確實是和牌形，而且等於「原手牌 − 打掉 + 摸到」。
    #[test]
    fn one_shanten_routes_are_consistent() {
        let v = view("234m456p678s2p45s9m");
        let start = v.hand;
        assert_eq!(shanten(&start), 1);
        let r = win_routes(&v, &start, 50);
        assert!(!r.is_empty());
        for x in &r {
            assert_eq!(x.draws.len(), 2);
            assert_eq!(x.discards.len(), 1);
            let mut h = start;
            for &d in &x.discards {
                h.remove(d);
            }
            for &d in &x.draws {
                h.add(d);
            }
            assert_eq!(h.tiles().collect::<Vec<_>>(), x.hand);
            assert!(mj_calc::is_agari(&h));
            assert!(x.draws.contains(&x.win_tile));
        }
        // 依機會排序。
        assert!(
            r.windows(2)
                .all(|w| w[0].chance >= w[1].chance || w[1].points > 0)
        );
    }

    /// 摸牌後（14 張）：第一張要打的牌就是向聽數最低的打法之一。
    #[test]
    fn after_draw_includes_the_discard() {
        let v = view("234m456p678s22p45s9m");
        let r = win_routes(&v, &v.hand, 10);
        assert!(!r.is_empty());
        assert!(
            r.iter().all(|x| x.discards.first() == Some(&t("9m"))),
            "{r:?}"
        );
        assert_eq!(r[0].draws.len(), 1);
    }

    /// 副露沒有役：路線還在，但點數為 0、沒有計分。
    #[test]
    fn open_hand_without_yaku() {
        let mut v = view("234m456p22p45s");
        v.melds[1].push(Meld {
            kind: MeldKind::Chi,
            tiles: vec![p("7s"), p("8s"), p("9s")],
            called: Some(p("7s")),
            from: Some(0),
        });
        let r = win_routes(&v, &v.hand, 10);
        assert!(!r.is_empty());
        assert!(
            r.iter()
                .all(|x| x.points == 0 && x.score.is_none() && !x.assumes_riichi)
        );
    }

    /// 兩向聽也算得出來；三向聽以上不算。
    #[test]
    fn shanten_limits() {
        let v = view("234m456p678s19m19p");
        assert_eq!(shanten(&v.hand), 2);
        let r = win_routes(&v, &v.hand, 8);
        assert!(!r.is_empty() && r.len() <= 9);
        assert!(r.iter().all(|x| x.draws.len() == 3));
        let far = view("159m159p159s1234z");
        assert!(win_routes(&far, &far.hand, 8).is_empty());
    }

    fn p(s: &str) -> Pai {
        s.parse().unwrap()
    }
}
