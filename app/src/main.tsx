import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { HoverProvider } from "./Tile";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <HoverProvider>
      <App />
    </HoverProvider>
  </StrictMode>,
);
