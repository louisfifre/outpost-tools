//! What the account menu draws of a skin: the head (8 x 8) and the player from the front (24 x 40), cut from the 64 x 64
//! skin picture (or the old 64 x 32 one) that the api gives as base64 in a PNG. The layout of the picture is Mojang's
//! (https://minecraft.wiki/w/Skin, read 2026-11-25): the head's front at (8, 8), the hat over it at (40, 8), the body's
//! front at (20, 20), the right arm's at (44, 20), the right leg's at (4, 20), and in the 64 x 64 form the left arm's at
//! (36, 52), the left leg's at (20, 52) and the outer layers (jacket, sleeves, trousers) below the base ones. The old
//! form has no left limbs of its own: they are the right ones, mirrored. The player is drawn flat, as board A2 shows it;
//! the 3D player the skin's mesh allows is not drawn here.

use preview::Pixels;
use serde_json::Value;

/// A skin as the menu draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct SkinView {
    pub face: Pixels,
    pub figure: Pixels,
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64, padding or not; None for a character that is not one.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut bits, mut have) = (0u32, 0u32);
    for c in text.bytes().filter(|c| *c != b'=' && !c.is_ascii_whitespace()) {
        let v = B64.iter().position(|b| *b == c)? as u32;
        bits = (bits << 6) | v;
        have += 6;
        if have >= 8 {
            have -= 8;
            out.push((bits >> have) as u8);
            bits &= (1 << have) - 1;
        }
    }
    Some(out)
}

/// Standard base64 with padding: for the fake api's skins.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The skin the api answered (`{"width", "height", "rgba": base64}`: the pixels, which the api reads from the picture
/// with a full inflate: mere-png only reads the blocks its own encoder writes, not the ones Mojang's pictures hold);
/// None when it is not a skin.
pub fn read(body: &Value) -> Option<SkinView> {
    let rgba = base64_decode(body["rgba"].as_str()?)?;
    view(body["width"].as_u64()? as u32, body["height"].as_u64()? as u32, &rgba, body["slim"].as_bool().unwrap_or(false))
}

/// The head and the player of a skin's pixels; None for a picture that is not a skin's size.
pub fn view(width: u32, height: u32, rgba: &[u8], slim: bool) -> Option<SkinView> {
    if rgba.len() as u64 != width as u64 * height as u64 * 4 {
        return None;
    }
    let old = match (width, height) {
        (64, 64) => false,
        (64, 32) => true,
        _ => return None,
    };
    let at = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * width + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    // A part of the skin laid on `to` at (dx, dy): its base, then its outer layer over it, mirrored for an old skin's left limb.
    let part = |to: &mut Pixels, (dx, dy): (u32, u32), (sx, sy): (u32, u32), (w, h): (u32, u32), mirror: bool| {
        for y in 0..h {
            for x in 0..w {
                let from = if mirror { w - 1 - x } else { x };
                over(to, dx + x, dy + y, at(sx + from, sy + y));
            }
        }
    };
    let blank = |w: u32, h: u32| Pixels { w, h, rgba: vec![0; (w * h * 4) as usize] };
    let mut face = blank(8, 8);
    part(&mut face, (0, 0), (8, 8), (8, 8), false);
    part(&mut face, (0, 0), (40, 8), (8, 8), false);
    let mut figure = blank(24, 40);
    let arm = if slim { 3 } else { 4 };
    // The head, then the body with its jacket.
    part(&mut figure, (8, 0), (8, 8), (8, 8), false);
    part(&mut figure, (8, 0), (40, 8), (8, 8), false);
    part(&mut figure, (8, 8), (20, 20), (8, 12), false);
    if !old {
        part(&mut figure, (8, 8), (20, 36), (8, 12), false);
    }
    // The right arm and leg are on the viewer's left; the left ones, in an old skin, are the same mirrored.
    part(&mut figure, (8 - arm, 8), (44, 20), (arm, 12), false);
    part(&mut figure, (8, 20), (4, 20), (4, 12), false);
    if old {
        part(&mut figure, (16, 8), (44, 20), (arm, 12), true);
        part(&mut figure, (12, 20), (4, 20), (4, 12), true);
    } else {
        part(&mut figure, (8 - arm, 8), (44, 36), (arm, 12), false);
        part(&mut figure, (8, 20), (4, 36), (4, 12), false);
        part(&mut figure, (16, 8), (36, 52), (arm, 12), false);
        part(&mut figure, (16, 8), (52, 52), (arm, 12), false);
        part(&mut figure, (12, 20), (20, 52), (4, 12), false);
        part(&mut figure, (12, 20), (4, 52), (4, 12), false);
    }
    Some(SkinView { face, figure })
}

/// A pixel laid over what is there: opaque replaces, clear leaves, between mixes.
fn over(to: &mut Pixels, x: u32, y: u32, px: [u8; 4]) {
    if px[3] == 0 {
        return;
    }
    let i = ((y * to.w + x) * 4) as usize;
    if px[3] == 255 || to.rgba[i + 3] == 0 {
        to.rgba[i..i + 4].copy_from_slice(&px);
        return;
    }
    let a = px[3] as f32 / 255.0;
    for c in 0..3 {
        to.rgba[i + c] = (px[c] as f32 * a + to.rgba[i + c] as f32 * (1.0 - a)).round() as u8;
    }
    to.rgba[i + 3] = 255;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_goes_both_ways() {
        for bytes in [&b""[..], b"a", b"ab", b"abc", b"abcd", &[0, 255, 128, 7, 9]] {
            assert_eq!(base64_decode(&base64_encode(bytes)).unwrap(), bytes);
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert!(base64_decode("not base64!").is_none());
    }

    #[test]
    fn a_skin_is_read_from_the_pixels_the_api_gives() {
        let pixels = crate::fake::sample_skin(0x4a2f1d);
        let body = serde_json::json!({"width": 64, "height": 64, "slim": false, "rgba": base64_encode(&pixels)});
        let skin = read(&body).expect("a skin of 64 by 64");
        assert_eq!((skin.face.w, skin.face.h), (8, 8));
        assert!(skin.face.rgba.chunks(4).all(|p| p[3] == 255), "the head's front is opaque");
        assert!(skin.figure.rgba.chunks(4).any(|p| p[3] == 255));
        // Pixels that are not the size the picture says, and a picture that is not a skin's size, are no skin.
        assert!(read(&serde_json::json!({"width": 64, "height": 64, "rgba": base64_encode(&pixels[..100])})).is_none());
        assert!(read(&serde_json::json!({"width": 32, "height": 32, "rgba": base64_encode(&pixels[..32 * 32 * 4])})).is_none());
        assert!(read(&serde_json::json!({"png": "iVBORw0KGgo="})).is_none(), "the old answer with no pixels");
    }
}
