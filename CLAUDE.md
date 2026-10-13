# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 專案概要

可解釋的日本立直麻將 AI：把人類打麻將的思路寫成**人類讀得懂的 if／else 決策程式**，
用兩種獨立的評分改進（預測看全知視角下的準確度、決策看反事實模擬的期望值），
最終做成能復盤並說明「為什麼」的練習軟體。Rust workspace；練習 App 的前端在 `app/`（Vite＋React＋TypeScript，桌面版 Tauri 在 `app/src-tauri/`）。

- 與使用者溝通、文件、**程式註解一律使用繁體中文**。
- 函式與型別名稱多為日語羅馬拼音（shanten 向聽、ukeire 進張、kuikae 食替、furiten 振聽……），對照表在 `docs/05-glossary.md`。牌的記法：`m` 萬、`p` 筒、`s` 索、`z` 字（1z–7z = 東南西北白發中）、`0m/0p/0s` 赤五。
- 設計文件：`docs/01`–`05`；里程碑與目前進度在 `docs/04-milestones.md`；已完成里程碑的紀錄與決策在 project-history skill。
- 給策略編輯者的文件：`crates/mj-strategy/TOOLS.md`（目錄寫法、決策函式、工具清單）與 `crates/mj-strategy/DATA.md`（`Ctx`、`PlayerView`、`Event` 等所有資料結構）。修改這些結構或新增工具時要同步更新。

## 里程碑

現行計畫與驗收條件在 `docs/04-milestones.md`（以它為準）。完成一個里程碑時：
把狀態改為 ✅、在 docs/04 寫下完成內容與驗收數據，並同步更新下表；
之後的計畫調整時，把已完成的紀錄移到 project-history skill。

**M0–M10 已完成**（引擎、`hand-reading` 策略、兩種評估、牌譜匯入、復盤、可調參數、mjai／Mortal、練習 App、印象操作）。
需要當時的完成內容、驗收數據、設計決策或否決過的做法時，使用 **project-history skill**（`.claude/skills/project-history/`）。

2026-10-13 重新規劃：**策略編輯的便利性與 UI／UX 優先，其他麻將規則延後**；依難度排序，前一項是後一項的基礎。

| # | 類型 | 內容 | 狀態 |
|---|---|---|---|
| M11 | 介面 | 牌桌介面改善：文字正置、四家點數、版面與可折疊面板、立直後自動摸切、音效、同種牌標示 | ✅ |
| M12 | 介面 | 牌譜存取與檢視器：匯出、讀入（天鳳／mjai）、逐打檢視、讀入的牌譜可復盤 | ✅ |
| M13 | 介面＋工具 | 和牌路線提示（`mj-tools` 的 `win_routes`） | ✅ |
| M14 | 評估＋介面 | 反事實實驗：對局與復盤中比較各選項「之後依策略打」的結果 | |
| M15 | 評估＋介面 | GUI 對戰測試平台：在 App 中執行 selfplay／simulate／場地對戰／預測評估，逐場檢視與實驗 | |
| M16 | 開發流程 | 策略編輯流程：參數即時編輯＋監看原始碼自動重建重啟（已選定 A＋B） | |

