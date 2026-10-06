import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { morphoRelease } from './server/release';

const HOST = '127.0.0.1';
const PORT = 30017;

export default defineConfig({
  plugins: [react(), morphoRelease()],
  server: { host: HOST, port: PORT, strictPort: true },
  preview: { host: HOST, port: PORT, strictPort: true },
  build: { outDir: 'dist' },
});
