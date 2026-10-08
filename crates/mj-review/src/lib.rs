//! 復盤（M7）：讀取牌譜，在玩家的每個決策點讓策略「旁觀」並做同樣的決策，
//! 逐手比較玩家與策略的選擇，附上策略的理由；選擇不同的地方用反事實模擬算出各選項的期望值，
//! 判斷誰的選擇比較好，並把玩家的失誤分類統計。
//!
//! 流程：
//! 1. 照牌譜重播，四家都掛上策略當旁觀者（`mj_paifu::replay`）：策略收到同樣的事件與詢問，
//!    記錄自己的決策（選項、選擇、理由），動作則照牌譜。同時記下每次詢問時玩家實際的動作。
//! 2. 把玩家的動作對應到策略的決策選項（`option_of_action`）。玩家走了另一條路
//!    （例如策略在想何切，玩家卻自摸了）的決策無法比較，差異會出現在更早的決策上。
//! 3. 選擇不同的決策（或 `all` 時每個決策）：重播到那一刻保存快照（`mj_paifu::replay::capture`），
//!    依玩家當時看得到的資訊重抽看不到的牌，各選項強制選定後由策略接著打完，比較平均結果
//!    （`mj_sim::counterfactual::evaluate_snapshot`）。
//! 4. 玩家的選擇比最佳選項差、而且差距顯著（大於 2 倍標準誤）時算「失誤」，依類型統計。

use mj_core::Pai;
use mj_engine::{Action, PlayerView, Seat, hand_seed};
use mj_paifu::GameRecord;
use mj_paifu::replay::{DecisionPoint, capture, replay};
use mj_sim::counterfactual::{self, Point, Settle, evaluate_snapshot, paired_diff};
use mj_strategy::framework::{
    DecisionKind, DecisionRecord, Strategy, StrategyPlayer, option_of_action,
};
use rayon::prelude::*;
use serde::Serialize;
use std::sync::Arc;

/// 復盤設定。
#[derive(Debug, Clone)]
pub struct Options {
    /// 要復盤的座位（空 = 四家）
    pub seats: Vec<Seat>,
    /// 是否對每個決策都做反事實評估（預設只評估玩家與策略選擇不同的決策）
    pub all: bool,
    /// 反事實評估的設定（世界數、衡量方式、是否重抽看不到的牌）
    pub eval: counterfactual::Options,
}

/// 反事實評估的結果。
///
/// 「失誤」只比較玩家與策略的選擇：兩者在同一組世界中重打，成對差值是無偏的估計。
/// 不拿「期望值最高的選項」當標準，因為最高者是從許多帶雜訊的估計中挑出來的，本身就系統性偏高。
#[derive(Debug, Clone, Serialize)]
pub struct Evaluation {
    /// 各選項的平均結果（名次點或點數）
    pub option_ev: Vec<f64>,
    /// 期望值最高的選項（參考用）
    pub best: usize,
    /// 策略的選擇比玩家的選擇多多少（正 = 策略較好）
    pub diff: f64,
    /// `diff` 的標準誤
    pub diff_se: f64,
    /// 使用的世界數
    pub rollouts: usize,
}

/// 單一決策標成「明顯失誤」（或「玩家明顯較好」）需要的差距：幾倍標準誤。
///
/// 一場對局有幾十個選擇不同的決策要評估，用 2 倍標準誤時，即使兩個選項其實一樣好，
/// 每個方向也約有 2.3% 會被判成顯著（5 場 20 位玩家 1282 個決策中，誤報約 30 個），
/// 所以單一決策用 3 倍（誤報約 0.13%）。整體的傾向看 [`Summary`] 的平均差值。
pub const MISTAKE_Z: f64 = 3.0;

impl Evaluation {
    /// 明顯失誤：策略的選擇比玩家好，差距大於 [`MISTAKE_Z`] 倍標準誤。
    pub fn is_mistake(&self) -> bool {
        self.diff > 0.0 && self.diff > MISTAKE_Z * self.diff_se
    }

