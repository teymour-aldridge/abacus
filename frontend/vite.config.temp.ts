import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { resolve } from "path";

export default defineConfig({
  plugins: [solid()],
  build: {
    outDir: "../static/dist_test",
    emptyOutDir: true,
    rollupOptions: {
      input: {
        "draw-editor": resolve(__dirname, "src/index.tsx"),
      },
      output: {
        entryFileNames: `[name].js`,
        chunkFileNames: `[name].js`,
        assetFileNames: `draw-editor-test.css`,
      },
    },
  },
  base: "",
});
