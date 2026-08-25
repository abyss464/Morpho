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
    plugins: [tanstackRouter({ target: 'react', autoCodeSplitting: true }), react()],
    resolve: {
      alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
    },
    server: {
      port: 5173,
      // Only proxy when NOT mocked; MSW intercepts at the service-worker layer otherwise.
      proxy: mocked
        ? undefined
        : {
            /*
             * `GET /api/stream` is an open-ended SSE response: no content-length,
             * no end, and the first bytes matter immediately. The dev proxy's
             * default buffering swallows it, so the stream is proxied on its own
             * rule with buffering off and the response headers flushed the moment
             * morphod answers. Production never goes through here — morphod
             * serves the built UI from its own origin.
             */
            '/api/stream': {
              target: morphod,
              changeOrigin: true,
              // No response timeout: an idle stream is normal, `: ping` every 30 s.
              proxyTimeout: 0,
              timeout: 0,
              configure: (proxy) => {
                proxy.on('proxyRes', (proxyRes, _req, res) => {
                  const contentType = proxyRes.headers['content-type'];
                  if (!contentType?.includes('text/event-stream')) return;
                  // flushHeaders() commits whatever is set right now, and the
                  // proxy copies the upstream headers after this hook — so the
                  // content type has to be restated here or EventSource rejects
                  // the response before the first frame.
                  res.setHeader('Content-Type', contentType);
                  res.setHeader('Cache-Control', 'no-cache, no-transform');
                  res.setHeader('X-Accel-Buffering', 'no');
                  res.flushHeaders();

                  // When morphod goes away the browser has to find out, or the
                  // console keeps a dead stream open and silently stops
                  // refreshing. Served straight from morphod there is no proxy
                  // to hold the socket up; in dev we have to close it by hand.
                  const close = () => {
                    if (!res.writableEnded) res.end();
                  };
                  proxyRes.on('close', close);
                  proxyRes.on('aborted', close);
                  proxyRes.socket?.on('close', close);
                });
                proxy.on('error', (_error, _req, target) => {
                  if (target && 'writableEnded' in target && !target.writableEnded) target.end();
                });
              },
            },
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
