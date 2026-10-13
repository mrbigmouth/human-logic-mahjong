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
  tenhou-normal-4p-hanchan/      ← 規則（天鳳一般規則・四人半莊）
    README.md                    ← 規則說明與這個規則下的策略一覽
    push-fold/                   ← 一個策略（名稱表示特色；特色太複雜時也可以用編號代號）
      README.md                  ← 策略說明（GUI 選策略時顯示，見下方格式）
      main.rs                    ← 登記：什麼情況呼叫哪一個決策
      memory.rs                  ← 對局記憶 `Memory`（整場共用）＋ observe（看事件更新記憶）
      decision/                  ← 每個檔案匯出一個 `decide` 函式
        kyuushu.rs  win.rs  kan.rs  discard.rs  riichi.rs  call.rs
      utils/                     ← 決策共用的輔助判斷
        efficiency.rs  defense.rs  value.rs
        predict/                 ← 預測函式（會被全知視角評分，見第 5 節）
          tenpai.rs  waits.rs  win.rs
    efficiency-fold/             ← 同一規則下的另一個策略
      ...
```

- 第一層目錄是**規則**，第二層是同一套規則下的各個**策略**。策略的完整名稱是「規則／策略」，
  例如 `tenhou-normal-4p-hanchan/push-fold`，`mj` 的各個指令都用這個名稱。
- 每個策略都**必須有 `README.md`**（建置時檢查），格式：
  1. 第一行 `# 標題`（短名稱）；
  2. 接著一段摘要（一段話說明主要特色），`mj strategies` 與 GUI 的策略選單顯示這一段；
  3. 之後自由撰寫：特色、強度、已知弱點、主要參數……（`mj strategies <名稱>` 顯示全文）。
  修改策略的打法時，記得同步更新 README。
- 新增檔案或子目錄**不需要寫 `mod` 宣告**，建置時會自動產生。檔名或目錄名中的 `-` 在程式中變成 `_`
  （例如 `strategy_list::tenhou_normal_4p_hanchan::push_fold::decision::discard`）。
- 同一個策略內互相引用：`decision/` 裡的檔案用 `super::super::utils::xxx`、`super::super::memory::Memory`。
- 新增一個策略：在規則目錄下複製一份現有的策略目錄、改名並改寫 README，`mj strategies` 會自動列出。
  修改策略前也可以先複製一份舊版本當作比較基準（例如 `efficiency-fold` 是 M4 時凍結的版本），
  再用 `mj simulate 新版 舊版` 確認改動是否真的變強。

## 2. 決策函式

框架依人類思考順序呼叫：

- **自己的回合**：九種九牌 → 和牌與否 → 開槓 → 何切 → 立直與否 → 手切摸切
- **他家打牌時**：和牌與否 → 鳴牌

| 決策 | 函式簽名 | 選項從哪裡來 | 沒登記時 |
|---|---|---|---|
| 九種九牌 | `fn(&Ctx, &mut Memory) -> bool` | true = 流局 | 不流局 |
| 和牌與否 | `fn(&Ctx, &mut Memory) -> bool` | `ctx.win_kind()` 告訴你是自摸／榮和／搶槓 | 和牌 |
| 開槓 | `fn(&Ctx, &mut Memory) -> Option<KanChoice>` | `ctx.kan_options()` | 不槓 |
| 何切 | `fn(&Ctx, &mut Memory) -> Pai` | `ctx.discard_candidates()` | **必須登記** |
| 立直與否 | `fn(&Ctx, &mut Memory) -> bool` | `ctx.riichi_discard()` = 已決定要打的牌 | 立直 |
| 鳴牌 | `fn(&Ctx, &mut Memory) -> CallChoice` | `ctx.call_options()`（含 `CallChoice::Pass`） | 不鳴 |
| 手切摸切 | `fn(&Ctx, &mut Memory) -> bool` | 打的牌和剛摸到的牌同種時才問；`ctx.tedashi_pai()`、`ctx.declaring_riichi()`；true = 從手中打出（手切） | 摸切 |

- 回傳值必須是選項之一；回傳不在選項中的值時，框架改用預設選項並在紀錄中標記 `invalid`。
- 只有一個選項時（例如立直後只能摸切）不會呼叫決策函式。

另外三種登記（不是決策）：

