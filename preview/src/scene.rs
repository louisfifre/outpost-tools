//! The 3D view of the page (preview.html `refresh3d`, lines 1842-1907), shared by the preview's stage and the
//! relief demo: the land as a mesh draped in the map's picture, the sheet of water, the orbit camera and the
//! ground under it. One unit is one block, heights included.

use mere_base::{Color, Gfx, ImageId, Material, MeshId, Orbit, Rect, Vec3, Vertex};

use crate::colour::steep_colour;
use crate::paint::View;
use crate::world::World;

pub const LAND_MESH: MeshId = MeshId(15);
pub const WATER_MESH: MeshId = MeshId(16);
pub const WALL_MESH: MeshId = MeshId(19);
/// The walls' one pixel of colour.
pub const WALL_IMAGE: ImageId = ImageId(20);
/// The page's water (line 1901).
pub const WATER_COLOUR: Color = Color::hex(0x2f5c9e);

/// What the walls round the land are painted.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum WallColour {
    /// The stage's own ground: the land reads as cut clean, and nothing shows under it.
    Ground,
    /// The Flat view's steep ground (`colour::steep_colour`), a dark neutral earth that holds on both themes.
    Earth,
}

/// The one place the walls' colour is chosen.
pub const WALL: WallColour = WallColour::Ground;

/// Where every vertex of the walls samples `WALL_IMAGE`: its one texel.
pub const WALL_UV: [f32; 2] = [0.5, 0.5];

// Blocks kept between the camera and the ground under it: the near plane is ten blocks, and the camera must not
// pass through the land it is turning round.
const MARGIN: f32 = 24.0;

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// The grid the land is drawn on, which the water and the camera's ground follow: one vertex every `skip`
/// samples, `m` vertices a side, `side` blocks from the first vertex to the last. One vertex per sample up to 512
/// a side; a wider grid gives one vertex every other sample (or fewer), which the eye does not miss at that
/// distance (the page does the same).
fn grid(world: &World) -> (usize, usize, f32) {
    let n = world.samples;
    let skip = n.div_ceil(512);
    let m = (n - 1) / skip + 1;
    let side = ((m - 1) * skip) as f32 * world.step as f32;
    (skip, m, side)
}

/// Vertex (col, row) of the grid, on sample (col * skip, row * skip): where it stands, and the middle of that
/// sample's texel, so each cell of the map lies under its own vertex.
fn point(world: &World, col: usize, row: usize) -> Vertex {
    let n = world.samples;
    let step = world.step as f32;
    let (skip, _, side) = grid(world);
    let (sc, sr) = (col * skip, row * skip);
    Vertex {
        pos: [(sc as f32) * step - side / 2.0, world.heights[sr * n + sc] as f32, (sr as f32) * step - side / 2.0],
        uv: [(sc as f32 + 0.5) / n as f32, (sr as f32 + 0.5) / n as f32],
    }
}

/// The grid's (col, row) round its border, one list per side, each walked so that the square lies on the same
/// hand: north with x growing, east with z growing, south with x falling, west with z falling. A corner is in
/// two lists.
fn border(m: usize) -> [Vec<(usize, usize)>; 4] {
    [
        (0..m).map(|c| (c, 0)).collect(),
        (0..m).map(|r| (m - 1, r)).collect(),
        (0..m).rev().map(|c| (c, m - 1)).collect(),
        (0..m).rev().map(|r| (0, r)).collect(),
    ]
}

/// The sheet of water's height: half a block under the sea level, so ground at that level is land and does not
/// lie in the sheet.
fn sea(world: &World) -> f32 {
    world.sea_level as f32 - 0.5
}

/// Where the walls round the land end: one step between two samples under both the lowest ground and the sheet
/// of water. Every wall then reaches under the ground and the sea beside it, and where the border is the lowest
/// ground the land still reads as a slab, not a sheet.
pub fn base(world: &World) -> f32 {
    let (lowest, _) = world.height_range();
    (lowest as f32).min(sea(world)) - world.step as f32
}

