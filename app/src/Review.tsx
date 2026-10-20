// 復盤：對讀入的牌譜（剛打完的對局也先讀入）的一家，在背景做反事實評估，
// 列出與策略選擇不同的決策、理由、期望值差與失誤統計。點一個決策可以看當時的牌桌，或跳到牌譜檢視器。

import { useEffect, useState } from "react";
import { AdvisorPicker, PolicyList } from "./Advisors";
import { call } from "./api";
import { Board, shortName } from "./Board";
import { Experiment } from "./Experiment";
import { Hand } from "./Game";
import { TileText } from "./Tile";
import {
  KIND_NAMES,
  MISTAKE_Z,
  type Frame,
  type GroupSummary,
  type LogFrames,
  type LogInfo,
  type ReviewConfig,
  type ReviewState,
  type Reviewed,
  type Seat,
  type StrategyInfo,
} from "./types";

const DEFAULT = "tenhou-normal-4p-hanchan/hand-reading";

interface Props {
  info: LogInfo;
  seat: Seat;
  /** 在牌譜檢視器中打開第幾幀 */
  onView: (frame: number) => void;
  onQuit: () => void;
}

function fmt(x: number, unit: string): string {
  const v = unit === "點" ? Math.round(x) : Math.round(x * 10) / 10;
  return `${v > 0 ? "+" : ""}${v} ${unit}`;
}

function verdict(d: Reviewed): "mistake" | "better" | "diff" | "same" {
  if (d.strategy === d.player) return "same";
  const e = d.eval;
  if (!e) return "diff";
  if (e.diff > 0 && e.diff > MISTAKE_Z * e.diff_se) return "mistake";
  if (e.diff < 0 && -e.diff > MISTAKE_Z * e.diff_se) return "better";
  return "diff";
}

const VERDICT_NAMES = { mistake: "明顯失誤", better: "你較好", diff: "不同", same: "相同" };

