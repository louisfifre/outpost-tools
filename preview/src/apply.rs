//! "Apply to pack": the panel's values written into the pack's `config/tectonic.json`, so that the game uses them.
//!
//! The file is the pack's source (the maintainer's own, comments and all), so it is edited as text: only a value
//! that differs is replaced, in place, and the file is swapped in by a rename. Nothing else is written: a game
//! client and the dev server get the file when packwiz-installer copies it into them at their next start, so
//! a running one is not in the way. After the write the machine is looked at once, only to say that.
//!
//! What runs is asked of a `Host`, so that a test fakes the machine. The real one asks PowerShell for the command
//! lines of the Java processes (a command line is not readable from Rust's std).
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use mere_base::Launcher;

use crate::settings::SETTINGS;

/// A running Java process: its program's name and its command line, both as the system gives them.
#[derive(Clone, Debug)]
pub struct Proc {
    pub name: String,
    pub cmd: String,
}

/// The machine, as far as an apply needs it.
pub trait Host: Send + Sync {
    /// The Java processes now running.
    fn processes(&self) -> Result<Vec<Proc>, String>;
}

/// What is running that only reads the new values at its next start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Running {
    /// A Minecraft client.
    Game,
    /// The pack's dev server, by the folder its Fabric launcher jar is in.
    Server(PathBuf),
}
/// Why a write did not happen, short enough for the panel (the detail goes to the log).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fail {
    Read,
    Write,
    /// A key of the panel is missing from the file, or appears twice.
    Key,
}

/// What the panel says about the last apply.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Notice {
    #[default]
    None,
    Working,
    /// Written; what was running then, which reads the file at its next start.
    Done { running: Vec<Running> },
    Failed(Fail),
}

/// Where the pack is, and the server the preview itself runs (not the dev server: it reads its own copy).
#[derive(Clone, Debug)]
pub struct Target {
    pub pack: PathBuf,
    pub own_server: Option<PathBuf>,
}

impl Target {
    pub fn config(&self) -> PathBuf {
        self.pack.join("config").join("tectonic.json")
    }
}

