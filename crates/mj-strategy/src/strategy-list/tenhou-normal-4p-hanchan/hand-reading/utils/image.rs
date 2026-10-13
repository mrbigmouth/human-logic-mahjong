//! 自我形象：把自己的讀牌預測（`utils/predict/`）用在自己的座位上，看「別人眼中的我」。
//!
//! 對手如果也會讀牌，他看到的就是這些線索：手切摸切、最後手切的牌、筋……
//! 印象操作（M10）就是在不損失（或很少損失）牌效的前提下，選擇對自己有利的形象：
//! - 聽牌時：看起來越不像聽牌越好，聽牌張看起來越安全越好（對手才會推、才會放銃）；
//! - 離聽牌還遠時：看起來像聽牌（推進的人會收手，減少他們和牌）。
//!
//! 這裡假設對手讀牌的方式和自己一樣——這正是「用自己的讀牌模型看自己」的意思。

use super::super::memory::Memory;
use super::predict::{tenpai, waits};
use crate::prelude::*;

/// 打出 `pai` 之後，別人眼中我聽牌的機率。
pub fn apparent_tenpai(ctx: &Ctx, mem: &Memory, pai: Pai, tsumogiri: bool, riichi: bool) -> f64 {
    let v = tools::river::view_after_discard(ctx.view, pai, tsumogiri, riichi);
    tenpai::estimate(&ctx.imagine(&v), mem, ctx.me())
}

/// 打出 `pai`（聽牌）之後，我的聽牌張在別人眼中的平均危險度（他們估計我聽這張的機率，依剩餘張數加權）。
/// `riichi` = 假設立直（立直者與未立直者的讀法不同）。不聽牌時回傳 `None`。
pub fn apparent_wait_danger(ctx: &Ctx, mem: &Memory, pai: Pai, riichi: bool) -> Option<f64> {
    let mut hand = ctx.hand();
    hand.remove(pai.tile);
    let waits = tools::hand::waits_with_remaining(ctx.view, &hand);
    let total: f32 = waits.iter().map(|&(_, n)| n).sum();
    if waits.is_empty() || total <= 0.0 {
        return None;
    }
    let tsumogiri = ctx.view.drawn == Some(pai);
    let v = tools::river::view_after_discard(ctx.view, pai, tsumogiri, riichi);
    let image = ctx.imagine(&v);
    let me = ctx.me();
    let danger: f64 = waits
        .iter()
        .map(|&(t, n)| f64::from(n) * waits::estimate(&image, mem, me, t))
        .sum();
    Some(danger / f64::from(total))
}
