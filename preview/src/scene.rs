//! The 3D view of the page (preview.html `refresh3d`, lines 1842-1907), shared by the preview's stage and the
//! relief demo: the land as a mesh draped in the map's picture, the sheet of water, the orbit camera and the
//! ground under it. One unit is one block, heights included.

use mere_base::{Color, Gfx, ImageId, Material, MeshId, Orbit, Rect, Vec3, Vertex};

use crate::paint::View;
use crate::world::World;

pub const LAND_MESH: MeshId = MeshId(15);
pub const WATER_MESH: MeshId = MeshId(16);
/// The page's water (line 1901).
pub const WATER_COLOUR: Color = Color::hex(0x2f5c9e);

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

/// The land on `grid`'s vertices, closed round its border. The square is centred on the origin, so the camera can
/// turn around it.
pub fn land(world: &World) -> Mesh {
    let (_, m, _) = grid(world);
    let mut vertices = Vec::with_capacity(m * m + 4 * m);
    for row in 0..m {
        for col in 0..m {
            vertices.push(point(world, col, row));
        }
    }
    let mut indices = Vec::with_capacity((m - 1) * (m - 1) * 6 + 4 * (m - 1) * 6);
    for row in 0..m - 1 {
        for col in 0..m - 1 {
            let a = (row * m + col) as u32;
            let (b, c, d) = (a + 1, a + m as u32, a + m as u32 + 1);
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    // The skirt: under each side a wall from the border's vertices down to `base`, its foot keeping the vertex's
    // uv so the wall takes the colour of the border's texel. Without it the land is an open sheet, and from a low
    // angle one sees under it. The renderer culls no face; the walls face out all the same, as the land faces up.
    let base = base(world);
    for edge in border(m) {
        let foot = vertices.len() as u32;
        for &(col, row) in &edge {
            let top = vertices[row * m + col];
            vertices.push(Vertex { pos: [top.pos[0], base, top.pos[2]], uv: top.uv });
        }
        for k in 0..edge.len() - 1 {
            let ((ca, ra), (cb, rb)) = (edge[k], edge[k + 1]);
            let (a, b) = ((ra * m + ca) as u32, (rb * m + cb) as u32);
            let (a_foot, b_foot) = (foot + k as u32, foot + k as u32 + 1);
            indices.extend_from_slice(&[a, b, a_foot, b, b_foot, a_foot]);
        }
    }
    Mesh { vertices, indices }
}

/// The sheet of water at `sea`, ending where the land does, and closed on its sides: where the border's ground
/// lies under the sheet, a strip goes from the sheet down to that ground, meeting the land's skirt there.
pub fn water(world: &World) -> Mesh {
    let (_, m, side) = grid(world);
    let half = side / 2.0;
    let y = sea(world);
    let at = |p: [f32; 3]| Vertex { pos: p, uv: [0.0, 0.0] };
    let mut vertices = vec![at([-half, y, -half]), at([half, y, -half]), at([-half, y, half]), at([half, y, half])];
    let mut indices = vec![0, 2, 1, 1, 2, 3];
    // Each piece is added with its own vertices, in the order that faces out, as the land's skirt does.
    let mut add = |points: &[[f32; 3]], order: &[u32]| {
        let first = vertices.len() as u32;
        vertices.extend(points.iter().map(|&p| at(p)));
        indices.extend(order.iter().map(|i| first + i));
    };
    for edge in border(m) {
        for k in 0..edge.len() - 1 {
            let (p, q) = (point(world, edge[k].0, edge[k].1).pos, point(world, edge[k + 1].0, edge[k + 1].1).pos);
            let (p_top, q_top) = ([p[0], y, p[2]], [q[0], y, q[2]]);
            // Where the ground between p and q, a straight line as the land's border draws it, meets the sheet.
            let cross = || {
                let t = (y - p[1]) / (q[1] - p[1]);
                [p[0] + (q[0] - p[0]) * t, y, p[2] + (q[2] - p[2]) * t]
            };
            match (p[1] < y, q[1] < y) {
                (true, true) => add(&[p_top, q_top, p, q], &[0, 1, 2, 1, 3, 2]),
                (true, false) => add(&[p_top, cross(), p], &[0, 1, 2]),
                (false, true) => add(&[cross(), q_top, q], &[0, 1, 2]),
                (false, false) => {}
            }
        }
    }
    Mesh { vertices, indices }
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
    // After the land: the sheet is tested against it but leaves no depth of its own.
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
        // The grid, then a foot under each vertex of the four sides and two triangles per segment of side.
        assert_eq!(m.vertices.len(), 256 * 256 + 4 * 256);
        assert_eq!(m.indices.len(), 255 * 255 * 6 + 4 * 255 * 6);
        assert!(m.indices.iter().all(|&i| (i as usize) < m.vertices.len()));
        // Centred on the origin, 255 steps across.
        assert_eq!(m.vertices[0].pos[0], -255.0 * 16.0 / 2.0);
        assert_eq!(m.vertices[255].pos[0], 255.0 * 16.0 / 2.0);
        assert_eq!(m.vertices[1].pos[1], 1.0);
    }

    #[test]
    fn a_wide_grid_is_thinned() {
        let w = world(1024);
        let m = land(&w);
        assert_eq!(m.vertices.len(), 512 * 512 + 4 * 512);
        assert_eq!(m.indices.len(), 511 * 511 * 6 + 4 * 511 * 6);
        assert!(m.indices.iter().all(|&i| (i as usize) < m.vertices.len()));
        let b = base(&w);
        assert!(m.vertices[512 * 512..].iter().all(|v| v.pos[1] == b));
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

    #[test]
    fn the_land_is_closed_by_walls_down_to_the_base() {
        let w = world(16);
        let (m, half) = (16, 15.0 * 16.0 / 2.0);
        let land = land(&w);
        let (grid_vertices, grid_indices) = (m * m, (m - 1) * (m - 1) * 6);
        assert_eq!(land.vertices.len(), grid_vertices + 4 * m);
        assert_eq!(land.indices.len(), grid_indices + 4 * (m - 1) * 6);
        assert!(land.indices.iter().all(|&i| (i as usize) < land.vertices.len()));
        // The base is under the lowest ground and under the sheet of water.
        let b = base(&w);
        assert!(b < w.height_range().0 as f32 && b < water(&w).vertices[0].pos[1], "base {b}");
        // Every foot is at the base, straight under a vertex of the border, with that vertex's uv.
        for foot in &land.vertices[grid_vertices..] {
            assert_eq!(foot.pos[1], b);
            let top = land.vertices[..grid_vertices]
                .iter()
                .find(|v| (v.pos[0], v.pos[2]) == (foot.pos[0], foot.pos[2]))
                .expect("a vertex of the grid above the foot");
            assert!(on_border(top.pos, half), "{:?} is on the border", top.pos);
            assert_eq!(top.uv, foot.uv);
        }
        // The land faces up; each wall triangle faces out, wound the same way.
        assert!((0..grid_indices / 3).all(|t| normal(&land, t)[1] > 0.0));
        assert!((grid_indices / 3..land.indices.len() / 3).all(|t| faces_out(&land, t)));
    }

    #[test]
    fn the_skirt_leaves_the_cameras_ground_on_top_of_the_land() {
        let w = world(16);
        let land = land(&w);
        let half = 15.0 * 16.0 / 2.0;
        // At every vertex of the grid, the border's included, the ground is the vertex's height, never the base.
        for v in &land.vertices[..16 * 16] {
            assert_eq!(ground_at(&w, v.pos[0], v.pos[2]), Some(v.pos[1]));
        }
        assert_eq!(ground_at(&w, half + 1.0, 0.0), None);
    }

    #[test]
    fn a_sea_over_the_whole_border_is_closed_down_to_the_ground() {
        let mut w = world(16);
        w.heights = vec![40; 16 * 16];
        let (water, land) = (water(&w), land(&w));
        let half = 15.0 * 16.0 / 2.0;
        // The sheet, then a quad on every segment of the four sides.
        assert_eq!(water.indices.len(), 6 + 4 * 15 * 6);
        assert!(water.indices.iter().all(|&i| (i as usize) < water.vertices.len()));
        for v in &water.vertices[4..] {
            assert!(on_border(v.pos, half), "{:?} is on the border", v.pos);
            // From the sheet down to the ground, whose points are the land's own border.
            assert!(v.pos[1] == 62.5 || land.vertices.iter().any(|l| l.pos == v.pos), "{:?}", v.pos);
        }
        assert!((2..water.indices.len() / 3).all(|t| faces_out(&water, t)));
    }

    #[test]
    fn land_over_the_sea_has_no_strip_and_a_shore_has_a_triangle() {
        let mut w = world(16);
        w.heights = vec![70; 16 * 16];
        assert_eq!(water(&w).indices.len(), 6);
        // The five western columns at 70, the rest at 40: the ground crosses the sea between columns 4 and 5.
        w.heights = (0..16 * 16).map(|i| if i % 16 < 5 { 70 } else { 40 }).collect();
        let water = water(&w);
        // North and south: ten segments under the sea and the shore's triangle each; east fifteen; west none.
        assert_eq!(water.indices.len(), 6 + 2 * (10 * 6 + 3) + 15 * 6);
        assert!(water.indices.iter().all(|&i| (i as usize) < water.vertices.len()));
        // From 70 to 40 the ground meets 62.5 a quarter of the way: 4 blocks east of column 4, at x -56.
        let half = 15.0 * 16.0 / 2.0;
        for z in [-half, half] {
            assert!(water.vertices.iter().any(|v| v.pos == [-52.0, 62.5, z]), "the shore's point at z {z}");
        }
        assert!((2..water.indices.len() / 3).all(|t| faces_out(&water, t)));
    }

    #[test]
    fn water_is_one_quad_below_the_sea_level() {
        let w = world(256);
        let m = water(&w);
        // The sheet is the first quad; the strips on its sides come after it.
        assert_eq!(&m.indices[..6], &[0, 2, 1, 1, 2, 3]);
        assert!(m.vertices[..4].iter().all(|v| v.pos[1] == 62.5));
        // The sheet ends at the land's last vertex, 255 steps across, not at the area's 256.
        let land = land(&w);
        assert_eq!(m.vertices[3].pos[0], land.vertices[255].pos[0]);
        assert_eq!(m.vertices[0].pos[0], land.vertices[0].pos[0]);
    }

    #[test]
    fn a_thinned_grid_keeps_the_water_and_the_ground_on_the_drawn_land() {
        // 2048 samples a side: one vertex every 4 samples, 512 a side, as a real wide run gives.
        let w = world(2048);
        let land = land(&w);
        let m = 512;
        let (lo, hi) = (land.vertices[0].pos[0], land.vertices[m * m - 1].pos[0]);
        assert_eq!(lo, -hi);
        // The water's corners are the land's.
        let water = water(&w);
        assert_eq!((water.vertices[0].pos[0], water.vertices[0].pos[2]), (lo, lo));
        assert_eq!((water.vertices[3].pos[0], water.vertices[3].pos[2]), (hi, hi));
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
