/* Shared plumbing for the tests.
 *
 * `load` is the awkward one and worth explaining: the modules under test are
 * TypeScript, and most of what wants testing in them is not exported — the
 * folding a search does, the two paths through recolouring. Rather than widen
 * the public surface of a module to suit its tests, the source is compiled in
 * memory with the imports stripped and the wanted names re-exported. */

import { readFileSync } from "node:fs";
import { transform } from "esbuild";

/**
 * Compile one of the app's modules and hand back the names asked for,
 * exported or not.
 *
 * @param {string} file    path to the .ts file, from the repo root
 * @param {string[]} names what to pull out of it
 */
export async function load(file, names) {
  const ts = readFileSync(file, "utf8")
    .replace(/^import type \{[\s\S]*?\} from [^;]*;$/gm, "")
    .replace(/^import[^;]*;$/gm, "")
    .replace(/^export /gm, "")
    + `\nexport { ${names.join(", ")} };`;
  const { code } = await transform(ts, { loader: "ts", format: "esm" });
  return import("data:text/javascript;base64," + Buffer.from(code).toString("base64"));
}

/**
 * The same, but for a module that has to run in a browser: the source is
 * evaluated in the page and the wanted names are hung off `globalThis.T`.
 *
 * `extra` is appended to the source before compiling, which is how a test
 * reaches a module-level binding — `let` inside an eval is scoped to the eval,
 * so it takes a closure to touch the real one.
 */
export async function sourceFor(file, names, extra = "") {
  const ts = readFileSync(file, "utf8")
    .replace(/^import type \{[\s\S]*?\} from [^;]*;$/gm, "")
    .replace(/^import[^;]*;$/gm, "")
    .replace(/^export /gm, "")
    + `\nglobalThis.T = { ${names.join(", ")}${extra ? `, ${extra}` : ""} };`;
  return (await transform(ts, { loader: "ts" })).code;
}
