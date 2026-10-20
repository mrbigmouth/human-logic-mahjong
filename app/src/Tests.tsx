// 對戰測試平台（M15）：不用終端機就能跑自我對戰、duplicate、場地對戰、預測評估。
// 結果存在核心的資料目錄；點開任一場會用同樣的 seed 與座位配置重打（Mortal 的對局讀保存的牌譜），
// 之後就是一般的牌譜：逐打檢視、復盤、反事實實驗。

import { useCallback, useEffect, useState } from "react";
import { formatUpdated, optionLabel } from "./Advisors";
import { call } from "./api";
import { shortName } from "./Board";
import type {
  EntrantResult,
  GamePage,
  LogInfo,
  PredictRow,
  StrategyInfo,
  TestMode,
  TestRequest,
  TestResult,
  TestState,
  TestSummary,
} from "./types";

const DEFAULT = "tenhou-normal-4p-hanchan/hand-reading";

const MODES: Record<TestMode, { name: string; desc: string; unit: string }> = {
  selfplay: {
    name: "自我對戰",
    desc: "四個座位各放一個參賽者（可以重複，同名的合併統計），每場輪換座位。",
    unit: "場",
  },
  duplicate: {
    name: "duplicate 對戰",
    desc: "同一組牌山輪換座位打 4 場，抵消牌運。兩方時各坐兩個位置，並算出兩方的成對差距與信賴區間。",
    unit: "組（每組 4 場）",
  },
  field: {
    name: "場地對戰",
    desc: "A、B 各自和三家「場地」同桌（不同桌），比較「面對這種對手時」誰比較好。",
    unit: "組（每組 8 場）",
  },
  predict: {
    name: "預測評估",
    desc: "四個策略輪換座位自我對戰，用全知視角判定每個讀牌預測，依「被預測的是誰」分組看準不準。",
    unit: "場",
  },
};

const SEATS = ["東", "南", "西", "北"];

/** 需要幾個參賽者。 */
function slotsOf(mode: TestMode, four: boolean): string[] {
  switch (mode) {
    case "selfplay":
    case "predict":
      return SEATS.map((s) => `${s}家（起始）`);
    case "duplicate":
      return four ? ["A", "B", "C", "D"] : ["A", "B"];
    case "field":
      return ["A", "B"];
  }
}

function load<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v ? { ...fallback, ...(JSON.parse(v) as T) } : fallback;
  } catch {
    return fallback;
  }
}

interface Form {
  mode: TestMode;
  four: boolean;
  entrants: string[];
  field: string;
  count: number;
  seed: string;
}

const FORM: Form = {
  mode: "duplicate",
  four: false,
  entrants: [DEFAULT, "tenhou-normal-4p-hanchan/push-fold", DEFAULT, DEFAULT],
  field: DEFAULT,
  count: 200,
  seed: "",
};

/** 名稱裡的參賽者全名換成短名（場地對戰的「X（與 A 同桌）」也一樣）。 */
function short(name: string, specs: string[]): string {
  let out = name;
  for (const s of [...specs].sort((a, b) => b.length - a.length)) out = out.split(s).join(shortName(s));
  return out;
}

const pct = (x: number) => `${(x * 100).toFixed(1)}%`;
const signed = (x: number, d = 2) => `${x >= 0 ? "+" : ""}${x.toFixed(d)}`;

interface Props {
  onOpen: (id: string) => void;
  onBack: () => void;
}

