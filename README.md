# 可解釋麻將策略 AI（Explainable Mahjong Strategy AI）

把人類打麻將的思路——牌效、牌型、讀牌、押引、局勢、詐騙——寫成**可程式化、可調權重、可解釋**的策略模組，透過大量自我對戰找出最佳組合，最後做成能**復盤並說明「為什麼」**的練習軟體。

- 初始目標：策略編輯 → 電腦麻將 AI
- 次要目標：AI 之間對戰，用 duplicate 方式分析勝率
- 終極目標：人機對戰練習、復盤指出失誤，並提供其他策略選項

規則：日本立直麻將（之後可擴充其他規則）。平台：Windows / macOS / Linux。

## 文件

| 文件 | 內容 |
|---|---|
| [docs/01-feasibility.md](docs/01-feasibility.md) | 可行性評估、風險、與現有 AI 的定位 |
| [docs/02-architecture.md](docs/02-architecture.md) | 技術選型、crate 結構、資料流、跨平台 |
| [docs/03-strategy-model.md](docs/03-strategy-model.md) | 六層策略模型、Utility AI、動態權重、Trace 與復盤 |
| [docs/04-milestones.md](docs/04-milestones.md) | 里程碑 M0–M9 與驗收條件 |
| [docs/05-glossary.md](docs/05-glossary.md) | 術語對照表：程式中的日語拼音名稱、牌的記法（m 萬、p 筒、s 索、z 字）、役種中文名 |

## 快速開始

需要 Rust（stable，1.85 以上）。

```sh
cargo build --workspace
cargo test --workspace
cargo run -p mj-cli -- --help                          # 執行檔名稱為 mj
cargo run -p mj-cli -- check profiles/example.toml     # 驗證策略側寫

# 自我對戰（參考機器人）。fuzz profile 會在每回合檢查牌數與點數守恆
cargo build --profile fuzz -p mj-cli
./target/fuzz/mj selfplay --games 10000 --bots greedy
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
  mj-strategy  Consideration / Modifier / Profile / Decider / Trace  ← 核心
  mj-sim       參考機器人；duplicate 自我對戰與統計
  mj-tuner     自動調參
  mj-review    牌譜匯入與復盤解說
  mj-mjai      mjai 協定（和外部 AI 對戰）
  mj-cli       命令列工具 mj
profiles/      策略側寫（TOML）
docs/          設計文件
tools/oracle/  點數差異測試（Python）
app/           （M7）Tauri 桌面 UI
```
