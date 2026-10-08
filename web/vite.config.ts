import fs from "node:fs";
import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

const catalogPath = fileURLToPath(new URL("../data/catalog.json", import.meta.url));

function catalogJson(): Plugin {
  const serve = (
    req: { url?: string },
    res: NodeJS.WritableStream & { setHeader: (name: string, value: string) => void },
    next: () => void,
  ) => {
    if (req.url?.split("?")[0] !== "/catalog.json") {
      next();
      return;
    }
    res.setHeader("Content-Type", "application/json; charset=utf-8");
    fs.createReadStream(catalogPath).pipe(res);
  };

  return {
    name: "catalog-json",
    configureServer(server) {
      server.middlewares.use(serve);
    },
    configurePreviewServer(server) {
      server.middlewares.use(serve);
    },
    generateBundle() {
      this.emitFile({
        type: "asset",
        fileName: "catalog.json",
        source: fs.readFileSync(catalogPath),
      });
    },
  };
}

export default defineConfig({
  base: "./",
  plugins: [react(), catalogJson()],
});
