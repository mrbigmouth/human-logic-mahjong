---
name: project-history
description: 查詢本專案 M0–M10 已完成里程碑的完成內容、驗收數據、當時的規劃原則與設計決策（含否決的做法與理由）。需要知道「某功能為什麼這樣設計」「某次評估的數字」「Mortal 比較或復盤的發現」「舊的里程碑定義」時使用。
---

# 專案歷史（M0–M10）

現行計畫在 `docs/04-milestones.md`（M11 起）。這裡保存已完成的部分，需要時再讀：

| 檔案 | 內容 |
|---|---|
| [decisions.md](decisions.md) | **先讀這個**：已定案的設計決策與理由，依主題整理（策略架構、評估方法、讀牌與印象操作、外部 AI、練習 App、規劃） |
| [milestones.md](milestones.md) | 從 docs/04 原文搬來的完整紀錄：原里程碑表、當時的規劃原則、M3／M9／M10 細項、M1–M10 完成內容與驗收數據、M10 完成時的「下一步」 |

## 各里程碑一行摘要（在 milestones.md 中搜尋「## Mx 完成內容」）

| # | 摘要 |
|---|---|
| M0–M1 | 骨架；規則引擎（點數、向聽與 Python `mahjong` 零差異，20 萬場模糊測試） |
| M2 | 第一個牌效 AI（後來重構） |
| M2.5 | 改為 if／else 決策函式、`mj-tools`、`strategy-list`、兩種評估 |
| M3 | 快照與重抽（去除全知）、名次點、`mj simulate`（duplicate）、常駐預測 |
| M4 | 天鳳牌譜匯入與零差異重播；真人牌譜上的預測評估 |
| M5 | 押引期望值、`predict::win`、立直或默聽、鳴牌時機（`push-fold`） |
| M6 | 讀牌（聽牌、聽牌張、打點）以牌譜校準（`hand-reading`） |
| M7 | 復盤 `mj review`：成對差值、失誤分類；鳳凰卓玩家的發現 |
| M8 | 可調參數與 `mj tune`；mjai 介接；Mortal 對戰與一致率 |
| M9 | 練習 App：`mj-app`、`mj serve`、React 前端、Tauri 三平台 |
| M10 | 印象操作（手切摸切、迷彩）有效 +0.24 名次點；反詐騙無效 |

在 milestones.md 裡，連到 `docs/` 的連結已改成從本目錄出發的相對路徑。
