// 畫面切換：設定 → 對局 → 牌譜（檢視器、復盤）；設定 → 對戰測試 → 測試結果 → 某一場的牌譜；設定 → 策略參數。
// 剛打完的對局、從檔案讀入的牌譜、測試中的某一場都走同一條路：先讀入核心，再檢視或復盤。
// 牌譜畫面記得從哪裡來（`back`），返回時回到測試結果；從測試打開的另記 `origin`（重啟後重新打開用）。
//
// 開發模式（`mj dev`，M16）：服務重啟後重新整理頁面，並回到原本的畫面：
// 目前的畫面存在 sessionStorage；進行中的對局用核心保存的動作接續（`resume_game`），
// 測試中的對局重新打開，從檔案讀入的牌譜則要重新讀入（核心重啟後就不在了）。

import { useCallback, useEffect, useState } from "react";
import { call } from "./api";
import { DevBanner } from "./Dev";
import { Game } from "./Game";
import { LogScreen } from "./LogScreen";
import { Params } from "./Params";
import { Review } from "./Review";
import { Setup } from "./Setup";
import { TestResultView, Tests } from "./Tests";
import { Viewer } from "./Viewer";
import type { LogInfo, NewGame, Seat } from "./types";

/** 從對戰測試打開的牌譜：哪個測試的第幾場。 */
interface Origin {
  test: string;
  index: number;
}

type Screen =
  | { kind: "setup" }
  | { kind: "game"; game: NewGame }
  | { kind: "log"; info: LogInfo; back?: Screen; origin?: Origin }
  | { kind: "viewer"; info: LogInfo; seat: Seat; open: boolean; frame?: number; back?: Screen; origin?: Origin }
  | { kind: "review"; info: LogInfo; seat: Seat; back?: Screen; origin?: Origin }
  | { kind: "tests" }
  | { kind: "test"; id: string }
  | { kind: "params" };

const SCREEN_KEY = "screen";
const RESTORE_KEY = "devRestore";

/** 開發模式重新整理後：回到原本的畫面（需要時向核心重建狀態）。 */
async function restore(saved: Screen): Promise<{ screen: Screen; note?: string }> {
  switch (saved.kind) {
    case "game": {
      const g = await call<{ seed: number } | null>("saved_game");
      if (g && g.seed === saved.game.seed) {
        return { screen: { kind: "game", game: await call<NewGame>("resume_game") } };
      }
      return { screen: { kind: "setup" }, note: "對局已經結束（或沒有保存），回到設定畫面。" };
    }
    case "log":
    case "viewer":
    case "review": {
      if (!saved.origin) {
        return { screen: { kind: "setup" }, note: "服務重新啟動了：讀入的牌譜要重新讀入。" };
      }
      const info = await call<LogInfo>("open_test_game", { id: saved.origin.test, index: saved.origin.index });
      return { screen: { ...saved, info } };
    }
    default:
      return { screen: saved };
  }
}

