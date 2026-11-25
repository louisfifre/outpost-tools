//! A stand-in for the api, for tests and for `--shot`: it answers from a table and remembers what it was asked.
//! Nothing here starts a process or reads a file.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde_json::{Value, json};

use crate::bridge::{Answer, Backend};

/// 2026-11-15 12:00 UTC by the brief's own words (the number lies on 2026-11-21: only the days between it and the
/// last play count, so the pictures do not depend on the date).
pub const FIXED_NOW_MS: i64 = 1_795_262_400_000;

#[derive(Default)]
pub struct FakeApi {
    /// The status and JSON a GET answers, by path; 404 and {} when not set.
    gets: Mutex<BTreeMap<String, (u16, Value)>>,
    /// The status and body a POST answers, by path; 200 and {} when not set.
    posts: Mutex<BTreeMap<String, (u16, Value)>>,
    /// Every request: method, path, body text.
    asked: Mutex<Vec<(String, String, String)>>,
}

impl FakeApi {
    pub fn set_get(&self, path: &str, body: Value) {
        self.set_get_status(path, 200, body);
    }
    /// A GET that answers status, as the api does when it cannot answer (a 503 with its reason).
    pub fn set_get_status(&self, path: &str, status: u16, body: Value) {
        self.gets.lock().unwrap().insert(path.to_string(), (status, body));
    }
    pub fn set_post(&self, path: &str, status: u16, body: Value) {
        self.posts.lock().unwrap().insert(path.to_string(), (status, body));
    }
    pub fn asked(&self) -> Vec<(String, String, String)> {
        self.asked.lock().unwrap().clone()
    }
    /// How many times `method path` was asked.
    pub fn count(&self, method: &str, path: &str) -> usize {
        self.asked.lock().unwrap().iter().filter(|(m, p, _)| m == method && p == path).count()
    }
    /// The body of the last `method path`, as JSON.
    pub fn last_body(&self, method: &str, path: &str) -> Option<Value> {
        self.asked.lock().unwrap().iter().rev().find(|(m, p, _)| m == method && p == path).map(|(_, _, b)| serde_json::from_str(b).unwrap_or(Value::Null))
    }
}

impl Backend for FakeApi {
    fn ask(&self, method: &str, path: &str, body: &[u8]) -> Answer {
        self.asked.lock().unwrap().push((method.to_string(), path.to_string(), String::from_utf8_lossy(body).into_owned()));
        let (status, value) = if method == "GET" {
            self.gets.lock().unwrap().get(path).cloned().unwrap_or((404, json!({})))
        } else {
            self.posts.lock().unwrap().get(path).cloned().unwrap_or((200, json!({})))
        };
        Answer { status, body: value.to_string().into_bytes() }
    }
}

/// The two profiles and the stopped server of the pictures and the tests.
pub fn fixture() -> FakeApi {
    let api = FakeApi::default();
    api.set_get(
        "/api/profiles",
        json!({
            "machineMb": 32768, "account": "PlayerOne", "locked": false,
            "profiles": [
                {"id": "atelier", "name": "Atelier", "memoryMb": 6144, "javaArgs": "-XX:+UseZGC", "gameDir": "C:\\Code\\atelier",
                 "lastPlayed": 0, "playedSeconds": 0, "running": false, "worlds": [], "open": {"kind": "menu", "target": ""}},
                {"id": "exploration", "name": "Exploration", "memoryMb": 6144, "javaArgs": "-XX:+UseZGC", "gameDir": "C:\\Code\\exploration",
                 "lastPlayed": FIXED_NOW_MS - 86_400_000, "playedSeconds": 6120, "running": false, "worlds": ["New World"],
                 "open": {"kind": "world", "target": "New World"}}
            ]
        }),
    );
    api.set_get(
        "/api/server",
        json!({
            "dir": "C:\\Code\\server", "state": "stopped", "eula": true, "memoryMb": 6144, "javaArgs": "-XX:+UseZGC",
            "props": {"motd": "Exploration (dev)", "whitelist": true, "viewDistance": 12, "simulationDistance": 8, "maxPlayers": 20, "port": 25565}
        }),
    );
    api.set_get("/api/launch", json!({"id": null, "state": "none"}));
    api.set_get("/api/mods/exploration", exploration_mods());
    api.set_get("/api/mods/atelier", json!({"gameVersion": "26.3", "gameVersions": ["26.3", "26.2", "26.1"], "fabric": null, "mods": []}));
    api
}

