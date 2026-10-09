//! `mj selfplay`：讓機器人或策略大量平行對戰，並輸出統計
//! （各方的平均順位、和牌率、放銃率，以及和牌、流局、役種次數、速度……）。
//! 用 `fuzz` 建置設定（release 速度＋除錯檢查）執行時，每回合還會檢查牌數與點數守恆，
//! 所以這同時也是引擎的模糊測試工具。

use mj_engine::{Event, GameConfig, HandEnd, Player, run_game};
use mj_rules::riichi::RiichiConfig;
use mj_sim::bots::{GreedyBot, RandomBot};
use mj_strategy::find_strategy_spec;
use mj_strategy::framework::Strategy;
use mj_strategy::framework::explain::{display_width, pad, pad_left};
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// 一個座位要放什麼玩家。
#[derive(Clone)]
enum SeatSpec {
    /// 貪婪機器人（純牌效，隨機鳴牌）
    Greedy,
    /// 隨機機器人
    Random,
    /// strategy-list 中的策略
    Strategy(Arc<dyn Strategy>),
    /// 說 mjai 協定的外部程式（程式與參數）
    Mjai(Vec<String>),
}

/// 一個座位設定與它在統計中的名稱。
#[derive(Clone)]
pub(crate) struct Seat {
    pub(crate) label: String,
    spec: SeatSpec,
}

/// 解析一個參賽者：`greedy`、`random`、`mjai:<外部程式 參數…>`（說 mjai 協定的 AI；行程在對局之間重複使用）
/// 或策略名稱「規則／策略」（可加 `@參數覆寫`，見 `find_strategy_spec`；
/// 座位清單以逗號分隔，所以這裡的多個參數改用分號分隔）。
pub(crate) fn parse_entrant(s: &str) -> Result<Seat, String> {
    match s.trim() {
        "greedy" => Ok(Seat {
            label: "greedy".into(),
            spec: SeatSpec::Greedy,
        }),
        "random" => Ok(Seat {
            label: "random".into(),
            spec: SeatSpec::Random,
        }),
        mjai if mjai.starts_with("mjai:") => {
            let cmd: Vec<String> = mjai["mjai:".len()..]
                .split_whitespace()
                .map(String::from)
                .collect();
            if cmd.is_empty() {
                return Err("mjai: 後面要接外部程式，例如 mjai:./mortal.sh".into());
            }
            Ok(Seat {
                label: mjai.to_string(),
                spec: SeatSpec::Mjai(cmd),
            })
        }
        name => find_strategy_spec(name).map(|st| Seat {
            label: name.to_string(),
            spec: SeatSpec::Strategy(Arc::from(st)),
        }),
    }
}

/// 解析座位設定：以逗號分隔的四項，每項見 [`parse_entrant`]。
fn parse_seats(spec: &str) -> Result<Vec<Seat>, String> {
    let seats: Vec<Seat> = spec
        .split(',')
        .map(parse_entrant)
        .collect::<Result<_, String>>()?;
    if seats.len() != 4 {
        return Err(format!("--seats 需要 4 項，目前是 {} 項", seats.len()));
    }
    Ok(seats)
}

/// 建立玩家（機器人的 seed 由對局 seed 推導，結果可重現）。
pub(crate) fn make_player(seat: &Seat, seed: u64, i: u64) -> Box<dyn Player> {
    let s = seed.wrapping_mul(31).wrapping_add(i);
    match &seat.spec {
        SeatSpec::Greedy => Box::new(GreedyBot::new(s, 0.3)),
        SeatSpec::Random => Box::new(RandomBot::new(s)),
        SeatSpec::Strategy(st) => st.player(false, None),
        SeatSpec::Mjai(cmd) => {
            let bot = mj_mjai::PooledBot::checkout(cmd).unwrap_or_else(|e| panic!("{e}"));
            let names = std::array::from_fn(|_| seat.label.clone());
            Box::new(mj_mjai::MjaiPlayer::new(bot, names))
        }
    }
}

/// 每一方（同名的座位合併計算）的統計。
#[derive(Default, Clone)]
struct PlayerStats {
    /// 參加的對局數（以座位計，同一方坐兩個位置就算兩次）
    games: u64,
    /// 各名次的次數（第 1–4 名）
    ranks: [u64; 4],
    /// 名次平方和（計算標準差用，名次以 1–4 計）
    rank_sq: u64,
    /// 打過的局數
    hands: u64,
    /// 和牌次數
    wins: u64,
    /// 放銃次數
    deal_ins: u64,
    /// 立直次數
    riichi: u64,
    /// 鳴牌次數（含暗槓）
    calls: u64,
    /// 最終點數總和
    score_sum: i64,
}

impl PlayerStats {
    fn merge(&mut self, o: &PlayerStats) {
        self.games += o.games;
        for i in 0..4 {
            self.ranks[i] += o.ranks[i];
        }
        self.rank_sq += o.rank_sq;
        self.hands += o.hands;
        self.wins += o.wins;
        self.deal_ins += o.deal_ins;
        self.riichi += o.riichi;
        self.calls += o.calls;
        self.score_sum += o.score_sum;
    }

