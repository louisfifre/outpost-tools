//! The 3D view of the page (preview.html `refresh3d`, lines 1842-1907), shared by the preview's stage and the
//! relief demo: the land draped in the map's picture, the water, the orbit camera and the ground under it. One
//! unit is one block, heights included.
//!
//! The land is drawn at one of three levels (`Level`), from the lightest to the finest:
//!
//! - slopes: one sloped surface through the samples kept, two triangles between four of them. The lightest, meant
//!   to run on any machine.
//! - blocks: one column per sample kept, a square as wide as the samples are apart, with a flat top at the sample's
//!   height and an upright side down to each neighbour that is lower. No face is sloped, so the relief reads as
//!   large blocks standing in space, as the game's distant land does.
//! - fine blocks: the same columns, one per sample of the map, so every square of the 2D map is a block.
//!
//! Whatever the level, the water lies only where the ground is under the sea, the underside closes the land past
//! its edge, and `ground_at` is the height of the surface that is drawn.

use mere_base::{Color, Gfx, ImageId, Material, MeshId, Orbit, Rect, Vec3, Vertex};

use crate::colour::steep_colour;
use crate::paint::View;
use crate::world::World;

/// The land: the sloped surface, or the columns' tops, draped in the map's picture.
pub const LAND_MESH: MeshId = MeshId(15);
pub const WATER_MESH: MeshId = MeshId(16);
pub const WALL_MESH: MeshId = MeshId(19);
/// The walls' one pixel of colour.
pub const WALL_IMAGE: ImageId = ImageId(20);
/// The page's water (line 1901).
pub const WATER_COLOUR: Color = Color::hex(0x2f5c9e);
/// The columns' upright sides, in `SIDE_IMAGE`. The slopes have none.
pub const SIDE_MESH: MeshId = MeshId(23);
/// The map's colour of each column, in each of the two shades a side can take (`side_image`).
pub const SIDE_IMAGE: ImageId = ImageId(24);
/// The shade of a side facing north, east, south and west: the game's own, so the steps read as its blocks do.
/// Two sides that face each other share a shade, so the picture holds two shades, not four.
pub const SIDE_SHADE: [f32; 4] = [0.8, 0.6, 0.8, 0.6];
const _: () = assert!(SIDE_SHADE[0] == SIDE_SHADE[2] && SIDE_SHADE[1] == SIDE_SHADE[3]);

/// How finely the land is drawn in 3D: the person's choice, by what their machine holds.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum Level {
    /// A sloped surface through at most `COARSE_MOST` samples a side.
    Slopes,
    /// A column per sample kept, at most `COARSE_MOST` a side.
    #[default]
    Blocks,
    /// A column per sample of the map, up to `FINE_MOST` a side.
    Fine,
}

/// The most samples a side the slopes and the blocks keep: a wider map gives one sample in two (or fewer), which
/// the eye does not miss at that distance (the page does the same).
pub const COARSE_MOST: usize = 512;
/// The most columns a side the fine blocks keep: the widest map the preview makes (32,768 blocks at one sample
/// every 16), so each of its samples is a column. Measured on that map (2026-11-18, `examples/relief_measure.rs`,
/// a Radeon RX 7900 XT at 120 frames a second): 50 million vertices, 1.4 GB of meshes, 1.2 s to build and send them,
/// 4 to 5 ms of the card a frame. A wider map would be thinned for the fine blocks too.
pub const FINE_MOST: usize = 2048;

impl Level {
    /// The three levels, from the lightest to the finest: the order the interface lists them in.
    pub const ALL: [Level; 3] = [Level::Slopes, Level::Blocks, Level::Fine];

    /// The level's place in `ALL`.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The word a command line names the level by.
    pub fn key(self) -> &'static str {
        ["slopes", "blocks", "fine"][self.index()]
    }

    pub fn from_key(key: &str) -> Option<Level> {
        Level::ALL.into_iter().find(|l| l.key() == key)
    }

    /// The most samples a side the level keeps.
    fn most(self) -> usize {
        if self == Level::Fine { FINE_MOST } else { COARSE_MOST }
    }

    /// The land is columns, not a sloped surface.
    fn blocks(self) -> bool {
        self != Level::Slopes
    }
}

/// What the walls round the land are painted.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum WallColour {
    /// The stage's own ground: the land reads as cut clean, and nothing shows under it.
    Ground,
    /// The Flat view's steep ground (`colour::steep_colour`), a dark neutral earth that holds on both themes.
    Earth,
}

/// How the land's edge is finished.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Edge {
    /// No wall: past the edge one sees the land's underside, grey and lit from below (`underside`).
    Underside,
    /// A wall round the square down to `base`, in one plain colour (`walls`).
    Wall(WallColour),
}

/// The one place the edge, and the walls' colour, are chosen.
pub const EDGE: Edge = Edge::Underside;

/// Where every vertex of the walls samples `WALL_IMAGE`: its one texel.
pub const WALL_UV: [f32; 2] = [0.5, 0.5];

pub const UNDER_MESH: MeshId = MeshId(21);
/// The underside's shades, one texel per point it hangs from (`underside_image`).
pub const UNDER_IMAGE: ImageId = ImageId(22);
/// Blocks between the land and its underside, at the least. The underside hangs from the land's own points, this
/// much lower (under blocks, from the lowest top round each point), so it is under the land everywhere and the
/// depth test keeps the land in front from above (the depth is 32-bit float and reversed, so half a block holds at
/// any distance the camera reaches).
pub const UNDER_GAP: f32 = 0.5;
/// Towards the light that shades the underside: from below, off to the north-west, so its slopes seen from under
/// read.
const UNDER_LIGHT: [f32; 3] = [-0.4, -0.8, -0.45];
/// The underside's darkest and lightest grey: a face turned away from the light, and one turned to it. The darkest
/// stays well off black, so the relief still reads on the sides the light does not reach.
const UNDER_GREYS: (f32, f32) = (100.0, 200.0);

// Blocks kept between the camera and the ground under it: the near plane is ten blocks, and the camera must not
// pass through the land it is turning round.
const MARGIN: f32 = 24.0;

/// The four directions a side faces, as (column, row) steps: north, east, south, west. A row is a step in z.
const DIRS: [(isize, isize); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
/// A quad's corners in `face` and `square`, wound to face out (or up), and the other way.
const OUT: [u32; 6] = [0, 1, 2, 1, 3, 2];
const IN: [u32; 6] = [0, 2, 1, 1, 2, 3];

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    fn new() -> Mesh {
        Mesh { vertices: Vec::new(), indices: Vec::new() }
    }

    /// Four corners, all on `uv`, as two triangles in `order`.
    fn quad(&mut self, corners: [[f32; 3]; 4], uv: [f32; 2], order: [u32; 6]) {
        let first = self.vertices.len() as u32;
        self.vertices.extend(corners.iter().map(|&pos| Vertex { pos, uv }));
        self.indices.extend(order.iter().map(|i| first + i));
    }

    /// mere-ui refuses an empty buffer, and would keep the mesh drawn before: a mesh with nothing to draw gets
    /// one empty triangle.
    fn never_empty(mut self) -> Mesh {
        if self.indices.is_empty() {
            self.vertices.push(Vertex::default());
            self.indices.extend_from_slice(&[0, 0, 0]);
        }
        self
    }
}