/// The land on `grid`'s vertices. The square is centred on the origin, so the camera can turn around it.
pub fn land(world: &World) -> Mesh {
    let (_, m, _) = grid(world);
    let mut vertices = Vec::with_capacity(m * m);
    for row in 0..m {
        for col in 0..m {
            vertices.push(point(world, col, row));
        }
    }
    let mut indices = Vec::with_capacity((m - 1) * (m - 1) * 6);
    for row in 0..m - 1 {
        for col in 0..m - 1 {
            let a = (row * m + col) as u32;
            let (b, c, d) = (a + 1, a + m as u32, a + m as u32 + 1);
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    Mesh { vertices, indices }
}

/// The water at `sea`, only where the ground is under it: over each triangle of the land, the part where the
/// triangle's ground lies below the sheet, cut along the straight line where it meets it. A sheet over the whole
/// square would lie under the land wherever the land is higher, and from a low angle past the edge it showed as a
/// flat blue slab under the land.
pub fn water(world: &World) -> Mesh {
    let (_, m, _) = grid(world);
    let y = sea(world);
    let ground: Vec<[f32; 3]> = (0..m * m).map(|i| point(world, i % m, i / m).pos).collect();
    let at = |p: [f32; 3]| Vertex { pos: [p[0], y, p[2]], uv: [0.0, 0.0] };
    // Every vertex of the grid, at the sheet's height, for the triangles wholly under it; the cut ones add their own.
    let mut vertices: Vec<Vertex> = ground.iter().map(|&p| at(p)).collect();
    let mut indices = Vec::new();
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
                    indices.extend(tri.map(|i| i as u32));
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
                    let first = vertices.len() as u32;
                    vertices.extend(part.iter().map(|&p| at(p)));
                    for k in 1..part.len() as u32 - 1 {
                        indices.extend_from_slice(&[first, first + k, first + k + 1]);
                    }
                }
            }
        }
    }
    // A world without sea still gets a mesh: mere-ui refuses an empty buffer, and would keep the last world's water.
    if indices.is_empty() {
        indices.extend_from_slice(&[0, 0, 0]);
    }
    Mesh { vertices, indices }
}

