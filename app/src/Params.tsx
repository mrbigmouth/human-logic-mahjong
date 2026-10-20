// 策略參數（M16 A）：編輯策略的可調參數，存成「參數版本」。不必編譯，存檔後新開的對局、提示、測試就會使用；
// 版本會列在所有策略選單中（標題後加「@版本名稱」）。參數檔的格式與 `mj params`、`mj tune` 相同。

import { useEffect, useMemo, useState } from "react";
import { formatUpdated, optionLabel } from "./Advisors";
import { call } from "./api";
import type { StrategyInfo } from "./types";

interface ParamInfo {
  name: string;
  doc: string;
  default: number;
  min: number;
  max: number;
}

interface Variant {
  strategy: string;
  name: string;
  spec: string;
  values: Record<string, number>;
  updated: number | null;
}

const DEFAULT = "tenhou-normal-4p-hanchan/hand-reading";

/** 參數檔文字（每行「名稱 = 值」，# 之後是註解）→ 值。 */
function parseParams(text: string): Record<string, number> {
  const out: Record<string, number> = {};
  for (const raw of text.split(/[\n,;]/)) {
    const line = raw.split("#")[0].trim();
    if (!line) continue;
    const [k, v] = line.split("=").map((x) => x.trim());
    if (!k || v === undefined || !Number.isFinite(Number(v))) throw new Error(`「${line}」應寫成 名稱 = 數字`);
    out[k] = Number(v);
  }
  return out;
}

/** 數值的步距：依範圍大小。 */
function stepOf(p: ParamInfo): number {
  const span = Math.abs(p.max - p.min);
  if (Number.isInteger(p.default) && Number.isInteger(p.min) && Number.isInteger(p.max) && span >= 2) return 1;
  return span > 0 ? Math.pow(10, Math.floor(Math.log10(span)) - 2) : 0.01;
}

