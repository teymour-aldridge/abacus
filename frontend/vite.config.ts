import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { resolve } from "path";

export default defineConfig({
  plugins: [solid()],
  build: {
    outDir: process.env.ABACUS_FRONTEND_OUT_DIR ?? "../static/dist",
    emptyOutDir: true,
    rollupOptions: {
      input: {
        "draw-editor": resolve(__dirname, "src/index.tsx"),
        "draw-room-allocator": resolve(__dirname, "src/room_allocator.tsx"),
      },
      output: {
        entryFileNames: `[name].js`,
        chunkFileNames: `[name].js`,
        assetFileNames: `[name].[ext]`,
      },
    },
  },
  base: "",
});