    /// 玩家的選擇明顯比策略好（同樣的門檻）。
    pub fn player_better(&self) -> bool {
        self.diff < 0.0 && -self.diff > MISTAKE_Z * self.diff_se
    }
}

/// 一個決策的復盤。
#[derive(Debug, Clone, Serialize)]
pub struct Reviewed {
    /// 座位
    pub seat: Seat,
    /// 對局中的第幾局（從 0 開始）
    pub hand_index: u32,
    /// 這個座位第幾次被詢問（快照點）
    pub act_index: u32,
    /// 局名，例如「東1局 0本場」
    pub round: String,
    /// 自己打過幾張（巡目）
    pub turn: u8,
    /// 決策種類
    pub kind: DecisionKind,
    /// 選項
    pub options: Vec<String>,
    /// 策略的選擇
    pub strategy: usize,
    /// 玩家的選擇
    pub player: usize,
    /// 策略的理由
    pub notes: Vec<String>,
    /// 玩家與策略選擇不同時的差異類型（例如「向聽倒退」「漏鳴」）
    pub difference: Option<String>,
    /// 反事實評估（只有評估過的決策才有）
    pub eval: Option<Evaluation>,
}

impl Reviewed {
    /// 玩家與策略的選擇相同。
    pub fn agrees(&self) -> bool {
        self.strategy == self.player
    }
}

/// 一場對局的復盤。
#[derive(Debug, Clone, Serialize)]
pub struct GameReview {
    /// 牌譜 ID
    pub id: String,
    /// 四家名稱
    pub players: [String; 4],
    /// 復盤的座位
    pub seats: Vec<Seat>,
    /// 衡量方式
    pub metric: counterfactual::Metric,
    /// 每個可比較的決策（依時間順序）
    pub decisions: Vec<Reviewed>,
    /// 玩家走了另一條路而無法比較的決策數
    pub incomparable: usize,
    /// 需要評估、但快照點不支援（例如搶槓）而略過的決策數
    pub skipped: usize,
}

/// 局名，例如「東1局 0本場」。
pub fn round_name(view: &PlayerView) -> String {
    let wind = ["東", "南", "西", "北"][usize::from(view.round_wind % 4)];
    format!("{wind}{}局 {}本場", view.kyoku + 1, view.honba)
}

/// 復盤一場對局。
pub fn review_game(
    strategy: &Arc<dyn Strategy>,
    rec: &GameRecord,
    opts: &Options,
) -> Result<GameReview, String> {
    let seats: Vec<Seat> = if opts.seats.is_empty() {
        vec![0, 1, 2, 3]
    } else {
        opts.seats.clone()
    };
    // 1. 重播：四家旁觀者記錄決策，牌譜記錄每次詢問的實際動作。
    let mut shadows: Vec<Box<dyn StrategyPlayer>> = (0..4u8)
        .map(|s| strategy.player(seats.contains(&s), None))
        .collect();
    let replayed = {
        let refs: [Option<&mut dyn mj_engine::Player>; 4] = {
            let mut it = shadows
                .iter_mut()
                .map(|p| Some(p.as_mut() as &mut dyn mj_engine::Player));
            std::array::from_fn(|_| it.next().unwrap())
        };
        replay(rec, refs, true)?
    };

    // 2. 把玩家的動作對應到策略的決策選項。
    let mut decisions = Vec::new();
    let mut incomparable = 0;
    for &seat in &seats {
        let records = shadows[usize::from(seat)].take_records();
        for (i, r) in records.iter().enumerate() {
            if r.options.len() < 2 || r.kind == DecisionKind::Predict {
                continue;
            }
            let Some(point) = replayed
                .points
                .iter()
                .find(|p| p.seat == seat && p.ask_index == r.act_index)
            else {
                continue;
            };
            let riichi_pai = (r.kind == DecisionKind::Riichi)
                .then(|| strategy_discard(&records, i, point))
                .flatten();
            match option_of_action(r.kind, &point.legal, &point.chosen, riichi_pai) {
                Some(player) => decisions.push(reviewed(seat, r, point, player)),
                None => incomparable += 1,
            }
        }
    }
    decisions.sort_by_key(|d| (d.hand_index, d.turn, d.seat));

    // 3. 反事實評估：選擇不同（或全部）的決策。
    let rules = replayed.cfg.rules.clone();
    let targets: Vec<usize> = (0..decisions.len())
        .filter(|&i| opts.all || !decisions[i].agrees())
        .collect();
    let evals: Vec<(usize, Option<Evaluation>)> = targets
        .par_iter()
        .map(|&i| {
            let d = &decisions[i];
            (i, evaluate(strategy, rec, &rules, d, opts))
        })
        .collect();
    let mut skipped = 0;
    for (i, e) in evals {
        match e {
            Some(e) => decisions[i].eval = Some(e),
            None => skipped += 1,
        }
    }
    Ok(GameReview {
        id: rec.id.clone(),
        players: rec.players.clone(),
        seats,
        metric: opts.eval.metric,
        decisions,
        incomparable,
        skipped,
    })
}

