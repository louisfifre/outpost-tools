//! The 3D view of the page (preview.html `refresh3d`, lines 1842-1907), shared by the preview's stage and the
//! relief demo: the land as blocks draped in the map's picture, the water, the orbit camera and the ground under
//! it. One unit is one block, heights included.
//!
//! The land is made of columns, one per sample kept: a square as wide as the samples are apart, with a flat top at
//! the sample's height, and an upright side down to each neighbour that is lower. No face is sloped, so the relief
//! reads as large blocks standing in space.

use mere_base::{Color, Gfx, ImageId, Material, MeshId, Orbit, Rect, Vec3, Vertex};

use crate::colour::steep_colour;
use crate::paint::View;
use crate::world::World;

/// The columns' tops, draped in the map's picture.
pub const LAND_MESH: MeshId = MeshId(15);
pub const WATER_MESH: MeshId = MeshId(16);
pub const WALL_MESH: MeshId = MeshId(19);
/// The walls' one pixel of colour.
pub const WALL_IMAGE: ImageId = ImageId(20);
/// The page's water (line 1901).
pub const WATER_COLOUR: Color = Color::hex(0x2f5c9e);
/// The columns' upright sides, in `SIDE_IMAGE`.
pub const SIDE_MESH: MeshId = MeshId(23);
/// The map's colour of each column, in each of the two shades a side can take (`side_image`).
pub const SIDE_IMAGE: ImageId = ImageId(24);
/// The shade of a side facing north, east, south and west: the game's own, so the steps read as its blocks do.
/// Two sides that face each other share a shade, so the picture holds two shades, not four.
pub const SIDE_SHADE: [f32; 4] = [0.8, 0.6, 0.8, 0.6];
const _: () = assert!(SIDE_SHADE[0] == SIDE_SHADE[2] && SIDE_SHADE[1] == SIDE_SHADE[3]);

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
/// Blocks between the land and its underside, at the least. The underside hangs from the lowest top round each of
/// its points, this much lower, so it is under the land everywhere and the depth test keeps the land in front from
/// above (the depth is 32-bit float and reversed, so half a block holds at any distance the camera reaches).
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
}

/// The columns: one every `skip` samples, `m` a side, each `cell` blocks wide. One per sample up to 512 a side; a
/// wider grid keeps one sample in two (or fewer), which the eye does not miss at that distance (the page does the
/// same). Together they make a square `m * cell` blocks wide, centred on the origin: the area itself when the
/// samples kept divide it evenly.
fn grid(world: &World) -> (usize, usize, f32) {
    let n = world.samples;
    let skip = n.div_ceil(512);
    let m = (n - 1) / skip + 1;
    (skip, m, (skip as f64 * world.step) as f32)
}

/// Half the square's width.
fn half(world: &World) -> f32 {
    let (_, m, cell) = grid(world);
    m as f32 * cell / 2.0
}

/// The height of column (col, row): its sample's.
fn height(world: &World, col: usize, row: usize) -> f32 {
    let (skip, _, _) = grid(world);
    world.heights[row * skip * world.samples + col * skip] as f32
}

/// The neighbour of column (col, row) towards `dirs[d]`, if the square has one.
fn neighbour(m: usize, col: usize, row: usize, d: usize) -> Option<(usize, usize)> {
    let (dc, dr) = DIRS[d];
    let (c, r) = (col as isize + dc, row as isize + dr);
    (c >= 0 && r >= 0 && (c as usize) < m && (r as usize) < m).then_some((c as usize, r as usize))
}

/// Column (col, row)'s footprint: [west x, north z, east x, south z].
fn footprint(world: &World, col: usize, row: usize) -> [f32; 4] {
    let (_, _, cell) = grid(world);
    let h = half(world);
    let (x, z) = (col as f32 * cell - h, row as f32 * cell - h);
    [x, z, x + cell, z + cell]
}

/// A flat square over (col, row)'s footprint at `y`: corners (west, north), (east, north), (west, south), (east,
/// south). `IN` faces it up, `OUT` down.
fn square(world: &World, col: usize, row: usize, y: f32) -> [[f32; 3]; 4] {
    let [x0, z0, x1, z1] = footprint(world, col, row);
    [[x0, y, z0], [x1, y, z0], [x0, y, z1], [x1, y, z1]]
}