pub enum Outcome {
    /// Written (how many values changed), and what was running.
    Applied(usize, Vec<Running>),
    Failed(Fail),
}
fn lower(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// The folder of the Fabric launcher jar in a command line (`-jar <dir>\fabric-server-<mc>-<loader>.jar`), as
/// `dev-server.ps1` starts it, with the full path.
fn server_dir_of(cmd: &str) -> Option<PathBuf> {
    let at = cmd.to_lowercase().find("fabric-server-")?;
    let head = &cmd[..at];
    // The path starts after the `-jar ` (and a quote) that is the last one before the file's name.
    let start = head.to_lowercase().rfind("-jar ").map(|i| i + 5)?;
    let dir = head[start..].trim_start().trim_start_matches('"');
    let dir = dir.trim_end_matches(['\\', '/']);
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

/// What is running that matters: a game client, and the dev server (a Fabric launcher jar that is not the preview's own).
pub fn running(procs: &[Proc], own_server: Option<&Path>) -> Vec<Running> {
    let java = |p: &&Proc| matches!(p.name.to_lowercase().as_str(), "java.exe" | "javaw.exe" | "java" | "javaw");
    // A client: Fabric's client entry point, Minecraft's own, Prism's launch wrapper, or a game folder.
    const CLIENT: [&str; 4] = ["knotclient", "net.minecraft.client.main", "org.prismlauncher.entrypoint", "--gamedir"];
    let mut out = Vec::new();
    if procs.iter().filter(java).any(|p| {
        let cmd = p.cmd.to_lowercase();
        CLIENT.iter().any(|m| cmd.contains(m))
    }) {
        out.push(Running::Game);
    }
    let own = own_server.map(lower);
    if let Some(dir) = procs.iter().filter(java).filter_map(|p| server_dir_of(&p.cmd)).find(|dir| own.as_deref() != Some(lower(dir).as_str())) {
        out.push(Running::Server(dir));
    }
    out
}
/// Reads one value: the word after `"key":`, as a number (a switch is 1 or 0), and where it is in the line.
struct Found {
    line: usize,
    from: usize,
    to: usize,
    token: String,
}

fn find(lines: &[&str], key: &str) -> Result<Found, ()> {
    let needle = format!("\"{key}\"");
    let mut hits = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        let mut from = 0;
        while let Some(at) = line[from..].find(&needle) {
            let after = from + at + needle.len();
            let rest = &line[after..];
            let trimmed = rest.trim_start();
            if let Some(value) = trimmed.strip_prefix(':') {
                let space = value.len() - value.trim_start().len();
                let begin = line.len() - value.len() + space;
                let token: String = line[begin..].chars().take_while(|c| !matches!(c, ',' | '}' | ' ' | '\t' | '\r' | '/')).collect();
                if !token.is_empty() {
                    hits.push(Found { line: i, from: begin, to: begin + token.len(), token });
                }
            }
            from = after;
        }
    }
    if hits.len() == 1 { Ok(hits.remove(0)) } else { Err(()) }
}

fn number_of(token: &str) -> Option<f64> {
    match token {
        "true" => Some(1.0),
        "false" => Some(0.0),
        t => t.parse().ok(),
    }
}

/// A value as the file spells it: a switch as a word; a number in the file's own style (a file's `128` stays whole,
/// its `0.0` keeps its point), cleaned of the float noise a field's arithmetic leaves.
fn spell(old: &str, switch: bool, value: f64) -> String {
    if switch {
        return if value != 0.0 { "true" } else { "false" }.to_string();
    }
    let clean = (value * 1e6).round() / 1e6;
    let plain = clean.to_string();
    if old.contains(['.', 'e', 'E']) && !plain.contains('.') { format!("{plain}.0") } else { plain }
}

/// The config with the values that differ replaced, and how many. A key that the file lacks (or has twice) is an
/// error: nothing is guessed.
pub fn rewrite(config: &str, values: &[f64; 43]) -> Result<(String, usize), Fail> {
    let lines: Vec<&str> = config.split('\n').collect();
    let mut edits: Vec<(usize, usize, usize, String)> = Vec::new();
    for (i, set) in SETTINGS.iter().enumerate() {
        let found = find(&lines, set.key).map_err(|_| {
            mere_base::observe!(Warn, "apply: the key '{}' is missing from the config or appears twice", set.key);
            Fail::Key
        })?;
        let old = number_of(&found.token).ok_or(Fail::Key)?;
        let switch = matches!(set.kind, crate::settings::Kind::Switch);
        let new = if switch { f64::from(values[i] != 0.0) } else { (values[i] * 1e6).round() / 1e6 };
        if (old - new).abs() > 1e-9 {
            edits.push((found.line, found.from, found.to, spell(&found.token, switch, values[i])));
        }
    }
    let count = edits.len();
    // From the right, so that an edit does not move the next one on its line.
    edits.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)));
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    for (line, from, to, text) in edits {
        out[line].replace_range(from..to, &text);
    }
    Ok((out.join("\n"), count))
}

/// Writes beside the file and swaps it in, so that a game reading it never sees half of one.
fn write_atomic(file: &Path, text: &str) -> std::io::Result<()> {
    let part = file.with_extension("json.tmp");
    std::fs::write(&part, text)?;
    std::fs::rename(&part, file).inspect_err(|_| {
        let _ = std::fs::remove_file(&part);
    })
}

/// The values the file holds now, in `SETTINGS` order (what "differs from the pack" means once a person edited the
/// file by hand). None when the file cannot be read or lacks a key.
pub fn read_values(config: &str) -> Option<[f64; 43]> {
    let lines: Vec<&str> = config.split('\n').collect();
    let mut out = [0.0; 43];
    for (i, set) in SETTINGS.iter().enumerate() {
        out[i] = number_of(&find(&lines, set.key).ok()?.token)?;
    }
    Some(out)
}

