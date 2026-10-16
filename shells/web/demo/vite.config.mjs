import { defineConfig } from "vite";
// demo 目錄 = root；build 產物到 demo/dist（size 閘門查 chunk 拆分真實性）。
// target es2022：top-level await（wasm 載入需要）；wasm32 本來就只跑現代瀏覽器。
export default defineConfig({
  root: ".",
  base: "./",
  server: { port: 5199 },
  build: { outDir: "dist", emptyOutDir: true, target: "es2022" },
});
