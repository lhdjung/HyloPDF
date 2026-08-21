/* A one-page PDF with the standard security handler, RC4 40-bit (V1/R2),
   so the password prompt has something to ask about. User password: "hylo". */
import { createHash } from "node:crypto";
import { writeFileSync } from "node:fs";

const PAD = Buffer.from([
  0x28,0xBF,0x4E,0x5E,0x4E,0x75,0x8A,0x41,0x64,0x00,0x4E,0x56,0xFF,0xFA,0x01,0x08,
  0x2E,0x2E,0x00,0xB6,0xD0,0x68,0x3E,0x80,0x2F,0x0C,0xA9,0xFE,0x64,0x53,0x69,0x7A]);

const pad = (pw) => Buffer.concat([Buffer.from(pw, "latin1"), PAD]).subarray(0, 32);
const md5 = (b) => createHash("md5").update(b).digest();

function rc4(key, data) {
  const s = [...Array(256).keys()];
  for (let i = 0, j = 0; i < 256; i++) {
    j = (j + s[i] + key[i % key.length]) & 255;
    [s[i], s[j]] = [s[j], s[i]];
  }
  const out = Buffer.alloc(data.length);
  for (let k = 0, i = 0, j = 0; k < data.length; k++) {
    i = (i + 1) & 255; j = (j + s[i]) & 255;
    [s[i], s[j]] = [s[j], s[i]];
    out[k] = data[k] ^ s[(s[i] + s[j]) & 255];
  }
  return out;
}

const USER = "hylo", OWNER = "owner";
const P = -1;                                    // permissions: allow everything
const id = md5(Buffer.from("hylopdf-fixture"));  // arbitrary but stable

const O = rc4(md5(pad(OWNER)).subarray(0, 5), pad(USER));
const pBytes = Buffer.alloc(4);
pBytes.writeInt32LE(P);
const key = md5(Buffer.concat([pad(USER), O, pBytes, id])).subarray(0, 5);
const U = rc4(key, PAD);

/** The per-object key: the file key plus the object and generation numbers. */
const objKey = (num, gen) => {
  const extra = Buffer.from([num & 255, (num >> 8) & 255, (num >> 16) & 255, gen & 255, (gen >> 8) & 255]);
  return md5(Buffer.concat([key, extra])).subarray(0, Math.min(key.length + 5, 16));
};

const text = "BT /F1 24 Tf 72 700 Td (Locked, but not broken.) Tj ET";
const stream = rc4(objKey(4, 0), Buffer.from(text, "latin1"));

const objects = [
  `<< /Type /Catalog /Pages 2 0 R >>`,
  `<< /Type /Pages /Count 1 /Kids [3 0 R] >>`,
  `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>`,
  { head: `<< /Length ${stream.length} >>`, body: stream },
  `<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>`,
  `<< /Filter /Standard /V 1 /R 2 /O <${O.toString("hex")}> /U <${U.toString("hex")}> /P ${P} >>`,
];

const parts = [Buffer.from("%PDF-1.4\n", "latin1")];
const offsets = [0];
let at = parts[0].length;
objects.forEach((obj, i) => {
  offsets.push(at);
  const head = typeof obj === "string" ? obj : obj.head;
  let chunk = Buffer.from(`${i + 1} 0 obj\n${head}\n`, "latin1");
  if (typeof obj !== "string") {
    chunk = Buffer.concat([chunk, Buffer.from("stream\n", "latin1"), obj.body,
                           Buffer.from("\nendstream\n", "latin1")]);
  }
  chunk = Buffer.concat([chunk, Buffer.from("endobj\n", "latin1")]);
  parts.push(chunk);
  at += chunk.length;
});

const xrefAt = at;
let xref = `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
for (let i = 1; i <= objects.length; i++) xref += `${String(offsets[i]).padStart(10, "0")} 00000 n \n`;
xref += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R /Encrypt 6 0 R `
      + `/ID [<${id.toString("hex")}> <${id.toString("hex")}>] >>\nstartxref\n${xrefAt}\n%%EOF\n`;
parts.push(Buffer.from(xref, "latin1"));

writeFileSync(process.argv[2], Buffer.concat(parts));
console.log(`wrote ${process.argv[2]} — user password "${USER}"`);
