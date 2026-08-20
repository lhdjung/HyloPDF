declare module "*?url" {
  const url: string;
  export default url;
}

interface ImportMeta {
  readonly env: { readonly DEV: boolean; readonly PROD: boolean };
}

/** The version from package.json, substituted at build time. */
declare const __APP_VERSION__: string;
