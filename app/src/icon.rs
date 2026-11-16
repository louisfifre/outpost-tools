// The launcher's icon, the grass block seen from above, as pixels: three faces of a cube. It lives in the library,
// with no window in it, so the page launcher (tao) and the native one (mere-win32) show the same picture in the
// taskbar, each handing these pixels to its own window.

/// The side of the square picture, in pixels.
const SIDE: u32 = 64;

/// The icon as RGBA bytes, row after row, and the side of its square.
pub fn cube_rgba() -> (Vec<u8>, u32) {
    let faces: [([(f32, f32); 4], [u8; 3]); 3] = [
        ([(16.0, 2.0), (30.0, 9.0), (16.0, 16.0), (2.0, 9.0)], [0x7f, 0xb8, 0x50]),
        ([(2.0, 9.0), (16.0, 16.0), (16.0, 30.0), (2.0, 23.0)], [0x8b, 0x63, 0x44]),
        ([(30.0, 9.0), (30.0, 23.0), (16.0, 30.0), (16.0, 16.0)], [0x6b, 0x4b, 0x33]),
    ];
    let mut rgba = vec![0u8; (SIDE * SIDE * 4) as usize];
    for py in 0..SIDE {
        for px in 0..SIDE {
            // The drawing is on a 32-unit grid, as the page's SVG.
            let (x, y) = ((px as f32 + 0.5) / 2.0, (py as f32 + 0.5) / 2.0);
            for (corners, color) in &faces {
                if inside(corners, x, y) {
                    let i = ((py * SIDE + px) * 4) as usize;
                    rgba[i..i + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
                    break;
                }
            }
        }
    }
    (rgba, SIDE)
}

// A point is inside a convex shape when it is on the same side of each of its edges.
fn inside(corners: &[(f32, f32); 4], x: f32, y: f32) -> bool {
    let mut sign = 0.0f32;
    for i in 0..4 {
        let (ax, ay) = corners[i];
        let (bx, by) = corners[(i + 1) % 4];
        let cross = (bx - ax) * (y - ay) - (by - ay) * (x - ax);
        if cross != 0.0 {
            if sign != 0.0 && cross.signum() != sign {
                return false;
            }
            sign = cross.signum();
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_is_a_full_square_with_a_cube_in_it() {
        let (rgba, side) = cube_rgba();
        assert_eq!(rgba.len(), (side * side * 4) as usize);
        let alpha = |x: u32, y: u32| rgba[((y * side + x) * 4 + 3) as usize];
        // The corner is outside the cube, the middle is on one of its faces.
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(side / 2, side / 2 + 4), 255);
    }
}
