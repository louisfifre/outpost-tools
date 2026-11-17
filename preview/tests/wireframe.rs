//! A picture of the 3D relief's mesh: every triangle's edges drawn over the land, close enough to read single
//! triangles. The renderer has no line mode, so each edge is a thin upright fin just above the surface, in a flat
//! colour tested against the land's depth. A one-off picture, not a check: run it with `cargo test --test wireframe
//! -- --ignored`, with an out folder at PREVIEW_DATA_DIR. It writes target/tmp/relief_wireframe.png.

use std::collections::BTreeSet;
use std::path::Path;

use mere_base::{Color, Gfx, Material, MeshId, Orbit, Rect, Vec3, Vertex};
use mere_win32::Renderer;
use preview::map::MAP_IMAGE;
use preview::paint::{View, paint};
use preview::scene;
use preview::world::World;

const SIZE: u32 = 1000;
const FINS: MeshId = MeshId(30);
// Blocks: the fin's foot above the edge, and its height.
const LIFT: f32 = 0.3;
const FIN: f32 = 1.5;
// Blocks round the point looked at whose edges are drawn: the rest is out of the picture.
const REACH: f32 = 700.0;

#[test]
#[ignore]
fn relief_wireframe_picture() {
    let dir = preview::world::data_dir();
    if !dir.join("preview.json").exists() {
        println!("skipped: PREVIEW_DATA_DIR names no out folder");
        return;
    }
    let world = World::load(&dir).expect("the data loads");
    // Looking at the ground under the middle, from 640 blocks away and about 40 degrees up: a cell of 16 blocks is
    // then some 25 pixels wide.
    let ground = scene::ground_at(&world, 0.0, 0.0).unwrap();
    let target = Vec3::new(0.0, ground, 0.0);
    let orbit = Orbit::new(target, target + Vec3::new(150.0, 410.0, 470.0));

    let land = scene::land(&world);
    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    for t in land.indices.chunks(3) {
        for (i, j) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (p, q) = (land.vertices[i as usize].pos, land.vertices[j as usize].pos);
            if p[0].abs().max(p[2].abs()) > REACH {
                continue;
            }
            let first = vertices.len() as u32;
            for (a, h) in [(p, LIFT), (q, LIFT), (p, LIFT + FIN), (q, LIFT + FIN)] {
                vertices.push(Vertex { pos: [a[0], a[1] + h, a[2]], uv: [0.0, 0.0] });
            }
            indices.extend_from_slice(&[first, first + 1, first + 2, first + 1, first + 3, first + 2]);
        }
    }

    let mut r = Renderer::offscreen(SIZE, SIZE, 1.0).expect("offscreen renderer");
    let n = world.samples as u32;
    r.upload_image(MAP_IMAGE, n, n, &paint(&world, View::Height, true, &BTreeSet::new()));
    scene::upload(&mut r, &world);
    scene::upload_wall(&mut r, Color::hex(0x000000));
    r.upload_mesh(FINS, &vertices, &indices);
    let viewport = Rect::new(0.0, 0.0, SIZE as f32, SIZE as f32);
    r.begin(Color::hex(0x000000));
    scene::draw(&mut r, viewport, &orbit, MAP_IMAGE, scene::water_alpha(View::Height));
    r.draw_mesh(viewport, FINS, &orbit.view_proj(1.0), Material::Flat { color: Color::hex(0xffffff).with_alpha(0.85) });
    r.end();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/relief_wireframe.png");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, mere_png::encode(SIZE, SIZE, &r.read_rgba().unwrap())).unwrap();
    println!("wrote {}", path.display());
}
