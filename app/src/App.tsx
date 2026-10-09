// 畫面切換：設定 → 對局 → 復盤。

import { useState } from "react";
import { Game } from "./Game";
import { Review } from "./Review";
import { Setup } from "./Setup";
import type { Frame, NewGame } from "./types";

type Screen =
  | { kind: "setup" }
  | { kind: "game"; game: NewGame }
  | { kind: "review"; game: NewGame; frames: Frame[]; askFrames: Map<number, number> };

export function App() {
  const [screen, setScreen] = useState<Screen>({ kind: "setup" });
  const quit = () => setScreen({ kind: "setup" });
  switch (screen.kind) {
    case "setup":
      return <Setup onStart={(game) => setScreen({ kind: "game", game })} />;
    case "game":
      return (
        <Game
          key={screen.game.seed}
          game={screen.game}
          onReview={(frames, askFrames) => setScreen({ kind: "review", game: screen.game, frames, askFrames })}
          onQuit={quit}
        />
      );
    case "review":
      return <Review game={screen.game} frames={screen.frames} askFrames={screen.askFrames} onQuit={quit} />;
  }
}
