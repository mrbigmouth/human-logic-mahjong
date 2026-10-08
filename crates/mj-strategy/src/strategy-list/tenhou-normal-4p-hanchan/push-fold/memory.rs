//! 對局記憶：一整場對局（半莊）中，所有決策共享的資訊。每場對局開始時重新建立。

use crate::prelude::*;

/// 從第幾巡開始留意各家的摸切：某家打過這麼多張牌之後才開始記。
/// （人類通常不會記住序盤所有的摸切，中巡以後才開始注意誰停止換牌。）
pub const WATCH_FROM_TURN: usize = 6;

/// 對局記憶。
#[derive(Default, Clone)]
pub struct Memory {
    /// 本局各家打過的牌（自己整理的牌河，觀察事件時更新）
    pub rivers: [Vec<Tile>; 4],
    /// 本局各家從中巡開始「連續摸切」的次數：手切或鳴牌就歸 0。
    /// 連續摸切代表手牌沒有變化，越多次越可能已經聽牌（或手牌卡住）。
    pub tsumogiri_streak: [u32; 4],
    /// 各家在本場對局中立直了幾次
    pub riichi_count: [u32; 4],
    /// 各家用「筋牌」和牌（筋牌騙）的次數：次數多的人，他的筋牌不可信
    pub suji_trap_wins: [u32; 4],
}

/// 觀察每個事件，更新記憶。
pub fn observe(me: Seat, event: &Event, mem: &mut Memory) {
    use mj_engine::HandEnd;
    match event {
        // 新的一局：本局的記憶重設，跨局的記憶（立直次數、筋牌騙）保留。
        Event::StartKyoku { .. } => {
            mem.rivers = Default::default();
            mem.tsumogiri_streak = [0; 4];
        }
        Event::Discard {
            seat,
            pai,
            tsumogiri,
            ..
        } => {
            let s = usize::from(*seat);
            mem.rivers[s].push(pai.tile);
            // 只留意他家，而且從中巡才開始記：摸切 +1，手切代表手牌有變化，歸 0。
            if *seat != me && mem.rivers[s].len() >= WATCH_FROM_TURN {
                if *tsumogiri {
                    mem.tsumogiri_streak[s] += 1;
                } else {
                    mem.tsumogiri_streak[s] = 0;
                }
            }
        }
        // 鳴牌改變了手牌結構，重新觀察。
        Event::Call { seat, .. } => mem.tsumogiri_streak[usize::from(*seat)] = 0,
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
