# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 專案概要

可解釋的日本立直麻將 AI：把人類打麻將的思路寫成**人類讀得懂的 if／else 決策程式**，
用兩種獨立的評分改進（預測看全知視角下的準確度、決策看反事實模擬的期望值），
最終做成能復盤並說明「為什麼」的練習軟體。Rust workspace；桌面 UI（Tauri）尚未開始（M9）。

- 與使用者溝通、文件、**程式註解一律使用繁體中文**。
- 函式與型別名稱多為日語羅馬拼音（shanten 向聽、ukeire 進張、kuikae 食替、furiten 振聽……），對照表在 `docs/05-glossary.md`。牌的記法：`m` 萬、`p` 筒、`s` 索、`z` 字（1z–7z = 東南西北白發中）、`0m/0p/0s` 赤五。
- 設計文件：`docs/01`–`05`；里程碑與目前進度在 `docs/04-milestones.md`。
- 給策略編輯者的文件：`crates/mj-strategy/TOOLS.md`（目錄寫法、決策函式、工具清單）與 `crates/mj-strategy/DATA.md`（`Ctx`、`PlayerView`、`Event` 等所有資料結構）。修改這些結構或新增工具時要同步更新。

## 里程碑

詳細內容、驗收條件與各里程碑的完成紀錄在 `docs/04-milestones.md`（以它為準）。完成一個里程碑時：
把狀態改為 ✅、在 docs/04 新增「完成內容」與驗收數據，並同步更新下表。

M2.5 重構後的開發循環是「寫判斷 → 評估（預測看準確度、決策看反事實期望值）→ 讀損失最大的例子與理由 → 修改」，
所以排序原則是：評估工具先行、共用的牌譜匯入提早、復盤（CLI／JSON）先於桌面 App。
標「策略」的里程碑主要是在 `strategy-list` 寫判斷（使用者可能親自編寫），其餘提供工具、預測種類與評估方式。

| # | 類型 | 內容 | 狀態 |
|---|---|---|---|
| M0–M2 | 基礎／策略 | 骨架、規則引擎（與 Python `mahjong` 零差異）、第一個牌效 AI | ✅ |
| M2.5 | 基礎 | 策略架構重構：`mj-tools`、`strategy-list`、決策函式、記憶、`predict-eval`、`evaluate` | ✅ |
| M3 | 評估 | 局面快照與重抽（去除全知）、名次點、自動加重打、依情境分組的報告與 JSON、`mj simulate`（duplicate）、常駐預測 | ✅ |
| **M4** | 資料 | 天鳳牌譜匯入 → 事件紀錄 → 在決策點重建 `PlayerView`；對人類牌譜做預測評估 | **下一步** |
| M5 | 策略 | 防守與押引、安全牌管理、立直或默聽、鳴牌時機與打點 | |
| M6 | 策略 | 讀牌：聽牌、聽牌張、打點預測（手切摸切、牌河順序、副露），以自我對戰與天鳳牌譜校準 | |
| M7 | 產品 | 復盤（CLI／JSON）：使用者牌譜逐手比對，附策略理由與反事實期望值、失誤統計 | |
| M8 | 評估 | 具名可調參數與自動調整（預測以對數損失、決策以反事實期望值）；mjai 介接與強 AI 比對 | |
| M9 | 產品 | Tauri 桌面練習 App（人機對戰、即時提示、對局後復盤）；`mj serve` | |
| M10 | 策略 | 印象操作（詐騙與反詐騙） | |

## 常用指令

