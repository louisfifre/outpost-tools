// The reference run of the page's own figures, lists and history: usage
//   node page_figures.mjs <preview.html> <data-dir> <dest-dir>
//
// It cuts measure(), the lists, showFigures(), remember(), showHistory() and what they use out of preview.html
// (the page itself, read as text and never edited), runs them on an out folder with a few stubs for the DOM, and
// writes what they produced to <dest-dir>/meta.json for the Rust port to be held against. Node built-ins only. The
// page's `fetch` is replaced by one that reads <data-dir> and nothing else, so no network access is possible, and
// nothing is written outside <dest-dir>.

import fs from 'node:fs';
import path from 'node:path';

const [htmlPath, dataDir, destDir] = process.argv.slice(2);
if (!htmlPath || !dataDir || !destDir) {
  console.error('usage: node page_figures.mjs <preview.html> <data-dir> <dest-dir>');
  process.exit(2);
}

const lines = fs.readFileSync(htmlPath, 'utf8').split(/\r?\n/);

function need(found, what) {
  if (found === -1) throw new Error(`page_figures: ${what} not found in ${htmlPath}; the page changed`);
  return found;
}

// (1) the colours, up to the start of the data section (the same cut as page_paint's).
const from = need(lines.findIndex(l => l.includes('// ---------- colours ----------')), 'the colours marker');
const to = need(lines.findIndex(l => l.includes('// ---------- the data and what we measure on it ----------')), 'the data marker');
if (to <= from) throw new Error('page_figures: the markers are out of order');
const colours = lines.slice(from, to);

// (2) the text object: from its opening line to the first later line that is exactly two spaces and `};`.
const textStart = need(lines.findIndex(l => l === '  const text = {'), 'the text object');
const textEnd = need(lines.findIndex((l, i) => i > textStart && l === '  };'), 'the end of the text object');
const textObject = lines.slice(textStart, textEnd + 1);

// (3) the single-line constants.
const constants = [];
for (const name of ['num', 'percent', 'blocks', 'heightSteps', 'seaDepths', 'clamp']) {
  const line = lines.find(l => new RegExp(`^  const ${name} = `).test(l));
  if (line === undefined) throw new Error(`page_figures: const ${name} not found in ${htmlPath}; the page changed`);
  constants.push(line);
}

// (4) the functions, each up to the first later line that is exactly two spaces and a brace.
const functions = [];
for (const name of ['zones', 'steepness', 'measure', 'listOf', 'structureList', 'bands', 'loadStructures', 'showFigures', 'remember', 'showHistory', 'changes']) {
  const start = need(lines.findIndex(l => new RegExp(`^  (async )?function ${name}\\(`).test(l)), `function ${name}`);
  const end = need(lines.findIndex((l, i) => i > start && l === '  }'), `the end of function ${name}`);
  functions.push(...lines.slice(start, end + 1));
}

// (5) runsHere is an arrow function: from its line to the first later line that is exactly `  };`.
const runsStart = need(lines.findIndex(l => l.startsWith('  const runsHere = ')), 'const runsHere');
const runsEnd = need(lines.findIndex((l, i) => i > runsStart && l === '  };'), 'the end of const runsHere');
const runsHere = lines.slice(runsStart, runsEnd + 1);

const prelude = `
// What the page's DOM and storage give these functions, in memory.
const store = new Map();
const localStorage = { getItem: k => store.has(k) ? store.get(k) : null, setItem: (k, v) => { store.set(k, String(v)); } };
let history = [], pointed = null, before = null, world, shown;
const element = (tag) => ({ tag, textContent: '', className: '', title: '', children: [], append(...c) { this.children.push(...c); } });
const document = { createElement: tag => element(tag) };
document.createElementNS = (ns, tag) => ({ tag, attrs: {}, children: [], setAttribute(k, v) { this.attrs[k] = v; }, append(...c) { this.children.push(...c); } });
const els = {
  figures: { hidden: false, clientWidth: 330, replaceChildren(...c) { this.kids = c; } },
  history: { hidden: false, clientWidth: 330, replaceChildren(...c) { this.kids = c; } },
};
const $ = id => els[id];
`;

