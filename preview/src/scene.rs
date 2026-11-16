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

/// One vertex per sample up to 512 a side; a wider grid gives one vertex every other sample, which the eye
/// does not miss at that distance (the page does the same). The square is centred on the origin, so the
/// camera can turn around it.
pub fn land(world: &World) -> Mesh {
    let n = world.samples;
    let step = world.step as f32;
    let skip = n.div_ceil(512);
    let m = (n - 1) / skip + 1;
    let side = ((m - 1) * skip) as f32 * step;
    let mut vertices = Vec::with_capacity(m * m);
    for row in 0..m {
        for col in 0..m {
            let (sc, sr) = (col * skip, row * skip);
            vertices.push(Vertex {
                pos: [(sc as f32) * step - side / 2.0, world.heights[sr * n + sc] as f32, (sr as f32) * step - side / 2.0],
                // The middle of the sample's texel, so each cell of the map lies under its own vertex.
                uv: [(sc as f32 + 0.5) / n as f32, (sr as f32 + 0.5) / n as f32],
            });
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

/// Half a block under the sea level: ground at that level is land, and must not lie in the sheet of water.
pub fn water(world: &World) -> Mesh {
    let half = world.span() as f32 / 2.0;
    let y = world.sea_level as f32 - 0.5;
    let v = |x: f32, z: f32| Vertex { pos: [x, y, z], uv: [0.0, 0.0] };
    Mesh {
        vertices: vec![v(-half, -half), v(half, -half), v(-half, half), v(half, half)],
        indices: vec![0, 2, 1, 1, 2, 3],
    }
}

/// The ground's height under (x, z) of the centred square, between the four nearest points as the mesh's
/// triangles do closely enough; None outside the square.
pub fn ground_at(world: &World, x: f32, z: f32) -> Option<f32> {
    let n = world.samples;
    let step = world.step as f32;
    let half = (n - 1) as f32 * step / 2.0;
    let (c, r) = ((x + half) / step, (z + half) / step);
    let top = (n - 1) as f32;
    if !(0.0..=top).contains(&c) || !(0.0..=top).contains(&r) {
        return None;
    }
    let (c0, r0) = ((c.floor() as usize).min(n - 2), (r.floor() as usize).min(n - 2));
    let (fc, fr) = (c - c0 as f32, r - r0 as f32);
    let h = |col: usize, row: usize| world.heights[row * n + col] as f32;
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
        let m = land(&world(1024));
        assert_eq!(m.vertices.len(), 512 * 512);
    }

    #[test]
    fn water_is_one_quad_below_the_sea_level() {
        let m = water(&world(256));
        assert_eq!(m.indices.len(), 6);
        assert!(m.vertices.iter().all(|v| v.pos[1] == 62.5));
        assert_eq!(m.vertices[3].pos[0], 2048.0);
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
