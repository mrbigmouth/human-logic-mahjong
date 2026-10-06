# 03 策略模型

## 六層策略

原本的四層（牌效、牌型、讀牌、詐騙）依**資訊流的順序**重新排列，並補上兩層：

| 層 | 名稱 | 輸入 → 輸出 | 例子 |
|---|---|---|---|
| L1 | 讀牌 Belief | 牌河、鳴牌、立直、手出或摸切 → 剩餘牌機率、各家聽牌率、各牌危險度、打點估計 | 「下家連續切中張後立直 → 聽牌率高，4m 是無筋」 |
| L2 | 牌效 Efficiency | 手牌＋L1 剩餘牌 → 向聽、進張、好型率、N 巡和牌率 | 「切 1p 進張 24 張，切 9s 進張 20 張」 |
| L3 | 牌型・打點 Value | 手牌、寶牌、役種路線 → 完成率 × 期望打點 | 「保留 5s 有機會斷么寶 1，期望值 +1800」 |
| L4 | 防守・押引 Defense **（新增）** | L1 危險度 × 對手打點 vs 自己的期望收入 | 「這張對立直家放銃率約 12%，期望損失 −960」 |
| L5 | 局勢 Situation **（新增獨立層）** | 分數、名次、莊位、剩餘局數、順位馬 → **調整其他層的權重** | 「all last 落後 6000，需要滿貫以上，打點權重 ×1.5」 |
| L6 | 印象操作 Image | 用自己的 L1 模型看自己的牌河（二階信念）→ 迷彩或威嚇的價值 | 「先切 5m 讓 2m-8m 看起來比較安全」 |

**為什麼 L1 在最前面？** 讀牌的結果（某張牌大概已經在別人手上）會直接改變牌效計算的剩餘張數，所以 L2 吃的是 L1 的輸出，而不是單純的「4 − 可見張數」（介面：`mj_calc::RemainingEstimate`）。

**為什麼局勢是獨立的一層？** 原本的「牌型」層同時包含「選擇目標」和「依局勢取捨」兩件事。把後者拆出來，變成**專門調整權重的修正器（Modifier）**，就是需求 4「實戰中動態改變權重」的具體做法。

**反詐騙**不另外成一層，而是在 L1 加上「對各家信號的信任度」參數；對手的詐騙傾向也可以從歷史牌譜統計。

## Utility AI：可解釋的決策核心

```
Ctx = PlayerView + Situation(L5 推導) + Belief(L1)
Weights = 基礎倍率 1.0 → 依序套用每個 Modifier
for action in legal_actions:
    for term in profile.considerations:
        eval    = term.evaluate(ctx, action)            # { value, reason }
        shaped  = term.curve(eval.value)                # 分段線性曲線
        weight  = term.weight × Weights.layer[term.layer]
        score  += shaped × weight                       # 每一項都記錄在 TermTrace
ranked = 依 score 排序（穩定排序）
```

### 介面（`crates/mj-strategy`）

```rust
pub trait Consideration: Send + Sync {
    fn id(&self) -> &'static str;                 // "efficiency.ukeire"
    fn layer(&self) -> Layer;
    fn evaluate(&self, ctx: &Ctx, action: &Action) -> Eval;   // Eval { value, reason }
}

pub trait Modifier: Send + Sync {
    fn id(&self) -> &'static str;                 // "situation.all_last_behind"
    fn adjust(&self, ctx: &Ctx, weights: &mut Weights) -> Option<Reason>;
}
```

**新增一個策略**：實作 trait → 在 `Registry` 註冊工廠函式（讀取 TOML 的 `params`）→ 在 profile 裡引用它的 id。

### Profile（TOML）

見 [`profiles/example.toml`](../profiles/example.toml)。一份 profile = 一組考量（權重＋曲線＋參數）＋一組修正器。
不同風格（速攻型、打點型、防守型）就是不同的 profile。復盤時可以拿多個 profile 分析同一個局面，提供「其他策略選項參考」。

### Trace 與復盤

`Decider::decide` 回傳 `Decision`：
- `ranked[i].terms[j]`：每個動作 × 每個考量的 raw、shaped、weight、contribution 和 reason
- `trace.modifiers_fired`：這次哪些修正器生效、原因是什麼
- `trace.weights`：最終的各層倍率

**復盤演算法（M8）**：
1. 用玩家當下的 `PlayerView` 重算 `Decision`。
2. 比較玩家選擇和 AI 首選的 `ActionTrace`，計算每個考量的 contribution 差。
3. 取差距最大的 2–3 項，用理由碼模板產生文字；同時列出生效的修正器（局勢說明）。
4. 換用其他 profile 重算，如果首選不同，就列為「若採用 ○○ 風格則會選擇……」。

輸出範例（目標）：
> 你切了 **5m**，建議切 **北**（差距 1.8 分）。
> - 牌效：切 5m 讓進張從 24 張降到 16 張（−0.9）
> - 防守：北對立直家是現物，5m 的放銃率約 11%（−1.1）
> - 局勢：南 3、第 2 位，和第 1 位相差 2000 點 → 防守權重 ×1.4

## 權重如何決定

1. **手動**：改 TOML，執行 `mj simulate` 跑 duplicate 對戰，看平均順位和信賴區間。
2. **消融（ablation）**：關掉某個考量再跑一次，確認每個模組都有正貢獻。
3. **自動**：`mj tune` 用 CMA-ES 搜尋權重和曲線控制點，目標函數是 duplicate 對戰的平均順位點數。
4. **動態**：實戰中由 Modifier 依局勢即時調整，而且調整本身也會記錄在 Trace 中，可以解釋。
