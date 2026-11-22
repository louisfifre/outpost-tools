//! The preview as an application: the shell built every frame, the map's picture painted and uploaded whenever
//! what it shows changes, and the 3D scene drawn into the stage's square under the interface.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use std::collections::VecDeque;

use mere_base::{App, Cursor, DrawList, Event, Gfx, Hit, Rect, Vec2, WindowRequest, observe};
use mere_ui::{Id, Theme, Ui, chrome_hit};

use crate::cut::{self, CUT_IMAGE, NAV_H, NAV_IMAGE, CutLook};
use crate::figures::Figures;
use crate::history;
use crate::home::Home;
use crate::json::Json;
use crate::map::MAP_IMAGE;
use crate::paint::{View, paint};
use crate::apply::{self, Fail, Host, Notice, Outcome, SystemHost, Target};
use crate::run::{self, Ask, DETAIL_BLOCKS, Drawn, Follow, Pace, Runs, Steps};
use crate::scene::{self, Level};
use crate::section::Shown;
use crate::shell::{Mode, PreviewState, RunEnd, ShellIds, Want, build_run, raise_to_min};
use crate::text;
use crate::world::World;

/// A run of the engine being followed: the engine works on a thread of its own and sends its lines here.
struct Running {
    thread: Option<JoinHandle<Result<PathBuf, String>>>,
    stop: Arc<AtomicBool>,
    lines: Receiver<String>,
    /// Where the lines are kept, as the generator's output was.
    log: Option<File>,
    /// This run's number in the process: tells two runs apart, as a process id did.
    id: u64,
    started: Instant,
    /// When the progress file was last read: it is read once a second, the first time one second after the start
    /// (line 2649).
    looked: Instant,
}

pub struct PreviewApp {
    ui: Ui,
    theme: Theme,
    dark: bool,
    size: Vec2,
    state: PreviewState,
    world: Option<World>,
    ids: ShellIds,
    /// What the uploaded picture was painted for: the view, the theme and the choice. None until a first
    /// upload, and again after `start`, because a rebuilt device has lost its images.
    painted: Option<(View, bool, BTreeSet<String>)>,
    /// The interface list of the last frame, which a test draws alone to see what the scene covered.
    last: DrawList,
    /// The cut being looked at, with its ground and the ores played on it: the heavy data, kept out of the state
    /// (which is cloned and compared at every build). None outside the section view.
    shown: Option<Shown>,
    /// The cut last shown, so that coming back to the same one keeps the look (the page's sameLine).
    last_at: Option<usize>,
    /// What the strip's picture was made for: the cut and its width. None until a first upload and after start.
    nav_painted: Option<(usize, u32)>,
    /// What the cut picture was made for: the cut, the blocks in sight, whether ores are drawn wide, the choice, and
    /// the layer's version.
    cut_painted: Option<CutKey>,
    /// How and where runs happen; None for a window that only looks (Generate then says it failed).
    runs: Option<Runs>,
    running: Option<Running>,
    /// What this machine measured: the file's content, which the app owns (the state holds a copy for the shell).
    pace: Pace,
    /// The sheet last uploaded for the cut, as pixels: what an export keeps as section.png.
    last_sheet: Option<(u32, u32, Vec<u8>)>,
    /// A run asked for while another was under way: it starts when that one has ended (line 2696).
    queued: bool,
    /// The window follows the system's light or dark setting (Event::Scheme); a fixed theme ignores it.
    follows_scheme: bool,
    /// What the window was asked for (the bar's buttons), and the title it was last given.
    requests: VecDeque<WindowRequest>,
    title_sent: Option<String>,
    /// The app's own time, for the tooltips' delay; a test fixes it with `set_clock`.
    born: Instant,
    clock: Option<f64>,
    /// The engine's end, seen and not yet acted on (`look`): the out folder, or what went wrong.
    ended: Option<Result<PathBuf, String>>,
    /// The level the device holds the stage's meshes at. None when it holds none of this world's: before the 3D view
    /// is first shown, after a new world, and after `start`. They are built when the 3D view next shows, so a
    /// window that stays in 2D builds none.
    meshed: Option<Level>,
    /// The level the sides' picture was made at, for the picture last painted; None when that picture changed.
    sided: Option<Level>,
    /// The map's picture last painted, which the columns' sides are shaded from when the 3D view shows.
    drape: Vec<u8>,
    /// The launcher's Home, when the host gave one.
    home: Option<Box<dyn Home>>,
    /// Apply to pack: the machine it asks (given with a pack), and the write under way with the values it writes.
    apply_host: Option<Arc<dyn Host>>,
    applying: Option<(JoinHandle<Outcome>, [f64; 43])>,
    /// The form's values as `draft.json` holds them; None without runs, where nothing is kept.
    draft_saved: Option<[f64; 43]>,
    /// The run's own picture, which the stage shows over `world` while the run goes on: its last sketch, then its
    /// map (`follow.drawn` says which). None outside a run and before its first sketch.
    drawn: Option<World>,
}

type CutKey = (usize, usize, usize, usize, usize, bool, BTreeSet<String>, u32);

