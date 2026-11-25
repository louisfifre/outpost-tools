//! The profile's page (boards B1, B3) and the page that adds a mod (board B4), drawn from the real api: a Prism folder
//! of instances in a temporary folder, its mods real files, and Modrinth, Fabric's list and the browser answered by a
//! table of recorded-shape JSON (`api::mods::canned`). Nothing is fetched, nothing is started, nothing outside the
//! temporary folder is read or written. The pictures go to `target\tmp` as `real_*.png`, to be set beside the boards.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::*;
use launcher::api::Api;
use launcher::api::mods::{self, canned::Canned};
use outpost_tools::ORIGIN;
use outpost_tools::bridge::Backend;
use outpost_tools::fake::FakeApi;
use mere_base::{App, Vec2};
use serde_json::{Value, json};

/// What the table knows of a mod: its project (the slug is its id here), the version it is at, the newer one.
struct Entry {
    slug: &'static str,
    title: &'static str,
    description: &'static str,
    version: &'static str,
    latest: Option<&'static str>,
}

const fn entry(slug: &'static str, title: &'static str, description: &'static str, version: &'static str, latest: Option<&'static str>) -> Entry {
    Entry { slug, title, description, version, latest }
}

/// The mods of the boards: B1's fourteen (the last is no entry, a file dropped in by hand), and the others B4 lists.
const CATALOG: &[Entry] = &[
    entry("sodium", "Sodium", "A faster renderer for the game.", "0.7.1", Some("0.7.3")),
    entry("lithium", "Lithium", "Faster game logic, the same game.", "0.16.0", Some("0.16.2")),
    entry("distanthorizons", "Distant Horizons", "A far view that costs little.", "2.4.0", Some("2.4.1")),
    entry("tectonic", "Tectonic", "Worlds of other shapes.", "3.1.0", None),
    entry("fabric-api", "Fabric API", "The base most mods stand on.", "0.140.0", None),
    entry("iris", "Iris Shaders", "Shaders, with Sodium.", "1.9.6", None),
    entry("modmenu", "Mod Menu", "A list of the mods in the game.", "15.0.0", None),
    entry("xaeros-minimap", "Xaero's Minimap", "A minimap.", "25.2.0", None),
    entry("xaeros-world-map", "Xaero's World Map", "A world map.", "1.39.2", None),
    entry("entityculling", "Entity Culling", "Skips what cannot be seen.", "1.7.4", None),
    entry("ferrite-core", "FerriteCore", "Less memory for the same game.", "8.0.0", None),
    entry("modernfix", "ModernFix", "Fixes and a faster start.", "5.21.0", None),
    entry("sodium-extra", "Sodium Extra", "More options for Sodium.", "0.6.4", None),
    entry("journeymap", "JourneyMap", "A minimap and a full map of what you explored.", "6.0.0", None),
    entry("chatheads", "Chat Heads", "The face of each player beside their messages.", "0.13.9", None),
    entry("continuity", "Continuity", "Connected textures for glass and bookshelves.", "3.0.1", None),
];

fn entry_of(slug: &str) -> &'static Entry {
    CATALOG.iter().find(|e| e.slug == slug).expect("in the catalog")
}

fn file_of(slug: &str, version: &str) -> String {
    format!("{slug}-{version}.jar")
}

fn bytes_of(slug: &str, version: &str) -> Vec<u8> {
    format!("{slug} {version}").into_bytes()
}

/// The SHA-1 Modrinth knows a file by, taken by the api's own function from a file written for the purpose.
fn sha1(bytes: &[u8]) -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("real_mods_hashes");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    fs::write(&file, bytes).unwrap();
    let hash = launcher::api::modrinth::file_sha1(&file).unwrap();
    let _ = fs::remove_file(file);
    hash
}

/// A version of a mod as Modrinth answers it.
fn version_of(slug: &str, number: &str, date: &str) -> Value {
    let bytes = bytes_of(slug, number);
    let needs: Vec<Value> = if slug == "modmenu" { vec![json!({ "project_id": "fabric-api", "version_id": null, "dependency_type": "required" })] } else { vec![] };
    json!({
        "id": format!("v-{slug}-{number}"), "project_id": slug, "name": number, "version_number": number, "version_type": "release",
        "game_versions": ["26.3"], "loaders": ["fabric"], "date_published": date, "status": "listed",
        "files": [{ "url": format!("https://cdn.test/{}", file_of(slug, number)), "filename": file_of(slug, number), "primary": true,
                    "size": bytes.len(), "hashes": { "sha1": sha1(&bytes), "sha512": "x" } }],
        "dependencies": needs
    })
}

/// A Prism in a folder of its own with one instance, `exploration`, holding the jars of `installed` (and a file dropped
/// in by hand when `by_hand`), and the real api over it, its outside world a table.
struct Setup {
    api: Arc<Api>,
    net: Arc<Canned>,
    root: PathBuf,
}

