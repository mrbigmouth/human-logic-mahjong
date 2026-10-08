//! 對局記憶：一整場對局（半莊）中，所有決策共享的資訊。每場對局開始時重新建立。

use crate::prelude::*;

/// 對局記憶。
#[derive(Default, Clone)]
pub struct Memory {
    /// 本局各家打過的牌（自己整理的牌河，觀察事件時更新）
    pub rivers: [Vec<Tile>; 4],
    /// 各家在本場對局中立直了幾次
    pub riichi_count: [u32; 4],
    /// 各家用「筋牌」和牌（筋牌騙）的次數：次數多的人，他的筋牌不可信
    pub suji_trap_wins: [u32; 4],
}

/// 觀察每個事件，更新記憶。
pub fn observe(_me: Seat, event: &Event, mem: &mut Memory) {
    use mj_engine::HandEnd;
    match event {
        // 新的一局：本局的記憶重設，跨局的記憶（立直次數、筋牌騙）保留。
        Event::StartKyoku { .. } => {
            mem.rivers = Default::default();
        }
        Event::Discard { seat, pai, .. } => mem.rivers[usize::from(*seat)].push(pai.tile),
        Event::RiichiAccepted { seat } => mem.riichi_count[usize::from(*seat)] += 1,
        Event::HandEnd {
            end:
                HandEnd::Win {
                    winner,
                    from,
                    win_tile,
                    ..
                },
            ..
        } if winner != from => {
            // 榮和的牌是和牌者自己牌河的筋牌 → 他用了筋牌騙。
            let river = &mem.rivers[usize::from(*winner)];
            if is_suji_of(river, win_tile.tile) {
                mem.suji_trap_wins[usize::from(*winner)] += 1;
            }
        }
        _ => {}
    }
}

/// `tile` 是否為這條牌河的筋牌（數牌 n 的 n±3 在牌河中；4–6 要兩邊都有）。
fn is_suji_of(river: &[Tile], tile: Tile) -> bool {
    if tile.is_honor() {
        return false;
    }
    let n = tile.number();
    let i = tile.index() as u8;
    let has = |k: u8| river.contains(&Tile::from_index(k));
    // 1–3 看 +3，7–9 看 −3，4–6 兩邊都要。
    let low = n <= 3 || has(i - 3);
    let high = n >= 7 || has(i + 3);
    low && high
}