impl PreviewApp {
    pub fn new(theme: Theme, dark: bool, size: Vec2, mut state: PreviewState, world: Option<World>) -> PreviewApp {
        // What the map was made from is a fact of the world, whatever the form says.
        state.made = world.as_ref().and_then(run::ask_of);
        // The look a caller gave (--cut-look) is for the cut it also named: it is not fresh at the first sync.
        let last_at = Some(state.cut_at);
        let mut app = PreviewApp {
            ui: Ui::new(theme),
            theme,
            dark,
            size: raise_to_min(size),
            state,
            world,
            ids: ShellIds::default(),
            painted: None,
            last: DrawList::default(),
            shown: None,
            last_at,
            nav_painted: None,
            cut_painted: None,
            runs: None,
            running: None,
            pace: Pace::default(),
            last_sheet: None,
            queued: false,
            follows_scheme: false,
            requests: VecDeque::new(),
            title_sent: None,
            born: Instant::now(),
            clock: None,
            ended: None,
            meshed: None,
            sided: None,
            drape: Vec::new(),
            home: None,
            apply_host: None,
            applying: None,
            draft_saved: None,
            drawn: None,
        };
        // Line 2815: the map that opens the page is kept like any other (in memory until there are runs).
        app.remember();
        app
    }

    /// Lets the window run a generator: the learned pace is read from the runs' folder, and so are the history
    /// and the figures of the run before, which the page keeps in its localStorage.
    pub fn with_runs(mut self, runs: Runs) -> PreviewApp {
        self.pace = Pace::load(&runs.pace());
        self.state.pace = self.pace.clone();
        self.state.history = history::load(&runs.history());
        self.state.before = std::fs::read_to_string(runs.before()).ok().and_then(|t| Json::parse(&t).ok()).and_then(|j| Figures::from_json(&j));
        // A pack is a place Apply can write to; the real machine is asked about what runs on it.
        self.state.pack_known = runs.pack.is_some();
        // The pack's values are the ones its file holds now: an apply, or an edit by hand, moved them since the
        // copy in `settings.rs` was made. A form still on that copy follows.
        let held = runs.pack.as_ref().and_then(|p| std::fs::read_to_string(p.join("config").join("tectonic.json")).ok()).and_then(|t| apply::read_values(&t));
        if let Some(held) = held {
            if self.state.made.is_none() && self.state.values == self.state.baseline {
                self.state.values = held;
            }
            self.state.baseline = held;
        }
        // What was typed last, generated or not, comes back over the last run's values.
        if let Some(typed) = crate::draft::load(&runs.draft(), &self.state.values) {
            self.state.values = typed;
        }
        self.draft_saved = Some(self.state.values);
        if self.apply_host.is_none() {
            self.apply_host = Some(Arc::new(SystemHost));
        }
        self.runs = Some(runs);
        self.remember();
        self
    }

    /// Puts another machine under Apply (a test's).
    pub fn with_apply_host(mut self, host: Arc<dyn Host>) -> PreviewApp {
        self.apply_host = Some(host);
        self
    }

    /// Whether a write to the pack is under way.
    pub fn applying(&self) -> bool {
        self.applying.is_some()
    }

    /// Gives the app the Home of its host (the launcher's): the pane under the bar while that tool is on.
    pub fn with_home(mut self, home: Box<dyn Home>) -> PreviewApp {
        self.home = Some(home);
        self
    }

    /// The host's Home, if there is one.
    pub fn home(&self) -> Option<&dyn Home> {
        self.home.as_deref()
    }

    /// The app's own time in seconds, or the one a test fixed.
    fn now(&self) -> f64 {
        self.clock.unwrap_or_else(|| self.born.elapsed().as_secs_f64())
    }

