// Servers as data: one folder per server inside the servers folder the user chose, an Outpost Tools file
// (`outpost-server.json`) next to the server's own files. A server is made from a profile: the Fabric server
// launcher for its game version, and the profile's mods that run on a server. It starts and stops from Rust, behind
// traits so the tests use a fake process and fixtures. The dev server (dev_server.rs) is a separate, older path.

use std::{
    fs, io,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Stdio},
    time::{Duration, Instant, SystemTime},
};

use serde_json::{Value, json};

use super::key_value::Props;

pub const META_FILE: &str = "outpost-server.json";
// The name the Fabric server launcher is saved under in a server's folder.
pub const LAUNCHER_JAR: &str = "fabric-server-launcher.jar";
pub const DEFAULT_PORT: u16 = 25565;

// ---- A server on disk ----

#[derive(Clone, Debug, PartialEq)]
pub struct Server {
    pub dir: PathBuf,
    pub name: String,
    pub profile: String,
    pub world_project: Option<String>,
}

impl Server {
    fn to_json(&self) -> Value {
        json!({ "name": self.name, "profile": self.profile, "worldProject": self.world_project })
    }

    pub fn read(dir: &Path) -> Option<Server> {
        let bytes = fs::read(dir.join(META_FILE)).ok()?;
        let meta: Value = serde_json::from_slice(super::key_value::strip_bom(&bytes)).ok()?;
        Some(Server {
            dir: dir.to_path_buf(),
            name: meta["name"].as_str()?.to_string(),
            profile: meta["profile"].as_str()?.to_string(),
            world_project: meta["worldProject"].as_str().map(String::from),
        })
    }

    pub fn write(&self) -> io::Result<()> {
        fs::write(self.dir.join(META_FILE), serde_json::to_string_pretty(&self.to_json()).unwrap_or_default())
    }
}

// The servers of a servers folder: every sub-folder with a readable Outpost Tools file, by folder name.
pub fn list_servers(servers_dir: &Path) -> Vec<Server> {
    let mut found: Vec<Server> = fs::read_dir(servers_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| Server::read(&e.path()))
        .collect();
    found.sort_by(|a, b| a.dir.cmp(&b.dir));
    found
}

// A folder name Windows accepts, made from what the user typed.
pub fn folder_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '-' } else { c })
        .take(60)
        .collect();
    // Windows drops a trailing dot or space, which would make two names one.
    let cleaned = cleaned.trim_end_matches(['.', ' ']).to_string();
    let stem = cleaned.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || ((stem.starts_with("COM") || stem.starts_with("LPT")) && stem.len() == 4 && stem[3..].chars().all(|c| ('1'..='9').contains(&c)));
    match cleaned {
        c if c.is_empty() => "server".into(),
        c if reserved => format!("{c}-server"),
        c => c,
    }
}

// ---- Mods that run on a server ----

// Modrinth's client_side / server_side values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    Required,
    Optional,
    Unsupported,
}