    /// 平均順位與其 95% 信賴區間的半寬。
    fn avg_rank(&self) -> (f64, f64) {
        let n = self.games as f64;
        let sum: u64 = self
            .ranks
            .iter()
            .enumerate()
            .map(|(i, &c)| (i as u64 + 1) * c)
            .sum();
        let mean = sum as f64 / n;
        let var = (self.rank_sq as f64 / n - mean * mean).max(0.0);
        (mean, 1.96 * (var / n).sqrt())
    }
}

/// 統計數字（各執行緒分別累計，最後合併）。
#[derive(Default)]
struct Stats {
    /// 對局數
    games: u64,
    /// 局數
    hands: u64,
    /// 自摸次數
    tsumo: u64,
    /// 榮和次數
    ron: u64,
    /// 雙響的局數
    multi_ron_hands: u64,
    /// 各流局原因的次數
    draws: BTreeMap<String, u64>,
    /// 各役種出現次數
    yaku: BTreeMap<String, u64>,
    /// 立直次數
    riichi: u64,
    /// 鳴牌次數（吃碰槓）
    calls: u64,
    /// 進入延長風場（西入）的對局數
    extended: u64,
    /// 有人被擊飛的對局數
    busted: u64,
    /// 各方的統計（依名稱）
    players: BTreeMap<String, PlayerStats>,
}

impl Stats {
    /// 合併兩份統計。
    fn merge(mut self, o: Stats) -> Stats {
        self.games += o.games;
        self.hands += o.hands;
        self.tsumo += o.tsumo;
        self.ron += o.ron;
        self.multi_ron_hands += o.multi_ron_hands;
        self.riichi += o.riichi;
        self.calls += o.calls;
        self.extended += o.extended;
        self.busted += o.busted;
        for (k, v) in o.draws {
            *self.draws.entry(k).or_default() += v;
        }
        for (k, v) in o.yaku {
            *self.yaku.entry(k).or_default() += v;
        }
        for (k, v) in &o.players {
            self.players.entry(k.clone()).or_default().merge(v);
        }
        self
    }

    /// 從一場對局的事件紀錄累計統計。`labels[s]` = 座位 s 是哪一方。
    fn record(
        &mut self,
        log: &[Event],
        regular_hands: u8,
        labels: &[String; 4],
        ranks: &[u8; 4],
        scores: &[i32; 4],
    ) {
        self.games += 1;
        let mut per: [PlayerStats; 4] = Default::default();
        let mut wins_this_hand = 0;
        for ev in log {
            match ev {
                Event::StartKyoku {
                    round_wind, kyoku, ..
                } => {
                    self.hands += 1;
                    per.iter_mut().for_each(|p| p.hands += 1);
                    wins_this_hand = 0;
                    // 延長風場的第一局（例如西一局）出現 = 這場有西入。
                    if *kyoku == 0 && *round_wind * 4 == regular_hands {
                        self.extended += 1;
                    }
                }
                Event::HandEnd {
                    end:
                        HandEnd::Win {
                            winner,
                            from,
                            score,
                            ..
                        },
                    ..
                } => {
                    wins_this_hand += 1;
                    if wins_this_hand == 2 {
                        self.multi_ron_hands += 1;
                    }
                    per[usize::from(*winner)].wins += 1;
                    if winner == from {
                        self.tsumo += 1
                    } else {
                        self.ron += 1;
                        // 雙響時同一張牌放銃只算一次。
                        if wins_this_hand == 1 {
                            per[usize::from(*from)].deal_ins += 1;
                        }
                    }
                    for (y, _) in &score.yaku {
                        *self.yaku.entry(format!("{y:?}")).or_default() += 1;
                    }
                }
                Event::HandEnd {
                    end:
                        HandEnd::Draw {
                            reason, nagashi, ..
                        },
                    ..
                } => {
                    let key = if !nagashi.is_empty() {
                        "NagashiMangan".to_string()
                    } else {
                        format!("{reason:?}")
                    };
                    *self.draws.entry(key).or_default() += 1;
                }
                Event::RiichiAccepted { seat } => {
                    self.riichi += 1;
                    per[usize::from(*seat)].riichi += 1;
                }
                Event::Call { seat, .. } => {
                    self.calls += 1;
                    per[usize::from(*seat)].calls += 1;
                }
                Event::EndGame { scores } if scores.iter().any(|&s| s < 0) => self.busted += 1,
                _ => {}
            }
        }
        for s in 0..4 {
            let p = &mut per[s];
            p.games = 1;
            p.ranks[usize::from(ranks[s])] = 1;
            p.rank_sq = u64::from(ranks[s] + 1).pow(2);
            p.score_sum = i64::from(scores[s]);
            self.players.entry(labels[s].clone()).or_default().merge(p);
        }
    }
}