    /// `remember` (1546-1560): the map on screen is added to the history unless it is the one kept last, and the
    /// file is written when there are runs to keep it in. There is no quota natively, so nothing is evicted.
    fn remember(&mut self) {
        let (Some(world), Some(made)) = (&self.world, &self.state.made) else { return };
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_millis() as f64);
        let entry = history::entry(made, &world.measured.figures, at);
        if !history::remember(&mut self.state.history, entry) {
            return;
        }
        if let Some(runs) = &self.runs {
            if let Err(e) = history::save(&runs.history(), &self.state.history) {
                observe!(Warn, "the history could not be kept in {}: {e}", runs.history().display());
            }
        }
    }

    /// Makes the app switch theme when the system's setting changes, as the page does with prefers-color-scheme.
    /// Fixes the time the tooltips see (seconds), or gives it back to the real clock with None. For tests.
    pub fn set_clock(&mut self, seconds: Option<f64>) {
        self.clock = seconds;
    }

    pub fn follow_scheme(mut self, yes: bool) -> PreviewApp {
        self.follows_scheme = yes;
        self
    }

    /// Switches to the dark or the light theme. The painted key holds dark, so the map and the drape repaint by
    /// themselves; the cut and the strip do not depend on the theme.
    pub fn set_dark(&mut self, dark: bool) {
        self.theme = if dark { Theme::dark() } else { Theme::light() };
        self.ui.set_theme(self.theme);
        self.dark = dark;
    }

    /// Whether a generator is under way.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// The number of the run under way, unique in this process.
    pub fn run_id(&self) -> Option<u64> {
        self.running.as_ref().map(|r| r.id)
    }

    /// Makes the run `seconds` old, as the page's `already` does for a run it found under way: the clock, the
    /// button's fill and the hairline then read from that age. A test uses it to see a run at 40 %.
    pub fn set_run_age(&mut self, seconds: u64) {
        if let Some(r) = self.running.as_mut() {
            r.started = Instant::now().checked_sub(Duration::from_secs(seconds)).unwrap_or_else(Instant::now);
        }
    }

    /// The sheet last uploaded for the cut: its width, its height and its RGBA rows.
    pub fn last_sheet(&self) -> Option<&(u32, u32, Vec<u8>)> {
        self.last_sheet.as_ref()
    }

    pub fn world(&self) -> Option<&World> {
        self.world.as_ref()
    }

    pub fn ui(&self) -> &Ui {
        &self.ui
    }

    /// Where the parts stood in the last build.
    pub fn ids(&self) -> &ShellIds {
        &self.ids
    }

    /// The interface list of the last frame (what is drawn over the 3D scene).
    pub fn list(&self) -> &DrawList {
        &self.last
    }

    /// The view the uploaded picture was painted for.
    pub fn painted_view(&self) -> Option<View> {
        self.painted.as_ref().map(|p| p.0)
    }

    /// The choice the uploaded picture was painted with.
    pub fn painted_choice(&self) -> Option<&BTreeSet<String>> {
        self.painted.as_ref().map(|p| &p.2)
    }

    /// Whether the cut's picture was made with ores two blocks wide (a scale under 0.75); None before a first picture.
    pub fn cut_reach(&self) -> Option<bool> {
        self.cut_painted.as_ref().map(|k| k.5)
    }

    /// The cut being shown, when the section view has one.
    pub fn shown(&self) -> Option<&Shown> {
        self.shown.as_ref()
    }

    pub fn state(&self) -> &PreviewState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut PreviewState {
        &mut self.state
    }

    pub fn ui_mut(&mut self) -> &mut Ui {
        &mut self.ui
    }

    /// What the map's picture should be painted for now, or None when the frame shows no map picture (no
    /// world, the section, or the structures' view of a preview without structures).
    fn wanted(&self) -> Option<(View, bool, BTreeSet<String>)> {
        let world = self.stage()?;
        let view = View::from_index(self.state.view)?;
        // The 3D view drapes the same picture over the land.
        if view == View::Structures && world.structures.is_none() {
            return None;
        }
        // The land under the marks does not change when the marks do, so the structures' key has no choice.
        let chosen = if view == View::Structures { BTreeSet::new() } else { self.state.chosen[view.index()].clone() };
        Some((view, self.dark, chosen))
    }

    /// The world the stage shows: the run's picture while there is one, else the map.
    fn stage(&self) -> Option<&World> {
        self.drawn.as_ref().or(self.world.as_ref())
    }

    /// Once a second while a run goes on, as the page's `look` (lines 2629-2648): the map once the run is past it
    /// (its cuts and structures are still to come), else the last sketch when it is finer than the one shown. The
    /// app removed both files before the run, so what is found is this run's.
    fn draw_run(&mut self) {
        let (Some(follow), Some(runs)) = (self.state.follow.as_ref(), self.runs.as_ref()) else { return };
        let out = runs.out();
        let (next, drawn) = if follow.pass_key.starts_with("extras") {
            if follow.drawn == Drawn::Map {
                return;
            }
            (World::load(&out), Drawn::Map)
        } else {
            let finer = |step: u32| match follow.drawn {
                Drawn::Nothing => true,
                Drawn::Sketch(shown) => step < shown,
                Drawn::Map => false,
            };
            let step = std::fs::read_to_string(out.join("coarse.json")).ok().and_then(|t| Json::parse(&t).ok()).and_then(|j| j.get("step").and_then(Json::num));
            match step.map(|s| s as u32).filter(|s| finer(*s)) {
                Some(step) => (World::load_sketch(&out), Drawn::Sketch(step)),
                None => return,
            }
        };
        match next {
            Ok(world) => {
                // A wider or narrower square puts the camera back, as a new map does (line 1873).
                if self.stage().map(World::span) != Some(world.span()) {
                    self.state.orbit = None;
                }
                observe!(Info, "the stage shows the run's {drawn:?}: {} points a side every {} blocks", world.samples, world.step);
                self.drawn = Some(world);
                if let Some(f) = self.state.follow.as_mut() {
                    f.drawn = drawn;
                }
                self.restage();
            }
            // Read again at the next look: a file may be between two writes.
            Err(e) => observe!(Debug, "the run's {drawn:?} is not readable yet: {e}"),
        }
    }

    /// The stage's world changed: its pictures are painted again, and its meshes built again when the 3D view shows.
    fn restage(&mut self) {
        self.painted = None;
        self.nav_painted = None;
        self.cut_painted = None;
        self.shown = None;
        self.meshed = None;
    }
}

