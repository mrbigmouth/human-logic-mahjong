//! 局面快照：在某個詢問點保存完整局面，之後可以從這裡繼續打（反事實評估用）。
//!
//! - [`crate::capture_snapshot`]：重播一場對局，在「即將第 n 次詢問某座位」時保存局面；
//! - [`Snapshot::determinize`]：依決策者看得到的資訊，重新分配看不到的牌（對手的門前牌、
//!   未摸的牌山與王牌），去除「偷看對手手牌」；
//! - [`crate::resume_game`]：從快照繼續，打到這一局結束或整場結束。

use crate::kyoku::{KyokuState, Resume};
use crate::wall::{LIVE_WALL, full_set};
use crate::{PlayerView, Seat};
use mj_calc::waits;
use mj_core::{Pai, Tile, TileCounts};
use mj_rules::riichi::RiichiState;
use rand::Rng;
use rand::seq::SliceRandom;

/// 保存的局面。
#[derive(Clone)]
pub struct Snapshot {
    pub(crate) state: KyokuState,
    pub(crate) resume: Resume,
    pub(crate) red_fives: u8,
}

/// 重抽的結果。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeterminizeReport {
    /// 已立直的對手中，找不到「聽牌且沒有振聽」的重抽手牌、只好保留原本手牌的人數
    pub riichi_fallbacks: usize,
}

/// 重抽立直者手牌時最多嘗試幾次。
const RIICHI_TRIES: usize = 2_000;

impl Snapshot {
    /// 即將被詢問的座位（決策者）。
    pub fn asked_seat(&self) -> Seat {
        match &self.resume {
            Resume::OwnTurn { seat, .. } => *seat,
            Resume::Response { discarder, k, .. } => (discarder + k) % 4,
        }
    }

    /// 決策者在這個詢問點看到的資訊（與原對局中他當下收到的 `PlayerView` 相同）。
    pub fn view(&self) -> PlayerView {
        match &self.resume {
            Resume::OwnTurn { seat, drawn, .. } => self.state.view(*seat, *drawn, None),
            Resume::Response { discarder, pai, .. } => {
                self.state
                    .view(self.asked_seat(), None, Some((*discarder, *pai)))
            }
        }
    }

    /// 這一局開始時的四家點數。
    pub fn hand_start_scores(&self) -> [i32; 4] {
        self.state.setup.scores
    }

    /// 這一局是對局中的第幾局（從 0 開始）。
    pub fn hand_index(&self) -> u32 {
        self.state.setup.hands_played
    }

    /// 依決策者看得到的資訊，重新分配所有看不到的牌。
    ///
    /// 可分配的牌 = 全部牌 − 決策者看得到的牌（自己的手牌、四家牌河、四家副露、已翻開的寶牌指示牌）。
    /// 只從決策者的視角計算，所以不會用到隱藏資訊。這些牌隨機分給：
    /// - 對手的門前手牌（張數不變）；
    /// - 未摸的牌山、未用的嶺上牌、未翻開的寶牌與裏寶牌指示牌。
    ///
    /// 已立直的對手必須拿到「聽牌、而且沒有振聽」的手牌（聽牌張不在自己的牌河，
    /// 也不在他立直後別人打出的牌中），以建構方式抽樣（見 `sample_tenpai`）；試太多次仍失敗時保留原本手牌。
    pub fn determinize(&mut self, rng: &mut impl Rng) -> DeterminizeReport {
        let decider = self.asked_seat();
        let st = &mut self.state;
        let mut report = DeterminizeReport::default();

        // 決策者看得到的牌（以 Pai 計，含赤五）。
        let mut visible: Vec<Pai> = st.p[usize::from(decider)].hand.clone();
        for ps in &st.p {
            visible.extend(ps.river.iter().filter(|d| !d.called).map(|d| d.pai));
            for m in &ps.melds {
                visible.extend(m.tiles.iter().copied());
            }
        }
        let revealed = st.wall.revealed();
        let kans = st.wall.kans();
        visible.extend((0..revealed).map(|i| st.wall.tiles_mut()[LIVE_WALL + 4 + i]));
        let mut pool = full_set(self.red_fives);
        for v in &visible {
            let i = pool
                .iter()
                .position(|p| p == v)
                .expect("visible tile exists in the full set");
            pool.swap_remove(i);
        }

        // 看不到的位置：對手門前牌與牌山中未摸、未翻開的部分。
        let next = st.wall.next_index();
        let wall_slots: Vec<usize> = (next..LIVE_WALL)
            .chain(LIVE_WALL + kans..LIVE_WALL + 4)
            .chain(LIVE_WALL + 4 + revealed..LIVE_WALL + 14)
            .collect();
        let opponents: Vec<usize> = (0..4).filter(|&s| s != usize::from(decider)).collect();
        let hidden_hand_tiles: usize = opponents.iter().map(|&s| st.p[s].hand.len()).sum();
        assert_eq!(
            pool.len(),
            hidden_hand_tiles + wall_slots.len(),
            "unseen tiles must fill every hidden slot"
        );

        pool.shuffle(rng);
        // 先處理立直者（有限制），再處理其他對手。
        let mut order = opponents.clone();
        order.sort_by_key(|&s| st.p[s].riichi == RiichiState::None);
        for &s in &order {
            let n = st.p[s].hand.len();
            let new_hand = if st.p[s].riichi != RiichiState::None {
                let forbidden = riichi_forbidden(st, s);
                match sample_tenpai(&mut pool, n, &forbidden, rng) {
                    Some(h) => h,
                    None => {
                        report.riichi_fallbacks += 1;
                        // 保留原本的手牌：把那些牌從可分配的牌中取出。
                        let keep = st.p[s].hand.clone();
                        for t in &keep {
                            let i = pool
                                .iter()
                                .position(|p| p == t)
                                .expect("kept tile is unseen");
                            pool.swap_remove(i);
                        }
                        keep
                    }
                }
            } else {
                pool.drain(..n).collect()
            };
            let ps = &mut st.p[s];
            ps.hand = new_hand;
            ps.hand.sort();
            ps.waits = waits(&ps.counts());
            ps.temp_furiten = false;
            ps.riichi_furiten = false;
        }
        pool.shuffle(rng);
        let tiles = st.wall.tiles_mut();
        for (slot, pai) in wall_slots.into_iter().zip(pool) {
            tiles[slot] = pai;
        }
        // 回應階段中、在決策者之前已經回答的對手，是依原本（真實）的手牌決定的：
        // 手牌重抽後要從頭重新詢問，否則他們的吃碰會用到已經不在手上的牌（也等於洩漏了他們的決定）。
        if let Resume::Response {
            start,
            rons,
            call,
            chi,
            ..
        } = &mut self.resume
        {
            *start = 1;
            rons.clear();
            *call = None;
            *chi = None;
        }
        report
    }
}