| 登記 | 函式簽名 | 何時呼叫 |
|---|---|---|
| `book.observe(f)` | `fn(Seat, &Event, &mut Memory)` | 每個事件；用來更新對局記憶 |
| `book.think_each_turn(f)` | `fn(&Ctx, &mut Memory)` | 每次輪到自己、在常駐預測與所有決策之前，**一定會執行**；把當下的判斷記進記憶（例如「我現在估計各家聽牌的機率」，局末公開時再與真相對照）。寫的理由與預測不會被記錄 |
| `book.predict_each_turn(f)` | `fn(&Ctx, &mut Memory)` | 每次輪到自己、在所有決策之前；只做預測（`ctx.claim`），不影響動作。用於「決策不一定用到、但希望持續被評估」的預測，只在評估（有記錄）時執行 |

## 3. `Ctx` 與所有資料結構

`Ctx` 的欄位與方法、`PlayerView` 的每個欄位、牌河（含摸切 `tsumogiri`、打牌順序 `seq`）、副露、
`Event`（observe 收到的事件）、`Claim`、`Score` 等，完整說明見 **[DATA.md](DATA.md)**。

最常用的幾個：`ctx.discard_candidates()`、`ctx.call_options()`、`ctx.after_discard(pai)`（打出後的向聽與進張）、
`ctx.shanten()`、`ctx.standing()`、`ctx.note("理由")`、`ctx.claim(來源, 預測)`。

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
| `suji(view, seat, tile) -> Suji` | 筋的種類：`None` 無筋、`Half` 片筋（4–6 只有一邊是現物）、`Full` 筋／両筋 |
| `is_no_chance(view, tile)` | No Chance：兩面聽這張所需的牌已 4 張可見 |
| `ryanmen_chances(view, tile) -> u8` | 兩面聽這張的搭子還有幾種可能存在（0–2；0 = No Chance） |
| `is_one_chance(view, tile)` | One Chance：兩面搭子還可能存在，但每一種都含有已見 3 張的牌 |
| `riichi_declaration_tile(view, seat) -> Option<Tile>` | 某家的立直宣言牌 |
| `honor_visible(view, tile) -> u8` | 字牌看得到幾張 |

### `tools::river` 牌河與副露的事實
| 函式 | 說明 |
|---|---|
| `turn(view, seat) -> usize` | 某家打過幾張（巡目） |
| `trailing_tsumogiri(view, seat) -> usize` | 牌河最後連續摸切的張數（剛手切為 0） |
| `recent_tedashi(view, seat, k) -> usize` | 最近 `k` 張中手切的張數 |
| `last_tedashi(view, seat) -> Option<(usize, Pai)>` | 最後一次手切（第幾張、哪張牌） |
| `middle_tedashi_since(view, seat, from) -> usize` | 第 `from` 張以後手切的中張牌（3–7）張數 |
| `is_middle(tile)` | 是否為中張牌（數牌 3–7） |
| `suit_counts(view, seat) -> [u8; 4]` / `suit_index(suit)` | 各花色打出的張數 `[萬, 筒, 索, 字]` |
| `open_melds(view, seat) -> usize` | 副露（不含暗槓）組數 |
| `meld_dora(view, seat) -> u8` | 副露中看得到的寶牌張數（含赤五） |
| `has_yakuhai_meld(view, seat)` | 副露中有沒有對他而言是役牌的刻子 |
| `meld_suits(view, seat) -> u8` | 副露用到的數牌花色（位元：萬 1、筒 2、索 4） |
| `view_after_discard(view, pai, tsumogiri, riichi) -> PlayerView` | 自己打出這張（手切／摸切、是否立直）之後的牌桌；搭配 `ctx.imagine(&v)` 看「別人眼中的我」 |

### `tools::yaku_path` 役的路線
| 函式 | 說明 |
|---|---|
| `yaku_after_call(view, &action) -> Option<OpenYakuPath>` | 這個鳴牌之後有沒有役（`None` 表示不是鳴牌動作） |
| `open_yaku_path(view, &melds, &concealed) -> OpenYakuPath` | 指定副露與門前牌時的役路線 |
| `OpenYakuPath` | `None`（無役）、`Yakuhai`（役牌）、`Tanyao`（斷么九）、`Flush`（染手） |

### `tools::routes` 和牌路線
| 函式 | 說明 |
|---|---|
| `win_routes(view, &hand, limit) -> Vec<WinRoute>` | 離和牌最近的完成形（向聽數 ≤ 2）：每一步摸一張讓向聽前進的牌、打一張維持向聽的牌。手牌是摸牌後的 3k+2 張時，第一張要打的牌在 `discards[0]`。依機會排序，最多 `limit` 條；點數最高的路線不在其中時附在最後 |
| `WinRoute` | `discards`（要打的牌）、`draws`（要摸的牌，排序）、`hand`（完成時的門前手牌）、`win_tile`（點數最高的和牌張）、`score`（榮和的計分；無役 `None`）、`points`、`assumes_riichi`（門前時假設立直）、`chance`（要摸的牌的剩餘張數組合數相乘，只用來比較路線） |

