import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed dev port; production builds are served by Tauri's custom protocol.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] } },
  // Desktop app: one local bundle, so a large chunk costs nothing over the network.
  build: { target: "safari16", outDir: "dist", chunkSizeWarningLimit: 2000 },
});
