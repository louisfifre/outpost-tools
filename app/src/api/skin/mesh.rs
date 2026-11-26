// The player's mesh as data: 12 boxes (6 body parts, then the 6 slightly larger boxes of the outer layer) with
// their UV rectangles on the 64x64 skin image. Nothing is drawn here.
//
// Units are skin pixels (1 unit = 1/16 of a block), the feet at y = 0, the front facing +z, the character's right
// at -x (so the right arm is at -x, as in skinview3d). UVs are in 0..1 with v counted from the image's top, like the
// image's rows; the renderer's Vertex (mere_base: pos + uv, no normal) takes `pos` and `uv` and drops `normal`.
// `app` cannot depend on mere_base (it lives in preview/ and native/), so these types are the pure geometry; the
// caller maps them to `Vertex` and draws `inner` then `outer` with Material::Image { nearest: true }, since the
// outer layer is mostly transparent (docs/research/launcher.md, section 9).
//
// Sources: box origins and sizes from skinview3d's src/model.ts (MIT, https://github.com/bs-community/skinview3d,
// read 2026-11-25), and the unfolding of a box (top, bottom, right, front, left, back from the origin) as the
// official skin layout shows it (https://minecraft.wiki/w/Skin: 64x64, outer layer 0.5 px larger, 1 px on the head,
// slim arms 3 px). The face orientations (which way u and v run on each face) were derived from that unfolding, not
// compared with a rendering: TO VALIDATE in the game with a real skin picture.

pub const SKIN_SIZE: f32 = 64.0;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerMesh {
    /// Head, body, right arm, left arm, right leg, left leg: opaque.
    pub inner: Mesh,
    /// The same six parts, slightly larger: drawn after `inner`, with its transparency.
    pub outer: Mesh,
}

/// One box of the model: where it is, how big, and where it sits on the image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxSpec {
    pub name: &'static str,
    pub center: [f32; 3],
    /// Width (x), height (y), depth (z) in pixels of the part itself.
    pub size: [f32; 3],
    /// How much larger the box is than the part, on each of its dimensions (0 inner, 1 head outer, 0.5 others).
    pub grow: f32,
    /// The box's origin (u, v) on the 64x64 image.
    pub origin: [f32; 2],
    /// Width, height, depth of the unfolding on the image: the part's size, not the grown one.
    pub unfold: [f32; 3],
}

/// The 12 boxes: six inner, then six outer.
pub fn boxes(slim: bool) -> [BoxSpec; 12] {
    let arm_w = if slim { 3.0 } else { 4.0 };
    // The arm's centre is beside the body (half width 4), plus half the arm's width.
    let arm_x = 4.0 + arm_w / 2.0;
    let part = |name, center: [f32; 3], size: [f32; 3], inner: [f32; 2], outer: [f32; 2], grow: f32| {
        [
            BoxSpec { name, center, size, grow: 0.0, origin: inner, unfold: size },
            BoxSpec { name, center, size, grow, origin: outer, unfold: size },
        ]
    };
    let parts = [
        part("head", [0.0, 28.0, 0.0], [8.0, 8.0, 8.0], [0.0, 0.0], [32.0, 0.0], 1.0),
        part("body", [0.0, 18.0, 0.0], [8.0, 12.0, 4.0], [16.0, 16.0], [16.0, 32.0], 0.5),
        part("right_arm", [-arm_x, 18.0, 0.0], [arm_w, 12.0, 4.0], [40.0, 16.0], [40.0, 32.0], 0.5),
        part("left_arm", [arm_x, 18.0, 0.0], [arm_w, 12.0, 4.0], [32.0, 48.0], [48.0, 48.0], 0.5),
        part("right_leg", [-2.0, 6.0, 0.0], [4.0, 12.0, 4.0], [0.0, 16.0], [0.0, 32.0], 0.5),
        part("left_leg", [2.0, 6.0, 0.0], [4.0, 12.0, 4.0], [16.0, 48.0], [0.0, 48.0], 0.5),
    ];
    let mut out = [parts[0][0]; 12];
    for (i, p) in parts.iter().enumerate() {
        out[i] = p[0];
        out[6 + i] = p[1];
    }
    out
}