impl Support {
    pub fn parse(word: &str) -> Option<Support> {
        match word {
            "required" => Some(Support::Required),
            "optional" => Some(Support::Optional),
            "unsupported" => Some(Support::Unsupported),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sides {
    pub client: Support,
    pub server: Support,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileMod {
    pub name: String,
    // The mod's jar in the profile.
    pub file: PathBuf,
}

// What the Modrinth client will implement: None when the mod is not known there (a mod of the pack's own).
pub trait ModSides {
    fn sides(&self, m: &ProfileMod) -> Option<Sides>;
}

#[derive(Debug, Default, PartialEq)]
pub struct ModSelection {
    // Copied to the server.
    pub kept: Vec<ProfileMod>,
    // Left out: server side unsupported (Sodium and the like).
    pub left_out: Vec<ProfileMod>,
    // Kept without proof (a subset of `kept`): nothing said which side they run on.
    pub unchecked: Vec<ProfileMod>,
}

pub fn select_mods(mods: &[ProfileMod], sides: &dyn ModSides) -> ModSelection {
    let mut out = ModSelection::default();
    for m in mods {
        match sides.sides(m) {
            Some(s) if s.server == Support::Unsupported => out.left_out.push(m.clone()),
            Some(_) => out.kept.push(m.clone()),
            None => {
                out.kept.push(m.clone());
                out.unchecked.push(m.clone());
            }
        }
    }
    out
}

// ---- Making a server ----

pub struct NewServer<'a> {
    pub name: &'a str,
    pub profile: &'a str,
    pub world_project: Option<&'a str>,
}

pub struct Created {
    pub server: Server,
    pub mods: ModSelection,
}

// Makes the server's folder, its Outpost Tools file and its mods folder. The launcher jar is fetched by the caller
// (see fabric_server_url) and saved as LAUNCHER_JAR.
pub fn create_server(servers_dir: &Path, new: &NewServer, mods: &[ProfileMod], sides: &dyn ModSides) -> io::Result<Created> {
    let name = new.name.trim();
    if name.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "A server needs a name."));
    }
    fs::create_dir_all(servers_dir)?;
    let base = folder_name(name);
    let mut dir = servers_dir.join(&base);
    let mut n = 2;
    // create_dir fails when the folder exists: no server is overwritten, even under a race.
    while let Err(e) = fs::create_dir(&dir) {
        if e.kind() != io::ErrorKind::AlreadyExists {
            return Err(e);
        }
        dir = servers_dir.join(format!("{base}-{n}"));
        n += 1;
    }
    let selection = select_mods(mods, sides);
    let mods_dir = dir.join("mods");
    fs::create_dir(&mods_dir)?;
    for m in &selection.kept {
        let file_name = m.file.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "A mod has no file name."))?;
        fs::copy(&m.file, mods_dir.join(file_name))?;
    }
    let server = Server {
        dir,
        name: name.to_string(),
        profile: new.profile.to_string(),
        world_project: new.world_project.map(String::from),
    };
    server.write()?;
    Ok(Created { server, mods: selection })
}

// ---- The Fabric server launcher, from Fabric's meta API ----

pub const FABRIC_INSTALLERS_URL: &str = "https://meta.fabricmc.net/v2/versions/installer";

pub fn fabric_loaders_url(game_version: &str) -> Result<String, String> {
    if game_version.is_empty() || !game_version.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
        return Err(format!("{game_version:?} is not a game version."));
    }
    Ok(format!("https://meta.fabricmc.net/v2/versions/loader/{game_version}"))
}

fn first_stable(list: &[u8], version_of: impl Fn(&Value) -> &Value, stable_of: impl Fn(&Value) -> &Value) -> Option<String> {
    let list: Value = serde_json::from_slice(list).ok()?;
    list.as_array()?
        .iter()
        .find(|e| stable_of(e).as_bool() == Some(true))
        .and_then(|e| version_of(e).as_str().map(String::from))
}

// The URL of the server launcher jar for a game version, from the two lists Fabric's meta API answers (newest
// first): the loaders of that game version and the installers. Nothing is fetched here.
pub fn fabric_server_url(game_version: &str, loaders_json: &[u8], installers_json: &[u8]) -> Result<String, String> {
    fabric_loaders_url(game_version)?;
    let loader = first_stable(loaders_json, |e| &e["loader"]["version"], |e| &e["loader"]["stable"])
        .ok_or("Fabric has no stable loader for this game version.")?;
    let installer = first_stable(installers_json, |e| &e["version"], |e| &e["stable"]).ok_or("Fabric lists no stable installer.")?;
    Ok(format!("https://meta.fabricmc.net/v2/versions/loader/{game_version}/{loader}/{installer}/server/jar"))
}

// ---- The EULA ----

// The line the user's Accept writes in eula.txt. Nothing here writes it: only the user's action does.
pub fn eula_text() -> &'static str {
    "eula=true"
}

pub fn eula_file(server_dir: &Path) -> PathBuf {
    server_dir.join("eula.txt")
}

// Whether a eula.txt's text holds the accepting line.
pub fn eula_text_accepts(text: &str) -> bool {
    text.lines().any(|l| l.trim_end() == eula_text())
}

pub fn eula_accepted(server_dir: &Path) -> bool {
    fs::read_to_string(eula_file(server_dir)).is_ok_and(|t| eula_text_accepts(&t))
}

