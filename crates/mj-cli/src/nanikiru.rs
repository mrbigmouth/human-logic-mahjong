//! `mj nanikiru`：何切（打哪張）分析。輸入一手 14 張的牌，執行策略的決策並顯示判斷理由，
//! 另外附上每種打法的牌效參考表。

use mj_calc::{is_agari, waits};
use mj_core::{Pai, TileCounts, parse_pais, parse_tiles};
use mj_engine::{Action, PlayerView};
use mj_strategy::find_strategy;
use mj_strategy::framework::explain::{action_text, pad, render_record};

/// 分析設定。
pub struct Options<'a> {
    /// 手牌（14 張，最後一張視為剛摸到的牌）
    pub hand: &'a str,
    /// 寶牌指示牌
    pub dora: &'a str,
    /// 策略名稱
    pub strategy: &'a str,
}

/// 門前、可以立直的合法動作：自摸、暗槓、每種打法（及其立直版本）。
fn legal_actions(pais: &[Pai], drawn: Pai) -> Vec<Action> {
    let counts = TileCounts::from_tiles(pais.iter().map(|p| p.tile));
    let mut out = Vec::new();
    if is_agari(&counts) {
        out.push(Action::Tsumo);
    }
    for t in mj_core::Tile::all().filter(|&t| counts.get(t) == 4) {
        out.push(Action::Ankan { tile: t });
    }
    let mut seen: Vec<Pai> = Vec::new();
    for &p in pais {
        if seen.contains(&p) {
            continue;
        }
        seen.push(p);
        let tsumogiri = p == drawn && pais.iter().filter(|&&q| q == p).count() == 1;
        let mut after = counts;
        after.remove(p.tile);
        out.push(Action::Discard { pai: p, tsumogiri });
        if !waits(&after).is_empty() {
            out.push(Action::Riichi { pai: p, tsumogiri });
        }
    }
    out
}

/// 執行分析並印出結果。
pub fn run(o: &Options) -> Result<(), String> {
    let pais = parse_pais(o.hand).map_err(|e| e.to_string())?;
    if pais.len() != 14 {
        return Err(format!(
            "手牌需要 14 張（含剛摸到的牌），目前是 {} 張",
            pais.len()
        ));
    }
    let dora = parse_tiles(o.dora).map_err(|e| e.to_string())?;
    let strategy = find_strategy(o.strategy).ok_or_else(|| format!("找不到策略 {}", o.strategy))?;

    let mut sorted = pais.clone();
    sorted.sort();
    let drawn = *pais.last().unwrap();
    let hand = TileCounts::from_tiles(pais.iter().map(|p| p.tile));
    // 東一局、南家、門前、各家 25000 點、牌山剩 60 張的一般情境。
    let view = PlayerView {
        me: 1,
        round_wind: 0,
        kyoku: 0,
        honba: 0,
        riichi_sticks: 0,
        scores: [25_000; 4],
        dora_indicators: dora,
        hand,
        hand_pais: sorted,
        drawn: Some(drawn),
        melds: Default::default(),
        rivers: Default::default(),
        riichi: [false; 4],
        tiles_left_in_wall: 60,
        total_kyoku: 8,
        last_discard: None,
    };
    let mut player = strategy.player(true, None);
    let action = player.act(&view, &legal_actions(&pais, drawn));
    println!("手牌 {hand}（摸 {drawn}）  策略：{}", strategy.name());
    println!();
    for r in player.take_records() {
        print!("{}", render_record(&r));
    }
    println!("→ 最終動作：{}", action_text(&action));
    println!();
    println!("牌效參考（向聽數、進張）：");
    for o in mj_tools::hand::discard_options(&view, &hand) {
        let s = if o.after.shanten == 0 {
            "聽牌".to_string()
        } else {
            format!("{} 向聽", o.after.shanten)
        };
        println!("  打 {}  {} 進張 {}", o.discard, pad(&s, 8), o.after.total);
    }
    Ok(())
}
