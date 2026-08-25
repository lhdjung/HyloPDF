/* The version exists five times, and a release is only coherent if they agree.
 *
 * `tauri.conf.json` names the DMG and the installer, the Windows installer
 * decides what it is upgrading from the same number, `Cargo.toml` is what the
 * app reports about itself, and the two lockfiles carry copies that cargo and
 * npm will rewrite anyway. `scripts/set-version.mjs` writes all five, which is
 * the only reason they stay together — but it writes them by pattern, and a
 * pattern that stops matching does not fail, it silently declines to change
 * anything. A release would then be tagged 0.3.0 and built as 0.2.0, and the
 * first sign of it would be the file name on the releases page.
 *
 * So this checks both halves: that the five agree today, and that each of the
 * five patterns the script depends on still finds something to change.
 */

import test from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { tmpdir } from "node:os";

/** Where each copy lives, and how to read it back out. */
const copies = {
  "package.json": (t) => JSON.parse(t).version,
  "package-lock.json": (t) => JSON.parse(t).version,
  "src-tauri/tauri.conf.json": (t) => JSON.parse(t).version,
  "src-tauri/Cargo.toml": (t) => /^version\s*=\s*"([^"]+)"/m.exec(t)?.[1],
  "src-tauri/Cargo.lock": (t) => /name = "hylopdf"\nversion = "([^"]+)"/.exec(t)?.[1],
};

test("the five copies of the version agree", () => {
  const found = Object.fromEntries(
    Object.entries(copies).map(([file, read]) => [file, read(readFileSync(file, "utf8"))]),
  );
  const version = found["src-tauri/tauri.conf.json"];
  assert.match(String(version), /^\d+\.\d+\.\d+$/, "tauri.conf.json holds a release version");
  for (const [file, value] of Object.entries(found)) {
    assert.equal(value, version, `${file} is at ${value}, tauri.conf.json at ${version}`);
  }
});

test("the root package's own entry in the npm lockfile moves with it", () => {
  const lock = JSON.parse(readFileSync("package-lock.json", "utf8"));
  assert.equal(lock.packages?.[""]?.version, lock.version);
});

test("set-version still writes every file it claims to", async () => {
  const { setVersion } = await import("../scripts/set-version.mjs");

  // A copy, not the repository: `npm test` has a vite dev server watching
  // these very files, and rewriting package.json underneath it restarts the
  // server the other tests are reading the app from.
  const at = mkdtempSync(join(tmpdir(), "hylopdf-version-"));
  const before = {};
  for (const file of Object.keys(copies)) {
    before[file] = readFileSync(file, "utf8");
    mkdirSync(dirname(join(at, file)), { recursive: true });
    writeFileSync(join(at, file), before[file]);
  }
  const version = String(copies["src-tauri/tauri.conf.json"](before["src-tauri/tauri.conf.json"]));

  // Out to a version nothing is at, so every file has something to do, and
  // back again, which must restore all five byte for byte. Both directions are
  // the point: the first says the patterns match, the second says they match
  // the whole of what they wrote.
  const other = version === "9.9.9" ? "9.9.8" : "9.9.9";
  try {
    assert.deepEqual(setVersion(other, at).sort(), Object.keys(copies).sort());
    for (const [file, read] of Object.entries(copies)) {
      assert.equal(read(readFileSync(join(at, file), "utf8")), other, `${file} moved`);
    }
    assert.deepEqual(setVersion(version, at).sort(), Object.keys(copies).sort());
    for (const [file, text] of Object.entries(before)) {
      assert.equal(readFileSync(join(at, file), "utf8"), text, `${file} came back unchanged`);
    }
    assert.deepEqual(setVersion(version, at), [], "setting the version it has writes nothing");
  } finally {
    rmSync(at, { recursive: true, force: true });
  }
});

test("a version that is not major.minor.patch is refused", async () => {
  const { setVersion } = await import("../scripts/set-version.mjs");
  for (const bad of ["1.2", "v1.2.3", "1.2.3-beta.1", "", "one.two.three"]) {
    assert.throws(() => setVersion(bad), /not a release version/, `refused: ${bad}`);
  }
});
