//! Every sentence Home shows, as the page's `text` object says it (index.html's `const text`), with the
//! key it comes from. The page's sentences are the source of truth: change them there first.

/// `noAccount`.
pub const NO_ACCOUNT: &str = "No account: sign in once in Prism";
/// `profilesGroup`.
pub const PROFILES_GROUP: &str = "Profiles";
/// The action at the head of the profiles, and of the servers (board A1).
pub const NEW_PROFILE: &str = "New profile";
pub const NEW_SERVER: &str = "New server";
/// The title of the page that adds a mod to a profile (board B4).
pub fn add_mod_title(profile: &str) -> String {
    format!("Add a mod to {profile}")
}
/// The search field, the file action and the words of a result line (board B4).
pub const SEARCH_MODRINTH: &str = "Search Modrinth";
pub const ADD_A_FILE: &str = "Add a file";
pub const ADD: &str = "Add";
pub const ADDING: &str = "Adding\u{2026}";
pub const UPDATE: &str = "Update";
pub const UPDATING: &str = "Updating\u{2026}";
pub const ADDED: &str = "Added";
pub const OPEN_ON_MODRINTH: &str = "Open on Modrinth";
pub const NO_MODS: &str = "No mod matches.";
/// The version a mod would be updated to: an arrow and the version.
pub fn new_version(version: &str) -> String {
    format!("\u{2192} {version}")
}
/// Under the line of a mod that was added: what came with it.
pub fn came_with(dependency: &str, by: &str) -> String {
    format!("{dependency} came with it: {by} needs it.")
}
/// The word of the Play action on a profile's page, after its icon.
pub const PLAY: &str = "Play";
/// `play`.
pub fn play(name: &str) -> String {
    format!("Play {name}")
}
/// `settings`.
pub fn settings(name: &str) -> String {
    format!("Settings of {name}")
}
/// `back`.
pub const BACK: &str = "Back";
/// `openOn`, and the keys after it in their order.
pub const OPEN_ON: &str = "Opens on";
pub const ON_MENU: &str = "the game menu";
pub fn on_world(name: &str) -> String {
    format!("world: {name}")
}
pub const ON_SERVER: &str = "a server";
pub const SERVER_ADDRESS: &str = "address, for example localhost";
pub const MEMORY: &str = "Memory";
pub const GIGABYTES: &str = "GB";
pub const JAVA_ARGS: &str = "Java arguments";
pub const FOLDER: &str = "Game folder";
pub const SERVER_FOLDER: &str = "Server folder";
pub const OPEN: &str = "Open";
pub const SAVED: &str = "Saved.";
pub const REFUSED: &str = "Setting refused, nothing changed.";
pub const LOCKED: &str = "Minecraft is running: settings can change once it is closed.";
pub const FAILED: &str = "The request failed.";
/// `launch`: what a launch is doing, by the word the server answers. `seconds` is counted by the caller.
pub fn launch(state: &str, seconds: i64) -> Option<String> {
    Some(match state {
        "starting" => "Starting\u{2026}".into(),
        "preparing" => "Syncing the pack\u{2026}".into(),
        "loading" => format!("Loading the game\u{2026} {seconds} s"),
        "running" => "Running".into(),
        "waiting" => "Prism is waiting for you: look at its window.".into(),
        "prepareFailed" => "The pack sync failed. Prism\u{2019}s window says why.".into(),
        "stopped" => "The game stopped before its window opened. Prism shows its log.".into(),
        "failed" => "Prism closed without starting the game.".into(),
        _ => return None,
    })
}
/// `joinsServer`, `opensWorld`, `playedHours`, `playedMinutes` and `neverPlayed` are in model.rs (`about`).
pub const SERVER_GROUP: &str = "Servers";
pub const SERVER_NAME: &str = "Dev server";
pub const START_SERVER: &str = "Start the server";
pub const STOP_SERVER: &str = "Stop the server";
/// `server`, by state; None for a word the page has no sentence for.
pub fn server(state: &str, port: i64) -> Option<String> {
    Some(match state {
        "stopped" => format!("stopped \u{b7} joins at localhost:{port}"),
        "preparing" => "Syncing the mods\u{2026} its window shows each step".into(),
        "starting" => "Starting the world\u{2026}".into(),
        "running" => format!("running \u{b7} localhost:{port}"),
        "failed" => "The start failed. Its window says why.".into(),
        "stopping" => "Saving and stopping\u{2026}".into(),
        _ => return None,
    })
}
pub const NO_EULA: &str = "EULA not accepted yet";
/// The server's page before its first start (board C1). The sentence is said in four pieces, the link between them.
pub const START: &str = "Start";
pub const EULA_BEFORE: &str = "To start this server, accept the ";
pub const EULA_LINK: &str = "Minecraft EULA";
pub const EULA_URL: &str = "https://aka.ms/MinecraftEULA";
pub const EULA_AFTER: &str = ". ";
pub const EULA_WRITES: &str = "It writes eula=true in its eula.txt.";
pub const ACCEPT: &str = "Accept";
pub const MADE_FROM: &str = "Made from";
pub const MADE_PROFILE: &str = "Profile";
pub const MADE_WORLD: &str = "World";
pub const MADE_FOLDER: &str = "Folder";
pub const NO_WORLD: &str = "No world project";
/// What the server leaves out of its profile's mods; none for a profile whose mods all run on a server.
pub fn left_out(n: i64) -> Option<String> {
    match n {
        n if n <= 0 => None,
        1 => Some("1 mod not needed on a server".into()),
        n => Some(format!("{n} mods not needed on a server")),
    }
}
/// The world project's name and its seed.
pub fn world_seed(world: &str, seed: i64) -> String {
    format!("{world} \u{b7} seed {seed}")
}
pub const VIEW_DISTANCE: &str = "View distance (chunks)";
pub const SIMULATION_DISTANCE: &str = "Simulation distance";
pub const MAX_PLAYERS: &str = "Players at most";
pub const PORT: &str = "Port";
pub const MOTD: &str = "Line in the server list";
pub const WHITELIST: &str = "Only players on the whitelist";
pub const ON: &str = "on";
pub const OFF: &str = "off";
pub const FIRST_START: &str = "Distances, players, port and line appear after the first start.";
pub const SERVER_LOCKED: &str = "The server is running: settings can change once it is stopped.";
pub const SERVER_SETTINGS: &str = "Settings of the server";
// The profile page (boards B1 to B3).
pub const GAME_SECTION: &str = "Game";
pub fn mods_section(count: usize) -> String {
    format!("Mods \u{b7} {count}")
}
pub const GAME_VERSION: &str = "Game version";
pub const FABRIC: &str = "Fabric";
pub const NOT_INSTALLED: &str = "not installed";
pub const INSTALL: &str = "Install";
pub fn update_all(count: usize) -> String {
    format!("Update all ({count})")
}
pub const ADD_A_MOD: &str = "Add a mod";
pub const UNDO: &str = "Undo";
pub fn removed(name: &str) -> String {
    format!("{name} removed.")
}
pub fn remove_mod(name: &str) -> String {
    format!("Remove {name}")
}
pub fn open_on_modrinth(name: &str) -> String {
    format!("Open {name} on Modrinth")
}
/// The arrow before a newer version.
pub fn newer(version: &str) -> String {
    format!("\u{2192} {version}")
}
pub fn memory_min(gb: i64) -> String {
    format!("min {gb} GB")
}
pub fn memory_max(gb: i64) -> String {
    format!("max {gb} GB")
}
pub const USED_BY_PC: &str = "used by the PC";
pub fn memory_total(gb: i64) -> String {
    format!("{gb} GB")
}
pub const USED_TIP: &str = "Used now by Windows and the open programs";
/// The first start's guide (boards E1 and E2): its three steps, then the Java step's sentences.
pub const SETUP_STEPS: [&str; 3] = ["Java", "Prism", "Sign in"];
pub const JAVA_NEEDED: &str = "Outpost Tools needs Java 25 to run the game.";
pub fn java_download(mb: Option<u64>) -> String {
    match mb {
        Some(mb) => format!("Download Java 25 \u{b7} {mb} MB"),
        None => "Download Java 25".into(),
    }
}
pub const JAVA_FROM: &str = "from Adoptium, into Outpost Tools' folder";
pub const JAVA_LATER: &str = "Later";
pub const JAVA_DOWNLOADING: &str = "Downloading Java 25.";
pub fn java_progress(done_mb: u64, size_mb: u64) -> String {
    format!("{done_mb} of {size_mb} MB")
}
pub const JAVA_CHECKING: &str = "Checking the download";
pub const JAVA_UNPACKING: &str = "Unpacking Java 25";
pub const JAVA_STOP: &str = "Stop";
pub const JAVA_NOTHING_KEPT: &str = "Nothing is kept.";
pub fn java_ready(version: &str) -> String {
    format!("Java {version} is ready.")
}
