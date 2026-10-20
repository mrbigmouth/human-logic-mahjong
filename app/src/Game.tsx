// 對局畫面：向核心長輪詢新的事件幀，依序播放（AI 的動作有間隔），輪到自己時顯示操作與提示。

import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { call, saveText } from "./api";
import { Board, Meld, REL_NAMES, WINDS, relative, shortName } from "./Board";
import { PolicyList } from "./Advisors";
import { Experiment } from "./Experiment";
import { useBoardSize } from "./layout";
import { play, type SoundKind } from "./sounds";
import { Tile, TileText, Tiles } from "./Tile";
import type {
  ActionView,
  Advice,
  Ask,
  EventView,
  ExperimentTarget,
  Frame,
  GameOver,
  Hint,
  HintDecision,
  NewGame,
  TableView,
  Update,
  WinRoute,
} from "./types";

interface Props {
  game: NewGame;
  /** 對局結束後：復盤或檢視這場的牌譜（App 會把牌譜讀入後切換畫面） */
  onOpenLog: (target: "review" | "view") => void;
  onQuit: () => void;
}

/** 下一幀要等多久（毫秒）：AI 的打牌與鳴牌依速度設定，其他事件很快帶過。 */
function delayFor(ev: EventView, me: number, speed: number): number {
  switch (ev.type) {
    case "discard":
      return ev.seat === me ? 0 : speed;
    case "call":
    case "riichi_accepted":
      return speed;
    case "draw":
      return ev.seat === me ? 0 : Math.min(120, speed / 3);
    case "start_kyoku":
      return 400;
    default:
      return 0;
  }
}