/// The whole of an apply: the file is written whatever runs, then the machine is looked at to say what will only
/// see the values at its next start. A machine that cannot be looked at is no reason to refuse.
pub fn apply(host: &dyn Host, target: &Target, values: &[f64; 43]) -> Outcome {
    let file = target.config();
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) => {
            mere_base::observe!(Warn, "apply: {}: {e}", file.display());
            return Outcome::Failed(Fail::Read);
        }
    };
    let (text, count) = match rewrite(&text, values) {
        Ok(r) => r,
        Err(f) => return Outcome::Failed(f),
    };
    if count > 0 {
        if let Err(e) = write_atomic(&file, &text) {
            mere_base::observe!(Warn, "apply: {}: {e}", file.display());
            return Outcome::Failed(Fail::Write);
        }
    }
    mere_base::observe!(Info, "apply: {count} values written to {}", file.display());
    let running = match host.processes() {
        Ok(p) => running(&p, target.own_server.as_deref()),
        Err(e) => {
            mere_base::observe!(Warn, "apply: the running programs could not be listed: {e}");
            Vec::new()
        }
    };
    Outcome::Applied(count, running)
}

/// The real machine.
pub struct SystemHost;

impl Host for SystemHost {
    fn processes(&self) -> Result<Vec<Proc>, String> {
        // Single quotes only, so that no quoting rule of the command line gets in the way; the tab separates.
        let script = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; Get-CimInstance Win32_Process | Where-Object { $_.Name -in 'java.exe','javaw.exe' } | ForEach-Object { $_.Name + [char]9 + $_.CommandLine }";
        let mut cmd = Command::new("powershell.exe");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", script]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = mere_win32::SystemLauncher.spawn(&mut cmd).map_err(|e| e.to_string())?;
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!("exit {:?}: {}", out.status.code(), String::from_utf8_lossy(&out.stderr).trim()));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        Ok(text.lines().filter_map(|l| l.split_once('\t')).map(|(n, c)| Proc { name: n.trim().to_string(), cmd: c.trim().to_string() }).collect())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::pack_values;

    /// A copy of the pack's file in the form that matters: comments, sections, an integer, a `0.0`, a switch.
    fn config() -> String {
        let mut s = String::from("{\n");
        s.push_str("  \"minor_version\": 1,\n");
        s.push_str("  \"biomes\": {\n");
        for (k, v) in [("temperature_multiplier", "0.9"), ("temperature_offset", "0.1"), ("temperature_scale", "0.05"), ("vegetation_multiplier", "1.1"), ("vegetation_offset", "0.0"), ("vegetation_scale", "0.05")] {
            s.push_str(&format!("    \"{k}\": {v},\n"));
        }
        s.push_str("  },\n  \"rest\": {\n");
        let done = ["temperature_multiplier", "temperature_offset", "temperature_scale", "vegetation_multiplier", "vegetation_offset", "vegetation_scale"];
        for set in SETTINGS.iter().filter(|s| !done.contains(&s.key)) {
            let v = match (set.kind, set.pack) {
                (crate::settings::Kind::Switch, v) => (v != 0.0).to_string(),
                (_, v) if v.fract() == 0.0 && set.key != "elevation_boost" => format!("{}", v as i64),
                (_, v) if v == 0.0 => "0.0".to_string(),
                (_, v) => v.to_string(),
            };
            // A comment that names another key in quotes: it must not be taken for the key.
            s.push_str(&format!("    // Sets \"{}\" and more. Game restart required!\n", set.key));
            s.push_str(&format!("    \"{}\": {v},\n", set.key));
        }
        s.push_str("    \"last\": 0\n  }\n}\n");
        s
    }

    fn with(key: &str, v: f64) -> [f64; 43] {
        let mut values = pack_values();
        values[SETTINGS.iter().position(|s| s.key == key).unwrap()] = v;
        values
    }

    #[test]
    fn nothing_differs_nothing_changes() {
        let (out, n) = rewrite(&config(), &pack_values()).unwrap();
        assert_eq!((out, n), (config(), 0));
    }

    #[test]
    fn only_the_changed_value_is_replaced_and_comments_stay() {
        let (out, n) = rewrite(&config(), &with("vertical_scale", 2.25)).unwrap();
        assert_eq!(n, 1);
        let before = config();
        let (a, b): (Vec<_>, Vec<_>) = (before.lines().collect(), out.lines().collect());
        let diff: Vec<_> = a.iter().zip(&b).filter(|(x, y)| x != y).collect();
        assert_eq!(diff.len(), 1);
        assert_eq!(*diff[0].1, "    \"vertical_scale\": 2.25,");
        assert!(out.contains("// Sets \"vertical_scale\" and more."), "the comment that names the key is untouched");
        assert!(out.ends_with("}\n"));
    }

