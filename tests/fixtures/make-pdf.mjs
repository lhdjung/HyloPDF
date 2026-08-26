/* A plain, uncompressed PDF with a lot of pages.
 *
 * Generated rather than committed: it is four hundred pages of the same
 * sentence, which is exactly the shape that makes the layout and the range
 * transport worth testing and exactly the shape that does not belong in a
 * repository as a third of a megabyte of binary. */
import { writeFileSync } from "node:fs";
const PAGES = Number(process.argv[3] ?? 400);
/* A third argument of "labels" numbers the pages the way a book does: roman
   front matter, then the body starting again at 1. That is the shape that
   makes the number in the toolbar and the position in the file disagree, and
   it is the only shape worth generating a second fixture for. */
const LABELLED = process.argv[4] === "labels";
const FRONT = 4;
/* "notext" draws a box on every page and writes no words at all, which is the
   shape of a scan that never went through OCR: nothing to search, nothing to
   select, and an empty table of contents. */
const NOTEXT = process.argv[4] === "notext";
/* "titled" gives the document a name of its own, which is what the toolbar
   and the recently-read list would rather show than `book.pdf`. */
const TITLED = process.argv[4] === "titled";
/* "notes" leaves two annotations on the first page: a sticky note, which is an
   icon, and a comment on a highlighted line, which is not. pdf.js paints both
   into the page and leaves their text unreachable, which is what the note
   layer is for. */
const NOTES = process.argv[4] === "notes";
const objects = [];   // 1-indexed body objects
const add = (body) => { objects.push(body); return objects.length; };

const fontId = add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
const pageIds = [];
const contentIds = [];
for (let i = 1; i <= PAGES; i++) {
  const filler = `(Page ${i}. ${"The quick brown fox jumps over the lazy dog. ".repeat(12)}) Tj`;
  // A block of ink in the middle of a wide-margined page, sitting a little
  // higher on some pages than on others — so that the union over a sample is
  // larger than any single page, which is the case margin trimming has to get
  // right.
  const stream = NOTEXT
    ? `0.2 0.2 0.2 rg 150 ${150 + (i % 2) * 50} 312 442 re f`
    : `BT /F1 11 Tf 54 720 Td 14 TL ${filler} ET`;
  contentIds.push(add(`<< /Length ${stream.length} >>\nstream\n${stream}\nendstream`));
  pageIds.push(0); // placeholder, filled below
}
const noteIds = [];
if (NOTES) {
  noteIds.push(
    add(
      "<< /Type /Annot /Subtype /Text /Rect [520 700 540 720] /Name /Comment " +
        "/T (A. Reviewer) /Contents (Check this figure against the appendix.) >>",
    ),
    add(
      "<< /Type /Annot /Subtype /Highlight /Rect [54 700 460 726] " +
        "/QuadPoints [54 726 460 726 54 700 460 700] /C [1 1 0] " +
        "/T (A. Reviewer) /Contents (This sentence needs a citation.) >>",
    ),
  );
}
const pagesId = objects.length + PAGES + 1;
for (let i = 0; i < PAGES; i++) {
  const annots = NOTES && i === 0 ? ` /Annots [${noteIds.map((id) => `${id} 0 R`).join(" ")}]` : "";
  pageIds[i] = add(
    `<< /Type /Page /Parent ${pagesId} 0 R /MediaBox [0 0 612 792] ` +
    `/Resources << /Font << /F1 ${fontId} 0 R >> >> /Contents ${contentIds[i]} 0 R${annots} >>`
  );
}
const realPagesId = add(`<< /Type /Pages /Count ${PAGES} /Kids [${pageIds.map((id) => `${id} 0 R`).join(" ")}] >>`);
const labels = LABELLED
  ? ` /PageLabels << /Nums [0 << /S /r >> ${FRONT} << /S /D /St 1 >>] >>`
  : "";
const catalogId = add(`<< /Type /Catalog /Pages ${realPagesId} 0 R${labels} >>`);
const infoId = TITLED
  ? add(
      "<< /Title (On the Quiet Reading of Documents) /Author (A. Reader) " +
        "/Creator (HyloPDF test fixtures) /CreationDate (D:20240131120000Z) >>",
    )
  : null;

let out = "%PDF-1.4\n";
const offsets = [0];
objects.forEach((body, i) => {
  offsets.push(out.length);
  out += `${i + 1} 0 obj\n${body}\nendobj\n`;
});
const xref = out.length;
out += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
for (let i = 1; i <= objects.length; i++) out += `${String(offsets[i]).padStart(10, "0")} 00000 n \n`;
const info = infoId ? ` /Info ${infoId} 0 R` : "";
out += `trailer\n<< /Size ${objects.length + 1} /Root ${catalogId} 0 R${info} >>\nstartxref\n${xref}\n%%EOF\n`;
writeFileSync(process.argv[2], out, "latin1");
console.log("wrote", process.argv[2], out.length, "bytes,", PAGES, "pages");
