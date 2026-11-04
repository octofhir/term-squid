import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath } from 'node:url'

export default defineConfig({
  resolve: { alias: [{ find: /^#\//, replacement: fileURLToPath(new URL('./node_modules/@octofhir/ui-kit/src/', import.meta.url)) }] },
  optimizeDeps: { esbuildOptions: { plugins: [{
    name: 'octofhir-source-alias',
    setup(build) {
      build.onResolve({ filter: /^#\// }, args => build.resolve(
        fileURLToPath(new URL(`./node_modules/@octofhir/ui-kit/src/${args.path.slice(2)}`, import.meta.url)),
        { kind: args.kind, resolveDir: args.resolveDir },
      ))
    },
  }] } },
  plugins: [react()],
  build: { outDir: '../crates/backend/static', emptyOutDir: true },
  server: { proxy: Object.fromEntries(['/r4', '/r5', '/r6', '/api', '/stats', '/health'].map(path => [path, { target: 'http://127.0.0.1:8081' }])) },
})