impl PreviewApp {
    /// Keeps shown for the cut the state names (the page's showCut): made when the section view is first built
    /// and when another cut is asked for, dropped outside the section view. A cut that is not the one last looked
    /// at starts with a fresh look; coming back to the same one keeps it.
    fn sync_cut(&mut self) {
        let Some(section) = self.drawn.as_ref().or(self.world.as_ref()).filter(|w| self.state.view == crate::paint::SECTION && !w.coarse()).and_then(|w| w.section.as_ref()) else {
            self.shown = None;
            return;
        };
        let at = self.state.cut_at.min(section.cuts.len() - 1);
        self.state.cut_at = at;
        if self.shown.as_ref().is_some_and(|s| s.at == at) {
            return;
        }
        if self.last_at != Some(at) {
            self.state.cut_look = CutLook::FRESH;
        }
        let started = Instant::now();
        self.shown = self.drawn.as_ref().or(self.world.as_ref()).and_then(|w| Shown::new(w, at));
        self.last_at = Some(at);
        observe!(Info, "cut {} of {}: ground and ore figures played in {} ms", at + 1, section.cuts.len(), started.elapsed().as_millis());
    }

    /// Uploads the strip and the part of the cut in sight when what they show has changed: the strip once per cut
    /// and width, the sheet whenever the sight, the choice or the drawn layer changes. Before the list is drawn.
    fn upload_cut(&mut self, gfx: &mut dyn Gfx) {
        let (Some(world), Some(shown)) = (self.drawn.as_ref().or(self.world.as_ref()), self.shown.as_mut()) else { return };
        let Some(section) = world.section.as_ref() else { return };
        let Some(nav) = self.ui.rect(self.ids.nav).filter(|_| self.ids.nav != Id::ROOT) else { return };
        let width = nav.w.round() as u32;
        if self.nav_painted != Some((shown.at, width)) {
            let (rgba, _) = cut::nav_image(section, &shown.ground, width as usize);
            gfx.upload_image(NAV_IMAGE, width, NAV_H as u32, &rgba);
            self.nav_painted = Some((shown.at, width));
        }
        let look = self.state.cut_look;
        // The box the shell settled the look against; the laid-out rect of the cut view is the same, which the tests check.
        let (bw, bh) = self.ids.cut_box;
        let Some(sight) = cut::sight(look, bw, bh, section.length, shown.ground.rows) else { return };
        shown.draw_chunks(world, (sight.x0 >> 4) as i64 - 1, (sight.x1 >> 4) as i64 + 1);
        let key: CutKey = (shown.at, sight.x0, sight.x1, sight.r0, sight.r1, look.k < 0.75, self.state.chosen_ores.clone(), shown.play.version);
        if self.cut_painted.as_ref() != Some(&key) {
            let started = Instant::now();
            let cells = section.cuts[shown.at].cells.as_ref().expect("a shown cut has its cells");
            let rgba = cut::sheet(section, cells, &shown.ground, &shown.play.layer, sight, look.k, &self.state.chosen_ores);
            gfx.upload_image(CUT_IMAGE, sight.w() as u32, sight.h() as u32, &rgba);
            self.last_sheet = Some((sight.w() as u32, sight.h() as u32, rgba));
            observe!(Debug, "cut picture {}x{} for blocks {}..{} in {} ms", sight.w(), sight.h(), sight.x0, sight.x1, started.elapsed().as_millis());
            self.cut_painted = Some(key);
        }
    }
}

impl PreviewApp {
    /// The form as the next run would take it.
    fn asked(&self) -> Ask {
        Ask { seed: self.state.seed.trim().to_string(), samples: self.state.samples, detail: DETAIL_BLOCKS[self.state.detail], values: self.state.values }
    }

    /// Looks at the generator: whether it has ended, how old it is, and (once a second) what its progress file says.
    /// Before the frame is built, so the frame shows what was found.
    fn poll(&mut self) {
        self.look();
        self.look_apply();
        if let Some(status) = self.ended.take() {
            self.finish(status);
        }
    }

    /// The part of `poll` that needs no drawing, so that it also runs while the window is hidden: a hidden window
    /// is not drawn, but its run goes on and its end must be noticed. An end found
    /// here is taken by `tick` (hidden) or by the next `poll` (drawn).
    fn look(&mut self) {
        if self.ended.is_some() {
            return;
        }
        let Some(run) = self.running.as_mut() else { return };
        // The lines first, so that the last ones are kept before the end is taken.
        while let Ok(line) = run.lines.try_recv() {
            if let Some(log) = run.log.as_mut() {
                let _ = writeln!(log, "{line}");
            }
            if let Some(follow) = self.state.follow.as_mut() {
                follow.hear(&line);
            }
        }
        if run.thread.as_ref().is_none_or(|t| t.is_finished()) {
            self.ended = Some(match run.thread.take().map(JoinHandle::join) {
                Some(Ok(result)) => result,
                Some(Err(_)) => Err("the engine panicked".to_string()),
                None => Err("the engine's thread was lost".to_string()),
            });
            return;
        }
        let (Some(follow), Some(runs)) = (self.state.follow.as_mut(), self.runs.as_ref()) else { return };
        follow.seconds = run.started.elapsed().as_secs();
        if run.looked.elapsed() >= Duration::from_secs(1) {
            let before = follow.pass_key.clone();
            follow.look(run::read_progress(&runs.out()).as_ref());
            run.looked = Instant::now();
            if follow.pass_key != before {
                observe!(Debug, "run pass '{}' at {} s", follow.pass_key, follow.seconds);
            }
            self.draw_run();
        }
    }

