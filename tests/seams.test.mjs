/* The two seams the architecture rests on, checked rather than asserted.
 *
 * Both are stated in AGENTS.md as facts about the code, and both are the kind
 * of fact that stops being true one convenient import at a time. `api.ts` is
 * the only door into Rust — that is half of why the renderer turned out to be
 * replaceable, and it is worth nothing if it is only true on the day somebody
 * last checked. `viewer.ts` is the only file that imports pdf.js for
 * rendering, which is the other half.
 *
 * A grep is exactly the right tool for this and exactly the wrong thing to
 * leave to a person to remember to run.
 */

import test from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";

const SRC = "src";
const sources = readdirSync(SRC)
  .filter((name) => name.endsWith(".ts") && !name.endsWith(".d.ts"))
  .map((name) => ({ name, body: readFileSync(path.join(SRC, name), "utf8") }));

/** Every module a file imports from, `import` and `import type` alike. */
function importsOf(body) {
  return [...body.matchAll(/^\s*(?:import|export)[\s\S]*?from\s+["']([^"']+)["']/gm)].map(
    (match) => match[1],
  );
}

test("api.ts is the only file that talks to Rust", () => {
  const offenders = sources
    .filter(({ name }) => name !== "api.ts")
    .filter(({ body }) => importsOf(body).some((from) => from.startsWith("@tauri-apps/")))
    .map(({ name }) => name);

  assert.deepEqual(
    offenders,
    [],
    "these reach past api.ts into Tauri; add a function to api.ts with a browser twin instead",
  );
});

test("viewer.ts is the only file that imports pdf.js for rendering", () => {
  // The others may have pdf.js's *types* — they are handed a
  // `PDFDocumentProxy` and have to name it — but nothing else may pull in the
  // library itself, or swapping the renderer stops being one file's problem.
  const offenders = sources
    .filter(({ name }) => name !== "viewer.ts")
    .filter(({ body }) =>
      [...body.matchAll(/^\s*import\s+(?!type\b)([\s\S]*?)from\s+["'](pdfjs-dist[^"']*)["']/gm)]
        .some(([, names]) => !names.trim().startsWith("type")),
    )
    .map(({ name }) => name);

  assert.deepEqual(offenders, [], "these import pdf.js itself rather than only its types");
});

test("the packaged dependencies are the ones that ship", () => {
  const pkg = JSON.parse(readFileSync("package.json", "utf8"));
  const deps = Object.keys(pkg.dependencies ?? {});
  const dev = Object.keys(pkg.devDependencies ?? {});

  // Anything `src/` imports has to be a real dependency: it is in the bundle,
  // and a build that works because a dev dependency happened to be installed
  // is a build that works by accident.
  const external = new Set(
    sources.flatMap(({ body }) =>
      importsOf(body).filter((from) => !from.startsWith(".") && !from.startsWith("/")),
    ),
  );
  for (const from of external) {
    const packageName = from.startsWith("@") ? from.split("/").slice(0, 2).join("/") : from.split("/")[0];
    assert.ok(
      deps.includes(packageName),
      `src/ imports ${packageName}, which is ${dev.includes(packageName) ? "a dev dependency" : "not a dependency at all"}`,
    );
  }
});