const driver = `
// loadWorld, line 1166: the same world the page builds.
const meta = await fetch('/preview/data/preview.json').then(r => r.ok ? r.json() : null);
const [heights, biomes] = await Promise.all([
  fetch('/preview/data/heights.bin').then(r => r.arrayBuffer()).then(b => new Int16Array(b)),
  fetch('/preview/data/biomes.bin').then(r => r.arrayBuffer()).then(b => new Uint16Array(b)),
]);
world = { ...meta, heights, biomeAt: biomes, colours: meta.biomes.map(b => biomeColour(b.id)), ...measure(meta, heights, biomes), section: null, structures: await loadStructures() };
const run = await fetch('/preview/data/run.json').then(r => r.ok ? r.json() : null);
if (!run) throw new Error('page_figures: run.json is not in the data folder');
shown = { seed: String(run.seed), values: run.tectonic };

const out = { figures: world.figures, absent: world.absent, lists: {}, showFigures: [], history: {}, runValues: run.tectonic };

// The lists the column holds, in the order of the native views.
for (const kind of ['biomes', 'flat', 'slope', 'cliffs', 'vegetation', 'height', 'temperature', 'structures']) {
  const L = listOf(kind);
  if (!L) { out.lists[kind] = null; continue; }
  const cells = Array.from(L.cells), widths = Array.from(L.widths);
  out.lists[kind] = {
    ids: L.ids, names: L.names, colours: L.colours.map(c => Array.from(c)), cells, widths, counted: !!L.counted, total: L.total ?? null,
    widthText: widths.map(blocks),
    shareText: cells.map(c => L.counted ? num(c) : percent(100 * c / world.samples ** 2)),
  };
}

// showFigures with no run before, with one that differs in every way a figure can, and with one over another area.
const f = world.figures;
const B1 = { ...f, ocean: f.ocean + 0.34, flat: f.flat - 1.06, top: f.top - 3, typical: f.typical + 49, expanse: f.expanse + 160, count: f.count - 2 };
for (const B of [null, B1, { ...B1, span: B1.span * 2 }]) {
  before = B;
  showFigures();
  out.showFigures.push(els.figures.kids.map(k => [k.textContent, k.className]));
}

// Five maps came to the screen: remember() keeps four of this seed's and one of another.
const v = shown.values, seed = shown.seed;
const step = (seed, values, figures) => { shown = { seed, values }; world.figures = figures; remember(); };
step(seed, v, f);
step(seed, { ...v, ocean_offset: v.ocean_offset + 0.05 }, { ...f, flat: f.flat + 2.3, expanse: f.expanse + 180, typical: f.typical - 75, ocean: f.ocean - 1.2, high: f.high + 0.4 });
step(seed + '1', v, f);
step(seed, { ...v, continents_scale: v.continents_scale + 0.01, ocean_depth: v.ocean_depth + 0.01, vertical_scale: v.vertical_scale + 0.01 }, { ...f, flat: null });
step(seed, v, f);
shown = { seed, values: v };
world.figures = f;
out.history.stored = localStorage.getItem('preview.history');

for (const p of [null, 0, 2]) {
  pointed = p;
  showHistory();
  const kids = els.history.kids;
  const probe = kids[kids.length - 1];
  out.history['pointed' + p] = {
    rows: kids.slice(0, -1).map(row => {
      const [label, value, plot] = row.children;
      const now = plot.children.find(c => c.attrs.class === 'now');
      const line = plot.children.find(c => c.attrs.class === 'line');
      const dot = plot.children.find(c => c.attrs.class === 'dot');
      return [label.textContent, value.textContent, now.attrs.x1, line.attrs.d, dot ? [dot.attrs.cx, dot.attrs.cy] : null];
    }),
    probe: probe.textContent,
  };
}
return out;
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
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const body = [prelude, ...colours, ...textObject, ...constants, ...functions, ...runsHere, driver].join('\n');
const result = await new AsyncFunction('fetch', body)(localFetch);
fs.writeFileSync(path.join(destDir, 'meta.json'), JSON.stringify(result));
console.log(`page_figures: ${Object.keys(result.lists).length} lists, ${result.showFigures.length} figure cases, history written to ${destDir}`);
