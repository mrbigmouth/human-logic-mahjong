//! 復盤的背景工作：對局結束後的 session 與讀入的牌譜共用。

use crate::advisor::{AdvisorInfo, Advisors};
use mj_engine::Seat;
use mj_paifu::GameRecord;
use mj_review::{GameReview, Options as ReviewOptions, Summary, review_game, summarize};
use mj_sim::counterfactual::{Metric, Options as CfOptions};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 復盤狀態。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ReviewState {
    /// 還沒開始
    Idle,
    /// 進行中（已經過的秒數）
    Running {
        seconds: f64,
    },
    Done {
        /// 顧問（與 `review.decisions[].picks` 對應；第一個是比較基準）
        advisors: Vec<AdvisorInfo>,
        review: Box<GameReview>,
        summary: Box<Summary>,
        seconds: f64,
    },
    Failed {
        error: String,
    },
}

/// 復盤設定。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReviewConfig {
    /// 每個選擇不同的決策重打幾個世界
    pub rollouts: usize,
    /// points = 這一局的點數得失；placement = 名次點（打到整場結束，較慢）
    pub metric: String,
    /// 一起比較的 mjai AI（例如 `tools/mortal/run.sh s42`）
    pub mjai: Option<String>,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        ReviewConfig {
            rollouts: 32,
            metric: "points".into(),
            mjai: None,
        }
    }
}

/// 一個復盤工作（同時只有一個在跑；完成後保留結果）。
#[derive(Default)]
pub struct ReviewJob {
    inner: Arc<Mutex<(Option<ReviewState>, Option<Instant>)>>,
}

impl ReviewJob {
    /// 在背景復盤 `record` 的 `seat`。已經在跑時不重複開始。
    pub fn start(
        &self,
        advisors: &Advisors,
        record: GameRecord,
        seat: Seat,
        rc: &ReviewConfig,
    ) -> Result<(), String> {
        let metric = match rc.metric.as_str() {
            "points" => Metric::Points,
            "placement" => Metric::Placement,
            m => return Err(format!("不認得的衡量方式 {m}（points 或 placement）")),
        };
        if seat > 3 {
            return Err("座位必須是 0–3".into());
        }
        {
            let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if matches!(g.0, Some(ReviewState::Running { .. })) {
                return Ok(());
            }
            *g = (
                Some(ReviewState::Running { seconds: 0.0 }),
                Some(Instant::now()),
            );
        }
        let opts = ReviewOptions {
            seats: vec![seat],
            all: false,
            eval: CfOptions {
                samples: 0,
                rollouts: rc.rollouts,
                max_rollouts: rc.rollouts,
                metric,
                determinize: true,
                kinds: Vec::new(),
            },
            evaluate: rc.rollouts > 0,
            mjai: rc
                .mjai
                .as_ref()
                .map(|m| m.split_whitespace().map(String::from).collect()),
            advisors: advisors.list.clone(),
            base_advisor: advisors.base_index,
        };
        let strategy = advisors.base.clone();
        let infos = advisors.infos();
        let inner = self.inner.clone();
        std::thread::Builder::new()
            .name("mj-review".into())
            .spawn(move || {
                let start = Instant::now();
                let r = review_game(&strategy, &record, &opts);
                let state = match r {
                    Ok(review) => ReviewState::Done {
                        advisors: infos,
                        summary: Box::new(summarize(std::slice::from_ref(&review))),
                        review: Box::new(review),
                        seconds: start.elapsed().as_secs_f64(),
                    },
                    Err(error) => ReviewState::Failed { error },
                };
                inner.lock().unwrap_or_else(|e| e.into_inner()).0 = Some(state);
            })
            .map_err(|e| format!("無法建立復盤執行緒：{e}"))?;
        Ok(())
    }

    /// 目前的狀態。
    pub fn state(&self) -> ReviewState {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match &g.0 {
            None => ReviewState::Idle,
            Some(ReviewState::Running { .. }) => ReviewState::Running {
                seconds: g.1.map_or(0.0, |t| t.elapsed().as_secs_f64()),
            },
            Some(r) => r.clone(),
        }
    }
}