// ---- server.properties ----

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub view_distance: u32,
    pub simulation_distance: u32,
    pub max_players: u32,
    pub port: u16,
    pub motd: String,
    pub whitelist: bool,
}

const VIEW: (u32, u32) = (2, 32);
const SIMULATION: (u32, u32) = (3, 32);
const PLAYERS: (u32, u32) = (1, 100);

pub fn props_file(server_dir: &Path) -> PathBuf {
    server_dir.join("server.properties")
}

// None until the server has started once: the game writes server.properties then.
pub fn read_settings(server_dir: &Path) -> Option<Settings> {
    let props = Props::read(&props_file(server_dir))?;
    let number = |key: &str, default: u32| props.get(key).and_then(|v| v.trim().parse().ok()).unwrap_or(default);
    Some(Settings {
        view_distance: number("view-distance", 10),
        simulation_distance: number("simulation-distance", 10),
        max_players: number("max-players", 20),
        port: props.get("server-port").and_then(|v| v.trim().parse().ok()).unwrap_or(DEFAULT_PORT),
        motd: props.get("motd").unwrap_or_default(),
        whitelist: props.get("white-list").is_some_and(|v| v.trim() == "true"),
    })
}

pub fn write_settings(server_dir: &Path, s: &Settings) -> Result<(), String> {
    let within = |name: &str, v: u32, (lo, hi): (u32, u32)| {
        if (lo..=hi).contains(&v) { Ok(()) } else { Err(format!("{name} must be between {lo} and {hi}.")) }
    };
    within("View distance", s.view_distance, VIEW)?;
    within("Simulation distance", s.simulation_distance, SIMULATION)?;
    within("Players at most", s.max_players, PLAYERS)?;
    if s.port < 1024 {
        return Err("The port must be between 1024 and 65535.".into());
    }
    // A line break would end the value early and start a line of its own.
    if s.motd.chars().any(char::is_control) {
        return Err("The line in the server list cannot hold a line break.".into());
    }
    let file = props_file(server_dir);
    let mut props = Props::read(&file).ok_or("The settings appear after the first start.")?;
    props.set("view-distance", &s.view_distance.to_string());
    props.set("simulation-distance", &s.simulation_distance.to_string());
    props.set("max-players", &s.max_players.to_string());
    props.set("server-port", &s.port.to_string());
    props.set("motd", &s.motd);
    props.set("white-list", if s.whitelist { "true" } else { "false" });
    props.write(&file).map_err(|e| e.to_string())
}

// Whether nothing listens on the port yet (the server binds all addresses).
pub fn port_is_free(port: u16) -> bool {
    TcpListener::bind(("0.0.0.0", port)).is_ok()
}

// ---- Start and stop ----

pub struct LaunchCommand {
    pub java: PathBuf,
    pub args: Vec<String>,
    pub dir: PathBuf,
}

pub fn launch_command(java: &Path, server_dir: &Path, memory_mb: u32, java_args: &[String]) -> LaunchCommand {
    let mut args = vec![format!("-Xmx{memory_mb}M")];
    args.extend(java_args.iter().cloned());
    args.extend(["-jar".into(), LAUNCHER_JAR.into(), "nogui".into()]);
    LaunchCommand { java: java.to_path_buf(), args, dir: server_dir.to_path_buf() }
}

// A running server, as far as starting and stopping need it.
pub trait ServerProcess {
    fn send_line(&mut self, line: &str) -> io::Result<()>;
    fn has_exited(&mut self) -> io::Result<bool>;
    fn kill(&mut self) -> io::Result<()>;
}

pub trait Spawner {
    fn spawn(&self, command: &LaunchCommand) -> io::Result<Box<dyn ServerProcess>>;
}

struct JavaProcess {
    child: Child,
}

impl ServerProcess for JavaProcess {
    fn send_line(&mut self, line: &str) -> io::Result<()> {
        use io::Write;
        let stdin = self.child.stdin.as_mut().ok_or_else(|| io::Error::from(io::ErrorKind::BrokenPipe))?;
        writeln!(stdin, "{line}")?;
        stdin.flush()
    }
    fn has_exited(&mut self) -> io::Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }
    fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }
}

