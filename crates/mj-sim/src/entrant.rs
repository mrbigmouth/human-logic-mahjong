//! 參賽者登錄（M15）：參賽者名稱 → 玩家。CLI（`mj selfplay`、`mj simulate`）與練習 App 的對戰測試共用。
//!
//! 名稱：`greedy`、`random`、`mjai:<外部程式 參數…>`（說 mjai 協定的 AI；行程在對局之間重複使用），
//! 或策略名稱「規則／策略」（可加 `@參數覆寫`，見 `mj_strategy::find_strategy_spec`）。
//! 機器人的 seed 由對局 seed 與座位推導，所以除了外部 AI，同樣的 seed 與座位配置一定重現同一場對局。

use crate::bots::{GreedyBot, RandomBot};
use mj_engine::Player;
use mj_strategy::find_strategy_spec;
use mj_strategy::framework::Strategy;
use std::sync::Arc;

/// 參賽者的種類。
#[derive(Clone)]
pub enum EntrantKind {
    /// 貪婪機器人（純牌效，隨機鳴牌）
    Greedy,
    /// 隨機機器人
    Random,
    /// strategy-list 中的策略
    Strategy(Arc<dyn Strategy>),
    /// 說 mjai 協定的外部程式（程式與參數）
    Mjai(Vec<String>),
}

/// 一個參賽者與它在統計中的名稱。
#[derive(Clone)]
pub struct EntrantSpec {
    pub label: String,
    pub kind: EntrantKind,
}

impl std::fmt::Debug for EntrantSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

impl EntrantSpec {
    /// 解析一個參賽者名稱（見模組說明）。
    pub fn parse(s: &str) -> Result<EntrantSpec, String> {
        let s = s.trim();
        let kind = match s {
            "greedy" => EntrantKind::Greedy,
            "random" => EntrantKind::Random,
            mjai if mjai.starts_with("mjai:") => {
                let cmd: Vec<String> = mjai["mjai:".len()..]
                    .split_whitespace()
                    .map(String::from)
                    .collect();
                if cmd.is_empty() {
                    return Err("mjai: 後面要接外部程式，例如 mjai:./mortal.sh".into());
                }
                EntrantKind::Mjai(cmd)
            }
            name => EntrantKind::Strategy(Arc::from(find_strategy_spec(name)?)),
        };
        Ok(EntrantSpec {
            label: s.to_string(),
            kind,
        })
    }

    /// 外部 AI（行程的狀態不能重現，對局要另外保存牌譜）。
    pub fn is_mjai(&self) -> bool {
        matches!(self.kind, EntrantKind::Mjai(_))
    }

    /// 建立玩家（機器人的 seed 由對局 seed 與座位推導，結果可重現）。
    pub fn try_player(&self, seed: u64, seat: u64) -> Result<Box<dyn Player + Send>, String> {
        let s = seed.wrapping_mul(31).wrapping_add(seat);
        Ok(match &self.kind {
            EntrantKind::Greedy => Box::new(GreedyBot::new(s, 0.3)),
            EntrantKind::Random => Box::new(RandomBot::new(s)),
            EntrantKind::Strategy(st) => st.player(false, None),
            EntrantKind::Mjai(cmd) => {
                let bot = mj_mjai::PooledBot::checkout(cmd)?;
                let names = std::array::from_fn(|_| self.label.clone());
                Box::new(mj_mjai::MjaiPlayer::new(bot, names))
            }
        })
    }

    /// 同 [`EntrantSpec::try_player`]；外部 AI 啟動失敗時 panic（大量對戰的工作函式不能回傳錯誤時用）。
    pub fn player(&self, seed: u64, seat: u64) -> Box<dyn Player> {
        self.try_player(seed, seat)
            .unwrap_or_else(|e| panic!("{e}"))
    }
}

/// 每場有 mjai AI 的對局預留的記憶體（MB）：最多 4 個 AI 行程，Mortal 每個約 380 MB。
pub const MJAI_MB_PER_JOB: u64 = 1600;

/// 用到 mjai AI 時同時進行的對局數：每場對局保留 [`MJAI_MB_PER_JOB`] 的可用記憶體（不超過 CPU 核心數）。
pub fn mjai_jobs() -> usize {
    let available_mb = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|m| {
            m.lines()
                .find(|l| l.starts_with("MemAvailable:"))?
                .split_whitespace()
                .nth(1)?
                .parse::<u64>()
                .ok()
        })
        .map_or(4 * MJAI_MB_PER_JOB, |kb| kb / 1024);
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    ((available_mb / MJAI_MB_PER_JOB) as usize).clamp(1, cores)
}
