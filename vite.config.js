import { readFileSync } from "node:fs";
import { defineConfig } from "vite";

// The version the About pane shows comes from package.json, so there is only
// ever one place to change it.
const pkg = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8"));

// Tauri serves the frontend from a fixed port in development and from the
// bundle in release, so the dev server must not wander to another port.
export default defineConfig({
  clearScreen: false,
  define: { __APP_VERSION__: JSON.stringify(pkg.version) },
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: ["safari15", "chrome105"],
    sourcemap: false,
    chunkSizeWarningLimit: 2048,
  },
});
