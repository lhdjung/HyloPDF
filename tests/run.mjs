/* Run the tests, with a dev server for the ones that need one.
 *
 * Two of the three test files are self-contained — they compile a module and
 * poke at it. `reader.test.mjs` drives the whole interface, which means vite
 * has to be serving it, so this starts one, waits for it to answer, and takes
 * it down again afterwards however the run ends. */

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { setTimeout as wait } from "node:timers/promises";

const URL_BASE = process.env.HYLOPDF_URL ?? "http://localhost:1420/";
const FIXTURE = "tests/fixtures/book.pdf";
const LOCKED = "tests/fixtures/locked.pdf";

async function answering() {
  try {
    return (await fetch(URL_BASE, { signal: AbortSignal.timeout(500) })).ok;
  } catch {
    return false;
  }
}

// Generated rather than committed: four hundred pages of the same sentence is
// exactly the shape worth testing and exactly the shape that should not sit in
// the repository as a third of a megabyte of binary.
async function make(script, ...args) {
  await new Promise((resolve, reject) => {
    const run = spawn("node", [script, ...args], { stdio: "inherit" });
    run.on("exit", (code) => (code === 0 ? resolve() : reject(new Error(`${script} failed`))));
  });
}

if (!existsSync(FIXTURE)) await make("tests/fixtures/make-pdf.mjs", FIXTURE, "400");
if (!existsSync(LOCKED)) await make("tests/fixtures/make-encrypted-pdf.mjs", LOCKED);

let vite = null;
if (!(await answering())) {
  vite = spawn("npm", ["run", "dev"], { stdio: "ignore", detached: true });
  const deadline = Date.now() + 30_000;
  while (!(await answering())) {
    if (Date.now() > deadline) {
      process.kill(-vite.pid);
      throw new Error(`no dev server at ${URL_BASE} after 30s`);
    }
    await wait(250);
  }
}

const tests = spawn("node", ["--test", "tests/*.test.mjs"], { stdio: "inherit" });
tests.on("exit", (code) => {
  // The whole process group: vite spawns esbuild alongside itself.
  if (vite) process.kill(-vite.pid);
  process.exit(code ?? 1);
});