impl Setup {
    fn mods(&self) -> PathBuf {
        self.root.join("prism").join("instances").join("exploration").join("minecraft").join("mods")
    }

    fn files(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.mods()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        names.sort();
        names
    }
}

fn setup(name: &str, installed: &[&str], by_hand: bool, hits: &[&str]) -> Setup {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("real_mods_{name}"));
    let _ = fs::remove_dir_all(&root);
    let prism = root.join("prism");
    let instance = prism.join("instances").join("exploration");
    let mods = instance.join("minecraft").join("mods");
    fs::create_dir_all(&mods).unwrap();
    fs::create_dir_all(root.join("repo").join("scripts")).unwrap();
    // What the api reads the pack's folders from: a script that names them, and where Prism is.
    let at = |d: &str| root.join(d).display().to_string();
    fs::write(
        root.join("repo").join("scripts").join("common.ps1"),
        format!("$DevServerDir = '{}'\n$PreviewDir = '{}'\n$PreviewExports = '{}'\nfunction Find-Prism {{ '{}' }}\n", at("server"), at("preview"), at("exports"), prism.join("prismlauncher.exe").display()),
    )
    .unwrap();
    fs::write(instance.join("instance.cfg"), "[General]\nname=Exploration\nOverrideMemory=true\nMinMemAlloc=2048\nMaxMemAlloc=6144\nJvmArgs=-XX:+UseZGC\nlastLaunchTime=1795176000000\ntotalTimePlayed=6120\n").unwrap();
    fs::write(
        instance.join("mmc-pack.json"),
        r#"{"components":[{"cachedName":"Minecraft","cachedVersion":"26.3","important":true,"uid":"net.minecraft","version":"26.3"},{"cachedName":"Intermediary Mappings","cachedVersion":"26.3","dependencyOnly":true,"uid":"net.fabricmc.intermediary","version":"26.3"},{"cachedName":"Fabric Loader","cachedVersion":"0.18.2","uid":"net.fabricmc.fabric-loader","version":"0.18.2"}],"formatVersion":1}"#,
    )
    .unwrap();
    for slug in installed {
        let e = entry_of(slug);
        fs::write(mods.join(file_of(slug, e.version)), bytes_of(slug, e.version)).unwrap();
    }
    if by_hand {
        fs::write(mods.join("my-local-mod.jar"), b"dropped in by hand").unwrap();
    }

    let net = Arc::new(Canned::default());
    // What Modrinth knows of every file of the table: by its hash, the version it is and the one it could be.
    let (mut current, mut latest) = (serde_json::Map::new(), serde_json::Map::new());
    for e in CATALOG {
        let now = version_of(e.slug, e.version, "2026-10-01T00:00:00Z");
        current.insert(sha1(&bytes_of(e.slug, e.version)), now.clone());
        let newest = e.latest.map(|n| version_of(e.slug, n, "2026-11-01T00:00:00Z")).unwrap_or(now);
        latest.insert(sha1(&bytes_of(e.slug, e.version)), newest);
    }
    net.when_post("/version_files/update", Value::Object(latest).to_string()).when_post("/version_files", Value::Object(current).to_string());
    let projects: Vec<Value> = CATALOG
        .iter()
        .map(|e| json!({ "id": e.slug, "slug": e.slug, "title": e.title, "description": e.description, "client_side": "required", "server_side": "required" }))
        .collect();
    net.when_get("/projects?ids=", Value::Array(projects).to_string());
    net.when_get("meta.fabricmc.net/v2/versions/loader/26.3", r#"[{"loader":{"version":"0.18.4","stable":true}},{"loader":{"version":"0.18.2","stable":true}}]"#);
    for e in CATALOG {
        let newest = e.latest.unwrap_or(e.version);
        net.when_get(&format!("/project/{}/version", e.slug), Value::Array(vec![version_of(e.slug, newest, "2026-11-01T00:00:00Z")]).to_string());
        for number in [Some(e.version), e.latest].into_iter().flatten() {
            net.when_get(&format!("https://cdn.test/{}", file_of(e.slug, number)), bytes_of(e.slug, number));
        }
    }
    // The search of the page that adds a mod, in the board's order.
    let found: Vec<Value> = hits
        .iter()
        .map(|slug| {
            let e = entry_of(slug);
            json!({ "project_id": e.slug, "slug": e.slug, "title": e.title, "description": e.description, "client_side": "required", "server_side": "required" })
        })
        .collect();
    net.when_get("/search", json!({ "hits": found, "total_hits": found.len() }).to_string());

    let api = Arc::new(Api::new(root.join("repo"), ORIGIN.to_string()));
    mods::install(&api.repo, net.clone());
    Setup { api, net, root }
}

