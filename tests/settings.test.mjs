/* The settings table exists twice, and the two copies have to agree.
 *
 * `settings.rs` defines every setting and its default; that table doubles as
 * the whitelist, so it is the real one. `api.ts` restates it as
 * `fallbackDefaults`, because the browser path has no Rust behind it and the
 * harness runs the whole interface down that path.
 *
 * This is the drift api.ts already warns about for themes — "the restated copy
 * went stale, and a stale copy is invisible: the file is right and what is on
 * screen is the copy" — and the themes are read from their TOML at build time
 * to avoid it. The settings cannot be: TOML has no null, and `window_x`
 * defaults to one, so the Rust table is not expressible as a file the
 * frontend could read. What is left is to check that the two say the same
 * thing, which is what this does.
 *
 * Add a setting to Rust and forget the twin, and the harness starts testing a
 * configuration the app never has. That is the failure worth catching, and it
 * is silent every other way.
 */

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const rust = readFileSync("src-tauri/src/settings.rs", "utf8");
const themeRust = readFileSync("src-tauri/src/theme.rs", "utf8");
const api = readFileSync("src/api.ts", "utf8");

/** The two theme ids `defaults()` names by constant rather than by value. */
function constants() {
  const found = {};
  for (const [, name, value] of themeRust.matchAll(
    /pub const (DEFAULT_LIGHT|DEFAULT_DARK): &str = "([^"]+)"/g,
  )) {
    found[name] = value;
  }
  return found;
}

/** The body of `defaults()` in settings.rs, as a plain object.
 *
 * A fraction of Rust rather than a parser, in the same spirit as the fraction
 * of TOML api.ts reads the packaged themes with: every line in that function
 * is `s.insert("key".into(), <value>);` and anything cleverer is not wanted
 * here. If the shape of the function changes this stops finding entries, and
 * the count assertion below is what says so. */
function rustDefaults() {
  const named = constants();
  const body = rust.slice(rust.indexOf("pub fn defaults()"), rust.indexOf("\nfn path("));
  const found = {};
  // One entry per line, which is what lets the closing `);` end the match.
  for (const [, key, raw] of body.matchAll(/s\.insert\("([a-z_]+)"\.into\(\),\s*(.+?)\);\s*$/gm)) {
    const value = raw.trim();
    if (value === "Value::Null") found[key] = null;
    else if (value.startsWith("json!(")) found[key] = literal(value.slice("json!(".length), named);
    else throw new Error(`settings.rs: cannot read the default for ${key}: ${value}`);
  }
  return found;
}

function literal(text, named) {
  const value = text.replace(/\)$/, "").trim();
  if (value.startsWith('"')) return value.slice(1, -1);
  if (value === "true" || value === "false") return value === "true";
  for (const [name, resolved] of Object.entries(named)) {
    if (value.endsWith(name)) return resolved;
  }
  const number = Number(value);
  if (Number.isNaN(number)) throw new Error(`settings.rs: not a value: ${value}`);
  return number;
}

/** `fallbackDefaults` in api.ts, as a plain object. It is a TypeScript object
    literal of scalars, so JSON is one quoting pass away. */
function browserDefaults() {
  const at = api.indexOf("const fallbackDefaults: Settings = {");
  const body = api.slice(api.indexOf("{", at), api.indexOf("\n};", at) + 2);
  const json = body
    .replace(/\/\/.*$/gm, "")
    .replace(/^(\s*)([a-z_]+):/gm, '$1"$2":')
    .replace(/,(\s*})/g, "$1");
  return JSON.parse(json);
}

test("every setting Rust knows has a browser default, and vice versa", () => {
  const fromRust = rustDefaults();
  const fromBrowser = browserDefaults();

  // If the parse above stops finding entries this is the assertion that says
  // so, rather than the two sides silently agreeing about nothing.
  assert.ok(Object.keys(fromRust).length > 15, "settings.rs was not read properly");

  assert.deepEqual(
    Object.keys(fromRust).sort(),
    Object.keys(fromBrowser).sort(),
    "settings.rs and api.ts disagree about which settings exist",
  );
});

test("and they agree about what each one defaults to", () => {
  const fromRust = rustDefaults();
  const fromBrowser = browserDefaults();
  for (const [key, value] of Object.entries(fromRust)) {
    assert.equal(
      fromBrowser[key],
      value,
      `${key} defaults to ${JSON.stringify(value)} in Rust and ${JSON.stringify(fromBrowser[key])} in the browser`,
    );
  }
});

test("the Settings type covers exactly those settings", () => {
  // The third copy, and the one a type checker would not catch either: the
  // type says what the frontend may ask for, so a setting missing from it is
  // one the interface cannot reach.
  const start = api.indexOf("export type Settings = {");
  const declared = api.slice(start, api.indexOf("\n};", start));
  const names = [...declared.matchAll(/^\s{2}([a-z_]+):/gm)].map(([, name]) => name);
  assert.deepEqual(names.sort(), Object.keys(rustDefaults()).sort());
});
