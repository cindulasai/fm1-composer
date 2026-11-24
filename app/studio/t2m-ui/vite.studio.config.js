// COMPOSER Studio's build of text2midi's window (its files unchanged): the two Tauri imports point at the browser
// stand-ins in studio/, and the result goes to ../web/t2m/.
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";

const here = (p) => fileURLToPath(new URL(p, import.meta.url));

// In the browser the key lives in local storage, not a system keychain: Settings.svelte's wording, rewritten in the
// build only (its file stays text2midi's). A string not found fails the build, so a text2midi update can't skip one.
const KEY_WORDS = [
  ["Your key is kept in your system keychain, never in a file. Only the words you type in the description box are sent to the provider.",
   "Your key is kept in this browser (its local storage), never in a file or sent to us. Only the words you type in the description box are sent to the provider."],
  ["Saved in your system keychain (type to replace)", "Saved in this browser (type to replace)"],
  ["The key was removed from the system keychain.", "The key was removed from this browser."],
  ["Removes this provider's key from the system keychain", "Removes this provider's key from this browser"],
];
const browserKeyWords = {
  name: "studio-browser-key-words",
  enforce: "pre",
  transform(code, id) {
    if (!id.endsWith("/src/Settings.svelte")) return null;                 // (not its ?svelte&type=style part)
    for (const [from, to] of KEY_WORDS) {
      if (!code.includes(from)) this.error(`Settings.svelte no longer says "${from}": update vite.studio.config.js`);
      code = code.split(from).join(to);
    }
    if (/keychain/i.test(code)) this.error("Settings.svelte still mentions a keychain: update vite.studio.config.js");
    return { code, map: null };
  },
};

export default defineConfig({
  root: here("."),
  base: "./",
  plugins: [browserKeyWords, svelte()],
  resolve: { alias: { "@tauri-apps/api/core": here("studio/core.js"), "@tauri-apps/api/event": here("studio/event.js") } },
  build: { outDir: here("../web/t2m"), emptyOutDir: true },
  logLevel: "warn",
});