### `tools::value` 點數
| 函式 | 說明 |
|---|---|
| `win_value(view, &hand, win_tile, tsumo, assume_riichi) -> Option<Score>` | 目前手牌以某張和牌值多少（`.han` 番、`.fu` 符、`.yaku` 役） |
| `ron_points(view, &score) -> i32` | 榮和時對方付的點數 |
| `tenpai_value(view, &hand, assume_riichi) -> Option<TenpaiValue>` | 聽牌手牌（3k+1 張）的聽牌張數 `.waits`、有役能榮和的張數 `.ron_waits`、依剩餘張數加權的平均榮和點數 `.avg_ron` |
| `points_for_han(han, dealer) -> i32` | 依番數粗估榮和點數（30 符；滿貫以上照規定），還沒聽牌時估價用 |
| `score(&concealed, &melds, &WinContext, &RiichiConfig)` | 完整的計分函式（任意條件） |

### `tools::dora` 寶牌
| 函式 | 說明 |
|---|---|
| `dora_tiles(view) -> Vec<Tile>` | 目前的寶牌 |
| `dora_value(view, tile) -> u8` | 某種牌值幾張寶牌（不含赤五） |
| `pai_dora_value(view, pai) -> u8` | 某張牌值幾張寶牌（含赤五） |
| `my_dora_count(view) -> u8` | 自己手上共有幾張寶牌 |
| `unseen_dora(view) -> u8` | 自己看不到的寶牌張數（含赤五；可能在別人手上或牌山中） |

### `tools::table` 牌桌
| 函式 | 說明 |
|---|---|
| `opponents(view) -> [Seat; 3]` | 其他三家（下家、對家、上家） |
| `relative(view, seat) -> u8` | 1 = 下家、2 = 對家、3 = 上家 |
| `riichi_opponents(view) -> Vec<Seat>` | 已立直的對手 |
| `riichi_seq(view, seat)` | 某家立直宣言牌的打牌順序 |
| `my_turn(view) -> usize` | 自己打過幾張（巡目） |
| `my_winds(view) -> (自風, 場風)` / `is_yakuhai_for_me(view, tile)` | 風位與役牌 |
| `is_yakuhai_for(view, seat, tile)` | 某種牌對某家是不是役牌 |
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
| `BigHand { seat, p }` | 某家如果本局和牌，是滿貫以上 | 他本局和牌時基本點 ≥ 2000；他沒和牌時不列入評分 |

需要新的預測種類時，在 `src/framework/claim.rs` 加上一種 `Claim`，並在 `mj-sim/src/predict_eval.rs` 的 `judge` 寫出真相怎麼算。

評估指令：

```sh
mj predict-eval --strategy tenhou-normal-4p-hanchan/hand-reading --games 300
```

輸出各預測函式的 Brier 分數、技巧分數（相對於永遠猜平均值）、對數損失與校準表。

用真人牌譜校準預測時（`hand-reading` 的讀牌就是這樣做的）：

1. `mj read-stats data/tenhou --out target/read-stats` 匯出兩個 CSV：
   - `tenpai.csv`：每個打牌時點 × 每家對手的特徵（巡目、副露、手切摸切、副露的寶牌……）與真相（聽牌、本局是否和牌與打點）；
   - `waits.csv`：已聽牌的對手 × 自己手上每種牌的安全度特徵（筋、壁、寶牌……）與真相（是否為聽牌張、振聽）。
2. 用任何工具（例如 Python）依特徵分組看實際發生率，或做邏輯迴歸得到各線索的勝算倍數，寫成具名常數。
3. 擬合與驗證用不同日期的牌譜，再用 `mj predict-eval --logs <驗證用牌譜>` 確認技巧分數與校準表。

## 6. 決策的評分（反事實模擬）

大局觀決策（何切、鳴牌、立直、押引）沒有立即的真相，所以用反事實模擬評分：
在決策點保存局面，依你當時看得到的資訊重抽對手手牌與牌山（不偷看），
改選每個選項重打多次，比較各選項的期望結果，算出實際選擇的「損失」。

```sh
mj evaluate --strategy tenhou-normal-4p-hanchan/hand-reading --games 20                   # 名次點（打到整場結束）
mj evaluate --strategy tenhou-normal-4p-hanchan/hand-reading --games 20 --metric points   # 這一局的點數（較快、雜訊小）
mj evaluate ... --kinds discard,call --json result.json                      # 只評估某些決策、輸出 JSON
```