pub fn player_mesh(slim: bool) -> PlayerMesh {
    let mut mesh = PlayerMesh::default();
    for (i, spec) in boxes(slim).iter().enumerate() {
        add_box(if i < 6 { &mut mesh.inner } else { &mut mesh.outer }, spec);
    }
    mesh
}

/// The six faces of a box, each as four vertices (top-left of its image rectangle first, then clockwise on the image)
/// and two triangles counter-clockwise seen from outside.
fn add_box(mesh: &mut Mesh, spec: &BoxSpec) {
    let [cx, cy, cz] = spec.center;
    let [hx, hy, hz] = spec.size.map(|s| (s + spec.grow) / 2.0);
    let [u, v] = spec.origin;
    let [w, h, d] = spec.unfold;
    // (position of the rectangle's top-left corner, direction u runs, direction v runs, image rectangle)
    let faces: [([f32; 3], [f32; 3], [f32; 3], [f32; 4]); 6] = [
        // top (+y): seen from above, the front edge is the rectangle's bottom edge
        ([cx - hx, cy + hy, cz - hz], [2.0 * hx, 0.0, 0.0], [0.0, 0.0, 2.0 * hz], [u + d, v, w, d]),
        // bottom (-y): the front edge is the rectangle's top edge
        ([cx - hx, cy - hy, cz + hz], [2.0 * hx, 0.0, 0.0], [0.0, 0.0, -2.0 * hz], [u + d + w, v, w, d]),
        // right (-x): u runs toward the front
        ([cx - hx, cy + hy, cz - hz], [0.0, 0.0, 2.0 * hz], [0.0, -2.0 * hy, 0.0], [u, v + d, d, h]),
        // front (+z)
        ([cx - hx, cy + hy, cz + hz], [2.0 * hx, 0.0, 0.0], [0.0, -2.0 * hy, 0.0], [u + d, v + d, w, h]),
        // left (+x): u runs toward the back
        ([cx + hx, cy + hy, cz + hz], [0.0, 0.0, -2.0 * hz], [0.0, -2.0 * hy, 0.0], [u + d + w, v + d, d, h]),
        // back (-z): u runs toward -x
        ([cx + hx, cy + hy, cz - hz], [-2.0 * hx, 0.0, 0.0], [0.0, -2.0 * hy, 0.0], [u + 2.0 * d + w, v + d, w, h]),
    ];
    for (origin, du, dv, [ru, rv, rw, rh]) in faces {
        let normal = unit(cross(dv, du));
        let first = mesh.vertices.len() as u32;
        let corner = |a: f32, b: f32| {
            let pos = [0, 1, 2].map(|k| origin[k] + du[k] * a + dv[k] * b);
            Vertex { pos, normal, uv: [(ru + rw * a) / SKIN_SIZE, (rv + rh * b) / SKIN_SIZE] }
        };
        mesh.vertices.extend([corner(0.0, 0.0), corner(1.0, 0.0), corner(1.0, 1.0), corner(0.0, 1.0)]);
        mesh.indices.extend([0, 2, 1, 0, 3, 2].map(|i| first + i));
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: [f32; 3]) -> [f32; 3] {
    let len = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    a.map(|x| x / len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cross_of(m: &Mesh, t: usize) -> [f32; 3] {
        let p = |k: usize| m.vertices[m.indices[t * 3 + k] as usize].pos;
        let (a, b, c) = (p(0), p(1), p(2));
        cross([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]])
    }

    #[test]
    fn twelve_boxes_twenty_four_vertices_each_layer_of_six() {
        for slim in [false, true] {
            assert_eq!(boxes(slim).len(), 12);
            let m = player_mesh(slim);
            for layer in [&m.inner, &m.outer] {
                assert_eq!(layer.vertices.len(), 6 * 24);
                assert_eq!(layer.indices.len(), 6 * 36);
            }
        }
    }

    #[test]
    fn triangles_face_outward_and_match_their_normal() {
        let m = player_mesh(false);
        for layer in [&m.inner, &m.outer] {
            for t in 0..layer.indices.len() / 3 {
                let n = unit(cross_of(layer, t));
                let stored = layer.vertices[layer.indices[t * 3] as usize].normal;
                for k in 0..3 {
                    assert!((n[k] - stored[k]).abs() < 1e-5, "triangle {t}");
                }
            }
        }
    }

    #[test]
    fn every_uv_lies_inside_the_image_and_on_the_skin_rectangles() {
        for slim in [false, true] {
            let m = player_mesh(slim);
            for v in m.inner.vertices.iter().chain(&m.outer.vertices) {
                assert!((0.0..=1.0).contains(&v.uv[0]) && (0.0..=1.0).contains(&v.uv[1]));
            }
        }
    }

    #[test]
    fn head_front_is_the_official_rectangle() {
        // Head origin (0,0), 8x8x8: the front is the 8x8 at (8, 8) .. (16, 16).
        let m = player_mesh(false);
        let front = &m.inner.vertices[3 * 4..3 * 4 + 4];
        assert_eq!(front[0].normal, [0.0, 0.0, 1.0]);
        assert_eq!(front[0].uv, [8.0 / 64.0, 8.0 / 64.0]);
        assert_eq!(front[2].uv, [16.0 / 64.0, 16.0 / 64.0]);
        // Its left edge is the character's right side (-x): x grows with u.
        assert_eq!(front[0].pos, [-4.0, 32.0, 4.0]);
        assert_eq!(front[2].pos, [4.0, 24.0, 4.0]);
    }

    #[test]
    fn outer_layer_is_larger_and_uses_its_own_origins() {
        let b = boxes(false);
        let extent = |s: &BoxSpec| s.size[0] + s.grow;
        assert_eq!(extent(&b[6]) - extent(&b[0]), 1.0); // head
        for i in 1..6 {
            assert_eq!(extent(&b[6 + i]) - extent(&b[i]), 0.5, "{}", b[i].name);
        }
        let origins: Vec<_> = b.iter().map(|s| s.origin).collect();
        assert_eq!(origins[0], [0.0, 0.0]);
        assert_eq!(origins[6], [32.0, 0.0]);
        assert_eq!(origins[1], [16.0, 16.0]);
        assert_eq!(origins[7], [16.0, 32.0]);
        assert_eq!(origins[2], [40.0, 16.0]);
        assert_eq!(origins[8], [40.0, 32.0]);
        assert_eq!(origins[3], [32.0, 48.0]);
        assert_eq!(origins[9], [48.0, 48.0]);
        assert_eq!(origins[4], [0.0, 16.0]);
        assert_eq!(origins[10], [0.0, 32.0]);
        assert_eq!(origins[5], [16.0, 48.0]);
        assert_eq!(origins[11], [0.0, 48.0]);
    }

    #[test]
    fn slim_arms_are_three_pixels_wide_and_stay_against_the_body() {
        let wide = boxes(false);
        let slim = boxes(true);
        assert_eq!(wide[2].size[0], 4.0);
        assert_eq!(slim[2].size[0], 3.0);
        assert_eq!(slim[8].size[0], 3.0);
        // Inner arm's inner edge touches the body's side (x = 4).
        assert_eq!(slim[3].center[0] - slim[3].size[0] / 2.0, 4.0);
        assert_eq!(wide[3].center[0] - wide[3].size[0] / 2.0, 4.0);
        // Only the arms differ.
        for i in [0, 1, 4, 5, 6, 7, 10, 11] {
            assert_eq!(wide[i], slim[i]);
        }
        // The unfolding of a slim arm is narrower: its front starts at u + d and is 3 wide.
        let m = player_mesh(true);
        let right_arm_front = &m.inner.vertices[(2 * 6 + 3) * 4..][..4];
        assert_eq!(right_arm_front[2].uv[0] - right_arm_front[0].uv[0], 3.0 / 64.0);
    }

    #[test]
    fn the_figure_stands_on_y_zero_and_is_32_tall() {
        let m = player_mesh(false);
        let ys = m.inner.vertices.iter().map(|v| v.pos[1]);
        assert_eq!(ys.clone().fold(f32::MAX, f32::min), 0.0);
        assert_eq!(ys.fold(f32::MIN, f32::max), 32.0);
    }
}
