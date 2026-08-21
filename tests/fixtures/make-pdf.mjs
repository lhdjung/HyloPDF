/* A plain, uncompressed PDF with a lot of pages.
 *
 * Generated rather than committed: it is four hundred pages of the same
 * sentence, which is exactly the shape that makes the layout and the range
 * transport worth testing and exactly the shape that does not belong in a
 * repository as a third of a megabyte of binary. */
import { writeFileSync } from "node:fs";
const PAGES = Number(process.argv[3] ?? 400);
const objects = [];   // 1-indexed body objects
const add = (body) => { objects.push(body); return objects.length; };

const fontId = add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
const pageIds = [];
const contentIds = [];
for (let i = 1; i <= PAGES; i++) {
  const filler = `(Page ${i}. ${"The quick brown fox jumps over the lazy dog. ".repeat(12)}) Tj`;
  const stream = `BT /F1 11 Tf 54 720 Td 14 TL ${filler} ET`;
  contentIds.push(add(`<< /Length ${stream.length} >>\nstream\n${stream}\nendstream`));
  pageIds.push(0); // placeholder, filled below
}
const pagesId = objects.length + PAGES + 1;
for (let i = 0; i < PAGES; i++) {
  pageIds[i] = add(
    `<< /Type /Page /Parent ${pagesId} 0 R /MediaBox [0 0 612 792] ` +
    `/Resources << /Font << /F1 ${fontId} 0 R >> >> /Contents ${contentIds[i]} 0 R >>`
  );
}
const realPagesId = add(`<< /Type /Pages /Count ${PAGES} /Kids [${pageIds.map((id) => `${id} 0 R`).join(" ")}] >>`);
const catalogId = add(`<< /Type /Catalog /Pages ${realPagesId} 0 R >>`);

let out = "%PDF-1.4\n";
const offsets = [0];
objects.forEach((body, i) => {
  offsets.push(out.length);
  out += `${i + 1} 0 obj\n${body}\nendobj\n`;
});
const xref = out.length;
out += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
for (let i = 1; i <= objects.length; i++) out += `${String(offsets[i]).padStart(10, "0")} 00000 n \n`;
out += `trailer\n<< /Size ${objects.length + 1} /Root ${catalogId} 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
writeFileSync(process.argv[2], out, "latin1");
console.log("wrote", process.argv[2], out.length, "bytes,", PAGES, "pages");
