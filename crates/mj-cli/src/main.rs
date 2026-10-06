use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

/// Explainable riichi mahjong AI toolkit.
#[derive(Parser)]
#[command(name = "mj", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check that a profile file parses.
    Check { profile: PathBuf },
    /// Run duplicate self-play between profiles (M3).
    Simulate {
        profiles: Vec<PathBuf>,
        #[arg(long, default_value_t = 1000)]
        games: u32,
        #[arg(long, default_value_t = 0)]
        seed: u64,
    },
    /// Tune a profile's weights by self-play (M6).
    Tune { profile: PathBuf },
    /// Review a game log against a profile (M8).
    Review { log: PathBuf, profile: PathBuf },
    /// Play against the AI in the terminal (M7).
    Play { profile: PathBuf },
    /// Serve the UI API over HTTP for browser development (M7).
    Serve {
        #[arg(long, default_value_t = 30100)]
        port: u16,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let milestone = match cli.command {
        Command::Check { profile } => return check(&profile),
        Command::Simulate { .. } => "M3",
        Command::Tune { .. } => "M6",
        Command::Review { .. } => "M8",
        Command::Play { .. } | Command::Serve { .. } => "M7",
    };
    eprintln!("not implemented yet (planned for {milestone}, see docs/04-milestones.md)");
    ExitCode::FAILURE
}

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
