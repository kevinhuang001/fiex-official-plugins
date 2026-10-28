import { build } from "vite";
import { viteSingleFile } from "vite-plugin-singlefile";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
for (const [slug, entries] of [
  ["archive", ["preview.html", "loading.html"]],
  ["data-preview", ["index.html", "loading.html"]],
  ["office-preview", ["index.html"]],
]) {
  for (const entry of entries) {
    const ui = path.join(root, "official", slug, "ui");
    await build({
      root: ui,
      configFile: false,
      plugins: [viteSingleFile()],
      build: {
        emptyOutDir: false,
        target: "es2022",
        rollupOptions: { input: path.join(ui, entry) },
      },
    });
  }
}
