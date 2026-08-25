/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_API_MOCK?: string;
  readonly VITE_MORPHOD_URL?: string;
  readonly VITE_ADMIN_USER?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
