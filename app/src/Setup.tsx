// 開局設定：座位、三家對手、提示與復盤的顧問（1–3 個策略或 AI）、半莊或單局、seed。
// 策略與 AI 的選項標上最後更新時間，由新至舊排列。

import { useEffect, useState } from "react";
import { AdvisorPicker, optionLabel } from "./Advisors";
import { call } from "./api";
import type { LogInfo, NewGame, Seat, SessionConfig, StrategyInfo } from "./types";

const DEFAULT = "tenhou-normal-4p-hanchan/hand-reading";

function load<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v ? (JSON.parse(v) as T) : fallback;
  } catch {
    return fallback;
  }
}

interface Props {
  onStart: (g: NewGame) => void;
  /** 讀入牌譜檔之後 */
  onLoaded: (info: LogInfo) => void;
  /** 對戰測試頁 */
  onTests: () => void;
  /** 策略參數頁 */
  onParams: () => void;
}

export function Setup({ onStart, onLoaded, onTests, onParams }: Props) {
  const [entrants, setEntrants] = useState<StrategyInfo[]>([]);
  const [opponents, setOpponents] = useState<[string, string, string]>(() =>
    load("opponents", [DEFAULT, DEFAULT, DEFAULT]),
  );
  const [advisors, setAdvisors] = useState<string[]>(() => load("advisors", [load("strategy", DEFAULT)]));
  const [seat, setSeat] = useState<number>(() => load("seat", -1));
  const [length, setLength] = useState<"hanchan" | "one">(() => load("length", "hanchan"));
  const [seed, setSeed] = useState("");
  // seed 上限 2^53 − 1（JavaScript 能精確表示的最大整數，16 位數）。
  const seedTooLarge = seed !== "" && !Number.isSafeInteger(Number(seed));
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [loading, setLoading] = useState(false);

  /** 讀入牌譜檔（天鳳 mjlog XML／JSON、mjai）：在核心中重播，成功後切到牌譜畫面。 */
  const loadFile = async (file: File) => {
    setLoading(true);
    setError(null);
    try {
      const text = await file.text();
      onLoaded(await call<LogInfo>("load_log", { text, name: file.name }));
    } catch (e) {
      setError(`無法讀入 ${file.name}：${String(e)}`);
    } finally {
      setLoading(false);
    }
  };

  // 上次沒打完的對局（服務重啟、關掉頁面）：可以接續。
  const [saved, setSaved] = useState<{ seed: number; asks: number } | null>(null);
  useEffect(() => {
    call<StrategyInfo[]>("entrants").then(setEntrants, (e) => setError(String(e)));
    call<{ seed: number; asks: number } | null>("saved_game").then(setSaved, () => setSaved(null));
  }, []);
  const resume = async () => {
    try {
      onStart(await call<NewGame>("resume_game"));
    } catch (e) {
      setError(String(e));
    }
  };

  const start = async () => {
    setStarting(true);
    setError(null);
    try {
      localStorage.setItem("opponents", JSON.stringify(opponents));
      localStorage.setItem("advisors", JSON.stringify(validAdvisors));
      localStorage.setItem("seat", JSON.stringify(seat));
      localStorage.setItem("length", JSON.stringify(length));
    } catch {
      // 無法儲存設定時忽略
    }
    const cfg: SessionConfig = {
      seat: (seat < 0 ? Math.floor(Math.random() * 4) : seat) as Seat,
      opponents,
      advisors: validAdvisors,
      max_hands: length === "one" ? 1 : null,
      seed: seed.trim() === "" ? null : Number(seed),
    };
    try {
      onStart(await call<NewGame>("new_game", cfg));
    } catch (e) {
      setError(String(e));
      setStarting(false);
    }
  };

  const choices = entrants;
  const advisorChoices = entrants.filter((c) => c.kind !== "bot");
  const describe = (name: string) => choices.find((c) => c.name === name);
  // 不再存在的顧問（例如 Mortal 沒裝）不送出。
  const validAdvisors = advisors.filter((a) => advisorChoices.some((c) => c.name === a));

  return (
    <div className="setup">
      <h1>Human Logic Mahjong</h1>
      <p className="lead">與可解釋的 AI 對戰：每一步都能看到策略的選擇與理由，打完可以復盤。</p>
      <div className="form">
        <label>
          你的座位
          <select value={seat} onChange={(e) => setSeat(Number(e.target.value))}>
            <option value={-1}>隨機</option>
            <option value={0}>東家（起家）</option>
            <option value={1}>南家</option>
            <option value={2}>西家</option>
            <option value={3}>北家</option>
          </select>
        </label>
        <label>
          長度
          <select value={length} onChange={(e) => setLength(e.target.value as "hanchan" | "one")}>
            <option value="hanchan">半莊</option>
            <option value="one">一局（練習）</option>
          </select>
        </label>
        {(["下家", "對家", "上家"] as const).map((label, i) => (
          <label key={label}>
            {label}
            <select
              value={opponents[i]}
              onChange={(e) => {
                const o = [...opponents] as [string, string, string];
                o[i] = e.target.value;
                setOpponents(o);
              }}
            >
              {choices.map((c) => (
                <option key={c.name} value={c.name}>
                  {optionLabel(c)}
                </option>
              ))}
            </select>
            <span className="desc">{describe(opponents[i])?.summary}</span>
          </label>
        ))}
        <div className="label">
          提示與復盤的策略（最多 3 個，第一個是主要的）
          <AdvisorPicker choices={advisorChoices} value={validAdvisors} onChange={setAdvisors} />
          <span className="desc">
            對局中同時列出每個的選擇；復盤以第一個為比較基準。
            {validAdvisors.some((a) => describe(a)?.kind === "mjai") && " Mortal 不會說明理由，只列出各動作的機率。"}
          </span>
        </div>
        <label>
          seed（空白 = 隨機）
          <input value={seed} onChange={(e) => setSeed(e.target.value.replace(/[^0-9]/g, "").slice(0, 16))} placeholder="隨機" />
          {seedTooLarge && <span className="desc error-text">seed 最大 9007199254740991</span>}
        </label>
      </div>
      {error && <div className="error">{error}</div>}
      <div className="buttons">
        <button className="primary big" onClick={start} disabled={starting || validAdvisors.length === 0 || seedTooLarge}>
          開始對局
        </button>
        {saved && (
          <button className="big" onClick={resume} title="照保存的動作重打到上次的詢問，接著繼續">
            繼續上次的對局（seed {saved.seed}）
          </button>
        )}
        <label className={`button big ${loading ? "disabled" : ""}`} title="天鳳牌譜（mjlog XML、JSON）或 mjai 牌譜（本 App 儲存的 .mjson）">
          {loading ? "讀入中…" : "讀入牌譜…"}
          <input
            type="file"
            accept=".mjson,.json,.jsonl,.xml,.mjlog,.txt"
            hidden
            disabled={loading}
            onChange={(e) => {
              const f = e.target.files?.[0];
              e.target.value = "";
              if (f) void loadFile(f);
            }}
          />
        </label>
        <button className="big" onClick={onTests} title="策略、機器人、Mortal 大量對戰：勝率、每場的細節、實驗">
          對戰測試…
        </button>
        <button className="big" onClick={onParams} title="調整策略的可調參數，存成參數版本（不必編譯）">
          策略參數…
        </button>
      </div>
    </div>
  );
}
