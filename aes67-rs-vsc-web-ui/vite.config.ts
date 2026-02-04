import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { VitePWA } from "vite-plugin-pwa";

export default defineConfig({
  plugins: [
    solid(),
    VitePWA({
      registerType: "autoUpdate",
      injectRegister: "auto",
      devOptions: {
        enabled: true,
      },
      includeAssets: ["icon_round.svg", "icon_maskable.svg"],
      manifest: {
        short_name: "AES67 VSC",
        name: "AES67 Virtual Sound Card",
        icons: [
          {
            src: "icon_round.svg",
            sizes: "any",
            type: "image/svg+xml",
            purpose: "any",
          },
          {
            src: "icon_maskable.svg",
            sizes: "any",
            type: "image/svg+xml",
            purpose: "maskable",
          },
        ],
        display: "standalone",
        theme_color: "#337ab7",
        background_color: "#3b3b3bff",
      },
    }),
  ],
  server: {
    proxy: {
      "/api": {
        target: "http://127.0.0.1:43567",
        changeOrigin: true,
      },
      "/ws": {
        target: "ws://127.0.0.1:43567",
        ws: true,
        rewriteWsOrigin: true,
      },
    },
  },
});
