//! What the api answers, read from JSON by hand (serde_derive is not available offline), and the small pure
//! functions of Home: the line under a profile, the order of the list, the memory's limits.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum Open {
    Menu,
    World(String),
    Server(String),
}

impl Open {
    pub fn kind(&self) -> &'static str {
        match self {
            Open::Menu => "menu",
            Open::World(_) => "world",
            Open::Server(_) => "server",
        }
    }
    pub fn target(&self) -> &str {
        match self {
            Open::Menu => "",
            Open::World(t) | Open::Server(t) => t,
        }
    }
    pub fn from(kind: &str, target: &str) -> Open {
        match kind {
            "world" => Open::World(target.to_string()),
            "server" => Open::Server(target.to_string()),
            _ => Open::Menu,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub memory_mb: i64,
    pub java_args: String,
    pub game_dir: String,
    pub last_played: i64,
    pub played_seconds: i64,
    pub running: bool,
    pub worlds: Vec<String>,
    pub open: Open,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profiles {
    pub machine_mb: i64,
    pub account: Option<String>,
    pub locked: bool,
    pub profiles: Vec<Profile>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Props {
    pub motd: String,
    pub whitelist: bool,
    pub view: i64,
    pub sim: i64,
    pub players: i64,
    pub port: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Server {
    pub dir: String,
    pub state: String,
    pub eula: bool,
    pub memory_mb: i64,
    pub java_args: String,
    pub props: Option<Props>,
}

fn s(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_string()
}

/// A number the way the page reads it: whole or fractional, missing is 0.
fn n(v: &Value, key: &str) -> i64 {
    v[key].as_i64().or_else(|| v[key].as_f64().map(|f| f as i64)).unwrap_or(0)
}

impl Profiles {
    pub fn parse(v: &Value) -> Profiles {
        let profiles = v["profiles"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|p| Profile {
                        id: s(p, "id"),
                        name: s(p, "name"),
                        memory_mb: n(p, "memoryMb"),
                        java_args: s(p, "javaArgs"),
                        game_dir: s(p, "gameDir"),
                        last_played: n(p, "lastPlayed"),
                        played_seconds: n(p, "playedSeconds"),
                        running: p["running"].as_bool().unwrap_or(false),
                        worlds: p["worlds"].as_array().map(|w| w.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
                        open: Open::from(p["open"]["kind"].as_str().unwrap_or("menu"), p["open"]["target"].as_str().unwrap_or("")),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Profiles {
            machine_mb: n(v, "machineMb"),
            account: v["account"].as_str().filter(|a| !a.is_empty()).map(String::from),
            locked: v["locked"].as_bool().unwrap_or(false),
            profiles,
        }
    }
}

impl Server {
    pub fn parse(v: &Value) -> Server {
        let p = &v["props"];
        let props = p.is_object().then(|| Props {
            motd: s(p, "motd"),
            whitelist: p["whitelist"].as_bool().unwrap_or(false),
            view: n(p, "viewDistance"),
            sim: n(p, "simulationDistance"),
            players: n(p, "maxPlayers"),
            port: n(p, "port"),
        });
        Server {
            dir: s(v, "dir"),
            state: s(v, "state"),
            eula: v["eula"].as_bool().unwrap_or(false),
            memory_mb: n(v, "memoryMb"),
            java_args: s(v, "javaArgs"),
            props,
        }
    }
    /// `serverBusy` (line 609).
    pub fn busy(&self) -> bool {
        matches!(self.state.as_str(), "preparing" | "starting" | "running")
    }
}

/// `Intl.RelativeTimeFormat('en', { numeric: 'auto' }).format(days, 'day')`.
pub fn relative_day(days: i64) -> String {
    match days {
        0 => "today".into(),
        -1 => "yesterday".into(),
        1 => "tomorrow".into(),
        d if d < 0 => format!("{} days ago", -d),
        d => format!("in {d} days"),
    }
}

/// JS `Number.prototype.toFixed` then the unary plus: round half up on the exact value, and a whole number
/// without its ".0".
fn to_fixed_plus(v: f64, digits: u32) -> String {
    let scale = 10f64.powi(digits as i32);
    // Ties exist only for values a binary float holds exactly: x.25 and x.75 at one digit, x.5 at none.
    let tie = (v * scale).fract() == 0.5 && (v * 4.0).fract() == 0.0;
    let text = if tie { format!("{:.*}", digits as usize, ((v * scale).ceil()) / scale) } else { format!("{:.*}", digits as usize, v) };
    let back: f64 = text.parse().unwrap_or(v);
    format!("{back}")
}

/// "yesterday · 1.7 h played · opens New World" (lines 477-489). `days` is the local calendar day of the last play
/// less today's.
pub fn about(p: &Profile, days: i64) -> String {
    if p.last_played == 0 {
        return "not played yet".into();
    }
    let mut parts = vec![relative_day(days)];
    // Math.round: half up.
    let minutes = (p.played_seconds as f64 / 60.0 + 0.5).floor() as i64;
    if minutes >= 60 {
        parts.push(format!("{} h played", to_fixed_plus(minutes as f64 / 60.0, if minutes < 600 { 1 } else { 0 })));
    } else if minutes > 0 {
        parts.push(format!("{minutes} min played"));
    }
    match &p.open {
        Open::World(w) => parts.push(format!("opens {w}")),
        Open::Server(a) => parts.push(format!("joins {a}")),
        Open::Menu => {}
    }
    parts.join(" \u{b7} ")
}

/// The profile played last first, then by name (`localeCompare`, read as a comparison without case: an
/// INTERPRETATION, equal for the names the launcher has).
pub fn ordered(profiles: &[Profile]) -> Vec<Profile> {
    let mut v = profiles.to_vec();
    v.sort_by(|a, b| b.last_played.cmp(&a.last_played).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.name.cmp(&b.name))));
    v
}

/// Leave the system a quarter of the machine's memory (line 525), in whole gigabytes. At least the field's own
/// minimum, so that a machine that reports nothing leaves a range.
pub fn memory_max(machine_mb: i64) -> f64 {
    ((machine_mb as f64 / 1024.0 * 0.75).floor()).max(2.0)
}

/// The page's `change` handler for a number field (lines 704-709): rounded to a whole number, then held within
/// its range.
pub fn round_clamp(v: f64, min: f64, max: f64) -> f64 {
    if !v.is_finite() {
        return min;
    }
    // Math.round: half up.
    (v + 0.5).floor().clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn profile(last: i64, secs: i64, open: Open) -> Profile {
        Profile { id: "x".into(), name: "X".into(), memory_mb: 0, java_args: String::new(), game_dir: String::new(), last_played: last, played_seconds: secs, running: false, worlds: vec![], open }
    }

    #[test]
    fn about_says_when_how_long_and_what_opens() {
        assert_eq!(about(&profile(0, 0, Open::Menu), 0), "not played yet");
        assert_eq!(about(&profile(1, 6120, Open::World("New World".into())), -1), "yesterday \u{b7} 1.7 h played \u{b7} opens New World");
        assert_eq!(about(&profile(1, 1200, Open::Menu), 0), "today \u{b7} 20 min played");
        assert_eq!(about(&profile(1, 0, Open::Server("localhost".into())), 3), "in 3 days \u{b7} joins localhost");
        assert_eq!(about(&profile(1, 7200, Open::Menu), -2), "2 days ago \u{b7} 2 h played");
        // 600 minutes and over: no decimal.
        assert_eq!(about(&profile(1, 39_600, Open::Menu), 0), "today \u{b7} 11 h played");
        // x.25 and x.75 round up, as toFixed does.
        assert_eq!(about(&profile(1, 4500, Open::Menu), 0), "today \u{b7} 1.3 h played");
        assert_eq!(about(&profile(1, 29, Open::Menu), 0), "today");
    }

    #[test]
    fn relative_days_as_intl_says_them() {
        assert_eq!(
            [0, -1, 1, -5, 7].map(relative_day),
            ["today", "yesterday", "tomorrow", "5 days ago", "in 7 days"].map(String::from)
        );
    }

    #[test]
    fn the_last_played_comes_first_then_the_name() {
        let mut a = profile(5, 0, Open::Menu);
        a.name = "beta".into();
        let mut b = profile(5, 0, Open::Menu);
        b.name = "Alpha".into();
        let c = profile(9, 0, Open::Menu);
        let names: Vec<String> = ordered(&[a, b, c]).iter().map(|p| p.name.clone()).collect();
        assert_eq!(names, ["X", "Alpha", "beta"]);
    }

    #[test]
    fn memory_is_three_quarters_of_the_machine_in_whole_gigabytes() {
        assert_eq!(memory_max(32768), 24.0);
        assert_eq!(memory_max(16384), 12.0);
        assert_eq!(memory_max(0), 2.0);
        assert_eq!(memory_max(10240), 7.0);
    }

    #[test]
    fn a_number_is_rounded_then_held() {
        assert_eq!(round_clamp(3.6, 2.0, 24.0), 4.0);
        assert_eq!(round_clamp(99.0, 2.0, 24.0), 24.0);
        assert_eq!(round_clamp(0.0, 2.0, 24.0), 2.0);
        assert_eq!(round_clamp(2.5, 2.0, 24.0), 3.0);
        assert_eq!(round_clamp(f64::NAN, 2.0, 24.0), 2.0);
    }

    #[test]
    fn the_answers_are_read_with_missing_keys_as_defaults() {
        let p = Profiles::parse(&json!({"machineMb": 8192, "profiles": [{"id": "a", "name": "A", "open": {"kind": "server", "target": "h"}}]}));
        assert_eq!(p.machine_mb, 8192);
        assert_eq!(p.account, None);
        assert!(!p.locked);
        assert_eq!(p.profiles[0].open, Open::Server("h".into()));
        assert_eq!(p.profiles[0].memory_mb, 0);
        let s = Server::parse(&json!({"state": "running", "props": null}));
        assert!(s.props.is_none() && s.busy());
        let s = Server::parse(&json!({"state": "stopped", "props": {"port": 25570, "whitelist": true}}));
        assert_eq!(s.props.unwrap().port, 25570);
    }
}