/// The upright face of column (col, row) towards `dirs[d]`, `inset` blocks inside its footprint's edge, from `top`
/// down to `low`. `OUT` faces it towards `d`, `IN` back into the column.
fn face(world: &World, col: usize, row: usize, d: usize, inset: f32, top: f32, low: f32) -> [[f32; 3]; 4] {
    let [x0, z0, x1, z1] = footprint(world, col, row);
    let (dx, dz) = (DIRS[d].0 as f32, DIRS[d].1 as f32);
    // The plane, and the two ends of the face, walked so that the column lies on the same hand for every side.
    let (a, b) = match d {
        0 => ([x0, z0 + inset], [x1, z0 + inset]),
        1 => ([x1 - inset, z0], [x1 - inset, z1]),
        2 => ([x1, z1 - inset], [x0, z1 - inset]),
        _ => ([x0 + inset, z1], [x0 + inset, z0]),
    };
    debug_assert!(dx.abs() + dz.abs() == 1.0);
    [[a[0], top, a[1]], [b[0], top, b[1]], [a[0], low, a[1]], [b[0], low, b[1]]]
}

/// Where column (col, row) samples the map's picture: the middle of its sample's texel, the same at its four
/// corners, so its top is that sample's colour.
fn drape_uv(world: &World, col: usize, row: usize) -> [f32; 2] {
    let (skip, _, _) = grid(world);
    let n = world.samples as f32;
    [((col * skip) as f32 + 0.5) / n, ((row * skip) as f32 + 0.5) / n]
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

/// The columns' flat tops, each in its sample's colour.
pub fn land(world: &World) -> Mesh {
    let (_, m, _) = grid(world);
    let mut mesh = Mesh::new();
    mesh.vertices.reserve(4 * m * m);
    mesh.indices.reserve(6 * m * m);
    for row in 0..m {
        for col in 0..m {
            mesh.quad(square(world, col, row, height(world, col, row)), drape_uv(world, col, row), IN);
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
/// facing the neighbour, in the column's colour shaded for that direction. Equal neighbours have none.
pub fn sides(world: &World) -> Mesh {
    let (_, m, _) = grid(world);
    let mut mesh = Mesh::new();
    for row in 0..m {
        for col in 0..m {
            let top = height(world, col, row);
            for d in 0..4 {
                let Some((c, r)) = neighbour(m, col, row, d) else { continue };
                let low = height(world, c, r);
                if low < top {
                    mesh.quad(face(world, col, row, d, 0.0, top, low), side_uv(m, col, row, d), OUT);
                }
            }
        }
    }
    // mere-ui refuses an empty buffer, and would keep the last world's mesh: flat land gets one empty triangle.
    if mesh.indices.is_empty() {
        mesh.vertices.push(Vertex::default());
        mesh.indices.extend_from_slice(&[0, 0, 0]);
    }
    mesh
}

/// `SIDE_IMAGE`, 2m wide and m high, RGBA, from the map's picture `drape` (samples by samples, RGBA): for each of
/// the two shades a half holding every column's colour times it.
pub fn side_image(world: &World, drape: &[u8]) -> (u32, u32, Vec<u8>) {
    let (skip, m, _) = grid(world);
    let n = world.samples;
    let w = 2 * m;
    let mut rgba = vec![0u8; w * m * 4];
    for (half, shade) in SIDE_SHADE[..2].iter().enumerate() {
        // A byte times a shade, worked out once for the 256 bytes.
        let shaded: [u8; 256] = std::array::from_fn(|c| (c as f32 * shade).round() as u8);
        for row in 0..m {
            for col in 0..m {
                let from = (row * skip * n + col * skip) * 4;
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

/// Uploads the map's picture for the 3D view's sides, after the picture itself: it follows the view and the theme
/// as the picture does.
pub fn upload_sides(gfx: &mut dyn Gfx, world: &World, drape: &[u8]) {
    let (w, h, rgba) = side_image(world, drape);
    gfx.upload_image(SIDE_IMAGE, w, h, &rgba);
}

/// The water at `sea`, flat over each column whose ground is under it, so it ends at the shores and at the
/// square's edge. A sheet over the whole square would lie under the land wherever the land is higher, and from a
/// low angle past the edge it showed as a flat blue slab under the land.
pub fn water(world: &World) -> Mesh {
    let (_, m, cell) = grid(world);
    let y = sea(world);
    let h = half(world);
    // The columns' corners, at the water's height, shared by the squares of water side by side: a corner gets its
    // vertex when a first square needs it, so land without water carries none.
    let corners = m + 1;
    let mut vertex_of = vec![u32::MAX; corners * corners];
    let mut mesh = Mesh::new();
    for row in 0..m {
        for col in 0..m {
            if height(world, col, row) < y {
                let a = row * corners + col;
                let corner = [a, a + 1, a + corners, a + corners + 1].map(|i| {
                    if vertex_of[i] == u32::MAX {
                        vertex_of[i] = mesh.vertices.len() as u32;
                        mesh.vertices.push(Vertex { pos: [(i % corners) as f32 * cell - h, y, (i / corners) as f32 * cell - h], uv: [0.0, 0.0] });
                    }
                    vertex_of[i]
                });
                mesh.indices.extend(IN.iter().map(|&k| corner[k as usize]));
            }
        }
    }
    // mere-ui refuses an empty buffer, and would keep the last world's water.
    if mesh.indices.is_empty() {
        mesh.vertices.push(Vertex::default());
        mesh.indices.extend_from_slice(&[0, 0, 0]);
    }
    mesh
}

/// The walls that close the land and the sea round the square: under each border column, from the higher of its
/// top and the water down to `base`. A mesh of its own, every vertex on `WALL_UV`, so the walls are one plain
/// colour and, drawn with an image, write depth as the land does (the water's Flat material only tests it).
pub fn walls(world: &World) -> Mesh {
    let (_, m, _) = grid(world);
    let (y, base) = (sea(world), base(world));
    let mut mesh = Mesh::new();
    for row in 0..m {
        for col in 0..m {
            for d in 0..4 {
                if neighbour(m, col, row, d).is_none() {
                    mesh.quad(face(world, col, row, d, 0.0, height(world, col, row).max(y), base), WALL_UV, OUT);
                }
            }
        }
    }
    mesh
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

/// The points the underside hangs from, `k` a side and `cell` blocks apart over the square, with the height of
/// the land at each. The surface drawn between them, `UNDER_GAP` lower, lies under the land everywhere.
struct Lattice {
    k: usize,
    cell: f32,
    half: f32,
    y: Vec<f32>,
}

/// The corners of the columns, each at the lowest of the tops that meet there: between four such corners the
/// surface is under the column's top, and along an edge under both columns' tops, so under the side between them.
fn lattice(world: &World) -> Lattice {
    let (_, m, cell) = grid(world);
    let k = m + 1;
    let mut y = vec![f32::MAX; k * k];
    for row in 0..m {
        for col in 0..m {
            let h = height(world, col, row);
            for i in [row * k + col, row * k + col + 1, (row + 1) * k + col, (row + 1) * k + col + 1] {
                y[i] = y[i].min(h);
            }
        }
    }
    Lattice { k, cell, half: half(world), y }
}

/// The lattice's (col, row) round its border, one list per side, each walked so that the square lies on the same
/// hand: north with x growing, east with z growing, south with x falling, west with z falling. A corner is in
/// two lists.
fn border(k: usize) -> [Vec<(usize, usize)>; 4] {
    [(0..k).map(|c| (c, 0)).collect(), (0..k).map(|r| (k - 1, r)).collect(), (0..k).rev().map(|c| (c, k - 1)).collect(), (0..k).rev().map(|r| (0, r)).collect()]
}

/// Where point `i` of the lattice samples `UNDER_IMAGE`: the middle of its own texel.
fn under_uv(i: usize, k: usize) -> [f32; 2] {
    [((i % k) as f32 + 0.5) / k as f32, ((i / k) as f32 + 0.5) / k as f32]
}

/// The land's underside, seen only from past the edge at a low angle: one sloped surface hung `UNDER_GAP` under
/// the lattice's points, facing down, and round the border a rim from each border column's top down to it, facing
/// out, so the edge is one clean line. Each vertex samples its own shade in `underside_image`. The blocks are not
/// drawn again from below: that doubled the land's triangles for a part hardly ever seen.
pub fn underside(world: &World) -> Mesh {
    let (_, m, _) = grid(world);
    let l = lattice(world);
    let k = l.k;
    let at = |i: usize, y: f32| Vertex { pos: [(i % k) as f32 * l.cell - l.half, y, (i / k) as f32 * l.cell - l.half], uv: under_uv(i, k) };
    let mut mesh = Mesh::new();
    mesh.vertices.reserve(k * k + 8 * m);
    mesh.vertices.extend((0..k * k).map(|i| at(i, l.y[i] - UNDER_GAP)));
    mesh.indices.reserve((k - 1) * (k - 1) * 6 + 4 * m * 6);
    for row in 0..k - 1 {
        for col in 0..k - 1 {
            let a = (row * k + col) as u32;
            let (b, c, d) = (a + 1, a + k as u32, a + k as u32 + 1);
            mesh.indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    for edge in border(k) {
        for pair in edge.windows(2) {
            let ((ca, ra), (cb, rb)) = (pair[0], pair[1]);
            // The border column this stretch of the rim runs along, and its flat top.
            let top = height(world, ca.min(cb).min(m - 1), ra.min(rb).min(m - 1));
            let (low_a, low_b) = ((ra * k + ca) as u32, (rb * k + cb) as u32);
            let first = mesh.vertices.len() as u32;
            mesh.vertices.push(at(low_a as usize, top));
            mesh.vertices.push(at(low_b as usize, top));
            mesh.indices.extend_from_slice(&[first, first + 1, low_a, first + 1, low_b, low_a]);
        }
    }
    mesh
}

/// `UNDER_IMAGE`, `k` by `k` RGBA: a grey per point of the lattice, by how much the surface there, seen from
/// under, turns to `UNDER_LIGHT`. Its normal is worked out from the points on each side, or the one at the border.
pub fn underside_image(world: &World) -> (u32, Vec<u8>) {
    let l = lattice(world);
    let k = l.k;
    let h = |col: usize, row: usize| l.y[row * k + col];
    let len = |v: [f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let light = UNDER_LIGHT.map(|c| c / len(UNDER_LIGHT));
    let mut rgba = Vec::with_capacity(k * k * 4);
    for row in 0..k {
        for col in 0..k {
            let (c0, c1) = (col.saturating_sub(1), (col + 1).min(k - 1));
            let (r0, r1) = (row.saturating_sub(1), (row + 1).min(k - 1));
            let dx = (h(c1, row) - h(c0, row)) / ((c1 - c0) as f32 * l.cell);
            let dz = (h(col, r1) - h(col, r0)) / ((r1 - r0) as f32 * l.cell);
            // The top's normal is (-dx, 1, -dz); the underside's points the other way.
            let down = [dx, -1.0, dz];
            let lit = ((down[0] * light[0] + down[1] * light[1] + down[2] * light[2]) / len(down)).max(0.0);
            let grey = (UNDER_GREYS.0 + (UNDER_GREYS.1 - UNDER_GREYS.0) * lit).round() as u8;
            rgba.extend_from_slice(&[grey, grey, grey, 255]);
        }
    }
    (k as u32, rgba)
}

/// The ground's height under (x, z): the top of the column there; None outside the square.
pub fn ground_at(world: &World, x: f32, z: f32) -> Option<f32> {
    let (_, m, cell) = grid(world);
    let h = half(world);
    if !(-h..=h).contains(&x) || !(-h..=h).contains(&z) {
        return None;
    }
    // The far edge belongs to the last column.
    let at = |v: f32| (((v + h) / cell) as usize).min(m - 1);
    Some(height(world, at(x), at(z)))
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

/// Moves the camera towards its goal and keeps it above the ground; true while it is still moving.
pub fn settle(orbit: &mut Orbit, world: &World, seconds: f32) -> bool {
    let moving = orbit.step(seconds);
    orbit.keep_above(&|x, z| ground_at(world, x, z).map_or(f32::NEG_INFINITY, |g| g + MARGIN));
    moving
}

/// The meshes live on the device, so they are uploaded again after a rebuild. The sides' picture comes with the
/// map's (`upload_sides`).
pub fn upload(gfx: &mut dyn Gfx, world: &World) {
    let land = land(world);
    gfx.upload_mesh(LAND_MESH, &land.vertices, &land.indices);
    let sides = sides(world);
    gfx.upload_mesh(SIDE_MESH, &sides.vertices, &sides.indices);
    let water = water(world);
    gfx.upload_mesh(WATER_MESH, &water.vertices, &water.indices);
    match EDGE {
        Edge::Underside => {
            let under = underside(world);
            gfx.upload_mesh(UNDER_MESH, &under.vertices, &under.indices);
            // The shades depend on the heights only, not on the theme.
            let (k, rgba) = underside_image(world);
            gfx.upload_image(UNDER_IMAGE, k, k, &rgba);
        }
        Edge::Wall(_) => {
            let walls = walls(world);
            gfx.upload_mesh(WALL_MESH, &walls.vertices, &walls.indices);
        }
    }
}

/// Line 1903: the water is thinner over the height view, whose colours are the ground's own.
pub fn water_alpha(view: View) -> f32 {
    if view == View::Height { 0.25 } else { 0.6 }
}

/// Draws the land draped in `image` and the water over it, into `viewport` (logical pixels). The aspect is the
/// viewport's own, so the picture is right in a square and in a window.
pub fn draw(gfx: &mut dyn Gfx, viewport: Rect, orbit: &Orbit, image: ImageId, water_alpha: f32) {
    if viewport.w <= 0.0 || viewport.h <= 0.0 {
        return;
    }
    let view_proj = orbit.view_proj(viewport.w / viewport.h);
    gfx.draw_mesh(viewport, LAND_MESH, &view_proj, Material::Image { image, nearest: true });
    gfx.draw_mesh(viewport, SIDE_MESH, &view_proj, Material::Image { image: SIDE_IMAGE, nearest: true });
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

    #[test]
    fn each_column_has_a_flat_top_at_its_height_in_its_colour() {
        let w = world(16);
        let land = land(&w);
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
        let x: Vec<f32> = land.vertices.iter().map(|v| v.pos[0]).collect();
        assert_eq!((x.iter().cloned().fold(f32::MAX, f32::min), x.iter().cloned().fold(f32::MIN, f32::max)), (-128.0, 128.0));
    }

    #[test]
    fn sides_stand_upright_only_where_a_neighbour_is_lower() {
        // Flat land has no side at all, only the empty triangle the renderer needs.
        let mut flat = world(16);
        flat.heights = vec![70; 16 * 16];
        assert_eq!(sides(&flat).indices, vec![0, 0, 0]);
        // One step, between columns 4 and 5, on every row: one side each, at x -48, from 70 down to 40, facing east.
        let w = shore();
        let s = sides(&w);
        assert_eq!(s.indices.len(), 16 * 6);
        assert!(triangles(&s).all(|t| facing(&s, t) == dir(1)));
        assert!(s.vertices.iter().all(|v| v.pos[0] == -48.0 && (v.pos[1] == 70.0 || v.pos[1] == 40.0)));
        assert!(s.vertices.chunks(4).enumerate().all(|(row, v)| v.iter().all(|v| v.uv == side_uv(16, 4, row, 1))));
        // Uneven ground: one side per pair of neighbours that differ, facing the lower one, from one top to the other.
        let w = world(16);
        let s = sides(&w);
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
        let (width, height, rgba) = side_image(&w, &drape);
        assert_eq!((width, height, rgba.len()), (32, 16, 32 * 16 * 4));
        for d in 0..4 {
            let [u, v] = side_uv(16, 3, 7, d);
            let at = (((v * 16.0) as usize) * 32 + (u * 32.0) as usize) * 4;
            let shade = |c: f32| (c * SIDE_SHADE[d]).round() as u8;
            assert_eq!(&rgba[at..at + 4], &[shade(30.0), shade(70.0), shade(200.0), 255], "direction {d}");
        }
    }

    #[test]
    fn the_cameras_ground_is_the_flat_top_of_the_column_under_it() {
        let w = world(16);
        for row in 0..16 {
            for col in 0..16 {
                let [x0, z0, x1, z1] = footprint(&w, col, row);
                let want = Some(w.heights[row * 16 + col] as f32);
                // The middle and near every corner of the footprint.
                for (x, z) in [((x0 + x1) / 2.0, (z0 + z1) / 2.0), (x0 + 0.01, z0 + 0.01), (x1 - 0.01, z1 - 0.01)] {
                    assert_eq!(ground_at(&w, x, z), want, "column ({col}, {row})");
                }
            }
        }
        // The far edges belong to the last columns; past them there is no ground.
        assert_eq!(ground_at(&w, 128.0, 128.0), Some(w.heights[16 * 16 - 1] as f32));
        assert_eq!(ground_at(&w, 128.5, 0.0), None);
        assert_eq!(ground_at(&w, 0.0, -128.5), None);
    }

    #[test]
    fn a_thinned_grid_keeps_one_column_per_sample_kept_over_the_whole_area() {
        // 2048 samples a side: one column every 4 samples, 512 a side, 64 blocks wide, as a real wide run gives.
        let w = world(2048);
        let land = land(&w);
        assert_eq!(land.vertices.len(), 512 * 512 * 4);
        let x: Vec<f32> = land.vertices.iter().map(|v| v.pos[0]).collect();
        let span = w.span() as f32;
        assert_eq!((x.iter().cloned().fold(f32::MAX, f32::min), x.iter().cloned().fold(f32::MIN, f32::max)), (-span / 2.0, span / 2.0));
        // Column (3, 5) stands on sample (12, 20), and the camera's ground there is its top.
        let [x0, z0, ..] = footprint(&w, 3, 5);
        assert_eq!(ground_at(&w, x0 + 1.0, z0 + 1.0), Some(w.heights[20 * 2048 + 12] as f32));
        assert_eq!(land.vertices[(5 * 512 + 3) * 4].pos[1], w.heights[20 * 2048 + 12] as f32);
    }

    #[test]
    fn water_is_flat_over_the_columns_under_the_sea_only() {
        let mut w = world(16);
        w.heights = vec![40; 16 * 16];
        let all = water(&w);
        assert_eq!(all.indices.len(), 16 * 16 * 6);
        assert_eq!(all.vertices.len(), 17 * 17);
        assert!(all.vertices.iter().all(|v| v.pos[1] == 62.5));
        w.heights = vec![70; 16 * 16];
        assert_eq!(water(&w).indices, vec![0, 0, 0]);
        // The shore: water over the eleven eastern columns, from x -48 to the edge, none over the land.
        let s = water(&shore());
        assert_eq!(s.indices.len(), 11 * 16 * 6);
        assert!(s.indices.iter().all(|&i| (i as usize) < s.vertices.len()));
        let used: Vec<[f32; 3]> = s.indices.iter().map(|&i| s.vertices[i as usize].pos).collect();
        assert!(used.iter().all(|p| (-48.0..=128.0).contains(&p[0]) && (-128.0..=128.0).contains(&p[2])));
        assert!(triangles(&s).all(|t| facing(&s, t) == [0, 1, 0]));
    }

    #[test]
    fn the_underside_hangs_under_every_top_and_side_with_a_rim_round_the_border() {
        for w in [shore(), world(16)] {
            let under = underside(&w);
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
                let top = ground_at(&w, x, z).unwrap();
                assert_eq!((pair[0].pos[1], pair[1].pos[1]), (top, top));
            }
        }
        // The camera's ground is still the top.
        assert_eq!(ground_at(&shore(), -100.0, 0.0), Some(70.0));
    }

    #[test]
    fn the_underside_is_grey_and_lit_from_below() {
        let mut w = world(16);
        w.heights = vec![70; 16 * 16];
        let (k, flat) = underside_image(&w);
        assert_eq!((k, flat.len()), (17, 17 * 17 * 4));
        // Grey, opaque, and one shade under flat ground.
        assert!(flat.chunks(4).all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 255 && p[0] == flat[0]));
        // Ground falling to the east: seen from under, its face turns down and west, towards the light (x < 0),
        // and is lighter than flat ground; rising to the east it turns away and is darker.
        w.heights = (0..16 * 16).map(|i| (i % 16) as i16 * 8).collect();
        let rising = underside_image(&w).1;
        w.heights = (0..16 * 16).map(|i| (15 - i % 16) as i16 * 8).collect();
        let falling = underside_image(&w).1;
        let middle = (8 * 17 + 8) * 4;
        assert!(falling[middle] > flat[middle] && flat[middle] > rising[middle], "{} {} {}", falling[middle], flat[middle], rising[middle]);
        assert!(rising.iter().chain(&falling).all(|&g| g >= UNDER_GREYS.0 as u8), "never darker than the darkest grey");
    }
    #[test]
    fn the_walls_close_each_border_column_down_to_the_base() {
        let w = shore();
        let walls = walls(&w);
        let b = base(&w);
        assert!(b < w.height_range().0 as f32 && b < sea(&w), "base {b}");
        assert_eq!(walls.indices.len(), 4 * 16 * 6);
        assert!(walls.vertices.iter().all(|v| v.uv == WALL_UV));
        for t in triangles(&walls) {
            let f = facing(&walls, t);
            let p = walls.vertices[walls.indices[3 * t] as usize].pos;
            assert!(f[1] == 0 && (p[0] * f[0] as f32 == 128.0 || p[2] * f[2] as f32 == 128.0), "triangle {t} faces out");
        }
        // Each reaches from the higher of its top and the water down to the base.
        for q in walls.vertices.chunks(4) {
            // The middle of the wall's top edge, just inside the square.
            let x = ((q[0].pos[0] + q[1].pos[0]) / 2.0).clamp(-127.0, 127.0);
            let z = ((q[0].pos[2] + q[1].pos[2]) / 2.0).clamp(-127.0, 127.0);
            let top = ground_at(&w, x, z).unwrap().max(62.5);
            assert_eq!((q[0].pos[1], q[2].pos[1]), (top, b));
        }
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
