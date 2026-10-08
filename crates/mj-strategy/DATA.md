# 策略編輯者資料結構說明

決策函式 `decide(ctx, mem)`、常駐預測與 `observe(me, event, mem)` 能接觸到的所有資料結構。
工具函式的清單見 [TOOLS.md](TOOLS.md)；術語對照見 `docs/05-glossary.md`。

> 修改 `PlayerView`、`Event`、`Ctx` 等結構時，請同步更新本檔。

目錄：
1. [Ctx：決策情境](#1-ctx決策情境)
2. [PlayerView：看得到的牌局資訊](#2-playerview看得到的牌局資訊)
3. [牌河與副露](#3-牌河與副露)
4. [牌的型別](#4-牌的型別)
5. [決策的選項與結果](#5-決策的選項與結果)
6. [計算結果](#6-計算結果)
7. [Event：事件（observe 收到的）](#7-event事件observe-收到的)
8. [Claim：預測](#8-claim預測)
9. [計分](#9-計分)
10. [讀取範例](#10-讀取範例)

座位編號：`Seat`（`u8`）= 0–3，0 是起家（東一局的莊家），依下家方向遞增。
相對位置：(seat − 自己) mod 4 = 1 下家、2 對家、3 上家（`tools::table::relative`）。

---

## 1. Ctx：決策情境

每次呼叫決策函式（或常駐預測）時建立，同一次決策內的查詢結果會快取。

### 欄位

| 欄位 | 型別 | 說明 |
|---|---|---|
| `view` | `&PlayerView` | 看得到的全部資訊（第 2 節） |
| `kind` | `DecisionKind` | 這次是哪一種決策（常駐預測時為 `Predict`） |

### 這次決策的選項

| 方法 | 回傳 | 何時有值 |
|---|---|---|
| `discard_candidates()` | `&[Pai]` | 何切：可以打的牌（已排除食替禁止的牌；同種牌只列一次，赤五與普通 5 分開） |
| `call_options()` | `&[CallChoice]` | 鳴牌：可選的鳴法，一定含 `CallChoice::Pass` |
| `kan_options()` | `&[KanChoice]` | 開槓：可槓的選項 |
| `win_kind()` | `Option<WinKind>` | 和牌決策：自摸／榮和／搶槓 |
| `riichi_discard()` | `Option<Pai>` | 立直決策：已經決定要打的那張 |
| `offered()` | `Option<(Seat, Pai)>` | 鳴牌、榮和決策：他家打出（或加槓）的牌與座位 |

不屬於這次決策的選項是空的（例如何切時 `call_options()` 為空）。

### 查詢

| 方法 | 回傳 | 說明 |
|---|---|---|
| `me()` | `Seat` | 自己的座位 |
| `hand()` | `TileCounts` | 門前手牌（含剛摸到的牌） |
| `shanten()` | `i8` | 目前向聽數；摸牌後（3k+2 張）取打出最好一張後的值 |
| `after(&action)` | `AfterEval` | 做了某個動作之後的手牌 |
| `after_discard(pai)` | `AfterEval` | 打出這張之後 |
| `after_call(choice)` | `AfterEval` | 這樣鳴牌並打出最好的一張之後；`Pass` = 不鳴（手牌不變） |
| `remaining()` | `&[f32; 34]` | 每種牌還剩幾張 = 4 − 自己看得到的張數（索引 = `Tile::index()`） |
| `standing()` | `Standing` | 名次與點差（第 6 節） |
| `dora_value(tile)` | `u8` | 這種牌值幾張寶牌（不含赤五） |

### 紀錄

| 方法 | 說明 |
|---|---|
| `note(text)` | 寫下判斷理由，復盤與評估報告會顯示 |
| `claim(source, Claim)` | 記錄一個預測（第 8 節）；`source` 是預測函式名稱，例如 `"predict::tenpai"` |
| `notes()` | 目前寫下的理由（測試用） |

測試時可以直接建立：`Ctx::new(&view, DecisionKind::Discard).with_discards(vec![...])`，
另有 `with_calls`、`with_win`、`with_riichi_discard`。

---

## 2. PlayerView：看得到的牌局資訊

`ctx.view` 是自己在這個時間點看得到的全部資訊。別人的手牌、牌山的順序不在其中。

| 欄位 | 型別 | 說明 |
|---|---|---|
| `me` | `Seat` | 自己的座位 |
| `round_wind` | `u8` | 場風：0 = 東場、1 = 南場、2 = 西場（西入） |
| `kyoku` | `u8` | 該場的第幾局（0–3）；莊家座位 = `kyoku` |
| `honba` | `u8` | 本場數 |
| `riichi_sticks` | `u8` | 場上供託（立直棒）數 |
| `scores` | `[i32; 4]` | 四家目前點數（依座位） |
| `dora_indicators` | `Vec<Tile>` | 已翻開的寶牌指示牌（開局 1 張，每槓 +1）；寶牌是它的下一張，用 `tools::dora` |
| `hand` | `TileCounts` | 自己的門前手牌（含剛摸到的牌） |
| `hand_pais` | `Vec<Pai>` | 同一手牌，保留赤五資訊，已排序 |
| `drawn` | `Option<Pai>` | 自己的回合剛摸到的牌（吃碰後打牌、回應他家時為 `None`） |
| `melds` | `[Vec<Meld>; 4]` | 四家的副露（第 3 節） |
| `rivers` | `[Vec<DiscardRecord>; 4]` | 四家的牌河，依打出順序（第 3 節） |
| `riichi` | `[bool; 4]` | 四家是否已立直（宣言牌被榮和時不算） |
| `tiles_left_in_wall` | `u8` | 牌山剩餘可摸張數（不含王牌）；0 = 已經是最後一張（海底／河底） |
| `total_kyoku` | `u8` | 正規局數（半莊 8）；判斷還剩幾局用 |
| `last_discard` | `Option<(Seat, Pai)>` | 正在回應的那張牌（他家打出或加槓）與座位 |

方法：

| 方法 | 回傳 | 說明 |
|---|---|---|
| `dealer()` | `Seat` | 莊家座位 |
| `seat_wind(seat)` | `u8` | 某座位的門風：0 = 東（莊家）… 3 = 北 |
| `current_kyoku_index()` | `u8` | 從東一局起算的局序（東一 = 0、南一 = 4） |

---

## 3. 牌河與副露

### `DiscardRecord`（`view.rivers[seat][i]`）

| 欄位 | 型別 | 說明 |
|---|---|---|
| `pai` | `Pai` | 打出的牌 |
| `tsumogiri` | `bool` | true = 摸切（打出剛摸到的牌）；false = 手切。吃碰後打出的牌一定是手切 |
| `riichi_declared` | `bool` | 是否為立直宣言牌（宣言牌被榮和時，`view.riichi` 仍是 false） |
| `called` | `bool` | 是否被別人吃碰槓拿走（此時這張牌改出現在別人的副露中） |
| `seq` | `u16` | 本局所有人合計的打牌順序（從 0 開始），用來判斷先後，例如「某家立直之後打出的牌」 |

`rivers[seat].len()` = 該家打過幾張，約等於巡目。

### `Meld`（`view.melds[seat][i]`）

| 欄位 | 型別 | 說明 |
|---|---|---|
| `kind` | `MeldKind` | `Chi` 吃、`Pon` 碰、`Daiminkan` 大明槓、`Ankan` 暗槓、`Kakan` 加槓 |
| `tiles` | `Vec<Pai>` | 這組的全部牌（3 或 4 張），含鳴進來的那張 |
| `called` | `Option<Pai>` | 從他家取得的那張（暗槓為 `None`） |
| `from` | `Option<u8>` | 被鳴牌者的座位（暗槓為 `None`） |

方法：`base()` 最小的牌種（順子起點）、`is_kan()`、`is_open()`（暗槓以外都是 true，即破壞門前）、`is_sequence()`。

---

## 4. 牌的型別

### `Tile`：牌種（34 種）

編號 0–33：`1m`–`9m` = 0–8、`1p`–`9p` = 9–17、`1s`–`9s` = 18–26、`1z`–`7z`（東南西北白發中）= 27–33。

| 方法 | 說明 |
|---|---|
| `index()` | 編號 0–33（陣列索引） |
| `suit()` | `Suit::Man`／`Pin`／`Sou`／`Honor` |
| `number()` | 數牌 1–9；字牌 1–7（東南西北白發中） |
| `is_honor()` / `is_terminal()` / `is_yaochu()` | 字牌／老頭牌（1、9）／幺九牌 |
| `is_wind()` / `is_dragon()` | 風牌／三元牌 |
| `Tile::all()` | 依序列出 34 種 |
| `Tile::wind(w)` | 風位編號 → 風牌 |
| `"5m".parse::<Tile>()` | 由記法建立 |

### `Pai`：實體牌

`{ tile: Tile, red: bool }`，`red` = 赤五。記法 `0m`／`0p`／`0s` 為赤五。赤五本身算一張寶牌。
比較時赤五與普通 5 不相等；只看牌種時用 `.tile`。

### `TileCounts`：牌數表

`[u8; 34]`，每種牌幾張。方法：`get(tile)`、`add(tile)`、`remove(tile)`、`total()`、`tiles()`（逐張列出）、
`TileCounts::from_tiles(...)`；輸出成記法 `to_string()`，例如 `123m55z`。

---

## 5. 決策的選項與結果

| 型別 | 值 |
|---|---|
| `DecisionKind` | `Kyuushu` 九種九牌、`Win` 和牌與否、`Kan` 開槓、`Discard` 何切、`Riichi` 立直與否、`Call` 鳴牌、`Predict` 常駐預測（不是決策） |
| `WinKind` | `Tsumo` 自摸、`Ron` 榮和、`Chankan` 搶槓 |
| `KanChoice` | `Ankan(Tile)` 暗槓、`Kakan(Pai)` 加槓 |
| `CallChoice` | `Pass` 不鳴、`Chi([Pai; 2])` 用手中這兩張吃、`Pon([Pai; 2])` 用手中這兩張碰、`Daiminkan` 明槓 |

---

## 6. 計算結果

### `AfterEval`（`ctx.after*()`）

| 欄位 | 型別 | 說明 |
|---|---|---|
| `win` | `bool` | 這個動作就是和牌 |
| `shanten` | `i8` | 動作後的向聽數：−1 = 和牌、0 = 聽牌、1 = 一向聽… |
| `ukeire` | `f32` | 動作後的進張張數（扣掉看得到的牌）；聽牌時 = 聽牌張剩餘數 |

### `Standing`（`ctx.standing()`）

| 欄位 | 型別 | 說明 |
|---|---|---|
| `rank` | `u8` | 目前名次，0 = 第一名（同分時離起家近者在前） |
| `gap_above` | `i32` | 與上一名的點差（第一名為 0） |
| `gap_below` | `i32` | 領先下一名的點差（最後一名為 0） |
| `is_dealer` | `bool` | 自己是否為莊家 |
| `hands_remaining` | `u8` | 本局之後還剩幾局（不計連莊） |
| `is_all_last` | `bool` | 是否為最終局（南四局） |

### `OpenYakuPath`（`tools::yaku_path`）

`None` 無役、`Yakuhai` 役牌、`Tanyao` 斷么九、`Flush` 染手（混一色／清一色）。

---

## 7. Event：事件（observe 收到的）

`observe(me, &event, &mut mem)` 對每個事件都會呼叫一次，`me` 是自己的座位。
收到的是**遮蔽過的版本**：別人的配牌是空的、別人摸到的牌是 `None`。

| 事件 | 欄位 | 說明 |
|---|---|---|
| `StartGame` | `seed` | 對局開始 |
| `StartKyoku` | `round_wind`、`kyoku`、`honba`、`riichi_sticks`、`scores`、`dora_indicator`、`hands` | 一局開始；`hands` 只有自己那一格有牌 |
| `Draw` | `seat`、`pai: Option<Pai>`、`rinshan` | 摸牌；別人摸的牌為 `None`；`rinshan` = 嶺上牌 |
| `Discard` | `seat`、`pai`、`tsumogiri`、`riichi` | 打牌；`tsumogiri` 摸切、`riichi` 立直宣言牌 |
| `Call` | `seat`、`meld` | 吃、碰、槓（含暗槓、加槓） |
| `RiichiAccepted` | `seat` | 立直成立（宣言牌沒被榮和、已付 1000 點） |
| `Dora` | `indicator` | 翻開新的寶牌指示牌（槓寶牌） |
| `HandEnd` | `end: HandEnd`、`deltas: [i32; 4]` | 一局的結算（雙響時有兩筆）；`deltas` = 這筆結算的點數增減 |
| `EndKyoku` | `scores` | 一局結束後的四家點數 |
| `EndGame` | `scores` | 對局結束 |

### `HandEnd`

- `Win { winner, from, hand, win_tile, score, ura_indicators }`：和牌。`from == winner` 為自摸；`hand` 是和牌者的門前手牌（含和牌張，和牌時會公開）；`score` 見第 9 節。
- `Draw { reason, tenpai: [bool; 4], nagashi }`：流局。`reason`：`Exhaustive` 荒牌、`KyuushuKyuuhai`、`SuufonRenda` 四風連打、`SuuchaRiichi` 四家立直、`Suukaikan` 四槓散了、`Sanchahou` 三家和；`nagashi` = 流局滿貫的座位。

---

## 8. Claim：預測

`ctx.claim(來源, Claim)` 記錄的預測，`p` = 預測「是」的機率（0–1）。
框架會自動附上被預測者當時的情境（`TargetSituation`：巡目、副露組數、是否立直），`mj predict-eval` 依此分組。

| `Claim` | 預測內容 | 真相（全知視角，由 `mj predict-eval` 判定） |
|---|---|---|
| `Tenpai { seat, p }` | 某家此刻已聽牌 | 他的手牌向聽數為 0 |
| `Waits { seat, tile, p }` | 打出這張會被某家榮和（不考慮役） | 這張在他的聽牌張中，且他沒有振聽 |
| `IWinThisHand { p }` | 本局最後由我和牌 | 本局結束時我有沒有和牌 |
| `BigHand { seat, p }` | 某家如果本局和牌，是滿貫以上 | 他本局和牌時基本點 ≥ 2000；他沒和牌時不列入評分 |

---

## 9. 計分

### `Score`（`tools::value::win_value` 等的結果）

| 欄位／方法 | 說明 |
|---|---|
| `yaku: Vec<(Yaku, u8)>` | 成立的役與番數（役滿每個記 13） |
| `dora`、`ura`、`aka` | 寶牌、裏寶牌、赤寶牌張數 |
| `han` | 總番數（含寶牌；役滿時為 0） |
| `fu` | 符數 |
| `limit` | `None`、`Mangan` 滿貫、`Haneman` 跳滿、`Baiman` 倍滿、`Sanbaiman` 三倍滿、`Yakuman(n)` n 倍役滿 |
| `base` | 基本點 |
| `ron_points(dealer, honba)` | 榮和時放銃者付的點數 |
| `tsumo_points(dealer, honba)` | 自摸時 (莊家付, 每個閒家付) |

`Yaku` 的各項與中文名見 `docs/05-glossary.md` 的「役種」。

---

## 10. 讀取範例

```rust
// 座位 2 最後 3 張是否都是摸切（完整紀錄，直接讀 view）
let river = &ctx.view.rivers[2];
let last3_tsumogiri = river.len() >= 3 && river.iter().rev().take(3).all(|d| d.tsumogiri);

// 座位 2 立直之後，其他人打出的牌（依打牌順序 seq 判斷）
if let Some(rs) = tools::table::riichi_seq(ctx.view, 2) {
    let after: Vec<Tile> = ctx.view.rivers.iter().enumerate()
        .filter(|(s, _)| *s != 2)
        .flat_map(|(_, r)| r.iter().filter(|d| d.seq > rs).map(|d| d.pai.tile))
        .collect();
}

// 座位 1 有幾組副露、是否有役牌刻子
let melds = &ctx.view.melds[1];
let open = melds.iter().filter(|m| m.is_open()).count();
let dragon_pon = melds.iter().any(|m| !m.is_sequence() && m.base().is_dragon());

// 我是否為最終局的最後一名
let st = ctx.standing();
let must_win = st.is_all_last && st.rank == 3;
```

### 完整紀錄 vs 人類的記憶

`ctx.view` 有完整的紀錄（每張牌的摸切、順序）。若要模擬人類有限的注意力，
就在 `memory.rs` 的 `observe` 中只記人類會記的事，預測函式改讀 `mem`。
例如 `tsumogiri_streak`：從中巡起各家的連續摸切次數，手切或鳴牌歸 0（見 `utils/predict/tenpai.rs`）。