    #[test]
    fn a_whole_number_stays_whole_and_a_point_stays() {
        let (out, _) = rewrite(&config(), &with("max_y", 384.0)).unwrap();
        assert!(out.contains("\"max_y\": 384,"));
        let (out, _) = rewrite(&config(), &with("vegetation_offset", 0.2)).unwrap();
        assert!(out.contains("\"vegetation_offset\": 0.2,"));
        let (out, _) = rewrite(&config(), &with("elevation_boost", 1.0)).unwrap();
        assert!(out.contains("\"elevation_boost\": 1.0,"), "a number written with a point keeps one");
    }

    #[test]
    fn a_switch_is_a_word_and_float_noise_is_cleaned() {
        let (out, n) = rewrite(&config(), &with("rolling_hills", 0.0)).unwrap();
        assert_eq!(n, 1);
        assert!(out.contains("\"rolling_hills\": false,"));
        let (out, _) = rewrite(&config(), &with("temperature_scale", 0.1 + 0.2)).unwrap();
        assert!(out.contains("\"temperature_scale\": 0.3,"));
    }

    #[test]
    fn a_crlf_file_keeps_its_line_ends() {
        let crlf = config().replace('\n', "\r\n");
        let (out, _) = rewrite(&crlf, &with("min_y", -128.0)).unwrap();
        assert!(out.contains("\"min_y\": -128,\r\n"));
        assert_eq!(out.matches("\r\n").count(), crlf.matches("\r\n").count());
    }

    #[test]
    fn a_missing_or_doubled_key_is_refused() {
        let missing = config().replace("\"ore_fix\"", "\"ore_fixx\"");
        assert_eq!(rewrite(&missing, &pack_values()).unwrap_err(), Fail::Key);
        let twice = config().replace("\"last\": 0", "\"ore_fix\": false");
        assert_eq!(rewrite(&twice, &pack_values()).unwrap_err(), Fail::Key);
    }

    #[test]
    fn the_file_s_values_can_be_read_back() {
        assert_eq!(read_values(&config()), Some(pack_values()));
    }

    struct Fake {
        procs: Vec<Proc>,
        fail_listing: bool,
    }

    impl Fake {
        fn new(procs: Vec<Proc>) -> Fake {
            Fake { procs, fail_listing: false }
        }
    }

    impl Host for Fake {
        fn processes(&self) -> Result<Vec<Proc>, String> {
            if self.fail_listing { Err("no powershell".into()) } else { Ok(self.procs.clone()) }
        }
    }
    fn java(cmd: &str) -> Proc {
        Proc { name: "java.exe".into(), cmd: cmd.into() }
    }
    const SERVER: &str = r#"D:\bin\java.exe -Xmx6144M -XX:+UseZGC -jar M:\server\fabric-server-26.3-0.19.2.jar nogui"#;
    const CLIENT: &str = r#"C:\prism\java\bin\javaw.exe -Xmx4G -cp lib.jar org.prismlauncher.EntryPoint"#;
    const GRADLE: &str = r#"D:\bin\java.exe --add-opens=java.base/java.lang=ALL-UNNAMED -cp gradle-launcher.jar org.gradle.launcher.daemon.bootstrap.GradleDaemon 8.10"#;

