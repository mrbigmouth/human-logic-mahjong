// 讀入牌譜後的畫面：牌譜概要，選要看（或復盤）哪一家。

import { useState } from "react";
import { WINDS, shortName } from "./Board";
import type { LogInfo, Seat } from "./types";

interface Props {
  info: LogInfo;
  onView: (seat: Seat, open: boolean) => void;
  onReview: (seat: Seat) => void;
  onBack: () => void;
}

export function LogScreen({ info, onView, onReview, onBack }: Props) {
  // 本專案匯出的牌譜記載了人類的座位，預設選他。
  const [seat, setSeat] = useState<Seat>((info.meta?.seat ?? 0) as Seat);
  const [open, setOpen] = useState(false);
  const ranks = [0, 1, 2, 3].sort((a, b) => info.final_scores[b] - info.final_scores[a] || a - b);
  return (
    <div className="setup">
      <h1>牌譜</h1>
      <p className="lead">
        {info.id}　{info.hands} 局
        {info.meta?.seed != null && `　seed ${info.meta.seed}`}
        {info.meta?.strategy && `　提示策略 ${info.meta.strategy}`}
      </p>
      <table className="final log-players">
        <thead>
          <tr>
            <th></th>
            <th>名次</th>
            <th>玩家</th>
            <th>終局點數</th>
          </tr>
        </thead>
        <tbody>
          {info.names.map((n, s) => (
            <tr key={s} className={seat === s ? "me" : ""} onClick={() => setSeat(s as Seat)}>
              <td>
                <input type="radio" checked={seat === s} onChange={() => setSeat(s as Seat)} />
              </td>
              <td>{ranks.indexOf(s) + 1} 位</td>
              <td>
                {shortName(n)}
                <span className="muted">（起始{WINDS[s]}家）</span>
              </td>
              <td>{info.final_scores[s]}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <label className="check">
        <input type="checkbox" checked={open} onChange={(e) => setOpen(e.target.checked)} /> 檢視時公開四家的手牌
      </label>
      <div className="buttons">
        <button className="primary big" onClick={() => onReview(seat)}>
          復盤這家
        </button>
        <button className="big" onClick={() => onView(seat, open)}>
          檢視牌譜
        </button>
        <button onClick={onBack}>返回</button>
      </div>
    </div>
  );
}
