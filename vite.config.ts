import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed dev port; production builds are served by Tauri's custom protocol.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] } },
  // Desktop app: local files, so a large chunk costs nothing over the network; what counts is the code a start parses.
  // GA-178: the pages but the board and the self-test load when first needed (App.tsx); GA-190: the drawers too
  // (lib/drawers.tsx), and with them the Markdown editor and CodeMirror. What a start needs stays in one file instead of
  // many small ones.
  build: {
    target: "safari16", outDir: "dist", chunkSizeWarningLimit: 2000,
    rolldownOptions: { output: { codeSplitting: { groups: [
      { name: "index", tags: ["$initial"], priority: 1 },
    ] } } },
  },
});
