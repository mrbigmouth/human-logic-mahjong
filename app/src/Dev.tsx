// 開發模式（M16 B，`mj dev`）：顯示重建狀態與編譯錯誤；服務重啟（或前端重建）後通知 App 重新整理並回到原本的畫面。
// 不是 `mj dev` 時（一般的 `mj serve`、桌面版）第一次查詢就停止。

import { useEffect, useRef, useState } from "react";
import { call } from "./api";

interface DevStatus {
  state: "ready" | "building" | "failed";
  target: "rust" | "frontend";
  changed: string[];
  version: number;
  time: number;
  started?: number;
  build_secs?: number;
  error?: string;
}

interface DevInfo {
  dev: boolean;
  boot: string;
  status: DevStatus | null;
}

/** 檔名只留專案中的相對路徑末段，例如 hand-reading/decision/discard.rs。 */
function shortPath(p: string): string {
  const parts = p.split(/[\\/]/);
  return parts.slice(-3).join("/");
}

export function DevBanner({ onRestart }: { onRestart: () => void }) {
  const [info, setInfo] = useState<DevInfo | null>(null);
  const [down, setDown] = useState(false);
  const [open, setOpen] = useState(false);
  const first = useRef<{ boot: string; version: number } | null>(null);
  const [notice] = useState(() => {
    try {
      const n = sessionStorage.getItem("devNotice");
      sessionStorage.removeItem("devNotice");
      return n;
    } catch {
      return null;
    }
  });
  const [showNotice, setShowNotice] = useState(notice !== null);

  useEffect(() => {
    let stop = false;
    let timer: ReturnType<typeof setTimeout>;
    const tick = async () => {
      try {
        const d = await call<DevInfo>("dev_status");
        if (stop) return;
        if (!d.dev) return; // 不是 mj dev：不再查詢
        setDown(false);
        setInfo(d);
        const version = d.status?.version ?? 0;
        if (first.current === null) {
          first.current = { boot: d.boot, version };
        } else if (d.boot !== first.current.boot || (version !== first.current.version && d.status?.state === "ready")) {
          // 服務重啟過或前端重建了：重新整理。
          try {
            const secs = d.status?.build_secs;
            sessionStorage.setItem("devNotice", `已重建${secs ? `（${secs.toFixed(1)} 秒）` : ""}並回到原本的畫面`);
          } catch {
            // 忽略
          }
          onRestart();
          return;
        }
      } catch {
        if (stop) return;
        // 第一次就失敗：不是 mj serve（例如桌面版沒有這個 API），停止。
        if (first.current === null) return;
        setDown(true);
      }
      timer = setTimeout(tick, 1000);
    };
    void tick();
    return () => {
      stop = true;
      clearTimeout(timer);
    };
  }, [onRestart]);

  useEffect(() => {
    if (!showNotice) return;
    const t = setTimeout(() => setShowNotice(false), 4000);
    return () => clearTimeout(t);
  }, [showNotice]);

  if (showNotice && notice) return <div className="dev-banner ok">{notice}</div>;
  if (down) return <div className="dev-banner building">服務重新啟動中…</div>;
  const s = info?.status;
  if (!info?.dev || !s) return null;
  const changed = s.changed.map(shortPath).slice(0, 3).join("、") + (s.changed.length > 3 ? ` 等 ${s.changed.length} 個檔案` : "");
  if (s.state === "building") {
    return (
      <div className="dev-banner building">
        {s.target === "frontend" ? "前端" : "策略／核心"}有變更{changed && `（${changed}）`}，重建中…
      </div>
    );
  }
  if (s.state === "failed") {
    return (
      <div className="dev-banner failed">
        <div className="dev-head">
          {s.target === "frontend" ? "前端" : ""}建置失敗{changed && `（${changed}）`}：目前執行的仍是舊版。修正後存檔會自動重試。
          <button className="small" onClick={() => setOpen(!open)}>
            {open ? "收起" : "顯示錯誤"}
          </button>
        </div>
        {open && <pre className="dev-error">{s.error}</pre>}
      </div>
    );
  }
  return null;
}
