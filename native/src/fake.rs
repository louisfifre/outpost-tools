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
    api
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