輸出各決策種類的「選到最佳的比例」「平均損失 ± 標準誤」「顯著損失比例」，何切與鳴牌再依
「有無立直者、向聽、巡目、名次」分組，並列出損失最大的顯著決策
（附上當時 `ctx.note` 寫下的理由與預測，方便找出思路哪裡錯了）。

比較兩個策略（或同一策略的兩個版本）的整體強弱用 duplicate 對戰：

```sh
mj simulate 策略A 策略B --blocks 1000
```

和高手比較用牌譜復盤：策略旁觀天鳳鳳凰卓的對局，在玩家與策略選擇不同的地方用反事實模擬比較兩者，
最後依差異類型（向聽倒退、進張較少、多鳴、漏鳴、默聽……）統計平均差值。
某一類的平均差值顯著為負（「你較好」）時，代表高手在這類局面比策略好，是改進策略的線索。

```sh
mj review data/tenhou --strategy tenhou-normal-4p-hanchan/hand-reading --json review.json
```

## 7. 對局記憶 `Memory`

`memory.rs` 定義 `pub struct Memory`（需要 `Default` 與 `Clone`，通常寫 `#[derive(Default, Clone)]`），整場對局（半莊）中所有決策共用，
每場對局開始時重新建立。`observe(me, &Event, &mut Memory)` 對每個事件呼叫，用來記下觀察到的事，例如：

- 某家立直了幾次（愛立直的人）
- 某家用筋牌騙和過牌（他的筋牌不可信）

決策函式收到 `&mut Memory`，可以讀也可以寫。

**慣例：預測函式讀 `mem`，不直接讀 `ctx.view` 的完整紀錄。** `ctx.view` 記得每一張牌的摸切與順序，
但人類不會全部記住；把「人類會記的事」寫在 `observe` 裡，預測讀記憶，策略的思路才是人類做得到的。
需要完整紀錄（例如復盤時指出「其實可以注意到」）才直接讀 `ctx.view`。

範例（`tenhou-normal-4p-hanchan/push-fold`）：`tsumogiri_streak` 從中巡（`WATCH_FROM_TURN = 6`）起記錄各家連續摸切次數，
手切或鳴牌歸 0；`predict::tenpai` 依此提高副露者的聽牌機率。

## 8. 可調參數與自動調整

策略中的門檻與倍數寫成 `Param` 常數：預設值寫在程式裡（讀程式就知道策略怎麼想），執行時可以覆寫，
自動調整工具靠覆寫來搜尋更好的值。

```rust
/// 最近 3 張中，每多 1 張手切，聽牌的勝算乘上這個值。
pub const PER_RECENT_TEDASHI: Param =
    Param::new("tenpai.per_recent_tedashi", 0.73, 0.4, 1.0, "最近 3 張每多 1 張手切的聽牌勝算倍數");
//                    名稱（唯一）            預設   範圍下限、上限  一句話說明

let factor = ctx.param(&PER_RECENT_TEDASHI).powi(n); // 讀取（有覆寫用覆寫值，否則用預設值）
```

- 建置時會自動找出策略中所有 `pub const 名稱: Param = Param::new(...)`，不需要另外登記。
- 整數意義的參數（幾向聽、幾張）也用 `f64` 宣告，比較時寫 `f64::from(x) >= ctx.param(&P)`。
- 查表（例如 `BASE` 這種依副露與巡目的表）、一起擬合的一組倍數，維持一般的常數。

```sh
mj params tenhou-normal-4p-hanchan/hand-reading                       # 列出參數：目前值、預設、範圍、說明
mj selfplay --seats "S@call.far_shanten=3;riichi.min_waits=3,S,greedy,greedy"   # 任何指令的策略名稱都可以加 @覆寫
mj simulate S@tuned.txt S --blocks 2000                                # 參數檔：每行一個「名稱 = 值」

# 預測的參數：在牌譜上以對數損失擬合，另一組牌譜驗證
mj tune S --objective logloss --source predict::tenpai --params tenpai. --logs <擬合用牌譜> --validate-logs <驗證用牌譜>
# 決策的參數：候選值與目前值直接 duplicate 對戰，好過 2 個標準誤才採用；最後在新的牌山 seed 上驗證
mj tune S --objective duplicate --params call. riichi.min_waits --blocks 2000 --out tuned.txt
```

調整找到更好的值之後，把程式裡的預設值改掉（參數檔只是暫時的覆寫），並在驗證時換一組牌山 seed，
避免對同一組牌山過度調整。預測變準不一定讓決策變好（M5、M8 都觀察到），預測參數調整後也要用 duplicate 確認決策沒有變差。