/// 立直決策時策略要打的牌：同一次詢問中策略何切的選擇；沒有何切決策（只能打一張）時是唯一的候選牌。
fn strategy_discard(records: &[DecisionRecord], i: usize, point: &DecisionPoint) -> Option<Pai> {
    let r = &records[i];
    let discard = records[..i]
        .iter()
        .rev()
        .find(|d| d.act_index == r.act_index && d.kind == DecisionKind::Discard);
    match discard {
        Some(d) => parse_discard_label(&d.options[d.chosen]),
        None => point.legal.iter().find_map(|a| match a {
            Action::Riichi { pai, .. } => Some(*pai),
            _ => None,
        }),
    }
}

/// 何切選項的文字「打 5m」→ 牌。
pub fn parse_discard_label(label: &str) -> Option<Pai> {
    label.strip_prefix("打 ")?.parse().ok()
}

fn reviewed(seat: Seat, r: &DecisionRecord, point: &DecisionPoint, player: usize) -> Reviewed {
    let difference =
        (player != r.chosen).then(|| classify(r.kind, &r.options, r.chosen, player, &point.view));
    Reviewed {
        seat,
        hand_index: r.hand_index,
        act_index: r.act_index,
        round: round_name(&point.view),
        turn: r.situation.turn,
        kind: r.kind,
        options: r.options.clone(),
        strategy: r.chosen,
        player,
        notes: r.notes.clone(),
        difference,
        eval: None,
    }
}

/// 評估一個決策（快照點不支援或重播沒有走到時為 `None`）。
fn evaluate(
    strategy: &Arc<dyn Strategy>,
    rec: &GameRecord,
    rules: &mj_rules::riichi::RiichiConfig,
    d: &Reviewed,
    opts: &Options,
) -> Option<Evaluation> {
    // 快照當下的四家策略玩家：重播時一起旁觀，記憶與詢問次數都接得上。
    let mut base: Vec<Box<dyn StrategyPlayer>> =
        (0..4).map(|_| strategy.player(false, None)).collect();
    let (_, snap) = {
        let refs: [Option<&mut dyn mj_engine::Player>; 4] = {
            let mut it = base
                .iter_mut()
                .map(|p| Some(p.as_mut() as &mut dyn mj_engine::Player));
            std::array::from_fn(|_| it.next().unwrap())
        };
        capture(rec, refs, d.seat, d.act_index).ok()??
    };
    let point = Point {
        seat: d.seat,
        act_index: d.act_index,
        kind: d.kind,
        options: d.options.len(),
    };
    let seed = hand_seed(
        u64::from(d.hand_index) ^ 0x5EE_D0FF,
        (u32::from(d.seat) << 24) ^ d.act_index,
    );
    // 選擇不同時，增加世界數直到玩家與策略的差距顯著（或到上限）；相同時與最佳選項比較。
    let settle = if d.agrees() {
        Settle::AgainstBest
    } else {
        Settle::Against(d.strategy)
    };
    let e = evaluate_snapshot(
        rules, &snap, &base, point, d.player, settle, seed, &opts.eval,
    )?;
    let (diff, diff_se) = paired_diff(&e.per_option, d.strategy, d.player);
    Some(Evaluation {
        option_ev: e.option_ev,
        best: e.best,
        diff,
        diff_se,
        rollouts: e.rollouts,
    })
}

