//! relief_measure --data DIR [--level slopes|blocks|fine] [--out FILE] [--windowed]
//!
//! Measures the 3D relief in the real window: opens the preview's own window on the out folder DIR, maximised
//! unless --windowed, with the 3D view at --level, and plays a fixed script while it times every frame. The script: the map in 2D, the switch to
//! 3D, a turn round the land from where the camera starts, one from as close as the wheel goes, one from as far,
//! then 2D and 3D again. For each part it writes how long a frame took from one to the next, how much of that the
//! processor spent building and sending the frame, and how long the frame waited to be shown; and for each mesh sent
//! to the graphics card its vertices, triangles, bytes and the time to send it. The lines go to FILE (by default
//! the standard output), one JSON object each, with the clock's milliseconds so that a sampler of the system's
//! graphics counters running beside it can be matched to the parts (`relief_measure.ps1`).

use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use mere_base::{App, Color, Cursor, DrawList, Event, Gfx, Hit, ImageId, Mat4, Material, MeshId, Rect, Vec2, Vertex, WindowRequest};
use mere_ui::Theme;
use preview::world::World;
use preview::{MIN_SIZE, PreviewApp, PreviewState, scene};

/// Frames of each turn: five seconds at 120 a second, ten at 60.
const TURN: usize = 600;
/// Frames left for the camera to glide to where the wheel sent it, not counted.
const GLIDE: usize = 90;
/// Pixels of drag a frame while turning: a full turn in 400 frames of a view 800 high.
const DRAG: f32 = 2.0;

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis())
}

/// What one frame did, in milliseconds.
#[derive(Default, Clone, Copy)]
struct Spent {
    /// Sending meshes and images to the card.
    upload: f64,
    /// Waiting in `end` for the frame to be shown: the screen's refresh, or the card when it is late.
    shown: f64,
}

/// A `Gfx` that times what passes through it and counts the meshes.
struct Probe<'a> {
    inner: &'a mut dyn Gfx,
    spent: Spent,
    lines: Rc<RefCell<Vec<String>>>,
}