    /// A pack folder of its own under the system's temp folder, with the file in it.
    fn pack(name: &str) -> Target {
        let dir = std::env::temp_dir().join(format!("mere-apply-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("pack").join("config")).unwrap();
        let t = Target { pack: dir.join("pack"), own_server: Some(PathBuf::from(r"M:\preview\server")) };
        std::fs::write(t.config(), config()).unwrap();
        t
    }

    #[test]
    fn the_servers_folder_is_read_from_its_command_line() {
        assert_eq!(server_dir_of(SERVER), Some(PathBuf::from(r"M:\server")));
        assert_eq!(server_dir_of(r#"java -jar "M:\my server\Fabric-Server-26.3.jar" nogui"#), Some(PathBuf::from(r"M:\my server")));
        assert_eq!(server_dir_of(GRADLE), None);
    }

    #[test]
    fn a_game_a_server_and_neither_are_told_apart() {
        assert_eq!(running(&[java(GRADLE)], None), vec![]);
        assert_eq!(running(&[java(GRADLE), java(CLIENT)], None), vec![Running::Game]);
        assert_eq!(running(&[java(SERVER)], None), vec![Running::Server(PathBuf::from(r"M:\server"))]);
        assert_eq!(running(&[java(SERVER), java(CLIENT)], None), vec![Running::Game, Running::Server(PathBuf::from(r"M:\server"))]);
        // The preview's own throwaway server is not the dev server.
        let own = r#"java -jar M:\preview\server\fabric-server-26.3-0.19.2.jar nogui"#;
        assert_eq!(running(&[java(own)], Some(Path::new(r"m:\preview\server"))), vec![]);
    }

    #[test]
    fn a_running_game_or_server_does_not_stop_the_write_and_is_named_after_it() {
        let t = pack("busy");
        let host = Fake::new(vec![java(CLIENT), java(SERVER)]);
        let Outcome::Applied(1, running) = apply(&host, &t, &with("vertical_scale", 2.0)) else { panic!("written") };
        assert_eq!(running, vec![Running::Game, Running::Server(PathBuf::from(r"M:\server"))]);
        let written = std::fs::read_to_string(t.config()).unwrap();
        assert!(written.contains("\"vertical_scale\": 2.0,"));
        assert!(!t.config().with_extension("json.tmp").exists(), "no part file is left");
        let _ = std::fs::remove_dir_all(t.pack.parent().unwrap());
    }

    #[test]
    fn an_unlistable_machine_still_writes() {
        let t = pack("blind");
        let mut host = Fake::new(vec![]);
        host.fail_listing = true;
        assert!(matches!(apply(&host, &t, &with("ocean_offset", -0.5)), Outcome::Applied(1, r) if r.is_empty()));
        assert!(std::fs::read_to_string(t.config()).unwrap().contains("\"ocean_offset\": -0.5,"));
        let _ = std::fs::remove_dir_all(t.pack.parent().unwrap());
    }

    #[test]
    fn nothing_to_change_writes_nothing() {
        let t = pack("same");
        let before = std::fs::metadata(t.config()).unwrap().modified().unwrap();
        assert!(matches!(apply(&Fake::new(vec![]), &t, &pack_values()), Outcome::Applied(0, _)));
        assert_eq!(std::fs::metadata(t.config()).unwrap().modified().unwrap(), before);
        let _ = std::fs::remove_dir_all(t.pack.parent().unwrap());
    }

    #[test]
    fn a_bad_file_is_refused_and_left_as_it_was() {
        let t = pack("bad");
        std::fs::write(t.config(), config().replace("\"ore_fix\"", "\"nope\"")).unwrap();
        assert!(matches!(apply(&Fake::new(vec![]), &t, &with("vertical_scale", 2.0)), Outcome::Failed(Fail::Key)));
        assert!(std::fs::read_to_string(t.config()).unwrap().contains("\"nope\""));
        let _ = std::fs::remove_dir_all(t.pack.parent().unwrap());
    }
    /// The real file of the pack, when `OUTPOST_REPO` names the pack repository: every key of the panel is found
    /// once, and applying the pack's own values changes nothing in a copy.
    #[test]
    fn the_real_config_is_understood() {
        let Some(dir) = std::env::var_os("OUTPOST_REPO") else {
            println!("skipped: OUTPOST_REPO names no pack repository");
            return;
        };
        let text = std::fs::read_to_string(Path::new(&dir).join("pack").join("config").join("tectonic.json")).unwrap();
        let held = read_values(&text).expect("every key is there once");
        assert_eq!(held, pack_values(), "the panel's pack values are the file's");
        let (out, n) = rewrite(&text, &held).unwrap();
        assert_eq!((out == text, n), (true, 0));
    }

    /// The real machine answers (PowerShell is needed): run by hand with `--ignored`.
    #[test]
    #[ignore]
    fn the_real_host_lists_java() {
        let procs = SystemHost.processes().expect("powershell answers");
        println!("{} java processes", procs.len());
        println!("{:?}", running(&procs, None));
    }
}