/// The profile page's board (B1): Exploration's game version, Fabric with a newer one, and its fourteen mods, three of
/// them with a newer version and the last a file dropped in by hand (no page, no update).
pub fn exploration_mods() -> Value {
    let list = [
        ("sodium", "Sodium", "0.7.1", Some("0.7.3")),
        ("lithium", "Lithium", "0.16.0", Some("0.16.2")),
        ("distanthorizons", "Distant Horizons", "2.4.0", Some("2.4.1")),
        ("tectonic", "Tectonic", "3.1.0", None),
        ("fabric-api", "Fabric API", "0.140.0", None),
        ("iris", "Iris Shaders", "1.9.6", None),
        ("modmenu", "Mod Menu", "15.0.0", None),
        ("xaeros-minimap", "Xaero's Minimap", "25.2.0", None),
        ("xaeros-world-map", "Xaero's World Map", "1.39.2", None),
        ("entityculling", "Entity Culling", "1.7.4", None),
        ("ferrite-core", "FerriteCore", "8.0.0", None),
        ("modernfix", "ModernFix", "5.21.0", None),
        ("sodium-extra", "Sodium Extra", "0.6.4", None),
    ];
    let mut mods: Vec<Value> = list.iter().map(|(id, name, version, latest)| json!({"id": id, "slug": id, "name": name, "version": version, "latest": latest})).collect();
    mods.push(json!({"id": "my-local-mod", "name": "my-local-mod.jar", "version": ""}));
    json!({
        "gameVersion": "26.3", "gameVersions": ["26.3", "26.2", "26.1"],
        "fabric": {"version": "0.18.2", "latest": "0.18.4"},
        "mods": mods
    })
}

/// One result of the search of Modrinth, as the page that adds a mod reads it.
pub fn found(id: &str, title: &str, description: &str, version: &str, state: &str, new_version: &str) -> Value {
    json!({"id": id, "title": title, "description": description, "version": version, "state": state, "newVersion": new_version,
           "url": format!("https://modrinth.com/mod/{id}")})
}

/// The list the page that adds a mod shows before any search (board B4): Sodium and Lithium to update, Iris added,
/// four to add. `mod_menu` is the state of Mod Menu; adding it brings Fabric API (`mod_menu_added`).
pub fn mods_fixture(api: &FakeApi, mod_menu: &str) {
    api.set_post(
        "/api/mods/search",
        200,
        json!({"mods": [
            found("sodium", "Sodium", "A faster renderer for the game.", "0.7.1", "update", "0.7.3"),
            found("iris", "Iris Shaders", "Shaders, with Sodium.", "1.9.6", "added", ""),
            found("modmenu", "Mod Menu", "A list of the mods in the game.", "15.0.0", mod_menu, ""),
            found("journeymap", "JourneyMap", "A minimap and a full map of what you explored.", "6.0.0", "add", ""),
            found("lithium", "Lithium", "Faster game logic, the same game.", "0.16.0", "update", "0.16.2"),
            found("chatheads", "Chat Heads", "The face of each player beside their messages.", "0.13.9", "add", ""),
            found("continuity", "Continuity", "Connected textures for glass and bookshelves.", "3.0.1", "add", ""),
        ]}),
    );
}

/// What adding Mod Menu answers: the mod, then Fabric API, which it needs.
pub fn mod_menu_added(api: &FakeApi) {
    api.set_post(
        "/api/mods/add",
        200,
        json!({"added": [
            {"id": "modmenu", "title": "Mod Menu", "version": "15.0.0", "neededBy": null},
            {"id": "fabric-api", "title": "Fabric API", "version": "0.150.0", "neededBy": "Mod Menu"},
        ]}),
    );
}

/// A skin picture (64 x 64) in the manner of the boards' player: `hair` over a plain face, a teal shirt, blue trousers.
/// The head's front is the 8 x 8 of board A2's face; the hat and the outer layers are clear.
pub fn sample_skin(hair: u32) -> Vec<u8> {
    let rgb = |c: u32| [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255];
    let mut px = vec![0u8; 64 * 64 * 4];
    let mut fill = |x: u32, y: u32, w: u32, h: u32, c: [u8; 4]| {
        for yy in y..y + h {
            for xx in x..x + w {
                let i = ((yy * 64 + xx) * 4) as usize;
                px[i..i + 4].copy_from_slice(&c);
            }
        }
    };
    let (skin, shirt, blue, shoe) = (rgb(0xc69a73), rgb(0x2b8a9a), rgb(0x32349a), rgb(0x4b4b4b));
    // The head's front: hair over the top two rows and the sides of the third, eyes, nose and mouth.
    fill(8, 8, 8, 8, skin);
    fill(8, 8, 8, 2, rgb(hair));
    fill(8, 10, 1, 1, rgb(hair));
    fill(15, 10, 1, 1, rgb(hair));
    for (x, c) in [(9, 0xffffff), (10, 0x3a5bb0), (13, 0x3a5bb0), (14, 0xffffff)] {
        fill(x, 12, 1, 1, rgb(c));
    }
    fill(11, 13, 2, 1, rgb(0x9c6e4f));
    fill(10, 14, 4, 1, rgb(0x7a4a33));
    // The body's front, the two arms' (shirt to the wrist, then the hand) and the two legs' (trousers, then the shoe).
    fill(20, 20, 8, 12, shirt);
    for (x, y) in [(44, 20), (36, 52)] {
        fill(x, y, 4, 9, shirt);
        fill(x, y + 9, 4, 3, skin);
    }
    for (x, y) in [(4, 20), (20, 52)] {
        fill(x, y, 4, 10, blue);
        fill(x, y + 10, 4, 2, shoe);
    }
    mere_png::encode(64, 64, &px)
}

