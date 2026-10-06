//! 規則情境測試：用手工排好的牌山，逐條驗證特定規則。
//! 玩家照著簡短的腳本行動，其餘時候一律摸切、跳過，
//! 所以每個測試只控制它要測的那幾個決定。
//!
//! 座位 0 是莊家（東家），1 是南家（莊家的下家），依此類推。

use mj_core::{Pai, parse_pais};
use mj_engine::{
    Action, DrawReason, Event, GameConfig, GameResult, HandEnd, Player, PlayerView, Seat, full_set,
    run_game,
};
use mj_rules::riichi::{RiichiConfig, Yaku};
use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// 各情境共用的王牌：嶺上牌 發×4、寶牌指示牌 中（寶牌 = 白）、裏寶牌指示牌 白（裏寶牌 = 發）。
/// 測試用的手牌都不含白、發，所以寶牌不會影響預期點數。
const DEAD: &str = "6666z7777z5555z33z";

/// 排牌山：四家配牌（從莊家開始；列出的牌之外隨機補到 13 張），
/// 接著依序是指定的摸牌（莊家、南家、西家、北家輪流），再來是隨機的其餘牌，最後是王牌。
fn wall(hands: [Option<&str>; 4], draws: &str, seed: u64) -> Vec<Pai> {
    wall_with_dead(hands, draws, DEAD, seed)
}

/// 同 [`wall`]，但可以自訂王牌。
fn wall_with_dead(hands: [Option<&str>; 4], draws: &str, dead: &str, seed: u64) -> Vec<Pai> {
    let mut pool = full_set(3);
    let mut take = |s: &str| -> Vec<Pai> {
        parse_pais(s)
            .unwrap()
            .into_iter()
            .map(|p| {
                let i = pool
                    .iter()
                    .position(|&q| q == p)
                    .unwrap_or_else(|| panic!("no {p} left for {s}"));
                pool.remove(i)
            })
            .collect()
    };
    let mut fixed: Vec<Vec<Pai>> = hands
        .iter()
        .map(|h| h.map(&mut take).unwrap_or_default())
        .collect();
    let draws = take(draws);
    let dead = take(dead);
    pool.shuffle(&mut ChaCha8Rng::seed_from_u64(seed));
    let mut out = Vec::new();
    for h in &mut fixed {
        assert!(h.len() <= 13);
        let missing = 13 - h.len();
        h.extend(pool.drain(..missing));
        out.extend(h.iter().copied());
    }
    out.extend(draws);
    out.append(&mut pool);
    out.extend(dead);
    assert_eq!(out.len(), 136);
    out
}

/// 每次詢問玩家時提供的合法動作清單（依時間順序），測試用來檢查「有沒有提供某個選項」。
type Offers = Rc<RefCell<Vec<(Seat, Vec<Action>)>>>;

/// 腳本玩家：腳本中的下一個動作變成合法時就執行它；否則摸切、跳過。
struct Scripted {
    script: VecDeque<Action>,
    offers: Offers,
}

impl Player for Scripted {
    fn act(&mut self, view: &PlayerView, legal: &[Action]) -> Action {
        self.offers.borrow_mut().push((view.me, legal.to_vec()));
        if let Some(a) = self.script.front() {
            if legal.contains(a) {
                return self.script.pop_front().unwrap();
            }
        }
        if let Some(a) = legal.iter().find(|a| {
            matches!(
                a,
                Action::Discard {
                    tsumogiri: true,
                    ..
                }
            )
        }) {
            return a.clone();
        }
        if legal.contains(&Action::Pass) {
            return Action::Pass;
        }
        legal
            .iter()
            .find(|a| matches!(a, Action::Discard { .. }))
            .cloned()
            .unwrap_or_else(|| legal[0].clone())
    }
}

/// 用指定牌山與四家腳本打一局。
fn play(wall: Vec<Pai>, scripts: [Vec<Action>; 4]) -> (GameResult, Offers) {
    let offers: Offers = Rc::default();
    let mut players: [Box<dyn Player>; 4] = scripts.map(|s| {
        Box::new(Scripted {
            script: s.into(),
            offers: offers.clone(),
        }) as Box<dyn Player>
    });
    let mut cfg = GameConfig::new(RiichiConfig::TENHOU, 0);
    cfg.preset_walls = vec![wall];
    cfg.max_hands = Some(1);
    (
        run_game(&cfg, &mut players, true).expect("legal play"),
        offers,
    )
}