GUI 的功能一律經由 `mj-app` 的 `App::call`，計算核心與 CLI 共用（在 `mj-sim`、`mj-review`）；
查策略一律透過參賽者登錄（`find_strategy_spec`、`entrants()`），讓 M16 的參數編輯與重建能直接接上。

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
mj strategies                                   # 列出策略與摘要；mj strategies <規則/策略> 顯示完整說明
mj nanikiru 23m456p789s11z224s9m --dora 1s      # 何切：執行策略決策並顯示理由（最後一張 = 剛摸的牌）
mj selfplay --games 2000 --seats tenhou-normal-4p-hanchan/hand-reading,greedy,greedy,random   # 對戰統計（預設每場輪換座位）
mj predict-eval --strategy tenhou-normal-4p-hanchan/hand-reading --games 300    # 預測準確度
mj evaluate --strategy tenhou-normal-4p-hanchan/hand-reading --games 20         # 決策品質（反事實模擬；--metric points 較快、雜訊小）
mj simulate tenhou-normal-4p-hanchan/hand-reading greedy --blocks 1000           # duplicate 對戰（成對比較兩個策略／版本）；附每局的和了率、放銃率
mj simulate <A> <B> --field <X> --blocks 5000                                    # 場地對戰：A、B 各自和三家 X 同桌，比較「面對 X 時」的差別
mj import data/tenhou                                               # 匯入牌譜：引擎重播並與原牌譜比對點數
mj predict-eval --logs data/tenhou                                  # 策略旁觀真人對局，評估預測準確度
mj read-stats data/tenhou --out target/read-stats                   # 匯出讀牌特徵＋真相的 CSV（校準讀牌預測用）
mj review <牌譜> --player <名稱>                                     # 復盤：逐手比較玩家與策略，反事實期望值、失誤統計（--json 輸出）
mj params <策略>                                                    # 可調參數（任何指令的策略名稱都可加 @參數檔 或 @名稱=值;名稱=值）
mj tune <策略> --objective logloss|duplicate --params <名稱或前綴.>   # 自動調整（預測：牌譜對數損失；決策：duplicate）
mj selfplay --seats "mjai:<外部程式 參數>,greedy,..."                # 說 mjai 協定的外部 AI 上桌（mj mjai-bot 是內建的測試機器人）
mj simulate tenhou-normal-4p-hanchan/hand-reading "mjai:tools/mortal/run.sh s42" --blocks 150   # 與 Mortal 對戰（安裝：tools/mortal/setup.sh）
mj agree data/tenhou --mjai "tools/mortal/run.sh s42"               # 一致率：策略、玩家、Mortal 的選擇（--selfplay N 改用自我對戰的局面）
mj review <牌譜> --player <名稱> --mjai "tools/mortal/run.sh s42"     # 復盤附 Mortal 的選擇（--show ai 只列與 Mortal 不同的決策）
mj serve                                                            # 練習 App 網頁版：http://localhost:30100（前端要先建置）
```

練習 App（`app/`）：

```sh
cd app && npm install
npm run build                 # tsc 型別檢查＋輸出到 app/dist（mj serve 提供這些檔案）
npm run dev                   # 開發：http://localhost:30101，/api 轉給 mj serve（30100）
npm run tauri dev             # 桌面版（需要 webkit2gtk 等系統套件，容器內已安裝；沒有 GUI 只能建置）
npm run tauri build           # 桌面版安裝檔；三平台由 .github/workflows/desktop.yml 建置
```

- 天鳳牌譜用 `tools/fetch-tenhou.sh <YYYYMMDDHH ...> -n <每小時場數>` 下載到 `data/tenhou/`（已 gitignore；天鳳只保留最近幾天的每小時清單）。
  `crates/mj-paifu/tests/data/` 有幾場匿名化的牌譜供測試；`data/tenhou/` 有資料時測試也會一併重播。

- `--profile fuzz`（Cargo.toml 定義：release 速度＋debug assertions）：引擎每回合檢查牌數與點數守恆，大量自我對戰即為模糊測試：`cargo build --profile fuzz -p mj-cli && ./target/fuzz/mj selfplay ...`
- 修改向聽或點數計算後，必須跑 Python 差異測試（與獨立的 `mahjong` 函式庫比對，目前 0 差異），步驟在 `docs/02-architecture.md` 的「測試策略」。venv 在 `tools/oracle/.venv`（已 gitignore，路徑寫死在 venv 內，專案搬移後需重建）。

## 架構

Crate 相依鏈：`mj-core → mj-calc → mj-rules → mj-engine → mj-tools → mj-strategy → mj-sim → mj-cli`，
另有 `mj-engine → mj-paifu → mj-sim`、`mj-sim, mj-paifu → mj-review／mj-tuner → mj-cli`、`mj-paifu → mj-mjai → mj-review, mj-cli`、`mj-review → mj-app → mj-cli（mj serve）, app/src-tauri`。

- **mj-core**：`Tile`（34 種，編號 0–33）、`Pai`（牌＋赤五旗標）、`TileCounts`（34 格牌數表，計算都用它）、`Meld`。
- **mj-calc**：精確向聽（各花色 DP，thread-local 快取）、進張、聽牌張、和牌拆解。與規則無關。
- **mj-rules::riichi**：役、符、點數（`score` 會列舉所有拆法與和牌張位置，取最高者）；`RiichiConfig::TENHOU` 為預設規則。
- **mj-engine**：對局狀態機。`kyoku.rs` 一局、`game.rs` 整場與終局判定（`match_over`）。
  - 玩家只拿得到 `PlayerView` 與遮蔽過的 `Event`，不能作弊。
  - 第 N 手牌山只由 `hand_seed(seed, N)` 決定（與打法無關）；`GameConfig.preset_walls` / `max_hands` 可指定牌山與手數。
  - `run_game_with` 借用玩家（對局後還能取回決策紀錄）；`keep_log` 時 `GameResult` 附完整事件紀錄與各局牌山。
- **mj-tools**：給策略編輯者的**純計算**工具（不得包含「該怎麼打」的判斷）：動作後的向聽／進張、看得到的牌、現物／筋（片筋）／No Chance／One Chance、牌河的手切摸切與副露（`river`）、鳴牌後的役、點數估計、名次點差、和牌路線（`routes`）。新增工具時同步更新 `TOOLS.md`。
- **mj-strategy**：
  - `src/strategy-list/<規則>/<策略>/`：策略本體，給人編輯。第一層是規則，第二層是同規則下的各個策略，完整名稱「規則／策略」（例如 `tenhou-normal-4p-hanchan/push-fold`）。每個策略必須有 `README.md`（`# 標題`＋一段摘要＋特色，`Strategy::title/summary/readme` 讀取，供 `mj strategies` 與日後 GUI 選單顯示；改打法時同步更新）；規則目錄也有 `README.md`。`main.rs` 的 `register(&mut Book<Memory>)` 登記決策；`memory.rs` 的 `Memory`（需 `Default`）是整場共用的記憶，`observe` 由事件更新；`decision/*.rs` 每檔匯出一個 `decide(ctx, mem)`；`utils/predict/` 是預測函式。
  - `build.rs` 掃描 strategy-list，產生帶 `#[path]` 的模組樹、`all_strategies()` 與 `all_rules()`（`-` 轉成 `_`，README 以 `include_str!` 嵌入），由 `lib.rs` 的 `include!` 引入。新增檔案不需要寫 `mod`。同策略內互相引用用 `super::super::...`；檔案開頭 `use crate::prelude::*;`。
  - 主力策略是 `tenhou-normal-4p-hanchan/hand-reading`（M6，M10 加上印象操作：`utils/image.rs` 用自己的讀牌預測看「別人眼中的我」）；`push-fold`（M5）與 `efficiency-fold`（M4）是凍結的舊版（比較基準，不再修改）。改策略後用 `mj simulate tenhou-normal-4p-hanchan/hand-reading tenhou-normal-4p-hanchan/push-fold --blocks 5000 --seed <新 seed>` 確認是否真的變強（調參時換 seed 再驗證一次，避免對同一組牌山過度調整）。
  - 可調參數：策略中的門檻寫成 `pub const X: Param = Param::new("名稱", 預設, 下限, 上限, "說明")`，以 `ctx.param(&X)` 讀取；build.rs 自動收集成 `Strategy::params()`，`Strategy::with_params(ParamSet)` 覆寫（`find_strategy_spec` 解析 `名稱@覆寫`）。調整後要把程式中的預設值改掉。
  - `memory.rs` 的 `observe` 只記「人類會記的事」（例如筋牌騙的次數）；牌河上看得到的手切／摸切標記、副露等用 `tools::river` 直接從 `ctx.view` 讀；`book.predict_each_turn` 登記每回合執行、只用於評估的常駐預測。
  - `framework/player.rs`（`BookPlayer`）把引擎的單次詢問拆成依人類順序的決策：自己回合「九種九牌 → 和牌 → 開槓 → 何切 → 立直 → 手切摸切」，他家打牌「和牌 → 鳴牌」。決策結果必須是框架列出的選項之一，否則改用預設並標記 `invalid`。每個決策可記錄成 `DecisionRecord`（選項、選擇、`ctx.note` 理由、`ctx.claim` 預測），也可用 `Force` 強制指定選項。