export function Game({ game, onOpenLog, onQuit }: Props) {
  const [frames, setFrames] = useState<Frame[]>([]);
  const [pos, setPos] = useState(0);
  const [ask, setAsk] = useState<Ask | null>(null);
  const [over, setOver] = useState<GameOver | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hold, setHold] = useState<number | null>(null); // 停在第幾幀（一局結果）
  const [released, setReleased] = useState<Set<number>>(new Set());
  const [riichiMode, setRiichiMode] = useState(false);
  const [showHint, setShowHint] = useState(() => localStorage.getItem("hint") !== "off");
  const [speed, setSpeed] = useState(() => Number(localStorage.getItem("speed") ?? 500));
  const [volume, setVolume] = useState(() => Number(localStorage.getItem("volume") ?? 0.6));
  const [panelOpen, setPanelOpen] = useState(() => localStorage.getItem("panel") !== "closed");
  const [tab, setTab] = useState<Tab>(() => (localStorage.getItem("tab") as Tab | null) ?? "hint");
  const [expTarget, setExpTarget] = useState<ExperimentTarget | null>(null);
  const area = useRef<HTMLDivElement>(null);
  const handArea = useRef<HTMLDivElement>(null);
  const boardSize = useBoardSize(area, handArea, frames.length > 0);
  const next = useRef(0);
  const waiting = useRef(false);

  useEffect(() => {
    try {
      localStorage.setItem("hint", showHint ? "on" : "off");
      localStorage.setItem("speed", String(speed));
      localStorage.setItem("volume", String(volume));
      localStorage.setItem("panel", panelOpen ? "open" : "closed");
      localStorage.setItem("tab", tab);
    } catch {
      // 無法儲存設定時忽略
    }
  }, [showHint, speed, volume, panelOpen, tab]);

  // 長輪詢：有詢問或對局結束時暫停，回答後再繼續。
  const poll = useMemo(
    () => async () => {
      if (waiting.current) return;
      waiting.current = true;
      try {
        for (;;) {
          const u = await call<Update>("poll", { since: next.current, wait_ms: 25000 });
          next.current = u.next;
          if (u.frames.length > 0) setFrames((f) => [...f, ...u.frames]);
          if (u.error) {
            setError(u.error);
            return;
          }
          if (u.ask) {
            setAsk(u.ask);
            return;
          }
          if (u.game_over) {
            setOver(u.game_over);
            return;
          }
        }
      } catch (e) {
        setError(String(e));
      } finally {
        waiting.current = false;
      }
    },
    [],
  );

  useEffect(() => {
    void poll();
  }, [poll]);

  // 播放：一幀一幀前進；遇到一局結果時停下等玩家按「繼續」。
  useEffect(() => {
    if (hold !== null || pos >= frames.length - 1) return;
    const nextFrame = frames[pos + 1];
    const t = setTimeout(
      () => {
        setPos(pos + 1);
        const sound = soundFor(nextFrame.event);
        if (sound) play(sound, volume);
        const type = nextFrame.event.type;
        if ((type === "win" || type === "ryuukyoku") && !released.has(pos + 1)) {
          setHold(pos + 1);
        }
      },
      delayFor(nextFrame.event, game.seat, speed),
    );
    return () => clearTimeout(t);
  }, [pos, frames, hold, speed, game.seat, released, volume]);

  const frame = frames[Math.min(pos, frames.length - 1)];
  // 詢問要等播放追上（看到的幀數 ≥ 詢問時的幀數）才顯示。
  const activeAsk = ask && frames.length > 0 && pos + 1 >= ask.frame && hold === null ? ask : null;

  const act = async (a: ActionView) => {
    if (!ask) return;
    const current = ask;
    setAsk(null);
    setRiichiMode(false);
    try {
      await call("act", { ask: current.id, index: a.index });
    } catch (e) {
      setError(String(e));
      return;
    }
    void poll();
  };

  const finished = over !== null && pos >= frames.length - 1 && hold === null;

  if (!frame) {
    return <div className="loading">{error ?? "對局準備中…"}</div>;
  }

  const table = frame.table;
  const events = frames.slice(Math.max(0, pos - 40), pos + 1).reverse();
  const hint = activeAsk && showHint ? activeAsk.hint : null;

  const tabs: [Tab, string, boolean][] = [
    ["hint", "提示", hint !== null],
    ["efficiency", "牌效", hint !== null && hint.efficiency.length > 0],
    ["routes", "和牌路線", hint !== null && hint.routes.length > 0],
    ["experiment", "實驗", false],
    ["log", "紀錄", false],
    ["info", "對局資訊", false],
  ];

  return (
    <div className={`game ${panelOpen ? "panel-open" : ""}`}>
      <div className="table-area" ref={area} style={{ "--B": `${boardSize}px` } as CSSProperties}>
        <Board table={table} names={game.names} />
        <div className="hand-wrap" ref={handArea}>
          <Hand
            table={table}
            ask={activeAsk}
            riichiMode={riichiMode}
            setRiichiMode={setRiichiMode}
            hint={hint ? hint.action : null}
            onAct={act}
          />
        </div>
      </div>
      <section className="bottom-panel">
        <div className="panel-bar">
          <button className="toggle" onClick={() => setPanelOpen(!panelOpen)} title={panelOpen ? "收起" : "展開"}>
            {panelOpen ? "▾" : "▴"}
          </button>
          <div className="tabs">
            {tabs.map(([id, label, dot]) => (
              <button
                key={id}
                className={`tab ${tab === id && panelOpen ? "on" : ""}`}
                onClick={() => {
                  setTab(id);
                  setPanelOpen(true);
                }}
              >
                {label}
                {dot && <span className="tab-dot" />}
              </button>
            ))}
          </div>
          <div className="controls">
            <label>
              <input type="checkbox" checked={showHint} onChange={(e) => setShowHint(e.target.checked)} /> 即時提示
            </label>
            <label title="AI 打牌的間隔">
              速度
              <input
                type="range"
                min={0}
                max={1200}
                step={100}
                value={1200 - speed}
                onChange={(e) => setSpeed(1200 - Number(e.target.value))}
              />
            </label>
            <label title="立直、鳴牌、和牌的音效（最左 = 關閉）">
              音效
              <input
                type="range"
                min={0}
                max={1}
                step={0.1}
                value={volume}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  setVolume(v);
                  play("call", v);
                }}
              />
            </label>
            <button onClick={() => (over || window.confirm("放棄這場對局？")) && onQuit()}>結束</button>
          </div>
        </div>
        {panelOpen && (
          <div className={`panel-body ${tab === "experiment" ? "tall" : ""}`}>
            {tab === "hint" &&
              (hint ? (
                <HintPanel
                  hint={hint}
                  onExperiment={(a, d) => {
                    if (!activeAsk) return;
                    setExpTarget({
                      source: "game",
                      ask: activeAsk.id,
                      kind: d.kind_key,
                      options: d.options,
                      reference: d.chosen,
                      referenceLabel: a.title,
                      marks: otherPicks(hint, a, d),
                      strategy: a.ai ? undefined : a.name,
                    });
                    setTab("experiment");
                  }}
                />
              ) : (
                <div className="muted">
                  {activeAsk ? "輪到你了（即時提示已關閉）" : "輪到你時，這裡會顯示策略（與 AI）的選擇與理由。"}
                </div>
              ))}
            {tab === "efficiency" &&
              (hint && hint.efficiency.length > 0 ? (
                <EfficiencyTable hint={hint} />
              ) : (
                <div className="muted">輪到你打牌時，這裡會列出每張牌打出後的向聽數與進張。</div>
              ))}
            {tab === "routes" &&
              (hint && hint.routes.length > 0 ? (
                <RoutesTable routes={hint.routes} />
              ) : (
                <div className="muted">輪到你打牌、離聽牌 2 向聽以內時，這裡會列出最近的和牌形、還要摸哪些牌與預估點數。</div>
              ))}
            {tab === "experiment" &&
              (expTarget ? (
                <Experiment target={expTarget} />
              ) : (
                <div className="muted">
                  在「提示」分頁的決策旁按「實驗」：比較這個選擇的各選項，之後四家都依策略打完的平均結果、和了率與放銃率。
                </div>
              ))}
            {tab === "log" && (
              <div className="log">
                {events.map((f, i) => (
                  <div key={i} className={`log-line ev-${f.event.type}`}>
                    <TileText text={f.event.text} />
                    {f.auto && <span className="muted">（自動）</span>}
                  </div>
                ))}
              </div>
            )}
            {tab === "info" && <GameInfo game={game} table={table} />}
          </div>
        )}
      </section>
      {error && (
        <div className="modal">
          <div className="dialog">
            <h2>發生錯誤</h2>
            <p>{error}</p>
            <button onClick={onQuit}>回到設定</button>
          </div>
        </div>
      )}
      {hold !== null && frames[hold] && (
        <HandResult
          frame={frames[hold]}
          names={game.names}
          onContinue={() => {
            setReleased((r) => new Set(r).add(hold));
            setHold(null);
          }}
        />
      )}
      {finished && over && (
        <GameResult
          over={over}
          names={game.names}
          seed={game.seed}
          onOpenLog={onOpenLog}
          onQuit={onQuit}
        />
      )}
    </div>
  );
}