```sh
cargo build --workspace
cargo test --workspace                      # 部分測試在 debug 模式較慢，可加 --release
cargo test -p mj-engine --test scenarios    # 單一整合測試檔
cargo test -p mj-calc shanten               # 依名稱篩選
cargo clippy --workspace --all-targets      # 目前無警告，請維持
cargo fmt --all
# cargo fmt 不會處理 strategy-list（模組由 build.rs 產生），要另外執行：
find crates/mj-strategy/src/strategy-list -name '*.rs' | xargs rustfmt --edition 2024
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

CLI（執行檔名 `mj`，`cargo build --release -p mj-cli` 後在 `target/release/mj`）：

```sh
mj strategies                                   # 列出 strategy-list 中的策略
mj nanikiru 23m456p789s11z224s9m --dora 1s      # 何切：執行策略決策並顯示理由（最後一張 = 剛摸的牌）
mj selfplay --games 2000 --seats tenhou-normal-4p-hanchan,greedy,greedy,random   # 對戰統計（預設每場輪換座位）
mj predict-eval --strategy tenhou-normal-4p-hanchan --games 300    # 預測準確度
mj evaluate --strategy tenhou-normal-4p-hanchan --games 20         # 決策品質（反事實模擬；--metric points 較快、雜訊小）
mj simulate tenhou-normal-4p-hanchan greedy --blocks 1000           # duplicate 對戰（成對比較兩個策略／版本）
```

- `--profile fuzz`（Cargo.toml 定義：release 速度＋debug assertions）：引擎每回合檢查牌數與點數守恆，大量自我對戰即為模糊測試：`cargo build --profile fuzz -p mj-cli && ./target/fuzz/mj selfplay ...`
- 修改向聽或點數計算後，必須跑 Python 差異測試（與獨立的 `mahjong` 函式庫比對，目前 0 差異），步驟在 `docs/02-architecture.md` 的「測試策略」。venv 在 `tools/oracle/.venv`（已 gitignore，路徑寫死在 venv 內，專案搬移後需重建）。

## 架構

Crate 相依鏈：`mj-core → mj-calc → mj-rules → mj-engine → mj-tools → mj-strategy → mj-sim → mj-cli`
（`mj-tuner`、`mj-review`、`mj-mjai` 目前只是預留的空殼）。

- **mj-core**：`Tile`（34 種，編號 0–33）、`Pai`（牌＋赤五旗標）、`TileCounts`（34 格牌數表，計算都用它）、`Meld`。
- **mj-calc**：精確向聽（各花色 DP，thread-local 快取）、進張、聽牌張、和牌拆解。與規則無關。
- **mj-rules::riichi**：役、符、點數（`score` 會列舉所有拆法與和牌張位置，取最高者）；`RiichiConfig::TENHOU` 為預設規則。
- **mj-engine**：對局狀態機。`kyoku.rs` 一局、`game.rs` 整場與終局判定（`match_over`）。
  - 玩家只拿得到 `PlayerView` 與遮蔽過的 `Event`，不能作弊。
  - 第 N 手牌山只由 `hand_seed(seed, N)` 決定（與打法無關）；`GameConfig.preset_walls` / `max_hands` 可指定牌山與手數。
  - `run_game_with` 借用玩家（對局後還能取回決策紀錄）；`keep_log` 時 `GameResult` 附完整事件紀錄與各局牌山。
- **mj-tools**：給策略編輯者的**純計算**工具（不得包含「該怎麼打」的判斷）：動作後的向聽／進張、看得到的牌、現物／筋／No Chance、鳴牌後的役、點數估計、名次點差。新增工具時同步更新 `TOOLS.md`。
- **mj-strategy**：
  - `src/strategy-list/<規則名>/`：策略本體，給人編輯。`main.rs` 的 `register(&mut Book<Memory>)` 登記決策；`memory.rs` 的 `Memory`（需 `Default`）是整場共用的記憶，`observe` 由事件更新；`decision/*.rs` 每檔匯出一個 `decide(ctx, mem)`；`utils/predict/` 是預測函式。
  - `build.rs` 掃描 strategy-list，產生帶 `#[path]` 的模組樹與 `all_strategies()`（`-` 轉成 `_`），由 `lib.rs` 的 `include!` 引入。新增檔案不需要寫 `mod`。同策略內互相引用用 `super::super::...`；檔案開頭 `use crate::prelude::*;`。
  - `memory.rs` 的 `observe` 只記「人類會記的事」（例如中巡起的連續摸切次數），預測函式讀 `mem` 而非 `ctx.view` 的完整紀錄；`book.predict_each_turn` 登記每回合執行、只用於評估的常駐預測。
  - `framework/player.rs`（`BookPlayer`）把引擎的單次詢問拆成依人類順序的決策：自己回合「九種九牌 → 和牌 → 開槓 → 何切 → 立直」，他家打牌「和牌 → 鳴牌」。決策結果必須是框架列出的選項之一，否則改用預設並標記 `invalid`。每個決策可記錄成 `DecisionRecord`（選項、選擇、`ctx.note` 理由、`ctx.claim` 預測），也可用 `Force` 強制指定選項。
- **mj-engine 的快照**（`snapshot.rs`）：`capture_snapshot` 重播到「即將第 n 次詢問某座位」時保存局面；`Snapshot::determinize` 依決策者視角重抽看不到的牌；`resume_game` 從快照繼續。快照點依賴 `Table.asks`（每座位被詢問次數 = 玩家的 `act_index`）。
- **mj-sim**：參考機器人（`RandomBot`、`GreedyBot`）；`replay`（從完整事件紀錄重建任一時刻的手牌、判定振聽）；`predict_eval`（用 `judge` 判定每個 `Claim` 的真相，算 Brier、技巧分數、校準表，依被預測者情境分組）；`counterfactual`（快照 → 重抽 R 個世界 → 各選項以 `Force` 強制後重打，名次點或本局點數，自動加重打）；`duplicate`（同牌山座位輪換的成對比較）。

### 重要的不變條件

- **策略必須是決定性的**（同樣輸入 → 同樣輸出，不用亂數）。快照靠「同牌山重播會走到同一個詢問點」，反事實評估也會檢查 `forced` 紀錄確認有走到。
- 策略的 `Memory` 必須實作 `Default` 與 `Clone`（反事實評估在快照點複製玩家）。
- 新增一種預測（`framework/claim.rs` 的 `Claim`）時，必須同時在 `mj-sim/src/predict_eval.rs` 的 `judge` 寫出全知視角下的真相怎麼算。
- `DiscardRecord.seq` 是本局全體打牌順序，現物、立直後等時序判斷都依賴它。
- 已知規則簡化（包牌複合役滿、純空聽、雙響供託歸屬）列在 `docs/02-architecture.md` 末尾；修改規則行為時一併更新。