function SummaryTable({ rows, unit }: { rows: GroupSummary[]; unit: string }) {
  return (
    <table className="summary">
      <thead>
        <tr>
          <th></th>
          <th>決策</th>
          <th>一致率</th>
          <th>評估過</th>
          <th>平均損失</th>
          <th>明顯失誤</th>
          <th>你較好</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.label}>
            <td>{KIND_NAMES[r.label as keyof typeof KIND_NAMES] ?? r.label}</td>
            <td>{r.decisions}</td>
            <td>{Math.round(r.agree * 100)}%</td>
            <td>{r.evaluated}</td>
            <td>{r.evaluated > 0 ? `${fmt(r.mean_diff, unit)} ±${fmt(r.mean_diff_se, unit).replace("+", "")}` : "—"}</td>
            <td>{r.mistakes}</td>
            <td>{r.player_better}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function Review({ info, seat, onView, onQuit }: Props) {
  const [config, setConfig] = useState<ReviewConfig>({ rollouts: 32, metric: "points" });
  // 顧問：牌譜記載的（本 App 的對局），否則預設策略。
  const [advisors, setAdvisors] = useState<string[]>(() => {
    const m = info.meta;
    if (m?.advisors && m.advisors.length > 0) return m.advisors;
    if (m?.strategy) return [m.strategy];
    // 對戰測試的牌譜：玩家名稱就是參賽者（策略或 Mortal），預設和它自己比較（看理由）。
    const own = info.names[seat];
    if (own.includes("/") || own.startsWith("mjai:")) return [own];
    return [DEFAULT];
  });
  const [choices, setChoices] = useState<StrategyInfo[]>([]);
  useEffect(() => {
    call<StrategyInfo[]>("entrants").then((e) => setChoices(e.filter((c) => c.kind !== "bot")));
  }, []);
  const [state, setState] = useState<ReviewState>({ state: "idle" });
  const [frames, setFrames] = useState<Frame[]>([]);
  const [askFrames, setAskFrames] = useState(new Map<number, number>());

  useEffect(() => {
    call<LogFrames>("log_frames", { seat, open: false }).then(
      (f) => {
        setFrames(f.frames);
        setAskFrames(new Map(f.asks));
      },
      (e) => setState({ state: "failed", error: String(e) }),
    );
    // 已經復盤過（例如從檢視器回來）就直接顯示結果。
    call<ReviewState>("log_review", { seat }).then((r) => {
      if (r.state !== "idle") setState(r);
    });
  }, [seat]);
  const [filter, setFilter] = useState<"diff" | "all">("diff");
  const [selected, setSelected] = useState<Reviewed | null>(null);
  const [experimenting, setExperimenting] = useState(false);

  useEffect(() => {
    if (state.state !== "running") return;
    const t = setInterval(() => {
      call<ReviewState>("log_review", { seat }).then(setState, (e) => setState({ state: "failed", error: String(e) }));
    }, 1000);
    return () => clearInterval(t);
  }, [state.state, seat]);

  const start = async () => {
    try {
      await call("start_log_review", { seat, advisors, ...config });
      setState({ state: "running", seconds: 0 });
    } catch (e) {
      setState({ state: "failed", error: String(e) });
    }
  };

  const unit = config.metric === "points" ? "點" : "名次點";

  if (state.state === "idle" || state.state === "failed") {
    return (
      <div className="review setup">
        <h1>復盤：{shortName(info.names[seat])}</h1>
        <p className="lead">
          逐手比較你與策略（或 AI）的選擇。與第一個選擇不同的地方，會從當時的局面依你看得到的資訊重抽對手手牌與牌山，
          兩個選項各重打很多次比較結果（重打一律用策略）。
        </p>
        <div className="form">
          <div className="label">
            比較的策略（最多 3 個，第一個是基準）
            <AdvisorPicker choices={choices} value={advisors} onChange={setAdvisors} />
          </div>
          <label>
            重打次數（每個不同的決策）
            <select value={config.rollouts} onChange={(e) => setConfig({ ...config, rollouts: Number(e.target.value) })}>
              <option value={0}>不評估（只比較選擇，最快）</option>
              <option value={16}>16（快）</option>
              <option value={32}>32</option>
              <option value={64}>64（較準）</option>
            </select>
          </label>
          <label>
            衡量方式
            <select
              value={config.metric}
              onChange={(e) => setConfig({ ...config, metric: e.target.value as "points" | "placement" })}
            >
              <option value="points">這一局的點數得失（較快、雜訊小）</option>
              <option value="placement">名次點（打到整場結束，較慢）</option>
            </select>
          </label>
        </div>
        {state.state === "failed" && <div className="error">{state.error}</div>}
        <div className="buttons">
          <button className="primary big" onClick={start} disabled={advisors.length === 0}>
            開始復盤
          </button>
          <button onClick={onQuit}>返回</button>
        </div>
      </div>
    );
  }

  if (state.state === "running") {
    return (
      <div className="review setup">
        <h1>復盤中…</h1>
        <p className="lead">已經過 {Math.round(state.seconds)} 秒。選擇不同的決策越多、重打次數越多，需要越久。</p>
      </div>
    );
  }

  const { review, summary } = state;
  const advs = state.advisors;
  const ref = advs[0]?.title ?? "策略";
  const base = advs.find((a) => !a.ai);
  /** 各顧問在這個決策的選擇：[名稱, 選項]。 */
  const picksOf = (d: Reviewed): [string, number][] =>
    advs.flatMap((a, k) => {
      const p = d.picks[k];
      return p !== null && p !== undefined ? [[a.title, p] as [string, number]] : [];
    });
  const decisions = review.decisions.filter((d) => filter === "all" || d.strategy !== d.player);
  const frameOf = (d: Reviewed) => askFrames.get(d.act_index + 1);
  const selFrame = selected ? frameOf(selected) : undefined;
  const table = selFrame !== undefined ? frames[Math.max(0, selFrame - 1)]?.table : undefined;

  return (
    <div className="review">
      <div className="review-list">
        <div className="review-head">
          <h1>復盤：{shortName(info.names[seat])}</h1>
          <div className="buttons">
            <label>
              <input type="checkbox" checked={filter === "all"} onChange={(e) => setFilter(e.target.checked ? "all" : "diff")} />
              顯示所有決策
            </label>
            <button onClick={onQuit}>返回</button>
          </div>
        </div>
        <p className="muted">
          {review.decisions.length} 個決策，與{ref}一致 {Math.round(summary.overall.agree * 100)}%；
          平均損失＝{ref}的選擇比你的選擇多得的{unit}（正 = {ref}較好）。花費 {Math.round(state.seconds)} 秒。
        </p>
        <SummaryTable rows={[{ ...summary.overall, label: "全部" }, ...summary.by_kind]} unit={unit} />
        {summary.by_type.length > 0 && (
          <>
            <h3>差異類型</h3>
            <SummaryTable rows={summary.by_type} unit={unit} />
          </>
        )}
        <h3>逐手</h3>
        <div className="decisions">
          {decisions.map((d, i) => {
            const v = verdict(d);
            return (
              <div
                key={i}
                className={`review-item ${v} ${selected === d ? "selected" : ""}`}
                onClick={() => {
                  setSelected(d);
                  setExperimenting(false);
                }}
              >
                <div className="where">
                  {d.round}　第 {d.turn} 巡　【{KIND_NAMES[d.kind]}】
                  <span className={`badge ${v}`}>{VERDICT_NAMES[v]}</span>
                  {d.difference && <span className="badge type">{d.difference}</span>}
                </div>
                <div className="choices">
                  你：<TileText text={d.options[d.player]} />
                  {advs.length === 0 && (
                    <>
                      　策略：<TileText text={d.options[d.strategy]} />
                    </>
                  )}
                  {picksOf(d).map(([name, p]) => (
                    <span key={name}>
                      {"　"}
                      {name}：<TileText text={d.options[p]} />
                    </span>
                  ))}
                  {d.ai !== null && (
                    <>
                      　AI：<TileText text={d.options[d.ai]} />
                    </>
                  )}
                  {d.eval && d.strategy !== d.player && (
                    <span className="ev">
                      　差 {fmt(d.eval.diff, unit)}（±{fmt(d.eval.diff_se, unit).replace("+", "")}）
                    </span>
                  )}
                </div>
              </div>
            );
          })}
          {decisions.length === 0 && <p className="muted">每個決策都與策略相同。</p>}
        </div>
      </div>
      <div className="review-detail">
        {selected ? (
          <>
            {table && (
              <div className="mini">
                <Board table={table} names={info.names} />
                <Hand table={table} ask={null} riichiMode={false} setRiichiMode={() => {}} hint={null} onAct={() => {}} />
              </div>
            )}
            <div className="panel">
              <h3>
                {selected.round}　第 {selected.turn} 巡　【{KIND_NAMES[selected.kind]}】
                {selFrame !== undefined && (
                  <button className="small" onClick={() => onView(Math.max(0, selFrame - 1))}>
                    在牌譜中檢視
                  </button>
                )}
              </h3>
              <table className="options">
                <thead>
                  <tr>
                    <th>選項</th>
                    <th></th>
                    {selected.eval && <th>平均結果</th>}
                  </tr>
                </thead>
                <tbody>
                  {selected.options.map((o, i) => {
                    const tags = [
                      i === selected.player ? "你" : null,
                      ...(advs.length === 0 ? [i === selected.strategy ? "策略" : null] : []),
                      ...picksOf(selected).map(([name, p]) => (p === i ? name : null)),
                      i === selected.ai ? "AI" : null,
                    ].filter(Boolean);
                    const ev = selected.eval?.option_ev[i] ?? null;
                    if (tags.length === 0 && ev === null) return null;
                    return (
                      <tr key={i}>
                        <td>
                          <TileText text={o} />
                        </td>
                        <td>{tags.join("、")}</td>
                        {selected.eval && <td>{ev !== null ? fmt(ev, unit) : ""}</td>}
                      </tr>
                    );
                  })}
                </tbody>
              </table>
              {!experimenting && (
                <button onClick={() => setExperimenting(true)} title="比較各選項之後依策略打的結果">
                  實驗：比較這個決策的選項
                </button>
              )}
              {advs.map((a, k) =>
                a.ai ? (
                  <div key={a.name}>
                    <h4>{a.title} 的機率</h4>
                    <PolicyList policy={selected.policies[k] ?? null} />
                  </div>
                ) : null,
              )}
              {selected.notes.length > 0 && (
                <>
                  <h4>{base?.title ?? "策略"}的理由</h4>
                  <ul>
                    {selected.notes.map((n, j) => (
                      <li key={j}>
                        <TileText text={n} />
                      </li>
                    ))}
                  </ul>
                </>
              )}
            </div>
            {experimenting && (
              <div className="panel">
                <Experiment
                  target={{
                    source: "log",
                    seat,
                    ask: selected.act_index + 1,
                    kind: selected.kind,
                    options: selected.options,
                    reference: selected.strategy,
                    referenceLabel: ref,
                    marks: picksOf(selected).slice(advs.length > 0 ? 1 : 0),
                    strategy: advs[0] && !advs[0].ai ? advs[0].name : undefined,
                    player: selected.player,
                  }}
                  onClose={() => setExperimenting(false)}
                />
              </div>
            )}
          </>
        ) : (
          <p className="muted">點選左邊的決策，查看當時的牌桌與策略的理由（或 AI 的機率）。</p>
        )}
      </div>
    </div>
  );
}
