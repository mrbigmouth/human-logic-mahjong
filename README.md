# Human Logic Mahjong：人類思路的可解釋麻將 AI

把人類打麻將的思路——牌效、牌型、讀牌、押引、局勢、詐騙——寫成**人類讀得懂的 if／else 決策程式**，
用兩種獨立的評分（預測看準確度、決策看反事實期望值）找出思路的漏洞並改進，
最後做成能**復盤並說明「為什麼」**的練習軟體。

- 初始目標：策略編輯 → 電腦麻將 AI
- 次要目標：AI 之間對戰，用 duplicate 方式分析勝率
- 終極目標：人機對戰練習、復盤指出失誤，並提供其他策略選項

規則：日本立直麻將（之後可擴充其他規則）。平台：Windows / macOS / Linux。

## 文件

| 文件 | 內容 |
|---|---|
| [docs/01-feasibility.md](docs/01-feasibility.md) | 可行性評估、風險、與現有 AI 的定位 |
| [docs/02-architecture.md](docs/02-architecture.md) | 技術選型、crate 結構、資料流、跨平台 |
| [docs/03-strategy-model.md](docs/03-strategy-model.md) | 策略模型：決策函式、對局記憶、預測與決策的兩種評分 |
| [crates/mj-strategy/TOOLS.md](crates/mj-strategy/TOOLS.md) | **策略編輯者工具手冊**：目錄寫法、決策函式、全部工具 |
| [crates/mj-strategy/DATA.md](crates/mj-strategy/DATA.md) | **策略編輯者資料結構說明**：`Ctx`、`PlayerView`、牌河、副露、`Event`、`Claim`、計分 |
| [docs/04-milestones.md](docs/04-milestones.md) | 里程碑 M0–M10 與驗收條件 |
| [docs/05-glossary.md](docs/05-glossary.md) | 術語對照表：程式中的日語拼音名稱、牌的記法（m 萬、p 筒、s 索、z 字）、役種中文名 |

## 快速開始

需要 Rust（stable，1.85 以上）。

```sh
cargo build --workspace
cargo test --workspace
cargo run -p mj-cli -- --help                          # 執行檔名稱為 mj
cargo run -p mj-cli -- strategies                      # 列出所有策略

# 何切分析：執行策略的決策並顯示理由（最後一張是剛摸到的牌）
cargo run -p mj-cli -- nanikiru 23m456p789s11z224s9m --dora 1s

cargo build --release -p mj-cli
S=tenhou-normal-4p-hanchan/hand-reading
./target/release/mj strategies                                             # 策略一覽（加名稱顯示完整說明）
./target/release/mj selfplay --games 2000 --seats $S,greedy,greedy,random   # 對戰成績
./target/release/mj predict-eval --strategy $S --games 300                 # 預測準確度
./target/release/mj evaluate --strategy $S --games 20                      # 決策品質（反事實模擬）
./target/release/mj simulate $S greedy --blocks 1000                        # duplicate 對戰比較
./target/release/mj simulate $S "$S@deceive.meisai=0" --field $S --blocks 5000   # 場地對戰：各自和三家同一對手同桌
H="$S@deceive.tedashi=0;deceive.meisai=0"
./target/release/mj predict-eval --seats "$S,$H,$H,$H" --games 1000         # 四家不同策略，依被預測者看讀牌準不準

# 天鳳牌譜：下載鳳凰卓牌譜（只保留最近幾天）→ 用引擎重播並比對點數 → 對真人對局評估預測
tools/fetch-tenhou.sh 2026100700 2026100701 -n 20
./target/release/mj import data/tenhou
./target/release/mj predict-eval --strategy $S --logs data/tenhou

# 復盤：逐手比較牌譜中某位玩家與策略的選擇（理由、反事實期望值、失誤統計）
./target/release/mj review data/tenhou/<牌譜>.xml --player <名稱> --json review.json

# 練習 App（網頁版）：建置前端後啟動，瀏覽器開 http://localhost:30100
(cd app && npm install && npm run build)
./target/release/mj serve
# 桌面版（需要 Tauri 的系統相依套件）：cd app && npm run tauri dev｜npm run tauri build

# fuzz 建置：每回合檢查牌數與點數守恆
cargo build --profile fuzz -p mj-cli
./target/fuzz/mj selfplay --games 1 --dump game.jsonl  # 輸出完整事件紀錄
```

點數計算的差異測試（與 Python `mahjong` 函式庫比對）請見 [docs/02-architecture.md](docs/02-architecture.md#測試策略)。

## 目錄

```
crates/
  mj-core      牌、赤寶牌、副露、TileCounts（與規則無關）
  mj-calc      向聽、進張、聽牌、和牌拆解（效能熱點）
  mj-rules     RuleSet trait；riichi：役、符、點數
  mj-engine    對局狀態機、Action、PlayerView、Player、事件紀錄
  mj-tools     純計算工具箱（策略編輯者使用）：手牌、安全度、役、點數、牌桌
  mj-strategy  src/strategy-list/ 策略（if／else 決策程式）＋ framework 呼叫框架  ← 核心
  mj-sim       參考機器人、牌譜重播、預測準確度評估、反事實決策評估、duplicate 對戰
  mj-tuner     （M8）可調參數的自動調整
  mj-paifu     牌譜匯入（天鳳 mjlog XML／JSON）：引擎重播、比對點數、重建每個決策點的視角
  mj-review    復盤：逐手比對玩家與策略、反事實評估、失誤統計（M7）
  mj-mjai      mjai 協定（和外部 AI 對戰）
  mj-app       練習 App 的核心：人機對戰、即時提示、給 UI 的牌桌狀態、對局後復盤（M9）
  mj-cli       命令列工具 mj
docs/          設計文件
tools/oracle/  點數差異測試（Python）
tools/fetch-tenhou.sh  天鳳牌譜下載（存到 data/tenhou/，不納入版本控制）
app/           練習 App 的前端（Vite＋React＋TypeScript）；app/src-tauri 是桌面版（Tauri）
```
