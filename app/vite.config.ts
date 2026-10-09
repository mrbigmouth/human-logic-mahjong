import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// 開發時（npm run dev，port 30101）把 /api 轉給 `mj serve`（port 30100）。
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: { "/api": "http://127.0.0.1:30100" },
  },
  // Tauri 打包時用相對路徑載入檔案。
  base: "./",
});