impl Gfx for Probe<'_> {
    fn size_px(&self) -> (u32, u32) {
        self.inner.size_px()
    }
    fn scale(&self) -> f32 {
        self.inner.scale()
    }
    fn upload_image(&mut self, id: ImageId, width: u32, height: u32, rgba: &[u8]) {
        let t = Instant::now();
        self.inner.upload_image(id, width, height, rgba);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        self.spent.upload += ms;
        self.lines.borrow_mut().push(format!(r#"{{"image":{},"width":{width},"height":{height},"bytes":{},"ms":{ms:.3}}}"#, id.0, rgba.len()));
    }
    fn upload_mesh(&mut self, id: MeshId, vertices: &[Vertex], indices: &[u32]) {
        let t = Instant::now();
        self.inner.upload_mesh(id, vertices, indices);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        self.spent.upload += ms;
        let bytes = size_of_val(vertices) + size_of_val(indices);
        self.lines.borrow_mut().push(format!(r#"{{"mesh":{},"vertices":{},"triangles":{},"bytes":{bytes},"ms":{ms:.3}}}"#, id.0, vertices.len(), indices.len() / 3));
    }
    fn begin(&mut self, ground: Color) {
        self.inner.begin(ground)
    }
    fn draw_mesh(&mut self, viewport: Rect, mesh: MeshId, view_proj: &Mat4, material: Material) {
        self.inner.draw_mesh(viewport, mesh, view_proj, material)
    }
    fn draw_list(&mut self, list: &DrawList) {
        self.inner.draw_list(list)
    }
    fn end(&mut self) {
        let t = Instant::now();
        self.inner.end();
        self.spent.shown += t.elapsed().as_secs_f64() * 1000.0;
    }
}

/// The parts of the script, in order.
#[derive(Copy, Clone, PartialEq, Debug)]
enum Part {
    Flat,
    Switch,
    Start,
    ToNear,
    Near,
    ToFar,
    Far,
    FlatAgain,
    SwitchAgain,
    Done,
}

impl Part {
    fn frames(self) -> usize {
        match self {
            Part::Flat | Part::FlatAgain => 60,
            Part::Switch | Part::SwitchAgain => 4,
            Part::Start | Part::Near | Part::Far => TURN,
            Part::ToNear | Part::ToFar => GLIDE,
            Part::Done => 0,
        }
    }
    fn next(self) -> Part {
        use Part::*;
        [Switch, Start, ToNear, Near, ToFar, Far, FlatAgain, SwitchAgain, Done, Done][self as usize]
    }
    fn name(self) -> &'static str {
        ["2d", "switch", "start", "to-near", "near", "to-far", "far", "2d-again", "switch-again", "done"][self as usize]
    }
}

struct Measure {
    app: PreviewApp,
    part: Part,
    /// Frames drawn in this part.
    at: usize,
    /// (from the frame before to this one, the processor's share, the wait to be shown), per frame of this part.
    frames: Vec<(f64, f64, f64)>,
    part_began: u128,
    last: Option<Instant>,
    lines: Rc<RefCell<Vec<String>>>,
    maximise: bool,
    close: bool,
}

impl Measure {
    fn enter(&mut self, part: Part) {
        self.part = part;
        self.at = 0;
        self.frames.clear();
        self.part_began = now_ms();
        let s = self.app.state_mut();
        match part {
            Part::Switch | Part::SwitchAgain => s.in3d = true,
            Part::FlatAgain => s.in3d = false,
            // Far more notches than the range holds: the camera stops at its limit.
            Part::ToNear => s.orbit.iter_mut().for_each(|o| o.zoom(400.0)),
            Part::ToFar => s.orbit.iter_mut().for_each(|o| o.zoom(-400.0)),
            Part::Done => self.close = true,
            _ => {}
        }
    }

    fn leave(&mut self) {
        let n = self.frames.len();
        if n == 0 {
            return;
        }
        let stat = |pick: fn(&(f64, f64, f64)) -> f64| {
            let mut v: Vec<f64> = self.frames.iter().map(pick).collect();
            v.sort_by(|a, b| a.total_cmp(b));
            let mean = v.iter().sum::<f64>() / n as f64;
            format!(r#"{{"mean":{mean:.3},"median":{:.3},"p95":{:.3},"max":{:.3}}}"#, v[n / 2], v[(n * 95 / 100).min(n - 1)], v[n - 1])
        };
        let each = if self.part.frames() <= 4 {
            let list: Vec<String> = self.frames.iter().map(|f| format!("[{:.2},{:.2},{:.2}]", f.0, f.1, f.2)).collect();
            format!(r#","each":[{}]"#, list.join(","))
        } else {
            String::new()
        };
        self.lines.borrow_mut().push(format!(
            r#"{{"part":"{}","from_ms":{},"to_ms":{},"frames":{n},"interval":{},"processor":{},"shown":{}{each}}}"#,
            self.part.name(),
            self.part_began,
            now_ms(),
            stat(|f| f.0),
            stat(|f| f.1),
            stat(|f| f.2)
        ));
    }
}

impl App for Measure {
    fn event(&mut self, event: Event) -> bool {
        // The hand is kept out of the script: only what the window itself says is passed on.
        match event {
            Event::Resized { .. } | Event::Window { .. } | Event::Scheme { .. } => self.app.event(event),
            _ => false,
        }
    }

    fn frame(&mut self, gfx: &mut dyn Gfx, seconds: f32) -> bool {
        if self.at >= self.part.frames() && self.part != Part::Done {
            self.leave();
            let next = self.part.next();
            self.enter(next);
        }
        if matches!(self.part, Part::Start | Part::Near | Part::Far) {
            if let Some(o) = self.app.state_mut().orbit.as_mut() {
                o.drag(DRAG, 0.0, 800.0);
            }
        }
        let began = Instant::now();
        let mut probe = Probe { inner: gfx, spent: Spent::default(), lines: self.lines.clone() };
        self.app.frame(&mut probe, seconds);
        let spent = probe.spent;
        let whole = began.elapsed().as_secs_f64() * 1000.0;
        let interval = self.last.map_or(whole, |l| began.duration_since(l).as_secs_f64() * 1000.0);
        self.last = Some(began);
        self.frames.push((interval, whole - spent.shown, spent.shown));
        self.at += 1;
        true
    }

    fn start(&mut self, gfx: &mut dyn Gfx) {
        let mut probe = Probe { inner: gfx, spent: Spent::default(), lines: self.lines.clone() };
        let began = Instant::now();
        self.app.start(&mut probe);
        self.lines.borrow_mut().push(format!(r#"{{"start_ms":{:.3}}}"#, began.elapsed().as_secs_f64() * 1000.0));
        self.part_began = now_ms();
    }

    fn hit(&self, pos: Vec2) -> Hit {
        self.app.hit(pos)
    }
    fn cursor(&self) -> Cursor {
        self.app.cursor()
    }
    fn take_request(&mut self) -> Option<WindowRequest> {
        if std::mem::take(&mut self.maximise) {
            return Some(WindowRequest::ToggleMaximize);
        }
        if std::mem::take(&mut self.close) {
            return Some(WindowRequest::Close);
        }
        self.app.take_request()
    }
}

fn usage() -> ! {
    eprintln!("usage: relief_measure --data DIR [--level slopes|blocks|fine] [--out FILE] [--windowed]");
    std::process::exit(2);
}

fn main() {
    let (mut data, mut out, mut windowed) = (None::<PathBuf>, None::<PathBuf>, false);
    let mut level = scene::Level::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--data" => data = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from),
            "--windowed" => windowed = true,
            "--level" => level = args.next().and_then(|k| scene::Level::from_key(&k)).unwrap_or_else(|| usage()),
            _ => usage(),
        }
    }
    let Some(data) = data else {
        eprintln!("relief_measure: --data names the out folder to draw");
        std::process::exit(2);
    };
    let world = World::load(&data).unwrap_or_else(|e| {
        eprintln!("relief_measure: {}: {e}", data.display());
        std::process::exit(1);
    });
    let lines = Rc::new(RefCell::new(Vec::new()));
    lines.borrow_mut().push(format!(r#"{{"pid":{},"samples":{},"step":{},"began_ms":{}}}"#, std::process::id(), world.samples, world.step, now_ms()));
    // What building the meshes costs the processor, before any window: each mesh's time, then all of them.
    let began = Instant::now();
    let built = [("land", scene::land(&world, level)), ("sides", scene::sides(&world, level)), ("water", scene::water(&world, level)), ("underside", scene::underside(&world, level))];
    let all = began.elapsed().as_secs_f64() * 1000.0;
    for (name, mesh) in &built {
        lines.borrow_mut().push(format!(r#"{{"built":"{name}","vertices":{},"triangles":{}}}"#, mesh.vertices.len(), mesh.indices.len() / 3));
    }
    lines.borrow_mut().push(format!(r#"{{"build_ms":{all:.3}}}"#));
    drop(built);

    let mut state = PreviewState::for_world(Some(&world));
    state.level = level;
    let app = PreviewApp::new(Theme::dark(), true, Vec2::new(1400.0, 940.0), state, Some(world));
    let mut measure = Measure { app, part: Part::Flat, at: 0, frames: Vec::new(), part_began: now_ms(), last: None, lines: lines.clone(), maximise: !windowed, close: false };
    let frame = mere_win32::Frame { own: true, min: MIN_SIZE, ..Default::default() };
    if let Err(e) = mere_win32::run_with("relief measure", &mut measure, frame) {
        eprintln!("relief_measure: {e}");
        std::process::exit(1);
    }
    let text = lines.borrow().join("\n") + "\n";
    match out {
        Some(path) => std::fs::write(&path, text).unwrap_or_else(|e| eprintln!("relief_measure: {}: {e}", path.display())),
        None => {
            let _ = std::io::stdout().write_all(text.as_bytes());
        }
    }
}
