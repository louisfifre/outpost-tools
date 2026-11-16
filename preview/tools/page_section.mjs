// The reference run of the page's own section code: usage
//   node page_section.mjs <preview.html> <data-dir> <dest-dir>
//
// It cuts the section block (the ore rules, groundOf, orePlay), the cut's look and its drawing, and the strip out of
// preview.html (the page itself, read as text and never edited), runs them in Node on an out folder, and writes what
// they produced: the rules, the ground of every cut, the figures and the drawn layer of two cuts, the strip, and two
// pictures of the cut with the look and the frame that went with them. The Rust port must find the same numbers and
// the same bytes. Node built-ins only. The page's `fetch` is replaced by one that reads <data-dir> and nothing else,
// so no network access is possible, and nothing is written outside <dest-dir>.

import fs from 'node:fs';
import path from 'node:path';

const [htmlPath, dataDir, destDir] = process.argv.slice(2);
if (!htmlPath || !dataDir || !destDir) {
  console.error('usage: node page_section.mjs <preview.html> <data-dir> <dest-dir>');
  process.exit(2);
}

const lines = fs.readFileSync(htmlPath, 'utf8').split(/\r?\n/);

function need(found, what) {
  if (found === -1) throw new Error(`page_section: ${what} not found in ${htmlPath}; the page changed`);
  return found;
}

// (1) the section block: from its marker up to the comment that opens the cut's look.
const from = need(lines.findIndex(l => l.includes('// ---------- the section: a cut through the ground')), 'the section marker');
const to = need(lines.findIndex(l => l.includes('// The cut is looked at through a window')), 'the look marker');
if (to <= from) throw new Error('page_section: the markers are out of order');
const block = lines.slice(from, to);

// (2) the single-line constants.
const constants = [];
for (const name of ['clamp', 'num']) {
  const line = lines.find(l => new RegExp(`^  const ${name} = `).test(l));
  if (line === undefined) throw new Error(`page_section: const ${name} not found in ${htmlPath}; the page changed`);
  constants.push(line);
}

// (3) the functions, each up to the first later line that is exactly two spaces and a brace.
const functions = [];
for (const name of ['hashOf', 'cutScales', 'showCutLook', 'drawCut', 'drawNav', 'showNavFrame']) {
  const start = need(lines.findIndex(l => new RegExp(`^  (async )?function ${name}\\(`).test(l)), `function ${name}`);
  const end = need(lines.findIndex((l, i) => i > start && l === '  }'), `the end of function ${name}`);
  functions.push(...lines.slice(start, end + 1));
}

// What the page's elements and canvases are, as far as these functions touch them. The drawing calls are captured.
const prelude = `
let world = null, view = 'section';
const devicePixelRatio = 1;
const chosen = { section: new Set() };
let cutFrame = 0;
const cutLook = { k: 0, x: 0, y: 0 };
let navRange = null;
const requestAnimationFrame = f => { f(); return 0; };
const box = { w: 0, h: 0 };
const cutView = { getBoundingClientRect: () => ({ left: 0, top: 0, width: box.w, height: box.h }), clientWidth: 0, clientHeight: 0 };
const nav = { clientWidth: 0, clientHeight: 0, getBoundingClientRect: () => ({ left: 0, top: 0, width: nav.clientWidth, height: 56 }) };
const navFrame = { style: {} };
const els = {};
const $ = id => (els[id] ??= { textContent: '' });
const text = { navNote: t => t };
const got = { nav: null, sheet: null, image: null };
const navCanvas = { width: 0, height: 0, getContext: () => ({
  createImageData: (w, h) => ({ width: w, height: h, data: new Uint8ClampedArray(w * h * 4) }),
  putImageData(img) { got.nav = img; },
}) };
class OffscreenCanvas {
  constructor(w, h) { this.width = w; this.height = h; this.img = null; }
  getContext() {
    return {
      createImageData: (w, h) => ({ width: w, height: h, data: new Uint8ClampedArray(w * h * 4) }),
      putImageData: img => { this.img = img; },
    };
  }
}
const cutCanvas = { width: 0, height: 0, getContext: () => ({
  imageSmoothingEnabled: true,
  drawImage(sheet, x, y, w, h) { got.sheet = sheet.img; got.image = [x, y, w, h]; },
}) };
`;

