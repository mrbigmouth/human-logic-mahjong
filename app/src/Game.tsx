// 對局畫面：向核心長輪詢新的事件幀，依序播放（AI 的動作有間隔），輪到自己時顯示操作與提示。

import { useEffect, useMemo, useRef, useState } from "react";
import { call } from "./api";
import { Board, Meld, REL_NAMES, relative, shortName } from "./Board";
import { Tile, TileText, Tiles } from "./Tile";
import type { ActionView, Ask, EventView, Frame, GameOver, NewGame, TableView, Update } from "./types";

interface Props {
  game: NewGame;
  /** 對局結束後要復盤：傳回每次詢問（編號 = act_index + 1）對應的幀 */
  onReview: (frames: Frame[], askFrames: Map<number, number>) => void;
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

export function Game({ game, onReview, onQuit }: Props) {
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
  const askFrames = useRef(new Map<number, number>());
  const next = useRef(0);
  const waiting = useRef(false);

  useEffect(() => {
    try {
      localStorage.setItem("hint", showHint ? "on" : "off");
      localStorage.setItem("speed", String(speed));
    } catch {
      // 無法儲存設定時忽略
    }
  }, [showHint, speed]);

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
            askFrames.current.set(u.ask.id, u.ask.frame);
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
    const t = setTimeout(() => {
      setPos(pos + 1);
      const type = nextFrame.event.type;
      if ((type === "win" || type === "ryuukyoku") && !released.has(pos + 1)) {
        setHold(pos + 1);
      }
    }, delayFor(nextFrame.event, game.seat, speed));
    return () => clearTimeout(t);
  }, [pos, frames, hold, speed, game.seat, released]);

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

  return (
    <div className="game">
      <div className="main">
        <Board table={table} names={game.names} />
        <Hand
          table={table}
          ask={activeAsk}
          riichiMode={riichiMode}
          setRiichiMode={setRiichiMode}
          hint={hint ? hint.action : null}
          onAct={act}
        />
      </div>
      <aside className="side">
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
          <button onClick={() => (over || window.confirm("放棄這場對局？")) && onQuit()}>結束</button>
        </div>
        {hint && <HintPanel hint={hint} />}
        {activeAsk && !showHint && <div className="panel muted">輪到你了</div>}
        <div className="panel log">
          <h3>紀錄</h3>
          {events.map((f, i) => (
            <div key={i} className={`log-line ev-${f.event.type}`}>
              <TileText text={f.event.text} />
            </div>
          ))}
        </div>
        <div className="panel seed">seed {game.seed}</div>
      </aside>
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
          onReview={() => onReview(frames, askFrames.current)}
          onQuit={onQuit}
        />
      )}
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
  const isHint = (pai: string, tsumogiri: boolean) =>
    hintAction !== undefined &&
    (hintAction.type === "discard" || hintAction.type === "riichi") &&
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
      <div className="actions">
        {canRiichi && (
          <button className={`act riichi ${riichiMode ? "on" : ""}`} onClick={() => setRiichiMode(!riichiMode)}>
            {riichiMode ? "取消立直" : "立直"}
            {hintAction?.type === "riichi" && <span className="hint-dot" />}
          </button>
        )}
        {buttons.map((a) => (
          <button key={a.index} className={`act ${a.type} ${hint === a.index ? "hinted" : ""}`} onClick={() => onAct(a)}>
            <TileText text={a.label} />
          </button>
        ))}
        {riichiMode && <span className="note">選擇立直要打的牌</span>}
      </div>
      <div className={`my-hand ${me.revealed ? "revealed" : ""}`}>
        <span className="hand-tiles">{me.hand.map((p, i) => tile(p, false, i))}</span>
        {me.drawn && <span className="drawn">{tile(me.drawn, true, "drawn")}</span>}
        <span className="melds">
          {me.melds.map((m, i) => (
            <Meld key={i} meld={m} seat={table.me} />
          ))}
        </span>
      </div>
    </div>
  );
}

function HintPanel({ hint }: { hint: NonNullable<Ask["hint"]> }) {
  return (
    <div className="panel hint">
      <h3>
        提示：<TileText text={hint.label} />
      </h3>
      {hint.decisions.map((d, i) => (
        <div key={i} className="decision">
          <div className="kind">
            【{d.kind}】<TileText text={d.options[d.chosen] ?? ""} />
          </div>
          <ul>
            {d.notes.map((n, j) => (
              <li key={j}>
                <TileText text={n} />
              </li>
            ))}
          </ul>
        </div>
      ))}
      {hint.efficiency.length > 0 && (
        <details className="efficiency">
          <summary>牌效參考</summary>
          <table>
            <tbody>
              {hint.efficiency.map((e) => (
                <tr key={e.pai}>
                  <td>
                    打 <Tile pai={e.pai} />
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
        </details>
      )}
    </div>
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
  onReview,
  onQuit,
}: {
  over: GameOver;
  names: string[];
  onReview: () => void;
  onQuit: () => void;
}) {
  const order = [0, 1, 2, 3].sort((a, b) => over.ranks[a] - over.ranks[b]);
  const download = async () => {
    const text = await call<string>("log");
    const blob = new Blob([text], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `mahjong-${new Date().toISOString().slice(0, 19).replace(/[:T]/g, "")}.mjson`;
    a.click();
    URL.revokeObjectURL(a.href);
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
          <button className="primary" onClick={onReview}>
            復盤
          </button>
          <button onClick={download}>下載牌譜</button>
          <button onClick={onQuit}>新對局</button>
        </div>
      </div>
    </div>
  );
}