fn rig_over(setup: &Setup) -> Rig {
    Rig::with_backend(setup.api.clone() as Arc<dyn Backend>, Arc::new(FakeApi::default()), 1400, 940, true, true)
}

fn said(rig: &Rig) -> String {
    rig.texts().iter().map(|t| t.1.as_str()).collect::<Vec<_>>().join("|")
}

/// The words are drawn: whole, or letter by letter as a heading's letter spacing draws them.
fn has(rig: &Rig, words: &str) -> bool {
    rig.says(words) || rig.texts().iter().map(|t| t.1.as_str()).collect::<String>().contains(words)
}

fn at(rig: &Rig, words: &str) -> Vec2 {
    rig.texts().into_iter().find(|t| t.1 == words).unwrap_or_else(|| panic!("{words:?} is not drawn: {}", said(rig))).0
}

fn on_line_of(rig: &Rig, line: &str, words: &str) -> Vec2 {
    let y = at(rig, line).y;
    rig.texts().into_iter().find(|t| t.1 == words && (t.0.y - y).abs() < 20.0).unwrap_or_else(|| panic!("{words:?} is not on the line of {line:?}")).0
}

fn press(rig: &mut Rig, at: Vec2) {
    rig.click_at(Vec2::new(at.x + 6.0, at.y + 6.0));
}

/// B1 and B3 over the real api: the profile of the board with its fourteen mods, three to update and one by hand; a
/// removal says itself with its Undo and the file comes back with it; an update replaces the file.
#[test]
fn the_profile_page_over_the_real_api() {
    let slugs: Vec<&str> = CATALOG.iter().take(13).map(|e| e.slug).collect();
    let world = setup("profile", &slugs, true, &[]);
    let mut rig = rig_over(&world);
    open_profile(&mut rig, "exploration");
    rig.frame();
    rig.hover_at(Vec2::new(5.0, 930.0));
    rig.write("real_profile_b1_1400.png");
    for words in [
        "Exploration", "GAME", "Game version", "26.3", "Fabric", "0.18.2", "\u{2192} 0.18.4", "Memory", "min 2 GB", "max 6 GB", "used by the PC", "Java arguments",
        "MODS \u{b7} 14", "Update all (3)", "Add a mod", "Sodium", "0.7.1", "\u{2192} 0.7.3", "Distant Horizons", "my-local-mod.jar",
    ] {
        assert!(has(&rig, words), "the page says {words}: {}", said(&rig));
    }
    // The three to update first (by name), the file by hand last.
    let lines = rig.home().pages.profile.ids.lines.clone();
    let order: Vec<&str> = lines.iter().map(|l| l.mod_id.as_str()).collect();
    assert_eq!(&order[..3], ["distanthorizons-2.4.0.jar", "lithium-0.16.0.jar", "sodium-0.7.1.jar"]);
    assert_eq!(order.last(), Some(&"my-local-mod.jar"));

    // Remove Sodium: said with its Undo, kept aside; Undo puts the file back.
    let sodium = lines.iter().find(|l| l.mod_id == "sodium-0.7.1.jar").unwrap();
    rig.click(sodium.remove);
    rig.hover_at(Vec2::new(5.0, 930.0));
    assert!(rig.says("Sodium removed.") && rig.says("Undo") && has(&rig, "MODS \u{b7} 13") && rig.says("Update all (2)"), "{}", said(&rig));
    assert!(!world.files().contains(&"sodium-0.7.1.jar".to_string()));
    rig.write("real_profile_b3_1400.png");
    rig.click(rig.home().pages.profile.ids.undo);
    assert!(world.files().contains(&"sodium-0.7.1.jar".to_string()) && !rig.says("Sodium removed.") && has(&rig, "MODS \u{b7} 14"), "{}", said(&rig));

    // Update Lithium: its file is replaced by the newer one, and the page says what is left to update.
    let lithium = rig.home().pages.profile.ids.lines.iter().find(|l| l.mod_id == "lithium-0.16.0.jar").unwrap().clone();
    rig.click(lithium.action);
    assert!(world.files().contains(&"lithium-0.16.2.jar".to_string()) && !world.files().contains(&"lithium-0.16.0.jar".to_string()), "{:?}", world.files());
    assert!(rig.says("Update all (2)") || rig.says("Update"), "{}", said(&rig));
    // Update all brings the others up.
    rig.click(rig.home().pages.profile.ids.update_all);
    assert!(world.files().contains(&"sodium-0.7.3.jar".to_string()) && world.files().contains(&"distanthorizons-2.4.1.jar".to_string()), "{:?}", world.files());
    assert!(!rig.says("Update all (2)") && !has(&rig, "Update all"), "{}", said(&rig));
    // Fabric moves to the newest loader, in the instance's own file.
    rig.click(rig.home().pages.profile.ids.fabric_update);
    let pack = fs::read_to_string(world.root.join("prism").join("instances").join("exploration").join("mmc-pack.json")).unwrap();
    assert!(pack.contains("0.18.4") && !pack.contains("0.18.2"), "{pack}");
    assert!(rig.says("0.18.4") && !has(&rig, "\u{2192} 0.18.4"), "{}", said(&rig));
    // A mod's name opens its page on Modrinth in the browser; a file by hand has no page.
    let tectonic = rig.home().pages.profile.ids.lines.iter().find(|l| l.mod_id == "tectonic-3.1.0.jar").unwrap().clone();
    rig.click(tectonic.name);
    assert_eq!(*world.net.opened.lock().unwrap(), ["https://modrinth.com/mod/tectonic"]);
}

