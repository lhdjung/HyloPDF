declare module "*?url" {
  const url: string;
  export default url;
}

interface ImportMeta {
  readonly env: { readonly DEV: boolean; readonly PROD: boolean };
  /** Vite's build-time directory read. Narrowed to the one shape the app uses:
      every match, as a string, resolved when the bundle is built. */
  glob(
    pattern: string,
    options: { query: "?raw"; import: "default"; eager: true },
  ): Record<string, string>;
}

/** The version from package.json, substituted at build time. */
declare const __APP_VERSION__: string;
