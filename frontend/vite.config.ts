import { defineConfig } from 'vite'
import solid from 'vite-plugin-solid'

export default defineConfig({
  plugins: [solid()],
  build: {
    outDir: '../crates/backend/static',
    emptyOutDir: true,
  },
  server: {
    proxy: {
      '/r4': {
        target: 'http://localhost:8081',
        changeOrigin: true,
      },
      '/r5': {
        target: 'http://localhost:8081',
        changeOrigin: true,
      },
      '/r6': {
        target: 'http://localhost:8081',
        changeOrigin: true,
      },
      '/stats': {
        target: 'http://localhost:8081',
        changeOrigin: true,
      },
      '/health': {
        target: 'http://localhost:8081',
        changeOrigin: true,
      },
    },
  },
})
