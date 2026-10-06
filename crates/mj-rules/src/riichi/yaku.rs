use serde::{Deserialize, Serialize};

/// 役種（yaku）：天鳳規則下的全部役。名稱為日語羅馬拼音，中文對照見各項說明。
/// 役牌依牌分開列出（白、發、中、自風、場風），方便決策紀錄說明是哪一個役牌。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Yaku {
    // ---- 1 番 ----
    /// 立直：門前聽牌時宣告
    Riichi,
    /// 一發：立直後一巡內（無人鳴牌）和牌
    Ippatsu,
    /// 門前清自摸和：門前自摸
    MenzenTsumo,
    /// 平和：門前、四組順子、雀頭非役牌、兩面聽
    Pinfu,
    /// 一盃口：門前、兩組完全相同的順子
    Iipeikou,
    /// 海底摸月：摸牌山最後一張自摸
    Haitei,
    /// 河底撈魚：榮和最後一張打出的牌
    Houtei,
    /// 嶺上開花：開槓後補的嶺上牌自摸
    Rinshan,
    /// 槍槓（搶槓）：榮和他家加槓的那張牌
    Chankan,
    /// 斷么九：全部是 2–8 的數牌
    Tanyao,
    /// 自風牌（門風刻）：自己門風的刻子
    SeatWind,
    /// 場風牌（場風刻）：場風的刻子
    RoundWind,
    /// 役牌 白：白的刻子
    Haku,
    /// 役牌 發：發的刻子
    Hatsu,
    /// 役牌 中：中的刻子
    Chun,
    // ---- 2 番（部分副露後減 1 番）----
    /// 兩立直（ダブリー）：第一巡、無人鳴牌前立直
    DoubleRiichi,
    /// 混全帶么九：每組面子與雀頭都含幺九牌，且有字牌、有順子
    Chanta,
    /// 一氣通貫：同一花色的 123、456、789
    Ittsu,
    /// 三色同順：萬筒索三種花色有相同數字的順子
    SanshokuDoujun,
    /// 三色同刻：萬筒索三種花色有相同數字的刻子
    SanshokuDoukou,
    /// 三槓子：三組槓子
    Sankantsu,
    /// 對對和（碰碰胡）：四組都是刻子
    Toitoi,
    /// 三暗刻：三組暗刻
    Sanankou,
    /// 小三元：兩組三元牌刻子＋三元牌雀頭
    Shousangen,
    /// 混老頭：全部是幺九牌（老頭牌＋字牌）
    Honroutou,
    /// 七對子：七組不同的對子
    Chiitoitsu,
    // ---- 3 番以上 ----
    /// 二盃口：兩組一盃口
    Ryanpeikou,
    /// 純全帶么九：每組面子與雀頭都含老頭牌，沒有字牌
    Junchan,
    /// 混一色：單一花色＋字牌
    Honitsu,
    /// 清一色：只有單一花色
    Chinitsu,
    // ---- 役滿 ----
    /// 國士無雙：13 種幺九牌各一張＋其中一種成對
    Kokushi,
    /// 四暗刻：四組暗刻
    Suuankou,
    /// 大三元：白發中三組刻子
    Daisangen,
    /// 小四喜：三組風牌刻子＋風牌雀頭
    Shousuushii,
    /// 大四喜：四組風牌刻子
    Daisuushii,
    /// 字一色：全部是字牌
    Tsuuiisou,
    /// 清老頭：全部是老頭牌（1、9）
    Chinroutou,
    /// 綠一色：只有 2s 3s 4s 6s 8s 發
    Ryuuiisou,
    /// 九蓮寶燈：門前清一色 1112345678999＋任一張同花色
    Chuuren,
    /// 四槓子：四組槓子
    Suukantsu,
    /// 天和：莊家第一次摸牌就自摸
    Tenhou,
    /// 地和：閒家第一次摸牌（無人鳴牌前）就自摸
    Chiihou,
}

impl Yaku {
    /// 番數。`closed` = 門前；副露後不成立的役回傳 `None`（門前限定役）。役滿回傳 13。
    /// 例：混一色門前 3 番、副露 2 番（「食い下がり」）；平和副露不成立。
    pub fn han(self, closed: bool) -> Option<u8> {
        use Yaku::*;
        let (c, o) = match self {
            Riichi | Ippatsu | MenzenTsumo | Pinfu | Iipeikou => (1, None),
            Haitei | Houtei | Rinshan | Chankan | Tanyao => (1, Some(1)),
            SeatWind | RoundWind | Haku | Hatsu | Chun => (1, Some(1)),
            DoubleRiichi => (2, None),
            Chanta | Ittsu | SanshokuDoujun => (2, Some(1)),
            SanshokuDoukou | Sankantsu | Toitoi | Sanankou | Shousangen | Honroutou => (2, Some(2)),
            Chiitoitsu => (2, None),
            Ryanpeikou => (3, None),
            Junchan | Honitsu => (3, Some(2)),
            Chinitsu => (6, Some(5)),
            Kokushi | Suuankou | Chuuren | Tenhou | Chiihou => (13, None),
            Daisangen | Shousuushii | Daisuushii | Tsuuiisou | Chinroutou | Ryuuiisou
            | Suukantsu => (13, Some(13)),
        };
        if closed { Some(c) } else { o }
    }

    /// 是否為役滿（列舉順序中國士無雙以後都是役滿）。
    pub fn is_yakuman(self) -> bool {
        self >= Yaku::Kokushi
    }

    /// 日文名稱（天鳳與多數計分資料使用的寫法）。
    pub fn name_ja(self) -> &'static str {
        use Yaku::*;
        match self {
            Riichi => "立直",
            Ippatsu => "一発",
            MenzenTsumo => "門前清自摸和",
            Pinfu => "平和",
            Iipeikou => "一盃口",
            Haitei => "海底摸月",
            Houtei => "河底撈魚",
            Rinshan => "嶺上開花",
            Chankan => "槍槓",
            Tanyao => "断幺九",
            SeatWind => "自風牌",
            RoundWind => "場風牌",
            Haku => "役牌 白",
            Hatsu => "役牌 發",
            Chun => "役牌 中",
            DoubleRiichi => "両立直",
            Chanta => "混全帯幺九",
            Ittsu => "一気通貫",
            SanshokuDoujun => "三色同順",
            SanshokuDoukou => "三色同刻",
            Sankantsu => "三槓子",
            Toitoi => "対々和",
            Sanankou => "三暗刻",
            Shousangen => "小三元",
            Honroutou => "混老頭",
            Chiitoitsu => "七対子",
            Ryanpeikou => "二盃口",
            Junchan => "純全帯幺九",
            Honitsu => "混一色",
            Chinitsu => "清一色",
            Kokushi => "国士無双",
            Suuankou => "四暗刻",
            Daisangen => "大三元",
            Shousuushii => "小四喜",
            Daisuushii => "大四喜",
            Tsuuiisou => "字一色",
            Chinroutou => "清老頭",
            Ryuuiisou => "緑一色",
            Chuuren => "九蓮宝燈",
            Suukantsu => "四槓子",
            Tenhou => "天和",
            Chiihou => "地和",
        }
    }
}