/// 玩家與策略選擇不同時的差異類型。
fn classify(
    kind: DecisionKind,
    options: &[String],
    strategy: usize,
    player: usize,
    view: &PlayerView,
) -> String {
    match kind {
        DecisionKind::Kyuushu if player == 1 => "流局（策略繼續打）".into(),
        DecisionKind::Kyuushu => "繼續打（策略流局）".into(),
        DecisionKind::Win if player == 1 => "和牌（策略不和）".into(),
        DecisionKind::Win => "見逃（策略和牌）".into(),
        DecisionKind::Kan if strategy == 0 => "開槓（策略不槓）".into(),
        DecisionKind::Kan if player == 0 => "不槓（策略開槓）".into(),
        DecisionKind::Kan => "槓法不同".into(),
        DecisionKind::Riichi if player == 1 => "立直（策略默聽）".into(),
        DecisionKind::Riichi => "默聽（策略立直）".into(),
        DecisionKind::Call if strategy == 0 => "多鳴（策略不鳴）".into(),
        DecisionKind::Call if player == 0 => "漏鳴（策略會鳴）".into(),
        DecisionKind::Call => "鳴法不同".into(),
        DecisionKind::Discard => classify_discard(options, strategy, player, view),
        DecisionKind::Predict => String::new(),
    }
}

/// 何切的差異類型：先看有人立直時的攻守，再看牌效。
///
/// - 有人立直：玩家打現物、策略打非現物 →「過度防守」；反過來 →「放銃風險」；
/// - 向聽數比策略的選擇多 →「向聽倒退」；
/// - 向聽相同、進張少 2 張以上 →「進張較少」；進張完全相同 →「牌效相同（打牌順序）」；
/// - 打出寶牌而策略留著 →「打掉寶牌」；
/// - 其他 →「牌效相近（打點、形狀）」。
fn classify_discard(
    options: &[String],
    strategy: usize,
    player: usize,
    view: &PlayerView,
) -> String {
    let (Some(s), Some(p)) = (
        parse_discard_label(&options[strategy]),
        parse_discard_label(&options[player]),
    ) else {
        return "何切".into();
    };
    let riichi = mj_tools::table::riichi_opponents(view);
    if !riichi.is_empty() {
        let safe = |pai: Pai| {
            riichi
                .iter()
                .all(|&r| mj_tools::safety::is_genbutsu(view, r, pai.tile))
        };
        match (safe(p), safe(s)) {
            (true, false) => return "過度防守（策略選擇對攻）".into(),
            (false, true) => return "放銃風險（策略打現物）".into(),
            _ => {}
        }
    }
    let after = |pai: Pai| {
        mj_tools::after::after(
            view,
            &Action::Discard {
                pai,
                tsumogiri: false,
            },
        )
    };
    let (a_p, a_s) = (after(p), after(s));
    if a_p.shanten > a_s.shanten {
        "向聽倒退".into()
    } else if a_p.shanten == a_s.shanten && a_p.ukeire + 2.0 <= a_s.ukeire {
        "進張較少".into()
    } else if a_p.shanten == a_s.shanten && a_p.ukeire == a_s.ukeire {
        "牌效相同（打牌順序）".into()
    } else if mj_tools::dora::pai_dora_value(view, p) > mj_tools::dora::pai_dora_value(view, s) {
        "打掉寶牌".into()
    } else {
        "牌效相近（打點、形狀）".into()
    }
}

/// 判斷一組決策的平均差值是否顯著時，至少要有幾個評估過的決策（太少時標準誤本身就估不準）。
pub const MIN_GROUP: usize = 10;

