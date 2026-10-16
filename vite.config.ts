import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

const watch = { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] };

/** GA-185: in the browser mode the address without a page is browser.html, GizAi's screens with the fake backend. */
const browserPage = (): Plugin => ({
  name: "gizai-browser-page",
  configureServer(server) {
    server.middlewares.use((req, _res, next) => {
      if (req.url === "/" || req.url?.startsWith("/?")) req.url = `/browser.html${req.url.slice(1)}`;
      next();
    });
  },
});

// Tauri expects a fixed dev port; production builds are served by Tauri's custom protocol.
// `vite --mode browser` (npm run browser, GA-185): GizAi's screens in a plain browser with a fake backend (browser.html,
// src/browser/), on 127.0.0.1 at the port a run's preview gets in GIZAI_PREVIEW_PORT (1430 without it). The build below
// has only index.html, so the app never contains it.
export default defineConfig(({ mode }) => ({
  plugins: mode === "browser" ? [react(), browserPage()] : [react()],
  clearScreen: false,
  server: mode === "browser"
    ? { host: "127.0.0.1", port: Number(process.env.GIZAI_PREVIEW_PORT) || 1430, strictPort: true, watch }
    : { port: 1420, strictPort: true, watch },
  // Desktop app: local files, so a large chunk costs nothing over the network; what counts is the code a start parses.
  // GA-178: the pages but the board and the self-test load when first needed (App.tsx). What a start needs stays in two
  // files instead of many small ones: CodeMirror's part (the drawers' Markdown editor) and the rest.
  build: {
    target: "safari16", outDir: "dist", chunkSizeWarningLimit: 2000,
    rolldownOptions: { output: { codeSplitting: { groups: [
      { name: "codemirror", tags: ["$initial"], test: /node_modules[\\/](@codemirror|@lezer|@marijn|style-mod|w3c-keyname|crelt)[\\/]/, priority: 2 },
      { name: "index", tags: ["$initial"], priority: 1 },
    ] } } },
  },
}));
