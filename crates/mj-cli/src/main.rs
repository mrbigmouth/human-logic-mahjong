//! 命令列工具 `mj`。每個子命令對應一個里程碑的功能；尚未實作的會提示預計在哪個里程碑完成。

mod evaluate;
mod nanikiru;
mod predict_eval;
mod selfplay;

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
    /// 列出所有策略（src/strategy-list/ 下的目錄）
    Strategies,
    /// 何切分析：列出一手 14 張牌每種打法的分數與理由
    Nanikiru {
        /// 手牌 14 張（最後一張視為剛摸到的牌），例如 123m406p789s1122z5z
        hand: String,
        /// 寶牌指示牌，例如 4p
        #[arg(long, default_value = "")]
        dora: String,
        /// 策略名稱
        #[arg(long, default_value = "tenhou-normal-4p-hanchan")]
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
    /// 預測準確度評估：讓策略自我對戰，用全知視角檢查每個預測，輸出各預測函式的準確度
    PredictEval {
        /// 策略名稱（四家都用這個策略）
        #[arg(long, default_value = "tenhou-normal-4p-hanchan")]
        strategy: String,
        /// 對局（半莊）數
        #[arg(long, default_value_t = 200)]
        games: u64,
        /// 起始 seed
        #[arg(long, default_value_t = 0)]
        seed: u64,
    },
    /// 決策品質評估（反事實模擬）：在決策點改選其他選項重打這一局，比較期望點數，
    /// 依決策種類統計「平均損失點數」
    Evaluate {
        /// 策略名稱（四家都用這個策略）
        #[arg(long, default_value = "tenhou-normal-4p-hanchan")]
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
        /// 每個選項重打幾次（牌山未摸的部分每次重新洗牌）
        #[arg(long, default_value_t = 24)]
        rollouts: usize,
        /// 列出損失最大的前幾個決策
        #[arg(long, default_value_t = 5)]
        worst: usize,
    },
    /// 策略之間的 duplicate 對戰（M3）
    Simulate {
        /// 參與對戰的策略名稱
        strategies: Vec<String>,
        /// 對局數
        #[arg(long, default_value_t = 1000)]
        games: u32,
    },
    /// 用策略復盤一份牌譜（M8）
    Review { log: PathBuf, strategy: String },
    /// 在終端機與 AI 對戰（M7）
    Play { strategy: String },
    /// 以 HTTP 提供 UI 用的 API，供瀏覽器開發（M7）
    Serve {
        /// 監聽的 port（容器對外開放 30100–30300）
        #[arg(long, default_value_t = 30100)]
        port: u16,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let milestone = match cli.command {
        Command::Strategies => {
            for s in mj_strategy::all_strategies() {
                println!("{}\n    {}", s.name(), s.description());
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
        Command::PredictEval {
            strategy,
            games,
            seed,
        } => {
            return exit(predict_eval::run(&strategy, games, seed));
        }
        Command::Evaluate {
            strategy,
            games,
            seed,
            samples,
            rollouts,
            worst,
        } => {
            return exit(evaluate::run(&evaluate::Options {
                strategy: &strategy,
                games,
                seed,
                samples,
                rollouts,
                worst,
            }));
        }
        Command::Simulate { .. } => "M3",
        Command::Review { .. } => "M8",
        Command::Play { .. } | Command::Serve { .. } => "M7",
    };
    eprintln!("尚未實作（預計在 {milestone} 完成，見 docs/04-milestones.md）");
    ExitCode::FAILURE
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