/// 取出牌譜中的結算（和牌或流局）與點數增減。
fn ends(r: &GameResult) -> Vec<(HandEnd, [i32; 4])> {
    r.log
        .iter()
        .filter_map(|e| match e {
            Event::HandEnd { end, deltas } => Some((end.clone(), *deltas)),
            _ => None,
        })
        .collect()
}

fn pai(s: &str) -> Pai {
    s.parse().unwrap()
}

/// 和牌的役種。
fn yaku_of(end: &HandEnd) -> Vec<Yaku> {
    match end {
        HandEnd::Win { score, .. } => score.yaku.iter().map(|(y, _)| *y).collect(),
        _ => panic!("not a win: {end:?}"),
    }
}

/// 南家（座位 1）的手牌：聽 3m/6m 兩面，有平和。
const PINFU_36M: &str = "45m123p456p78999s";

/// 天和：莊家第一次摸牌就自摸，16000 all。
#[test]
fn tenhou() {
    let w = wall([Some("123m456p789s1122z"), None, None, None], "2z", 1);
    let (r, _) = play(w, [vec![Action::Tsumo], vec![], vec![], vec![]]);
    let e = ends(&r);
    assert_eq!(yaku_of(&e[0].0), [Yaku::Tenhou]);
    assert_eq!(e[0].1, [48000, -16000, -16000, -16000]);
}

/// 莊家打出 3m，南家榮和平和 1000 點。
#[test]
fn ron_on_a_discard() {
    let w = wall([None, Some(PINFU_36M), None, None], "3m", 2);
    let (r, _) = play(w, [vec![], vec![Action::Ron], vec![], vec![]]);
    let e = ends(&r);
    assert_eq!(e.len(), 1);
    assert_eq!(yaku_of(&e[0].0), [Yaku::Pinfu]);
    assert_eq!(e[0].1, [-1000, 1000, 0, 0]);
}

/// 捨牌振聽：自己打過聽牌張後不能榮和。
#[test]
fn discard_furiten_blocks_ron() {
    // 南家摸到自己的聽牌張 6m 卻打掉；之後莊家打 3m 也不能榮和。
    let w = wall([None, Some(PINFU_36M), None, None], "1z6m2z3z3m", 3);
    let (r, offers) = play(w, [vec![], vec![], vec![], vec![]]);
    let seat1: Vec<_> = offers
        .borrow()
        .iter()
        .filter(|(s, _)| *s == 1)
        .map(|(_, l)| l.clone())
        .collect();
    assert!(
        seat1[0].contains(&Action::Tsumo),
        "drawing the wait offers tsumo"
    );
    assert!(
        seat1.iter().skip(1).all(|l| !l.contains(&Action::Ron)),
        "furiten seat was offered ron"
    );
    assert!(
        r.log
            .iter()
            .any(|e| matches!(e, Event::Discard { seat: 0, pai: p, .. } if *p == pai("3m")))
    );
}

/// 雙響：南家、西家同時榮和莊家打出的 3m，分別支付。
#[test]
fn double_ron_pays_both() {
    let w = wall(
        [
            None,
            Some(PINFU_36M),
            Some("111789p111z444z3m"),
            Some("147m239p369s2233z"),
        ],
        "3m",
        4,
    );
    let (r, _) = play(w, [vec![], vec![Action::Ron], vec![Action::Ron], vec![]]);
    let e = ends(&r);
    assert_eq!(e.len(), 2);
    assert_eq!(e[0].1, [-1000, 1000, 0, 0]);
    // 場風東＋三暗刻，3 番 60 符（20+10+8+8+8+單騎 2 = 56 → 60）= 7700。
    let mut y = yaku_of(&e[1].0);
    y.sort();
    assert_eq!(y, [Yaku::RoundWind, Yaku::Sanankou]);
    assert_eq!(e[1].1, [-7700, 0, 7700, 0]);
}

