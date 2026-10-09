# Mortal（mjai 機器人）

[Mortal](https://github.com/Equim-chan/Mortal) 是以深度強化學習訓練的日麻 AI，本專案拿它當比較基準（M8）：
對戰（`mj simulate`）、一致率（`mj agree`）、復盤時列出它的選擇（`mj review --mjai`）。

## 授權隔離

Mortal 的程式碼是 AGPL-3.0-or-later。本目錄的 `mortal_bot.py`、`run.sh`、`setup.sh` 會載入或下載它，
所以同樣以 AGPL-3.0-or-later 授權。專案的其他部分只透過 mjai 協定（子行程的標準輸入／輸出）和它溝通，
不連結、不散布它的程式碼與權重；Mortal 原始碼與權重都下載到 `data/mortal/`（已 gitignore）。

## 權重

官方沒有公開權重，使用社群的兩份（M8 時選定）：

| 名稱 | 來源 | 版本 | 權重授權 | 說明 |
|---|---|---|---|---|
| `s42` | [banxiaweiliang/Mortal-S42](https://huggingface.co/banxiaweiliang/Mortal-S42)（2026-10） | v4 | Apache-2.0 | 天鳳鳳凰約 110 萬場＋雀魂高段位微調；與社群的 `model_v4_20240308` 對戰點估計略好、不顯著 |
| `hhim8826` | [hhim8826/mortal3-b40c192](https://huggingface.co/hhim8826/mortal3-b40c192)（2023 訓練、2026-09 上傳） | v3 | AGPL-3.0 | 天鳳鳳凰 19 萬場；只和同一批檢查點比較過，絕對強度不明 |

`setup.sh` 會檢查兩者的 SHA-256（與各模型卡相同）。

## 安裝

```sh
pipx install uv          # 若還沒有
tools/mortal/setup.sh    # Python 3.12 venv＋CPU 版 PyTorch、Mortal 原始碼（固定版本）與 libriichi、權重
```

約占 1.5 GB（venv 約 0.7 GB、Mortal 編譯產物、權重 90 MB）。venv 與 `libriichi.so` 依 CPU 架構而定，
換架構後執行 `tools/mortal/setup.sh --rebuild`。

## 使用

```sh
mj simulate tenhou-normal-4p-hanchan/hand-reading "mjai:tools/mortal/run.sh s42" --blocks 150   # 對戰（2 對 2）
mj agree data/tenhou --mjai "tools/mortal/run.sh s42"                                           # 一致率（牌譜）
mj agree --selfplay 100 --mjai "tools/mortal/run.sh s42"                                        # 一致率（策略自我對戰的局面）
mj review <牌譜> --player <名稱> --mjai "tools/mortal/run.sh s42" --show ai                       # 復盤附 AI 的選擇
```

- 每個 AI 座位是一個行程（約 380 MB），行程在對局之間重複使用；用到 mjai AI 時，`--jobs`（同時進行的對局數）預設依可用記憶體決定。
- `run.sh` 的選項：`--threads N`（PyTorch 執行緒，預設 1）、`--q-values`（回應附上各動作的 Q 值，較慢）。
- 擴充請求 `mj_peek`：旁觀模式（`mj agree`、`mj review`）詢問立直時要打的牌而不改變 AI 的局面，見 `mortal_bot.py`。
