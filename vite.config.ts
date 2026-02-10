import react from '@vitejs/plugin-react-swc'
import { defineConfig } from 'vite'
import { resolve } from 'node:path'

export default defineConfig(async () => {
  const tailwind = (await import('@tailwindcss/vite')).default()

  return {
    root: resolve(__dirname, 'src/renderer'),
    base: './',
    plugins: [tailwind, react({ tsDecorators: true })],
    resolve: {
      alias: {
        '@renderer': resolve(__dirname, 'src/renderer/src'),
        '@shared': resolve(__dirname, 'packages/shared'),
        '@types': resolve(__dirname, 'src/renderer/src/types'),
        '@logger': resolve(__dirname, 'src/renderer/src/services/LoggerService'),
        '@mcp-trace/trace-core': resolve(__dirname, 'packages/mcp-trace/trace-core'),
        '@mcp-trace/trace-web': resolve(__dirname, 'packages/mcp-trace/trace-web'),
        '@cherrystudio/ai-core/provider': resolve(__dirname, 'packages/aiCore/src/core/providers'),
        '@cherrystudio/ai-core/built-in/plugins': resolve(__dirname, 'packages/aiCore/src/core/plugins/built-in'),
        '@cherrystudio/ai-core': resolve(__dirname, 'packages/aiCore/src'),
        '@cherrystudio/extension-table-plus': resolve(__dirname, 'packages/extension-table-plus/src'),
        '@cherrystudio/ai-sdk-provider': resolve(__dirname, 'packages/ai-sdk-provider/src')
      }
    },
    server: {
      port: 1420,
      strictPort: true
    },
    optimizeDeps: {
      exclude: ['pyodide']
    },
    worker: {
      format: 'es'
    },
    build: {
      outDir: resolve(__dirname, 'dist'),
      emptyOutDir: true,
      target: 'esnext',
      rollupOptions: {
        input: {
          index: resolve(__dirname, 'src/renderer/index.html'),
          miniWindow: resolve(__dirname, 'src/renderer/miniWindow.html'),
          selectionToolbar: resolve(__dirname, 'src/renderer/selectionToolbar.html'),
          selectionAction: resolve(__dirname, 'src/renderer/selectionAction.html'),
          traceWindow: resolve(__dirname, 'src/renderer/traceWindow.html')
        }
      }
    }
  }
})