/// The two accounts of board A2 with their skins, for the bar's menu.
pub fn with_accounts(api: &FakeApi) {
    use crate::skin_view::base64_encode;
    api.set_get("/api/accounts", json!({"accounts": [{"name": "PlayerOne", "inUse": true}, {"name": "SecondAccount", "inUse": false}]}));
    for (name, hair) in [("PlayerOne", 0x4a2f1d), ("SecondAccount", 0xc8a04a)] {
        api.set_get(&format!("/api/skin?name={name}"), json!({"name": name, "slim": false, "png": base64_encode(&sample_skin(hair))}));
    }
}

/// The server of board C1: Test server, made from Exploration (three of its mods left out) and the world project
/// Tectonic islands, its EULA not accepted yet.
pub fn board_server(api: &FakeApi) {
    api.set_get(
        "/api/server",
        json!({
            "name": "Test server", "dir": "C:\\Code\\servers\\Test server", "state": "stopped", "eula": false, "memoryMb": 6144, "javaArgs": "-XX:+UseZGC",
            "props": null,
            "madeFrom": {"profile": "exploration", "leftOut": 3, "world": "Tectonic islands", "seed": 48213}
        }),
    );
}

/// Home's servers of board A1: Friday world running with two players, Test server stopped and made from Exploration. Each
/// one's own description answers where the api answers it, by its id in the query (`%20` for a space).
pub fn board_servers(api: &FakeApi) {
    api.set_get(
        "/api/servers",
        json!({"servers": [
            {"id": "Friday world", "name": "Friday world", "state": "running", "eula": true, "players": 2, "port": 25565,
             "madeFrom": {"profile": "exploration", "leftOut": 0, "world": null, "seed": null}},
            {"id": "Test server", "name": "Test server", "state": "stopped", "eula": false, "players": null, "port": 25565,
             "madeFrom": {"profile": "exploration", "leftOut": 3, "world": "Tectonic islands", "seed": 48213}},
        ], "broken": []}),
    );
    api.set_get(
        "/api/server?id=Friday%20world",
        json!({"id": "Friday world", "name": "Friday world", "dir": "C:\\Code\\servers\\Friday world", "state": "running", "eula": true, "memoryMb": 6144,
               "javaArgs": "-XX:+UseZGC", "props": {"motd": "Friday", "whitelist": true, "viewDistance": 12, "simulationDistance": 8, "maxPlayers": 20, "port": 25565},
               "madeFrom": {"profile": "exploration", "leftOut": 0, "world": null, "seed": null}}),
    );
    api.set_get(
        "/api/server?id=Test%20server",
        json!({"id": "Test server", "name": "Test server", "dir": "C:\\Code\\servers\\Test server", "state": "stopped", "eula": false, "memoryMb": 6144,
               "javaArgs": "-XX:+UseZGC", "props": null,
               "madeFrom": {"profile": "exploration", "leftOut": 3, "world": "Tectonic islands", "seed": 48213}}),
    );
}

/// The Java step of the first start (`/api/java`): no Java on the PC, a download of `size` bytes on offer.
pub fn java_missing(api: &FakeApi, size: u64) {
    api.set_get("/api/java", json!({"state": "missing", "size": size}));
    api.set_post("/api/java/download", 200, json!({}));
    api.set_post("/api/java/stop", 200, json!({}));
}

/// The same step while the download runs, `done` bytes of `total` in.
pub fn java_downloading(api: &FakeApi, done: u64, total: u64) {
    api.set_get("/api/java", json!({"state": "downloading", "stage": "downloading", "done": done, "total": total}));
}
