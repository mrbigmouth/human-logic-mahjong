//! `mj review`：牌譜復盤。逐手比較玩家與策略的選擇，附策略的理由與反事實期望值，最後彙整失誤類型。
//! 加上 `--mjai` 時，另一個說 mjai 的 AI（例如 Mortal）也一起旁觀，列出它的選擇與三方一致率。

use mj_engine::Seat;
use mj_paifu::GameRecord;
use mj_review::{
    Agreement, GameReview, Options as ReviewOptions, Reviewed, agreement, review_game, summarize,
};
use mj_sim::counterfactual::{Metric, Options as CfOptions};
use mj_strategy::find_strategy_spec;
use mj_strategy::framework::explain::{pad, pad_left};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// 復盤設定。
pub struct Options {
    pub logs: Vec<PathBuf>,
    pub strategy: String,
    pub player: Option<String>,
    pub seat: Option<Seat>,
    pub all: bool,
    pub show: String,
    pub rollouts: usize,
    pub metric: Metric,
    pub omniscient: bool,
    pub json: Option<PathBuf>,
    /// 一起旁觀的 mjai AI（程式與參數）
    pub mjai: Option<Vec<String>>,
}

/// 要復盤的座位：指定座位，或依玩家名稱找；都沒指定時四家都復盤。
fn seats_of(rec: &GameRecord, opts: &Options) -> Result<Vec<Seat>, String> {
    if let Some(s) = opts.seat {
        return Ok(vec![s]);
    }
    if let Some(name) = &opts.player {
        let seats: Vec<Seat> = (0..4u8)
            .filter(|&s| &rec.players[usize::from(s)] == name)
            .collect();
        if seats.is_empty() {
            return Err(format!(
                "牌譜 {} 中沒有玩家「{name}」（玩家：{}）",
                rec.id,
                rec.players.join("、")
            ));
        }
        return Ok(seats);
    }
    Ok(vec![0, 1, 2, 3])
}

pub fn run(opts: &Options) -> Result<(), String> {
    let st: Arc<dyn mj_strategy::framework::Strategy> =
        Arc::from(find_strategy_spec(&opts.strategy)?);
    let (records, errors) = mj_paifu::load_all(&opts.logs);
    for e in &errors {
        eprintln!("✗ {e}");
    }
    if records.is_empty() {
        return Err("沒有可以復盤的牌譜".into());
    }
    let unit = match opts.metric {
        Metric::Points => "點",
        Metric::Placement => "名次點",
    };
    let start = Instant::now();
    let mut reviews = Vec::new();
    for rec in &records {
        let seats = match seats_of(rec, opts) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("✗ {e}");
                continue;
            }
        };
        let ro = ReviewOptions {
            seats,
            all: opts.all,
            eval: CfOptions {
                samples: 0,
                rollouts: opts.rollouts,
                // 固定世界數（不因差距顯著就提早停止，提早停止會讓誤報變多）
                max_rollouts: opts.rollouts,
                metric: opts.metric,
                determinize: !opts.omniscient,
                kinds: Vec::new(),
            },
            evaluate: true,
            mjai: opts.mjai.clone(),
        };
        match review_game(&st, rec, &ro) {
            Ok(r) => {
                print_game(&r, opts, unit);
                reviews.push(r);
            }
            Err(e) => eprintln!("✗ {}：{e}", rec.id),
        }
    }
    print_summary(&reviews, unit);
    if opts.mjai.is_some() {
        println!();
        print_agreement(&agreement(&reviews), "你");
    }
    println!(
        "\n策略 {}；衡量方式：{}；對手手牌：{}；{:.1} 秒",
        opts.strategy,
        match opts.metric {
            Metric::Points => "這一局的點數得失",
            Metric::Placement => "名次點（打到整場結束）",
        },
        if opts.omniscient {
            "使用真實手牌（結果論）"
        } else {
            "依當時看得到的資訊重抽"
        },
        start.elapsed().as_secs_f64()
    );
    if let Some(path) = &opts.json {
        let body = serde_json::to_string_pretty(&reviews).map_err(|e| e.to_string())?;
        std::fs::write(path, body).map_err(|e| format!("寫入 {} 失敗：{e}", path.display()))?;
        println!("復盤結果已寫入 {}", path.display());
    }
    Ok(())
}

