// 反事實實驗：在一個選擇點比較幾個選項。每個選項強制選定後，四家都依策略打完；
// 看不到的牌依決策者看得到的資訊重抽，各選項使用同一組世界（差值的雜訊較小）。

import { useEffect, useState } from "react";
import { call } from "./api";
import { TileText } from "./Tile";
import { KIND_NAMES, type ExperimentState, type ExperimentTarget } from "./types";

interface Props {
  target: ExperimentTarget;
  /** 世界數的預設值（對局中 1024；復盤時逐一查看，預設較少） */
  defaultWorlds?: number;
  onClose?: () => void;
}

const pct = (x: number) => `${Math.round(x * 1000) / 10}%`;

export function Experiment({ target, defaultWorlds = 64, onClose }: Props) {
  const [worlds, setWorlds] = useState(defaultWorlds);
  const [metric, setMetric] = useState<"points" | "placement">("points");
  const [chosen, setChosen] = useState<Set<number>>(() => new Set(target.options.map((_, i) => i)));
  const [state, setState] = useState<ExperimentState>({ state: "idle" });
  // 按下停止後到實驗真的結束之間（進行中的世界要打完）：按鈕停用，避免重複點擊。
  const [stopping, setStopping] = useState(false);
  useEffect(() => {
    if (state.state !== "running") setStopping(false);
  }, [state.state]);

  // 換了決策就重設選項；如果核心裡已有這個決策的結果，直接顯示。
  useEffect(() => {
    setChosen(new Set(target.options.map((_, i) => i)));
    call<ExperimentState>("experiment").then((s) => {
      const mine =
        s.state === "done" &&
        s.source === target.source &&
        (target.seat === undefined || s.seat === target.seat) &&
        s.ask === target.ask &&
        s.kind === target.kind &&
        s.reference === target.reference;
      setState(mine || s.state === "running" ? s : { state: "idle" });
    });
  }, [target.source, target.seat, target.ask, target.kind, target.options, target.reference]);

  useEffect(() => {
    if (state.state !== "running") return;
    const t = setInterval(() => {
      call<ExperimentState>("experiment").then(setState, (e) => setState({ state: "failed", error: String(e) }));
    }, 400);
    return () => clearInterval(t);
  }, [state.state]);

  const start = async () => {
    try {
      await call("start_experiment", {
        source: target.source,
        seat: target.seat,
        ask: target.ask,
        kind: target.kind,
        option_count: target.options.length,
        options: [...chosen],
        reference: target.reference,
        worlds,
        metric,
        strategy: target.strategy,
      });
      setState({ state: "running", done: 0, total: worlds, seconds: 0 });
    } catch (e) {
      setState({ state: "failed", error: String(e) });
    }
  };

  // 結果的格式依結果本身的衡量方式（設定可能已經改了）。
  const shown = state.state === "done" ? state.metric : metric;
  const fmt = (x: number) => (shown === "points" ? `${Math.round(x)}` : `${Math.round(x * 10) / 10}`);
  const ref = target.referenceLabel;
  /** 選項的標籤：比較基準、其他顧問、玩家。 */
  const badges = (i: number) => (
    <>
      {i === target.reference && <span className="badge">{ref}</span>}
      {target.marks
        .filter(([, o]) => o === i)
        .map(([name]) => (
          <span key={name} className="badge other">
            {name}
          </span>
        ))}
      {i === target.player && <span className="badge type">你</span>}
    </>
  );
  const toggle = (i: number) => {
    const n = new Set(chosen);
    if (n.has(i)) n.delete(i);
    else n.add(i);
    setChosen(n);
  };

  return (
    <div className="experiment">
      <div className="exp-head">
        <h3>
          實驗：【{KIND_NAMES[target.kind]}】
          <span className="muted">第 {target.ask} 次詢問</span>
        </h3>
        {onClose && (
          <button className="small" onClick={onClose}>
            關閉
          </button>
        )}
      </div>
      <p className="muted">
        每個選項強制選定後，四家都依{target.strategy ? "這個" : ""}策略打完（對手不論實際是誰，重打時都用策略；AI 的決策用基礎策略重打）；看不到的牌依你當時看得到的資訊重抽。
      </p>
      <div className="exp-options">
        {target.options.map((o, i) => (
          <label key={i} className={i === target.reference ? "strategy" : ""}>
            <input type="checkbox" checked={chosen.has(i) || i === target.reference} disabled={i === target.reference} onChange={() => toggle(i)} />
            <TileText text={o} />
            {badges(i)}
          </label>
        ))}
      </div>
      <div className="exp-controls">
        <label>
          世界數
          <select value={worlds} onChange={(e) => setWorlds(Number(e.target.value))}>
            <option value={32}>32（快）</option>
            <option value={64}>64</option>
            <option value={128}>128</option>
            <option value={256}>256</option>
            <option value={512}>512</option>
            <option value={1024}>1024（較準、較慢）</option>
          </select>
        </label>
        <label>
          衡量
          <select value={metric} onChange={(e) => setMetric(e.target.value as "points" | "placement")}>
            <option value="points">本局點數得失</option>
            <option value="placement">名次點（打到整場結束，較慢）</option>
          </select>
        </label>
        {state.state === "running" ? (
          <>
            <progress className={stopping ? "stopping" : ""} value={state.done} max={state.total} />
            <span className="muted">
              {state.done}／{state.total}　{Math.round(state.seconds)} 秒{stopping && "　停止中…"}
            </span>
            <button
              className={stopping ? "stopping" : ""}
              disabled={stopping}
              onClick={() => {
                setStopping(true);
                void call("cancel_experiment").catch(() => setStopping(false));
              }}
            >
              {stopping ? "停止中…" : "停止"}
            </button>
          </>
        ) : (
          <button className="primary" onClick={start}>
            開始實驗
          </button>
        )}
      </div>
      {state.state === "failed" && <div className="error">{state.error}</div>}
      {state.state === "done" && (
        <>
          <p className="muted">
            {state.worlds} 個世界{state.cancelled ? "（已停止）" : ""}，{Math.round(state.seconds * 10) / 10} 秒；
            差值 = 這個選項比{ref}的選擇多得的{state.metric === "points" ? "點" : "名次點"}（同一組世界的成對差，± 為標準誤）。
          </p>
          <table className="exp-results">
            <thead>
              <tr>
                <th>選項</th>
                <th>平均（{state.metric === "points" ? "點" : "名次點"}）</th>
                <th>與{ref}的差</th>
                <th>本局和了</th>
                <th>本局放銃</th>
                {state.metric === "placement" && <th>平均順位</th>}
              </tr>
            </thead>
            <tbody>
              {[...state.results]
                .sort((a, b) => b.mean - a.mean)
                .map((r) => {
                  const sig = Math.abs(r.diff) > 2 * r.diff_se && r.option !== state.reference;
                  return (
                    <tr key={r.option} className={r.option === state.reference ? "strategy" : ""}>
                      <td>
                        <TileText text={target.options[r.option]} />
                        {badges(r.option)}
                      </td>
                      <td className="num">
                        {fmt(r.mean)} <span className="muted">±{fmt(r.se)}</span>
                      </td>
                      <td className={`num ${sig ? (r.diff > 0 ? "plus" : "minus") : ""}`}>
                        {r.option === state.reference ? "—" : `${r.diff > 0 ? "+" : ""}${fmt(r.diff)} ±${fmt(r.diff_se)}`}
                      </td>
                      <td className="num">{pct(r.win_rate)}</td>
                      <td className="num">{pct(r.deal_in_rate)}</td>
                      {state.metric === "placement" && <td className="num">{r.avg_rank?.toFixed(2)}</td>}
                    </tr>
                  );
                })}
            </tbody>
          </table>
          <p className="muted">差值超過 2 倍標準誤時以顏色標示（綠 = 比{ref}好、紅 = 比{ref}差）。</p>
        </>
      )}
    </div>
  );
}
