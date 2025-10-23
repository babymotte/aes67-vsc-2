import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid()],
  server: {
    proxy: {
      "/api": {
        target: "http://127.0.0.1:55667",
        changeOrigin: true,
      },
      "/ws": {
        target: "ws://127.0.0.1:55667",
        ws: true,
        rewriteWsOrigin: true,
      },
    },
  },
});
