// 牌譜檢視器：一打一打地看讀入的牌譜，可以切換視角（任一家），也可以公開四家的手牌。
// 幀由核心的 `log_frames` 產生（每個事件一幀），與對局中看到的牌桌相同。

import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { call } from "./api";
import { Board, REL_NAMES, WINDS, relative, shortName } from "./Board";
import { Hand } from "./Game";
import { useBoardSize } from "./layout";
import { TileText } from "./Tile";
import type { Frame, LogFrames, LogInfo, Seat } from "./types";

interface Props {
  info: LogInfo;
  seat: Seat;
  open: boolean;
  /** 一開始停在第幾幀 */
  frame?: number;
  /** 切換視角或公開手牌（傳回目前的幀，讓畫面停在同一個地方） */
  onChange: (seat: Seat, open: boolean, frame: number) => void;
  onReview: (seat: Seat) => void;
  onBack: () => void;
}

/** 值得停下來看的幀：別人摸牌（看不到牌）以外的事件。 */
function isStop(f: Frame, seat: Seat): boolean {
  return !(f.event.type === "draw" && f.event.seat !== seat) && f.event.type !== "end_kyoku";
}

export function Viewer({ info, seat, open, frame, onChange, onReview, onBack }: Props) {
  const [frames, setFrames] = useState<Frame[]>([]);
  const [pos, setPos] = useState(frame ?? 0);
  const [error, setError] = useState<string | null>(null);
  const area = useRef<HTMLDivElement>(null);
  const handArea = useRef<HTMLDivElement>(null);
  const boardSize = useBoardSize(area, handArea, frames.length > 0);

  useEffect(() => {
    call<LogFrames>("log_frames", { seat, open }).then(
      (f) => setFrames(f.frames),
      (e) => setError(String(e)),
    );
  }, [seat, open]);

  const stops = useMemo(() => frames.map((f, i) => (isStop(f, seat) ? i : -1)).filter((i) => i >= 0), [frames, seat]);
  const hands = useMemo(
    () => frames.map((f, i) => (f.event.type === "start_kyoku" ? i : -1)).filter((i) => i >= 0),
    [frames],
  );
  const clamp = (i: number) => Math.max(0, Math.min(frames.length - 1, i));
  const next = () => setPos((p) => stops.find((i) => i > p) ?? p);
  const prev = () => setPos((p) => [...stops].reverse().find((i) => i < p) ?? p);
  const nextHand = () => setPos((p) => hands.find((i) => i > p) ?? p);
  const prevHand = () =>
    setPos((p) => {
      // 在一局的開頭時回到上一局，否則回到本局開頭。
      const before = hands.filter((i) => i < p);
      return before.length > 0 ? before[before.length - 1] : 0;
    });

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
      if (e.key === "ArrowRight") next();
      else if (e.key === "ArrowLeft") prev();
      else if (e.key === "ArrowDown" || e.key === "PageDown") nextHand();
      else if (e.key === "ArrowUp" || e.key === "PageUp") prevHand();
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (error) {
    return (
      <div className="loading">
        {error} <button onClick={onBack}>返回</button>
      </div>
    );
  }
  const current = frames[clamp(pos)];
  if (!current) return <div className="loading">牌譜準備中…</div>;
  const table = current.table;
  const handIndex = hands.filter((i) => i <= pos).length - 1;
  const recent = frames.slice(Math.max(0, pos - 12), pos + 1).reverse();

  return (
    <div className="game viewer panel-open">
      <div className="table-area" ref={area} style={{ "--B": `${boardSize}px` } as CSSProperties}>
        <Board table={table} names={info.names} />
        <div className="hand-wrap" ref={handArea}>
          <Hand table={table} ask={null} riichiMode={false} setRiichiMode={() => {}} hint={null} onAct={() => {}} />
        </div>
      </div>
      <section className="bottom-panel">
        <div className="panel-bar">
          <div className="nav">
            <button onClick={prevHand} title="上一局（↑）">
              ⏮
            </button>
            <button onClick={prev} title="上一步（←）">
              ◀
            </button>
            <button onClick={next} title="下一步（→）">
              ▶
            </button>
            <button onClick={nextHand} title="下一局（↓）">
              ⏭
            </button>
            <select value={Math.max(0, handIndex)} onChange={(e) => setPos(hands[Number(e.target.value)] ?? 0)}>
              {hands.map((i, k) => (
                <option key={i} value={k}>
                  {frames[i].table.round}
                </option>
              ))}
            </select>
            <input
              className="scrub"
              type="range"
              min={0}
              max={frames.length - 1}
              value={pos}
              onChange={(e) => setPos(Number(e.target.value))}
            />
          </div>
          <div className="controls">
            <label>
              視角
              <select value={seat} onChange={(e) => onChange(Number(e.target.value) as Seat, open, pos)}>
                {info.names.map((n, s) => (
                  <option key={s} value={s}>
                    {shortName(n)}（起始{WINDS[s]}家）
                  </option>
                ))}
              </select>
            </label>
            <label>
              <input type="checkbox" checked={open} onChange={(e) => onChange(seat, e.target.checked, pos)} /> 公開手牌
            </label>
            <button onClick={() => onReview(seat)}>復盤這家</button>
            <button onClick={onBack}>返回</button>
          </div>
        </div>
        <div className="panel-body viewer-body">
          <div className="now">
            <TileText text={current.event.text} />
            <span className="muted">
              　第 {pos + 1}／{frames.length} 幀
            </span>
          </div>
          <div className="log">
            {recent.map((f, i) => (
              <div key={i} className={`log-line ev-${f.event.type}`}>
                <TileText text={f.event.text} />
                {f.auto && <span className="muted">（自動）</span>}
              </div>
            ))}
          </div>
          <div className="muted players">
            {info.names.map((n, s) => (
              <span key={s}>
                {REL_NAMES[relative(seat, s)]}：{n}
              </span>
            ))}
          </div>
        </div>
      </section>
    </div>
  );
}
