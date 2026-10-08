//! 天鳳一般規則・四人半莊的策略（說明見同目錄的 README.md）。
//!
//! 這裡登記「什麼情況呼叫哪一個決策」。框架呼叫的順序：
//! - 自己的回合：九種九牌 → 和牌與否 → 開槓 → 何切 → 立直與否
//! - 他家打牌時：和牌與否 → 鳴牌

use super::{decision, memory, utils};
use crate::prelude::*;
use memory::Memory;

pub fn register(book: &mut Book<Memory>) {
    book.kyuushu(decision::kyuushu::decide);
    book.win(decision::win::decide);
    book.kan(decision::kan::decide);
    book.discard(decision::discard::decide);
    book.riichi(decision::riichi::decide);
    book.call(decision::call::decide);
    book.observe(memory::observe);
    book.predict_each_turn(|ctx, mem| {
        utils::predict::tenpai::each_turn(ctx, mem);
        utils::predict::win::each_turn(ctx, mem);
    });
}
