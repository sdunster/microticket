/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Required in a production build; see `lib/api.ts`. */
  readonly VITE_API_URL?: string;
  /** Stamped with the deploy commit SHA in CI; `"dev"` locally. */
  readonly VITE_CLIENT_VERSION?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
