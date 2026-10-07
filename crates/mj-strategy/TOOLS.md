# 策略編輯者工具手冊

策略程式（`src/strategy-list/` 底下的檔案）開頭寫：

```rust
use crate::prelude::*;
```

就能使用下面列出的全部型別與工具。工具都是「只計算、不做決定」的函式，實作在 `crates/mj-tools`；
「該怎麼打」的判斷寫在策略自己的 `decision/` 與 `utils/` 裡。

術語對照（m 萬、p 筒、s 索、z 字；shanten 向聽、ukeire 進張……）見 `docs/05-glossary.md`。

---

## 1. 策略目錄的寫法

```
src/strategy-list/
  tenhou-normal-4p-hanchan/      ← 一個策略（以規則命名）
    main.rs                      ← 登記：什麼情況呼叫哪一個決策
    memory.rs                    ← 對局記憶 `Memory`（整場共用）＋ observe（看事件更新記憶）
    decision/                    ← 每個檔案匯出一個 `decide` 函式
      kyuushu.rs  win.rs  kan.rs  discard.rs  riichi.rs  call.rs
    utils/                       ← 決策共用的輔助判斷
      efficiency.rs  defense.rs
      predict/                   ← 預測函式（會被全知視角評分，見第 5 節）
        tenpai.rs  waits.rs
```

- 新增檔案或子目錄**不需要寫 `mod` 宣告**，建置時會自動產生。檔名或目錄名中的 `-` 在程式中變成 `_`。
- 同一個策略內互相引用：`decision/` 裡的檔案用 `super::super::utils::xxx`、`super::super::memory::Memory`。
- 新增一個策略：複製一份現有的策略目錄改名即可，`mj strategies` 會自動列出。

## 2. 決策函式

框架依人類思考順序呼叫：

- **自己的回合**：九種九牌 → 和牌與否 → 開槓 → 何切 → 立直與否
- **他家打牌時**：和牌與否 → 鳴牌

| 決策 | 函式簽名 | 選項從哪裡來 | 沒登記時 |
|---|---|---|---|
| 九種九牌 | `fn(&Ctx, &mut Memory) -> bool` | true = 流局 | 不流局 |
| 和牌與否 | `fn(&Ctx, &mut Memory) -> bool` | `ctx.win_kind()` 告訴你是自摸／榮和／搶槓 | 和牌 |
| 開槓 | `fn(&Ctx, &mut Memory) -> Option<KanChoice>` | `ctx.kan_options()` | 不槓 |
| 何切 | `fn(&Ctx, &mut Memory) -> Pai` | `ctx.discard_candidates()` | **必須登記** |
| 立直與否 | `fn(&Ctx, &mut Memory) -> bool` | `ctx.riichi_discard()` = 已決定要打的牌 | 立直 |
| 鳴牌 | `fn(&Ctx, &mut Memory) -> CallChoice` | `ctx.call_options()`（含 `CallChoice::Pass`） | 不鳴 |

- 回傳值必須是選項之一；回傳不在選項中的值時，框架改用預設選項並在紀錄中標記 `invalid`。
- 只有一個選項時（例如立直後只能摸切）不會呼叫決策函式。

## 3. `Ctx`：決策情境

| 方法 | 回傳 | 說明 |
|---|---|---|
| `ctx.view` | `&PlayerView` | 看得到的全部資訊：手牌、牌河、副露、點數、寶牌指示牌、立直狀態…… |
| `ctx.kind` | `DecisionKind` | 這次是哪一種決策 |
| `ctx.me()` | `Seat` | 自己的座位 |
| `ctx.hand()` | `TileCounts` | 門前手牌 |
| `ctx.shanten()` | `i8` | 目前向聽數（摸牌後取打出最好一張後的值） |
| `ctx.after_discard(pai)` | `AfterEval` | 打出這張後的向聽數與進張 |
| `ctx.after_call(choice)` | `AfterEval` | 這樣鳴牌（再打最好的一張）後的手牌；`Pass` = 不鳴 |
| `ctx.after(&action)` | `AfterEval` | 任何動作之後的手牌 |
| `ctx.remaining()` | `&[f32; 34]` | 每種牌還剩幾張（4 − 看得到的張數） |
| `ctx.standing()` | `Standing` | 名次、與上下名的點差、剩幾局、是否最終局 |
| `ctx.dora_value(tile)` | `u8` | 這種牌值幾張寶牌 |
| `ctx.offered()` | `Option<(Seat, Pai)>` | 鳴牌／榮和時，他家打出的牌 |
| `ctx.note("…")` | | **寫下判斷理由**，復盤時顯示 |
| `ctx.claim(來源, Claim)` | | 記錄一個預測（第 5 節） |

