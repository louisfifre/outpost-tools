//! Apply: the panel's values written into the `config/tectonic.json` of each place ticked (`places.rs`), so that
//! the game uses them.
//!
//! A file may be the pack's source (the maintainer's own, comments and all), so each is edited as text: only a value
//! that differs is replaced, in place, and the file is swapped in by a rename. A place whose file is missing is
//! refused, never given a new one. Each place has its own outcome: one refused does not stop the others. After the
//! writes the machine is looked at once, only to say what takes the values at its next start.
//!
//! What runs is asked of a `Host`, so that a test fakes the machine. The real one asks PowerShell for the command
//! lines of the Java processes (a command line is not readable from Rust's std).
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use mere_base::Launcher;

use crate::places::Kind;
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
/// Why a place was not written, short enough for the panel (the detail goes to the log).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The place has no Tectonic config (any more): none is created.
    Missing,
    /// Another program holds the file (the game, most likely).
    InUse,
    Read,
    Write,
    /// A key of the panel is missing from the file, or appears twice.
    Key,
}

/// What the panel says about the last apply.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Notice {
    #[default]
    None,
    Working,
    Done(Report),
}

/// The end of an apply: the values written, each place's outcome in the list's order, and what was running then,
/// which reads the files at its next start.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub values: [f64; 43],
    pub places: Vec<Written>,
    pub running: Vec<Running>,
}

impl Report {
    pub fn saved(&self) -> impl Iterator<Item = &Written> {
        self.places.iter().filter(|w| w.result.is_ok())
    }
}

/// What became of the second file, the snapshot a prod copy of the launcher runs from (the game's sync reads it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Snapshot {
    /// There is none: the program runs from a checkout, one file is the whole of the write.
    #[default]
    Absent,
    /// Written too: the game takes the values at its next start, until a deploy replaces the snapshot.
    Written,
    /// Not written (the detail goes to the log): the game keeps the old values.
    Failed,
}

/// One place to write: its file, and for a prod copy's pack source the snapshot's after it.
#[derive(Clone, Debug, PartialEq)]
pub struct Spot {
    pub key: String,
    pub short: String,
    pub kind: Kind,
    pub file: PathBuf,
    pub snapshot: Option<PathBuf>,
}

impl Spot {
    pub fn of(p: &crate::places::Place) -> Spot {
        Spot { key: p.key.clone(), short: p.short.clone(), kind: p.kind, file: p.config(), snapshot: p.snapshot_config() }
    }
}