/// B4 over the real api: the board's profile (Sodium and Lithium to update, Iris added) with the board's seven results;
/// Add on Mod Menu brings Fabric API with it and says so; Update replaces the file; the file action adds what is chosen.
#[test]
fn the_add_a_mod_page_over_the_real_api() {
    let world = setup("add", &["sodium", "lithium", "iris"], false, &["sodium", "iris", "modmenu", "journeymap", "lithium", "chatheads", "continuity"]);
    let mut rig = rig_over(&world);
    open_profile(&mut rig, "exploration");
    rig.frame();
    rig.click(rig.home().pages.profile.ids.add_mod);
    rig.frame();
    for words in ["Search Modrinth", "Add a file", "Sodium", "0.7.1", "\u{2192} 0.7.3", "Update", "Iris Shaders", "Added", "Mod Menu", "15.0.0", "JourneyMap", "6.0.0", "Chat Heads", "0.13.9", "Continuity", "3.0.1", "\u{2192} 0.16.2"] {
        assert!(rig.says(words), "the page says {words:?}: {}", said(&rig));
    }
    assert!(has(&rig, "Add a mod to Exploration"));
    rig.hover_at(Vec2::new(5.0, 930.0));
    rig.write("real_add_mod_b4_before_1400.png");

    // Add on Mod Menu: both files are in the folder, the line is Added, and Fabric API is named under it.
    let add = on_line_of(&rig, "Mod Menu", "Add");
    press(&mut rig, add);
    rig.frame();
    assert!(world.files().contains(&"modmenu-15.0.0.jar".to_string()) && world.files().contains(&"fabric-api-0.140.0.jar".to_string()), "{:?}", world.files());
    assert!(rig.says("Fabric API came with it: Mod Menu needs it."), "{}", said(&rig));
    assert_eq!(rig.texts().iter().filter(|t| t.1 == "Added").count(), 2, "Iris and Mod Menu are added: {}", said(&rig));
    rig.hover_at(Vec2::new(5.0, 930.0));
    rig.write("real_add_mod_b4_1400.png");

    // Update on Lithium replaces its file.
    let update = on_line_of(&rig, "Lithium", "Update");
    press(&mut rig, update);
    rig.frame();
    assert!(world.files().contains(&"lithium-0.16.2.jar".to_string()) && !world.files().contains(&"lithium-0.16.0.jar".to_string()), "{:?}", world.files());

    // The file action asks the person for a jar and copies it as it is.
    let from = world.root.join("downloads");
    fs::create_dir_all(&from).unwrap();
    fs::write(from.join("shader-helper.jar"), b"by hand").unwrap();
    *world.net.file.lock().unwrap() = Some(from.join("shader-helper.jar"));
    let file = at(&rig, "Add a file");
    press(&mut rig, file);
    assert!(world.files().contains(&"shader-helper.jar".to_string()), "{:?}", world.files());

    // The icon at the end of Sodium's line opens its page.
    let end = Vec2::new(320.0 + 760.0 - 16.0, at(&rig, "Sodium").y + 8.0);
    rig.click_at(end);
    assert_eq!(*world.net.opened.lock().unwrap(), ["https://modrinth.com/mod/sodium"]);
}

/// A search typed in the field is asked of Modrinth for the profile's game and Fabric, and its answer is the list.
#[test]
fn typing_searches_modrinth_for_the_profiles_game() {
    let world = setup("typing", &["sodium"], false, &["sodium", "journeymap"]);
    let mut rig = rig_over(&world);
    open_profile(&mut rig, "exploration");
    rig.frame();
    rig.click(rig.home().pages.profile.ids.add_mod);
    rig.frame();
    for ch in "map".chars() {
        rig.app.event(mere_base::Event::Char { ch });
        rig.frame();
    }
    let search = world.net.requests().into_iter().rev().find(|r| r.contains("/search")).unwrap();
    assert!(search.contains("query=map") && search.contains("versions%3A26.3") && search.contains("categories%3Afabric"), "{search}");
}
