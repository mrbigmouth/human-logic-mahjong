// 牌桌：中央的局況、四家的牌河、對手的手牌與副露。自己的手牌畫在牌桌下方（見 Hand）。
// 每一家畫成「坐在下方」的樣子，再以牌桌中心旋轉到自己的位置（下家在右、對家在上、上家在左）。

import { Tile } from "./Tile";
import type { MeldView, RiverTile, Seat, SeatView, TableView } from "./types";

export const WINDS = ["東", "南", "西", "北"];

/** 相對位置：0 = 自己、1 = 下家、2 = 對家、3 = 上家。 */
export function relative(me: Seat, seat: number): number {
  return (seat - me + 4) % 4;
}

/** 參賽者的短名稱：策略只取「規則／策略」的策略部分。 */
export function shortName(name: string): string {
  if (name.startsWith("mjai:")) {
    const last = name.trim().split(/\s+/).pop() ?? name;
    return name.includes("mortal") ? `Mortal ${last}` : last;
  }
  return name.split("/").pop() ?? name;
}

export const REL_NAMES = ["你", "下家", "對家", "上家"];

export function Meld({ meld, seat }: { meld: MeldView; seat: number }) {
  if (meld.kind === "ankan") {
    return (
      <span className="meld">
        <Tile back />
        <Tile pai={meld.tiles[1]} />
        <Tile pai={meld.tiles[2]} />
        <Tile back />
      </span>
    );
  }
  // 鳴進來的牌橫放，位置表示來源：上家在左、對家在中、下家在右。
  const others = [...meld.tiles];
  const called = meld.called;
  if (called) {
    const i = others.indexOf(called);
    if (i >= 0) others.splice(i, 1);
  }
  let added: string | null = null;
  if (meld.kind === "kakan") {
    added = others.pop() ?? null;
  }
  const from = meld.from === null ? 3 : relative(seat as Seat, meld.from);
  const at = meld.kind === "chi" || from === 3 ? 0 : from === 2 ? 1 : others.length;
  const items = others.map((p, i) => <Tile key={i} pai={p} />);
  const side = (
    <span key="called" className="called-stack">
      {added && <Tile pai={added} sideways />}
      <Tile pai={called} sideways />
    </span>
  );
  items.splice(at, 0, side);
  return <span className="meld">{items}</span>;
}

function River({ river, last }: { river: RiverTile[]; last: number | null }) {
  const rows: RiverTile[][] = [];
  for (let i = 0; i < river.length; i += 6) {
    rows.push(river.slice(i, i + 6));
  }
  // 第三排之後不換行（與實際牌桌相同）。
  const shown = rows.length > 3 ? [...rows.slice(0, 2), rows.slice(2).flat()] : rows;
  let index = 0;
  return (
    <div className="river">
      {shown.map((row, r) => (
        <div className="river-row" key={r}>
          {row.map((t) => {
            const i = index++;
            return (
              <Tile
                key={i}
                pai={t.pai}
                sideways={t.riichi}
                dim={t.called}
                mark={i === last ? "last" : null}
                className={t.tsumogiri ? "tsumogiri" : undefined}
                title={`${t.pai}${t.tsumogiri ? "（摸切）" : "（手切）"}`}
              />
            );
          })}
        </div>
      ))}
    </div>
  );
}

function OpponentHand({ seat }: { seat: SeatView }) {
  if (seat.revealed) {
    return (
      <div className="opp-hand revealed">
        {seat.hand.map((p, i) => (
          <Tile key={i} pai={p} />
        ))}
      </div>
    );
  }
  const n = seat.hand_count - (seat.has_drawn ? 1 : 0);
  return (
    <div className="opp-hand">
      {Array.from({ length: Math.max(n, 0) }, (_, i) => (
        <Tile key={i} back />
      ))}
      {seat.has_drawn && <Tile back className="drawn" />}
    </div>
  );
}

interface BoardProps {
  table: TableView;
  names: string[];
}

export function Board({ table, names }: BoardProps) {
  const me = table.me;
  return (
    <div className="board">
      {table.seats.map((seat, s) => {
        const rel = relative(me, s);
        const last = table.last_discard && table.last_discard[0] === s ? table.last_discard[1] : null;
        return (
          <div key={s} className={`layer rel-${rel}`} style={{ transform: `rotate(${-90 * rel}deg)` }}>
            <div className={`seat-label ${table.turn === s ? "turn" : ""}`}>
              <span className="wind">{WINDS[seat.wind]}</span>
              <span className="score">{table.scores[s]}</span>
            </div>
            {seat.riichi && <span className="riichi-stick" title="立直" />}
            <River river={seat.river} last={last} />
            {rel !== 0 && (
              <div className="opp-area">
                <span className="name" title={names[s]}>
                  {shortName(names[s])}
                </span>
                <OpponentHand seat={seat} />
                <div className="melds">
                  {seat.melds.map((m, i) => (
                    <Meld key={i} meld={m} seat={s} />
                  ))}
                </div>
              </div>
            )}
          </div>
        );
      })}
      <div className="center">
        <div className="round">{table.round.split(" ")[0]}</div>
        <div className="sticks">
          <span title="本場">{table.honba} 本場</span>
          <span title="供託">供託 {table.riichi_sticks}</span>
        </div>
        <div className="wall">餘 {table.wall_left}</div>
        <div className="dora">
          {Array.from({ length: 5 }, (_, i) =>
            i < table.dora_indicators.length ? (
              <Tile key={i} pai={table.dora_indicators[i]} />
            ) : (
              <Tile key={i} back />
            ),
          )}
        </div>
      </div>
    </div>
  );
}