    /// The generator ended (`follow`'s end, lines 2664-2696): a stopped run leaves the map it found, a failed one
    /// says so, a finished one is read and shown, and the pace learns from it.
    /// It needs no device, so it also runs while the window is hidden: the new map's meshes are built by the frame
    /// that next shows the 3D view.
    fn finish(&mut self, status: Result<PathBuf, String>) {
        let (Some(run), Some(mut follow)) = (self.running.take(), self.state.follow.take()) else { return };
        let Some(runs) = self.runs.as_ref() else { return };
        let (out, pace_file, before_file, log_file) = (runs.out(), runs.pace(), runs.before(), runs.log());
        let seconds = run.started.elapsed().as_secs();
        follow.seconds = seconds;
        let mut finished = false;
        if follow.stopping {
            self.state.trouble.clear();
            self.state.said = text::STOPPED.to_string();
        } else if status.is_ok() && out.join("preview.json").exists() {
            match World::load(&out) {
                Ok(world) => {
                    // The camera starts again when the area's width changed (line 1873).
                    if self.world.as_ref().map(World::span) != Some(world.span()) {
                        self.state.orbit = None;
                    }
                    self.meshed = None;
                    // The figures of the map being replaced are the run before (lines 2656-2658).
                    if let Some(old) = self.world.as_ref().map(|w| w.measured.figures) {
                        self.state.before = Some(old);
                        if let Err(e) = std::fs::write(&before_file, old.to_json()) {
                            observe!(Warn, "the figures of the run before could not be kept: {e}");
                        }
                    }
                    self.world = Some(world);
                    self.painted = None;
                    self.nav_painted = None;
                    self.cut_painted = None;
                    self.shown = None;
                    self.last_at = None;
                    self.last_sheet = None;
                    // `loadWorld` puts the run's settings back in the form.
                    self.state.made = self.world.as_ref().and_then(run::ask_of);
                    if let Some(made) = self.state.made.clone() {
                        run::adopt(&mut self.state, &made);
                    }
                    let last = if follow.stage == text::STEP_START { text::step_last(follow.ask.detail) } else { follow.stage.clone() };
                    follow.done.push((last, seconds));
                    self.state.trouble.clear();
                    self.state.said = text::done(&run::duration(seconds));
                    self.pace.learn(follow.ask.samples, follow.ask.detail, seconds as f64, run::work_seconds(&out));
                    if let Err(e) = self.pace.save(&pace_file) {
                        observe!(Warn, "the pace could not be kept: {e}");
                    }
                    self.state.pace = self.pace.clone();
                    // Line 2661: the new map is kept in the history.
                    self.remember();
                    finished = true;
                    self.state.ended = RunEnd::Ready;
                }
                Err(e) => {
                    observe!(Warn, "the run's result could not be read: {e}");
                    self.state.trouble = text::FAILED.to_string();
                }
            }
        } else {
            observe!(Warn, "the run failed: {} (see {})", status.err().unwrap_or_else(|| "no result".to_string()), log_file.display());
            self.state.trouble = text::FAILED.to_string();
            self.state.said.clear();
        }
        // `tell(finished ? 'ready' : trouble ? 'failed' : 'idle')` (line 2673).
        if !finished {
            self.state.ended = if !self.state.trouble.is_empty() { RunEnd::Failed } else { RunEnd::Idle };
        }
        // The run's picture leaves the stage: a finished run's map took its place above; a stopped or failed one
        // leaves the map it found (line 2670), drawn again.
        if let Some(drawn) = self.drawn.take() {
            if !finished {
                if self.world.as_ref().map(World::span) != Some(drawn.span()) {
                    self.state.orbit = None;
                }
                self.restage();
            }
        }
        // A run that ends while another tool is on screen has not been seen (the launcher's line 881).
        if self.state.mode != Mode::Preview && self.state.ended != RunEnd::Idle {
            self.state.preview_seen = false;
        }
        // The steps stay, to see where the time went; the one that did not end is left out.
        self.state.steps = Steps::Run { done: follow.done.clone(), now: String::new(), share: if finished { 1.0 } else { follow.share() } };
        observe!(Info, "run ended after {seconds} s: {}", if finished { "finished" } else if follow.stopping { "stopped" } else { "failed" });
        // A run asked for while this one was under way starts now that it has stopped (line 2696).
        if std::mem::take(&mut self.queued) {
            self.start();
        }
    }

    /// Acts on a press the shell recorded.
    fn act(&mut self, want: Want) {
        match want {
            Want::Generate => {
                if let Some(follow) = &self.state.follow {
                    // The run under way is the one on screen: the button only says so (line 2721). A setting moved:
                    // the new run waits for this one to stop, then starts by itself.
                    if !follow.ask.same_as(&self.state.seed, self.state.samples, DETAIL_BLOCKS[self.state.detail], &self.state.values) {
                        self.queued = true;
                        self.stop();
                    }
                } else {
                    self.start();
                }
            }
            Want::Stop => self.stop(),
            Want::Minimize => self.requests.push_back(WindowRequest::Minimize),
            Want::Maximize => self.requests.push_back(WindowRequest::ToggleMaximize),
            Want::Close => self.requests.push_back(WindowRequest::Close),
            Want::Export => self.export(),
            Want::Apply => self.apply(),
            Want::OpenFolder => {
                if let (Steps::Saved { folder }, Some(runs)) = (&self.state.steps, &self.runs) {
                    if let Err(e) = runs.launcher.open_folder(&PathBuf::from(folder)) {
                        observe!(Warn, "the folder could not be opened: {e}");
                        self.state.trouble = text::OPEN_FAILED.to_string();
                    }
                }
            }
        }
    }