/// A place's outcome: how many values changed in its file, or why it was refused.
#[derive(Clone, Debug, PartialEq)]
pub struct Written {
    pub key: String,
    pub short: String,
    pub kind: Kind,
    pub result: Result<usize, Refusal>,
    pub snapshot: Snapshot,
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
pub fn rewrite(config: &str, values: &[f64; 43]) -> Result<(String, usize), Refusal> {
    let lines: Vec<&str> = config.split('\n').collect();
    let mut edits: Vec<(usize, usize, usize, String)> = Vec::new();
    for (i, set) in SETTINGS.iter().enumerate() {
        let found = find(&lines, set.key).map_err(|_| {
            mere_base::observe!(Warn, "apply: the key '{}' is missing from the config or appears twice", set.key);
            Refusal::Key
        })?;
        let old = number_of(&found.token).ok_or(Refusal::Key)?;
        let switch = matches!(set.kind, crate::settings::Kind::Switch);
        let new = if switch { f64::from(values[i] != 0.0) } else { values[i] };
        if !crate::settings::same_value(old, new) {
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

/// Another program holds the file: Windows' sharing and lock violations.
fn in_use(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(32 | 33))
}

/// One file: read, its values that differ replaced, swapped in. A missing file is refused, not created; a file that
/// already holds the values is left alone. How many values changed.
pub fn write_place(file: &Path, values: &[f64; 43]) -> Result<usize, Refusal> {
    let said = |e: &std::io::Error| mere_base::observe!(Warn, "apply: {}: {e}", file.display());
    let text = std::fs::read_to_string(file).map_err(|e| {
        said(&e);
        match e.kind() {
            std::io::ErrorKind::NotFound => Refusal::Missing,
            _ if in_use(&e) => Refusal::InUse,
            _ => Refusal::Read,
        }
    })?;
    let (text, count) = rewrite(&text, values)?;
    if count > 0 {
        write_atomic(file, &text).map_err(|e| {
            said(&e);
            // Swapping a file in over one another program has open without sharing its deletion is denied: when the
            // file is not read-only, that program is what is in the way.
            let denied = e.raw_os_error() == Some(5) && std::fs::metadata(file).is_ok_and(|m| !m.permissions().readonly());
            if in_use(&e) || denied { Refusal::InUse } else { Refusal::Write }
        })?;
    }
    mere_base::observe!(Info, "apply: {count} values written to {}", file.display());
    Ok(count)
}

/// The whole of an apply: each place is written whatever runs and whatever became of the others, a source's snapshot
/// after it; then the machine is looked at to say what will only see the values at its next start. A machine that
/// cannot be looked at is no reason to refuse. `own_server` is the preview's own server, which is not the dev server.
pub fn apply(host: &dyn Host, spots: &[Spot], own_server: Option<&Path>, values: &[f64; 43]) -> Report {
    let places = spots
        .iter()
        .map(|s| {
            let result = write_place(&s.file, values);
            let snapshot = match (&result, &s.snapshot) {
                (Ok(_), Some(f)) => match write_place(f, values) {
                    Ok(_) => Snapshot::Written,
                    Err(why) => {
                        mere_base::observe!(Warn, "apply: the snapshot {} was not written: {why:?}", f.display());
                        Snapshot::Failed
                    }
                },
                _ => Snapshot::Absent,
            };
            Written { key: s.key.clone(), short: s.short.clone(), kind: s.kind, result, snapshot }
        })
        .collect();
    let running = match host.processes() {
        Ok(p) => running(&p, own_server),
        Err(e) => {
            mere_base::observe!(Warn, "apply: the running programs could not be listed: {e}");
            Vec::new()
        }
    };
    Report { values: *values, places, running }
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
        assert_eq!(rewrite(&missing, &pack_values()).unwrap_err(), Refusal::Key);
        let twice = config().replace("\"last\": 0", "\"ore_fix\": false");
        assert_eq!(rewrite(&twice, &pack_values()).unwrap_err(), Refusal::Key);
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
    const SERVER: &str = r#"C:\bin\java.exe -Xmx6144M -XX:+UseZGC -jar C:\server\fabric-server-26.3-0.19.2.jar nogui"#;
    const CLIENT: &str = r#"C:\prism\java\bin\javaw.exe -Xmx4G -cp lib.jar org.prismlauncher.EntryPoint"#;
    const GRADLE: &str = r#"C:\bin\java.exe --add-opens=java.base/java.lang=ALL-UNNAMED -cp gradle-launcher.jar org.gradle.launcher.daemon.bootstrap.GradleDaemon 8.10"#;

    /// A place of its own under the system's temp folder, with the file in it.
    fn place(name: &str) -> Spot {
        let dir = std::env::temp_dir().join(format!("mere-apply-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("pack").join("config")).unwrap();
        let file = crate::places::config_of(&dir.join("pack"));
        std::fs::write(&file, config()).unwrap();
        Spot { key: name.into(), short: name.into(), kind: Kind::Game, file, snapshot: None }
    }

    fn root_of(s: &Spot) -> PathBuf {
        s.file.ancestors().nth(3).unwrap().to_path_buf()
    }

    const OWN: &str = r"C:\preview\server";

    fn one(host: &dyn Host, s: &Spot, values: &[f64; 43]) -> (Result<usize, Refusal>, Vec<Running>) {
        let mut r = apply(host, std::slice::from_ref(s), Some(Path::new(OWN)), values);
        (r.places.remove(0).result, r.running)
    }

    #[test]
    fn the_servers_folder_is_read_from_its_command_line() {
        assert_eq!(server_dir_of(SERVER), Some(PathBuf::from(r"C:\server")));
        assert_eq!(server_dir_of(r#"java -jar "C:\my server\Fabric-Server-26.3.jar" nogui"#), Some(PathBuf::from(r"C:\my server")));
        assert_eq!(server_dir_of(GRADLE), None);
    }

    #[test]
    fn a_game_a_server_and_neither_are_told_apart() {
        assert_eq!(running(&[java(GRADLE)], None), vec![]);
        assert_eq!(running(&[java(GRADLE), java(CLIENT)], None), vec![Running::Game]);
        assert_eq!(running(&[java(SERVER)], None), vec![Running::Server(PathBuf::from(r"C:\server"))]);
        assert_eq!(running(&[java(SERVER), java(CLIENT)], None), vec![Running::Game, Running::Server(PathBuf::from(r"C:\server"))]);
        // The preview's own throwaway server is not the dev server.
        let own = r#"java -jar C:\preview\server\fabric-server-26.3-0.19.2.jar nogui"#;
        assert_eq!(running(&[java(own)], Some(Path::new(r"c:\preview\server"))), vec![]);
    }

    #[test]
    fn a_running_game_or_server_does_not_stop_the_write_and_is_named_after_it() {
        let t = place("busy");
        let host = Fake::new(vec![java(CLIENT), java(SERVER)]);
        let (result, running) = one(&host, &t, &with("vertical_scale", 2.0));
        assert_eq!(result, Ok(1));
        assert_eq!(running, vec![Running::Game, Running::Server(PathBuf::from(r"C:\server"))]);
        let written = std::fs::read_to_string(&t.file).unwrap();
        assert!(written.contains("\"vertical_scale\": 2.0,"));
        assert!(!t.file.with_extension("json.tmp").exists(), "no part file is left");
        let _ = std::fs::remove_dir_all(root_of(&t));
    }

    #[test]
    fn an_unlistable_machine_still_writes() {
        let t = place("blind");
        let mut host = Fake::new(vec![]);
        host.fail_listing = true;
        assert_eq!(one(&host, &t, &with("ocean_offset", -0.5)), (Ok(1), vec![]));
        assert!(std::fs::read_to_string(&t.file).unwrap().contains("\"ocean_offset\": -0.5,"));
        let _ = std::fs::remove_dir_all(root_of(&t));
    }

    #[test]
    fn nothing_to_change_writes_nothing() {
        let t = place("same");
        let before = std::fs::metadata(&t.file).unwrap().modified().unwrap();
        assert_eq!(one(&Fake::new(vec![]), &t, &pack_values()).0, Ok(0));
        assert_eq!(std::fs::metadata(&t.file).unwrap().modified().unwrap(), before);
        let _ = std::fs::remove_dir_all(root_of(&t));
    }

    #[test]
    fn a_bad_file_is_refused_and_left_as_it_was() {
        let t = place("bad");
        std::fs::write(&t.file, config().replace("\"ore_fix\"", "\"nope\"")).unwrap();
        assert_eq!(one(&Fake::new(vec![]), &t, &with("vertical_scale", 2.0)).0, Err(Refusal::Key));
        assert!(std::fs::read_to_string(&t.file).unwrap().contains("\"nope\""));
        let _ = std::fs::remove_dir_all(root_of(&t));
    }

    #[test]
    fn a_missing_file_is_refused_and_not_created() {
        let t = place("missing");
        std::fs::remove_file(&t.file).unwrap();
        assert_eq!(one(&Fake::new(vec![]), &t, &with("vertical_scale", 2.0)).0, Err(Refusal::Missing));
        assert!(!t.file.exists(), "no file was made");
        let _ = std::fs::remove_dir_all(root_of(&t));
    }

    /// A file another program holds without sharing it, as a game can: that place is refused as in use, and the
    /// place after it is still written.
    #[test]
    fn a_locked_file_is_refused_as_in_use_and_the_next_place_is_written() {
        use std::os::windows::fs::OpenOptionsExt;
        let (locked, free) = (place("locked"), place("free"));
        let held = std::fs::OpenOptions::new().read(true).share_mode(0).open(&locked.file).unwrap();
        let r = apply(&Fake::new(vec![]), &[locked.clone(), free.clone()], None, &with("vertical_scale", 2.0));
        drop(held);
        let results: Vec<_> = r.places.iter().map(|w| (w.key.as_str(), w.result)).collect();
        assert_eq!(results, vec![("locked", Err(Refusal::InUse)), ("free", Ok(1))]);
        assert!(std::fs::read_to_string(&locked.file).unwrap().contains("\"vertical_scale\": 1.5,"), "the locked file is as it was");
        // Held by a reader that lets others read but not delete: the swap is what fails then, still as in use.
        let shared = std::fs::OpenOptions::new().read(true).share_mode(1).open(&locked.file).unwrap();
        let swap = one(&Fake::new(vec![]), &locked, &with("vertical_scale", 2.0)).0;
        drop(shared);
        assert_eq!(swap, Err(Refusal::InUse));
        assert!(!locked.file.with_extension("json.tmp").exists(), "no part file is left");
        // A read-only file is no game's doing.
        let mut perms = std::fs::metadata(&locked.file).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&locked.file, perms.clone()).unwrap();
        let ro = one(&Fake::new(vec![]), &locked, &with("vertical_scale", 2.0)).0;
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&locked.file, perms).unwrap();
        assert_eq!(ro, Err(Refusal::Write));
        let _ = std::fs::remove_dir_all(root_of(&locked));
        let _ = std::fs::remove_dir_all(root_of(&free));
    }

    /// A prod copy's pack source: the snapshot is written after it, and only when the source was.
    #[test]
    fn a_source_s_snapshot_follows_it() {
        let (mut source, snap) = (place("source"), place("snap"));
        source.snapshot = Some(snap.file.clone());
        let r = apply(&Fake::new(vec![]), std::slice::from_ref(&source), None, &with("max_y", 448.0));
        assert_eq!((r.places[0].result, r.places[0].snapshot), (Ok(1), Snapshot::Written));
        assert!(std::fs::read_to_string(&snap.file).unwrap().contains("\"max_y\": 448,"));
        std::fs::remove_file(&snap.file).unwrap();
        let r = apply(&Fake::new(vec![]), std::slice::from_ref(&source), None, &with("max_y", 384.0));
        assert_eq!((r.places[0].result, r.places[0].snapshot), (Ok(1), Snapshot::Failed));
        let _ = std::fs::remove_dir_all(root_of(&source));
        let _ = std::fs::remove_dir_all(root_of(&snap));
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