/// The samples a level keeps: one every `skip`, `m` a side, `cell` blocks apart, over a square `half` blocks from
/// its middle, which is the origin so that the camera can turn round it. Slopes join the samples kept, so their
/// square ends on the last one; a column stands on each, so the blocks' square is one cell wider: the area itself
/// when the samples kept divide it evenly.
#[derive(Copy, Clone)]
struct Grid {
    samples: usize,
    skip: usize,
    m: usize,
    cell: f32,
    half: f32,
}

fn grid(world: &World, level: Level) -> Grid {
    let n = world.samples;
    let skip = n.div_ceil(level.most());
    let m = (n - 1) / skip + 1;
    let cell = (skip as f64 * world.step) as f32;
    let cells = if level.blocks() { m } else { m - 1 };
    Grid { samples: n, skip, m, cell, half: cells as f32 * cell / 2.0 }
}

impl Grid {
    /// The height of the sample kept at (col, row).
    fn height(&self, world: &World, col: usize, row: usize) -> f32 {
        world.heights[row * self.skip * self.samples + col * self.skip] as f32
    }

    /// Where line `i` of the grid stands, along x or z: sample `i` under slopes, the western (or northern) edge of
    /// column `i` under blocks.
    fn at(&self, i: usize) -> f32 {
        i as f32 * self.cell - self.half
    }

    /// The middle of the texel of the sample kept at (col, row) in the map's picture.
    fn drape_uv(&self, col: usize, row: usize) -> [f32; 2] {
        let n = self.samples as f32;
        [((col * self.skip) as f32 + 0.5) / n, ((row * self.skip) as f32 + 0.5) / n]
    }

    /// The neighbour of (col, row) towards `DIRS[d]`, if the square has one.
    fn neighbour(&self, col: usize, row: usize, d: usize) -> Option<(usize, usize)> {
        let (dc, dr) = DIRS[d];
        let (c, r) = (col as isize + dc, row as isize + dr);
        (c >= 0 && r >= 0 && (c as usize) < self.m && (r as usize) < self.m).then_some((c as usize, r as usize))
    }

    /// Column (col, row)'s footprint: [west x, north z, east x, south z].
    fn footprint(&self, col: usize, row: usize) -> [f32; 4] {
        [self.at(col), self.at(row), self.at(col + 1), self.at(row + 1)]
    }

    /// A flat square over (col, row)'s footprint at `y`: corners (west, north), (east, north), (west, south),
    /// (east, south). `IN` faces it up, `OUT` down.
    fn square(&self, col: usize, row: usize, y: f32) -> [[f32; 3]; 4] {
        let [x0, z0, x1, z1] = self.footprint(col, row);
        [[x0, y, z0], [x1, y, z0], [x0, y, z1], [x1, y, z1]]
    }

    /// The upright face of column (col, row) towards `DIRS[d]`, from `top` down to `low`: its two ends at `top`,
    /// then at `low`, walked so that the column lies on the same hand for every side. `OUT` faces it towards `d`.
    fn face(&self, col: usize, row: usize, d: usize, top: f32, low: f32) -> [[f32; 3]; 4] {
        let [x0, z0, x1, z1] = self.footprint(col, row);
        let (a, b) = match d {
            0 => ([x0, z0], [x1, z0]),
            1 => ([x1, z0], [x1, z1]),
            2 => ([x1, z1], [x0, z1]),
            _ => ([x0, z1], [x0, z0]),
        };
        [[a[0], top, a[1]], [b[0], top, b[1]], [a[0], low, a[1]], [b[0], low, b[1]]]
    }

    /// Sample (col, row) as a vertex of the slopes: where it stands, in its own colour of the map.
    fn point(&self, world: &World, col: usize, row: usize) -> Vertex {
        Vertex { pos: [self.at(col), self.height(world, col, row), self.at(row)], uv: self.drape_uv(col, row) }
    }
}

/// The sheet of water's height: half a block under the sea level, so ground at that level is land and does not
/// lie in the water.
fn sea(world: &World) -> f32 {
    world.sea_level as f32 - 0.5
}

/// Where the walls round the land end: one step between two samples under both the lowest ground and the water.
/// Every wall then reaches under the ground and the sea beside it, and where the border is the lowest ground the
/// land still reads as a slab.
pub fn base(world: &World) -> f32 {
    let (lowest, _) = world.height_range();
    (lowest as f32).min(sea(world)) - world.step as f32
}