const driver = `
const d_meta = await fetch('/preview/data/preview.json').then(r => r.ok ? r.json() : null);
const d_biomeAt = await fetch('/preview/data/biomes.bin').then(r => r.arrayBuffer()).then(b => new Uint16Array(b));
world = { ...d_meta, biomeAt: d_biomeAt };
const d_section = await loadSection();
if (!d_section) throw new Error('no section in the data folder');
world.section = d_section;
const d_out = { cuts: d_section.cuts.length, length: d_section.length, height: d_section.height, minY: d_section.minY };

// The rules, and the rows the mods add.
d_out.rules = d_section.rules.map(r => ({ id: r.id, index: r.index, kind: r.kind, playable: r.playable, anchored: r.anchored }));
d_out.rows = rowsOfMods(d_section);
d_out.unplayed = d_section.rules.filter(rule => !rule.playable).length;

const d_cellsOf = async cut => new Uint8Array(await fetch('/preview/data/' + cut.file).then(r => r.arrayBuffer()));
const d_f64 = a => Buffer.from(new Float64Array(a).buffer);

// The ground of every cut, then the play of cuts 0 and 2.
d_out.ground = [];
d_out.play = {};
const d_cells = [];
for (let i = 0; i < d_section.cuts.length; i++) {
  const cut = d_section.cuts[i];
  const cells = await d_cellsOf(cut);
  d_cells.push(cells);
  const g = groundOf(d_section, cells, cut);
  emit('ground_' + i + '_surface.f64', d_f64(g.surface));
  emit('ground_' + i + '_floor.f64', d_f64(g.floor));
  d_out.ground.push({ chunks: g.chunks, high: Array.from(g.high), biomes: g.biomes.map(b => b ?? null), top: g.top, bottom: g.bottom, peak: g.peak, crop: g.crop, rows: g.rows });
}
for (const i of [0, 2]) {
  const cut = d_section.cuts[i];
  d_section.at = i;
  const g = groundOf(d_section, d_cells[i], cut);
  const play = orePlay(d_section, d_cells[i], cut, g, rowsOfMods(d_section));
  d_out.play[i] = { perChunk: play.perChunk, counted: play.counted };
  play.drawChunks(0, g.chunks - 1);
  emit('layer_' + i + '.u8', Buffer.from(play.layer.buffer, play.layer.byteOffset, play.layer.byteLength));
}

// The strip and the two pictures. showCut (2335-2360) builds cutShown like this.
const d_show = i => {
  const cut = d_section.cuts[i];
  d_section.at = i;
  const ground = groundOf(d_section, d_cells[i], cut);
  const play = orePlay(d_section, d_cells[i], cut, ground, rowsOfMods(d_section));
  cutShown = { section: d_section, cut, cells: d_cells[i], ground, play, perChunk: play.perChunk };
};
const d_style = () => Object.fromEntries(['left', 'width', 'top', 'bottom'].map(k => [k, parseFloat(navFrame.style[k])]));
const d_case = (name, i, w, h, look, picked) => {
  d_show(i);
  nav.clientWidth = 638; nav.clientHeight = 55;
  drawNav();
  const navOut = { text: $('navNote').textContent, range: navRange };
  if (name === 'A') emit('nav.rgba', Buffer.from(got.nav.data.buffer, got.nav.data.byteOffset, got.nav.data.byteLength));
  box.w = w; box.h = h; cutView.clientWidth = w; cutView.clientHeight = h - 1;
  Object.assign(cutLook, look);
  chosen.section = new Set(picked);
  got.sheet = null;
  showCutLook();
  if (!got.sheet) throw new Error('case ' + name + ' drew nothing');
  emit('sheet_' + name + '.rgba', Buffer.from(got.sheet.data.buffer, got.sheet.data.byteOffset, got.sheet.data.byteLength));
  return { nav: navOut, look: { ...cutLook }, image: got.image, size: [got.sheet.width, got.sheet.height], frame: d_style(), box: [w, h], cut: i, picked };
};
d_out.cases = {
  A: d_case('A', 0, 638, 700, { k: 0, x: 0, y: 0 }, []),
  B: d_case('B', 2, 638, 300, { k: 0.6, x: -1000, y: 0 }, ['iron']),
  // Zoomed in with the window inside the cut's height: the frame on the strip has a top and a bottom.
  C: d_case('C', 1, 638, 300, { k: 4, x: -3000, y: -500 }, []),
};
return d_out;
`;

// The page's fetch is shadowed: only files of the data folder, by their name, and nothing from the network.
function localFetch(url) {
  const file = path.join(dataDir, path.basename(String(url)));
  if (!fs.existsSync(file)) return Promise.resolve({ ok: false });
  const bytes = fs.readFileSync(file);
  return Promise.resolve({
    ok: true,
    json: async () => JSON.parse(bytes.toString('utf8')),
    arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
  });
}

fs.mkdirSync(destDir, { recursive: true });
const emit = (name, data) => fs.writeFileSync(path.join(destDir, name), data);

const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const body = [prelude, ...block, ...constants, ...functions, driver].join('\n');
const result = await new AsyncFunction('fetch', 'emit', body)(localFetch, emit);
fs.writeFileSync(path.join(destDir, 'meta.json'), JSON.stringify(result));
console.log(`page_section: ${result.cuts} cuts, ${result.rules.length} ore rules, written to ${destDir}`);
