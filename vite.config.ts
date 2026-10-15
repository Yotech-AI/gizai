import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed dev port; production builds are served by Tauri's custom protocol.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] } },
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
});
