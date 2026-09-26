import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";
import tauriConfig from "./src-tauri/tauri.conf.json";

const devUrl = new URL(tauriConfig.build.devUrl);

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    host: devUrl.hostname,
    port: Number(devUrl.port),
    strictPort: false,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "es2022",
    outDir: "dist",
  },
  test: {
    environment: "jsdom",
    globals: true,
    css: true,
    setupFiles: "./src/test/setup.ts",
  },
});
