//! 命令列工具 `mj`。每個子命令對應一個里程碑的功能；尚未實作的會提示預計在哪個里程碑完成。

mod evaluate;
mod import;
mod nanikiru;
mod predict_eval;
mod review;
mod selfplay;
mod simulate;
mod tune;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

/// 可解釋的日麻 AI 工具組。
#[derive(Parser)]
#[command(name = "mj", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// 子命令。註解同時是 `mj --help` 顯示的說明文字。
#[derive(Subcommand)]
enum Command {
    /// 列出所有策略（src/strategy-list/<規則>/<策略>/）與摘要；指定名稱時顯示該策略的完整說明
    Strategies {
        /// 策略名稱「規則／策略」，例如 tenhou-normal-4p-hanchan/push-fold
        name: Option<String>,
    },
    /// 何切分析：列出一手 14 張牌每種打法的分數與理由
    Nanikiru {
        /// 手牌 14 張（最後一張視為剛摸到的牌），例如 123m406p789s1122z5z
        hand: String,
        /// 寶牌指示牌，例如 4p
        #[arg(long, default_value = "")]
        dora: String,
        /// 策略名稱
        #[arg(long, default_value = "tenhou-normal-4p-hanchan/hand-reading")]
        strategy: String,
    },
    /// 機器人或策略自我對戰並輸出統計（兼引擎模糊測試）
    Selfplay {
        /// 對局（半莊）數
        #[arg(long, default_value_t = 1000)]
        games: u64,
        /// 起始 seed；第 g 場使用 seed + g
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// 四個座位，以逗號分隔：greedy（貪婪機器人）、random（隨機機器人）或策略名稱
        #[arg(long, default_value = "greedy,greedy,greedy,greedy")]
        seats: String,
        /// 不輪換座位（預設每場輪換，消除起家位置的影響）
        #[arg(long)]
        no_rotate: bool,
        /// 把第一場的完整事件紀錄（牌譜）以 JSON lines 寫到這個檔案
        #[arg(long)]
        dump: Option<PathBuf>,
    },
    /// 匯入牌譜（天鳳 mjlog XML／JSON）：用引擎重播並與原牌譜的點數比對
    Import {
        /// 牌譜檔或目錄（目錄中的 .xml／.mjlog／.json；同一個牌譜 ID 只算一次）
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// 把引擎的事件紀錄寫到這個目錄（每場一個 <牌譜 ID>.events.json）
        #[arg(long)]
        dump: Option<PathBuf>,
        /// 最多列出幾個失敗原因
        #[arg(long, default_value_t = 20)]
        show_errors: usize,
    },
    /// 預測準確度評估：讓策略自我對戰（或旁觀牌譜中的真人對局），用全知視角檢查每個預測，
    /// 輸出各預測函式的準確度
    PredictEval {
        /// 策略名稱（四家都用這個策略）
        #[arg(long, default_value = "tenhou-normal-4p-hanchan/hand-reading")]
        strategy: String,
        /// 改用這些牌譜（檔案或目錄）：策略旁觀真人對局並預測，不自我對戰
        #[arg(long, num_args = 1..)]
        logs: Vec<PathBuf>,
        /// 對局（半莊）數
        #[arg(long, default_value_t = 200)]
        games: u64,
        /// 起始 seed
        #[arg(long, default_value_t = 0)]
        seed: u64,
    },
    /// 列出策略的可調參數（名稱、目前值、預設值、範圍、說明）。策略名稱可加 `@覆寫`，
    /// 例如 `策略@params.txt` 或 `策略@call.far_shanten=3`（所有指令的策略名稱都可以這樣寫）
    Params {
        /// 策略名稱
        #[arg(default_value = "tenhou-normal-4p-hanchan/hand-reading")]
        strategy: String,
    },
    /// 自動調整可調參數：預測以牌譜上的對數損失擬合，決策以 duplicate 對戰調整；最後用另一組資料驗證
    Tune {
        /// 策略名稱（可加 @覆寫，作為調整的起點）
        #[arg(default_value = "tenhou-normal-4p-hanchan/hand-reading")]
        strategy: String,
        /// 要調整的參數（名稱，或以 . 結尾的前綴，例如 tenpai.）
        #[arg(long, num_args = 1.., required = true)]
        params: Vec<String>,
        /// 目標：logloss（預測，需要 --logs）或 duplicate（決策）
        #[arg(long, default_value = "duplicate")]
        objective: String,
        /// logloss：要擬合的預測函式
        #[arg(long, default_value = "predict::tenpai")]
        source: String,
        /// logloss：擬合用的牌譜
        #[arg(long, num_args = 1..)]
        logs: Vec<PathBuf>,
        /// logloss：驗證用的牌譜（應與擬合用的不同）
        #[arg(long, num_args = 1..)]
        validate_logs: Vec<PathBuf>,
        /// duplicate：對戰的基準（預設 = 調整前的同一策略）
        #[arg(long)]
        baseline: Option<String>,
        /// duplicate：每次比較的組數
        #[arg(long, default_value_t = 1000)]
        blocks: u64,
        /// duplicate：調整用的牌山 seed
        #[arg(long, default_value_t = 700_000)]
        seed: u64,
        /// duplicate：驗證的組數
        #[arg(long, default_value_t = 4000)]
        validate_blocks: u64,
        /// duplicate：驗證用的牌山 seed（應與調整用的不同）
        #[arg(long, default_value_t = 800_000)]
        validate_seed: u64,
        /// 座標搜尋輪數
        #[arg(long, default_value_t = 2)]
        rounds: usize,
        /// duplicate：候選值要好過幾個標準誤才採用
        #[arg(long, default_value_t = 2.0)]
        accept_z: f64,
        /// 把調整後的參數寫成參數檔
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// 以 mjai 協定在標準輸入／輸出上執行內建的簡單機器人（驗證 mjai 介接用）：
    /// 每讀一行事件陣列，回應一行動作
    MjaiBot {
        /// 機器人種類：shanten（向聽數最低）或 tsumogiri（永遠摸切）
        #[arg(long, default_value = "shanten")]
        kind: String,
    },
    /// 牌譜復盤：逐手比較玩家與策略的選擇，附策略的理由；選擇不同的地方用反事實模擬算出各選項的期望值，
    /// 最後彙整失誤類型
    Review {
        /// 牌譜檔或目錄（天鳳 mjlog XML／JSON）
        #[arg(required = true)]
        logs: Vec<PathBuf>,
        /// 復盤哪位玩家（牌譜中的名稱）；沒指定時四家都復盤
        #[arg(long)]
        player: Option<String>,
        /// 復盤哪個座位（0 = 起家）；優先於 --player
        #[arg(long)]
        seat: Option<u8>,
        /// 策略名稱
        #[arg(long, default_value = "tenhou-normal-4p-hanchan/hand-reading")]
        strategy: String,
        /// 每個決策都做反事實評估（預設只評估與策略選擇不同的決策）
        #[arg(long)]
        all: bool,
        /// 逐手報告列出哪些決策：differences（與策略不同）、mistakes（失誤）、all
        #[arg(long, default_value = "differences")]
        show: String,
        /// 每個選項重打幾次（每次是一個重抽過看不到的牌的「可能世界」；越多越準、越慢）
        #[arg(long, default_value_t = 64)]
        rollouts: usize,
        /// 衡量方式：points（這一局的點數，預設）或 placement（名次點，打到整場結束）
        #[arg(long, default_value = "points")]
        metric: String,
        /// 使用對手的真實手牌（結果論），不重抽
        #[arg(long)]
        omniscient: bool,
        /// 把復盤結果寫成 JSON 檔
        #[arg(long)]
        json: Option<PathBuf>,
    },
    /// 讀牌資料匯出：在牌譜的每個打牌時點，對三家對手輸出「看得到的特徵＋真相」的 CSV
    /// （tenpai.csv、waits.csv），供分析與校準讀牌預測
    ReadStats {
        /// 牌譜檔或目錄
        #[arg(required = true)]
        logs: Vec<PathBuf>,
        /// 輸出目錄
        #[arg(long, default_value = "target/read-stats")]
        out: PathBuf,
    },
    /// 決策品質評估（反事實模擬）：在決策點改選其他選項重打這一局，比較期望點數，
    /// 依決策種類統計「平均損失點數」
    Evaluate {
        /// 策略名稱（四家都用這個策略）
        #[arg(long, default_value = "tenhou-normal-4p-hanchan/hand-reading")]
        strategy: String,
        /// 對局（半莊）數
        #[arg(long, default_value_t = 20)]
        games: u64,
        /// 起始 seed
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// 每場對局抽樣幾個決策點來評估
        #[arg(long, default_value_t = 20)]
        samples: usize,
        /// 每個選項最少重打幾次（每次是一個重抽過看不到的牌的「可能世界」）
        #[arg(long, default_value_t = 16)]
        rollouts: usize,
        /// 差距不顯著時，重打次數最多增加到多少
        #[arg(long, default_value_t = 64)]
        max_rollouts: usize,
        /// 衡量方式：placement（名次點，打到整場結束）或 points（這一局的點數）
        #[arg(long, default_value = "placement")]
        metric: String,
        /// 使用真實的對手手牌（全知），不重抽；用來和預設的重抽方式比較
        #[arg(long)]
        omniscient: bool,
        /// 把評估結果寫成 JSON 檔
        #[arg(long)]
        json: Option<PathBuf>,
        /// 只評估這些決策種類，以逗號分隔：kyuushu,win,kan,discard,riichi,call（預設全部）
        #[arg(long, default_value = "")]
        kinds: String,
        /// 列出損失最大的前幾個（顯著的）決策
        #[arg(long, default_value_t = 5)]
        worst: usize,
    },
    /// duplicate 對戰：同一組牌山輪換座位，比較 2 個（各坐兩個位置）或 4 個參賽者
    Simulate {
        /// 參賽者：策略名稱、greedy 或 random（2 或 4 個；同名兩次即 A/A 測試）
        entrants: Vec<String>,
        /// 組數（每組 4 場）
        #[arg(long, default_value_t = 500)]
        blocks: u64,
        /// 起始 seed
        #[arg(long, default_value_t = 0)]
        seed: u64,
    },
    /// 在終端機與 AI 對戰（M9）
    Play { strategy: String },
    /// 以 HTTP 提供 UI 用的 API，供瀏覽器開發（M9）
    Serve {
        /// 監聽的 port（容器對外開放 30100–30300）
        #[arg(long, default_value_t = 30100)]
        port: u16,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let milestone = match cli.command {
        Command::Strategies { name } => {
            if let Some(name) = name {
                let Some(s) = mj_strategy::find_strategy(&name) else {
                    eprintln!("找不到策略 {name}");
                    return ExitCode::FAILURE;
                };
                println!("{}", s.readme());
                return ExitCode::SUCCESS;
            }
            let strategies = mj_strategy::all_strategies();
            for (rule, readme) in mj_strategy::all_rules() {
                let title = readme
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim_start_matches("# ");
                println!("{rule}（{title}）");
                for s in strategies.iter().filter(|s| s.rule() == rule) {
                    println!("  {}：{}", s.name(), s.title());
                    for line in s.summary().lines() {
                        println!("      {line}");
                    }
                }
            }
            return ExitCode::SUCCESS;
        }
        Command::Nanikiru {
            hand,
            dora,
            strategy,
        } => {
            return exit(nanikiru::run(&nanikiru::Options {
                hand: &hand,
                dora: &dora,
                strategy: &strategy,
            }));
        }
        Command::Selfplay {
            games,
            seed,
            seats,
            no_rotate,
            dump,
        } => return exit(selfplay::run(games, seed, &seats, !no_rotate, dump)),
        Command::Import {
            paths,
            dump,
            show_errors,
        } => return exit(import::run(&paths, dump, show_errors)),
        Command::PredictEval {
            strategy,
            logs,
            games,
            seed,
        } => {
            return exit(predict_eval::run(&strategy, &logs, games, seed));
        }
        Command::Tune {
            strategy,
            params,
            objective,
            source,
            logs,
            validate_logs,
            baseline,
            blocks,
            seed,
            validate_blocks,
            validate_seed,
            rounds,
            accept_z,
            out,
        } => {
            return exit(tune::run(&tune::Options {
                strategy,
                params,
                objective,
                source,
                logs,
                validate_logs,
                baseline,
                blocks,
                seed,
                validate_blocks,
                validate_seed,
                rounds,
                accept_z,
                out,
            }));
        }
        Command::MjaiBot { kind } => {
            let kind = match kind.as_str() {
                "shanten" => mj_mjai::bots::Kind::Shanten,
                "tsumogiri" => mj_mjai::bots::Kind::Tsumogiri,
                other => {
                    eprintln!("未知的機器人 {other}（可用 shanten 或 tsumogiri）");
                    return ExitCode::FAILURE;
                }
            };
            let mut bot = mj_mjai::bots::MjaiBot::new(kind);
            let stdin = std::io::stdin();
            return exit(mj_mjai::serve(&mut bot, stdin.lock(), std::io::stdout()));
        }
        Command::Params { strategy } => {
            let st = match mj_strategy::find_strategy_spec(&strategy) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            };
            if st.params().is_empty() {
                println!("{} 沒有可調參數", st.name());
            }
            for p in st.params() {
                let v = st.overrides().get(p);
                let mark = if v != p.default { "（覆寫）" } else { "" };
                println!(
                    "{} = {v}{mark}\n    預設 {}，範圍 {}–{}：{}",
                    p.name, p.default, p.min, p.max, p.doc
                );
            }
            return ExitCode::SUCCESS;
        }
        Command::Review {
            logs,
            player,
            seat,
            strategy,
            all,
            show,
            rollouts,
            metric,
            omniscient,
            json,
        } => {
            let metric = match metric.as_str() {
                "placement" => mj_sim::counterfactual::Metric::Placement,
                "points" => mj_sim::counterfactual::Metric::Points,
                other => {
                    eprintln!("未知的衡量方式 {other}（可用 placement 或 points）");
                    return ExitCode::FAILURE;
                }
            };
            return exit(review::run(&review::Options {
                logs,
                strategy,
                player,
                seat,
                all,
                show,
                rollouts,
                metric,
                omniscient,
                json,
            }));
        }
        Command::ReadStats { logs, out } => {
            let (records, mut errors) = mj_paifu::load_all(&logs);
            let (tenpai, waits, replay_errors) = mj_sim::read_stats::collect_records(&records);
            errors.extend(replay_errors);
            let write = |name: &str, body: &str| {
                std::fs::create_dir_all(&out)
                    .and_then(|_| std::fs::write(out.join(name), body))
                    .map_err(|e| format!("寫入 {} 失敗：{e}", out.join(name).display()))
            };
            let r = write("tenpai.csv", &tenpai).and_then(|_| write("waits.csv", &waits));
            println!(
                "牌譜 {} 場（無法使用 {} 場）→ {}：tenpai.csv {} 列、waits.csv {} 列",
                records.len(),
                errors.len(),
                out.display(),
                tenpai.lines().count() - 1,
                waits.lines().count() - 1
            );
            return exit(r);
        }
        Command::Evaluate {
            strategy,
            games,
            seed,
            samples,
            rollouts,
            max_rollouts,
            metric,
            omniscient,
            json,
            kinds,
            worst,
        } => {
            let kinds: Vec<mj_strategy::framework::DecisionKind> = match kinds
                .split(',')
                .filter(|k| !k.trim().is_empty())
                .map(|k| parse_kind(k.trim()))
                .collect::<Result<_, _>>()
            {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            };
            let metric = match metric.as_str() {
                "placement" => mj_sim::counterfactual::Metric::Placement,
                "points" => mj_sim::counterfactual::Metric::Points,
                other => {
                    eprintln!("未知的衡量方式 {other}（可用 placement 或 points）");
                    return ExitCode::FAILURE;
                }
            };
            return exit(evaluate::run(&evaluate::Options {
                strategy: &strategy,
                games,
                seed,
                samples,
                rollouts,
                max_rollouts,
                metric,
                omniscient,
                worst,
                json,
                kinds,
            }));
        }
        Command::Simulate {
            entrants,
            blocks,
            seed,
        } => return exit(simulate::run(&entrants, blocks, seed)),
        Command::Play { .. } | Command::Serve { .. } => "M9",
    };
    eprintln!("尚未實作（預計在 {milestone} 完成，見 docs/04-milestones.md）");
    ExitCode::FAILURE
}

/// 解析決策種類名稱。
fn parse_kind(s: &str) -> Result<mj_strategy::framework::DecisionKind, String> {
    use mj_strategy::framework::DecisionKind::*;
    Ok(match s {
        "kyuushu" => Kyuushu,
        "win" => Win,
        "kan" => Kan,
        "discard" => Discard,
        "riichi" => Riichi,
        "call" => Call,
        other => {
            return Err(format!(
                "未知的決策種類 {other}（可用 kyuushu, win, kan, discard, riichi, call）"
            ));
        }
    })
}

/// 把結果轉成結束碼，錯誤訊息印到 stderr。
fn exit(r: Result<(), String>) -> ExitCode {
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