/// The land: the sloped surface through the samples kept, or the columns' flat tops, each in its sample's colour.
pub fn land(world: &World, level: Level) -> Mesh {
    let g = grid(world, level);
    let m = g.m;
    let mut mesh = Mesh::new();
    if level.blocks() {
        mesh.vertices.reserve(4 * m * m);
        mesh.indices.reserve(6 * m * m);
        for row in 0..m {
            for col in 0..m {
                mesh.quad(g.square(col, row, g.height(world, col, row)), g.drape_uv(col, row), IN);
            }
        }
    } else {
        mesh.vertices.reserve(m * m);
        for row in 0..m {
            for col in 0..m {
                mesh.vertices.push(g.point(world, col, row));
            }
        }
        mesh.indices.reserve((m - 1) * (m - 1) * 6);
        for row in 0..m - 1 {
            for col in 0..m - 1 {
                let a = (row * m + col) as u32;
                let (b, c, d) = (a + 1, a + m as u32, a + m as u32 + 1);
                mesh.indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
    }
    mesh
}

/// Where side `d` of column (col, row) samples `SIDE_IMAGE`: one half of the image per shade, north and south in
/// the first, east and west in the second.
fn side_uv(m: usize, col: usize, row: usize, d: usize) -> [f32; 2] {
    [(((d % 2) * m + col) as f32 + 0.5) / (2 * m) as f32, (row as f32 + 0.5) / m as f32]
}

/// The columns' upright sides: one wherever a neighbour is lower, from the column's top down to the neighbour's,
/// facing the neighbour, in the column's colour shaded for that direction. Equal neighbours have none, and
/// neither have the slopes.
pub fn sides(world: &World, level: Level) -> Mesh {
    let g = grid(world, level);
    let mut mesh = Mesh::new();
    if level.blocks() {
        for row in 0..g.m {
            for col in 0..g.m {
                let top = g.height(world, col, row);
                for d in 0..4 {
                    let Some((c, r)) = g.neighbour(col, row, d) else { continue };
                    let low = g.height(world, c, r);
                    if low < top {
                        mesh.quad(g.face(col, row, d, top, low), side_uv(g.m, col, row, d), OUT);
                    }
                }
            }
        }
    }
    mesh.never_empty()
}

/// `SIDE_IMAGE`, 2m wide and m high, RGBA, from the map's picture `drape` (samples by samples, RGBA): for each of
/// the two shades a half holding every column's colour times it.
pub fn side_image(world: &World, level: Level, drape: &[u8]) -> (u32, u32, Vec<u8>) {
    let g = grid(world, level);
    let (m, w) = (g.m, 2 * g.m);
    let mut rgba = vec![0u8; w * m * 4];
    for (half, shade) in SIDE_SHADE[..2].iter().enumerate() {
        // A byte times a shade, worked out once for the 256 bytes.
        let shaded: [u8; 256] = std::array::from_fn(|c| (c as f32 * shade).round() as u8);
        for row in 0..m {
            for col in 0..m {
                let from = (row * g.skip * g.samples + col * g.skip) * 4;
                let to = (row * w + half * m + col) * 4;
                for k in 0..3 {
                    rgba[to + k] = shaded[drape[from + k] as usize];
                }
                rgba[to + 3] = 255;
            }
        }
    }
    (w as u32, m as u32, rgba)
}

/// Uploads the map's picture for the columns' sides: it follows the view and the theme as the picture does, and
/// the level as the columns do. The slopes have no side, so no picture.
pub fn upload_sides(gfx: &mut dyn Gfx, world: &World, level: Level, drape: &[u8]) {
    if level.blocks() {
        let (w, h, rgba) = side_image(world, level, drape);
        gfx.upload_image(SIDE_IMAGE, w, h, &rgba);
    }
}

/// The water at `sea`, only where the ground is under it, so it ends at the shores and at the square's edge. A
/// sheet over the whole square would lie under the land wherever the land is higher, and from a low angle past the
/// edge it showed as a flat blue slab under the land.
///
/// Over blocks it is flat over each column whose top is under it. Over slopes it is, over each triangle of the
/// land, the part where the triangle's ground lies below it, cut along the straight line where the two meet.
pub fn water(world: &World, level: Level) -> Mesh {
    let g = grid(world, level);
    let y = sea(world);
    let mesh = if level.blocks() { water_over_blocks(world, &g, y) } else { water_over_slopes(world, &g, y) };
    mesh.never_empty()
}

fn water_over_blocks(world: &World, g: &Grid, y: f32) -> Mesh {
    let m = g.m;
    // The columns' corners, at the water's height, shared by the squares of water side by side: a corner gets its
    // vertex when a first square needs it, so land without water carries none.
    let corners = m + 1;
    let mut vertex_of = vec![u32::MAX; corners * corners];
    let mut mesh = Mesh::new();
    for row in 0..m {
        for col in 0..m {
            if g.height(world, col, row) < y {
                let a = row * corners + col;
                let corner = [a, a + 1, a + corners, a + corners + 1].map(|i| {
                    if vertex_of[i] == u32::MAX {
                        vertex_of[i] = mesh.vertices.len() as u32;
                        mesh.vertices.push(Vertex { pos: [g.at(i % corners), y, g.at(i / corners)], uv: [0.0, 0.0] });
                    }
                    vertex_of[i]
                });
                mesh.indices.extend(IN.iter().map(|&k| corner[k as usize]));
            }
        }
    }
    mesh
}

fn water_over_slopes(world: &World, g: &Grid, y: f32) -> Mesh {
    let m = g.m;
    let ground: Vec<[f32; 3]> = (0..m * m).map(|i| g.point(world, i % m, i / m).pos).collect();
    let at = |p: [f32; 3]| Vertex { pos: [p[0], y, p[2]], uv: [0.0, 0.0] };
    // Every vertex of the grid, at the sheet's height, for the triangles wholly under it; the cut ones add their own.
    let mut mesh = Mesh { vertices: ground.iter().map(|&p| at(p)).collect(), indices: Vec::new() };
    // Where the edge (i, j) meets the sheet, worked out from its lower index first so that the two triangles that
    // share the edge get the very same point and leave no crack.
    let cross = |i: usize, j: usize| {
        let (p, q) = if i < j { (ground[i], ground[j]) } else { (ground[j], ground[i]) };
        let t = (y - p[1]) / (q[1] - p[1]);
        [p[0] + (q[0] - p[0]) * t, y, p[2] + (q[2] - p[2]) * t]
    };
    for row in 0..m - 1 {
        for col in 0..m - 1 {
            let a = row * m + col;
            let (b, c, d) = (a + 1, a + m, a + m + 1);
            // The land's own two triangles, in its order, so the water faces up as the land does.
            for tri in [[a, c, b], [b, c, d]] {
                let under = tri.map(|i| ground[i][1] < y);
                if under.iter().all(|&u| u) {
                    mesh.indices.extend(tri.map(|i| i as u32));
                } else if under.iter().any(|&u| u) {
                    let mut part = Vec::with_capacity(4);
                    for k in 0..3 {
                        let (i, j) = (tri[k], tri[(k + 1) % 3]);
                        if under[k] {
                            part.push(ground[i]);
                        }
                        if under[k] != under[(k + 1) % 3] {
                            part.push(cross(i, j));
                        }
                    }
                    let first = mesh.vertices.len() as u32;
                    mesh.vertices.extend(part.iter().map(|&p| at(p)));
                    for k in 1..part.len() as u32 - 1 {
                        mesh.indices.extend_from_slice(&[first, first + k, first + k + 1]);
                    }
                }
            }
        }
    }
    mesh
}

/// The points the edge of the land is finished from, `k` a side over the square, with a height of the land at
/// each: under slopes the samples kept, each at its height; under blocks the corners of the columns, each at the
/// lowest of the tops that meet there. A surface between them, `UNDER_GAP` lower, lies under the land everywhere:
/// between four corners it is under the column's top, and along an edge under both columns' tops, so under the
/// side between them.
struct Lattice {
    k: usize,
    y: Vec<f32>,
}

fn lattice(world: &World, level: Level) -> Lattice {
    let g = grid(world, level);
    let m = g.m;
    if !level.blocks() {
        return Lattice { k: m, y: (0..m * m).map(|i| g.height(world, i % m, i / m)).collect() };
    }
    let k = m + 1;
    let mut y = vec![f32::MAX; k * k];
    for row in 0..m {
        for col in 0..m {
            let h = g.height(world, col, row);
            for i in [row * k + col, row * k + col + 1, (row + 1) * k + col, (row + 1) * k + col + 1] {
                y[i] = y[i].min(h);
            }
        }
    }
    Lattice { k, y }
}

/// The lattice's (col, row) round its border, one list per side, each walked so that the square lies on the same
/// hand: north with x growing, east with z growing, south with x falling, west with z falling. A corner is in
/// two lists.
fn border(k: usize) -> [Vec<(usize, usize)>; 4] {
    [(0..k).map(|c| (c, 0)).collect(), (0..k).map(|r| (k - 1, r)).collect(), (0..k).rev().map(|c| (c, k - 1)).collect(), (0..k).rev().map(|r| (0, r)).collect()]
}

/// The land's height at the two ends of the border's stretch from lattice point `a` to `b`: under slopes the two
/// samples' own, under blocks the flat top of the border column the stretch runs along.
fn border_tops(world: &World, g: &Grid, level: Level, a: (usize, usize), b: (usize, usize)) -> (f32, f32) {
    if level.blocks() {
        let top = g.height(world, a.0.min(b.0).min(g.m - 1), a.1.min(b.1).min(g.m - 1));
        (top, top)
    } else {
        (g.height(world, a.0, a.1), g.height(world, b.0, b.1))
    }
}

/// The walls that close the land and the sea round the square: along each side, from the higher of the land's
/// border and the water down to `base`. A mesh of its own, every vertex on `WALL_UV`, so the walls are one plain
/// colour and, drawn with an image, write depth as the land does (the water's Flat material only tests it). Each
/// stretch is a top and its foot at either end. Under slopes, a stretch whose ground crosses the water is cut
/// there, so the wall's top follows the ground down to the shore and the water from it.
pub fn walls(world: &World, level: Level) -> Mesh {
    let g = grid(world, level);
    let (y, base) = (sea(world), base(world));
    let mut mesh = Mesh::new();
    let mut stretch = |a: [f32; 3], b: [f32; 3]| {
        let first = mesh.vertices.len() as u32;
        for p in [a, b] {
            mesh.vertices.push(Vertex { pos: p, uv: WALL_UV });
            mesh.vertices.push(Vertex { pos: [p[0], base, p[2]], uv: WALL_UV });
        }
        // Each top, then its foot; wound to face out as the land faces up.
        mesh.indices.extend_from_slice(&[first, first + 2, first + 1, first + 2, first + 3, first + 1]);
    };
    for edge in border(lattice_side(&g, level)) {
        for pair in edge.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let (ta, tb) = border_tops(world, &g, level, a, b);
            let (pa, pb) = ([g.at(a.0), ta.max(y), g.at(a.1)], [g.at(b.0), tb.max(y), g.at(b.1)]);
            if (ta < y) != (tb < y) {
                let t = (y - ta) / (tb - ta);
                let shore = [pa[0] + (pb[0] - pa[0]) * t, y, pa[2] + (pb[2] - pa[2]) * t];
                stretch(pa, shore);
                stretch(shore, pb);
            } else {
                stretch(pa, pb);
            }
        }
    }
    mesh
}

/// The lattice's points a side, without its heights.
fn lattice_side(g: &Grid, level: Level) -> usize {
    if level.blocks() { g.m + 1 } else { g.m }
}

/// The walls' pixel for `choice`, over a stage whose ground is `ground`.
pub fn wall_rgba(choice: WallColour, ground: Color) -> [u8; 4] {
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    match choice {
        WallColour::Ground => [byte(ground.r), byte(ground.g), byte(ground.b), 255],
        WallColour::Earth => {
            let [r, g, b] = steep_colour();
            [r, g, b, 255]
        }
    }
}

/// The walls' image, in `EDGE`'s colour: uploaded with the drape, which is painted again when the theme changes.
/// Nothing to do when the edge shows the underside.
pub fn upload_wall(gfx: &mut dyn Gfx, ground: Color) {
    if let Edge::Wall(colour) = EDGE {
        gfx.upload_image(WALL_IMAGE, 1, 1, &wall_rgba(colour, ground));
    }
}

/// Where point `i` of the lattice samples `UNDER_IMAGE`: the middle of its own texel.
fn under_uv(i: usize, k: usize) -> [f32; 2] {
    [((i % k) as f32 + 0.5) / k as f32, ((i / k) as f32 + 0.5) / k as f32]
}

/// The land's underside, seen only from past the edge at a low angle: one sloped surface hung `UNDER_GAP` under
/// the lattice's points, facing down, and round the border a rim from the land's own edge down to it, facing out,
/// so the edge is one clean line. Each vertex samples its own shade in `underside_image`. Blocks are not drawn
/// again from below: that doubled the land's triangles for a part hardly ever seen.
pub fn underside(world: &World, level: Level) -> Mesh {
    let g = grid(world, level);
    let l = lattice(world, level);
    let k = l.k;
    let at = |i: usize, y: f32| Vertex { pos: [g.at(i % k), y, g.at(i / k)], uv: under_uv(i, k) };
    let mut mesh = Mesh::new();
    mesh.vertices.reserve(k * k + 8 * (k - 1));
    mesh.vertices.extend((0..k * k).map(|i| at(i, l.y[i] - UNDER_GAP)));
    mesh.indices.reserve((k - 1) * (k - 1) * 6 + 4 * (k - 1) * 6);
    for row in 0..k - 1 {
        for col in 0..k - 1 {
            let a = (row * k + col) as u32;
            let (b, c, d) = (a + 1, a + k as u32, a + k as u32 + 1);
            // The slopes' [a, c, b, b, c, d] turned over.
            mesh.indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    for edge in border(k) {
        for pair in edge.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let (ta, tb) = border_tops(world, &g, level, a, b);
            let (low_a, low_b) = ((a.1 * k + a.0) as u32, (b.1 * k + b.0) as u32);
            let first = mesh.vertices.len() as u32;
            mesh.vertices.push(at(low_a as usize, ta));
            mesh.vertices.push(at(low_b as usize, tb));
            mesh.indices.extend_from_slice(&[first, first + 1, low_a, first + 1, low_b, low_a]);
        }
    }
    mesh
}

/// `UNDER_IMAGE`, `k` by `k` RGBA: a grey per point of the lattice, by how much the surface there, seen from
/// under, turns to `UNDER_LIGHT`. Its normal is worked out from the points on each side, or the one at the border.
pub fn underside_image(world: &World, level: Level) -> (u32, Vec<u8>) {
    let cell = grid(world, level).cell;
    let l = lattice(world, level);
    let k = l.k;
    let h = |col: usize, row: usize| l.y[row * k + col];
    let len = |v: [f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let light = UNDER_LIGHT.map(|c| c / len(UNDER_LIGHT));
    let mut rgba = Vec::with_capacity(k * k * 4);
    for row in 0..k {
        for col in 0..k {
            let (c0, c1) = (col.saturating_sub(1), (col + 1).min(k - 1));
            let (r0, r1) = (row.saturating_sub(1), (row + 1).min(k - 1));
            let dx = (h(c1, row) - h(c0, row)) / ((c1 - c0) as f32 * cell);
            let dz = (h(col, r1) - h(col, r0)) / ((r1 - r0) as f32 * cell);
            // The top's normal is (-dx, 1, -dz); the underside's points the other way.
            let down = [dx, -1.0, dz];
            let lit = ((down[0] * light[0] + down[1] * light[1] + down[2] * light[2]) / len(down)).max(0.0);
            let grey = (UNDER_GREYS.0 + (UNDER_GREYS.1 - UNDER_GREYS.0) * lit).round() as u8;
            rgba.extend_from_slice(&[grey, grey, grey, 255]);
        }
    }
    (k as u32, rgba)
}

/// The height of the drawn land under (x, z): the top of the column there under blocks, and under slopes the
/// height between the four nearest samples, as the triangles draw it closely enough; None outside the square.
pub fn ground_at(world: &World, level: Level, x: f32, z: f32) -> Option<f32> {
    let g = grid(world, level);
    if !(-g.half..=g.half).contains(&x) || !(-g.half..=g.half).contains(&z) {
        return None;
    }
    let (c, r) = ((x + g.half) / g.cell, (z + g.half) / g.cell);
    if level.blocks() {
        // The far edge belongs to the last column.
        return Some(g.height(world, (c as usize).min(g.m - 1), (r as usize).min(g.m - 1)));
    }
    // A map of one sample a side has no slope: its square is that point.
    let last = g.m.saturating_sub(2);
    let (c0, r0) = ((c as usize).min(last), (r as usize).min(last));
    let (c1, r1) = ((c0 + 1).min(g.m - 1), (r0 + 1).min(g.m - 1));
    let (fc, fr) = (c - c0 as f32, r - r0 as f32);
    let h = |col: usize, row: usize| g.height(world, col, row);
    let north = h(c0, r0) + (h(c1, r0) - h(c0, r0)) * fc;
    let south = h(c0, r1) + (h(c1, r1) - h(c0, r1)) * fc;
    Some(north + (south - north) * fr)
}

/// The camera as the page starts it (lines 1873-1880): far and high enough to hold the whole square, its near
/// edge included, at (0, 1.1w, w) in widths of the area unless `camera` gives the start (`?camera=`).
pub fn orbit(world: &World, camera: Option<Vec3>) -> Orbit {
    let w = world.span() as f32;
    let default = Vec3::new(0.0, 1.1 * w, w);
    let mut orbit = Orbit::new(Vec3::default(), camera.map(|c| c * w).unwrap_or(default));
    // From the default start, so that a custom start does not change how far the wheel can take the camera.
    orbit.max_distance = 2.0 * default.length();
    // The page's target sits at height 0, well under the land: closer than this the camera is lifted to clear
    // the ground and looks at one huge block, which says nothing.
    orbit.min_distance = world.step as f32 * 40.0;
    orbit
}

/// Moves the camera towards its goal and keeps it above the drawn ground; true while it is still moving.
pub fn settle(orbit: &mut Orbit, world: &World, level: Level, seconds: f32) -> bool {
    let moving = orbit.step(seconds);
    orbit.keep_above(&|x, z| ground_at(world, level, x, z).map_or(f32::NEG_INFINITY, |g| g + MARGIN));
    moving
}

/// Builds the meshes of `level` and sends them to the device, which holds them: again after a rebuild of the
/// device, a new world or another level. Each mesh is dropped once sent, so the fine blocks never hold two of
/// theirs at once. The sides' picture comes with the map's (`upload_sides`).
pub fn upload(gfx: &mut dyn Gfx, world: &World, level: Level) {
    let send = |gfx: &mut dyn Gfx, id: MeshId, mesh: Mesh| gfx.upload_mesh(id, &mesh.vertices, &mesh.indices);
    send(gfx, LAND_MESH, land(world, level));
    // Under slopes, the one empty triangle: the device lets go of the sides of the level before.
    send(gfx, SIDE_MESH, sides(world, level));
    send(gfx, WATER_MESH, water(world, level));
    match EDGE {
        Edge::Underside => {
            send(gfx, UNDER_MESH, underside(world, level));
            // The shades depend on the heights only, not on the theme.
            let (k, rgba) = underside_image(world, level);
            gfx.upload_image(UNDER_IMAGE, k, k, &rgba);
        }
        Edge::Wall(_) => send(gfx, WALL_MESH, walls(world, level)),
    }
}

/// Line 1903: the water is thinner over the height view, whose colours are the ground's own.
pub fn water_alpha(view: View) -> f32 {
    if view == View::Height { 0.25 } else { 0.6 }
}

/// Draws the land of `level` draped in `image` and the water over it, into `viewport` (logical pixels). The
/// aspect is the viewport's own, so the picture is right in a square and in a window.
pub fn draw(gfx: &mut dyn Gfx, viewport: Rect, orbit: &Orbit, level: Level, image: ImageId, water_alpha: f32) {
    if viewport.w <= 0.0 || viewport.h <= 0.0 {
        return;
    }
    let view_proj = orbit.view_proj(viewport.w / viewport.h);
    gfx.draw_mesh(viewport, LAND_MESH, &view_proj, Material::Image { image, nearest: true });
    if level.blocks() {
        gfx.draw_mesh(viewport, SIDE_MESH, &view_proj, Material::Image { image: SIDE_IMAGE, nearest: true });
    }
    match EDGE {
        // Smooth: each vertex's uv is the middle of its own texel, so the shades blend across the triangles.
        Edge::Underside => gfx.draw_mesh(viewport, UNDER_MESH, &view_proj, Material::Image { image: UNDER_IMAGE, nearest: false }),
        // `upload_wall` must have given the walls their pixel.
        Edge::Wall(_) => gfx.draw_mesh(viewport, WALL_MESH, &view_proj, Material::Image { image: WALL_IMAGE, nearest: true }),
    }
    // After the land and its edge: the water is tested against them but leaves no depth of its own.
    gfx.draw_mesh(viewport, WATER_MESH, &view_proj, Material::Flat { color: WATER_COLOUR.with_alpha(water_alpha) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use Level::{Blocks, Fine, Slopes};

    fn world(samples: usize) -> World {
        World {
            seed: String::new(),
            samples,
            step: 16.0,
            origin_x: 0.0,
            origin_z: 0.0,
            sea_level: 63.0,
            heights: (0..samples * samples).map(|i| (i % 100) as i16).collect(),
            biome_at: vec![0; samples * samples],
            biomes: Vec::new(),
            colours: Vec::new(),
            flat_at: vec![0; samples * samples],
            structures: None,
            section: None,
            biome_total: None,
            absent: Vec::new(),
            measured: Default::default(),
            made: None,
        }
    }

    /// 16 a side: the five western columns at 70, the rest at 40, under the sea.
    fn shore() -> World {
        let mut w = world(16);
        w.heights = (0..16 * 16).map(|i| if i % 16 < 5 { 70 } else { 40 }).collect();
        w
    }

    /// (v1 - v0) x (v2 - v0) of triangle `t`: the side it faces.
    fn normal(mesh: &Mesh, t: usize) -> [f32; 3] {
        let p = |k: usize| mesh.vertices[mesh.indices[3 * t + k] as usize].pos;
        let (a, b, c) = (p(0), p(1), p(2));
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
    }

    /// The axis triangle `t` faces, as a unit vector: it must face one of the six exactly, never a slope.
    fn facing(mesh: &Mesh, t: usize) -> [i32; 3] {
        let n = normal(mesh, t);
        let axes = n.iter().filter(|c| **c != 0.0).count();
        assert_eq!(axes, 1, "triangle {t} is sloped: {n:?}");
        n.map(|c| c.signum() as i32 * (c != 0.0) as i32)
    }

    fn triangles(mesh: &Mesh) -> std::ops::Range<usize> {
        0..mesh.indices.len() / 3
    }

    /// The direction `d` of `DIRS` as a facing.
    fn dir(d: usize) -> [i32; 3] {
        [DIRS[d].0 as i32, 0, DIRS[d].1 as i32]
    }

    /// Whether triangle `t` is upright and faces away from the square's middle, as a wall round it does.
    fn faces_out(mesh: &Mesh, t: usize) -> bool {
        let n = normal(mesh, t);
        let at = |k: usize| mesh.vertices[mesh.indices[3 * t + k] as usize].pos;
        let middle = [(at(0)[0] + at(1)[0] + at(2)[0]) / 3.0, (at(0)[2] + at(1)[2] + at(2)[2]) / 3.0];
        n[1] == 0.0 && n[0] * middle[0] + n[2] * middle[1] > 0.0
    }

    /// Twice the area of a mesh's triangles seen from above, positive when they face up as the land does.
    fn twice_area_up(mesh: &Mesh) -> f32 {
        triangles(mesh).map(|t| normal(mesh, t)[1]).sum()
    }

    fn span_x(mesh: &Mesh) -> (f32, f32) {
        let x = || mesh.vertices.iter().map(|v| v.pos[0]);
        (x().fold(f32::MAX, f32::min), x().fold(f32::MIN, f32::max))
    }

    #[test]
    fn the_levels_have_their_keys_and_the_middle_one_is_where_a_window_starts() {
        for level in Level::ALL {
            assert_eq!(Level::from_key(level.key()), Some(level));
            assert_eq!(Level::ALL[level.index()], level);
        }
        assert_eq!(Level::from_key("smooth"), None);
        assert_eq!(Level::default(), Blocks);
    }

    #[test]
    fn each_column_has_a_flat_top_at_its_height_in_its_colour() {
        let w = world(16);
        for level in [Blocks, Fine] {
            let land = land(&w, level);
            assert_eq!((land.vertices.len(), land.indices.len()), (16 * 16 * 4, 16 * 16 * 6));
            assert!(land.indices.iter().all(|&i| (i as usize) < land.vertices.len()));
            assert!(triangles(&land).all(|t| facing(&land, t) == [0, 1, 0]), "every top faces up");
            for (k, top) in land.vertices.chunks(4).enumerate() {
                let (col, row) = (k % 16, k / 16);
                assert!(top.iter().all(|v| v.pos[1] == w.heights[row * 16 + col] as f32), "column {k} is flat at its height");
                assert!(top.iter().all(|v| v.uv == [(col as f32 + 0.5) / 16.0, (row as f32 + 0.5) / 16.0]));
                // A square a step wide.
                let xs: Vec<f32> = top.iter().map(|v| v.pos[0]).collect();
                assert_eq!(xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min), 16.0);
            }
            // The square is the area itself, centred: 16 columns of 16 blocks.
            assert_eq!(span_x(&land), (-128.0, 128.0));
        }
    }

    #[test]
    fn the_slopes_have_a_vertex_per_sample_and_two_triangles_per_cell() {
        let w = world(256);
        let m = land(&w, Slopes);
        assert_eq!(m.vertices.len(), 256 * 256);
        assert_eq!(m.indices.len(), 255 * 255 * 6);
        assert!(m.indices.iter().all(|&i| (i as usize) < m.vertices.len()));
        // Centred on the origin, 255 steps across, each vertex at its sample's height in its sample's colour.
        assert_eq!(span_x(&m), (-255.0 * 16.0 / 2.0, 255.0 * 16.0 / 2.0));
        assert_eq!((m.vertices[1].pos[1], m.vertices[1].uv), (1.0, [1.5 / 256.0, 0.5 / 256.0]));
        // The land faces up.
        assert!(triangles(&m).all(|t| normal(&m, t)[1] > 0.0));
        // No side stands on a slope: only the empty triangle the renderer needs.
        assert_eq!(sides(&w, Slopes).indices, vec![0, 0, 0]);
    }

    #[test]
    fn sides_stand_upright_only_where_a_neighbour_is_lower() {
        // Flat land has no side at all, only the empty triangle the renderer needs.
        let mut flat = world(16);
        flat.heights = vec![70; 16 * 16];
        assert_eq!(sides(&flat, Blocks).indices, vec![0, 0, 0]);
        // One step, between columns 4 and 5, on every row: one side each, at x -48, from 70 down to 40, facing east.
        let w = shore();
        let s = sides(&w, Blocks);
        assert_eq!(s.indices.len(), 16 * 6);
        assert!(triangles(&s).all(|t| facing(&s, t) == dir(1)));
        assert!(s.vertices.iter().all(|v| v.pos[0] == -48.0 && (v.pos[1] == 70.0 || v.pos[1] == 40.0)));
        assert!(s.vertices.chunks(4).enumerate().all(|(row, v)| v.iter().all(|v| v.uv == side_uv(16, 4, row, 1))));
        // Uneven ground: one side per pair of neighbours that differ, facing the lower one, from one top to the other.
        let w = world(16);
        let s = sides(&w, Blocks);
        let h = |c: usize, r: usize| w.heights[r * 16 + c];
        let pairs = (0..16).flat_map(|r| (0..15).map(move |c| (c, r))).filter(|&(c, r)| h(c, r) != h(c + 1, r)).count()
            + (0..15).flat_map(|r| (0..16).map(move |c| (c, r))).filter(|&(c, r)| h(c, r) != h(c, r + 1)).count();
        assert_eq!(s.indices.len(), pairs * 6);
        assert!(s.indices.iter().all(|&i| (i as usize) < s.vertices.len()));
        assert!(triangles(&s).all(|t| facing(&s, t)[1] == 0), "every side is upright");
    }

    #[test]
    fn the_side_picture_is_each_columns_colour_shaded_by_direction() {
        let w = world(16);
        // A picture whose red is the sample's column and green its row.
        let drape: Vec<u8> = (0..16 * 16).flat_map(|i| [(i % 16) as u8 * 10, (i / 16) as u8 * 10, 200, 255]).collect();
        let (width, height, rgba) = side_image(&w, Blocks, &drape);
        assert_eq!((width, height, rgba.len()), (32, 16, 32 * 16 * 4));
        for d in 0..4 {
            let [u, v] = side_uv(16, 3, 7, d);
            let at = (((v * 16.0) as usize) * 32 + (u * 32.0) as usize) * 4;
            let shade = |c: f32| (c * SIDE_SHADE[d]).round() as u8;
            assert_eq!(&rgba[at..at + 4], &[shade(30.0), shade(70.0), shade(200.0), 255], "direction {d}");
        }
    }

    #[test]
    fn the_cameras_ground_is_the_surface_drawn_at_every_level() {
        let w = world(16);
        for level in [Blocks, Fine] {
            let g = grid(&w, level);
            for row in 0..16 {
                for col in 0..16 {
                    let [x0, z0, x1, z1] = g.footprint(col, row);
                    let want = Some(w.heights[row * 16 + col] as f32);
                    // The middle and near every corner of the footprint.
                    for (x, z) in [((x0 + x1) / 2.0, (z0 + z1) / 2.0), (x0 + 0.01, z0 + 0.01), (x1 - 0.01, z1 - 0.01)] {
                        assert_eq!(ground_at(&w, level, x, z), want, "column ({col}, {row})");
                    }
                }
            }
            // The far edges belong to the last columns; past them there is no ground.
            assert_eq!(ground_at(&w, level, 128.0, 128.0), Some(w.heights[16 * 16 - 1] as f32));
            assert_eq!(ground_at(&w, level, 128.5, 0.0), None);
            assert_eq!(ground_at(&w, level, 0.0, -128.5), None);
        }
        // Under slopes: each vertex's own height, the mean of two halfway along an edge, and nothing past the
        // last sample, where the blocks still have half a column.
        let land = land(&w, Slopes);
        for v in &land.vertices {
            assert_eq!(ground_at(&w, Slopes, v.pos[0], v.pos[2]), Some(v.pos[1]));
        }
        let (a, b) = (land.vertices[16 + 3].pos, land.vertices[16 + 4].pos);
        assert_eq!(ground_at(&w, Slopes, (a[0] + b[0]) / 2.0, a[2]), Some((a[1] + b[1]) / 2.0));
        assert_eq!(ground_at(&w, Slopes, 120.5, 0.0), None);
        assert_eq!(ground_at(&w, Slopes, 0.0, -120.5), None);
        // The same place has not the same ground under the two: a column's flat top, a slope's height.
        assert_ne!(ground_at(&w, Slopes, 4.0, 0.0), ground_at(&w, Blocks, 4.0, 0.0));
    }

    #[test]
    fn a_wide_map_is_thinned_under_slopes_and_blocks_and_whole_under_fine_blocks() {
        // 2048 samples a side: one sample in 4 kept, 512 a side, 64 blocks apart, as a real wide run gives.
        let w = world(2048);
        let span = w.span() as f32;
        let tops = land(&w, Blocks);
        assert_eq!(tops.vertices.len(), 512 * 512 * 4);
        assert_eq!(span_x(&tops), (-span / 2.0, span / 2.0));
        // Column (3, 5) stands on sample (12, 20), and the camera's ground there is its top.
        let [x0, z0, ..] = grid(&w, Blocks).footprint(3, 5);
        assert_eq!(ground_at(&w, Blocks, x0 + 1.0, z0 + 1.0), Some(w.heights[20 * 2048 + 12] as f32));
        assert_eq!(tops.vertices[(5 * 512 + 3) * 4].pos[1], w.heights[20 * 2048 + 12] as f32);
        // The slopes join the 512 samples kept: their square ends on the last one.
        let slopes = land(&w, Slopes);
        assert_eq!(slopes.vertices.len(), 512 * 512);
        assert_eq!(span_x(&slopes), (-511.0 * 64.0 / 2.0, 511.0 * 64.0 / 2.0));
        assert_eq!(slopes.vertices[5 * 512 + 3].pos[1], w.heights[20 * 2048 + 12] as f32);
        // Fine blocks: a column per sample, 16 blocks wide, so every square of the 2D map is a block. On a map
        // 1024 a side, to keep the test light: the blocks thin it, the fine blocks do not.
        let w = world(1024);
        let (coarse, fine) = (grid(&w, Blocks), grid(&w, Fine));
        assert_eq!((coarse.m, coarse.cell, fine.m, fine.cell), (512, 32.0, 1024, 16.0));
        let tops = land(&w, Fine);
        assert_eq!(tops.vertices.len(), 1024 * 1024 * 4);
        assert_eq!(span_x(&tops), (-8192.0, 8192.0));
        assert_eq!(tops.vertices[(5 * 1024 + 3) * 4].pos[1], w.heights[5 * 1024 + 3] as f32);
        assert_eq!(ground_at(&w, Fine, fine.at(3) + 1.0, fine.at(5) + 1.0), Some(w.heights[5 * 1024 + 3] as f32));
        assert_eq!(side_image(&w, Fine, &vec![0; 1024 * 1024 * 4]).0, 2048);
    }

    #[test]
    fn fine_blocks_stop_at_their_most_columns_a_side() {
        assert_eq!(grid(&world(FINE_MOST), Fine).m, FINE_MOST);
        // A map wider than the fine blocks hold is thinned for them too.
        let mut w = world(16);
        w.samples = 2 * FINE_MOST;
        assert_eq!((grid(&w, Fine).skip, grid(&w, Fine).m), (2, FINE_MOST));
    }

    #[test]
    fn water_is_flat_over_the_columns_under_the_sea_only() {
        let mut w = world(16);
        w.heights = vec![40; 16 * 16];
        let all = water(&w, Blocks);
        assert_eq!(all.indices.len(), 16 * 16 * 6);
        assert_eq!(all.vertices.len(), 17 * 17);
        assert!(all.vertices.iter().all(|v| v.pos[1] == 62.5));
        w.heights = vec![70; 16 * 16];
        assert_eq!(water(&w, Blocks).indices, vec![0, 0, 0]);
        // The shore: water over the eleven eastern columns, from x -48 to the edge, none over the land, and no
        // vertex but the corners of those columns.
        let s = water(&shore(), Blocks);
        assert_eq!((s.indices.len(), s.vertices.len()), (11 * 16 * 6, 12 * 17));
        assert!(s.indices.iter().all(|&i| (i as usize) < s.vertices.len()));
        assert!(s.vertices.iter().all(|v| (-48.0..=128.0).contains(&v.pos[0]) && (-128.0..=128.0).contains(&v.pos[2])));
        assert!(triangles(&s).all(|t| facing(&s, t) == [0, 1, 0]));
    }

    #[test]
    fn water_over_slopes_is_cut_where_the_ground_meets_the_sea() {
        let mut w = world(16);
        let cell = 16.0f32;
        // All under: the land's two triangles per cell, at the sheet, on the grid's own vertices.
        w.heights = vec![40; 16 * 16];
        let under = water(&w, Slopes);
        assert_eq!(under.indices.len(), 15 * 15 * 6);
        assert!(under.vertices.iter().all(|v| v.pos[1] == 62.5));
        assert!((twice_area_up(&under) - 2.0 * (15.0 * cell).powi(2)).abs() < 1.0);
        // All above: no water.
        w.heights = vec![70; 16 * 16];
        assert_eq!(twice_area_up(&water(&w, Slopes)), 0.0);
        // The shore is the line a quarter of the way from column 4 to column 5, x -52, so the sea is 172 blocks
        // wide and 240 long.
        let shore = water(&shore(), Slopes);
        assert!(shore.indices.iter().all(|&i| (i as usize) < shore.vertices.len()));
        let used: Vec<[f32; 3]> = shore.indices.iter().map(|&i| shore.vertices[i as usize].pos).collect();
        assert!(used.iter().all(|p| p[1] == 62.5 && p[0] >= -52.0 - 1e-3), "no water over the land");
        assert!((twice_area_up(&shore) - 2.0 * 172.0 * 240.0).abs() < 1.0, "{}", twice_area_up(&shore));
        assert!(triangles(&shore).all(|t| normal(&shore, t)[1] > 0.0));
    }

    #[test]
    fn the_underside_hangs_under_every_top_and_side_with_a_rim_round_the_border() {
        for w in [shore(), world(16)] {
            let under = underside(&w, Blocks);
            let k = 17;
            // A point per corner of the columns, two triangles per column, and a rim along each border column.
            assert_eq!(under.vertices.len(), k * k + 4 * 16 * 2);
            assert_eq!(under.indices.len(), (16 * 16 + 4 * 16) * 6);
            assert!(under.indices.iter().all(|&i| (i as usize) < under.vertices.len()));
            // Each corner is `UNDER_GAP` under the lowest of the tops that meet there, on its own shade's texel.
            for (i, v) in under.vertices[..k * k].iter().enumerate() {
                let (c, r) = (i % k, i / k);
                let lowest = [(c.saturating_sub(1), r.saturating_sub(1)), (c.min(15), r.saturating_sub(1)), (c.saturating_sub(1), r.min(15)), (c.min(15), r.min(15))]
                    .iter()
                    .map(|&(col, row)| w.heights[row * 16 + col] as f32)
                    .fold(f32::MAX, f32::min);
                assert_eq!((v.pos, v.uv), ([c as f32 * 16.0 - 128.0, lowest - UNDER_GAP, r as f32 * 16.0 - 128.0], under_uv(i, k)));
            }
            // The surface faces down; the rim is upright, faces out, and starts at its border column's top.
            let surface = 16 * 16 * 2;
            assert!((0..surface).all(|t| normal(&under, t)[1] < 0.0));
            for t in surface..under.indices.len() / 3 {
                let f = facing(&under, t);
                let p = under.vertices[under.indices[3 * t] as usize].pos;
                assert!(f[1] == 0 && (p[0] * f[0] as f32 == 128.0 || p[2] * f[2] as f32 == 128.0), "rim triangle {t} faces out");
            }
            for pair in under.vertices[k * k..].chunks(2) {
                let x = ((pair[0].pos[0] + pair[1].pos[0]) / 2.0).clamp(-127.0, 127.0);
                let z = ((pair[0].pos[2] + pair[1].pos[2]) / 2.0).clamp(-127.0, 127.0);
                let top = ground_at(&w, Blocks, x, z).unwrap();
                assert_eq!((pair[0].pos[1], pair[1].pos[1]), (top, top));
            }
        }
        // The camera's ground is still the top.
        assert_eq!(ground_at(&shore(), Blocks, -100.0, 0.0), Some(70.0));
    }

    #[test]
    fn the_slopes_underside_is_the_land_turned_over_just_under_it_with_a_closed_rim() {
        let w = world(16);
        let m = 16;
        let (land, under) = (land(&w, Slopes), underside(&w, Slopes));
        // The grid again, then the rim's two tops per stretch of the four sides.
        assert_eq!(under.vertices.len(), m * m + 4 * (m - 1) * 2);
        assert_eq!(under.indices.len(), (m - 1) * (m - 1) * 6 + 4 * (m - 1) * 6);
        assert!(under.indices.iter().all(|&i| (i as usize) < under.vertices.len()));
        // Every vertex of the grid has its underside straight under it, the gap lower, on its own shade's texel.
        for (i, (top, low)) in land.vertices.iter().zip(&under.vertices[..m * m]).enumerate() {
            assert_eq!(low.pos, [top.pos[0], top.pos[1] - UNDER_GAP, top.pos[2]]);
            assert_eq!(low.uv, under_uv(i, m));
        }
        // The underside faces down; the rim joins the land's border to it and faces out.
        let surface = (m - 1) * (m - 1) * 2;
        assert!((0..surface).all(|t| normal(&under, t)[1] < 0.0));
        assert!((surface..under.indices.len() / 3).all(|t| faces_out(&under, t)));
        for v in &under.vertices[m * m..] {
            assert!(v.pos[0].abs() == 120.0 || v.pos[2].abs() == 120.0);
            assert!(land.vertices.iter().any(|l| l.pos == v.pos), "a rim's top is a vertex of the land's border");
        }
    }

    #[test]
    fn the_underside_is_grey_and_lit_from_below() {
        for (level, k) in [(Blocks, 17), (Slopes, 16)] {
            let mut w = world(16);
            w.heights = vec![70; 16 * 16];
            let (side, flat) = underside_image(&w, level);
            assert_eq!((side as usize, flat.len()), (k, k * k * 4));
            // Grey, opaque, and one shade under flat ground.
            assert!(flat.chunks(4).all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 255 && p[0] == flat[0]));
            // Ground falling to the east: seen from under, its face turns down and west, towards the light (x < 0),
            // and is lighter than flat ground; rising to the east it turns away and is darker.
            w.heights = (0..16 * 16).map(|i| (i % 16) as i16 * 8).collect();
            let rising = underside_image(&w, level).1;
            w.heights = (0..16 * 16).map(|i| (15 - i % 16) as i16 * 8).collect();
            let falling = underside_image(&w, level).1;
            let middle = (8 * k + 8) * 4;
            assert!(falling[middle] > flat[middle] && flat[middle] > rising[middle], "{} {} {}", falling[middle], flat[middle], rising[middle]);
            assert!(rising.iter().chain(&falling).all(|&g| g >= UNDER_GREYS.0 as u8), "never darker than the darkest grey");
        }
    }

    /// The checks every wall mesh passes: one plain colour, a top and its foot at each end of a stretch, the tops
    /// on the border at the higher of the drawn ground and the water, the feet at the base, every triangle facing
    /// out.
    fn check_walls(w: &World, level: Level) -> Mesh {
        let walls = walls(w, level);
        let half = grid(w, level).half;
        let (y, b) = (sea(w), base(w));
        assert!(b < w.height_range().0 as f32 && b < y, "base {b}");
        assert!(walls.indices.iter().all(|&i| (i as usize) < walls.vertices.len()));
        assert!(walls.vertices.iter().all(|v| v.uv == WALL_UV), "every wall vertex has the plain colour's uv");
        for stretch in walls.vertices.chunks(4) {
            for end in stretch.chunks(2) {
                let (top, foot) = (end[0].pos, end[1].pos);
                assert!(top[0].abs() == half || top[2].abs() == half, "{top:?} is on the border");
                assert_eq!(foot, [top[0], b, top[2]]);
            }
            // The middle of the stretch, just inside the square: its top is over the ground drawn there.
            let inside = |k: usize| ((stretch[0].pos[k] + stretch[2].pos[k]) / 2.0).clamp(-half + 0.01, half - 0.01);
            let ground = ground_at(w, level, inside(0), inside(2)).expect("the stretch is over the land");
            let top = (stretch[0].pos[1] + stretch[2].pos[1]) / 2.0;
            assert!((top - ground.max(y)).abs() < 0.5, "top {top} over ground {ground}");
        }
        assert!(triangles(&walls).all(|t| faces_out(&walls, t)));
        walls
    }

    #[test]
    fn the_walls_close_each_border_column_down_to_the_base() {
        for w in [shore(), world(16)] {
            let walls = check_walls(&w, Blocks);
            // A stretch per border column, flat at the higher of its top and the water.
            assert_eq!(walls.indices.len(), 4 * 16 * 6);
            assert!(walls.vertices.chunks(4).all(|s| s[0].pos[1] == s[2].pos[1]));
        }
    }

    #[test]
    fn the_slopes_walls_follow_the_ground_and_meet_the_water_at_the_shore() {
        check_walls(&world(16), Slopes);
        // Land over the sea all round: a stretch per segment of each side, with no shore point.
        let mut high = world(16);
        high.heights = vec![70; 16 * 16];
        assert_eq!(check_walls(&high, Slopes).indices.len(), 4 * 15 * 6);
        // All under the sea: every top is at the water.
        high.heights = vec![40; 16 * 16];
        assert!(check_walls(&high, Slopes).vertices.chunks(2).all(|p| p[0].pos[1] == 62.5));
        // From 70 to 40 the ground meets 62.5 a quarter of the way: 4 blocks east of column 4, at x -52. North
        // and south each get that shore point, so one more stretch than segments.
        let walls = check_walls(&shore(), Slopes);
        for z in [-120.0, 120.0] {
            assert!(walls.vertices.iter().any(|v| v.pos == [-52.0, 62.5, z]), "the shore's point at z {z}");
        }
        assert_eq!(walls.indices.len(), (4 * 15 + 2) * 6);
    }

    #[test]
    fn the_wall_colour_is_the_stage_ground_or_the_steep_earth() {
        assert_eq!(wall_rgba(WallColour::Ground, Color::hex(0x000000)), [0, 0, 0, 255]);
        assert_eq!(wall_rgba(WallColour::Ground, Color::hex(0xfafaf8)), [0xfa, 0xfa, 0xf8, 255]);
        let [r, g, b] = steep_colour();
        assert_eq!(wall_rgba(WallColour::Earth, Color::hex(0x000000)), [r, g, b, 255]);
        assert_eq!(wall_rgba(WallColour::Earth, Color::hex(0xfafaf8)), [r, g, b, 255]);
    }

    #[test]
    fn a_custom_start_keeps_the_zoom_range_of_the_default_one() {
        let w = world(256);
        let a = orbit(&w, None);
        let b = orbit(&w, Some(Vec3::new(0.3, 0.25, 0.6)));
        assert_eq!((a.max_distance, a.min_distance), (b.max_distance, b.min_distance));
        assert!(a != b);
    }
}