`AfterEval` 欄位：`shanten`（向聽數，聽牌 0、和牌 −1）、`ukeire`（進張張數；聽牌時 = 聽牌張剩餘數）、`win`。

## 4. 工具箱 `tools::*`

### `tools::hand` 手牌
| 函式 | 說明 |
|---|---|
| `shanten(&TileCounts) -> i8` | 向聽數（一般型、七對子、國士取最小） |
| `shanten_standard` / `shanten_chiitoi` / `shanten_kokushi` | 各型的向聽數 |
| `ukeire(view, &hand) -> Ukeire` | 進張（`.tiles` 每種牌與剩餘張數、`.total` 總數） |
| `discard_options(view, &hand) -> Vec<DiscardOption>` | 何切排序：每種打法的向聽與進張，最好的在前 |
| `waits_with_remaining(view, &hand) -> Vec<(Tile, f32)>` | 聽牌張與各自剩幾張 |
| `is_tenpai(&hand) -> bool` | 是否聽牌（3k+1 張） |
| `my_shanten(view) -> i8` | 自己目前的向聽數 |
| `is_agari` / `decompose` / `is_chiitoi` / `is_kokushi` | 和牌判定與拆解 |

### `tools::after` 動作後的手牌
| 函式 | 說明 |
|---|---|
| `after(view, &action) -> AfterEval` | 做了某個動作後的向聽與進張 |
| `evaluate_after(view, &remaining, &action)` | 同上，自己提供剩餘牌估計（例如讀牌後修正過的） |

### `tools::visible` 看得到的牌
| 函式 | 說明 |
|---|---|
| `visible_counts(view) -> TileCounts` | 手牌＋牌河＋副露＋寶牌指示牌 |
| `visible_count(view, tile) -> u8` | 某種牌看得到幾張 |
| `remaining(view) -> [f32; 34]` | 每種牌還剩幾張 |

### `tools::safety` 安全度的「事實」
| 函式 | 說明 |
|---|---|
| `genbutsu(view, seat) -> TileCounts` | 對某家的現物（他打過的、他立直後別人打過的、同巡內別人打過的） |
| `is_genbutsu(view, seat, tile)` | 是否為現物 |
| `is_suji(view, seat, tile)` | 是否為筋牌（1–3 看 +3、7–9 看 −3、4–6 兩邊都要） |
| `is_no_chance(view, tile)` | No Chance：兩面聽這張所需的牌已 4 張可見 |
| `honor_visible(view, tile) -> u8` | 字牌看得到幾張 |

### `tools::yaku_path` 役的路線
| 函式 | 說明 |
|---|---|
| `yaku_after_call(view, &action) -> Option<OpenYakuPath>` | 這個鳴牌之後有沒有役（`None` 表示不是鳴牌動作） |
| `open_yaku_path(view, &melds, &concealed) -> OpenYakuPath` | 指定副露與門前牌時的役路線 |
| `OpenYakuPath` | `None`（無役）、`Yakuhai`（役牌）、`Tanyao`（斷么九）、`Flush`（染手） |

### `tools::value` 點數
| 函式 | 說明 |
|---|---|
| `win_value(view, &hand, win_tile, tsumo, assume_riichi) -> Option<Score>` | 目前手牌以某張和牌值多少（`.han` 番、`.fu` 符、`.yaku` 役） |
| `ron_points(view, &score) -> i32` | 榮和時對方付的點數 |
| `score(&concealed, &melds, &WinContext, &RiichiConfig)` | 完整的計分函式（任意條件） |

