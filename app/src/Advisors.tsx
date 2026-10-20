// 顧問（M14.5）：提示與復盤時列出選擇的策略或 AI（最多三個）。
// 選單的選項標上最後更新時間（核心已由新至舊排序）；AI 的建議只顯示各動作的機率。

import { TileText } from "./Tile";
import type { Policy, Prob, StrategyInfo } from "./types";

export const MAX_ADVISORS = 3;

/** 最後更新時間：今年的省略年份。 */
export function formatUpdated(ts: number | null): string {
  if (ts === null) return "";
  const d = new Date(ts * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  const md = `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
  return d.getFullYear() === new Date().getFullYear() ? md : `${d.getFullYear()}-${md}`;
}

/** 選單中的名稱：標題（策略短名）・更新時間。 */
export function optionLabel(c: StrategyInfo): string {
  // 參數版本的名稱是「策略@參數檔路徑」：短名用原本的策略。
  const short = c.kind === "strategy" ? `（${(c.base ?? c.name).split("/").pop()}）` : "";
  const when = c.updated !== null ? `　${c.kind === "mjai" ? "權重 " : ""}${formatUpdated(c.updated)}` : "";
  return `${c.title}${short}${when}`;
}

interface PickerProps {
  /** 可以當顧問的：策略與 AI（不含內建機器人） */
  choices: StrategyInfo[];
  value: string[];
  onChange: (v: string[]) => void;
}

/** 選 1–3 個顧問：第一個必選（主要的），第二、三個可以不選。 */
export function AdvisorPicker({ choices, value, onChange }: PickerProps) {
  const known = (n: string) => choices.some((c) => c.name === n);
  const slots = Array.from({ length: MAX_ADVISORS }, (_, i) => value[i] ?? "");
  const set = (i: number, name: string) => {
    const next = [...slots];
    next[i] = name;
    // 去掉空的與重複的，保持順序。
    onChange(next.filter((n, j) => n !== "" && next.indexOf(n) === j));
  };
  return (
    <div className="advisor-picker">
      {slots.map((v, i) =>
        i > value.length ? null : (
          <select key={i} value={known(v) ? v : ""} onChange={(e) => set(i, e.target.value)}>
            {i > 0 && <option value="">（不使用）</option>}
            {i === 0 && !known(v) && <option value="">（選擇）</option>}
            {choices.map((c) => (
              <option key={c.name} value={c.name} disabled={slots.includes(c.name) && c.name !== v}>
                {optionLabel(c)}
              </option>
            ))}
          </select>
        ),
      )}
    </div>
  );
}

function Rows({ probs, limit }: { probs: Prob[]; limit: number }) {
  return (
    <>
      {probs.slice(0, limit).map((p, i) => (
        <div key={i} className="prob-row">
          <span className="prob-label">
            <TileText text={p.label} />
          </span>
          <span className="prob-bar">
            <span style={{ width: `${Math.max(1, p.prob * 100)}%` }} />
          </span>
          <span className="prob-num">{(p.prob * 100).toFixed(1)}%</span>
        </div>
      ))}
    </>
  );
}

/** AI 各動作的機率（由高到低）；選了立直時另列立直後打哪張。 */
export function PolicyList({ policy, limit = 6 }: { policy: Policy | null; limit?: number }) {
  if (!policy || policy.probs.length === 0) return <div className="muted">只有一個選擇（沒有機率）</div>;
  return (
    <div className="policy">
      <Rows probs={policy.probs} limit={limit} />
      {policy.riichi.length > 0 && (
        <>
          <div className="muted">立直後打：</div>
          <Rows probs={policy.riichi} limit={3} />
        </>
      )}
    </div>
  );
}