/// 食替：用 23m 吃 1m 後，不能打 1m（現物）也不能打 4m（筋）。
#[test]
fn kuikae_after_chi() {
    let w = wall([None, Some("1234m456p789p112z"), None, None], "1m", 5);
    let chi = Action::Chi {
        consumed: [pai("2m"), pai("3m")],
    };
    let (_, offers) = play(w, [vec![], vec![chi.clone()], vec![], vec![]]);
    let offers = offers.borrow();
    let i = offers
        .iter()
        .position(|(s, l)| *s == 1 && l.contains(&chi))
        .expect("chi offered");
    let (_, after) = &offers[i + 1];
    let discards: Vec<String> = after
        .iter()
        .filter_map(|a| match a {
            Action::Discard { pai, .. } => Some(pai.to_string()),
            _ => None,
        })
        .collect();
    assert!(
        !discards.contains(&"1m".into()) && !discards.contains(&"4m".into()),
        "{discards:?}"
    );
    assert!(discards.contains(&"1z".into()));
}

/// 荒牌流局：只有南家聽牌，收 3000、其他三家各付 1000。
#[test]
fn exhaustive_draw_pays_noten() {
    let w = wall(
        [
            Some("1357s2468p1357m1z"),
            Some(PINFU_36M),
            Some("5566778899p124z"),
            Some("147m239p369s2233z"),
        ],
        "",
        6,
    );
    let (r, _) = play(w, [vec![], vec![], vec![], vec![]]);
    let e = ends(&r);
    match &e[0].0 {
        HandEnd::Draw { reason, tenpai, .. } => {
            assert_eq!(*reason, DrawReason::Exhaustive);
            assert_eq!(*tenpai, [false, true, false, false]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(e[0].1, [-1000, 3000, -1000, -1000]);
}

/// 兩立直＋一發＋平和榮和，並拿回自己的立直棒。
#[test]
fn double_riichi_ippatsu_ron() {
    let riichi = Action::Riichi {
        pai: pai("1z"),
        tsumogiri: true,
    };
    let w = wall([None, Some(PINFU_36M), None, None], "2z1z3m", 7);
    let (r, _) = play(w, [vec![], vec![riichi, Action::Ron], vec![], vec![]]);
    let e = ends(&r);
    let mut y = yaku_of(&e[0].0);
    y.sort();
    assert_eq!(y, [Yaku::Ippatsu, Yaku::Pinfu, Yaku::DoubleRiichi]);
    // 4 番 30 符 = 7700，加上供託 1000。
    assert_eq!(e[0].1, [0, 8700, -7700, 0]);
    assert_eq!(r.scores[1], 25_000 - 1000 + 8700);
}

/// 四風連打：第一巡四家都打東 → 途中流局。
#[test]
fn suufon_renda_aborts() {
    let (r, _) = play(
        wall([None; 4], "1z1z1z1z", 8),
        [vec![], vec![], vec![], vec![]],
    );
    match &ends(&r)[0].0 {
        HandEnd::Draw { reason, .. } => assert_eq!(*reason, DrawReason::SuufonRenda),
        other => panic!("{other:?}"),
    }
}

/// 包牌：南家依序碰白、發、中完成大三元，第三組（中）是西家打的 → 自摸時由西家支付全額。
#[test]
fn pao_on_daisangen_tsumo() {
    let w = wall_with_dead(
        [None, Some("556677z123m99s19p"), None, None],
        "5z6z7z1s2s3s9s",
        "1111z2222z3333z44z",
        9,
    );
    let d = |s: &str| Action::Discard {
        pai: pai(s),
        tsumogiri: false,
    };
    let pon = |s: &str| Action::Pon {
        consumed: [pai(s), pai(s)],
    };
    let script = vec![
        pon("5z"),
        d("1p"),
        pon("6z"),
        d("9p"),
        pon("7z"),
        d("9s"),
        Action::Tsumo,
    ];
    let (r, _) = play(w, [vec![], script, vec![], vec![]]);
    let e = ends(&r);
    assert_eq!(yaku_of(&e[0].0), [Yaku::Daisangen]);
    // 西家（座位 2）放出第三組三元牌，自摸的全額由他支付。
    assert_eq!(e[0].1, [0, 32000, -32000, 0]);
}

/// 搶槓：西家加槓 6m，聽 6m 的南家榮和（搶槓＋平和 = 2000）。
#[test]
fn chankan_robs_an_added_kan() {
    let w = wall(
        [None, Some(PINFU_36M), Some("66m"), None],
        "1z2z3z6m4z1s2s6m",
        10,
    );
    let (r, _) = play(
        w,
        [
            vec![],
            vec![Action::Pass, Action::Ron],
            vec![
                Action::Pon {
                    consumed: [pai("6m"), pai("6m")],
                },
                Action::Kakan { pai: pai("6m") },
            ],
            vec![],
        ],
    );
    let e = ends(&r);
    let mut y = yaku_of(&e[0].0);
    y.sort();
    assert_eq!(y, [Yaku::Pinfu, Yaku::Chankan]);
    assert_eq!(e[0].1, [0, 2000, -2000, 0]);
}

/// 立直振聽：立直後放過一次和牌，之後即使自己再打過牌，也永遠不能榮和。
#[test]
fn riichi_furiten_after_passing_a_win() {
    let riichi = Action::Riichi {
        pai: pai("1z"),
        tsumogiri: true,
    };
    // 南家放過 3m，接著自己又打了一張牌（一般的同巡振聽到這裡就會解除），
    // 然後西家才打出 6m。
    let w = wall([None, Some(PINFU_36M), None, None], "2z1z3m4z1s2s6m", 11);
    let (r, offers) = play(w, [vec![], vec![riichi, Action::Pass], vec![], vec![]]);
    let order: Vec<(u8, String)> = r
        .log
        .iter()
        .filter_map(|e| match e {
            Event::Discard { seat, pai, .. } => Some((*seat, pai.to_string())),
            _ => None,
        })
        .take(7)
        .collect();
    assert_eq!(order[5], (1, "2s".to_string()));
    assert_eq!(order[6], (2, "6m".to_string()));
    let seat1: Vec<_> = offers
        .borrow()
        .iter()
        .filter(|(s, _)| *s == 1)
        .map(|(_, l)| l.clone())
        .collect();
    let passed = seat1
        .iter()
        .position(|l| l.contains(&Action::Ron))
        .expect("ron offered on 3m");
    assert!(
        seat1[passed + 1..]
            .iter()
            .all(|l| !l.contains(&Action::Ron)),
        "ron offered after riichi furiten"
    );
}

/// 流局滿貫：南家整局摸到並打出的全是幺九牌，流局時以滿貫計算。
#[test]
fn nagashi_mangan() {
    // 南家整局只摸到幺九牌（並摸切）。
    let mut pool = full_set(3);
    let mut dead = Vec::new();
    for p in parse_pais(DEAD).unwrap() {
        let i = pool.iter().position(|&q| q == p).unwrap();
        dead.push(pool.remove(i));
    }
    pool.shuffle(&mut ChaCha8Rng::seed_from_u64(12));
    let (mut yaochu, mut rest): (Vec<Pai>, Vec<Pai>) =
        pool.into_iter().partition(|p| p.tile.is_yaochu());
    let seat1_draws: Vec<Pai> = yaochu.drain(..18).collect();
    rest.extend(yaochu);
    rest.shuffle(&mut ChaCha8Rng::seed_from_u64(13));
    let mut wall: Vec<Pai> = rest.drain(..52).collect();
    let mut s1 = seat1_draws.into_iter();
    for i in 0..70 {
        wall.push(if i % 4 == 1 {
            s1.next().unwrap()
        } else {
            rest.pop().unwrap()
        });
    }
    wall.extend(dead);

    let (r, _) = play(wall, [vec![], vec![], vec![], vec![]]);
    let e = ends(&r);
    match &e[0].0 {
        HandEnd::Draw {
            reason, nagashi, ..
        } => {
            assert_eq!(*reason, DrawReason::Exhaustive);
            assert_eq!(*nagashi, vec![1]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(e[0].1, [-4000, 8000, -2000, -2000]);
}