// Starts java with no window. The server writes its own log (logs/latest.log), so its output is dropped.
pub struct JavaSpawner;

impl Spawner for JavaSpawner {
    fn spawn(&self, command: &LaunchCommand) -> io::Result<Box<dyn ServerProcess>> {
        let child = crate::system::hidden(&command.java)
            .args(&command.args)
            .current_dir(&command.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Box::new(JavaProcess { child }))
    }
}

#[derive(Debug)]
pub enum StartError {
    EulaNotAccepted,
    JavaMissing(PathBuf),
    LauncherMissing,
    PortTaken(u16),
    Spawn(io::Error),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartError::EulaNotAccepted => write!(f, "The Minecraft EULA is not accepted."),
            StartError::JavaMissing(p) => write!(f, "Java is missing: {} does not exist.", p.display()),
            StartError::LauncherMissing => write!(f, "The server's launcher file is missing."),
            StartError::PortTaken(p) => write!(f, "Port {p} is already in use."),
            StartError::Spawn(e) => write!(f, "The server did not start: {e}"),
        }
    }
}

// Starts a server unless something the user must settle first is missing. Nothing starts before the EULA is accepted.
pub fn start_server(spawner: &dyn Spawner, server_dir: &Path, java: &Path, memory_mb: u32, java_args: &[String]) -> Result<Box<dyn ServerProcess>, StartError> {
    start_checked(spawner, server_dir, java, memory_mb, java_args, eula_accepted(server_dir))
}

// The EULA's answer is a parameter so a test can check the other refusals without a eula.txt on disk.
fn start_checked(
    spawner: &dyn Spawner,
    server_dir: &Path,
    java: &Path,
    memory_mb: u32,
    java_args: &[String],
    eula_ok: bool,
) -> Result<Box<dyn ServerProcess>, StartError> {
    if !eula_ok {
        return Err(StartError::EulaNotAccepted);
    }
    if !java.is_file() {
        return Err(StartError::JavaMissing(java.to_path_buf()));
    }
    if !server_dir.join(LAUNCHER_JAR).is_file() {
        return Err(StartError::LauncherMissing);
    }
    let port = read_settings(server_dir).map_or(DEFAULT_PORT, |s| s.port);
    if !port_is_free(port) {
        return Err(StartError::PortTaken(port));
    }
    spawner.spawn(&launch_command(java, server_dir, memory_mb, java_args)).map_err(StartError::Spawn)
}

#[derive(Debug, PartialEq, Eq)]
pub enum StopOutcome {
    // It ended on its own after "stop", having saved.
    Saved,
    // It did not end in time and was killed: the last minutes may be lost.
    Killed,
}

// Sends "stop" and waits for the server to save and end; kills it when `timeout` passes.
pub fn stop_server(process: &mut dyn ServerProcess, timeout: Duration, poll: Duration) -> io::Result<StopOutcome> {
    if process.has_exited()? {
        return Ok(StopOutcome::Saved);
    }
    // A failed write means the pipe is gone, so the server is ending anyway: the wait below decides.
    let _ = process.send_line("stop");
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if process.has_exited()? {
            return Ok(StopOutcome::Saved);
        }
        std::thread::sleep(poll);
    }
    process.kill()?;
    Ok(StopOutcome::Killed)
}

// "Done (" is the line Minecraft writes once players can join.
pub fn log_says_ready(log: &str) -> bool {
    log.contains("Done (")
}

