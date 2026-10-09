// 開局設定：座位、三家對手、提示用的策略、半莊或單局、seed。

import { useEffect, useState } from "react";
import { call } from "./api";
import type { NewGame, Seat, SessionConfig, StrategyInfo } from "./types";

const DEFAULT = "tenhou-normal-4p-hanchan/hand-reading";

function load<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v ? (JSON.parse(v) as T) : fallback;
  } catch {
    return fallback;
  }
}

export function Setup({ onStart }: { onStart: (g: NewGame) => void }) {
  const [strategies, setStrategies] = useState<StrategyInfo[]>([]);
  const [entrants, setEntrants] = useState<StrategyInfo[]>([]);
  const [opponents, setOpponents] = useState<[string, string, string]>(() =>
    load("opponents", [DEFAULT, DEFAULT, DEFAULT]),
  );
  const [strategy, setStrategy] = useState(() => load("strategy", DEFAULT));
  const [seat, setSeat] = useState<number>(() => load("seat", -1));
  const [length, setLength] = useState<"hanchan" | "one">(() => load("length", "hanchan"));
  const [seed, setSeed] = useState("");
  // seed 上限 2^53 − 1（JavaScript 能精確表示的最大整數，16 位數）。
  const seedTooLarge = seed !== "" && !Number.isSafeInteger(Number(seed));
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);

  useEffect(() => {
    call<StrategyInfo[]>("strategies").then(setStrategies, (e) => setError(String(e)));
    call<StrategyInfo[]>("entrants").then(setEntrants, (e) => setError(String(e)));
  }, []);

  const start = async () => {
    setStarting(true);
    setError(null);
    try {
      localStorage.setItem("opponents", JSON.stringify(opponents));
      localStorage.setItem("strategy", JSON.stringify(strategy));
      localStorage.setItem("seat", JSON.stringify(seat));
      localStorage.setItem("length", JSON.stringify(length));
    } catch {
      // 無法儲存設定時忽略
    }
    const cfg: SessionConfig = {
      seat: (seat < 0 ? Math.floor(Math.random() * 4) : seat) as Seat,
      opponents,
      strategy,
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
  const describe = (name: string) => choices.find((c) => c.name === name);

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
                  {c.title}
                  {c.name.includes("/") ? `（${c.name.split("/").pop()}）` : ""}
                </option>
              ))}
            </select>
            <span className="desc">{describe(opponents[i])?.summary}</span>
          </label>
        ))}
        <label>
          提示與復盤的策略
          <select value={strategy} onChange={(e) => setStrategy(e.target.value)}>
            {strategies.map((c) => (
              <option key={c.name} value={c.name}>
                {c.title}（{c.name}）
              </option>
            ))}
          </select>
          <span className="desc">{describe(strategy)?.summary}</span>
        </label>
        <label>
          seed（空白 = 隨機）
          <input value={seed} onChange={(e) => setSeed(e.target.value.replace(/[^0-9]/g, "").slice(0, 16))} placeholder="隨機" />
          {seedTooLarge && <span className="desc error-text">seed 最大 9007199254740991</span>}
        </label>
      </div>
      {error && <div className="error">{error}</div>}
      <button className="primary big" onClick={start} disabled={starting || strategies.length === 0 || seedTooLarge}>
        開始對局
      </button>
    </div>
  );
}
