// pdf.js needs a few data directories at runtime: character maps for CJK
// documents, the fourteen standard fonts for PDFs that embed none, colour
// profiles, and the wasm decoders for JPEG 2000 and friends. Copy them next to
// the built app so everything works offline.
import { copyFileSync, cpSync, rmSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const from = resolve(root, "node_modules/pdfjs-dist");
const to = resolve(root, "public/pdfjs");

rmSync(to, { recursive: true, force: true });
for (const dir of ["cmaps", "standard_fonts", "iccs", "wasm"]) {
  cpSync(resolve(from, dir), resolve(to, dir), { recursive: true });
}
// Apache-2.0 asks that a copy of it travel with what it covers, and the
// directories above already carry the licences of everything *inside* them —
// the fonts, the decoders, the colour profile — but not pdf.js's own. This is
// that copy. See THIRD-PARTY.md.
copyFileSync(resolve(from, "LICENSE"), resolve(to, "LICENSE"));
console.log("pdf.js runtime data copied to public/pdfjs");
