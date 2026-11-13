import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

export default defineConfig({
  plugins: [react()],
  base: "./",
  server: { host: "127.0.0.1", port: 8792 },
  build: {
    outDir: "review-mock-dist",
    rollupOptions: { input: resolve(__dirname, "review-mock.html") },
  },
});
