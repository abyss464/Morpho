import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { morphoRelease } from './server/release';

const HOST = '127.0.0.1';
// MORPHO_WEB_PORT runs a second copy beside the real one, e.g. for testing sync.
const PORT = Number(process.env.MORPHO_WEB_PORT ?? 30017);

export default defineConfig({
  plugins: [react(), morphoRelease()],
  server: { host: HOST, port: PORT, strictPort: true },
  preview: { host: HOST, port: PORT, strictPort: true },
  build: { outDir: 'dist' },
});