### `tools::dora` 寶牌
| 函式 | 說明 |
|---|---|
| `dora_tiles(view) -> Vec<Tile>` | 目前的寶牌 |
| `dora_value(view, tile) -> u8` | 某種牌值幾張寶牌（不含赤五） |
| `pai_dora_value(view, pai) -> u8` | 某張牌值幾張寶牌（含赤五） |
| `my_dora_count(view) -> u8` | 自己手上共有幾張寶牌 |

### `tools::table` 牌桌
| 函式 | 說明 |
|---|---|
| `opponents(view) -> [Seat; 3]` | 其他三家（下家、對家、上家） |
| `relative(view, seat) -> u8` | 1 = 下家、2 = 對家、3 = 上家 |
| `riichi_opponents(view) -> Vec<Seat>` | 已立直的對手 |
| `riichi_seq(view, seat)` | 某家立直宣言牌的打牌順序 |
| `my_turn(view) -> usize` | 自己打過幾張（巡目） |
| `my_winds(view) -> (自風, 場風)` / `is_yakuhai_for_me(view, tile)` | 風位與役牌 |
| `i_am_closed(view)` | 自己是否門前清 |
| `standing(view) -> Standing` | 名次與點差 |

## 5. 預測函式（`utils/predict/`）與評分

預測是「對看不到的事實做機率判斷」，例如「某家聽牌了嗎」「打這張會不會放銃」「跟立直者對攻我能贏嗎」。
它和大局觀決策不同：在全知視角下，預測對不對可以立刻判定。所以預測用**準確度**評分，與輸贏無關。

寫法：計算出機率後呼叫 `ctx.claim`，再回傳機率給決策使用。

```rust
pub fn predict(ctx: &Ctx, seat: Seat) -> f64 {
    let p = /* 你的判斷 */;
    ctx.claim("predict::tenpai", Claim::Tenpai { seat, p });
    p
}
```

| `Claim` | 預測內容 | 真相（全知視角） |
|---|---|---|
| `Tenpai { seat, p }` | 某家此刻已聽牌 | 他的手牌向聽數為 0 |
| `Waits { seat, tile, p }` | 打出這張會被某家榮和（不考慮役） | 這張在他的聽牌張中，且他沒有振聽 |
| `IWinThisHand { p }` | 本局最後由我和牌 | 本局結束時我有沒有和牌 |

需要新的預測種類時，在 `src/framework/claim.rs` 加上一種 `Claim`，並在 `mj-sim/src/predict_eval.rs` 的 `judge` 寫出真相怎麼算。

評估指令：

```sh
mj predict-eval --strategy tenhou-normal-4p-hanchan --games 300
```

輸出各預測函式的 Brier 分數、技巧分數（相對於永遠猜平均值）、對數損失與校準表。

## 6. 決策的評分（反事實模擬）

大局觀決策（何切、鳴牌、立直、押引）沒有立即的真相，所以用反事實模擬評分：
在決策點改選其他選項重打這一局多次，比較各選項的期望點數，算出實際選擇的「損失」。

```sh
mj evaluate --strategy tenhou-normal-4p-hanchan --games 20 --samples 20 --rollouts 24
```

輸出各決策種類的「選到最佳的比例」「平均損失」「顯著損失比例」，並列出損失最大的決策
（附上當時 `ctx.note` 寫下的理由與預測，方便找出思路哪裡錯了）。

## 7. 對局記憶 `Memory`

`memory.rs` 定義 `pub struct Memory`（需要 `Default`），整場對局（半莊）中所有決策共用，
每場對局開始時重新建立。`observe(me, &Event, &mut Memory)` 對每個事件呼叫，用來記下觀察到的事，例如：

- 某家立直了幾次（愛立直的人）
- 某家用筋牌騙和過牌（他的筋牌不可信）

決策函式收到 `&mut Memory`，可以讀也可以寫。