// Whether the server started at or after `since` is ready: latest.log is rewritten at each start, so an older file
// is the last run's.
pub fn is_ready(server_dir: &Path, since: SystemTime) -> bool {
    let log = server_dir.join("logs").join("latest.log");
    // A file's write time can read slightly earlier than a clock reading taken just before the write
    // (coarse timestamps on Windows), so a short margin keeps this run's log from being taken for an old one.
    let since = since.checked_sub(Duration::from_secs(2)).unwrap_or(since);
    fs::metadata(&log).and_then(|m| m.modified()).is_ok_and(|t| t >= since)
        && fs::read_to_string(&log).is_ok_and(|text| log_says_ready(&text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::atomic::{AtomicU32, Ordering},
    };

    fn temp_dir(name: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("servers_{name}_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    struct Table(Vec<(&'static str, Option<(Support, Support)>)>);
    impl ModSides for Table {
        fn sides(&self, m: &ProfileMod) -> Option<Sides> {
            let (_, s) = self.0.iter().find(|(n, _)| *n == m.name)?;
            s.map(|(client, server)| Sides { client, server })
        }
    }

    fn mod_in(dir: &Path, name: &str) -> ProfileMod {
        let file = dir.join(format!("{name}.jar"));
        fs::write(&file, name.as_bytes()).unwrap();
        ProfileMod { name: name.into(), file }
    }

    #[test]
    fn folder_names_are_valid_on_windows() {
        assert_eq!(folder_name("  My: world?  "), "My- world-");
        assert_eq!(folder_name("..."), "server");
        assert_eq!(folder_name("con"), "con-server");
        assert_eq!(folder_name("LPT1.txt"), "LPT1.txt-server");
        assert_eq!(folder_name("Comet"), "Comet");
        assert_eq!(folder_name(&"a".repeat(100)).len(), 60);
    }

    #[test]
    fn mods_that_do_not_run_on_a_server_stay_out() {
        use Support::*;
        let dir = temp_dir("sel");
        let table = Table(vec![
            ("sodium", Some((Required, Unsupported))),
            ("lithium", Some((Optional, Required))),
            ("fabric-api", Some((Required, Required))),
            ("ours", None),
        ]);
        let mods: Vec<_> = ["sodium", "lithium", "fabric-api", "ours"].iter().map(|n| mod_in(&dir, n)).collect();
        let sel = select_mods(&mods, &table);
        let names = |v: &[ProfileMod]| v.iter().map(|m| m.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&sel.kept), ["lithium", "fabric-api", "ours"]);
        assert_eq!(names(&sel.left_out), ["sodium"]);
        assert_eq!(names(&sel.unchecked), ["ours"]);
        assert_eq!(Support::parse("unsupported"), Some(Unsupported));
        assert_eq!(Support::parse("nope"), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_server_is_made_from_a_profile_and_listed() {
        use Support::*;
        let profile = temp_dir("profile");
        let root = temp_dir("root");
        let servers = root.join("servers");
        let table = Table(vec![("sodium", Some((Required, Unsupported))), ("lithium", Some((Optional, Required)))]);
        let mods = [mod_in(&profile, "sodium"), mod_in(&profile, "lithium")];
        let new = NewServer { name: "Friends: Fall", profile: "Pack", world_project: Some("island") };
        let made = create_server(&servers, &new, &mods, &table).unwrap();
        assert_eq!(made.server.dir, servers.join("Friends- Fall"));
        assert!(made.server.dir.join("mods").join("lithium.jar").is_file());
        assert!(!made.server.dir.join("mods").join("sodium.jar").exists());
        // The same name again gets a folder of its own.
        let again = create_server(&servers, &NewServer { name: "Friends: Fall", profile: "Pack", world_project: None }, &[], &table).unwrap();
        assert_eq!(again.server.dir, servers.join("Friends- Fall-2"));
        // A folder with no Outpost Tools file is not a server.
        fs::create_dir(servers.join("stray")).unwrap();
        let listed = list_servers(&servers);
        assert_eq!(listed, vec![made.server.clone(), again.server.clone()]);
        assert_eq!(listed[0].name, "Friends: Fall");
        assert_eq!(listed[0].world_project.as_deref(), Some("island"));
        assert_eq!(listed[1].world_project, None);
        assert!(create_server(&servers, &NewServer { name: "  ", profile: "p", world_project: None }, &[], &table).is_err());
        // Making a server writes no eula.txt: only the user's Accept does.
        assert!(!eula_file(&made.server.dir).exists());
        let _ = fs::remove_dir_all(profile);
        let _ = fs::remove_dir_all(root);
    }

    const LOADERS: &str = r#"[
      {"loader":{"separator":".","build":1,"maven":"net.fabricmc:fabric-loader:0.20.2-beta.1","version":"0.20.2-beta.1","stable":false},"intermediary":{"version":"1.21.1","stable":true}},
      {"loader":{"separator":".","build":1,"maven":"net.fabricmc:fabric-loader:0.20.1","version":"0.20.1","stable":true},"intermediary":{"version":"1.21.1","stable":true}},
      {"loader":{"version":"0.19.0","stable":true}}]"#;
    const INSTALLERS: &str = r#"[
      {"url":"https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.2.0-rc.1/fabric-installer-1.2.0-rc.1.jar","version":"1.2.0-rc.1","stable":false},
      {"url":"https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.1.2/fabric-installer-1.1.2.jar","version":"1.1.2","stable":true},
      {"version":"1.1.1","stable":true}]"#;

    #[test]
    fn the_fabric_launcher_url_takes_the_newest_stable_pair() {
        assert_eq!(
            fabric_server_url("1.21.1", LOADERS.as_bytes(), INSTALLERS.as_bytes()).unwrap(),
            "https://meta.fabricmc.net/v2/versions/loader/1.21.1/0.20.1/1.1.2/server/jar"
        );
        assert_eq!(fabric_loaders_url("1.21.1").unwrap(), "https://meta.fabricmc.net/v2/versions/loader/1.21.1");
        assert!(fabric_loaders_url("1/../x").is_err());
        assert!(fabric_server_url("1.21.1", b"[]", INSTALLERS.as_bytes()).is_err());
        assert!(fabric_server_url("1.21.1", LOADERS.as_bytes(), b"not json").is_err());
    }

    #[test]
    fn the_eula_is_a_text_that_only_the_users_accept_writes() {
        assert_eq!(eula_text(), "eula=true");
        assert!(eula_text_accepts("#By changing the setting below...\r\neula=true\r\n"));
        assert!(!eula_text_accepts("eula=false\n"));
        // A folder with no eula.txt has not accepted.
        let dir = temp_dir("eula");
        assert!(!eula_accepted(&dir));
        assert!(!eula_file(&dir).exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn settings_are_read_and_written_in_server_properties() {
        let dir = temp_dir("props");
        let s = Settings { view_distance: 12, simulation_distance: 8, max_players: 6, port: 25570, motd: "Hello".into(), whitelist: true };
        // Before the first start there is no file: nothing to read, nothing written.
        assert_eq!(read_settings(&dir), None);
        assert!(write_settings(&dir, &s).is_err());
        fs::write(props_file(&dir), "#Minecraft server properties\nlevel-name=world\nview-distance=10\nmotd=A Minecraft Server\n").unwrap();
        let before = read_settings(&dir).unwrap();
        assert_eq!((before.view_distance, before.port, before.whitelist), (10, 25565, false));
        write_settings(&dir, &s).unwrap();
        assert_eq!(read_settings(&dir), Some(s.clone()));
        // The keys we do not know stay.
        assert!(fs::read_to_string(props_file(&dir)).unwrap().contains("level-name=world"));
        for bad in [
            Settings { view_distance: 1, ..s.clone() },
            Settings { view_distance: 33, ..s.clone() },
            Settings { simulation_distance: 2, ..s.clone() },
            Settings { max_players: 0, ..s.clone() },
            Settings { port: 80, ..s.clone() },
            Settings { motd: "a\nb".into(), ..s.clone() },
        ] {
            assert!(write_settings(&dir, &bad).is_err());
        }
        assert_eq!(read_settings(&dir), Some(s));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_launch_command_is_java_with_the_launcher_jar() {
        let c = launch_command(Path::new("java.exe"), Path::new("srv"), 4096, &["-XX:+UseZGC".to_string()]);
        assert_eq!(c.args, ["-Xmx4096M", "-XX:+UseZGC", "-jar", "fabric-server-launcher.jar", "nogui"]);
    }

    // A process that ends `after` polls after "stop" (never when None).
    struct Fake {
        lines: Rc<RefCell<Vec<String>>>,
        killed: Rc<RefCell<bool>>,
        after: Option<u32>,
        stopped_at: Option<u32>,
        polls: u32,
    }
    impl ServerProcess for Fake {
        fn send_line(&mut self, line: &str) -> io::Result<()> {
            self.lines.borrow_mut().push(line.into());
            if line == "stop" {
                self.stopped_at = Some(self.polls);
            }
            Ok(())
        }
        fn has_exited(&mut self) -> io::Result<bool> {
            self.polls += 1;
            Ok(*self.killed.borrow() || matches!((self.after, self.stopped_at), (Some(a), Some(s)) if self.polls >= s + a))
        }
        fn kill(&mut self) -> io::Result<()> {
            *self.killed.borrow_mut() = true;
            Ok(())
        }
    }
    fn fake(after: Option<u32>) -> (Fake, Rc<RefCell<Vec<String>>>, Rc<RefCell<bool>>) {
        let lines = Rc::new(RefCell::new(vec![]));
        let killed = Rc::new(RefCell::new(false));
        (Fake { lines: lines.clone(), killed: killed.clone(), after, stopped_at: None, polls: 0 }, lines, killed)
    }

    #[test]
    fn stop_sends_stop_and_waits_for_the_save() {
        let (mut p, lines, killed) = fake(Some(3));
        let out = stop_server(&mut p, Duration::from_secs(5), Duration::from_millis(1)).unwrap();
        assert_eq!(out, StopOutcome::Saved);
        assert_eq!(*lines.borrow(), ["stop"]);
        assert!(!*killed.borrow());
    }

    #[test]
    fn stop_kills_a_server_that_does_not_end_in_time() {
        let (mut p, _, killed) = fake(None);
        let out = stop_server(&mut p, Duration::from_millis(30), Duration::from_millis(5)).unwrap();
        assert_eq!(out, StopOutcome::Killed);
        assert!(*killed.borrow());
    }

    struct FakeSpawner(Rc<RefCell<Vec<Vec<String>>>>);
    impl Spawner for FakeSpawner {
        fn spawn(&self, c: &LaunchCommand) -> io::Result<Box<dyn ServerProcess>> {
            self.0.borrow_mut().push(c.args.clone());
            Ok(Box::new(fake(Some(1)).0))
        }
    }

    #[test]
    fn start_refuses_until_everything_is_settled() {
        let dir = temp_dir("start");
        let java = dir.join("java.exe");
        let calls = Rc::new(RefCell::new(vec![]));
        let spawner = FakeSpawner(calls.clone());
        // No eula.txt in the folder: the public start refuses.
        assert!(matches!(start_server(&spawner, &dir, &java, 2048, &[]).err(), Some(StartError::EulaNotAccepted)));
        assert!(!eula_file(&dir).exists());
        let go = |java: &Path| start_checked(&spawner, &dir, java, 2048, &[], true).err();
        assert!(matches!(go(&java), Some(StartError::JavaMissing(_))));
        fs::write(&java, b"").unwrap();
        assert!(matches!(go(&java), Some(StartError::LauncherMissing)));
        fs::write(dir.join(LAUNCHER_JAR), b"").unwrap();
        // A taken port is refused.
        let held = TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        fs::write(props_file(&dir), format!("server-port={port}\n")).unwrap();
        assert!(matches!(go(&java), Some(StartError::PortTaken(p)) if p == port));
        assert!(calls.borrow().is_empty());
        drop(held);
        assert!(go(&java).is_none());
        assert_eq!(calls.borrow().len(), 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ready_is_the_done_line_of_this_run() {
        assert!(log_says_ready("[12:00:00] [Server thread/INFO]: Done (4.2s)! For help, type \"help\"\n"));
        assert!(!log_says_ready("[12:00:00] Preparing spawn area: 50%\n"));
        let dir = temp_dir("ready");
        let before = SystemTime::now();
        assert!(!is_ready(&dir, before));
        fs::create_dir(dir.join("logs")).unwrap();
        fs::write(dir.join("logs").join("latest.log"), "Done (1s)!").unwrap();
        assert!(is_ready(&dir, before));
        assert!(!is_ready(&dir, SystemTime::now() + Duration::from_secs(60)));
        let _ = fs::remove_dir_all(dir);
    }
}
