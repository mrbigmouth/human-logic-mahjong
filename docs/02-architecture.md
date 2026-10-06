# 02 程式架構

## 技術選型

| 部分 | 選擇 | 理由 |
|---|---|---|
| 核心（規則、計算、策略、模擬） | **Rust**（Cargo workspace） | 自我對戰需要大量模擬；速度接近 C++ 且記憶體安全；可以編譯成 WASM |
| 桌面 UI | **Tauri v2** ＋ TypeScript | 一套程式碼打包 Windows / macOS / Linux，執行檔小，可直接呼叫 Rust |
| 策略設定 | 程式模組（Rust trait）＋ TOML profile | 邏輯寫在程式裡，權重、曲線和參數放在 TOML，方便調整、diff 和自動調參 |
| 外部 AI 對接 | mjai 協定（JSON over stdio/TCP） | 社群標準，可以和 Mortal、akochan 等 AI 對戰 |

## Crate 相依關係

```
mj-core ─┬─ mj-rules ── mj-engine ─┬─ mj-strategy ─┬─ mj-sim ── mj-tuner
         └─ mj-calc ───────────────┘               ├─ mj-review
                                    mj-engine ──── mj-mjai
mj-cli / app(Tauri) ── 以上全部
```

| Crate | 職責 | 里程碑 |
|---|---|---|
| `mj-core` | `Tile`（34 種）、`TileCounts`、牌的記法解析 | M0 ✅ |
| `mj-rules` | `RuleSet` trait；`riichi`（役、符、點數、流局、`RiichiConfig` 規則選項） | M1 |
| `mj-engine` | 對局狀態機、`Action`、`PlayerView`（玩家可見資訊）、seed 牌山、事件流 | M1 |
| `mj-calc` | 向聽、進張、和牌機率、期望打點；接受讀牌層的 `RemainingEstimate` | M1–M2 |
| `mj-strategy` | `Consideration`、`Modifier`、`Profile`、`Registry`、`Decider`、`Trace` | M0 框架 ✅ / M2+ 模組 |
| `mj-sim` | duplicate 對戰、rayon 平行化、統計 | M3 |
| `mj-tuner` | CMA-ES 調參 | M6 |
| `mj-review` | 天鳳／mjai 牌譜匯入、Trace diff、解說模板 | M8 |
| `mj-mjai` | mjai bridge | M6 |
| `mj-cli` | 執行檔 `mj`：`check`、`simulate`、`tune`、`review`、`play`、`serve` | 持續 |

## 關鍵設計

### 規則可擴充（需求 5）
所有和規則相關的東西都放在 `RuleSet` 後面。engine 對 `RuleSet` 泛型化；strategy 只看 `PlayerView` 和 `Action`。
之後加入台灣 16 張麻將時，要新增的是 `mj-rules::taiwan`，以及該規則專屬的考量模組（例如台數計算）；牌效、讀牌、模擬器、復盤都可以沿用。

### 資訊隔離
策略只拿得到 `PlayerView`，看不到牌山或他人手牌，從型別上就防止 AI「作弊」。復盤時也從同一個 `PlayerView` 重算，所以解說和實際打牌時的判斷一致。

### 可重現性
牌山由 seed 決定；`Decider::decide` 是決定性的（同分時用穩定排序）。任何一局都可以用（seed, profiles）重播，方便除錯和比較。

### UI 與核心的介面
UI 只透過一層 API（`new_game`、`act`、`get_view`、`analyze`）和核心溝通：
- 桌面版：Tauri command 直接呼叫 Rust。
- 開發和網頁版：`mj serve` 提供 HTTP API（容器內用 port 30100，瀏覽器開 `http://localhost:30100`）；之後核心也可以編成 WASM，完全在前端執行。

### 跨平台建置
Tauri 打包要在目標平台或 CI 上進行（GitHub Actions 三平台 matrix）。開發容器沒有 GUI，所以 UI 先用瀏覽器開發，最後在 Mac 上打包驗證。
