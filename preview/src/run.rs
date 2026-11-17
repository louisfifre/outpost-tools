//! A run, as the page's `follow()`, `generate()` and `learnPace()` know it (preview.html 767-786, 2585-2745),
//! kept apart from the window: what was asked, how the generator is started, what its progress file says, how
//! long a run should take, and the name of an export's folder. Everything here is pure or touches the files it is
//! given, so unit tests cover it without a game or a window.

use std::collections::BTreeMap;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use mere_base::Launcher;

use crate::json::Json;
use crate::settings::{Kind, SETTINGS};
use crate::shell::PreviewState;
use crate::text;
use crate::world::World;

pub use crate::text::duration;

/// The detail tabs' blocks, in `text::DETAILS` order.
pub const DETAIL_BLOCKS: [u32; 3] = [64, 32, 16];

/// What a run was asked for, or what a map was made from: the page's `run` and `shown` objects.
#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    pub seed: String,
    /// Points on a side of the scanned area, in 16-block points: what the area select holds.
    pub samples: u32,
    /// Blocks between two points.
    pub detail: u32,
    /// The Tectonic values, in `SETTINGS` order.
    pub values: [f64; 43],
}

impl Ask {
    /// `sameAs` (line 785): the form's seed (trimmed), area, detail and every value, to 1e-9.
    pub fn same_as(&self, seed: &str, samples: u32, detail: u32, values: &[f64; 43]) -> bool {
        seed.trim() == self.seed && samples == self.samples && detail == self.detail && self.values.iter().zip(values).all(|(a, b)| (a - b).abs() < 1e-9)
    }
}

/// What a loaded world was made from (`loadWorld`, lines 1150-1163): `run.json`'s seed and values, the pack's for
/// a key it does not list. None without a run file.
pub fn ask_of(world: &World) -> Option<Ask> {
    let made = world.made.as_ref()?;
    let mut values = [0.0; 43];
    for (i, set) in SETTINGS.iter().enumerate() {
        values[i] = made.tectonic.iter().find(|(k, _)| k == set.key).map_or(set.pack, |(_, v)| *v);
    }
    Some(Ask {
        seed: made.seed.clone(),
        samples: (world.samples as f64 * world.step / 16.0) as u32,
        detail: world.step as u32,
        values,
    })
}

/// A loaded world puts its run's settings back in the form (`loadWorld`).
pub fn adopt(state: &mut PreviewState, ask: &Ask) {
    state.seed = ask.seed.clone();
    state.values = ask.values;
    state.detail = DETAIL_BLOCKS.iter().position(|&d| d == ask.detail).unwrap_or(2);
    state.samples = ask.samples;
}

/// serve.ps1's `^[\w -]{1,64}$`. .NET's `\w` also takes the letters of every script; this takes ASCII only, a
/// stricter subset (a seed in another script is refused here and accepted there).
pub fn seed_ok(seed: &str) -> bool {
    let n = seed.chars().count();
    (1..=64).contains(&n) && seed.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ' ' | '-'))
}

/// An empty seed is one drawn here (line 2702): `a * 2^20 + b % 2^20` with two random 32-bit numbers, which
/// stays under 2^52, a whole number for a double as well.
pub fn random_seed() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let draw = || {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(nanos);
        h.finish() as u32
    };
    let (a, b) = (draw(), draw());
    (a as u64 * (1 << 20) + (b as u64 % (1 << 20))).to_string()
}

