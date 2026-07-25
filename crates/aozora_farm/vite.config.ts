import { defineConfig } from 'vite'
import { resolve } from 'path'

export default defineConfig({
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
  // Tauri expects a fixed port in development
  server: {
    port: 1420,
    strictPort: true,
  },
  // Tauri uses clearScreen: false to allow seeing Rust logs
  clearScreen: false,
  build: {
    // Tauri uses Chromium, we can use modern features
    target: 'esnext',
    // Don't minify for easier debugging
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
    // Produce sourcemaps for error reporting
    sourcemap: !!process.env.TAURI_DEBUG,
  },
})