/// 這個決策要不要在逐手報告中列出。
fn shown(d: &Reviewed, show: &str) -> bool {
    match show {
        "all" => true,
        "mistakes" => d.eval.as_ref().is_some_and(|e| e.is_mistake()),
        "ai" => d.ai.is_some_and(|a| a != d.player),
        _ => !d.agrees(),
    }
}

fn print_game(r: &GameReview, opts: &Options, unit: &str) {
    println!("════ 牌譜 {} ════", r.id);
    for &seat in &r.seats {
        let mine: Vec<&Reviewed> = r.decisions.iter().filter(|d| d.seat == seat).collect();
        let g = mj_review::GroupSummary::of(String::new(), &mine);
        println!(
            "座位 {seat}「{}」：可比較的決策 {} 個，與策略相同 {:.0}%；選擇不同的 {} 個平均差值 {:+.0} ± {:.0} {unit}，明顯失誤 {} 個",
            r.players[usize::from(seat)],
            g.decisions,
            g.agree * 100.0,
            g.evaluated,
            g.mean_diff,
            g.mean_diff_se,
            g.mistakes
        );
    }
    if r.ai_errors > 0 {
        println!(
            "（AI 的回應有 {} 次無法對應到合法動作，例如：{}）",
            r.ai_errors,
            r.ai_first_error.as_deref().unwrap_or_default()
        );
    }
    if r.incomparable > 0 || r.skipped > 0 {
        println!(
            "（玩家走了另一條路而無法比較的決策 {} 個；無法從快照評估而略過 {} 個）",
            r.incomparable, r.skipped
        );
    }
    for d in r.decisions.iter().filter(|d| shown(d, &opts.show)) {
        print_decision(d, r, unit);
    }
    println!();
}

fn print_decision(d: &Reviewed, r: &GameReview, unit: &str) {
    println!();
    println!(
        "── {} 第 {} 巡　座位 {}「{}」　{} ──",
        d.round,
        d.turn + 1,
        d.seat,
        r.players[usize::from(d.seat)],
        d.kind.name()
    );
    let tag = d
        .difference
        .as_ref()
        .map_or(String::new(), |t| format!("　〔{t}〕"));
    if d.agrees() {
        println!("  你與策略：{}", d.options[d.player]);
    } else {
        println!(
            "  你：{}　策略：{}{tag}",
            d.options[d.player], d.options[d.strategy]
        );
    }
    if let Some(a) = d.ai {
        let same = match (a == d.player, a == d.strategy) {
            (true, true) => "（與你和策略相同）",
            (true, false) => "（與你相同）",
            (false, true) => "（與策略相同）",
            (false, false) => "",
        };
        println!("  AI：{}{same}", d.options[a]);
    }
    if let Some(e) = &d.eval {
        let verdict = if d.agrees() {
            String::new()
        } else if e.is_mistake() {
            format!(
                "策略的選擇比你多 {:.0} ± {:.0} {unit}（明顯失誤）",
                e.diff, e.diff_se
            )
        } else if e.player_better() {
            format!(
                "你的選擇比策略多 {:.0} ± {:.0} {unit}（你明顯較好）",
                -e.diff, e.diff_se
            )
        } else {
            format!(
                "策略的選擇 − 你的選擇 = {:+.0} ± {:.0} {unit}",
                e.diff, e.diff_se
            )
        };
        if !verdict.is_empty() {
            println!("  反事實：{verdict}");
        }
        // 列出期望值最高的幾個選項，並確保包含你與策略的選擇。
        let mut order: Vec<usize> = (0..e.option_ev.len()).collect();
        order.sort_by(|&a, &b| e.option_ev[b].total_cmp(&e.option_ev[a]));
        let mut list: Vec<usize> = order.iter().copied().take(4).collect();
        for i in [d.player, d.strategy] {
            if !list.contains(&i) {
                list.push(i);
            }
        }
        let items: Vec<String> = list
            .iter()
            .map(|&i| {
                let mut mark = String::new();
                if i == e.best {
                    mark.push('★');
                }
                if i == d.player {
                    mark.push_str("←你");
                }
                if i == d.strategy {
                    mark.push_str("←策略");
                }
                format!("{} {:+.0}{mark}", d.options[i], e.option_ev[i])
            })
            .collect();
        println!("  各選項期望值：{}", items.join("、"));
    }
    if !d.notes.is_empty() {
        println!("  策略的理由：");
        for n in &d.notes {
            println!("    ・{n}");
        }
    }
}

