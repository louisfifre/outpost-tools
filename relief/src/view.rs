//! The relief as an application: the textured land, the sheet of water, the orbit camera and a small
//! overlay of facts about the area.

use crate::text;
use mere_base::{App, Button, Color, DrawList, Event, Gfx, ImageId, Orbit, Rect, Vec2};
use preview::paint::{View, paint};
use preview::scene;
use preview::world::World;
use std::collections::BTreeSet;

const MAP: ImageId = ImageId(1);

// The page's colours: its black ground (the dark theme's --ground), and the interface's hairline, text and
// muted text.
const GROUND: Color = Color::hex(0x000000);
const LINE: Color = Color::hex(0x2a2a2a);
const INK: Color = Color::hex(0xececec);
const MUTED: Color = Color::hex(0x8a8a8a);
// Behind the overlay's text, so it stays legible over bright snow.
const PLATE: Color = Color::hex(0x0b0b0b).with_alpha(0.78);

pub struct Relief {
    world: World,
    orbit: Orbit,
    /// Where the pointer was at the last move while a button holds the drag.
    drag_from: Option<Vec2>,
    /// The window's logical height, which sets how far a pixel of drag turns the camera.
    height: f32,
}

impl Relief {
    pub fn new(world: World) -> Relief {
        let orbit = scene::orbit(&world, None);
        Relief { world, orbit, drag_from: None, height: 800.0 }
    }

    fn overlay(&self) -> DrawList {
        let mut list = DrawList::default();
        let plate = Rect::new(16.0, 16.0, 290.0, 138.0);
        list.fill(plate, PLATE);
        list.frame(plate, 1.0, LINE);
        list.text(Vec2::new(28.0, 26.0), text::TITLE, 16.0, INK);
        list.text(Vec2::new(28.0, 50.0), text::area(&self.world), 12.0, INK);
        list.text(Vec2::new(28.0, 66.0), text::grid(&self.world), 12.0, MUTED);
        list.text(Vec2::new(28.0, 82.0), text::seed(&self.world), 12.0, MUTED);
        list.text(Vec2::new(28.0, 98.0), text::heights(&self.world), 12.0, MUTED);
        list.text(Vec2::new(28.0, 114.0), text::biomes(&self.world), 12.0, MUTED);
        list.text(Vec2::new(28.0, 130.0), text::HINT, 12.0, MUTED);
        list
    }
}

impl App for Relief {
    fn start(&mut self, gfx: &mut dyn Gfx) {
        let n = self.world.samples as u32;
        // The relief tab's picture in the dark theme, with nothing chosen.
        gfx.upload_image(MAP, n, n, &paint(&self.world, View::Height, true, &BTreeSet::new()));
        scene::upload(gfx, &self.world);
    }

    fn event(&mut self, event: Event) -> bool {
        match event {
            Event::Resized { height_px, scale, .. } => {
                self.height = height_px as f32 / scale;
                true
            }
            Event::PointerDown { pos, button: Button::Left, .. } => {
                self.drag_from = Some(pos);
                false
            }
            Event::PointerUp { button: Button::Left, .. } => {
                self.drag_from = None;
                false
            }
            Event::PointerMoved { pos } => match self.drag_from {
                Some(from) => {
                    self.orbit.drag(pos.x - from.x, pos.y - from.y, self.height);
                    self.drag_from = Some(pos);
                    true
                }
                None => false,
            },
            Event::Wheel { delta, .. } => {
                self.orbit.zoom(delta);
                true
            }
            _ => false,
        }
    }

    fn frame(&mut self, gfx: &mut dyn Gfx, seconds: f32) -> bool {
        let (wp, hp) = gfx.size_px();
        let s = gfx.scale();
        let (w, h) = (wp as f32 / s, hp as f32 / s);
        self.height = h;
        let moving = scene::settle(&mut self.orbit, &self.world, seconds);
        gfx.begin(GROUND);
        scene::draw(gfx, Rect::new(0.0, 0.0, w, h), &self.orbit, MAP, scene::water_alpha(View::Height));
        gfx.draw_list(&self.overlay());
        gfx.end();
        moving
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mere_base::Vec3;
    use mere_win32::Renderer;
    use std::f32::consts::FRAC_PI_2;
    use std::path::Path;

    const SIZE: (u32, u32) = (960, 600);

    fn app() -> Option<(Relief, Renderer)> {
        let dir = &preview::world::data_dir();
        if !dir.join("preview.json").exists() {
            return None;
        }
        let mut app = Relief::new(World::load(dir).unwrap());
        let mut r = Renderer::offscreen(SIZE.0, SIZE.1, 1.0).unwrap();
        app.start(&mut r);
        app.event(Event::Resized { width_px: SIZE.0, height_px: SIZE.1, scale: 1.0 });
        Some((app, r))
    }

    /// Draws frames until the camera has glided to rest, as the window does, then saves the last one.
    fn shot(app: &mut Relief, r: &mut Renderer, name: &str) {
        let mut frames = 0;
        while app.frame(r, 1.0 / 60.0) {
            frames += 1;
            assert!(frames < 600, "{name}: never came to rest");
        }
        let path = format!("{}/target/tmp/interact-{name}.png", env!("CARGO_MANIFEST_DIR"));
        std::fs::create_dir_all(Path::new(&path).parent().unwrap()).unwrap();
        std::fs::write(&path, mere_png::encode(SIZE.0, SIZE.1, &r.read_rgba().unwrap())).unwrap();
    }

    fn drag(app: &mut Relief, from: Vec2, to: Vec2) {
        app.event(Event::PointerDown { pos: from, button: Button::Left, clicks: 1 });
        // In small steps, like a real hand.
        for i in 1..=10 {
            let t = i as f32 / 10.0;
            app.event(Event::PointerMoved { pos: Vec2::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t) });
        }
        app.event(Event::PointerUp { pos: to, button: Button::Left });
    }

