import { fileURLToPath } from "node:url";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

const fromDemo = (path: string) => fileURLToPath(new URL(path, import.meta.url));

export default defineConfig({
  root: fromDemo("."),
  plugins: [svelte()],
  resolve: {
    alias: {
      "@tauri-apps/api/core": fromDemo("./stub-core.ts"),
      "@tauri-apps/api/event": fromDemo("./stub-event.ts"),
    },
  },
  server: {
    port: 1421,
    strictPort: true,
  },
});
