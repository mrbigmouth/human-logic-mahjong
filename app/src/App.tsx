// 畫面切換：設定 → 對局 → 牌譜（檢視器、復盤）。
// 剛打完的對局與從檔案讀入的牌譜走同一條路：先讀入核心（`load_log`），再檢視或復盤。

import { useState } from "react";
import { call } from "./api";
import { Game } from "./Game";
import { LogScreen } from "./LogScreen";
import { Review } from "./Review";
import { Setup } from "./Setup";
import { Viewer } from "./Viewer";
import type { LogInfo, NewGame, Seat } from "./types";

type Screen =
  | { kind: "setup" }
  | { kind: "game"; game: NewGame }
  | { kind: "log"; info: LogInfo }
  | { kind: "viewer"; info: LogInfo; seat: Seat; open: boolean; frame?: number }
  | { kind: "review"; info: LogInfo; seat: Seat };

export function App() {
  const [screen, setScreen] = useState<Screen>({ kind: "setup" });
  const [error, setError] = useState<string | null>(null);
  const quit = () => setScreen({ kind: "setup" });

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
    switch (screen.kind) {
      case "setup":
        return <Setup onStart={(game) => setScreen({ kind: "game", game })} onLoaded={(info) => setScreen({ kind: "log", info })} />;
      case "game":
        return (
          <Game
            key={screen.game.seed}
            game={screen.game}
            onOpenLog={(target) => void openSessionLog(screen.game, target)}
            onQuit={quit}
          />
        );
      case "log":
        return (
          <LogScreen
            info={screen.info}
            onView={(seat, open) => setScreen({ kind: "viewer", info: screen.info, seat, open })}
            onReview={(seat) => setScreen({ kind: "review", info: screen.info, seat })}
            onBack={quit}
          />
        );
      case "viewer":
        return (
          <Viewer
            info={screen.info}
            seat={screen.seat}
            open={screen.open}
            frame={screen.frame}
            onChange={(seat, open, frame) => setScreen({ ...screen, seat, open, frame })}
            onReview={(seat) => setScreen({ kind: "review", info: screen.info, seat })}
            onBack={() => setScreen({ kind: "log", info: screen.info })}
          />
        );
      case "review":
        return (
          <Review
            key={screen.seat}
            info={screen.info}
            seat={screen.seat}
            onView={(frame) => setScreen({ kind: "viewer", info: screen.info, seat: screen.seat, open: false, frame })}
            onQuit={() => setScreen({ kind: "log", info: screen.info })}
          />
        );
    }
  })();

  return (
    <>
      {content}
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
