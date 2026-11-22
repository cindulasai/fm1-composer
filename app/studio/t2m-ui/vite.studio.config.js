// COMPOSER Studio's build of text2midi's window (its files unchanged): the two Tauri imports point at the browser
// stand-ins in studio/, and the result goes to ../web/t2m/.
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";

const here = (p) => fileURLToPath(new URL(p, import.meta.url));
export default defineConfig({
  root: here("."),
  base: "./",
  plugins: [svelte()],
  resolve: { alias: { "@tauri-apps/api/core": here("studio/core.js"), "@tauri-apps/api/event": here("studio/event.js") } },
  build: { outDir: here("../web/t2m"), emptyOutDir: true },
  logLevel: "warn",
});
