# 押引期望值

牌效為主；有人立直時，每張牌以「和牌機率 × 我的打點 − 放銃機率 × 對手打點」決定對攻、回し打ち或棄和。無役就立直、默聽已有滿貫就默聽；不做又遠又便宜的鳴牌；進張差不多時保留寶牌。

目前的主力策略，持續改進中（M5 起）。

## 特色

- **何切**
  - 沒有人立直：照牌效打（向聽數最低 → 進張最多 → 保留寶牌 → 先打字牌、老頭牌）。
  - 有人立直：押引。每張候選牌估計「打了之後我和牌的機率」、「我和牌的價值」（含供託與阻止對手和牌）與「放銃機率 × 對手打點」，選分數最高的一張。
    理由會寫出數字，例如「棄和：3s，和牌約 4% × 9800 點，放銃約 0.0%」。
- **立直或默聽**：聽牌張已一張不剩 → 默聽；沒有役 → 立直；默聽已有滿貫、牌山快沒了、或最終局第一名 → 默聽；聽牌張太少 → 默聽等改聽；其餘立直。
- **鳴牌**：鳴了要有役、向聽數要前進；有人立直時只有鳴了就聽牌才鳴；不是役牌、鳴了還要 2 向聽以上、寶牌不到 2 張 → 不鳴。
- **讀牌**：對手的聽牌機率（巡目、副露數、中巡起連續摸切）與聽牌張（現物、筋、No Chance）；本局自己和牌的機率（`predict::win`）。

## 強度（M5 完成時）

- 同牌山 duplicate 對 `efficiency-fold`（5000 組）：平均順位差 −0.130 ± 0.018、名次點 +3.55 ± 0.59。
- 對貪婪機器人：平均順位差 −0.934 ± 0.025。
- 四家自我對戰：和了率 20.8%、放銃率 12.5%、立直率 25.1%（天鳳鳳凰卓真人：21.0%、12.2%、17.0%）。

## 已知弱點

- 只提防立直者，不提防副露者（副露者的聽牌預測還不準，開啟反而變弱；`utils/defense.rs` 的 `DEFEND_AGAINST_OPEN_HANDS`）。
- 沒有人立直時的何切只看牌效，不考慮打點。
- 立直率比真人高。

## 主要參數

| 檔案 | 參數 |
|---|---|
| `decision/discard.rs` | `DENY_VALUE`、`STICK_VALUE` |
| `decision/riichi.rs` | `DAMA_POINTS`、`LATE_WALL`、`MIN_WAITS_FOR_RIICHI` |
| `decision/call.rs` | `FAR_SHANTEN`、`MIN_DORA_FOR_FAR_CALL` |
| `utils/defense.rs` | `DEFEND_AGAINST_OPEN_HANDS`、`RIICHI_VALUE`、`OPEN_VALUE` |
| `utils/predict/win.rs` | `ASSUMED_WAITS`、`LATER_UKEIRE`、`RON_CHANCES`、`OTHERS_WIN_PER_TURN` |
| `utils/efficiency.rs` | `DORA_DISCOUNT` |
