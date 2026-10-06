//! 命令列工具 `mj`。每個子命令對應一個里程碑的功能；尚未實作的會提示預計在哪個里程碑完成。

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
    /// 檢查策略側寫（TOML）的格式
    Check { profile: PathBuf },
    /// 參考機器人自我對戰並輸出統計（兼引擎模糊測試）
    Selfplay {
        /// 對局（半莊）數
        #[arg(long, default_value_t = 1000)]
        games: u64,
        /// 起始 seed；第 g 場使用 seed + g
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// 機器人組合
        #[arg(long, value_enum, default_value = "greedy")]
        bots: selfplay::Bots,
        /// 把第一場的完整事件紀錄（牌譜）以 JSON lines 寫到這個檔案
        #[arg(long)]
        dump: Option<PathBuf>,
    },
    /// 策略側寫之間的 duplicate 對戰（M3）
    Simulate {
        /// 參與對戰的策略側寫
        profiles: Vec<PathBuf>,
        /// 對局數
        #[arg(long, default_value_t = 1000)]
        games: u32,
        /// 起始 seed
        #[arg(long, default_value_t = 0)]
        seed: u64,
    },
    /// 以自我對戰自動調整策略權重（M6）
    Tune { profile: PathBuf },
    /// 用策略側寫復盤一份牌譜（M8）
    Review { log: PathBuf, profile: PathBuf },
    /// 在終端機與 AI 對戰（M7）
    Play { profile: PathBuf },
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
        Command::Check { profile } => return check(&profile),
        Command::Selfplay {
            games,
            seed,
            bots,
            dump,
        } => {
            return match selfplay::run(games, seed, bots, dump) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            };
        }
        Command::Simulate { .. } => "M3",
        Command::Tune { .. } => "M6",
        Command::Review { .. } => "M8",
        Command::Play { .. } | Command::Serve { .. } => "M7",
    };
    eprintln!("not implemented yet (planned for {milestone}, see docs/04-milestones.md)");
    ExitCode::FAILURE
}

/// `mj check`：讀取並解析策略側寫，印出摘要。
fn check(path: &PathBuf) -> ExitCode {
    let parsed = std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|s| s.parse::<mj_strategy::Profile>().map_err(|e| e.to_string()));
    match parsed {
        Ok(p) => {
            println!(
                "{}: {} considerations, {} modifiers",
                p.name,
                p.considerations.len(),
                p.modifiers.len()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            ExitCode::FAILURE
        }
    }
}