export function App() {
  const [screen, setScreen] = useState<Screen | null>(() => {
    try {
      if (sessionStorage.getItem(RESTORE_KEY) === null) return { kind: "setup" };
    } catch {
      return { kind: "setup" };
    }
    return null; // 還原中
  });
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const quit = () => setScreen({ kind: "setup" });

  // 開發模式重新整理後還原畫面。
  useEffect(() => {
    if (screen !== null) return;
    let saved: Screen = { kind: "setup" };
    try {
      sessionStorage.removeItem(RESTORE_KEY);
      saved = JSON.parse(sessionStorage.getItem(SCREEN_KEY) ?? "null") ?? saved;
    } catch {
      // 忽略
    }
    restore(saved).then(
      (r) => {
        setScreen(r.screen);
        if (r.note) setNote(r.note);
      },
      (e) => {
        setScreen({ kind: "setup" });
        setError(`無法回到原本的畫面：${String(e)}`);
      },
    );
  }, [screen]);

  // 記下目前的畫面（開發模式重新整理時用）。
  useEffect(() => {
    if (screen === null) return;
    try {
      sessionStorage.setItem(SCREEN_KEY, JSON.stringify(screen));
    } catch {
      // 忽略
    }
  }, [screen]);

  const onRestart = useCallback(() => {
    try {
      sessionStorage.setItem(RESTORE_KEY, "1");
    } catch {
      // 忽略
    }
    window.location.reload();
  }, []);

  /** 把剛打完的對局讀入，再復盤或檢視。 */
  const openSessionLog = async (game: NewGame, target: "review" | "view") => {
    try {
      const text = await call<string>("log");
      const info = await call<LogInfo>("load_log", { text, name: `session-${game.seed}` });
      setScreen(
        target === "review"
          ? { kind: "review", info, seat: game.seat }
          : { kind: "viewer", info, seat: game.seat, open: false },
      );
    } catch (e) {
      setError(String(e));
    }
  };

  const content = (() => {
    if (screen === null) return <div className="loading">回到原本的畫面…</div>;
    switch (screen.kind) {
      case "setup":
        return (
          <Setup
            onStart={(game) => setScreen({ kind: "game", game })}
            onLoaded={(info) => setScreen({ kind: "log", info })}
            onTests={() => setScreen({ kind: "tests" })}
            onParams={() => setScreen({ kind: "params" })}
          />
        );
      case "params":
        return <Params onBack={quit} />;
      case "tests":
        return <Tests onOpen={(id) => setScreen({ kind: "test", id })} onBack={quit} />;
      case "test":
        return (
          <TestResultView
            key={screen.id}
            id={screen.id}
            onOpenGame={(info, index) =>
              setScreen({ kind: "log", info, back: screen, origin: { test: screen.id, index } })
            }
            onBack={() => setScreen({ kind: "tests" })}
          />
        );
      case "game":
        return (
          <Game
            key={`${screen.game.seed}-${screen.game.resumed ? "r" : ""}`}
            game={screen.game}
            onOpenLog={(target) => void openSessionLog(screen.game, target)}
            onQuit={quit}
          />
        );
      case "log": {
        const keep = { back: screen.back, origin: screen.origin };
        return (
          <LogScreen
            info={screen.info}
            onView={(seat, open) => setScreen({ kind: "viewer", info: screen.info, seat, open, ...keep })}
            onReview={(seat) => setScreen({ kind: "review", info: screen.info, seat, ...keep })}
            onBack={() => setScreen(screen.back ?? { kind: "setup" })}
          />
        );
      }
      case "viewer": {
        const keep = { back: screen.back, origin: screen.origin };
        return (
          <Viewer
            info={screen.info}
            seat={screen.seat}
            open={screen.open}
            frame={screen.frame}
            onChange={(seat, open, frame) => setScreen({ ...screen, seat, open, frame })}
            onReview={(seat) => setScreen({ kind: "review", info: screen.info, seat, ...keep })}
            onBack={() => setScreen({ kind: "log", info: screen.info, ...keep })}
          />
        );
      }
      case "review": {
        const keep = { back: screen.back, origin: screen.origin };
        return (
          <Review
            key={screen.seat}
            info={screen.info}
            seat={screen.seat}
            onView={(frame) =>
              setScreen({ kind: "viewer", info: screen.info, seat: screen.seat, open: false, frame, ...keep })
            }
            onQuit={() => setScreen({ kind: "log", info: screen.info, ...keep })}
          />
        );
      }
    }
  })();

  return (
    <>
      <DevBanner onRestart={onRestart} />
      {content}
      {note && (
        <div className="dev-banner ok" onClick={() => setNote(null)}>
          {note}（點一下關閉）
        </div>
      )}
      {error && (
        <div className="modal">
          <div className="dialog">
            <h2>發生錯誤</h2>
            <p>{error}</p>
            <button onClick={() => setError(null)}>關閉</button>
          </div>
        </div>
      )}
    </>
  );
}
