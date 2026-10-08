# 02 程式架構

## 技術選型

| 部分 | 選擇 | 理由 |
|---|---|---|
| 核心（規則、計算、策略、模擬） | **Rust**（Cargo workspace） | 自我對戰需要大量模擬；速度接近 C++ 且記憶體安全；可以編譯成 WASM |
| 桌面 UI | **Tauri v2** ＋ TypeScript | 一套程式碼打包 Windows / macOS / Linux，執行檔小，可直接呼叫 Rust |
| 策略 | Rust 的 if／else 決策函式（`strategy-list` 目錄） | 判斷直接寫成人類讀得懂的程式，理由用 `ctx.note` 寫在判斷旁邊；計算交給 `mj-tools` |
| 外部 AI 對接 | mjai 協定（JSON over stdio/TCP） | 社群標準，可以和 Mortal、akochan 等 AI 對戰 |

## Crate 相依關係

```
mj-core ── mj-calc ── mj-rules ── mj-engine ── mj-tools ── mj-strategy ─┬─ mj-sim ── mj-tuner
                                     │                                  └─ mj-review
                                     └──────── mj-mjai
mj-cli / app(Tauri) ── 以上全部
```

| Crate | 職責 | 里程碑 |
|---|---|---|
| `mj-core` | `Tile`（34 種）、`Pai`（含赤寶牌）、`Meld`、`TileCounts`、牌的記法解析 | M1 ✅ |
| `mj-calc` | 精確向聽（標準型、七對、國士）、進張、聽牌、和牌拆解；接受讀牌層的 `RemainingEstimate` | M1 ✅（和牌機率、期望打點：M2） |
| `mj-rules` | `RuleSet` trait；`riichi`：全部役種與役滿、符、點數、`RiichiConfig` 規則選項（預設天鳳規則） | M1 ✅ |
| `mj-engine` | 對局狀態機（`run_game`）、`Action`、`PlayerView`、`Player` trait、seed 牌山、事件流；局面快照、重抽看不到的牌、從快照繼續（M3） | M1 ✅ / M3 ✅ |
| `mj-tools` | 策略編輯者的純計算工具箱：手牌、動作後評估、看得到的牌、安全度事實（現物、筋、No Chance）、役的路線、點數、寶牌、牌桌 | M2.5 ✅ |
| `mj-strategy` | `src/strategy-list/` 策略（main.rs、memory.rs、decision/、utils/predict/）；`framework/` 把引擎詢問拆成決策、記錄理由與預測；`build.rs` 自動產生模組；`TOOLS.md` 工具手冊 | M2.5 ✅ / M4+ 策略內容 |
| `mj-sim` | 參考機器人（`RandomBot`、`GreedyBot`）；`replay` 牌譜重播；`predict_eval` 預測準確度；`counterfactual` 反事實決策評估；`duplicate` 對戰 | M3 ✅ |
| `mj-tuner` | 可調參數的自動調整（預測以準確度擬合、決策以反事實期望值） | M8 |
| `mj-review` | 天鳳／mjai 牌譜匯入（M4）；逐手比對使用者與策略的選擇，附理由與反事實期望值（M7） | M4 / M7 |
| `mj-mjai` | mjai bridge，與外部強 AI 對戰與比對 | M8 |
| `mj-cli` | 執行檔 `mj`：`strategies`、`nanikiru`、`selfplay`、`predict-eval`、`evaluate`、`simulate`、`review`、`play`、`serve` | 持續 |

## 關鍵設計

### 規則可擴充（需求 5）
規則相關的計算（役、符、點數、規則選項）放在 `mj-rules` 的各規則模組；與規則無關的牌型計算（向聽、拆解）放在 `mj-calc`，各規則共用。
對局流程目前是日麻專用的狀態機（`mj-engine` 的 `kyoku` / `game`）。不同麻將的流程差異很大（花牌、16 張、計台），硬做成一個泛型狀態機反而難讀，所以之後加入台灣 16 張時，會新增 `mj-rules::taiwan` 和一個對應的狀態機模組，但**沿用同一套 `Action` / `PlayerView` / `Player` / `Event` 介面**。strategy、sim、review 只依賴這些介面，所以不必修改。

### 資訊隔離
策略只拿得到 `PlayerView`，看不到牌山或他人手牌，從型別上就防止 AI「作弊」。復盤時也從同一個 `PlayerView` 重算，所以解說和實際打牌時的判斷一致。

### 可重現性
牌山由 seed 決定，策略是決定性的程式，所以任何一局都可以用（seed, 策略）重播。反事實評估正是利用這點：重播到決策點、強制改選另一個選項，再繼續打。

### UI 與核心的介面
UI 只透過一層 API（`new_game`、`act`、`get_view`、`analyze`）和核心溝通：
- 桌面版：Tauri command 直接呼叫 Rust。
- 開發和網頁版：`mj serve` 提供 HTTP API（容器內用 port 30100，瀏覽器開 `http://localhost:30100`）；之後核心也可以編成 WASM，完全在前端執行。

### 跨平台建置
Tauri 打包要在目標平台或 CI 上進行（GitHub Actions 三平台 matrix）。開發容器沒有 GUI，所以 UI 先用瀏覽器開發，最後在 Mac 上打包驗證。

## 測試策略

規則引擎的正確性是一切的基礎（錯的點數會讓所有勝率分析失真），所以採用多層驗證，而不是只靠手寫案例：

| 層 | 方法 | 位置 |
|---|---|---|
| 向聽 | **定義性質測試**：13 張手牌「聽牌 ⇔ 存在一張和牌」，且「向聽 = 1 + 摸一打一後的最小向聽」；另與 Python `mahjong` 函式庫做 10 萬手差異測試 | `crates/mj-calc/tests/shanten_definition.rs`、`tools/oracle/check_shanten.py` |
| 點數 | **差異測試**：隨機產生 20 萬手和牌（含副露、槓、赤寶牌、各種狀況役），與獨立實作的 Python [`mahjong`](https://pypi.org/project/mahjong/) 函式庫比對翻、符、點數 | `crates/mj-rules/examples/gen_wins.rs`、`tools/oracle/check_scoring.py` |
| 規則情境 | 指定牌山與腳本玩家，逐條驗證振聽、雙響、食替、流局罰符、天和、雙立直一發、包牌、搶槓、立直振聽、四風連打 | `crates/mj-engine/tests/scenarios.rs` |
| 終局條件 | 純函式 `match_over` 的單元測試（南四、西入、和了止め、擊飛） | `crates/mj-engine/src/game.rs` |
| 模糊測試 | `fuzz` profile（release 速度＋debug assertions）下大量自我對戰，每回合檢查**牌數守恆**與**點數守恆** | `mj selfplay`（`cargo build --profile fuzz`） |

執行差異測試：

```sh
python3 -m venv tools/oracle/.venv && tools/oracle/.venv/bin/pip install mahjong==2.0.0
cargo run --release -p mj-rules --example gen_wins -- 200000 11 > /tmp/wins.jsonl
tools/oracle/.venv/bin/python tools/oracle/check_scoring.py /tmp/wins.jsonl

cargo build --release -p mj-calc --example shanten_stdin
tools/oracle/.venv/bin/python tools/oracle/check_shanten.py 100000
```

### 已知的規則簡化
- 包牌（大三元、大四喜）：多個役滿複合時，整手都依包牌規則支付（天鳳只對包牌的那個役滿部分適用）。
- 純空聽（聽的牌自己已經拿了 4 張）在流局時算未聽牌。
- 雙響時，本場和供託都歸離放銃者最近（上家側）的和牌者。