- **mj-engine 的快照**（`snapshot.rs`）：`capture_snapshot` 重播到「即將第 n 次詢問某座位」時保存局面；`Snapshot::determinize` 依決策者視角重抽看不到的牌；`resume_game` 從快照繼續。快照點依賴 `Table.asks`（每座位被詢問次數 = 玩家的 `act_index`）。
- **mj-paifu**：牌譜匯入。`tenhou_xml`／`tenhou_json` 解析成 `GameRecord`（配牌、依時間排序的 `Step`、原牌譜點數）；`replay` 由配牌與摸牌順序還原牌山（`build_wall`）、四家用共用進度的 `ScriptedPlayer` 照牌譜出牌讓引擎重打，再比對每局開始／結束與終局點數。可記錄每次詢問的 `DecisionPoint`（視角、合法動作、牌譜的選擇），也可掛策略當「旁觀者」（收到事件與詢問、做預測，動作照牌譜）。新格式只要輸出 `GameRecord` 即可。
- **mj-review**（M7）：牌譜復盤。可另外掛 mjai AI（`Options::mjai`）一起旁觀，算三方一致率（`agreement`）；`evaluate: false` 只比選擇不做反事實評估。重播時四家掛策略旁觀、記下策略的決策，用 `framework::option_of_action` 把玩家的實際動作對應到決策選項（走了另一條路的決策無法比較）；選擇不同的決策用 `mj_paifu::replay::capture` 在牌譜局面保存快照，再用 `counterfactual::evaluate_snapshot` 重打比較。「失誤」只比較玩家與策略兩個選項的成對差值（不和「最高期望值」比，最高者有挑選偏差）；差異依類型分類並彙整。
- **mj-tuner**（M8）：座標搜尋調整 `Param`：預測以牌譜上的對數損失（黃金分割）、決策以 duplicate（候選值與目前值直接對戰，好過 2 個標準誤才採用）。
- **mj-mjai**（M8）：`MjaiPlayer` 讓說 mjai 協定的 AI 上桌（`Bot` trait；`ProcessBot` = 子行程，一行事件陣列／一行回應，與 mjai.app 相同；立直兩段式；搶槓前補送槓事件）；`PooledBot` 在對局之間重複使用行程；`shadow()` 旁觀模式只記錄選擇，立直改用擴充請求 `mj_peek` 問要打的牌；`bots` 內建簡單機器人。事件與牌的轉換、mjai 牌譜匯入在 `mj_paifu::mjai`（引擎完整紀錄 ↔ mjai 牌譜可來回轉換）。只透過協定溝通，外部 AI 的授權（如 AGPL）不影響本專案。
- **tools/mortal**（AGPL-3.0-or-later，與專案隔離）：Mortal 的 mjai 包裝（`run.sh s42|hhim8826`）、安裝腳本 `setup.sh`（uv 的 Python 3.12 venv、固定版本的 Mortal 原始碼與 libriichi、權重 SHA-256 檢查，都放在 gitignore 的 `data/mortal/` 與 `tools/mortal/.venv/`）。每個 Mortal 行程約 380 MB；用到 mjai AI 時 `--jobs` 預設依可用記憶體決定。
- **mj-app**（M9）：練習 App 的核心，與 UI 無關。`Session` 在背景執行緒進行一場對局（人類座位 `HumanSeat` 詢問時暫停，等 `act`；立直後只能摸切時自動打出、幀標記 `auto`；提示策略在人類座位旁觀，每次詢問附上策略的選擇、理由與牌效表）；`table::TableBuilder` 把人類視角的事件轉成「事件＋牌桌」的幀（除錯建置時每次詢問都與引擎的 `PlayerView` 比對）；對局後轉成牌譜交給 `mj-review` 復盤（`review.rs` 的背景工作）。`paifu.rs` 的 `LoadedLog`（M12）讀入牌譜並重播，`table::log_frames` 產生任一座位視角（可公開手牌）的幀；剛打完的對局也先讀入再檢視或復盤；`table::table_mismatch` 檢查牌桌與引擎視角一致。`api::App::call(名稱, JSON)` 是 UI 唯一的入口：`mj serve` 的 `POST /api/<名稱>` 與 Tauri 的 `api` command 都轉給它，前端 `app/src/api.ts` 依環境選一種。修改 JSON 結構時同步更新 `app/src/types.ts`。
- **app/**（M9，M11 改版面）：前端。`Game.tsx` 長輪詢 `poll` 取得新幀並依序播放（一局結果停下等「繼續」），詢問要等播放追上才顯示；牌桌邊長由 `useBoardSize` 依可用空間計算（CSS 變數 `--B`，所有大小依它），自己的手牌疊在牌桌下緣，資訊在下方可折疊面板；`Board.tsx` 牌河與手牌畫成坐在下方再旋轉，文字（點數、名稱）放在不旋轉的圖層；`Tile.tsx` 的 `HoverProvider` 做同種牌標示；`sounds.ts` 以 Web Audio 合成音效；`Viewer.tsx` 牌譜檢視器、`LogScreen.tsx` 讀入後選玩家；`Review.tsx` 復盤（點決策看當時牌桌，靠 `ask.id - 1 = act_index` 對應幀）。存檔走 `api.ts` 的 `saveText`（桌面版：dialog 外掛＋只在 Tauri 的 command `save_text`；不放進 `App::call`，避免 `mj serve` 開放寫檔）。牌圖是 FluffyStuff 的 CC0 素材（`app/public/tiles/`）。
- **mj-sim**：參考機器人（`RandomBot`、`GreedyBot`）；`replay`（從完整事件紀錄重建任一時刻的手牌、判定振聽）；`predict_eval`（用 `judge` 判定每個 `Claim` 的真相，算 Brier、技巧分數、校準表，依被預測者情境分組；`collect_records` 在牌譜上旁觀評估）；`counterfactual`（快照 → 重抽 R 個世界 → 各選項以 `Force` 強制後重打，名次點或本局點數，自動加重打）；`duplicate`（同牌山座位輪換的成對比較）；`read_stats`（`mj read-stats`：在牌譜每個打牌時點輸出對手的讀牌特徵＋真相 CSV，用來校準讀牌預測）。

### 重要的不變條件

- **策略必須是決定性的**（同樣輸入 → 同樣輸出，不用亂數）。快照靠「同牌山重播會走到同一個詢問點」，反事實評估也會檢查 `forced` 紀錄確認有走到。
- 策略的 `Memory` 必須實作 `Default` 與 `Clone`（反事實評估在快照點複製玩家）。
- 新增一種預測（`framework/claim.rs` 的 `Claim`）時，必須同時在 `mj-sim/src/predict_eval.rs` 的 `judge` 寫出全知視角下的真相怎麼算。
- `DiscardRecord.seq` 是本局全體打牌順序，現物、立直後等時序判斷都依賴它。
- 修改引擎的規則行為後，要確認天鳳牌譜仍能零差異重播（`cargo test -p mj-paifu`，有下載資料時再跑 `mj import data/tenhou`）。
- 已知規則簡化（包牌複合役滿、純空聽、雙響供託歸屬）列在 `docs/02-architecture.md` 末尾；修改規則行為時一併更新。
