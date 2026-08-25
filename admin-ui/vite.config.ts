/// <reference types="vitest/config" />
import { fileURLToPath, URL } from 'node:url';
import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';
import { tanstackRouter } from '@tanstack/router-plugin/vite';

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), 'VITE_');
  // Mocked mode is the default in dev; set VITE_API_MOCK=0 to talk to a real morphod.
  const mocked = (env.VITE_API_MOCK ?? (mode === 'development' ? '1' : '0')) === '1';
  const morphod = env.VITE_MORPHOD_URL ?? 'http://127.0.0.1:8787';

  return {
    plugins: [
      tanstackRouter({ target: 'react', autoCodeSplitting: true }),
      react(),
    ],
    resolve: {
      alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
    },
    server: {
      port: 5173,
      // Only proxy when NOT mocked; MSW intercepts at the service-worker layer otherwise.
      proxy: mocked
        ? undefined
        : {
            '/api': { target: morphod, changeOrigin: true },
          },
    },
    build: {
      outDir: 'dist',
      sourcemap: true,
      chunkSizeWarningLimit: 1400,
    },
    test: {
      globals: true,
      environment: 'jsdom',
      setupFiles: ['./src/test/setup.ts'],
      css: false,
      include: ['src/**/*.{test,spec}.{ts,tsx}'],
      restoreMocks: true,
    },
  };
});
