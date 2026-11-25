use super::super::Config;
use super::canned::Canned;
use super::*;

const ORIGIN: &str = "native://test";
const GAME: &str = "26.3";

// A Prism with one instance, "exploration", on 26.3 with Fabric 0.18.2, in a folder of its own; the world outside is
// a table. Nothing reads the network or the machine's own Prism.
struct World {
    api: Api,
    root: PathBuf,
    net: Arc<Canned>,
}

impl World {
    fn new(name: &str) -> World {
        let root = std::env::temp_dir().join(format!("outpost-mods-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let prism = root.join("prism");
        let instance = prism.join("instances").join("exploration");
        fs::create_dir_all(instance.join("minecraft").join("mods")).unwrap();
        fs::write(instance.join("instance.cfg"), "[General]\nname=Exploration\nMaxMemAlloc=6144\nMinMemAlloc=2048\n").unwrap();
        let pack = instance::with_fabric_loader(include_str!("../../../fixtures/instance/prism-mmc-pack.json"), "0.18.2").unwrap();
        fs::write(instance.join("mmc-pack.json"), pack).unwrap();
        let api = Api::new(root.join("repo"), ORIGIN.into());
        let dir = |d: &str| root.join(d);
        api.config
            .set(Config { dev_server: dir("server"), preview_dir: dir("preview"), preview_exports: dir("exports"), prism: Ok(prism.join("prismlauncher.exe")) })
            .ok()
            .unwrap();
        let net = Arc::new(Canned::default());
        install(&api.repo, net.clone());
        World { api, root, net }
    }

    fn instance(&self) -> PathBuf {
        self.root.join("prism").join("instances").join("exploration")
    }

    fn mods(&self) -> PathBuf {
        self.instance().join("minecraft").join("mods")
    }

    fn jar(&self, file: &str, bytes: &[u8]) -> String {
        fs::write(self.mods().join(file), bytes).unwrap();
        sha1_of(bytes)
    }

    fn files(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.mods()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        names.sort();
        names
    }

    fn get(&self, path: &str) -> (u16, Value) {
        let reply = self.api.handle("GET", path, None, b"");
        (reply.status, serde_json::from_slice(&reply.body).unwrap_or(Value::Null))
    }

    fn post_from(&self, origin: &str, path: &str, body: Value) -> (u16, Value) {
        let reply = self.api.handle("POST", path, Some(origin), body.to_string().as_bytes());
        (reply.status, serde_json::from_slice(&reply.body).unwrap_or(Value::Null))
    }

    fn post(&self, path: &str, body: Value) -> (u16, Value) {
        self.post_from(ORIGIN, path, body)
    }

    fn asked(&self, part: &str) -> usize {
        self.net.requests().iter().filter(|r| r.contains(part)).count()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

// A version of a mod as Modrinth answers it, whose file is `bytes` at https://cdn.test/<file>.
fn version(id: &str, project: &str, number: &str, date: &str, file: &str, bytes: &[u8], needs: &[&str]) -> Value {
    json!({
        "id": id, "project_id": project, "name": number, "version_number": number, "version_type": "release",
        "game_versions": [GAME], "loaders": ["fabric"], "date_published": date, "status": "listed",
        "files": [{ "url": format!("https://cdn.test/{file}"), "filename": file, "primary": true, "size": bytes.len(),
                    "hashes": { "sha1": sha1_of(bytes), "sha512": "x" } }],
        "dependencies": needs.iter().map(|p| json!({ "project_id": p, "version_id": null, "dependency_type": "required" })).collect::<Vec<_>>()
    })
}

fn project(id: &str, slug: &str, title: &str, description: &str) -> Value {
    json!({ "id": id, "slug": slug, "title": title, "description": description, "client_side": "required", "server_side": "required" })
}

fn array(values: Vec<Value>) -> String {
    Value::Array(values).to_string()
}

const SODIUM_OLD: &[u8] = b"sodium 0.7.1";
const SODIUM_NEW: &[u8] = b"sodium 0.7.3";
const LITHIUM: &[u8] = b"lithium 0.16.0";
const LOCAL: &[u8] = b"a jar dropped in by hand";

// The profile of the board: Sodium to update, Lithium up to date, a jar Modrinth does not know.
fn board(w: &World) {
    let (sodium_hash, lithium_hash) = (w.jar("sodium-0.7.1.jar", SODIUM_OLD), w.jar("lithium-0.16.0.jar", LITHIUM));
    w.jar("my-local-mod.jar", LOCAL);
    let old = version("v-sodium-1", "sod", "0.7.1", "2026-10-01T00:00:00Z", "sodium-0.7.1.jar", SODIUM_OLD, &[]);
    let new = version("v-sodium-2", "sod", "0.7.3", "2026-11-01T00:00:00Z", "sodium-0.7.3.jar", SODIUM_NEW, &[]);
    let lithium = version("v-lithium-1", "lit", "0.16.0", "2026-10-01T00:00:00Z", "lithium-0.16.0.jar", LITHIUM, &[]);
    w.net
        .when_post("/version_files/update", json!({ sodium_hash.clone(): new, lithium_hash.clone(): lithium }).to_string())
        .when_post("/version_files", json!({ sodium_hash: old, lithium_hash: lithium }).to_string())
        .when_get("/projects?ids=", array(vec![project("sod", "sodium", "Sodium", "A faster renderer."), project("lit", "lithium", "Lithium", "Faster logic.")]))
        .when_get("meta.fabricmc.net/v2/versions/loader/26.3", r#"[{"loader":{"version":"0.18.4","stable":true}},{"loader":{"version":"0.18.2","stable":true}}]"#)
        .when_get("https://cdn.test/sodium-0.7.3.jar", SODIUM_NEW);
}

#[test]
fn the_page_of_a_profile_lists_its_game_fabric_and_mods_with_what_is_newer() {
    let w = World::new("page");
    board(&w);
    let (status, page) = w.get("/api/mods/exploration");
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["gameVersion"], GAME);
    assert_eq!(page["gameVersions"], json!([GAME]));
    assert_eq!(page["fabric"], json!({ "version": "0.18.2", "latest": "0.18.4" }));
    // The one to update first, then the others by name, the file Modrinth does not know last.
    let names: Vec<_> = page["mods"].as_array().unwrap().iter().map(|m| m["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Sodium", "Lithium", "my-local-mod.jar"]);
    assert_eq!(page["mods"][0], json!({ "id": "sodium-0.7.1.jar", "slug": "sodium", "name": "Sodium", "version": "0.7.1", "latest": "0.7.3" }));
    assert_eq!(page["mods"][1]["latest"], Value::Null);
    assert_eq!(page["mods"][2], json!({ "id": "my-local-mod.jar", "slug": null, "name": "my-local-mod.jar", "version": "", "latest": null }));
    // Modrinth was asked by the files' SHA-1, for this game and Fabric.
    let asked = w.net.requests().into_iter().find(|r| r.contains("/version_files/update")).unwrap();
    assert!(asked.contains(&sha1_of(SODIUM_OLD)) && asked.contains(r#""game_versions":["26.3"]"#) && asked.contains(r#""loaders":["fabric"]"#), "{asked}");
}

#[test]
fn a_profile_without_fabric_or_mods_says_so() {
    let w = World::new("plain");
    fs::write(w.instance().join("mmc-pack.json"), r#"{"components":[{"uid":"net.minecraft","version":"26.2"}],"formatVersion":1}"#).unwrap();
    fs::remove_dir_all(w.mods()).unwrap();
    let (status, page) = w.get("/api/mods/exploration");
    assert_eq!(status, 200);
    assert_eq!(page, json!({ "gameVersion": "26.2", "gameVersions": ["26.2"], "fabric": null, "mods": [] }));
    // No jar, no question to Modrinth.
    assert!(w.net.requests().is_empty());
}

#[test]
fn a_page_read_offline_names_the_mods_by_their_files_and_offers_no_update() {
    let w = World::new("offline");
    w.jar("sodium-0.7.1.jar", SODIUM_OLD);
    let (status, page) = w.get("/api/mods/exploration");
    assert_eq!(status, 200);
    assert_eq!(page["mods"][0], json!({ "id": "sodium-0.7.1.jar", "slug": null, "name": "sodium-0.7.1.jar", "version": "", "latest": null }));
    assert_eq!(page["fabric"], json!({ "version": "0.18.2", "latest": null }));
}

#[test]
fn fabric_is_offered_a_newer_loader_only() {
    assert!(is_newer_loader("0.18.10", "0.18.9"));
    assert!(!is_newer_loader("0.18.2", "0.18.2"));
    assert!(!is_newer_loader("0.18.2", "0.18.4"));
    assert!(is_newer_loader("0.19.0-beta.1", "0.18.4"));
}

#[test]
fn an_unknown_profile_or_one_that_is_no_folder_name_is_a_404() {
    let w = World::new("unknown");
    assert_eq!(w.get("/api/mods/nothing").0, 404);
    assert_eq!(w.get("/api/mods/..").0, 404);
    assert_eq!(w.post("/api/mods/nothing/remove", json!({ "mod": "a.jar" })).0, 404);
    assert_eq!(w.post("/api/mods/add", json!({ "profile": "../exploration", "mod": "sod" })).0, 404);
    assert_eq!(w.post("/api/mods/exploration/nothing", json!({})).0, 404);
}

#[test]
fn only_the_page_itself_changes_anything() {
    let w = World::new("origin");
    board(&w);
    assert_eq!(w.post_from("https://elsewhere.example", "/api/mods/exploration/remove", json!({ "mod": "my-local-mod.jar" })).0, 403);
    assert_eq!(w.post_from("https://elsewhere.example", "/api/open-url", json!({ "url": "https://modrinth.com/mod/sodium" })).0, 403);
    assert!(w.files().contains(&"my-local-mod.jar".to_string()));
    assert!(w.net.opened.lock().unwrap().is_empty());
}

#[test]
fn a_removed_mod_is_kept_aside_and_comes_back_with_restore() {
    let w = World::new("remove");
    board(&w);
    assert_eq!(w.post("/api/mods/exploration/remove", json!({ "mod": "my-local-mod.jar" })).0, 200);
    assert!(!w.files().contains(&"my-local-mod.jar".to_string()));
    assert_eq!(fs::read(w.instance().join("outpost-removed").join("my-local-mod.jar")).unwrap(), LOCAL);
    let ids: Vec<_> = w.get("/api/mods/exploration").1["mods"].as_array().unwrap().iter().map(|m| m["id"].clone()).collect();
    assert!(!ids.contains(&json!("my-local-mod.jar")));
    assert_eq!(w.post("/api/mods/exploration/restore", json!({ "mod": "my-local-mod.jar" })).0, 200);
    assert_eq!(fs::read(w.mods().join("my-local-mod.jar")).unwrap(), LOCAL);
    // Nothing is kept once it is back; a second restore has nothing to put back.
    assert!(!w.instance().join("outpost-removed").exists());
    assert_eq!(w.post("/api/mods/exploration/restore", json!({ "mod": "my-local-mod.jar" })).0, 404);
}

#[test]
fn only_the_last_removal_is_kept_and_another_change_lets_it_go() {
    let w = World::new("last");
    board(&w);
    w.post("/api/mods/exploration/remove", json!({ "mod": "my-local-mod.jar" }));
    w.post("/api/mods/exploration/remove", json!({ "mod": "lithium-0.16.0.jar" }));
    assert_eq!(w.post("/api/mods/exploration/restore", json!({ "mod": "my-local-mod.jar" })).0, 404);
    assert_eq!(w.post("/api/mods/exploration/restore", json!({ "mod": "lithium-0.16.0.jar" })).0, 200);
    w.post("/api/mods/exploration/remove", json!({ "mod": "my-local-mod.jar" }));
    assert_eq!(w.post("/api/mods/exploration/update", json!({ "mod": "sodium-0.7.1.jar" })).0, 200);
    assert!(!w.instance().join("outpost-removed").exists());
}

#[test]
fn a_restore_does_not_overwrite_a_file_that_took_its_place() {
    let w = World::new("restore-clash");
    board(&w);
    w.post("/api/mods/exploration/remove", json!({ "mod": "my-local-mod.jar" }));
    fs::write(w.mods().join("my-local-mod.jar"), b"another").unwrap();
    let (status, said) = w.post("/api/mods/exploration/restore", json!({ "mod": "my-local-mod.jar" }));
    assert_eq!(status, 422);
    assert!(said["error"].as_str().unwrap().contains("my-local-mod.jar"));
    assert_eq!(fs::read(w.mods().join("my-local-mod.jar")).unwrap(), b"another");
}

#[test]
fn a_mod_is_named_by_a_plain_jar_file_of_the_profile_and_nothing_else() {
    let w = World::new("names");
    board(&w);
    fs::write(w.instance().join("instance.cfg.jar"), b"x").unwrap();
    for bad in ["../instance.cfg.jar", "..\\mmc-pack.json", "mmc-pack.json", "", "nothing.jar"] {
        assert_eq!(w.post("/api/mods/exploration/remove", json!({ "mod": bad })).0, 404, "{bad}");
    }
    assert!(w.instance().join("instance.cfg.jar").exists());
    assert!(!w.instance().join("outpost-removed").exists());
}

#[test]
fn nothing_changes_while_minecraft_runs() {
    let w = World::new("locked");
    board(&w);
    *w.net.locked.lock().unwrap() = Some(true);
    for (path, body) in [
        ("/api/mods/exploration/remove", json!({ "mod": "my-local-mod.jar" })),
        ("/api/mods/exploration/restore", json!({ "mod": "my-local-mod.jar" })),
        ("/api/mods/exploration/update", json!({ "mod": "sodium-0.7.1.jar" })),
        ("/api/mods/exploration/update-all", json!({})),
        ("/api/mods/exploration/fabric", json!({})),
        ("/api/mods/add", json!({ "profile": "exploration", "mod": "sod" })),
        ("/api/mods/file", json!({ "profile": "exploration" })),
    ] {
        let (status, said) = w.post(path, body);
        assert_eq!(status, 409, "{path}");
        assert_eq!(said["error"], RUNNING);
    }
    assert_eq!(w.files().len(), 3);
    assert_eq!(w.asked("cdn.test"), 0);
    // Looking is allowed: the page reads, and a search is asked (here the table has no answer for it).
    assert_eq!(w.get("/api/mods/exploration").0, 200);
    assert_eq!(w.post("/api/mods/search", json!({ "profile": "exploration", "query": "" })).0, 503);
}

#[test]
fn an_update_fetches_the_newer_file_checks_it_and_replaces_the_old_one() {
    let w = World::new("update");
    board(&w);
    assert_eq!(w.post("/api/mods/exploration/update", json!({ "mod": "sodium-0.7.1.jar" })).0, 200);
    assert_eq!(w.files(), ["lithium-0.16.0.jar", "my-local-mod.jar", "sodium-0.7.3.jar"]);
    assert_eq!(fs::read(w.mods().join("sodium-0.7.3.jar")).unwrap(), SODIUM_NEW);
}

#[test]
fn the_page_that_adds_a_mod_updates_it_by_its_project() {
    let w = World::new("update-by-project");
    board(&w);
    assert_eq!(w.post("/api/mods/update", json!({ "profile": "exploration", "mod": "sod" })).0, 200);
    assert!(w.files().contains(&"sodium-0.7.3.jar".to_string()) && !w.files().contains(&"sodium-0.7.1.jar".to_string()));
    assert_eq!(w.post("/api/mods/update", json!({ "profile": "exploration", "mod": "nope" })).0, 404);
}

#[test]
fn a_file_that_does_not_match_its_checksum_changes_nothing() {
    let w = World::new("checksum");
    board(&w);
    // The first rule for an address wins: the wrong answer goes in front of the right one.
    let wrong = Canned::default();
    wrong.when_get("https://cdn.test/sodium-0.7.3.jar", b"not the file".to_vec());
    install(&w.api.repo, Arc::new(InFront { first: wrong, then: w.net.clone() }));
    let (status, said) = w.post("/api/mods/exploration/update", json!({ "mod": "sodium-0.7.1.jar" }));
    assert_eq!(status, 502);
    assert!(said["error"].as_str().unwrap().contains("checksum"), "{said}");
    assert_eq!(w.files(), ["lithium-0.16.0.jar", "my-local-mod.jar", "sodium-0.7.1.jar"]);
}

// Answers a GET from `first` when it has a rule, else from `then`.
struct InFront {
    first: Canned,
    then: Arc<Canned>,
}

impl Http for InFront {
    fn get(&self, url: &str) -> Result<Vec<u8>, HttpError> {
        Http::get(&self.first, url).or_else(|_| Http::get(&*self.then, url))
    }
    fn post_json(&self, url: &str, body: &str) -> Result<Vec<u8>, HttpError> {
        self.then.post_json(url, body)
    }
}

impl Outside for InFront {
    fn open(&self, url: &str) -> Result<(), String> {
        self.then.open(url)
    }
    fn pick_file(&self) -> Option<PathBuf> {
        None
    }
    fn locked(&self) -> Option<bool> {
        Some(false)
    }
}

#[test]
fn update_all_brings_every_mod_that_has_a_newer_version_up() {
    let w = World::new("update-all");
    board(&w);
    assert_eq!(w.post("/api/mods/exploration/update-all", json!({})).0, 200);
    assert_eq!(w.files(), ["lithium-0.16.0.jar", "my-local-mod.jar", "sodium-0.7.3.jar"]);
    assert_eq!(w.asked("cdn.test"), 1);
}

// What a search of Modrinth answers for the three mods of the board: a hit names its project `project_id` and the id of
// its newest version, and Modrinth answers those ids in one call.
fn recorded_search(w: &World) {
    recorded_search_with(w, version("v-jm", "jm", "6.0.0", "2026-11-02T00:00:00Z", "journeymap-6.0.0.jar", b"jm", &[]));
}

fn recorded_search_with(w: &World, jm: Value) {
    let hits = [("sod", "sodium", "Sodium", "A faster renderer."), ("lit", "lithium", "Lithium", "Faster logic."), ("jm", "journeymap", "JourneyMap", "A minimap.")];
    let hits: Vec<Value> = hits
        .iter()
        .map(|(id, slug, title, description)| {
            let mut hit = project(id, slug, title, description);
            hit["project_id"] = hit["id"].take();
            hit["latest_version"] = json!(format!("v-{id}"));
            hit.as_object_mut().unwrap().remove("id");
            hit
        })
        .collect();
    w.net.when_get("/search", json!({ "hits": hits, "total_hits": 3 }).to_string());
    w.net.when_get("/versions?ids=", array(vec![jm]));
}

fn search_for(w: &World, query: &str) -> (u16, Value) {
    w.post("/api/mods/search", json!({ "profile": "exploration", "query": query }))
}

#[test]
fn a_search_is_for_the_profiles_game_and_fabric_and_says_what_the_profile_has() {
    let w = World::new("search");
    board(&w);
    recorded_search(&w);
    let (status, found) = search_for(&w, "map");
    assert_eq!(status, 200, "{found}");
    assert_eq!(
        found["mods"],
        json!([
            { "id": "sod", "title": "Sodium", "description": "A faster renderer.", "version": "0.7.1", "url": "https://modrinth.com/mod/sodium", "state": "update", "newVersion": "0.7.3" },
            { "id": "lit", "title": "Lithium", "description": "Faster logic.", "version": "0.16.0", "url": "https://modrinth.com/mod/lithium", "state": "added", "newVersion": "" },
            { "id": "jm", "title": "JourneyMap", "description": "A minimap.", "version": "6.0.0", "url": "https://modrinth.com/mod/journeymap", "state": "add", "newVersion": "" },
        ])
    );
    let url = w.net.requests().into_iter().find(|r| r.contains("/search")).unwrap();
    assert!(url.contains("versions%3A26.3") && url.contains("categories%3Afabric") && url.contains("query=map"), "{url}");
    // The versions the rows need come in one call, by the ids the hits name; no request per mod.
    assert_eq!(w.asked("/versions?ids="), 1);
    let ask = w.net.requests().into_iter().find(|r| r.contains("/versions?ids=")).unwrap();
    assert!(ask.contains("v-sod") && ask.contains("v-lit") && ask.contains("v-jm"), "{ask}");
    assert_eq!(w.asked("/project/"), 0);
}

#[test]
fn a_query_is_two_calls_to_modrinth_and_the_profiles_files_are_looked_up_once() {
    let w = World::new("search-calls");
    board(&w);
    recorded_search(&w);
    assert_eq!(search_for(&w, "map").0, 200);
    // The search, the versions, and the two lookups of the profile's files.
    assert_eq!(w.net.requests().len(), 4, "{:?}", w.net.requests());
    assert_eq!((w.asked("/search"), w.asked("/versions?ids="), w.asked("/version_files")), (1, 1, 2));
    // Another query asks again for its own list and versions, not for the files.
    assert_eq!(search_for(&w, "mini").0, 200);
    assert_eq!((w.asked("/search"), w.asked("/versions?ids="), w.asked("/version_files")), (2, 2, 2));
}

#[test]
fn the_same_query_is_answered_from_memory() {
    let w = World::new("search-cache");
    board(&w);
    recorded_search(&w);
    let first = search_for(&w, "map");
    let before = w.net.requests().len();
    // The same words, in other capitals and with a space, are the same query.
    let again = search_for(&w, " Map ");
    assert_eq!(first, again);
    assert_eq!(w.net.requests().len(), before, "nothing was asked");
    // A change of the profile's files shows at once: the lines say what the profile has now.
    w.jar("journeymap-6.0.0.jar", b"jm");
    let after = search_for(&w, "map");
    assert_eq!(after.0, 200);
    assert_eq!(w.asked("/search"), 1, "the list of Modrinth is still the one kept");
    assert_eq!(w.asked("/versions?ids="), 1);
}

#[test]
fn what_modrinth_said_is_kept_a_few_minutes_and_no_more() {
    let w = World::new("search-expiry");
    board(&w);
    recorded_search(&w);
    let searches = search::Searches::default();
    let outside: Arc<dyn Outside> = w.net.clone();
    let profile = w.api.mods_profile("exploration").ok().flatten().unwrap();
    let start = std::time::Instant::now();
    let ask = |at: std::time::Instant| search::search_at(&searches, &profile, &outside, "map", at).unwrap_or_else(|stop| stop);
    assert_eq!(ask(start).status, 200);
    assert_eq!(ask(start + std::time::Duration::from_secs(60)).status, 200);
    assert_eq!(w.asked("/search"), 1, "a minute later it is kept");
    assert_eq!(ask(start + search::KEPT + std::time::Duration::from_secs(1)).status, 200);
    assert_eq!(w.asked("/search"), 2, "after its time it is asked again");
    assert_eq!(w.asked("/versions?ids="), 2);
    assert_eq!(w.asked("/version_files"), 4);
}

// An outside where another search is asked while Modrinth answers the first.
struct Overtaken {
    net: Arc<Canned>,
    searches: Arc<search::Searches>,
}

impl Http for Overtaken {
    fn get(&self, url: &str) -> Result<Vec<u8>, HttpError> {
        if url.contains("/search") {
            self.searches.ticket();
        }
        self.net.get(url)
    }
    fn post_json(&self, url: &str, body: &str) -> Result<Vec<u8>, HttpError> {
        self.net.post_json(url, body)
    }
}

impl Outside for Overtaken {
    fn open(&self, url: &str) -> Result<(), String> {
        self.net.open(url)
    }
    fn pick_file(&self) -> Option<PathBuf> {
        None
    }
}

#[test]
fn a_search_that_a_newer_one_overtook_stops_and_says_stale() {
    let w = World::new("search-stale");
    board(&w);
    recorded_search(&w);
    let searches = Arc::new(search::Searches::default());
    let outside: Arc<dyn Outside> = Arc::new(Overtaken { net: w.net.clone(), searches: searches.clone() });
    let profile = w.api.mods_profile("exploration").ok().flatten().unwrap();
    let reply = search::search_at(&searches, &profile, &outside, "map", std::time::Instant::now()).unwrap_or_else(|stop| stop);
    assert_eq!(reply.status, 200);
    assert_eq!(serde_json::from_slice::<Value>(&reply.body).unwrap(), json!({ "stale": true }));
    // It stopped after the search: no versions, no lookup of the files, nothing kept for the next.
    assert_eq!((w.asked("/search"), w.asked("/versions?ids="), w.asked("/version_files")), (1, 0, 0));
}

#[test]
fn a_hit_whose_newest_version_does_not_fit_is_listed_with_no_version() {
    let w = World::new("search-nofit");
    board(&w);
    let mut other = version("v-jm", "jm", "7.0.0", "2026-11-02T00:00:00Z", "journeymap-7.0.0.jar", b"jm7", &[]);
    other["game_versions"] = json!(["27.1"]);
    recorded_search_with(&w, other);
    let (_, found) = search_for(&w, "map");
    let jm = found["mods"].as_array().unwrap().iter().find(|m| m["id"] == "jm").unwrap();
    assert_eq!((jm["state"].as_str(), jm["version"].as_str()), (Some("add"), Some("")));
}

#[test]
fn a_search_that_cannot_reach_modrinth_says_so() {
    let w = World::new("search-offline");
    let (status, said) = w.post("/api/mods/search", json!({ "profile": "exploration", "query": "" }));
    assert_eq!(status, 503);
    assert!(said["error"].as_str().unwrap().starts_with("Modrinth cannot be reached"), "{said}");
}

// Mod Menu needs Fabric API.
fn mod_menu(w: &World, api_bytes: &[u8]) {
    let (menu_bytes, api_file) = (b"modmenu".as_slice(), b"fabric api".as_slice());
    w.net
        .when_get("/project/mm/version", array(vec![version("v-mm", "mm", "15.0.0", "2026-11-02T00:00:00Z", "menu-15.0.0.jar", menu_bytes, &["fa"])]))
        .when_get("/project/fa/version", array(vec![version("v-fa", "fa", "0.150.0", "2026-11-02T00:00:00Z", "fabric-api-0.150.0.jar", api_file, &[])]))
        .when_get("/projects?ids=", array(vec![project("mm", "modmenu", "Mod Menu", "A list."), project("fa", "fabric-api", "Fabric API", "The base.")]))
        .when_get("https://cdn.test/menu-15.0.0.jar", menu_bytes)
        .when_get("https://cdn.test/fabric-api-0.150.0.jar", api_bytes);
}

#[test]
fn a_mod_is_added_with_what_it_needs_and_says_what_came_with_it() {
    let w = World::new("add");
    mod_menu(&w, b"fabric api");
    let (status, added) = w.post("/api/mods/add", json!({ "profile": "exploration", "mod": "mm" }));
    assert_eq!(status, 200, "{added}");
    assert_eq!(
        added["added"],
        json!([
            { "id": "mm", "title": "Mod Menu", "version": "15.0.0", "neededBy": null },
            { "id": "fa", "title": "Fabric API", "version": "0.150.0", "neededBy": "Mod Menu" },
        ])
    );
    assert_eq!(w.files(), ["fabric-api-0.150.0.jar", "menu-15.0.0.jar"]);
}

#[test]
fn a_mod_whose_dependency_is_corrupt_is_not_added_at_all() {
    let w = World::new("add-corrupt");
    mod_menu(&w, b"fabric api, cut short");
    let (status, said) = w.post("/api/mods/add", json!({ "profile": "exploration", "mod": "mm" }));
    assert_eq!(status, 502);
    assert!(said["error"].as_str().unwrap().contains("Fabric API"), "{said}");
    assert!(w.files().is_empty());
}

#[test]
fn a_mod_with_no_version_for_the_game_is_refused_with_the_versions_it_has() {
    let w = World::new("add-no-version");
    let mut other = version("v-old", "old", "1.0", "2026-01-01T00:00:00Z", "old.jar", b"o", &[]);
    other["game_versions"] = json!(["1.20.1"]);
    w.net.when_get("game_versions=", "[]").when_get("/project/old/version", array(vec![other])).when_get("/projects?ids=", array(vec![project("old", "old", "Old Mod", "")]));
    let (status, said) = w.post("/api/mods/add", json!({ "profile": "exploration", "mod": "old" }));
    assert_eq!(status, 422);
    let said = said["error"].as_str().unwrap();
    assert!(said.contains("Old Mod") && said.contains("1.20.1"), "{said}");
    assert!(w.files().is_empty());
}

#[test]
fn a_mod_the_profile_has_is_not_added_again() {
    let w = World::new("add-twice");
    board(&w);
    let (status, added) = w.post("/api/mods/add", json!({ "profile": "exploration", "mod": "lit" }));
    assert_eq!(status, 200);
    assert_eq!(added["added"], json!([]));
    assert_eq!(w.files().len(), 3);
}

#[test]
fn a_file_is_added_as_it_is_and_cancelling_adds_nothing() {
    let w = World::new("file");
    let from = w.root.join("downloads");
    fs::create_dir_all(&from).unwrap();
    fs::write(from.join("shaderpack-helper.jar"), b"by hand").unwrap();
    fs::write(from.join("notes.txt"), b"text").unwrap();
    // Cancelled.
    assert_eq!(w.post("/api/mods/file", json!({ "profile": "exploration" })).0, 200);
    assert!(w.files().is_empty());
    // Not a jar.
    *w.net.file.lock().unwrap() = Some(from.join("notes.txt"));
    assert_eq!(w.post("/api/mods/file", json!({ "profile": "exploration" })).0, 422);
    // A jar.
    *w.net.file.lock().unwrap() = Some(from.join("shaderpack-helper.jar"));
    assert_eq!(w.post("/api/mods/file", json!({ "profile": "exploration" })).0, 200);
    assert_eq!(fs::read(w.mods().join("shaderpack-helper.jar")).unwrap(), b"by hand");
    // The same again: the first stays.
    let (status, said) = w.post("/api/mods/file", json!({ "profile": "exploration" }));
    assert_eq!(status, 422);
    assert!(said["error"].as_str().unwrap().contains("shaderpack-helper.jar"));
    assert_eq!(w.files(), ["shaderpack-helper.jar"]);
    assert!(from.join("shaderpack-helper.jar").exists(), "the person's file is copied, never moved");
}

#[test]
fn only_a_web_address_is_opened_in_the_browser() {
    let w = World::new("open");
    for (route, url) in [("/api/mods/open", "https://modrinth.com/mod/sodium"), ("/api/open-url", "http://localhost:8080/x?a=1&b=2")] {
        assert_eq!(w.post(route, json!({ "url": url })).0, 200, "{url}");
    }
    for bad in ["file:///C:/Windows/System32/cmd.exe", "C:\\Windows\\notepad.exe", "javascript:alert(1)", "https://a.example/x y", "https://a.example/\"x", "", "ms-settings:"] {
        assert_eq!(w.post("/api/mods/open", json!({ "url": bad })).0, 400, "{bad}");
    }
    assert_eq!(*w.net.opened.lock().unwrap(), ["https://modrinth.com/mod/sodium", "http://localhost:8080/x?a=1&b=2"]);
}

#[test]
fn fabric_is_moved_to_the_newest_stable_loader_in_the_instances_own_file() {
    let w = World::new("fabric");
    board(&w);
    assert_eq!(w.post("/api/mods/exploration/fabric", json!({})).0, 200);
    let text = fs::read_to_string(w.instance().join("mmc-pack.json")).unwrap();
    assert_eq!(instance::pack_versions(&text), Some((GAME.to_string(), Some("0.18.4".to_string()))));
    assert_eq!(text.matches("net.fabricmc.fabric-loader").count(), 1);
    // The page now sees the loader it has, with nothing newer.
    assert_eq!(w.get("/api/mods/exploration").1["fabric"], json!({ "version": "0.18.4", "latest": null }));
}

#[test]
fn fabric_is_installed_in_a_profile_that_has_none() {
    let w = World::new("fabric-install");
    board(&w);
    fs::write(
        w.instance().join("mmc-pack.json"),
        r#"{"components":[{"cachedName":"LWJGL 3","uid":"org.lwjgl3","version":"3.3.3"},{"uid":"net.minecraft","version":"26.3"}],"formatVersion":1}"#,
    )
    .unwrap();
    assert_eq!(w.get("/api/mods/exploration").1["fabric"], Value::Null);
    assert_eq!(w.post("/api/mods/exploration/fabric", json!({})).0, 200);
    let text = fs::read_to_string(w.instance().join("mmc-pack.json")).unwrap();
    assert_eq!(instance::pack_versions(&text), Some((GAME.to_string(), Some("0.18.4".to_string()))));
    // What Prism wrote is kept.
    assert!(text.contains("org.lwjgl3") && text.contains("net.fabricmc.intermediary"));
}

#[test]
fn fabric_unreachable_leaves_the_file_as_it_was() {
    let w = World::new("fabric-offline");
    let before = fs::read_to_string(w.instance().join("mmc-pack.json")).unwrap();
    let (status, said) = w.post("/api/mods/exploration/fabric", json!({}));
    assert_eq!(status, 503);
    assert!(said["error"].as_str().unwrap().contains("could not be fetched"), "{said}");
    assert_eq!(fs::read_to_string(w.instance().join("mmc-pack.json")).unwrap(), before);
}

#[test]
fn the_profiles_carry_their_least_memory_and_what_the_pc_uses() {
    let w = World::new("memory");
    let (status, profiles) = w.get("/api/profiles");
    assert_eq!(status, 200, "{profiles}");
    assert_eq!(profiles["profiles"][0]["minMemoryMb"], 2048);
    assert_eq!(profiles["profiles"][0]["memoryMb"], 6144);
    let used = profiles["usedMb"].as_i64().unwrap();
    assert!(used > 0 && used <= w.api.machine_mb as i64, "{used}");
}

#[test]
fn a_profile_without_a_least_memory_says_null() {
    let w = World::new("memory-null");
    fs::write(w.instance().join("instance.cfg"), "[General]\nname=Exploration\nMaxMemAlloc=6144\n").unwrap();
    assert_eq!(w.get("/api/profiles").1["profiles"][0]["minMemoryMb"], Value::Null);
}

#[test]
fn the_least_memory_is_saved_with_the_most_and_kept_within_it() {
    let w = World::new("memory-save");
    // The settings route reads the processes to know whether Prism runs, which is the machine's own: this goes
    // straight to the part that validates and writes.
    let cfg = w.instance().join("instance.cfg");
    let (most, least) = (2048.min(w.api.machine_mb as i64), 1024);
    let body = json!({ "memoryMb": most, "minMemoryMb": least, "javaArgs": "-XX:+UseZGC", "open": { "kind": "menu", "target": "" } });
    let reply = w.api.profile_settings(&cfg, &body).ok().unwrap();
    // 409 when Prism runs on the machine the tests run on: the write is not reached then.
    if reply.status == 409 {
        return;
    }
    assert_eq!(reply.status, 200);
    let saved = super::super::key_value::Cfg::read(&cfg);
    assert_eq!((saved.number("MaxMemAlloc"), saved.number("MinMemAlloc")), (most, least));
    // A least above the most, or under 1 GB, is refused and nothing changes.
    for bad in [most + 1024, 512, -1] {
        let body = json!({ "memoryMb": most, "minMemoryMb": bad, "javaArgs": "", "open": { "kind": "menu", "target": "" } });
        assert_eq!(w.api.profile_settings(&cfg, &body).ok().unwrap().status, 400, "{bad}");
    }
    assert_eq!(super::super::key_value::Cfg::read(&cfg).number("MinMemAlloc"), least);
}

#[test]
fn the_profiles_say_how_many_mods_they_hold_and_how_many_the_last_look_found_newer_without_asking_the_network() {
    let w = World::new("summary");
    board(&w);
    // A file that is no jar is not a mod.
    fs::write(w.mods().join("readme.txt"), "x").unwrap();
    let profile = |w: &World| w.get("/api/profiles").1["profiles"][0].clone();
    // Before any look: the mods are counted from the folder, the updates are not known.
    let first = profile(&w);
    assert_eq!((first["mods"].clone(), first["updates"].clone()), (json!(3), Value::Null));
    assert!(w.net.requests().is_empty(), "Home's route never asks Modrinth");
    // The profile's page looks; Home then reads what it found.
    w.get("/api/mods/exploration");
    let asked = w.net.requests().len();
    let after = profile(&w);
    assert_eq!(after["updates"], json!(1));
    assert_eq!(w.net.requests().len(), asked, "reading the profiles asked nothing more");
    // Updating the one brings the count down at the next look.
    w.post("/api/mods/exploration/update", json!({ "mod": "sodium-0.7.1.jar" }));
    w.get("/api/mods/exploration");
    assert_eq!(profile(&w)["updates"], json!(0));
}

#[test]
fn a_look_that_could_not_reach_modrinth_leaves_the_updates_unknown() {
    let w = World::new("summary-offline");
    w.jar("sodium-0.7.1.jar", SODIUM_OLD);
    w.get("/api/mods/exploration");
    assert_eq!(w.get("/api/profiles").1["profiles"][0]["updates"], Value::Null, "nothing found, nothing said");
}
