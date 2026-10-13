// 一張牌。牌面圖疊在牌底（front.svg）上；牌背用 CSS 畫。
// 滑鼠指向一張明牌時，畫面上同種的牌（赤五視為 5）都會標示（`HoverProvider` 包住的範圍）。

import { createContext, useContext, useMemo, useState, type ReactNode } from "react";

/** 牌種：赤五視為普通的 5。 */
export function kindOf(pai: string): string {
  return pai[0] === "0" ? `5${pai[1]}` : pai;
}

interface HoverState {
  hovered: string | null;
  setHovered: (kind: string | null) => void;
}

const HoverContext = createContext<HoverState>({ hovered: null, setHovered: () => {} });

/** 同種牌標示的範圍。 */
export function HoverProvider({ children }: { children: ReactNode }) {
  const [hovered, setHovered] = useState<string | null>(null);
  const value = useMemo(() => ({ hovered, setHovered }), [hovered]);
  return <HoverContext.Provider value={value}>{children}</HoverContext.Provider>;
}

interface Props {
  pai?: string | null;
  /** 顯示牌背 */
  back?: boolean;
  /** 橫放（立直宣言牌、鳴進來的牌） */
  sideways?: boolean;
  /** 變暗（被鳴走、不能打） */
  dim?: boolean;
  /** 標記：hint = 提示、last = 最後打出、chosen = 選擇 */
  mark?: "hint" | "last" | "player" | "strategy" | null;
  onClick?: () => void;
  title?: string;
  className?: string;
}

const base = "./tiles/";

export function Tile({ pai, back, sideways, dim, mark, onClick, title, className }: Props) {
  // 牌面圖載入失敗時：先加查詢字串重試一次（避開快取的失敗結果），再失敗就改顯示文字，
  // 以免空白的牌面被誤認成白。
  const [failed, setFailed] = useState<{ pai: string; tries: number } | null>(null);
  const tries = failed && failed.pai === pai ? failed.tries : 0;
  const { hovered, setHovered } = useContext(HoverContext);
  const face = back || !pai ? null : kindOf(pai);
  const classes = ["tile"];
  if (face && face === hovered) classes.push("mark-same");
  if (sideways) classes.push("sideways");
  if (dim) classes.push("dim");
  if (mark) classes.push(`mark-${mark}`);
  if (onClick) classes.push("clickable");
  if (className) classes.push(className);
  if (back || !pai) classes.push("back");
  const image = !face || !pai ? null : (
    <>
      <img className="base" src={`${base}front.svg`} alt="" draggable={false} />
      {tries < 2 ? (
        <img
          className="face"
          src={`${base}${pai}.svg${tries ? "?retry" : ""}`}
          alt=""
          draggable={false}
          onError={() => setFailed({ pai, tries: tries + 1 })}
        />
      ) : (
        <span className="face-text">{pai}</span>
      )}
    </>
  );
  return (
    <span
      className={classes.join(" ")}
      onClick={onClick}
      onMouseEnter={face ? () => setHovered(face) : undefined}
      onMouseLeave={face ? () => setHovered(null) : undefined}
      title={title ?? pai ?? undefined}
    >
      <span className="tile-inner">{image}</span>
    </span>
  );
}

/** 一排牌（例如提示中的手牌）。 */
export function Tiles({ tiles, className }: { tiles: string[]; className?: string }) {
  return (
    <span className={`tiles ${className ?? ""}`}>
      {tiles.map((p, i) => (
        <Tile key={i} pai={p} />
      ))}
    </span>
  );
}

/** 展開牌記法：「47m5p」→ 4m、7m、5p。 */
export function expandTiles(s: string): string[] {
  const out: string[] = [];
  for (const [, digits, suit] of s.matchAll(/([0-9]+)([mpsz])/g)) {
    for (const d of digits) out.push(d + suit);
  }
  return out;
}

/** 把一段文字中的牌記法（例如「打 5m」「聽 47m」）換成牌圖。 */
export function TileText({ text }: { text: string }) {
  // 前後不能接英數字，避免把一般數字或英文當成牌。
  const parts = text.split(/(?<![A-Za-z0-9])((?:[0-9]{1,9}[mpsz])+)(?![A-Za-z0-9])/);
  return (
    <>
      {parts.map((part, i) =>
        i % 2 === 1 ? (
          <Tiles key={i} className="inline" tiles={expandTiles(part)} />
        ) : (
          <span key={i}>{part}</span>
        ),
      )}
    </>
  );
}
