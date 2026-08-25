// The version number lives in five places, and a release is only coherent if
// they agree: the DMG is named from `tauri.conf.json`, the Windows installer
// reads the same number to decide what it is upgrading, `cargo` rewrites
// `Cargo.lock` from `Cargo.toml` whether or not the change was committed, and
// npm's lockfile carries a copy of the package's own version. Bumping one by
// hand and missing another produces a release that builds, ships, and then
// disagrees with itself about what it is.
//
//   npm run set-version 0.1.0
//
// Prints the files it changed. Setting the version the tree already has is not
// an error and writes nothing, which is what lets the release workflow run
// this before deciding whether there is anything to commit.

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

/**
 * What a release version may look like. Deliberately narrow: the Windows
 * installer wants three numbers and nothing else, so a `0.1.0-beta.1` builds
 * on two platforms and fails on the third, at the end of the run.
 */
const VERSION = /^\d+\.\d+\.\d+$/;

/**
 * @param {string} at
 * @param {string} file
 * @param {(text: string, version: string) => string} edit
 * @param {string} version
 * @returns {boolean} whether the file needed changing
 */
function rewrite(at, file, edit, version) {
  const path = join(at, file);
  const before = readFileSync(path, "utf8");
  const after = edit(before, version);
  if (after === before) return false;
  writeFileSync(path, after);
  return true;
}

/**
 * Replace the value of a top-level `"version"` key in JSON, in place, so the
 * file keeps its own formatting rather than being reprinted.
 *
 * @param {string} text
 * @param {string} version
 * @returns {string}
 */
function jsonVersion(text, version) {
  const re = /("version"\s*:\s*")[^"]*(")/;
  if (!re.test(text)) throw new Error("no version to set");
  return text.replace(re, `$1${version}$2`);
}

/**
 * @param {string} version
 * @param {string} [at] the tree to write into; the repository by default, and
 *   anything else only so that a test can round-trip a copy of the five files
 *   without editing the ones vite is watching underneath the running app.
 * @returns {string[]} the files that changed
 */
export function setVersion(version, at = root) {
  if (!VERSION.test(version)) {
    throw new Error(`not a release version: ${version} (want major.minor.patch)`);
  }

  /** @type {string[]} */
  const changed = [];
  /**
   * @param {string} file
   * @param {(text: string, version: string) => string} edit
   */
  const touch = (file, edit) => {
    if (rewrite(at, file, edit, version)) changed.push(file);
  };

  touch("package.json", jsonVersion);
  touch("src-tauri/tauri.conf.json", jsonVersion);

  // Two copies in the npm lockfile: the document's own version and the root
  // package's entry. Everything else under `packages` is a dependency's
  // version and must not move, so this one is edited structurally rather than
  // by pattern. npm writes two-space indent and a final newline, which is what
  // reprinting it gives back.
  touch("package-lock.json", (text, v) => {
    const lock = JSON.parse(text);
    lock.version = v;
    if (lock.packages?.[""]) lock.packages[""].version = v;
    return JSON.stringify(lock, null, 2) + "\n";
  });

  touch("src-tauri/Cargo.toml", (text, v) => {
    const re = /^(version\s*=\s*")[^"]*(")/m;
    if (!re.test(text)) throw new Error("no version in Cargo.toml");
    return text.replace(re, `$1${v}$2`);
  });

  // Only the app's own entry in the lock, which is the one naming this crate.
  touch("src-tauri/Cargo.lock", (text, v) => {
    const re = /(name = "hylopdf"\nversion = ")[^"]*(")/;
    if (!re.test(text)) throw new Error("no hylopdf entry in Cargo.lock");
    return text.replace(re, `$1${v}$2`);
  });

  return changed;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const version = process.argv[2];
  if (!version) {
    console.error("usage: npm run set-version <major.minor.patch>");
    process.exit(2);
  }
  try {
    const changed = setVersion(version);
    console.log(changed.length ? `${version}: ${changed.join(", ")}` : `already ${version}`);
  } catch (e) {
    console.error(e instanceof Error ? e.message : String(e));
    process.exit(1);
  }
}
