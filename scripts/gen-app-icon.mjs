// Erzeugt das App-Icon (1024 px PNG): dunkles Squircle, glühendes Quadrat, Leiterbahnen mit Kupferpads.
import { writeFileSync } from "node:fs";
import { deflateSync } from "node:zlib";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const N = 1024;
const SS = 3; // Supersampling pro Achse
const out = resolve(dirname(fileURLToPath(import.meta.url)), "../apps/desktop/app-icon.png");

const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
const mix = (a, b, t) => a.map((v, i) => v + (b[i] - v) * t);
const clamp = (v, lo = 0, hi = 1) => Math.min(hi, Math.max(lo, v));

const sdRoundRect = (x, y, cx, cy, hw, hh, r) => {
  const dx = Math.abs(x - cx) - (hw - r);
  const dy = Math.abs(y - cy) - (hh - r);
  return Math.hypot(Math.max(dx, 0), Math.max(dy, 0)) + Math.min(Math.max(dx, dy), 0) - r;
};
const sdSegment = (x, y, ax, ay, bx, by) => {
  const px = x - ax, py = y - ay, bxx = bx - ax, byy = by - ay;
  const h = clamp((px * bxx + py * byy) / (bxx * bxx + byy * byy));
  return Math.hypot(px - bxx * h, py - byy * h);
};

const traces = [
  [[362, 512], [300, 512], [250, 462], [250, 360]],
  [[662, 512], [724, 512], [774, 562], [774, 664]],
  [[512, 362], [512, 300], [560, 252], [660, 252]],
  [[512, 662], [512, 724], [464, 772], [364, 772]],
];

const BG_TOP = hex("#171a1e"), BG_BOT = hex("#0b0d0f");
const EDGE = hex("#2a3037"), TRACE = hex("#343b43"), COPPER = hex("#c08552");
const G1 = hex("#ffa070"), G2 = hex("#ff7f3f"), G3 = hex("#e2561f");

function shade(x, y) {
  // Rückgabe: [r, g, b, a] in 0..255 bzw. 0..1
  const d = sdRoundRect(x, y, 512, 512, 412, 412, 190);
  if (d > 0) return [0, 0, 0, 0];
  let c = mix(BG_TOP, BG_BOT, y / N);
  // dezenter Rand
  if (d > -3) c = mix(c, EDGE, 0.9);
  // Glut-Schein
  const r = Math.hypot(x - 512, y - 512);
  c = mix(c, G2, 0.30 * Math.exp(-((r / 250) ** 2)));
  // Leiterbahnen
  for (const t of traces) {
    let best = 1e9;
    for (let i = 0; i < t.length - 1; i++) best = Math.min(best, sdSegment(x, y, ...t[i], ...t[i + 1]));
    if (best < 7) c = mix(c, TRACE, clamp(7.5 - best));
    const [ex, ey] = t[t.length - 1];
    const pd = Math.hypot(x - ex, y - ey);
    if (pd < 24) c = mix(c, COPPER, clamp(24.5 - pd));
    if (pd < 9) c = mix(c, hex("#0b0d0f"), clamp(9.5 - pd));
  }
  // Kern
  const core = sdRoundRect(x, y, 512, 512, 150, 150, 58);
  if (core < 0.5) {
    const ty = clamp((y - 362) / 300);
    let k = ty < 0.5 ? mix(G1, G2, ty / 0.5) : mix(G2, G3, (ty - 0.5) / 0.5);
    const edge = sdRoundRect(x, y, 512, 512, 146, 146, 54);
    if (edge > -2) k = mix(k, G1, 0.35);
    c = mix(c, k, clamp(0.5 - core));
  }
  return [c[0], c[1], c[2], 1];
}

const raw = Buffer.alloc(N * (N * 4 + 1));
for (let py = 0; py < N; py++) {
  raw[py * (N * 4 + 1)] = 0;
  for (let px = 0; px < N; px++) {
    let r = 0, g = 0, b = 0, a = 0;
    for (let sy = 0; sy < SS; sy++)
      for (let sx = 0; sx < SS; sx++) {
        const s = shade(px + (sx + 0.5) / SS, py + (sy + 0.5) / SS);
        r += s[0] * s[3]; g += s[1] * s[3]; b += s[2] * s[3]; a += s[3];
      }
    const o = py * (N * 4 + 1) + 1 + px * 4;
    const n = SS * SS;
    raw[o] = a ? Math.round(r / a) : 0;
    raw[o + 1] = a ? Math.round(g / a) : 0;
    raw[o + 2] = a ? Math.round(b / a) : 0;
    raw[o + 3] = Math.round((a / n) * 255);
  }
}

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
};
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(N, 0); ihdr.writeUInt32BE(N, 4); ihdr[8] = 8; ihdr[9] = 6;
writeFileSync(out, Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr), chunk("IDAT", deflateSync(raw, { level: 9 })), chunk("IEND", Buffer.alloc(0)),
]));
console.log("app-icon.png geschrieben");
