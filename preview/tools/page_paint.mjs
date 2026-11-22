// The reference run of the page's own paint(): usage
//   node page_paint.mjs <preview.html> <data-dir> <dest-dir>
//
// It cuts the colour functions, the measuring and the painting out of preview.html (the page itself, read as
// text and never edited), runs them on an out folder, and writes what paint() produced as raw RGBA, one file
// per case, plus meta.json with the numbers the Rust port must also find. Node built-ins only. The page's
// `fetch` is replaced by one that reads <data-dir> and nothing else, so no network access is possible, and
// nothing is written outside <dest-dir>.

import fs from 'node:fs';
import path from 'node:path';

const [htmlPath, dataDir, destDir] = process.argv.slice(2);
if (!htmlPath || !dataDir || !destDir) {
  console.error('usage: node page_paint.mjs <preview.html> <data-dir> <dest-dir>');
  process.exit(2);
}

const lines = fs.readFileSync(htmlPath, 'utf8').split(/\r?\n/);

function need(found, what) {
  if (found === -1) throw new Error(`page_paint: ${what} not found in ${htmlPath}; the page changed`);
  return found;
}

// (1) the colours, up to the start of the data section.
const from = need(lines.findIndex(l => l.includes('// ---------- colours ----------')), 'the colours marker');
const to = need(lines.findIndex(l => l.includes('// ---------- the data and what we measure on it ----------')), 'the data marker');
if (to <= from) throw new Error('page_paint: the markers are out of order');
const colours = lines.slice(from, to);

// (2) the single-line constants.
const constants = [];
for (const name of ['heightSteps', 'seaDepths', 'clamp']) {
  const line = lines.find(l => new RegExp(`^  const ${name} = `).test(l));
  if (line === undefined) throw new Error(`page_paint: const ${name} not found in ${htmlPath}; the page changed`);
  constants.push(line);
}

// (3) the functions, each up to the first later line that is exactly two spaces and a brace.
const functions = [];
for (const name of ['zones', 'steepness', 'measure', 'listOf', 'structureList', 'bands', 'paint', 'loadStructures']) {
  const start = need(lines.findIndex(l => new RegExp(`^  (async )?function ${name}\\(`).test(l)), `function ${name}`);
  const end = need(lines.findIndex((l, i) => i > start && l === '  }'), `the end of function ${name}`);
  functions.push(...lines.slice(start, end + 1));
}

const prelude = `
let world, view, quiet = null, exporting = false, in3d = false;
let drawing3d, drawingCut;
const scheme = { matches: true };
const chosen = { biomes: new Set(), flat: new Set(), slope: new Set(), cliffs: new Set(), vegetation: new Set(), height: new Set(), temperature: new Set(), structures: new Set(), section: new Set() };
let captured = null;
const map = { width: 0, height: 0, classList: { toggle() {} } };
const pen = {
  createImageData: (w, h) => ({ width: w, height: h, data: new Uint8ClampedArray(w * h * 4) }),
  putImageData(img) { captured = img; },
};
const document = { createElement: () => ({ getContext: () => ({ putImageData(img) { captured = img; } }) }) };
const drawMarks = () => {};
const refresh3d = async () => {}, showCut = async () => {};
const text = {
  heightBands: (s, st, d) => Array(d.length + 1 + st.length + 1).fill(''),
  legend: { flat: ['', '', ''], temperature: ['', '', '', '', ''], slope: Array(6).fill(''), cliffs: Array(5).fill(''), vegetation: Array(5).fill('') },
};
`;

const driver = `
// loadWorld, line 1166: the same world the page builds.
const meta = await fetch('/preview/data/preview.json').then(r => r.ok ? r.json() : null);
const [heights, biomes] = await Promise.all([
  fetch('/preview/data/heights.bin').then(r => r.arrayBuffer()).then(b => new Int16Array(b)),
  fetch('/preview/data/biomes.bin').then(r => r.arrayBuffer()).then(b => new Uint16Array(b)),
]);
world = { ...meta, heights, biomeAt: biomes, colours: meta.biomes.map(b => biomeColour(b.id)), ...measure(meta, heights, biomes), section: null, structures: await loadStructures() };

const cases = [];
const run = (name, dark, key, ids = []) => {
  for (const k in chosen) chosen[k].clear();
  quiet = null; captured = null;
  for (const id of ids) chosen[key].add(id);
  scheme.matches = dark;
  view = key;
  paint();
  if (!captured) throw new Error('paint() produced no picture for ' + name);
  emit(name, captured.data);
  cases.push(name);
};
for (const [scheme_name, dark] of [['dark', true], ['light', false]]) {
  for (const key of ['biomes', 'flat', 'slope', 'cliffs', 'vegetation', 'height', 'temperature', 'structures']) run(key + '_' + scheme_name, dark, key);
}
run('biomes_chosen_dark', true, 'biomes', ['minecraft:badlands', 'minecraft:savanna']);
run('height_chosen_light', false, 'height', ['3', '4']);

const s = world.structures;
return {
  n: world.samples,
  cases,
  biomeColours: world.colours,
  structures: s ? { ids: s.ids, colours: s.colours, under: s.under, order: s.order, count: Array.from(s.count) } : null,
  flat: world.flatAt.reduce((a, v) => a + v, 0),
};
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
const emit = (name, data) => fs.writeFileSync(path.join(destDir, `${name}.rgba`), Buffer.from(data.buffer, data.byteOffset, data.byteLength));

const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const body = [prelude, ...colours, ...constants, ...functions, driver].join('\n');
const result = await new AsyncFunction('fetch', 'emit', body)(localFetch, emit);
fs.writeFileSync(path.join(destDir, 'meta.json'), JSON.stringify(result));
console.log(`page_paint: ${result.cases.length} cases, ${result.n}x${result.n}, written to ${destDir}`);
