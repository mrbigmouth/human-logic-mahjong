//! 把決策紀錄排成給人看的文字。

use super::claim::Claim;
use super::player::DecisionRecord;
use mj_core::Pai;
use mj_engine::Action;
use std::fmt::Write;

/// 動作的中文名稱，例如「打 5m（摸切）」「碰（用 5z5z）」。
pub fn action_text(action: &Action) -> String {
    let pais = |ps: &[Pai]| ps.iter().map(|p| p.to_string()).collect::<String>();
    let tg = |t: bool| if t { "（摸切）" } else { "" };
    match action {
        Action::Discard { pai, tsumogiri } => format!("打 {pai}{}", tg(*tsumogiri)),
        Action::Riichi { pai, tsumogiri } => format!("立直 打 {pai}{}", tg(*tsumogiri)),
        Action::Chi { consumed } => format!("吃（用 {}）", pais(consumed)),
        Action::Pon { consumed } => format!("碰（用 {}）", pais(consumed)),
        Action::Daiminkan => "明槓".into(),
        Action::Ankan { tile } => format!("暗槓 {tile}"),
        Action::Kakan { pai } => format!("加槓 {pai}"),
        Action::Tsumo => "自摸".into(),
        Action::Ron => "榮和".into(),
        Action::KyuushuKyuuhai => "九種九牌（流局）".into(),
        Action::Pass => "跳過".into(),
    }
}

/// 預測的中文說明，例如「座位 2 聽 3z（打出會放銃）的機率 3%」。
pub fn claim_text(c: &Claim) -> String {
    match *c {
        Claim::Tenpai { seat, p } => format!("座位 {seat} 已聽牌的機率 {:.0}%", p * 100.0),
        Claim::Waits { seat, tile, p } => {
            format!("打 {tile} 被座位 {seat} 榮和的機率 {:.0}%", p * 100.0)
        }
        Claim::IWinThisHand { p } => format!("本局由我和牌的機率 {:.0}%", p * 100.0),
    }
}

/// 終端機顯示寬度：ASCII 佔 1 格，中文等全形字元佔 2 格。
pub fn display_width(s: &str) -> usize {
    s.chars().map(|c| if c.is_ascii() { 1 } else { 2 }).sum()
}

/// 依顯示寬度在右側補空白到 `width` 格（中文對齊用）。
pub fn pad(s: &str, width: usize) -> String {
    format!("{s}{}", " ".repeat(width.saturating_sub(display_width(s))))
}

/// 依顯示寬度在左側補空白到 `width` 格（數字靠右對齊用）。
pub fn pad_left(s: &str, width: usize) -> String {
    format!("{}{s}", " ".repeat(width.saturating_sub(display_width(s))))
}

/// 一筆決策紀錄的文字：決策種類、選擇、理由、預測。
pub fn render_record(r: &DecisionRecord) -> String {
    let mut out = String::new();
    let chosen = r.options.get(r.chosen).map_or("?", String::as_str);
    let _ = writeln!(
        out,
        "【{}】選擇：{chosen}（選項 {} 個）",
        r.kind.name(),
        r.options.len()
    );
    for n in &r.notes {
        let _ = writeln!(out, "  ・{n}");
    }
    for c in &r.claims {
        let _ = writeln!(out, "  ◇ 預測（{}）{}", c.source, claim_text(&c.claim));
    }
    if r.invalid {
        let _ = writeln!(out, "  ！決策函式回傳了不在選項中的結果，已改用預設選項");
    }
    out
}