/// What the engine is asked for one run: the engine's repository, the folders, the form and the Tectonic values,
/// the switches as booleans. Without `pack` the engine reads WORLD_PREVIEW_PACK, without `mods` WORLD_PREVIEW_MODS.
/// The engine's points are 16 blocks apart and the area is counted in them, as the form counts it.
pub fn options(engine: &Path, pack: Option<&Path>, dir: &Path, out: &Path, mods: &[PathBuf], ask: &Ask, stop: Arc<AtomicBool>) -> world_preview::Options {
    let mut o = world_preview::Options::new(engine.to_path_buf());
    o.pack = pack.map(Path::to_path_buf);
    o.dir = Some(dir.to_path_buf());
    o.out = Some(out.to_path_buf());
    o.seed = ask.seed.clone();
    o.samples = ask.samples;
    o.step = 16;
    o.detail = Some(ask.detail);
    o.mods = mods.to_vec();
    o.tectonic = SETTINGS
        .iter()
        .zip(ask.values)
        .map(|(set, v)| {
            let value = match set.kind {
                Kind::Switch => world_preview::Value::Bool(v != 0.0),
                Kind::Number(_) => world_preview::Value::from(v),
            };
            (set.key.to_string(), value)
        })
        .collect();
    o.stop = Some(stop);
    o
}

/// One run of the engine: `world_preview::run`, or a stand-in for tests. It is called on a thread of its own and
/// sends its progress lines to `log` as they come.
pub type Engine = Arc<dyn Fn(&world_preview::Options, &mut dyn FnMut(&str)) -> Result<PathBuf, String> + Send + Sync>;

/// The real engine.
pub fn engine() -> Engine {
    Arc::new(|o, log| world_preview::run(o, log).map_err(|e| e.to_string()))
}

/// `progress.json` (Progress.java): the pass under way, its step in blocks and how many tiles are done of the total.
#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub pass: String,
    pub step: u32,
    pub done: u32,
    pub total: u32,
}

/// None for any failure, as the page's `.catch(() => null)`: the file is absent until the game has started, and
/// the mod replaces it atomically, so a half-written one is rare but is the same as none.
pub fn read_progress(out: &Path) -> Option<Progress> {
    let text = std::fs::read_to_string(out.join("progress.json")).ok()?;
    let json = Json::parse(&text).ok()?;
    let num = |k: &str| json.get(k).and_then(Json::num).filter(|v| *v >= 0.0).map(|v| v as u32);
    Some(Progress { pass: json.get("pass")?.text()?, step: num("step")?, done: num("done")?, total: num("total")? })
}

/// `timing.json`: the seconds the mod itself worked (`workMillis`).
pub fn work_seconds(out: &Path) -> Option<f64> {
    let text = std::fs::read_to_string(out.join("timing.json")).ok()?;
    Some(Json::parse(&text).ok()?.get("workMillis")?.num()? / 1000.0)
}

/// What the page tracks while a run is followed (`follow`, lines 2606-2648).
#[derive(Clone, Debug, PartialEq)]
pub struct Follow {
    pub ask: Ask,
    /// The steps done, each with the second it ended at.
    pub done: Vec<(String, u64)>,
    /// The step under way.
    pub stage: String,
    /// How far the step is, in percent, when it counts tiles.
    pub part: Option<u32>,
    pub pass_key: String,
    pub seconds: u64,
    /// The wait this machine took last time at this detail; None on a first run (it is being measured).
    pub expected: Option<u64>,
    /// The stop was pressed: the button greys until the run ends.
    pub stopping: bool,
    /// The engine's last line (a build, a download, the pack's sync): shown beside the first step, the long one
    /// whose progress the mod cannot count yet.
    pub said: String,
}

impl Follow {
    pub fn new(ask: Ask, expected: Option<u64>) -> Follow {
        Follow { ask, done: Vec::new(), stage: text::STEP_START.to_string(), part: None, pass_key: String::new(), seconds: 0, expected, stopping: false, said: String::new() }
    }

    /// A line of the engine, as it comes.
    pub fn hear(&mut self, line: &str) {
        let line = line.trim();
        if !line.is_empty() {
            self.said = line.to_string();
        }
    }

    /// One look at the progress file: a new pass closes the step before it.
    pub fn look(&mut self, p: Option<&Progress>) {
        if let Some(p) = p {
            let key = format!("{} {}", p.pass, p.step);
            if key != self.pass_key {
                self.pass_key = key;
                self.done.push((std::mem::take(&mut self.stage), self.seconds));
                self.stage = match p.pass.as_str() {
                    "sketch" => text::step_sketch(p.step),
                    "map" => text::step_last(p.step),
                    _ => text::STEP_EXTRAS.to_string(),
                };
            }
        }
        self.part = p.filter(|p| p.total > 0).map(|p| (p.done as u64 * 100 / p.total as u64) as u32);
    }

