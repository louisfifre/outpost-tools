//! Every sentence Home shows, as the page's `text` object says it (index.html's `const text`), with the
//! key it comes from. The page's sentences are the source of truth: change them there first.

/// `noAccount`.
pub const NO_ACCOUNT: &str = "No account: sign in once in Prism";
/// `profilesGroup`.
pub const PROFILES_GROUP: &str = "Profiles";
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
pub const SERVER_GROUP: &str = "Server";
pub const SERVER_NAME: &str = "Dev server";
pub const START_SERVER: &str = "Start the server";
pub const STOP_SERVER: &str = "Stop the server";
/// `server`, by state; None for a word the page has no sentence for.
pub fn server(state: &str, port: i64) -> Option<String> {
    Some(match state {
        "stopped" => format!("stopped \u{b7} joins at localhost:{port}"),
        "preparing" => "Syncing the mods\u{2026} its window shows each step".into(),
        "starting" => "Starting the world\u{2026}".into(),
        "running" => format!("Running \u{b7} localhost:{port}"),
        "failed" => "The start failed. Its window says why.".into(),
        "stopping" => "Saving and stopping\u{2026}".into(),
        _ => return None,
    })
}
pub const NO_EULA: &str = "EULA not accepted yet";
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