    /// `generate()` (lines 2699-2710): an empty seed is one drawn here, a seed the server would refuse is refused here,
    /// then the generator starts with its output in the runs' folder.
    fn start(&mut self) {
        let Some(engine_dir) = self.runs.as_ref().and_then(|r| r.engine_dir.clone()) else {
            observe!(Warn, "no world-preview repository given (--engine): a run cannot start");
            self.state.trouble = text::FAILED.to_string();
            return;
        };
        let runs = self.runs.as_ref().expect("an engine comes from the runs");
        if self.state.seed.trim().is_empty() {
            self.state.seed = run::random_seed();
        }
        let ask = self.asked();
        self.state.seed = ask.seed.clone();
        if !run::seed_ok(&ask.seed) {
            self.state.trouble = text::REFUSED.to_string();
            self.state.said.clear();
            return;
        }
        let (out, server) = (runs.out(), runs.server());
        // As the server does before it takes a run (lines ~300-335): the sketch and the progress of the run before.
        for name in ["coarse.json", "progress.json", "partial.json"] {
            let _ = std::fs::remove_file(out.join(name));
        }
        static RUNS: AtomicU64 = AtomicU64::new(1);
        let stop = Arc::new(AtomicBool::new(false));
        let options = run::options(&engine_dir, runs.pack.as_deref(), &server, &out, &runs.mods, &ask, stop.clone());
        let engine = runs.engine.clone();
        let (tx, lines) = mpsc::channel::<String>();
        let started = (|| -> std::io::Result<(File, JoinHandle<Result<PathBuf, String>>)> {
            std::fs::create_dir_all(&out)?;
            std::fs::create_dir_all(&server)?;
            let log = File::create(runs.log())?;
            let thread = std::thread::Builder::new().name("world-preview run".into()).spawn(move || {
                let result = engine(&options, &mut |line| {
                    let _ = tx.send(line.to_string());
                });
                if let Err(e) = &result {
                    let _ = tx.send(e.clone());
                }
                result
            })?;
            Ok((log, thread))
        })();
        match started {
            Ok((log, thread)) => {
                let id = RUNS.fetch_add(1, Ordering::Relaxed);
                observe!(Info, "run {id} started: seed {}, samples {}, detail {}, out {}", ask.seed, ask.samples, ask.detail, out.display());
                let expected = self.pace.about(ask.samples, ask.detail);
                self.state.trouble.clear();
                self.state.said.clear();
                self.state.follow = Some(Follow::new(ask, expected));
                self.state.steps = Steps::Run { done: Vec::new(), now: text::STEP_START.to_string(), share: 0.0 };
                let now = Instant::now();
                self.running = Some(Running { thread: Some(thread), stop, lines, log: Some(log), id, started: now, looked: now });
            }
            Err(e) => {
                observe!(Warn, "the engine could not be started: {e}");
                self.state.trouble = text::FAILED.to_string();
            }
        }
    }

    /// Apply to pack: the write goes to a thread of its own, since looking at the machine takes a moment. The values
    /// are kept with it, for they become the pack's if it works.
    fn apply(&mut self) {
        let (Some(runs), Some(host)) = (self.runs.as_ref(), self.apply_host.clone()) else { return };
        let (Some(pack), None) = (runs.pack.clone(), self.applying.as_ref()) else { return };
        let target = Target { pack, own_server: Some(runs.server()) };
        let values = self.state.values;
        self.state.apply = Notice::Working;
        self.applying = Some((std::thread::spawn(move || apply::apply(&*host, &target, &values)), values));
    }

    /// The write ended: its words, and the pack's values become the ones written.
    fn look_apply(&mut self) -> bool {
        if !self.applying.as_ref().is_some_and(|(t, _)| t.is_finished()) {
            return false;
        }
        let Some((thread, values)) = self.applying.take() else { return false };
        match thread.join() {
            Ok(Outcome::Applied(_, running)) => {
                self.state.baseline = values;
                self.state.apply = Notice::Done { running };
            }
            Ok(Outcome::Failed(f)) => self.state.apply = Notice::Failed(f),
            Err(_) => self.state.apply = Notice::Failed(Fail::Write),
        }
        true
    }

    /// The square beside the button (lines 2712-2715): the run ends a moment later and the wait for it says so.
    fn stop(&mut self) {
        let (Some(run), Some(follow)) = (self.running.as_ref(), self.state.follow.as_mut()) else { return };
        follow.stopping = true;
        run.stop.store(true, Ordering::Relaxed);
    }