/// 執行 `games` 場對局並印出統計。
///
/// `seats`：四個座位的設定（見 [`parse_seats`]）。`rotate` = 每場輪換座位
/// （第 g 場的座位 s 使用第 (s + g) mod 4 項設定），消除座位（起家）造成的偏差。
/// 任何一場出現引擎錯誤（例如不合法動作）都會回傳錯誤。
pub fn run(
    games: u64,
    seed: u64,
    seats: &str,
    rotate: bool,
    dump: Option<PathBuf>,
) -> Result<(), String> {
    let seats = parse_seats(seats)?;
    let rules = RiichiConfig::TENHOU;
    let regular = if rules.hanchan { 8 } else { 4 };
    let start = Instant::now();
    // 以 rayon 平行執行各場對局。
    let stats = (0..games)
        .into_par_iter()
        .map(|g| {
            let cfg = GameConfig::new(rules.clone(), seed.wrapping_add(g));
            let shift = if rotate { g as usize % 4 } else { 0 };
            let at: [&Seat; 4] = std::array::from_fn(|s| &seats[(s + shift) % 4]);
            let mut ps: [Box<dyn Player>; 4] =
                std::array::from_fn(|s| make_player(at[s], cfg.seed, s as u64));
            let result = run_game(&cfg, &mut ps, true)
                .map_err(|e| format!("第 {g} 場（seed {}）：{e}", cfg.seed))?;
            if g == 0 {
                if let Some(path) = &dump {
                    let lines: Vec<String> = result
                        .log
                        .iter()
                        .map(|e| serde_json::to_string(e).unwrap())
                        .collect();
                    std::fs::write(path, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
                }
            }
            let labels: [String; 4] = std::array::from_fn(|s| at[s].label.clone());
            let mut s = Stats::default();
            s.record(&result.log, regular, &labels, &result.ranks, &result.scores);
            Ok::<_, String>(s)
        })
        .try_reduce(Stats::default, |a, b| Ok(a.merge(b)))?;
    let secs = start.elapsed().as_secs_f64();

    let wins = stats.tsumo + stats.ron;
    let draws: u64 = stats.draws.values().sum();
    println!(
        "對局 {}  局數 {}（平均每場 {:.1} 局）  耗時 {secs:.2} 秒（每分鐘 {:.0} 場半莊）",
        stats.games,
        stats.hands,
        stats.hands as f64 / stats.games as f64,
        stats.games as f64 / secs * 60.0
    );
    println!();
    // 欄位：名稱、座位場數、平均順位（95% 信賴區間）、各名次比例、和牌率、放銃率、立直率、每局鳴牌數、平均點數。
    let name_width = stats
        .players
        .keys()
        .map(|k| display_width(k))
        .max()
        .unwrap_or(4)
        .max(8)
        + 2;
    let widths = [name_width, 10, 18, 24, 8, 8, 8, 9, 10];
    let header = [
        "玩家",
        "座位場數",
        "平均順位",
        "1/2/3/4 位率(%)",
        "和牌率",
        "放銃率",
        "立直率",
        "鳴牌/局",
        "平均點數",
    ];
    let row = |cells: Vec<String>| {
        cells
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if i == 0 {
                    pad(c, widths[i])
                } else {
                    pad_left(c, widths[i])
                }
            })
            .collect::<String>()
    };
    println!("{}", row(header.iter().map(|s| s.to_string()).collect()));
    for (label, p) in &stats.players {
        let (mean, ci) = p.avg_rank();
        let pct = |x: u64, n: u64| 100.0 * x as f64 / n.max(1) as f64;
        let dist = p
            .ranks
            .iter()
            .map(|&c| format!("{:.1}", pct(c, p.games)))
            .collect::<Vec<_>>()
            .join("/");
        println!(
            "{}",
            row(vec![
                label.clone(),
                p.games.to_string(),
                format!("{mean:.3}±{ci:.3}"),
                dist,
                format!("{:.1}%", pct(p.wins, p.hands)),
                format!("{:.1}%", pct(p.deal_ins, p.hands)),
                format!("{:.1}%", pct(p.riichi, p.hands)),
                format!("{:.2}", p.calls as f64 / p.hands.max(1) as f64),
                format!("{:.0}", p.score_sum as f64 / p.games as f64),
            ])
        );
    }
    println!();
    println!(
        "和牌 {wins}（自摸 {}、榮和 {}、雙響 {} 局）  流局 {draws}  立直 {}  鳴牌 {}  西入 {} 場  擊飛 {} 場",
        stats.tsumo,
        stats.ron,
        stats.multi_ron_hands,
        stats.riichi,
        stats.calls,
        stats.extended,
        stats.busted
    );
    println!("流局：{}", fmt_map(&stats.draws));
    println!("役種：{}", fmt_map(&stats.yaku));
    Ok(())
}

/// 把計數表依次數由多到少排成一行文字。
fn fmt_map(m: &BTreeMap<String, u64>) -> String {
    let mut v: Vec<_> = m.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    v.iter()
        .map(|(k, n)| format!("{k}={n}"))
        .collect::<Vec<_>>()
        .join(", ")
}
