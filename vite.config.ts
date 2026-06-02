import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// @ts-expect-error process is a nodejs global
// @ts-expect-error process 是一个 nodejs 全局变量
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [vue()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  // 专为 Tauri 开发量身定制的 Vite 选项，仅在 `tauri dev` 或 `tauri build` 中应用
  //
  // 1. prevent Vite from obscuring rust errors
  // 1. 防止 Vite 掩盖 rust 错误
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  // 2. Tauri 期望一个固定端口，如果该端口不可用则失败
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      // 3. 告诉 Vite 忽略监视 `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