    /// Export (lines 2736-2796), the 2D pictures only: the whole map of each view whatever is chosen, then the cut as
    /// it was last looked at. Synchronous, so the page's "Pictures: view" steps are not shown one by one.
    fn export(&mut self) {
        if self.state.follow.is_some() {
            return;
        }
        let (Some(world), Some(runs)) = (self.world.as_ref(), self.runs.as_ref()) else {
            if self.world.is_some() {
                observe!(Warn, "no --runs: an export has nowhere to go");
                self.state.trouble = text::EXPORT_FAILED.to_string();
            }
            return;
        };
        self.state.trouble.clear();
        self.state.said.clear();
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let folder = runs.exports().join(run::folder_name(self.state.made.as_ref().map(|m| m.seed.as_str()), secs));
        // A preview stopped at a coarser detail has no level land to give; structures need the marks' rasteriser.
        let kinds: Vec<View> = [View::Biomes, View::Flat, View::Slope, View::Cliffs, View::Vegetation, View::Height, View::Temperature]
            .into_iter()
            .filter(|k| !(k.needs_full() && world.coarse()))
            .collect();
        let n = world.samples as u32;
        let mut steps = Vec::new();
        let result = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(&folder)?;
            for kind in &kinds {
                let rgba = paint(world, *kind, self.dark, &BTreeSet::new());
                std::fs::write(folder.join(format!("{}.png", kind.key())), mere_png::encode(n, n, &rgba))?;
                steps.push((text::export_step(kind.key()), 0));
            }
            if let Some((w, h, rgba)) = &self.last_sheet {
                std::fs::write(folder.join("section.png"), mere_png::encode(*w, *h, rgba))?;
            }
            // The settings and figures come last, as the page's last step; the pictures are already kept if this fails.
            let (seed, values) = self.state.made.as_ref().map_or((self.state.seed.as_str(), &self.state.values), |m| (m.seed.as_str(), &m.values));
            let ores = self.shown.as_ref().map(|s| &s.play.per_chunk);
            let data = crate::export_data::export_json(world, seed, values, &kinds, self.state.before.as_ref(), ores);
            std::fs::write(folder.join("export.json"), data)?;
            steps.push((text::EXPORT_DATA.to_string(), 0));
            Ok(())
        })();
        match result {
            Ok(()) => {
                observe!(Info, "exported {} pictures to {}", kinds.len() + usize::from(self.last_sheet.is_some()), folder.display());
                self.state.said = text::EXPORTED.to_string();
                self.state.steps = Steps::Saved { folder: folder.display().to_string() };
            }
            Err(e) => {
                observe!(Warn, "the export failed: {e}");
                self.state.trouble = text::EXPORT_FAILED.to_string();
                let share = steps.len() as f32 / (kinds.len() + 1) as f32;
                self.state.steps = Steps::Run { done: steps, now: String::new(), share };
            }
        }
    }
}

impl App for PreviewApp {    fn start(&mut self, gfx: &mut dyn Gfx) {
        self.painted = None;
        // A rebuilt device has lost the cut's pictures as well.
        self.nav_painted = None;
        self.cut_painted = None;
        // Also after a device rebuild, which has lost the meshes: they are built again when the 3D view shows.
        let _ = gfx;
        self.meshed = None;
        self.sided = None;
    }

    fn event(&mut self, e: Event) -> bool {
        if let Event::Resized { width_px, height_px, scale } = e {
            self.size = raise_to_min(Vec2::new(width_px as f32 / scale, height_px as f32 / scale));
            return true;
        }
        if let Event::Window { maximized } = e {
            let changed = self.state.maximized != maximized;
            self.state.maximized = maximized;
            return changed;
        }
        if let Event::Scheme { dark } = e {
            observe!(Info, "the system is now {}{}", if dark { "dark" } else { "light" }, if self.follows_scheme { "" } else { " (not followed)" });
            if self.follows_scheme && dark != self.dark {
                self.set_dark(dark);
                return true;
            }
            return false;
        }
        if crate::shell::help_escape(&mut self.state, &self.ui, &e) {
            return true;
        }
        self.ui.event(&e)
    }

