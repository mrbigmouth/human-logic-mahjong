# 讀牌押引

以天鳳鳳凰卓牌譜校準的讀牌（誰聽牌、聽什麼、多大），加上 M5 的押引期望值：立直者與看起來已聽牌的副露者、默聽者都會提防，每張牌以「和牌機率 × 我的打點 − 放銃機率 × 對手打點」決定對攻、回し打ち或棄和。

目前的主力策略（M6 起），持續改進中。

## 特色

- **讀牌：聽牌**（`utils/predict/tenpai.rs`）：依副露組數與巡目查基準聽牌率，再依牌河線索調整勝算：
  最近 3 張手切越多越不像聽牌、最後手切中張較像聽牌、最後手切字牌較不像、連續摸切較像、有別人立直時較不像（沒聽牌的人在防守）。
- **讀牌：聽牌張**（`utils/predict/waits.rs`）：依牌的位置（1·9／2·8／3·7／4·5·6）查基準值，
  再看筋、片筋、No Chance、One Chance、寶牌與寶牌旁、他捨牌中這門的比例（染手）、最後手切的牌附近、立直宣言牌的筋、巡目；字牌看已見張數與是否為他的役牌。
- **讀牌：打點**（`utils/predict/hand_value.rs`）：立直者依立直巡目、副露者依副露中的寶牌與碰的組數、門前者依場上看不到的寶牌數。
- **押引**：有威脅時（立直者，或聽牌機率 ≥ 50% 的沒立直者），每張候選牌算期望值；威脅的份量以聽牌機率計。
- **印象操作**（M10，`utils/image.rs`）：用自己的讀牌模型看自己（「別人眼中的我」）。
  - 手切摸切（`decision/tedashi.rs`）：打的牌和剛摸到的牌同種時，聽牌或一向聽選看起來較不像聽牌的打法，離聽牌還遠時選較像聽牌的。
  - 迷彩（`decision/discard.rs`）：聽牌的打法中，聽牌張數相近、點數不比較低時，選聽牌張在別人眼中較安全的（筋、不在宣言牌附近）。
  - 反詐騙（`memory.rs`、`predict/tenpai.rs`）：每回合記下對各家的聽牌估計，荒牌流局時與公開的聽牌狀態對照，常藏聽牌的人提高估計、常虛張聲勢的人降低。
- **立直或默聽、鳴牌、牌效**：架構與 `push-fold` 相同（無役立直、默聽跳滿左右就默聽；有人立直時只有鳴了就聽牌才鳴；進張差不多時保留寶牌）；
  M8 以 duplicate 調整門檻：非役牌的鳴牌除非鳴了就聽牌，否則要 3 張寶牌；默聽門檻 8000 → 9000 點；牌山剩不到 5 張才改默聽。

## 校準與強度（M6 完成時）

- 預測以天鳳牌譜校準（2026-10-06 的 434 場擬合，2026-10-07–08 的牌譜驗證），驗證資料上的技巧分數：
  聽牌 +0.61（`push-fold` +0.42；未立直者 +0.36，原本 +0.06）、聽牌張 +0.05（原本 +0.03）、打點（滿貫以上）+0.05；校準表貼近對角線。
- 自我對戰中與 `push-fold` 強度相同（duplicate 3000 組：平均順位差 +0.012 ± 0.020），放銃率 12.6% → 11.5%、和了率 20.9% → 20.1%。
  對手是機器人，打法與真人不同；讀牌對真人對局的效果要在 M7（牌譜復盤）中衡量。

## 已知弱點

- 沒有人立直時的何切只看牌效，不考慮打點（反事實損失最大的部分）。
- 「本局和牌機率」在序盤偏低（預測 10%，實際 20%）。
- 立直率比真人高。

## 主要參數

單一數值的門檻與倍數都是可調參數（`Param`），`mj params tenhou-normal-4p-hanchan/hand-reading` 可以列出目前值、範圍與說明，
任何指令都能用 `策略@名稱=值` 試別的值。查表與一起擬合的倍數維持常數。

| 檔案 | 參數 |
|---|---|
| `utils/predict/tenpai.rs` | `BASE`（副露 × 巡目的基準聽牌率，常數）；`tenpai.*`（各線索的勝算倍數） |
| `utils/predict/waits.rs` | `RIICHI_BASE`、`OPEN_BASE`、`RIICHI`／`OPEN`（各線索的勝算倍數）、字牌表（常數）；`waits.suji_trap` |
| `utils/predict/hand_value.rs` | `RIICHI_MANGAN`、`OPEN_POINTS`、`OPEN_MANGAN`、`CLOSED_MANGAN`（常數）；`hand_value.*` |
| `utils/predict/win.rs` | `win.*`（預測用，以牌譜校準）、`push.*`（押引用，以對戰調整），見 `Model` |
| `utils/defense.rs` | `DEFEND_AGAINST_UNDECLARED`；`defense.undeclared_threat_tenpai` |
| `decision/discard.rs` | `discard.deny_value`、`discard.win_weight`；`STICK_VALUE` |
| `decision/riichi.rs` | `riichi.dama_points`、`riichi.late_wall`、`riichi.min_waits` |
| `decision/call.rs` | `call.far_shanten`、`call.min_dora_for_far_call` |
| `utils/efficiency.rs` | `efficiency.dora_discount` |
| `decision/tedashi.rs` | `deceive.tedashi`（開關）、`deceive.bluff_shanten` |
| `decision/discard.rs`（迷彩） | `deceive.meisai`（開關）、`deceive.meisai_min_ukeire`、`deceive.meisai_danger_ratio` |
| `utils/predict/tenpai.rs`（反詐騙） | `antideceive.calibrate`（開關）、`antideceive.prior` |
