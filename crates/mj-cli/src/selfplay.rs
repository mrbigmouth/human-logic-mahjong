//! `mj selfplay`：讓參考機器人大量平行對戰，並輸出統計（和牌、流局、役種次數、速度……）。
//! 用 `fuzz` 建置設定（release 速度＋除錯檢查）執行時，每回合還會檢查牌數與點數守恆，
//! 所以這同時也是引擎的模糊測試工具。

use clap::ValueEnum;
use mj_engine::{Event, GameConfig, HandEnd, Player, run_game};
use mj_rules::riichi::RiichiConfig;
use mj_sim::bots::{GreedyBot, RandomBot};
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

/// 機器人組合。
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Bots {
    /// 四個貪婪機器人（和牌、立直多）
    Greedy,
    /// 四個隨機機器人（鳴牌、槓多，和牌少）
    Random,
    /// 兩種各兩個
    Mixed,
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
        self
    }

    /// 從一場對局的事件紀錄累計統計。
    fn record(&mut self, log: &[Event], regular_hands: u8) {
        self.games += 1;
        let mut wins_this_hand = 0;
        for ev in log {
            match ev {
                Event::StartKyoku {
                    round_wind, kyoku, ..
                } => {
                    self.hands += 1;
                    wins_this_hand = 0;
                    // 延長風場的第一局（例如西一局）出現 = 這場有西入。
                    if round_wind * 4 + kyoku >= regular_hands
                        && *kyoku == 0
                        && *round_wind * 4 == regular_hands
                    {
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
                    if winner == from {
                        self.tsumo += 1
                    } else {
                        self.ron += 1
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
                Event::RiichiAccepted { .. } => self.riichi += 1,
                Event::Call { .. } => self.calls += 1,
                Event::EndGame { scores } if scores.iter().any(|&s| s < 0) => self.busted += 1,
                _ => {}
            }
        }
    }
}

/// 依組合建立四個機器人（各自的 seed 由對局 seed 推導，結果可重現）。
fn players(bots: Bots, seed: u64) -> [Box<dyn Player>; 4] {
    let g = |i: u64| {
        Box::new(GreedyBot::new(seed.wrapping_mul(31).wrapping_add(i), 0.3)) as Box<dyn Player>
    };
    let r =
        |i: u64| Box::new(RandomBot::new(seed.wrapping_mul(31).wrapping_add(i))) as Box<dyn Player>;
    match bots {
        Bots::Greedy => [g(0), g(1), g(2), g(3)],
        Bots::Random => [r(0), r(1), r(2), r(3)],
        Bots::Mixed => [g(0), r(1), g(2), r(3)],
    }
}

/// 執行 `games` 場對局並印出統計。任何一場出現引擎錯誤（例如不合法動作）都會回傳錯誤。
pub fn run(games: u64, seed: u64, bots: Bots, dump: Option<PathBuf>) -> Result<(), String> {
    let rules = RiichiConfig::TENHOU;
    let regular = if rules.hanchan { 8 } else { 4 };
    let start = Instant::now();
    // 以 rayon 平行執行各場對局。
    let stats = (0..games)
        .into_par_iter()
        .map(|g| {
            let cfg = GameConfig::new(rules.clone(), seed.wrapping_add(g));
            let mut ps = players(bots, cfg.seed);
            let result = run_game(&cfg, &mut ps, true)
                .map_err(|e| format!("game {g} (seed {}): {e}", cfg.seed))?;
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
            let mut s = Stats::default();
            s.record(&result.log, regular);
            Ok::<_, String>(s)
        })
        .try_reduce(Stats::default, |a, b| Ok(a.merge(b)))?;
    let secs = start.elapsed().as_secs_f64();

    let wins = stats.tsumo + stats.ron;
    let draws: u64 = stats.draws.values().sum();
    println!(
        "games {}  hands {}  ({:.1} hands/game)",
        stats.games,
        stats.hands,
        stats.hands as f64 / stats.games as f64
    );
    println!(
        "time {secs:.2}s  ({:.0} games/s, {:.0} hanchan/min)",
        stats.games as f64 / secs,
        stats.games as f64 / secs * 60.0
    );
    println!(
        "wins {wins} (tsumo {}, ron {}, multi-ron hands {})  draws {draws}  riichi {}  calls {}",
        stats.tsumo, stats.ron, stats.multi_ron_hands, stats.riichi, stats.calls
    );
    println!(
        "games extended past the last regular hand {}  busted {}",
        stats.extended, stats.busted
    );
    println!("draws: {}", fmt_map(&stats.draws));
    println!("yaku: {}", fmt_map(&stats.yaku));
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
