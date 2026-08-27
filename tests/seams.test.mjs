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
      // `[^;]` rather than `[\s\S]`: a statement ends at its semicolon, and a
      // pattern allowed past one starts at some earlier import and swallows
      // everything up to the pdf.js line — so a file whose type import is not
      // its first import was reported as importing pdf.js itself.
      [...body.matchAll(/^\s*import\s+(?!type\b)([^;]*?)from\s+["'](pdfjs-dist[^"']*)["']/gm)]
        .some(([, names]) => !names.trim().startsWith("type")),
    )
    .map(({ name }) => name);

  assert.deepEqual(offenders, [], "these import pdf.js itself rather than only its types");
});

/** Every JSDoc block comment, as an offset pair, so two of them can be
    asked whether anything but whitespace lies between them. */
function docBlocks(body) {
  return [...body.matchAll(/\/\*\*[\s\S]*?\*\//g)].map((match) => ({
    start: match.index,
    end: match.index + match[0].length,
  }));
}

test("no doc comment is orphaned above the one below it", () => {
  // AGENTS.md records this exact fault once already — a comment left above
  // `joinRuns` when the function it described was `tintLinks` — and a critical
  // read later found four more of it: a doc comment sits above a function, but
  // describes the *next* one down, with nothing but whitespace between the two
  // comment blocks to say the first is not where it belongs. A stacked pair
  // like that is invisible to a reader who trusts the comment nearest the code
  // and never checked which symbol wrote it.
  const offenders = [];
  for (const { name, body } of sources) {
    const blocks = docBlocks(body);
    for (let i = 1; i < blocks.length; i++) {
      const between = body.slice(blocks[i - 1].end, blocks[i].start);
      if (between.length > 0 && /^\s*$/.test(between)) {
        const line = body.slice(0, blocks[i - 1].start).split("\n").length;
        offenders.push(`${name}:${line}`);
      }
    }
  }
  assert.deepEqual(
    offenders,
    [],
    "two doc comments in a row with nothing between them — the first describes whatever follows the second",
  );
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

test("every shipped theme says where it goes, and no two say the same", () => {
  // The shipped set used to be listed twice, in `theme.rs` and in `api.ts`,
  // and the copies drifted: a theme missing from the second one simply never
  // appeared under `npm run dev`, with nothing to say why. Both lists are gone
  // — each side reads the directory — and what is left to keep straight is the
  // one thing a directory cannot say, which is what order to list them in.
  //
  // `build.rs` refuses to build a file without an `order`, so on the Rust side
  // this is already loud. It is not loud here: `orderOf` in `api.ts` falls
  // back rather than throwing, deliberately, so a half-written theme sorts to
  // the end instead of taking the reader's list apart. This is what makes the
  // browser path say so too, without waiting for a cargo build.
  const dir = "src-tauri/themes";
  const seen = new Map();
  for (const file of readdirSync(dir).filter((name) => name.endsWith(".toml"))) {
    const source = readFileSync(path.join(dir, file), "utf8");
    const found = source.match(/^\s*order\s*=\s*(-?\d+)/m);
    assert.ok(found, `${file} has no \`order\`, so nothing knows where to list it`);
    const order = Number(found[1]);
    assert.ok(!seen.has(order), `${file} and ${seen.get(order)} both claim order ${order}`);
    seen.set(order, file);
  }
  assert.ok(seen.size > 0, "no themes found; the directory moved");
});

test("no shipped theme still spells it `selection`", () => {
  // The key is `selection_area` now, and the old spelling is still *read* —
  // `#[serde(alias)]` in `theme.rs`, the `??` in `parsePackaged` — because a
  // theme somebody wrote is a file this app does not own. That mercy is not
  // meant for the files we ship: a built-in on the old key would load fine,
  // say nothing, and quietly keep the alias earning its place long after the
  // rename was supposed to be over.
  const dir = "src-tauri/themes";
  for (const file of readdirSync(dir).filter((name) => name.endsWith(".toml"))) {
    const source = readFileSync(path.join(dir, file), "utf8");
    assert.ok(
      !/^\s*selection\s*=/m.test(source),
      `${file} uses the old \`selection\` key; it is \`selection_area\``,
    );
  }
});

/* --------------------------------------------------- one window's events */

/** The two events that belong to one window and not to the app.
 *
 * A plain `listen` from `@tauri-apps/api/event` registers for *any* target and
 * hears everything — including an `emit_to` naming a different window. So the
 * moment there was more than one window, a bare `listen` for either of these
 * meant every window opening a document meant for the empty one, and every
 * window reopening its own document because somebody else's had been
 * recompiled. There is nothing to see when this is wrong until two windows are
 * open, and a grep is the only cheap way to hold the line.
 */
test("a document meant for one window is listened for on that window", () => {
  const api = sources.find(({ name }) => name === "api.ts").body;
  for (const event of ["open-document", "document-changed"]) {
    const line = api
      .split("\n")
      .find((each) => each.includes(`"${event}"`) && each.includes("listen"));
    assert.ok(line, `nothing listens for ${event}`);
    assert.match(
      line,
      /getCurrentWindow\(\)\.listen/,
      `${event} is listened for app-wide; every window would answer it`,
    );
  }
});

/** The other half of the same claim, on the Rust side: a document is followed
 *  per window, and told to the window that asked. */
test("Rust keeps a file handle and a watch per window", () => {
  const lib = readFileSync("src-tauri/src/lib.rs", "utf8");
  const watch = readFileSync("src-tauri/src/watch.rs", "utf8");
  assert.match(
    lib,
    /struct OpenFiles\(Mutex<HashMap<String, \(String, File\)>>\)/,
    "the open file went back to being one slot; a second window would take the first's handle",
  );
  assert.match(
    watch,
    /emit_to\(window\.as_str\(\), "document-changed"/,
    "a recompiled document is announced app-wide; every window would reload",
  );
});