    /// Lets the glide finish the way frames do, and checks the camera is above the ground and not too far.
    fn rest(app: &mut Relief) -> Vec3 {
        let mut frames = 0;
        loop {
            let moving = scene::settle(&mut app.orbit, &app.world, 1.0 / 60.0);
            frames += 1;
            assert!(frames < 600, "never came to rest");
            if !moving {
                break;
            }
        }
        let eye = app.orbit.eye();
        if let Some(g) = scene::ground_at(&app.world, eye.x, eye.z) {
            assert!(eye.y >= g + 24.0 - 0.5, "camera under the ground: {eye:?} over {g}");
        }
        assert!((eye - app.orbit.target).length() <= app.orbit.max_distance * 1.001, "too far");
        eye
    }

    #[test]
    fn the_camera_answers_the_hand_as_orbit_controls_does() {
        let Some((mut app, mut r)) = app() else { return };
        shot(&mut app, &mut r, "0-start");
        let start = app.orbit.eye();
        assert!(start.x.abs() < 1.0 && start.y > 0.0 && start.z > 0.0);

        // Dragging right: three.js lowers the camera's angle round the vertical, so the camera goes to -x
        // and the scene seems to turn to the right.
        drag(&mut app, Vec2::new(400.0, 300.0), Vec2::new(300.0, 300.0));
        // Left by 100 px (60 degrees at this height): the camera goes to +x.
        let left = rest(&mut app);
        assert!(left.x > 0.0, "a drag to the left must bring the camera to +x, got {left:?}");
        shot(&mut app, &mut r, "1-drag-left");
        drag(&mut app, Vec2::new(300.0, 300.0), Vec2::new(400.0, 300.0));
        let back = rest(&mut app);
        assert!((back.x - start.x).abs() < 1.0 && (back.z - start.z).abs() < 1.0, "right undoes left");
        let height = back.y;

        // Dragging down lifts the camera in three.js; dragging up lowers it, down to the limit.
        drag(&mut app, Vec2::new(300.0, 100.0), Vec2::new(300.0, 500.0));
        let up = rest(&mut app);
        assert!(up.y > height, "a drag down must lift the camera");
        assert!(up.y / (up - app.orbit.target).length() <= (FRAC_PI_2 - 0.02).sin() + 1e-4, "pitch passes the limit");
        drag(&mut app, Vec2::new(300.0, 500.0), Vec2::new(300.0, 0.0));
        drag(&mut app, Vec2::new(300.0, 600.0), Vec2::new(300.0, 0.0));
        drag(&mut app, Vec2::new(300.0, 600.0), Vec2::new(300.0, 0.0));
        let low = rest(&mut app);
        shot(&mut app, &mut r, "2-drag-up-to-limit");
        assert!(low.y > 0.0, "the camera went under the horizon");

        let before = (rest(&mut app) - app.orbit.target).length();
        for _ in 0..5 {
            app.event(Event::Wheel { pos: Vec2::new(480.0, 300.0), delta: 1.0 });
        }
        let near = (rest(&mut app) - app.orbit.target).length();
        assert!(near < before, "wheel forward must come closer");
        shot(&mut app, &mut r, "3-wheel-in-5");
        for _ in 0..20 {
            app.event(Event::Wheel { pos: Vec2::new(480.0, 300.0), delta: -1.0 });
        }
        let far = (rest(&mut app) - app.orbit.target).length();
        assert!(far > near);
        shot(&mut app, &mut r, "4-wheel-out-20");

        // Far in: the camera must stay above the ground it looks at, and not vanish.
        for _ in 0..200 {
            app.event(Event::Wheel { pos: Vec2::new(480.0, 300.0), delta: 1.0 });
        }
        let close = rest(&mut app);
        shot(&mut app, &mut r, "5-wheel-in-max");
        assert!(close.is_finite_all());

        // A smaller window keeps the picture right: same scene, new aspect.
        app.event(Event::Resized { width_px: 480, height_px: 600, scale: 1.0 });
        r.resize(480, 600, 1.0).unwrap();
        while app.frame(&mut r, 1.0 / 60.0) {}
        let small = r.read_rgba().unwrap();
        assert_eq!(small.len(), 480 * 600 * 4);
        let path = format!("{}/target/tmp/interact-6-resized.png", env!("CARGO_MANIFEST_DIR"));
        std::fs::write(&path, mere_png::encode(480, 600, &small)).unwrap();
    }

    trait Finite {
        fn is_finite_all(&self) -> bool;
    }
    impl Finite for Vec3 {
        fn is_finite_all(&self) -> bool {
            self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
        }
    }
}