    fn frame(&mut self, gfx: &mut dyn Gfx, seconds: f32) -> bool {
        self.poll();
        let clock = self.now();
        self.ui.set_clock(clock);
        // What came back since the last frame, before the page is built from it.
        let mut home_wants = false;
        let at_home = self.state.mode == Mode::Home;
        if let Some(h) = self.home.as_mut() {
            home_wants = h.poll(clock, at_home);
            self.state.account = h.account();
        }
        // Before building, so the frame shows the settled pose.
        let moving = match (&mut self.state.orbit, self.drawn.as_ref().or(self.world.as_ref())) {
            (Some(orbit), Some(world)) => scene::settle(orbit, world, self.state.level, seconds),
            _ => false,
        };
        gfx.begin(self.theme.ground);
        // The scrollbar's gutter comes a frame late, and a click changes the state after its widget was
        // built: build again until neither is pending, at most three times, so a picture shows the settled page.
        let mut list = DrawList::default();
        // A press the shell recorded is acted on here, after its build, and the page is built again to show it.
        for _ in 0..4 {
            let before = self.state.clone();
            self.sync_cut();
            self.ui.begin_frame(self.size);
            let stage = self.drawn.as_ref().or(self.world.as_ref());
            self.ids = build_run(&mut self.ui, &mut self.state, self.size, self.world.as_ref(), stage, self.shown.as_ref(), self.home.as_mut().map(|h| &mut **h as &mut dyn Home));
            list = self.ui.end_frame().clone();
            if let Some(want) = self.state.want.take() {
                self.act(want);
            }
            // What Home recorded in this build is acted on now, and the page built again to show it.
            let mut acted = false;
            if let Some(h) = self.home.as_mut() {
                if h.act() {
                    home_wants |= h.poll(clock, self.state.mode == Mode::Home);
                    acted = true;
                }
                // Switching to Home asks for what it shows (the page's showMode).
                if before.mode != Mode::Home && self.state.mode == Mode::Home {
                    h.shown();
                    home_wants |= h.poll(clock, true);
                    acted = true;
                }
                self.state.account = h.account();
            }
            if !self.ui.want_redraw() && self.state == before && !acted {
                break;
            }
        }
        // A value that moved is kept at once: a window closed without a run still has it at its next start.
        if let (Some(runs), Some(saved)) = (&self.runs, self.draft_saved) {
            if saved != self.state.values {
                if let Err(e) = crate::draft::save(&runs.draft(), &self.state.values) {
                    observe!(Warn, "draft: {}: {e}", runs.draft().display());
                }
                self.draft_saved = Some(self.state.values);
            }
        }
        // Before the list is drawn, so that the first frame already shows the picture.
        if let (Some(world), Some(key)) = (self.stage(), self.wanted()) {
            if self.painted.as_ref() != Some(&key) {
                let started = Instant::now();
                let rgba = paint(world, key.0, key.1, &key.2);
                let n = world.samples as u32;
                gfx.upload_image(MAP_IMAGE, n, n, &rgba);

                // The walls' colour follows the theme, as the drape does.
                scene::upload_wall(gfx, self.theme.ground);
                observe!(
                    Info,
                    "painted {} {} choice {:?} {n}x{n} in {} ms",
                    key.0.key(),
                    if key.1 { "dark" } else { "light" },
                    key.2,
                    started.elapsed().as_millis()
                );
                self.painted = Some(key);
                // The columns' sides are the map's colours, shaded: they follow the picture, once the 3D view shows.
                self.drape = rgba;
                self.sided = None;
            }
        }
        self.upload_cut(gfx);
        // The meshes come before the list: the interface is drawn over the scene, and the viewport clips it to the
        // square.
        if let (Some(rect), Some(orbit), Some((view, ..))) = (self.ui.rect(self.ids.relief).filter(|_| self.ids.relief != Id::ROOT), &self.state.orbit, &self.painted) {
            let level = self.state.level;
            // The 3D view shows: the device gets the meshes of the level asked for, when it has not got them yet.
            if let Some(world) = self.drawn.as_ref().or(self.world.as_ref()) {
                if self.meshed != Some(level) {
                    let started = Instant::now();
                    scene::upload(gfx, world, level);
                    observe!(Info, "3D relief as {}: meshes built and sent in {} ms", level.key(), started.elapsed().as_millis());
                    self.meshed = Some(level);
                }
                if self.sided != Some(level) {
                    scene::upload_sides(gfx, world, level, &self.drape);
                    self.sided = Some(level);
                }
            }
            scene::draw(gfx, rect, orbit, level, MAP_IMAGE, scene::water_alpha(*view));
        }
        gfx.draw_list(&list);
        self.last = list;
        gfx.end();
        // A run under way no longer asks for frames at full speed: `wake_in` says when the clock next changes.
        // The tools' slide asks for frames only while it moves.
        let sliding = self.state.options.moving(self.ui.clock(), self.state.reduce_motion);
        self.ui.want_redraw() || moving || sliding || home_wants
    }

    fn wake_in(&self) -> Option<f32> {
        // The status shows whole seconds and the end of the run is looked for four times a second.
        let run = (self.running.is_some() || self.applying.is_some()).then_some(0.25);
        let home = self.home.as_ref().and_then(|h| h.wake_in(self.now()));
        [self.ui.wake_in(), run, home].into_iter().flatten().reduce(f32::min)
    }

    fn tick(&mut self) -> bool {
        self.look();
        let applied = self.look_apply();
        // The end is told at once, title included: no frame comes while the window is hidden. Only the meshes wait.
        let ended = match self.ended.take() {
            Some(status) => {
                self.finish(status);
                true
            }
            None => false,
        };
        let clock = self.now();
        let home = self.home.as_mut().is_some_and(|h| h.poll(clock, false));
        ended || home || applied
    }

    fn hit(&self, pos: Vec2) -> Hit {
        chrome_hit(&self.ui, pos, self.size, self.state.maximized, self.ids.bar, &self.ids.bar_buttons)
    }

    fn ime_spot(&self) -> Option<Rect> {
        self.ui.ime_spot()
    }

    fn cursor(&self) -> Cursor {
        self.ui.cursor()
    }

    fn take_request(&mut self) -> Option<WindowRequest> {
        // The title follows the marker (renderModes, line 794); the first frame sets it too.
        let title = crate::bar::window_title(&self.state);
        if self.title_sent.as_deref() != Some(title.as_str()) {
            self.title_sent = Some(title.clone());
            self.requests.push_back(WindowRequest::Title(title));
        }
        self.requests.pop_front()
    }
}

impl Drop for PreviewApp {
    /// Quitting stops the run under way and waits for its end, which kills the game server: the engine's job object
    /// would end it with the process anyway, but a host that keeps running (a test) must not keep one behind.
    fn drop(&mut self) {
        if let Some(mut run) = self.running.take() {
            run.stop.store(true, Ordering::Relaxed);
            if let Some(thread) = run.thread.take() {
                let _ = thread.join();
            }
        }
    }
}