/// The walls that close the land and the sea round the square: under each side, from the higher of the border's
/// ground and the sheet of water down to `base`. Without them the land is an open sheet, and from a low angle one
/// sees under it. A mesh of its own, every vertex on `WALL_UV`, so the walls are one plain colour and, drawn with
/// an image, write depth as the land does (the water's Flat material only tests it).
pub fn walls(world: &World) -> Mesh {
    let (_, m, _) = grid(world);
    let (y, base) = (sea(world), base(world));
    let at = |p: [f32; 3]| Vertex { pos: p, uv: WALL_UV };
    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    for edge in border(m) {
        // The wall's top along this side: each border vertex at the higher of its ground and the sheet, and between
        // two vertices on either side of the sheet, the point where the ground's straight line meets it.
        let mut tops: Vec<[f32; 3]> = Vec::with_capacity(edge.len() + 2);
        for k in 0..edge.len() {
            let q = point(world, edge[k].0, edge[k].1).pos;
            if k > 0 {
                let p = point(world, edge[k - 1].0, edge[k - 1].1).pos;
                if (p[1] < y) != (q[1] < y) {
                    let t = (y - p[1]) / (q[1] - p[1]);
                    tops.push([p[0] + (q[0] - p[0]) * t, y, p[2] + (q[2] - p[2]) * t]);
                }
            }
            tops.push([q[0], q[1].max(y), q[2]]);
        }
        let first = vertices.len() as u32;
        for t in &tops {
            vertices.push(at(*t));
            vertices.push(at([t[0], base, t[2]]));
        }
        // Each top, then its foot; wound to face out as the land faces up (the renderer culls no face all the same).
        for k in 0..tops.len() as u32 - 1 {
            let (a, a_foot, b, b_foot) = (first + 2 * k, first + 2 * k + 1, first + 2 * k + 2, first + 2 * k + 3);
            indices.extend_from_slice(&[a, b, a_foot, b, b_foot, a_foot]);
        }
    }
    Mesh { vertices, indices }
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

/// The walls' image, in `WALL`'s colour: uploaded with the drape, which is painted again when the theme changes.
pub fn upload_wall(gfx: &mut dyn Gfx, ground: Color) {
    gfx.upload_image(WALL_IMAGE, 1, 1, &wall_rgba(WALL, ground));
}

/// The ground's height under (x, z) of the centred square, between the four nearest vertices of the mesh as its
/// triangles do closely enough; None outside the square.
pub fn ground_at(world: &World, x: f32, z: f32) -> Option<f32> {
    let n = world.samples;
    let (skip, m, side) = grid(world);
    // Blocks between two vertices of the mesh.
    let cell = (skip as f64 * world.step) as f32;
    let half = side / 2.0;
    let (c, r) = ((x + half) / cell, (z + half) / cell);
    let top = (m - 1) as f32;
    if !(0.0..=top).contains(&c) || !(0.0..=top).contains(&r) {
        return None;
    }
    let (c0, r0) = ((c.floor() as usize).min(m - 2), (r.floor() as usize).min(m - 2));
    let (fc, fr) = (c - c0 as f32, r - r0 as f32);
    // Vertex (col, row) of the mesh stands on sample (col * skip, row * skip).
    let h = |col: usize, row: usize| world.heights[row * skip * n + col * skip] as f32;
    let north = h(c0, r0) + (h(c0 + 1, r0) - h(c0, r0)) * fc;
    let south = h(c0, r0 + 1) + (h(c0 + 1, r0 + 1) - h(c0, r0 + 1)) * fc;
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
    // the ground and looks at one huge triangle, which says nothing.
    orbit.min_distance = world.step as f32 * 40.0;
    orbit
}

/// Moves the camera towards its goal and keeps it above the ground; true while it is still moving.
pub fn settle(orbit: &mut Orbit, world: &World, seconds: f32) -> bool {
    let moving = orbit.step(seconds);
    orbit.keep_above(&|x, z| ground_at(world, x, z).map_or(f32::NEG_INFINITY, |g| g + MARGIN));
    moving
}

/// The meshes live on the device, so they are uploaded again after a rebuild.
pub fn upload(gfx: &mut dyn Gfx, world: &World) {
    let land = land(world);
    gfx.upload_mesh(LAND_MESH, &land.vertices, &land.indices);
    let water = water(world);
    gfx.upload_mesh(WATER_MESH, &water.vertices, &water.indices);
    let walls = walls(world);
    gfx.upload_mesh(WALL_MESH, &walls.vertices, &walls.indices);
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
    // `upload_wall` must have given the walls their pixel.
    gfx.draw_mesh(viewport, WALL_MESH, &view_proj, Material::Image { image: WALL_IMAGE, nearest: true });
    // After the land and the walls: the sheet is tested against it but leaves no depth of its own.
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

    #[test]
    fn land_has_a_vertex_per_sample_and_two_triangles_per_cell() {
        let m = land(&world(256));
        assert_eq!(m.vertices.len(), 256 * 256);
        assert_eq!(m.indices.len(), 255 * 255 * 6);
        assert!(m.indices.iter().all(|&i| (i as usize) < m.vertices.len()));
        // Centred on the origin, 255 steps across.
        assert_eq!(m.vertices[0].pos[0], -255.0 * 16.0 / 2.0);
        assert_eq!(m.vertices[255].pos[0], 255.0 * 16.0 / 2.0);
        assert_eq!(m.vertices[1].pos[1], 1.0);
    }

    #[test]
    fn a_wide_grid_is_thinned() {
        let w = world(1024);
        assert_eq!(land(&w).vertices.len(), 512 * 512);
        // The walls follow the thinned grid: a top and a foot per border vertex, plus one per shore point.
        let walls = walls(&w);
        assert!(walls.vertices.len() >= 4 * 512 * 2);
        assert!(walls.indices.iter().all(|&i| (i as usize) < walls.vertices.len()));
    }

    /// (v1 - v0) x (v2 - v0) of triangle `t`: the side it faces, with the land's winding.
    fn normal(mesh: &Mesh, t: usize) -> [f32; 3] {
        let p = |k: usize| mesh.vertices[mesh.indices[3 * t + k] as usize].pos;
        let (a, b, c) = (p(0), p(1), p(2));
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
    }

    /// Whether triangle `t` is upright and faces away from the square's middle, as a wall round it does.
    fn faces_out(mesh: &Mesh, t: usize) -> bool {
        let n = normal(mesh, t);
        let at = |k: usize| mesh.vertices[mesh.indices[3 * t + k] as usize].pos;
        let middle = [(at(0)[0] + at(1)[0] + at(2)[0]) / 3.0, (at(0)[2] + at(1)[2] + at(2)[2]) / 3.0];
        n[1] == 0.0 && n[0] * middle[0] + n[2] * middle[1] > 0.0
    }

    /// Whether (x, z) lies on the border of a square `half` from the middle.
    fn on_border(p: [f32; 3], half: f32) -> bool {
        p[0].abs() == half || p[2].abs() == half
    }

    /// The checks every wall mesh passes: indices in range, one plain colour, tops on the border at the higher of
    /// the ground and the sheet, feet at the base, every triangle facing out.
    fn check_walls(w: &World) -> Mesh {
        let walls = walls(w);
        let half = grid(w).2 / 2.0;
        let (y, b) = (sea(w), base(w));
        assert!(walls.indices.iter().all(|&i| (i as usize) < walls.vertices.len()));
        assert!(walls.vertices.iter().all(|v| v.uv == WALL_UV), "every wall vertex has the plain colour's uv");
        for pair in walls.vertices.chunks(2) {
            let (top, foot) = (pair[0].pos, pair[1].pos);
            assert!(on_border(top, half), "{top:?} is on the border");
            assert_eq!((foot[0], foot[1], foot[2]), (top[0], b, top[2]));
            let ground = ground_at(w, top[0], top[2]).expect("the top is over the land");
            assert!((top[1] - ground.max(y)).abs() < 1e-3, "{top:?} over ground {ground}");
        }
        assert!((0..walls.indices.len() / 3).all(|t| faces_out(&walls, t)));
        walls
    }

    #[test]
    fn the_walls_close_the_land_down_to_the_base_in_one_plain_colour() {
        let w = world(16);
        // The base is under the lowest ground and under the sheet of water.
        let b = base(&w);
        assert!(b < w.height_range().0 as f32 && b < water(&w).vertices[0].pos[1], "base {b}");
        check_walls(&w);
        // Land over the sea all round: a quad per segment of each side, with no shore point.
        let mut high = world(16);
        high.heights = vec![70; 16 * 16];
        let walls = check_walls(&high);
        assert_eq!((walls.vertices.len(), walls.indices.len()), (4 * 16 * 2, 4 * 15 * 6));
        // The land faces up, the way the walls are wound to match.
        let land = land(&w);
        assert!((0..land.indices.len() / 3).all(|t| normal(&land, t)[1] > 0.0));
    }

    #[test]
    fn under_the_sea_the_wall_rises_to_the_sheet_and_a_shore_has_its_point() {
        let mut w = world(16);
        w.heights = vec![40; 16 * 16];
        // All under the sea: every top is at the sheet.
        assert!(check_walls(&w).vertices.chunks(2).all(|p| p[0].pos[1] == 62.5));
        // The five western columns at 70, the rest at 40: the ground crosses the sheet between columns 4 and 5.
        w.heights = (0..16 * 16).map(|i| if i % 16 < 5 { 70 } else { 40 }).collect();
        let walls = check_walls(&w);
        // From 70 to 40 the ground meets 62.5 a quarter of the way: 4 blocks east of column 4, at x -52.
        let half = 15.0 * 16.0 / 2.0;
        for z in [-half, half] {
            assert!(walls.vertices.iter().any(|v| v.pos == [-52.0, 62.5, z]), "the shore's point at z {z}");
        }
        // North and south each get one shore point: two more columns of wall than segments.
        assert_eq!(walls.vertices.len(), (4 * 16 + 2) * 2);
    }

    #[test]
    fn the_walls_leave_the_cameras_ground_on_top_of_the_land() {
        let w = world(16);
        let land = land(&w);
        let half = 15.0 * 16.0 / 2.0;
        // At every vertex of the grid, the border's included, the ground is the vertex's height, never the base.
        for v in &land.vertices {
            assert_eq!(ground_at(&w, v.pos[0], v.pos[2]), Some(v.pos[1]));
        }
        assert_eq!(ground_at(&w, half + 1.0, 0.0), None);
    }

    #[test]
    fn the_wall_colour_is_the_stage_ground_or_the_steep_earth() {
        assert_eq!(wall_rgba(WallColour::Ground, Color::hex(0x000000)), [0, 0, 0, 255]);
        assert_eq!(wall_rgba(WallColour::Ground, Color::hex(0xfafaf8)), [0xfa, 0xfa, 0xf8, 255]);
        let [r, g, b] = steep_colour();
        assert_eq!(wall_rgba(WallColour::Earth, Color::hex(0x000000)), [r, g, b, 255]);
        assert_eq!(wall_rgba(WallColour::Earth, Color::hex(0xfafaf8)), [r, g, b, 255]);
    }

    /// Twice the area of the water's triangles seen from above, positive when they face up as the land does.
    fn twice_area_up(mesh: &Mesh) -> f32 {
        (0..mesh.indices.len() / 3).map(|t| normal(mesh, t)[1]).sum()
    }

    #[test]
    fn water_lies_only_where_the_ground_is_under_the_sea() {
        let mut w = world(16);
        let cell = 16.0f32;
        // All under: the land's two triangles per cell, at the sheet, on the grid's own vertices.
        w.heights = vec![40; 16 * 16];
        let under = water(&w);
        assert_eq!(under.indices.len(), 15 * 15 * 6);
        assert!(under.vertices.iter().all(|v| v.pos[1] == 62.5));
        assert!((twice_area_up(&under) - 2.0 * (15.0 * cell).powi(2)).abs() < 1.0);
        // All above: no water, only the one empty triangle the renderer needs.
        w.heights = vec![70; 16 * 16];
        let none = water(&w);
        assert_eq!(none.indices, vec![0, 0, 0]);
        assert_eq!(twice_area_up(&none), 0.0);
        // The five western columns at 70, the rest at 40: the shore is the line a quarter of the way from column 4
        // to column 5, x -52, so the sea is 172 blocks wide and 240 long.
        w.heights = (0..16 * 16).map(|i| if i % 16 < 5 { 70 } else { 40 }).collect();
        let shore = water(&w);
        assert!(shore.indices.iter().all(|&i| (i as usize) < shore.vertices.len()));
        let used: Vec<[f32; 3]> = shore.indices.iter().map(|&i| shore.vertices[i as usize].pos).collect();
        assert!(used.iter().all(|p| p[1] == 62.5 && p[0] >= -52.0 - 1e-3), "no water over the land");
        assert!((twice_area_up(&shore) - 2.0 * 172.0 * 240.0).abs() < 1.0, "{}", twice_area_up(&shore));
        // Every triangle faces up, as the land's do.
        assert!((0..shore.indices.len() / 3).all(|t| normal(&shore, t)[1] > 0.0));
    }

    #[test]
    fn a_thinned_grid_keeps_the_water_and_the_ground_on_the_drawn_land() {
        // 2048 samples a side: one vertex every 4 samples, 512 a side, as a real wide run gives.
        let w = world(2048);
        let land = land(&w);
        let m = 512;
        let (lo, hi) = (land.vertices[0].pos[0], land.vertices[m * m - 1].pos[0]);
        assert_eq!(lo, -hi);
        // The water reaches the land's edges and never passes them.
        let water = water(&w);
        let used: Vec<[f32; 3]> = water.indices.iter().map(|&i| water.vertices[i as usize].pos).collect();
        assert!(used.iter().all(|p| (lo..=hi).contains(&p[0]) && (lo..=hi).contains(&p[2])));
        for k in [0, 2] {
            assert_eq!(used.iter().map(|p| p[k]).fold(f32::MAX, f32::min), lo);
            assert_eq!(used.iter().map(|p| p[k]).fold(f32::MIN, f32::max), hi);
        }
        // The camera's ground is the height of the vertex under it, at the edges as in the middle.
        for i in [0, 1, m - 1, m, m * m / 2 + 7, m * m - 1] {
            let v = land.vertices[i].pos;
            assert_eq!(ground_at(&w, v[0], v[2]), Some(v[1]), "vertex {i}");
        }
        // Halfway between two vertices of a row, the mean of the two, as the mesh's edge draws it.
        let (a, b) = (land.vertices[m + 3].pos, land.vertices[m + 4].pos);
        assert_eq!(ground_at(&w, (a[0] + b[0]) / 2.0, a[2]), Some((a[1] + b[1]) / 2.0));
        // Past the drawn edge there is no ground, even though the area's samples go further.
        assert_eq!(ground_at(&w, hi + 1.0, 0.0), None);
        assert_eq!(ground_at(&w, 0.0, lo - 1.0), None);
    }

    #[test]
    fn flat_ground_is_that_height_inside_the_square_and_nothing_outside() {
        let mut w = world(16);
        w.heights = vec![70; 16 * 16];
        let half = 15.0 * 16.0 / 2.0;
        assert_eq!(ground_at(&w, 0.0, 0.0), Some(70.0));
        assert_eq!(ground_at(&w, -half, half), Some(70.0));
        assert_eq!(ground_at(&w, half + 1.0, 0.0), None);
        assert_eq!(ground_at(&w, 0.0, -half - 1.0), None);
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