export function Params({ onBack }: { onBack: () => void }) {
  const [strategies, setStrategies] = useState<StrategyInfo[]>([]);
  const [strategy, setStrategy] = useState(() => localStorage.getItem("paramsStrategy") ?? DEFAULT);
  const [params, setParams] = useState<ParamInfo[]>([]);
  const [variants, setVariants] = useState<Variant[]>([]);
  /** 正在編輯的版本（"" = 新版本） */
  const [current, setCurrent] = useState("");
  const [name, setName] = useState("");
  const [values, setValues] = useState<Record<string, number>>({});
  const [filter, setFilter] = useState("");
  const [onlyChanged, setOnlyChanged] = useState(false);
  const [importText, setImportText] = useState<string | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    call<StrategyInfo[]>("strategies").then((s) => setStrategies(s.filter((x) => !x.name.includes("@"))));
  }, []);

  const reload = async (keep?: string) => {
    const r = await call<{ params: ParamInfo[]; variants: Variant[] }>("params", { strategy });
    setParams(r.params);
    setVariants(r.variants);
    const v = r.variants.find((x) => x.name === keep);
    setCurrent(v ? v.name : "");
    setName(v ? v.name : "");
    setValues(v ? { ...v.values } : {});
  };
  useEffect(() => {
    try {
      localStorage.setItem("paramsStrategy", strategy);
    } catch {
      // 忽略
    }
    setMessage(null);
    reload().catch((e) => setMessage({ ok: false, text: String(e) }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [strategy]);

  const saved = variants.find((v) => v.name === current);
  const dirty = JSON.stringify(values) !== JSON.stringify(saved?.values ?? {});
  const shown = useMemo(
    () =>
      params.filter(
        (p) =>
          (!onlyChanged || values[p.name] !== undefined) &&
          (filter === "" || p.name.includes(filter) || p.doc.includes(filter)),
      ),
    [params, values, filter, onlyChanged],
  );

  const pick = (n: string) => {
    if (dirty && !window.confirm("目前的修改還沒儲存，要放棄嗎？")) return;
    const v = variants.find((x) => x.name === n);
    setCurrent(v ? v.name : "");
    setName(v ? v.name : "");
    setValues(v ? { ...v.values } : {});
    setMessage(null);
  };

  const set = (p: ParamInfo, raw: string) => {
    const x = Number(raw);
    const next = { ...values };
    if (raw.trim() === "" || !Number.isFinite(x) || x === p.default) delete next[p.name];
    else next[p.name] = x;
    setValues(next);
  };

  const save = async () => {
    try {
      const v = await call<Variant>("save_params", { strategy, variant: name.trim(), values });
      await reload(v.name);
      setMessage({ ok: true, text: `已儲存「${v.name}」：新開的對局與測試立即使用。參賽者名稱 ${v.spec}` });
    } catch (e) {
      setMessage({ ok: false, text: String(e) });
    }
  };

  const remove = async () => {
    if (!saved || !window.confirm(`刪除版本「${saved.name}」？`)) return;
    try {
      await call("delete_params", { strategy, variant: saved.name });
      await reload();
      setMessage({ ok: true, text: `已刪除「${saved.name}」` });
    } catch (e) {
      setMessage({ ok: false, text: String(e) });
    }
  };

  const doImport = () => {
    try {
      const parsed = parseParams(importText ?? "");
      const unknown = Object.keys(parsed).filter((k) => !params.some((p) => p.name === k));
      if (unknown.length > 0) throw new Error(`這個策略沒有參數：${unknown.join("、")}`);
      setValues({ ...values, ...parsed });
      setImportText(null);
      setMessage({ ok: true, text: `匯入 ${Object.keys(parsed).length} 個參數（還沒儲存）` });
    } catch (e) {
      setMessage({ ok: false, text: String(e) });
    }
  };

  const fileText = Object.entries(values)
    .map(([k, v]) => `${k} = ${v}`)
    .join("\n");

  return (
    <div className="setup params-page">
      <div className="review-head">
        <h1>策略參數</h1>
        <button onClick={() => (!dirty || window.confirm("目前的修改還沒儲存，要離開嗎？")) && onBack()}>返回</button>
      </div>
      <p className="lead">
        調整策略的門檻與係數，存成「參數版本」：不必編譯，儲存後新開的對局、提示與對戰測試立即使用。
        版本會出現在所有策略選單中（標題後加「@版本名稱」）。要改判斷的邏輯，請改策略的原始碼（用 <code>mj dev</code> 會自動重建）。
      </p>
      <div className="form">
        <label>
          策略
          <select value={strategy} onChange={(e) => setStrategy(e.target.value)}>
            {strategies.map((s) => (
              <option key={s.name} value={s.name}>
                {optionLabel(s)}
              </option>
            ))}
          </select>
        </label>
        <label>
          版本
          <select value={current} onChange={(e) => pick(e.target.value)}>
            <option value="">（新版本，從預設值開始）</option>
            {variants.map((v) => (
              <option key={v.name} value={v.name}>
                @{v.name}　{Object.keys(v.values).length} 個覆寫　{formatUpdated(v.updated)}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="buttons params-filter">
        <input placeholder="搜尋參數名稱或說明" value={filter} onChange={(e) => setFilter(e.target.value)} />
        <label className="check">
          <input type="checkbox" checked={onlyChanged} onChange={(e) => setOnlyChanged(e.target.checked)} />
          只看改過的（{Object.keys(values).length}）
        </label>
        <button onClick={() => setValues({})} disabled={Object.keys(values).length === 0}>
          全部重設為預設值
        </button>
        <button onClick={() => setImportText(importText === null ? fileText : null)}>匯入／匯出參數檔</button>
      </div>
      {importText !== null && (
        <div className="panel import">
          <p className="muted">
            每行一個「名稱 = 值」（與 <code>mj params</code>、<code>mj tune</code> 的輸出相同）。貼上後按「套用」；要匯出就複製這裡的文字。
          </p>
          <textarea rows={8} value={importText} onChange={(e) => setImportText(e.target.value)} />
          <div className="buttons">
            <button className="primary" onClick={doImport}>
              套用
            </button>
            <button onClick={() => setImportText(null)}>關閉</button>
          </div>
        </div>
      )}
      <table className="summary params-table">
        <thead>
          <tr>
            <th>參數</th>
            <th>預設</th>
            <th>範圍</th>
            <th>值</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {shown.map((p) => {
            const v = values[p.name] ?? p.default;
            const changed = values[p.name] !== undefined;
            const out = v < p.min || v > p.max;
            return (
              <tr key={p.name} className={changed ? "changed" : ""}>
                <td>
                  <div className="param-name">{p.name}</div>
                  <div className="muted">{p.doc}</div>
                </td>
                <td>{p.default}</td>
                <td className="muted">
                  {p.min}–{p.max}
                </td>
                <td className="param-input">
                  <input
                    type="range"
                    min={Math.min(p.min, v)}
                    max={Math.max(p.max, v)}
                    step={stepOf(p)}
                    value={v}
                    onChange={(e) => set(p, e.target.value)}
                  />
                  <input
                    type="number"
                    step={stepOf(p)}
                    value={v}
                    className={out ? "out" : ""}
                    title={out ? "超出建議範圍（mj tune 只在範圍內搜尋）" : ""}
                    onChange={(e) => set(p, e.target.value)}
                  />
                </td>
                <td>
                  {changed && (
                    <button className="small" onClick={() => set(p, String(p.default))}>
                      重設
                    </button>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {message && <div className={message.ok ? "notice" : "error"}>{message.text}</div>}
      <div className="buttons params-save">
        <label>
          版本名稱
          <input value={name} placeholder="例如 aggressive" onChange={(e) => setName(e.target.value)} />
        </label>
        <button className="primary" onClick={save} disabled={name.trim() === "" || (!dirty && name === current)}>
          {name.trim() !== "" && name.trim() !== current && variants.some((v) => v.name === name.trim())
            ? "覆蓋這個版本"
            : name.trim() === current && current !== ""
              ? "儲存"
              : "另存新版本"}
        </button>
        {saved && <button onClick={remove}>刪除這個版本</button>}
        {dirty && <span className="muted">有尚未儲存的修改</span>}
      </div>
    </div>
  );
}