/// 一組決策（某種決策或某種差異類型）的統計。
#[derive(Debug, Clone, Default, Serialize)]
pub struct GroupSummary {
    /// 組名（決策種類或差異類型）
    pub label: String,
    /// 決策數
    pub decisions: usize,
    /// 與策略選擇相同的比例
    pub agree: f64,
    /// 選擇不同、而且做過反事實評估的決策數
    pub evaluated: usize,
    /// 這些決策的平均差值：策略的選擇 − 玩家的選擇（正 = 策略較好，也就是玩家平均損失多少）
    pub mean_diff: f64,
    /// 平均差值的標準誤（決策之間的差異＋每個決策的估計誤差）
    pub mean_diff_se: f64,
    /// 明顯失誤數（單一決策差距大於 [`MISTAKE_Z`] 倍標準誤）
    pub mistakes: usize,
    /// 玩家明顯較好的決策數
    pub player_better: usize,
}

impl GroupSummary {
    /// 由一組決策計算。
    pub fn of(label: String, ds: &[&Reviewed]) -> Self {
        let diffs: Vec<f64> = ds
            .iter()
            .filter(|d| !d.agrees())
            .filter_map(|d| d.eval.as_ref().map(|e| e.diff))
            .collect();
        let n = diffs.len() as f64;
        let mean = if diffs.is_empty() {
            0.0
        } else {
            diffs.iter().sum::<f64>() / n
        };
        let var = diffs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
        let evals = || {
            ds.iter()
                .filter_map(|d| d.eval.as_ref().filter(|_| !d.agrees()))
        };
        GroupSummary {
            label,
            decisions: ds.len(),
            agree: ds.iter().filter(|d| d.agrees()).count() as f64 / ds.len().max(1) as f64,
            evaluated: diffs.len(),
            mean_diff: mean,
            mean_diff_se: if diffs.len() > 1 {
                (var / n).sqrt()
            } else {
                0.0
            },
            mistakes: evals().filter(|e| e.is_mistake()).count(),
            player_better: evals().filter(|e| e.player_better()).count(),
        }
    }

    /// 平均差值是否顯著（至少 [`MIN_GROUP`] 個評估過的決策，而且大於 2 倍標準誤）：
    /// 正 = 策略的選擇平均較好，負 = 玩家平均較好。
    pub fn significant(&self) -> bool {
        self.evaluated >= MIN_GROUP && self.mean_diff.abs() > 2.0 * self.mean_diff_se
    }
}

/// 多場復盤的彙整。
#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    /// 全部決策
    pub overall: GroupSummary,
    /// 依決策種類
    pub by_kind: Vec<GroupSummary>,
    /// 依差異類型（只含與策略選擇不同的決策；依次數排序）
    pub by_type: Vec<GroupSummary>,
}

/// 彙整多場復盤。
///
/// 單一決策的反事實評估雜訊很大，很少能單獨判定好壞；同一類決策的**平均差值**才是可靠的訊號
/// （例如「漏鳴」平均讓玩家多得或少得多少點）。平均差值顯著為負時，代表玩家在這類局面比策略好，
/// 是改進策略的線索。
pub fn summarize(reviews: &[GameReview]) -> Summary {
    let all: Vec<&Reviewed> = reviews.iter().flat_map(|r| r.decisions.iter()).collect();
    let by_kind = DecisionKind::ALL
        .iter()
        .filter_map(|&k| {
            let v: Vec<&Reviewed> = all.iter().copied().filter(|d| d.kind == k).collect();
            (!v.is_empty()).then(|| GroupSummary::of(k.name().to_string(), &v))
        })
        .collect();
    let mut types: Vec<String> = Vec::new();
    for d in &all {
        if let Some(t) = &d.difference
            && !types.contains(t)
        {
            types.push(t.clone());
        }
    }
    let mut by_type: Vec<GroupSummary> = types
        .into_iter()
        .map(|t| {
            let v: Vec<&Reviewed> = all
                .iter()
                .copied()
                .filter(|d| d.difference.as_ref() == Some(&t))
                .collect();
            GroupSummary::of(t, &v)
        })
        .collect();
    by_type.sort_by_key(|g| std::cmp::Reverse(g.decisions));
    Summary {
        overall: GroupSummary::of("全部".into(), &all),
        by_kind,
        by_type,
    }
}