fn print_summary(reviews: &[GameReview], unit: &str) {
    let s = summarize(reviews);
    println!("════ 彙整（{} 場）════", reviews.len());
    println!(
        "平均差值 = 選擇不同的決策中，策略的選擇 − 你的選擇的期望值（正 = 策略較好，即你平均損失；負 = 你較好）。"
    );
    let header = |first: &str| {
        println!(
            "{}{}{}{}{}{}{}",
            pad(first, 30),
            pad_left("決策數", 8),
            pad_left("與策略相同", 12),
            pad_left("評估數", 8),
            pad_left(&format!("平均差值（{unit}）"), 22),
            pad_left("明顯失誤", 10),
            pad_left("你明顯較好", 12)
        );
    };
    let row = |g: &mj_review::GroupSummary, show_agree: bool| {
        let mark = if !g.significant() {
            ""
        } else if g.mean_diff > 0.0 {
            " 策略較好"
        } else {
            " 你較好"
        };
        println!(
            "{}{}{}{}{}{}{}{mark}",
            pad(&g.label, 30),
            pad_left(&g.decisions.to_string(), 8),
            pad_left(
                &if show_agree {
                    format!("{:.0}%", g.agree * 100.0)
                } else {
                    "—".into()
                },
                12
            ),
            pad_left(&g.evaluated.to_string(), 8),
            pad_left(
                &if g.evaluated == 0 {
                    "—".into()
                } else {
                    format!("{:+.0} ± {:.0}", g.mean_diff, g.mean_diff_se)
                },
                22
            ),
            pad_left(&g.mistakes.to_string(), 10),
            pad_left(&g.player_better.to_string(), 12)
        );
    };
    println!();
    header("決策種類");
    for g in &s.by_kind {
        row(g, true);
    }
    row(&s.overall, true);
    println!();
    header("與策略不同的類型");
    for g in &s.by_type {
        row(g, false);
    }
    println!();
    println!(
        "「策略較好／你較好」= 至少 {} 個評估過的決策、平均差值大於 2 倍標準誤。明顯失誤／你明顯較好 = 單一決策的差距大於 {} 倍標準誤\n\
         （一場有幾十個決策要評估，單一決策用較嚴的門檻以免誤報；整體傾向請看平均差值）。",
        mj_review::MIN_GROUP,
        mj_review::MISTAKE_Z
    );
}

/// 三方一致率表。`player` = 玩家欄的稱呼（復盤是「你」，自我對戰時沒有玩家欄）。
pub(crate) fn print_agreement(rows: &[Agreement], player: &str) {
    println!("════ 與 AI 的一致率 ════");
    let with_player = !player.is_empty();
    print!(
        "{}{}{}",
        pad("決策種類", 30),
        pad_left("決策數", 8),
        pad_left("策略＝AI", 10)
    );
    if with_player {
        print!(
            "{}{}",
            pad_left(&format!("{player}＝AI"), 10),
            pad_left(&format!("策略＝{player}"), 12)
        );
    }
    println!(
        "{}{}",
        pad_left("策略做AI不做", 14),
        pad_left("AI做策略不做", 14)
    );
    for a in rows {
        print!(
            "{}{}{}",
            pad(&a.label, 30),
            pad_left(&a.decisions.to_string(), 8),
            pad_left(&format!("{:.1}%", a.strategy_ai * 100.0), 10)
        );
        if with_player {
            print!(
                "{}{}",
                pad_left(&format!("{:.1}%", a.player_ai * 100.0), 10),
                pad_left(&format!("{:.1}%", a.strategy_player * 100.0), 12)
            );
        }
        let acts = |x: f64| {
            if a.label.starts_with("何切") || a.label == "全部" {
                "—".to_string()
            } else {
                format!("{:.1}%", x * 100.0)
            }
        };
        println!(
            "{}{}",
            pad_left(&acts(a.strategy_acts), 14),
            pad_left(&acts(a.ai_acts), 14)
        );
    }
    println!(
        "（只計入有兩個以上選項、而且 AI 的選擇能對應到選項的決策。一致率是「像不像」而不是「好不好」：\n\
         很多局面有幾個差不多好的選擇；一致率特別低的類別再用反事實評估或對戰確認誰比較好。\n\
         「做」= 是非型決策中選了選項 0（不鳴、不槓、默聽、不和、不流局）以外的選項。\n\
         同一局中同樣的局面每巡都算一次決策，例如玩家連續幾巡默聽時，策略每巡都會想立直。）"
    );
}