/// 立直者不能聽的牌種：自己的牌河（捨牌振聽），以及他立直後別人打出的牌（他沒有和 → 立直振聽）。
fn riichi_forbidden(st: &KyokuState, s: usize) -> Vec<Tile> {
    let river = &st.p[s].river;
    let mut out: Vec<Tile> = river.iter().map(|d| d.pai.tile).collect();
    if let Some(rs) = river.iter().find(|d| d.riichi_declared).map(|d| d.seq) {
        for (o, ps) in st.p.iter().enumerate() {
            if o != s {
                out.extend(ps.river.iter().filter(|d| d.seq > rs).map(|d| d.pai.tile));
            }
        }
    }
    out
}

/// 從 `pool` 抽出 `n`（= 3k+1）張聽牌、且聽牌張不在 `forbidden` 中的手牌。成功時從 pool 移除。
///
/// 純隨機抽 13 張幾乎不會聽牌，所以用建構的方式：從 pool 中隨機組出 k 組面子（順子或刻子）
/// 與一組雀頭，再隨機拿掉一張——剩下的牌一定聽牌（至少聽被拿掉的那張）。最後檢查振聽限制。
fn sample_tenpai(
    pool: &mut Vec<Pai>,
    n: usize,
    forbidden: &[Tile],
    rng: &mut impl Rng,
) -> Option<Vec<Pai>> {
    let sets = n / 3;
    let available = TileCounts::from_tiles(pool.iter().map(|p| p.tile));
    for _ in 0..RIICHI_TRIES {
        let Some(mut tiles) = build_complete(&available, sets, rng) else {
            continue;
        };
        tiles.remove(rng.random_range(0..tiles.len()));
        let w = waits(&TileCounts::from_tiles(tiles.iter().copied()));
        if w.is_empty() || w.iter().any(|t| forbidden.contains(t)) {
            continue;
        }
        // 依牌種從 pool 取出實際的牌（赤五與否隨 pool 的順序而定，pool 已洗過）。
        let mut hand = Vec::with_capacity(n);
        for t in tiles {
            let i = pool
                .iter()
                .position(|p| p.tile == t)
                .expect("tile available");
            hand.push(pool.swap_remove(i));
        }
        return Some(hand);
    }
    None
}

/// 從可用的牌中隨機組出 `sets` 組面子＋一組雀頭（3·sets + 2 張）。組不出來時回傳 `None`。
fn build_complete(available: &TileCounts, sets: usize, rng: &mut impl Rng) -> Option<Vec<Tile>> {
    let mut left = *available;
    let mut out = Vec::with_capacity(sets * 3 + 2);
    let take = |left: &mut TileCounts, ts: &[Tile]| -> bool {
        let mut trial = *left;
        if ts.iter().all(|&t| trial.remove(t)) {
            *left = trial;
            true
        } else {
            false
        }
    };
    for _ in 0..sets {
        let mut ok = false;
        for _ in 0..50 {
            let t = Tile::from_index(rng.random_range(0..34u8));
            let shape: Vec<Tile> = if !t.is_honor() && t.number() <= 7 && rng.random_bool(0.6) {
                (0..3)
                    .map(|k| Tile::from_index(t.index() as u8 + k))
                    .collect()
            } else {
                vec![t; 3]
            };
            if take(&mut left, &shape) {
                out.extend(shape);
                ok = true;
                break;
            }
        }
        if !ok {
            return None;
        }
    }
    for _ in 0..50 {
        let t = Tile::from_index(rng.random_range(0..34u8));
        if take(&mut left, &[t, t]) {
            out.extend([t, t]);
            return Some(out);
        }
    }
    None
}
