import { defineConfig } from "vite";
import { VitePWA } from "vite-plugin-pwa";

export default defineConfig({
  base: "./",
  plugins: [
    VitePWA({
      // Register from the app, so we can report when offline setup succeeds.
      injectRegister: false,
      registerType: "prompt",
      manifest: false,
      includeAssets: ["manifest.webmanifest", "nosleep.LICENSE.txt"],
      workbox: {
        cacheId: "qrx",
        globPatterns: ["**/*.{js,css,html,wasm}"],
        cleanupOutdatedCaches: true,
        clientsClaim: true,
        // Activate updates after existing tabs close; never interrupt a transfer.
        skipWaiting: false,
        navigateFallback: "index.html",
      },
    }),
  ],
});