/** 測試頁：新測試的設定、進度、已完成的測試。 */
export function Tests({ onOpen, onBack }: Props) {
  const [entrants, setEntrants] = useState<StrategyInfo[]>([]);
  const [form, setForm] = useState<Form>(() => load("testForm", FORM));
  const [state, setState] = useState<TestState>({ state: "idle" });
  const [list, setList] = useState<TestSummary[]>([]);
  const [error, setError] = useState<string | null>(null);
  // 按下停止後到測試真的結束之間（進行中的對局要打完）：按鈕停用，避免重複點擊。
  const [stopping, setStopping] = useState(false);

  const refresh = useCallback(() => {
    call<TestSummary[]>("tests").then(setList, (e) => setError(String(e)));
  }, []);
  useEffect(() => {
    call<StrategyInfo[]>("entrants").then(setEntrants, (e) => setError(String(e)));
    call<TestState>("test_status").then(setState);
    refresh();
  }, [refresh]);
  useEffect(() => {
    try {
      localStorage.setItem("testForm", JSON.stringify(form));
    } catch {
      // 無法儲存時忽略
    }
  }, [form]);
  useEffect(() => {
    if (state.state !== "running") return;
    const t = setInterval(() => {
      call<TestState>("test_status").then((s) => {
        setState(s);
        if (s.state !== "running") {
          setStopping(false);
          refresh();
        }
      });
    }, 500);
    return () => clearInterval(t);
  }, [state.state, refresh]);

  const slots = slotsOf(form.mode, form.four);
  const choices = form.mode === "predict" ? entrants.filter((c) => c.kind === "strategy") : entrants;
  const seedBad = form.seed !== "" && !Number.isSafeInteger(Number(form.seed));

  const start = async () => {
    setError(null);
    const req: TestRequest = {
      mode: form.mode,
      entrants: form.entrants.slice(0, slots.length),
      field: form.mode === "field" ? form.field : null,
      count: form.count,
      seed: form.seed === "" ? null : Number(form.seed),
    };
    try {
      await call("start_test", req);
      setState(await call<TestState>("test_status"));
    } catch (e) {
      setError(String(e));
    }
  };

  const select = (value: string, onChange: (v: string) => void) => (
    <select value={value} onChange={(e) => onChange(e.target.value)}>
      {!choices.some((c) => c.name === value) && <option value={value}>{value}</option>}
      {choices.map((c) => (
        <option key={c.name} value={c.name}>
          {optionLabel(c)}
        </option>
      ))}
    </select>
  );

  const running = state.state === "running";
  return (
    <div className="setup tests">
      <div className="review-head">
        <h1>對戰測試</h1>
        <button onClick={onBack}>返回</button>
      </div>
      <p className="lead">讓策略、機器人、Mortal 大量對戰，查看勝率與每場的細節；點開任一場可以逐打檢視、復盤與實驗。</p>
      <div className="form">
        <label>
          種類
          <select value={form.mode} onChange={(e) => setForm({ ...form, mode: e.target.value as TestMode })}>
            {(Object.keys(MODES) as TestMode[]).map((m) => (
              <option key={m} value={m}>
                {MODES[m].name}
              </option>
            ))}
          </select>
          <span className="desc">{MODES[form.mode].desc}</span>
        </label>
        {form.mode === "duplicate" && (
          <label>
            參賽者數
            <select value={form.four ? 4 : 2} onChange={(e) => setForm({ ...form, four: e.target.value === "4" })}>
              <option value={2}>兩方（各坐兩個位置，成對比較）</option>
              <option value={4}>四方（各坐一個位置）</option>
            </select>
          </label>
        )}
        {slots.map((label, i) => (
          <label key={label}>
            {label}
            {select(form.entrants[i] ?? DEFAULT, (v) => {
              const e = [...form.entrants];
              e[i] = v;
              setForm({ ...form, entrants: e });
            })}
          </label>
        ))}
        {form.mode === "field" && (
          <label>
            場地（其他三家）
            {select(form.field, (v) => setForm({ ...form, field: v }))}
          </label>
        )}
        <label>
          {form.mode === "duplicate" || form.mode === "field" ? "組數" : "場數"}
          <input
            type="number"
            min={1}
            value={form.count}
            onChange={(e) => setForm({ ...form, count: Math.max(1, Number(e.target.value) || 1) })}
          />
          <span className="desc">單位：{MODES[form.mode].unit}。有 Mortal 時同時進行的對局數依可用記憶體限制。</span>
        </label>
        <label>
          seed（空白 = 隨機）
          <input
            value={form.seed}
            placeholder="隨機"
            onChange={(e) => setForm({ ...form, seed: e.target.value.replace(/[^0-9]/g, "").slice(0, 16) })}
          />
          {seedBad && <span className="desc error-text">seed 最大 9007199254740991</span>}
        </label>
      </div>
      {error && <div className="error">{error}</div>}
      {state.state === "failed" && <div className="error">測試失敗：{state.error}</div>}
      <div className="buttons">
        {running ? (
          <>
            <progress className={stopping ? "stopping" : ""} value={state.done} max={state.total} />
            <span className="muted">
              {state.done}／{state.total} 場　{Math.round(state.seconds)} 秒
              {stopping
                ? "　停止中：等進行中的對局打完…"
                : state.done > 0 && `（約再 ${Math.round((state.seconds / state.done) * (state.total - state.done))} 秒）`}
            </span>
            <button
              className={stopping ? "stopping" : ""}
              disabled={stopping}
              onClick={() => {
                setStopping(true);
                void call("cancel_test").catch(() => setStopping(false));
              }}
            >
              {stopping ? "停止中…" : "停止（以完成的對局計算）"}
            </button>
          </>
        ) : (
          <button className="primary big" onClick={start} disabled={seedBad || entrants.length === 0}>
            開始測試
          </button>
        )}
      </div>

      <h3>測試紀錄</h3>
      {list.length === 0 ? (
        <p className="muted">還沒有測試。</p>
      ) : (
        <table className="summary tests-list">
          <thead>
            <tr>
              <th>時間</th>
              <th>種類</th>
              <th>參賽者</th>
              <th>場數</th>
              <th>秒</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {list.map((t) => (
              <tr key={t.id} className="clickable" onClick={() => onOpen(t.id)}>
                <td>{formatUpdated(t.created)}</td>
                <td>{MODES[t.mode].name}</td>
                <td className="names">
                  {t.entrants.map(shortName).join(" vs ")}
                  {t.field && `（場地 ${shortName(t.field)}）`}
                </td>
                <td>
                  {t.games_done}／{t.games_total}
                  {t.cancelled && <span className="badge diff">停止</span>}
                </td>
                <td>{Math.round(t.seconds)}</td>
                <td>
                  <button
                    className="small"
                    onClick={(e) => {
                      e.stopPropagation();
                      if (window.confirm("刪除這個測試？")) void call("delete_test", { id: t.id }).then(refresh);
                    }}
                  >
                    刪除
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

function EntrantTable({ rows, specs }: { rows: EntrantResult[]; specs: string[] }) {
  return (
    <table className="summary">
      <thead>
        <tr>
          <th>參賽者</th>
          <th>座位場數</th>
          <th>平均順位</th>
          <th>1／2／3／4 位</th>
          <th>平均名次點</th>
          <th>和了率</th>
          <th>放銃率</th>
          <th>立直率</th>
          <th>平均和了</th>
          <th>平均放銃</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((e) => {
          const h = e.hands;
          const wins = h.ron + h.tsumo;
          const rate = (n: number) => pct(n / Math.max(1, h.hands));
          return (
            <tr key={e.name}>
              <td title={e.name}>{short(e.name, specs)}</td>
              <td>{e.games}</td>
              <td>
                {e.avg_rank.toFixed(3)} <span className="muted">±{e.rank_ci.toFixed(3)}</span>
              </td>
              <td>{e.ranks.map((r) => pct(r / Math.max(1, e.games))).join("／")}</td>
              <td>
                {signed(e.avg_points)} <span className="muted">±{e.points_ci.toFixed(2)}</span>
              </td>
              <td>{rate(wins)}</td>
              <td>{rate(h.deal_ins)}</td>
              <td>{rate(h.riichi)}</td>
              <td>{Math.round(h.win_points / Math.max(1, wins))}</td>
              <td>{Math.round(h.deal_in_points / Math.max(1, h.deal_ins))}</td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function PredictTable({ rows, specs }: { rows: PredictRow[]; specs: string[] }) {
  const [unriichi, setUnriichi] = useState(false);
  const groups = [...new Set(rows.map((r) => `${r.source}（${r.category}）`))];
  return (
    <>
      <label className="check">
        <input type="checkbox" checked={unriichi} onChange={(e) => setUnriichi(e.target.checked)} />
        只看沒立直的被預測者（印象操作只在沒立直時有作用）
      </label>
      {groups.map((g) => (
        <div key={g}>
          <h4>{g}</h4>
          <table className="summary">
            <thead>
              <tr>
                <th>被預測者</th>
                <th>筆數</th>
                <th>平均預測</th>
                <th>實際發生率</th>
                <th>Brier</th>
                <th>技巧分數</th>
                <th>發生時預測</th>
                <th>沒發生時預測</th>
              </tr>
            </thead>
            <tbody>
              {rows
                .filter((r) => `${r.source}（${r.category}）` === g && r.unriichi === unriichi)
                .map((r) => (
                  <tr key={r.target}>
                    <td>{short(r.target, specs)}</td>
                    <td>{r.metrics.n}</td>
                    <td>{r.metrics.mean_p.toFixed(3)}</td>
                    <td>{r.metrics.base_rate.toFixed(3)}</td>
                    <td>{r.metrics.brier.toFixed(4)}</td>
                    <td className={r.metrics.skill > 0 ? "plus" : "minus"}>{signed(r.metrics.skill, 3)}</td>
                    <td>{r.when_true.toFixed(3)}</td>
                    <td>{r.when_false.toFixed(3)}</td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      ))}
      <p className="muted">
        技巧分數 = 1 − Brier ÷ 基準 Brier（基準 = 永遠猜平均發生率），&gt; 0 代表比基準好。
        「發生時預測」被騙時會偏低，「沒發生時預測」被騙時會偏高。
      </p>
    </>
  );
}

const PAGE = 50;

interface ResultProps {
  id: string;
  onOpenGame: (info: LogInfo) => void;
  onBack: () => void;
}

/** 一個測試的結果：彙總表、成對比較、預測評估、對局清單。 */
export function TestResultView({ id, onOpenGame, onBack }: ResultProps) {
  const [result, setResult] = useState<TestResult | null>(null);
  const [page, setPage] = useState<GamePage | null>(null);
  const [sort, setSort] = useState("index");
  const [desc, setDesc] = useState(false);
  const [offset, setOffset] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [opening, setOpening] = useState<number | null>(null);

  useEffect(() => {
    call<TestResult>("test_result", { id }).then(setResult, (e) => setError(String(e)));
  }, [id]);
  useEffect(() => {
    if (!result || result.games === 0) return;
    call<GamePage>("test_games", { id, sort, desc, offset, limit: PAGE }).then(setPage, (e) => setError(String(e)));
  }, [id, result, sort, desc, offset]);

  if (error && !result) {
    return (
      <div className="setup">
        <div className="error">{error}</div>
        <button onClick={onBack}>返回</button>
      </div>
    );
  }
  if (!result) return <div className="loading">讀取中…</div>;

  const s = result.summary;
  const specs = [...s.entrants, ...(s.field ? [s.field] : [])];
  const cmp = result.report?.comparison;
  const names = result.names;
  const open = async (index: number) => {
    setOpening(index);
    setError(null);
    try {
      onOpenGame(await call<LogInfo>("open_test_game", { id, index }));
    } catch (e) {
      setError(String(e));
    } finally {
      setOpening(null);
    }
  };
  const verdict = (diff: number, ci: number, lowerIsBetter: boolean) =>
    Math.abs(diff) <= ci ? "差距在誤差範圍內" : (diff < 0) === lowerIsBetter ? "前者顯著較好" : "後者顯著較好";

  return (
    <div className="test-result">
      <div className="review-head">
        <h1>
          {MODES[s.mode].name}
          {s.cancelled && <span className="badge diff">中途停止</span>}
        </h1>
        <button onClick={onBack}>返回</button>
      </div>
      <p className="lead">
        {s.entrants.map(shortName).join(" vs ")}
        {s.field && `，場地 ${shortName(s.field)}`}　{s.count} {MODES[s.mode].unit}（完成 {s.games_done}／{s.games_total}{" "}
        場）　seed {s.seed}　{Math.round(s.seconds)} 秒
      </p>
      {result.report && (
        <>
          <EntrantTable rows={result.report.entrants} specs={specs} />
          <p className="muted">± 為 95% 信賴區間（以每場為一個樣本）。名次點：天鳳的順位點（含返點），四家合計 0。</p>
        </>
      )}
      {cmp && (
        <div className="panel comparison">
          <div>
            {short(names[0], specs)} − {short(names[1], specs)}：名次點差{" "}
            <b className={Math.abs(cmp.points_diff) > cmp.points_ci ? (cmp.points_diff > 0 ? "plus" : "minus") : ""}>
              {signed(cmp.points_diff)} ± {cmp.points_ci.toFixed(2)}
            </b>
            （{verdict(cmp.points_diff, cmp.points_ci, false)}）
          </div>
          <div>
            平均順位差 {signed(cmp.rank_diff, 3)} ± {cmp.rank_ci.toFixed(3)}（{verdict(cmp.rank_diff, cmp.rank_ci, true)}）
          </div>
          <div className="muted">以每組（同一組牌山）的成對差距計算，{cmp.blocks} 組。</div>
        </div>
      )}
      {result.predict.length > 0 && <PredictTable rows={result.predict} specs={specs} />}
      {result.games > 0 && (
        <>
          <h3>對局（{result.games} 場）</h3>
          <div className="buttons">
            <label>
              排序
              <select
                value={sort}
                onChange={(e) => {
                  setSort(e.target.value);
                  setOffset(0);
                }}
              >
                <option value="index">場次</option>
                <option value="hands">局數</option>
                {names.map((n, i) => (
                  <option key={i} value={`item:${i}`}>
                    {short(n, specs)} 的名次點
                  </option>
                ))}
              </select>
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={desc}
                onChange={(e) => {
                  setDesc(e.target.checked);
                  setOffset(0);
                }}
              />
              由大到小
            </label>
            <span className="muted">點一場：重打並打開（逐打檢視、復盤、實驗）</span>
          </div>
          {error && <div className="error">{error}</div>}
          {page && (
            <>
              <table className="summary games">
                <thead>
                  <tr>
                    <th>場次</th>
                    <th>組</th>
                    <th>局數</th>
                    {SEATS.map((w) => (
                      <th key={w}>{w}家（起始）</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {page.rows.map((g) => (
                    <tr key={g.index} className={`clickable ${opening === g.index ? "opening" : ""}`} onClick={() => void open(g.index)}>
                      <td>{g.index + 1}</td>
                      <td>{g.block + 1}</td>
                      <td>{g.hands}</td>
                      {g.seats.map((seat, i) => (
                        <td key={i} className={`item-${seat.item % 4}`}>
                          <span className="seat-name">{short(seat.name, specs)}</span> {seat.score}
                          <span className="muted">（{seat.rank + 1} 位）</span>
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
              <div className="buttons">
                <button disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE))}>
                  上一頁
                </button>
                <span className="muted">
                  {offset + 1}–{Math.min(offset + PAGE, page.total)}／{page.total}
                </span>
                <button disabled={offset + PAGE >= page.total} onClick={() => setOffset(offset + PAGE)}>
                  下一頁
                </button>
              </div>
            </>
          )}
        </>
      )}
    </div>
  );
}