type Tab = "hint" | "efficiency" | "routes" | "experiment" | "log" | "info";

/** 播放到這一幀時的音效。 */
function soundFor(ev: EventView): SoundKind | null {
  switch (ev.type) {
    case "discard":
      return ev.riichi ? "riichi" : null;
    case "call":
      return "call";
    case "win":
      return "win";
    default:
      return null;
  }
}

/** 對局資訊：四家的參賽者、seed。 */
function GameInfo({ game, table }: { game: NewGame; table: TableView }) {
  return (
    <div className="game-info">
      <table>
        <tbody>
          {game.names.map((n, s) => (
            <tr key={s} className={s === table.me ? "me" : ""}>
              <td>{REL_NAMES[relative(table.me, s)]}</td>
              <td>{WINDS[table.seats[s].wind]}</td>
              <td className="score">{table.scores[s]}</td>
              <td>{n}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="muted">seed {game.seed}（設定畫面輸入同一個 seed 可以重打同樣的牌山）</div>
    </div>
  );
}

interface HandProps {
  table: TableView;
  ask: Ask | null;
  riichiMode: boolean;
  setRiichiMode: (b: boolean) => void;
  /** 提示的動作編號 */
  hint: number | null;
  onAct: (a: ActionView) => void;
}

/** 自己的手牌與操作按鈕。 */
export function Hand({ table, ask, riichiMode, setRiichiMode, hint, onAct }: HandProps) {
  const me = table.seats[table.me];
  const actions = ask?.actions ?? [];
  const discardType = riichiMode ? "riichi" : "discard";
  const find = (pai: string, tsumogiri: boolean) =>
    actions.find((a) => a.type === discardType && a.pai === pai && a.tsumogiri === tsumogiri);
  const hintAction = hint !== null ? actions[hint] : undefined;
  // 提示是立直時，要先按「立直」（按鈕上有提示點），牌才會標示；避免直接點牌而變成默聽。
  const isHint = (pai: string, tsumogiri: boolean) =>
    hintAction !== undefined &&
    hintAction.type === discardType &&
    hintAction.pai === pai &&
    hintAction.tsumogiri === tsumogiri;
  const buttons = actions.filter((a) => a.type !== "discard" && a.type !== "riichi");
  const canRiichi = actions.some((a) => a.type === "riichi");
  const tile = (pai: string, tsumogiri: boolean, key: number | string) => {
    const a = find(pai, tsumogiri);
    return (
      <Tile
        key={key}
        pai={pai}
        dim={ask !== null && !a && actions.some((x) => x.type === discardType)}
        mark={isHint(pai, tsumogiri) ? "hint" : null}
        onClick={a ? () => onAct(a) : undefined}
      />
    );
  };
  return (
    <div className="my-area">
      <div className={`my-hand ${me.revealed ? "revealed" : ""}`}>
        <span className="hand-tiles">{me.hand.map((p, i) => tile(p, false, i))}</span>
        {me.drawn && <span className="drawn">{tile(me.drawn, true, "drawn")}</span>}
        <span className="melds">
          {me.melds.map((m, i) => (
            <Meld key={i} meld={m} seat={table.me} />
          ))}
        </span>
      </div>
      <div className="actions">
        {canRiichi && (
          <button className={`act riichi ${riichiMode ? "on" : ""}`} onClick={() => setRiichiMode(!riichiMode)}>
            {riichiMode ? "取消立直" : "立直"}
            {hintAction?.type === "riichi" && <span className="hint-dot" />}
          </button>
        )}
        {buttons.map((a) => (
          <button
            key={a.index}
            className={`act ${a.type} ${hint === a.index ? "hinted" : ""}`}
            onClick={() => onAct(a)}
          >
            <TileText text={a.label} />
          </button>
        ))}
        {riichiMode && <span className="note">選擇立直要打的牌</span>}
      </div>
    </div>
  );
}

/** 其他顧問在這個決策的選擇（實驗的結果表標示用）。 */
function otherPicks(hint: Hint, a: Advice, d: HintDecision): [string, number][] {
  const k = hint.advisors.indexOf(a);
  return hint.advisors.flatMap((b, j) => {
    const p = d.picks[j];
    return j !== k && p !== null && p !== undefined ? [[b.title, p] as [string, number]] : [];
  });
}

/** 各顧問的建議並排：策略列出決策與理由，AI 列出各動作的機率。 */
function HintPanel({ hint, onExperiment }: { hint: Hint; onExperiment: (a: Advice, d: HintDecision) => void }) {
  return (
    <div className="hint advisors-grid" style={{ "--n": hint.advisors.length } as CSSProperties}>
      {hint.advisors.map((a, k) => (
        <div key={a.name} className={`advice ${k === 0 ? "main" : ""}`}>
          <h3>
            <span className="advisor-name">{a.title}</span>
            <TileText text={a.label} />
          </h3>
          {a.error && <div className="error-text">{a.error}</div>}
          {a.ai && <PolicyList policy={a.policy} />}
          <div className="decisions-grid">
            {a.decisions.map((d, i) => (
              <div key={i} className="decision">
                <div className="kind">
                  【{d.kind}】<TileText text={d.options[d.chosen] ?? ""} />
                  {d.options.length > 1 && (
                    <button
                      className="small"
                      onClick={() => onExperiment(a, d)}
                      title="比較各選項之後依策略打的結果"
                    >
                      實驗
                    </button>
                  )}
                </div>
                {d.notes.length > 0 && (
                  <ul>
                    {d.notes.map((n, j) => (
                      <li key={j}>
                        <TileText text={n} />
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function EfficiencyTable({ hint }: { hint: NonNullable<Ask["hint"]> }): ReactNode {
  return (
    <table className="efficiency">
      <thead>
        <tr>
          <th>打</th>
          <th>向聽</th>
          <th>進張</th>
          <th>有效牌</th>
        </tr>
      </thead>
      <tbody>
        {hint.efficiency.map((e) => (
          <tr key={e.pai}>
            <td>
              <Tile pai={e.pai} />
            </td>
            <td>{e.shanten === 0 ? "聽牌" : `${e.shanten} 向聽`}</td>
            <td>{Math.round(e.ukeire)} 張</td>
            <td>
              <Tiles className="inline" tiles={e.tiles} />
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** 和牌路線：還要摸的牌、要打的牌、完成形、役與點數。機會以列出的路線合計為 100% 換算。 */
function RoutesTable({ routes }: { routes: WinRoute[] }) {
  const total = routes.reduce((a, r) => a + r.chance, 0) || 1;
  return (
    <table className="routes">
      <thead>
        <tr>
          <th>打</th>
          <th>要摸</th>
          <th>完成形</th>
          <th>役</th>
          <th>榮和</th>
          <th title="各路線要摸的牌的剩餘張數組合，換算成列出的路線之間的比例">機會</th>
        </tr>
      </thead>
      <tbody>
        {routes.map((r, i) => {
          // 完成形中標出和牌張（只標一張）。
          const win = r.hand.indexOf(r.win_tile);
          return (
            <tr key={i}>
              <td>
                <Tiles className="inline" tiles={r.discards} />
              </td>
              <td>
                <Tiles className="inline" tiles={r.draws} />
              </td>
              <td className="route-hand">
                {r.hand.map((p, j) => (
                  <Tile key={j} pai={p} mark={j === win ? "hint" : null} />
                ))}
              </td>
              <td className="yaku-list">
                {r.yaku.length > 0
                  ? r.yaku.map(([n, h]) => `${n}${h >= 13 ? "" : ` ${h}`}`).join("・")
                  : "無役"}
              </td>
              <td className="pts">
                {r.points > 0 ? (
                  <>
                    {r.points}
                    <span className="muted">
                      {r.limit ? ` ${r.limit}` : ` ${r.fu}符${r.han}番`}
                      {r.riichi ? "（立直）" : ""}
                    </span>
                  </>
                ) : (
                  "—"
                )}
              </td>
              <td className="pts">{Math.round((r.chance / total) * 100)}%</td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function Deltas({ deltas, names, me }: { deltas: number[]; names: string[]; me: number }) {
  return (
    <table className="deltas">
      <tbody>
        {deltas.map((d, s) => (
          <tr key={s} className={s === me ? "me" : ""}>
            <td>{REL_NAMES[relative(me as 0, s)]}</td>
            <td className="name">{shortName(names[s])}</td>
            <td className={d > 0 ? "plus" : d < 0 ? "minus" : ""}>{d > 0 ? `+${d}` : d}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function HandResult({ frame, names, onContinue }: { frame: Frame; names: string[]; onContinue: () => void }) {
  const ev = frame.event;
  const me = frame.table.me;
  return (
    <div className="modal">
      <div className="dialog result">
        <h2>{frame.table.round}</h2>
        {ev.type === "win" && (
          <>
            <h3>
              {REL_NAMES[relative(me, ev.winner)]}（{shortName(names[ev.winner])}）
              {ev.winner === ev.from ? "自摸" : `榮和　放銃：${REL_NAMES[relative(me, ev.from)]}`}
            </h3>
            <div className="win-hand">
              {ev.hand
                .filter((_, i) => i !== ev.hand.indexOf(ev.win_tile))
                .map((p, i) => (
                  <Tile key={i} pai={p} />
                ))}
              <span className="win-tile">
                <Tile pai={ev.win_tile} />
              </span>
              <span className="melds">
                {frame.table.seats[ev.winner].melds.map((m, i) => (
                  <Meld key={i} meld={m} seat={ev.winner} />
                ))}
              </span>
            </div>
            <table className="yaku">
              <tbody>
                {ev.yaku.map(([name, han], i) => (
                  <tr key={i}>
                    <td>{name}</td>
                    <td>{han >= 13 ? "役滿" : `${han} 番`}</td>
                  </tr>
                ))}
                {ev.dora > 0 && (
                  <tr>
                    <td>寶牌</td>
                    <td>{ev.dora} 番</td>
                  </tr>
                )}
                {ev.aka > 0 && (
                  <tr>
                    <td>赤寶牌</td>
                    <td>{ev.aka} 番</td>
                  </tr>
                )}
                {ev.ura > 0 && (
                  <tr>
                    <td>裏寶牌</td>
                    <td>{ev.ura} 番</td>
                  </tr>
                )}
              </tbody>
            </table>
            <div className="value">
              {ev.han > 0 && `${ev.fu} 符 ${ev.han} 番`} {ev.limit}
            </div>
            {ev.ura_indicators.length > 0 && (
              <div className="ura">
                裏寶牌指示牌
                {ev.ura_indicators.map((p, i) => (
                  <Tile key={i} pai={p} />
                ))}
              </div>
            )}
          </>
        )}
        {ev.type === "ryuukyoku" && (
          <>
            <h3>{ev.reason}</h3>
            <div>
              聽牌：
              {ev.tenpai
                .map((t, s) => (t ? REL_NAMES[relative(me, s)] : null))
                .filter(Boolean)
                .join("、") || "無"}
            </div>
          </>
        )}
        {(ev.type === "win" || ev.type === "ryuukyoku") && <Deltas deltas={ev.deltas} names={names} me={me} />}
        <button className="primary" onClick={onContinue} autoFocus>
          繼續
        </button>
      </div>
    </div>
  );
}

function GameResult({
  over,
  names,
  seed,
  onOpenLog,
  onQuit,
}: {
  over: GameOver;
  names: string[];
  seed: number;
  onOpenLog: (target: "review" | "view") => void;
  onQuit: () => void;
}) {
  const order = [0, 1, 2, 3].sort((a, b) => over.ranks[a] - over.ranks[b]);
  const [saved, setSaved] = useState<string | null>(null);
  const save = async () => {
    try {
      const text = await call<string>("log");
      const stamp = new Date().toISOString().slice(0, 19).replace(/[-:T]/g, "");
      if (await saveText(`mahjong-${stamp}-${seed}.mjson`, text)) setSaved("已儲存");
    } catch (e) {
      setSaved(`無法儲存：${String(e)}`);
    }
  };
  return (
    <div className="modal">
      <div className="dialog result">
        <h2>對局結束（{over.hands_played} 局）</h2>
        <table className="final">
          <tbody>
            {order.map((s) => (
              <tr key={s} className={names[s] === "你" ? "me" : ""}>
                <td>{over.ranks[s] + 1} 位</td>
                <td className="name">{shortName(names[s])}</td>
                <td>{over.scores[s]}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <div className="buttons">
          <button className="primary" onClick={() => onOpenLog("review")}>
            復盤
          </button>
          <button onClick={() => onOpenLog("view")}>檢視牌譜</button>
          <button onClick={save}>儲存牌譜</button>
          <button onClick={onQuit}>新對局</button>
        </div>
        {saved && <div className="muted">{saved}</div>}
      </div>
    </div>
  );
}