    /// The hairline and the button's fill: the clock against the wait, never quite whole before the end.
    pub fn share(&self) -> f32 {
        self.expected.map_or(0.0, |e| (self.seconds as f32 / e as f32).min(0.97))
    }

    /// The step under way as the line shows it.
    pub fn current(&self) -> String {
        match self.part {
            Some(p) => format!("{} \u{b7} {p} %", self.stage),
            None if self.pass_key.is_empty() && !self.said.is_empty() => format!("{} \u{b7} {}", self.stage, self.said),
            None => self.stage.clone(),
        }
    }
}

/// `#steps` (lines 263-271, 2563-2581, 2798-): empty, a run's steps with its hairline, or an export's notice.
#[derive(Clone, Debug, PartialEq)]
pub enum Steps {
    Empty,
    Run { done: Vec<(String, u64)>, now: String, share: f32 },
    Saved { folder: String },
}

/// What this machine measured (the page's `timing`, lines 767-781, kept in the page's localStorage; here in a file).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pace {
    /// Seconds a run takes besides the mod's work: the game's start and stop.
    pub fixed: Option<f64>,
    /// Per detail: the area (in points) and the seconds the mod worked on it.
    pub work: BTreeMap<u32, (u32, f64)>,
}

/// `Math.round`: halves go up.
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

impl Pace {
    /// `about` (lines 772-776): the work grows with the square of the area, the game's part does not.
    pub fn about(&self, span: u32, d: u32) -> Option<u64> {
        let fixed = self.fixed?;
        let &(work_span, seconds) = self.work.get(&d)?;
        let ratio = span as f64 / work_span as f64;
        let wait = js_round((fixed + seconds * ratio * ratio) / 5.0) * 5.0;
        Some(wait.max(5.0) as u64)
    }

    /// `learnPace` (lines 777-782): a run without a timing, or one that claims more work than its whole, teaches nothing.
    pub fn learn(&mut self, span: u32, d: u32, seconds: f64, work_seconds: Option<f64>) {
        let Some(work) = work_seconds.filter(|w| *w >= 0.0 && *w <= seconds) else { return };
        self.fixed = Some(seconds - work);
        self.work.insert(d, (span, work));
    }

    /// A missing or unreadable file is a machine that measured nothing yet.
    pub fn load(path: &Path) -> Pace {
        let parse = || -> Option<Pace> {
            let json = Json::parse(&std::fs::read_to_string(path).ok()?).ok()?;
            let fixed = json.get("fixed").and_then(Json::num);
            let mut work = BTreeMap::new();
            if let Some(Json::Obj(fields)) = json.get("work") {
                for (k, v) in fields {
                    let (Ok(d), Some(span), Some(seconds)) = (k.parse::<u32>(), v.get("span").and_then(Json::num), v.get("seconds").and_then(Json::num)) else { continue };
                    work.insert(d, (span as u32, seconds));
                }
            }
            Some(Pace { fixed, work })
        };
        parse().unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let fixed = self.fixed.map_or("null".to_string(), |f| f.to_string());
        let work: Vec<String> = self.work.iter().map(|(d, (span, seconds))| format!("\"{d}\":{{\"span\":{span},\"seconds\":{seconds}}}")).collect();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, format!("{{\"fixed\":{fixed},\"work\":{{{}}}}}", work.join(",")))
    }
}

/// The name of an export's folder (line 2739): the seed (or "preview") and the stamp, every char outside letters,
/// digits, `_` and `-` replaced by `_`. The stamp is UTC (std has no time zone); the page's is local time:
/// a known difference.
pub fn folder_name(seed: Option<&str>, unix_seconds: u64) -> String {
    let days = (unix_seconds / 86_400) as i64;
    let rest = unix_seconds % 86_400;
    // Days to civil date (Howard Hinnant's algorithm), the era being 400 years of 146 097 days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let stamp = format!("{year:04}-{month:02}-{day:02}_{:02}-{:02}-{:02}", rest / 3_600, rest % 3_600 / 60, rest % 60);
    format!("{}_{stamp}", seed.unwrap_or("preview")).chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

/// Where and how runs happen: the engine and its repository, the pack, and the folder under which everything a run
/// or an export writes goes.
pub struct Runs {
    /// The world-preview repository (its `mod/` is built and given to the server). None: Generate says it failed.
    pub engine_dir: Option<PathBuf>,
    pub engine: Engine,
    pub pack: Option<PathBuf>,
    pub root: PathBuf,
    /// Jars added to the server on top of the pack's mods.
    pub mods: Vec<PathBuf>,
    pub launcher: Box<dyn Launcher>,
    /// Folders named one by one, for a host that already has them (the launcher gives serve.ps1 its own);
    /// None puts them under `root`. The pace, the log, the history and `before` always stay under `root`.
    pub server_dir: Option<PathBuf>,
    pub out_dir: Option<PathBuf>,
    pub exports_dir: Option<PathBuf>,
}

impl Runs {
    /// The generator's output folder.
    pub fn out(&self) -> PathBuf {
        self.out_dir.clone().unwrap_or_else(|| self.root.join("out"))
    }
    /// The generator's working folder (the game server's).
    pub fn server(&self) -> PathBuf {
        self.server_dir.clone().unwrap_or_else(|| self.root.join("server"))
    }
    /// The learned pace.
    pub fn pace(&self) -> PathBuf {
        self.root.join("pace.json")
    }
    pub fn exports(&self) -> PathBuf {
        self.exports_dir.clone().unwrap_or_else(|| self.root.join("exports"))
    }
    /// What the engine said.
    pub fn log(&self) -> PathBuf {
        self.root.join("run.log")
    }
    /// The page's localStorage `preview.history`, as a file of the same JSON.
    pub fn history(&self) -> PathBuf {
        self.root.join("history.json")
    }
    /// The form's values as last typed (`draft.rs`).
    pub fn draft(&self) -> PathBuf {
        self.root.join("draft.json")
    }
    /// The page's localStorage `preview.before`: the figures of the run before the one on screen.
    pub fn before(&self) -> PathBuf {
        self.root.join("before.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::pack_values;
    use std::process::Command;

    fn ask() -> Ask {
        Ask { seed: "cap 1".to_string(), samples: 256, detail: 16, values: pack_values() }
    }

    fn stop() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn u1_the_options_carry_the_form() {
        use world_preview::Value;
        let o = options(Path::new(r"C:\wp"), None, Path::new(r"C:\w\server"), Path::new(r"C:\w\out"), &[], &ask(), stop());
        assert_eq!((o.repo.as_path(), o.dir.as_deref(), o.out.as_deref()), (Path::new(r"C:\wp"), Some(Path::new(r"C:\w\server")), Some(Path::new(r"C:\w\out"))));
        assert_eq!((o.seed.as_str(), o.samples, o.step, o.detail, o.pack.as_ref()), ("cap 1", 256, 16, Some(16), None));
        assert_eq!(o.tectonic.len(), 43);
        let get = |k: &str| o.tectonic.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone()).unwrap();
        assert_eq!((get("rolling_hills"), get("ultrasmooth")), (Value::Bool(true), Value::Bool(false)));
        assert_eq!((get("vertical_scale").as_f64(), get("min_y").as_f64()), (Some(1.5), Some(-64.0)));
        assert!(o.stop.is_some());
        let o = options(Path::new("e"), Some(Path::new(r"D:\pack")), Path::new("d"), Path::new("o"), &[PathBuf::from("glue.jar")], &ask(), stop());
        assert_eq!((o.pack.as_deref(), o.mods.as_slice()), (Some(Path::new(r"D:\pack")), [PathBuf::from("glue.jar")].as_slice()));
    }

    struct Nothing;
    impl Launcher for Nothing {
        fn spawn(&self, _: &mut Command) -> io::Result<std::process::Child> {
            Err(io::Error::other("unused"))
        }
        fn stop(&self, _: &mut std::process::Child) -> io::Result<()> {
            Ok(())
        }
        fn open_folder(&self, _: &Path) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn u1b_named_folders_win_over_the_root() {
        let mut r = Runs { engine_dir: Some(PathBuf::from("wp")), engine: engine(), pack: None, root: PathBuf::from(r"C:\r"), mods: Vec::new(), launcher: Box::new(Nothing), server_dir: None, out_dir: None, exports_dir: None };
        assert_eq!((r.server(), r.out(), r.exports()), (PathBuf::from(r"C:\r\server"), PathBuf::from(r"C:\r\out"), PathBuf::from(r"C:\r\exports")));
        r.server_dir = Some(PathBuf::from(r"D:\s"));
        r.out_dir = Some(PathBuf::from(r"D:\o"));
        r.exports_dir = Some(PathBuf::from(r"D:\e"));
        assert_eq!((r.server(), r.out(), r.exports(), r.pace()), (PathBuf::from(r"D:\s"), PathBuf::from(r"D:\o"), PathBuf::from(r"D:\e"), PathBuf::from(r"C:\r\pace.json")));
        let o = options(r.engine_dir.as_ref().unwrap(), None, &r.server(), &r.out(), &r.mods, &ask(), stop());
        assert_eq!((o.dir.as_deref(), o.out.as_deref()), (Some(Path::new(r"D:\s")), Some(Path::new(r"D:\o"))));
    }

    #[test]
    fn u5b_the_engine_speaks_beside_the_first_step() {
        let mut f = Follow::new(ask(), None);
        assert_eq!(f.current(), "Starting the game");
        f.hear("Syncing the pack\r\n");
        f.hear("  ");
        assert_eq!(f.current(), "Starting the game \u{b7} Syncing the pack");
        f.look(Some(&progress("map", 16, 1, 10)));
        assert_eq!(f.current(), "Map, every 16 blocks \u{b7} 10 %");
        f.look(Some(&progress("extras", 16, 0, 0)));
        assert_eq!(f.current(), "Cuts, ores and structures", "once the game counts, its line is the step");
    }

    #[test]
    fn u2_seeds() {
        assert!(seed_ok("a-b 1_c") && seed_ok("94427382"));
        assert!(!seed_ok("") && !seed_ok("a/b") && !seed_ok(&"x".repeat(65)) && seed_ok(&"x".repeat(64)));
    }

    #[test]
    fn u3_random_seed_is_digits_under_two_to_the_52() {
        for _ in 0..20 {
            let s = random_seed();
            assert!(s.chars().all(|c| c.is_ascii_digit()) && s.parse::<u64>().unwrap() < 1 << 52 && seed_ok(&s), "{s}");
        }
    }

    #[test]
    fn u4_pace() {
        let mut p = Pace::default();
        assert_eq!(p.about(256, 16), None);
        p.learn(256, 16, 50.0, Some(40.0));
        assert_eq!(p.fixed, Some(10.0));
        assert_eq!(p.about(256, 16), Some(50));
        assert_eq!(p.about(512, 16), Some(170));
        assert_eq!(p.about(256, 32), None);
        // A tiny wait floors at 5.
        let tiny = Pace { fixed: Some(0.0), work: BTreeMap::from([(16, (256, 0.5))]) };
        assert_eq!(tiny.about(256, 16), Some(5));
        // More work than the whole run, or none known, teaches nothing.
        let before = p.clone();
        p.learn(256, 32, 10.0, Some(11.0));
        p.learn(256, 32, 10.0, None);
        p.learn(256, 32, 10.0, Some(-1.0));
        assert_eq!(p, before);
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/tmp")).join("run_unit");
        let file = dir.join("pace.json");
        p.learn(128, 64, 30.5, Some(20.25));
        p.save(&file).unwrap();
        assert_eq!(Pace::load(&file), p);
        assert_eq!(Pace::load(&dir.join("absent.json")), Pace::default());
        std::fs::write(&file, "not json").unwrap();
        assert_eq!(Pace::load(&file), Pace::default());
    }

    fn progress(pass: &str, step: u32, done: u32, total: u32) -> Progress {
        Progress { pass: pass.to_string(), step, done, total }
    }

    #[test]
    fn u5_follow() {
        let mut f = Follow::new(ask(), Some(50));
        f.seconds = 3;
        f.look(None);
        assert_eq!((f.stage.as_str(), f.part, f.done.len()), ("Starting the game", None, 0));
        f.seconds = 4;
        f.look(Some(&progress("sketch", 64, 0, 4)));
        assert_eq!(f.stage, "Sketch, every 64 blocks");
        assert_eq!(f.done, vec![("Starting the game".to_string(), 4)]);
        assert_eq!(f.part, Some(0));
        f.look(Some(&progress("sketch", 64, 2, 4)));
        assert_eq!((f.part, f.done.len()), (Some(50), 1));
        f.look(Some(&progress("map", 16, 4, 10)));
        assert_eq!((f.part, f.done.len()), (Some(40), 2));
        assert_eq!(f.current(), "Map, every 16 blocks \u{b7} 40 %");
        f.look(Some(&progress("extras", 16, 0, 0)));
        assert_eq!((f.stage.as_str(), f.part), ("Cuts, ores and structures", None));
        assert_eq!(f.current(), "Cuts, ores and structures");
        f.seconds = 20;
        assert!((f.share() - 0.4).abs() < 1e-6);
        f.seconds = 100;
        assert!((f.share() - 0.97).abs() < 1e-6);
        f.expected = None;
        assert_eq!(f.share(), 0.0);
    }

    #[test]
    fn u6_durations() {
        assert_eq!((duration(5).as_str(), duration(60).as_str(), duration(75).as_str(), duration(3600).as_str()), ("5 s", "1 min", "1 min 15 s", "60 min"));
    }

    #[test]
    fn u7_folder_names() {
        assert_eq!(folder_name(Some("cap-1"), 1_700_000_000), "cap-1_2023-11-14_22-13-20");
        assert_eq!(folder_name(None, 0), "preview_1970-01-01_00-00-00");
        assert!(folder_name(Some("a b/c"), 0).starts_with("a_b_c_"));
        // A leap day.
        assert_eq!(folder_name(None, 1_709_164_800), "preview_2024-02-29_00-00-00");
    }

    #[test]
    fn u8_same_as() {
        let a = ask();
        let mut v = pack_values();
        assert!(a.same_as("cap 1", 256, 16, &v) && a.same_as("  cap 1 ", 256, 16, &v));
        v[0] += 1e-10;
        assert!(a.same_as("cap 1", 256, 16, &v));
        v[0] += 1e-6;
        assert!(!a.same_as("cap 1", 256, 16, &v));
        assert!(!a.same_as("cap 1", 512, 16, &pack_values()) && !a.same_as("cap 1", 256, 32, &pack_values()) && !a.same_as("cap-1", 256, 16, &pack_values()));
    }

    #[test]
    fn u9_progress_file() {
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/target/tmp")).join("run_unit_progress");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_progress(&dir.join("nowhere")), None);
        std::fs::write(dir.join("progress.json"), r#"{"pass":"map","step":16,"done":4,"total":10}"#).unwrap();
        assert_eq!(read_progress(&dir), Some(progress("map", 16, 4, 10)));
        std::fs::write(dir.join("progress.json"), r#"{"pass":"map","ste"#).unwrap();
        assert_eq!(read_progress(&dir), None);
        std::fs::write(dir.join("progress.json"), "garbage").unwrap();
        assert_eq!(read_progress(&dir), None);
        std::fs::write(dir.join("timing.json"), r#"{"workMillis":1500}"#).unwrap();
        assert_eq!(work_seconds(&dir), Some(1.5));
    }
}
