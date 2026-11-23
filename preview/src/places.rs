//! Where Apply writes: every place found on this machine that holds a Tectonic config (`config/tectonic.json`), and
//! every game folder the person chose, each a row of the menu above Apply. A place's file is only ever
//! rewritten, never created: a folder found without one is no place, and a place whose file is missing (a chosen game
//! that has none, a file that went away since) is refused when it is written.
//!
//! The places, in the list's order:
//! - the games: the Prism instances the host lists, then the installations of the official launcher, read from the
//!   game folders its own `launcher_profiles.json` declares, then the folders the person chose ("Choose a folder…"),
//!   each as long as it is a game, one that holds a mods folder;
//! - when the program drives a pack repository, under a separator: its test server, and the pack's source folder (the
//!   write of the former "Apply to pack", a prod copy's snapshot included).
//!
//! Which places are ticked is the person's: it is kept in the window's own data (`places.json` beside `draft.json`),
//! with the folders they chose, and nothing is ticked that they did not tick, except a single game when it is the
//! only place found and nothing was kept yet.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::json::Json;
use crate::settings::same_values;
use crate::text;

/// What kind of place a row is: what its sentences say after an Apply depends on it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Game,
    /// The pack repository's test server.
    Server,
    /// The pack repository's own pack, the source of its next version.
    Source,
}

/// A game folder a launcher knows, by the name the person gave it there.
#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub name: String,
    pub dir: PathBuf,
}

/// What the host knows of the machine: what places are looked for in. A test gives folders of its own.
#[derive(Clone, Debug, Default)]
pub struct Sources {
    /// Prism Launcher's instances, by their name in Prism and their game folder.
    pub prism: Vec<Game>,
    /// The official launcher's folder, the one that holds `launcher_profiles.json` (`%APPDATA%\.minecraft`).
    pub minecraft: Option<PathBuf>,
    /// The pack repository's test server (`$DevServerDir`).
    pub dev_server: Option<PathBuf>,
}

impl Sources {
    /// The real machine's: the official launcher's folder under `%APPDATA%`, nothing else.
    pub fn system() -> Sources {
        Sources { minecraft: std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(".minecraft")), ..Sources::default() }
    }
}

/// One row of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    /// What the ticked set keeps: the folder, lowercased.
    pub key: String,
    pub kind: Kind,
    /// The row's first line.
    pub name: String,
    /// What the place line and the sentences after an Apply call it.
    pub short: String,
    /// The row's second line: how the place is reached, and its folder.
    pub how: String,
    pub path: String,
    /// The folder whose `config/tectonic.json` is written.
    pub dir: PathBuf,
    /// A prod copy's snapshot of the pack, written after the source (the game's sync serves it).
    pub snapshot: Option<PathBuf>,
    pub ticked: bool,
    /// What its file holds now, and its snapshot's after it: None when it cannot be read or lacks a key.
    pub held: Vec<Option<[f64; 43]>>,
}

impl Place {
    pub fn config(&self) -> PathBuf {
        config_of(&self.dir)
    }

    pub fn snapshot_config(&self) -> Option<PathBuf> {
        self.snapshot.as_deref().map(config_of)
    }

    /// Every file of the place already holds these values: Apply has nothing to write there.
    pub fn holds(&self, values: &[f64; 43]) -> bool {
        self.held.iter().all(|h| h.as_ref().is_some_and(|h| same_values(h, values)))
    }

    /// Reads again what its files hold.
    pub fn refresh(&mut self) {
        self.held = std::iter::once(self.config()).chain(self.snapshot_config()).map(|f| read_held(&f)).collect();
    }
}

pub fn config_of(dir: &Path) -> PathBuf {
    dir.join("config").join("tectonic.json")
}

fn read_held(file: &Path) -> Option<[f64; 43]> {
    std::fs::read_to_string(file).ok().and_then(|t| crate::apply::read_values(&t))
}

/// What the ticked set keeps of a folder: the folder, lowercased, with Windows' separators.
pub fn key_of(dir: &Path) -> String {
    dir.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// A JSON file's text without the byte-order mark some Windows tools put first.
fn read_json(file: &Path) -> Option<Json> {
    let text = std::fs::read_to_string(file).ok()?;
    Json::parse(text.trim_start_matches('\u{feff}')).ok()
}

/// The official launcher's installations: each profile of its `launcher_profiles.json`, in the folder it declares
/// (`gameDir`), or the launcher's own folder when it declares none. Two profiles on one folder are one place, the
/// first one's. A profile with no name of its own goes by its folder's.
pub fn official(minecraft: &Path) -> Vec<Game> {
    let Some(data) = read_json(&minecraft.join("launcher_profiles.json")) else { return Vec::new() };
    let Some(Json::Obj(profiles)) = data.get("profiles") else { return Vec::new() };
    let mut games: Vec<Game> = Vec::new();
    for (_, p) in profiles {
        let dir = p.get("gameDir").and_then(Json::text).filter(|d| !d.trim().is_empty()).map_or_else(|| minecraft.to_path_buf(), PathBuf::from);
        if games.iter().any(|g| key_of(&g.dir) == key_of(&dir)) {
            continue;
        }
        let name = p.get("name").and_then(Json::text).map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        let name = name.unwrap_or_else(|| dir.file_name().map_or_else(|| dir.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned()));
        games.push(Game { name, dir });
    }
    games.sort_by_key(|g| g.name.to_lowercase());
    games
}

/// The pack repository's two places, when the program drives one: the pack (with a prod copy's snapshot) and its
/// test server.
#[derive(Clone, Debug, Default)]
pub struct Repo {
    pub pack: Option<PathBuf>,
    pub snapshot: Option<PathBuf>,
}

/// The game folder of a folder the person chose: the folder itself when it holds a mods folder, or the game folder
/// inside it (a Prism instance's `minecraft`) when that one does. None: it is not a game. A Tectonic config is not
/// asked for: Apply says so when it writes.
pub fn game_dir(chosen: &Path) -> Option<PathBuf> {
    [chosen.to_path_buf(), chosen.join("minecraft"), chosen.join(".minecraft")].into_iter().find(|d| d.join("mods").is_dir())
}

/// Every place found, none ticked, each with what its file holds. `chosen` are the folders the person chose, as kept.
pub fn find(sources: &Sources, repo: &Repo, chosen: &[PathBuf]) -> Vec<Place> {
    let mut places: Vec<Place> = Vec::new();
    let mut add = |kind: Kind, name: String, short: String, how: &str, shown: &Path, dir: PathBuf, snapshot: Option<PathBuf>| {
        let key = key_of(&dir);
        if places.iter().any(|p| p.key == key) {
            return;
        }
        let mut p = Place { key, kind, name, short, how: how.to_string(), path: shown.to_string_lossy().replace('/', "\\"), dir, snapshot, ticked: false, held: Vec::new() };
        p.refresh();
        places.push(p);
    };
    let has_config = |dir: &Path| config_of(dir).is_file();
    let prism = sources.prism.iter().filter(|g| has_config(&g.dir)).map(|g| (g, text::PRISM));
    let launcher = sources.minecraft.as_deref().map(official).unwrap_or_default();
    let launcher: Vec<Game> = launcher.into_iter().filter(|g| has_config(&g.dir)).collect();
    // A chosen folder goes by its own name and is shown as it was chosen; its game folder may be inside it.
    let picked: Vec<(Game, &PathBuf)> = chosen
        .iter()
        .filter_map(|c| game_dir(c).map(|dir| (Game { name: c.file_name().map_or_else(|| c.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned()), dir }, c)))
        .collect();
    let games: Vec<(&Game, &str)> = prism.chain(launcher.iter().map(|g| (g, text::OFFICIAL_LAUNCHER))).chain(picked.iter().map(|(g, _)| (g, text::CHOSEN_HOW))).collect();
    for (g, how) in &games {
        // Two games of one name are told apart by their launcher, in the place line as on the board.
        let twin = games.iter().filter(|(o, _)| o.name.to_lowercase() == g.name.to_lowercase()).count() > 1;
        let short = if twin { format!("{} ({how})", g.name) } else { g.name.clone() };
        // A Prism instance is known by its own folder, which holds its game folder (`minecraft`).
        let inner = g.dir.file_name().is_some_and(|n| n.eq_ignore_ascii_case("minecraft") || n.eq_ignore_ascii_case(".minecraft"));
        let chose = picked.iter().find(|(p, _)| std::ptr::eq(p, *g)).map(|(_, c)| c.as_path());
        let shown = match chose {
            Some(c) => c,
            None if *how == text::PRISM && inner => g.dir.parent().unwrap_or(&g.dir),
            None => &g.dir,
        };
        add(Kind::Game, text::your_game(&g.name), short, how, shown, g.dir.clone(), None);
    }
    if let Some(dir) = sources.dev_server.as_ref().filter(|d| d.join("config").is_dir()) {
        add(Kind::Server, text::SERVER_PLACE.to_string(), text::SERVER_SHORT.to_string(), text::SERVER_HOW, dir, dir.clone(), None);
    }
    if let Some(pack) = &repo.pack {
        // The repository is what the person knows, not its pack folder.
        let shown = pack.parent().unwrap_or(pack);
        add(Kind::Source, text::SOURCE_PLACE.to_string(), text::SOURCE_SHORT.to_string(), text::SOURCE_HOW, shown, pack.clone(), repo.snapshot.clone());
    }
    places
}

/// The ticked set kept from the last session, on the places found: a kept place that is gone is dropped. With
/// nothing kept, a game that is the only place found is ticked.
pub fn tick(places: &mut [Place], kept: Option<&BTreeSet<String>>) {
    match kept {
        Some(kept) => places.iter_mut().for_each(|p| p.ticked = kept.contains(&p.key)),
        None => {
            if let [only] = places {
                only.ticked = only.kind == Kind::Game;
            }
        }
    }
}

pub fn ticked_set(places: &[Place]) -> BTreeSet<String> {
    places.iter().filter(|p| p.ticked).map(|p| p.key.clone()).collect()
}

/// The ticked set as `save` wrote it; None when nothing was kept (or the file is not one of ours).
pub fn load(file: &Path) -> Option<BTreeSet<String>> {
    let data = read_json(file)?;
    let list = data.get("ticked")?.array()?;
    Some(list.iter().filter_map(Json::text).collect())
}

/// The folders the person chose, as `save` wrote them, in the order they were chosen.
pub fn load_chosen(file: &Path) -> Vec<PathBuf> {
    let Some(data) = read_json(file) else { return Vec::new() };
    data.get("chosen").and_then(Json::array).map_or_else(Vec::new, |l| l.iter().filter_map(Json::text).map(PathBuf::from).collect())
}

pub fn save(file: &Path, ticked: &BTreeSet<String>, chosen: &[PathBuf]) -> std::io::Result<()> {
    let quote = |k: &str| format!("\"{}\"", k.replace('\\', "\\\\").replace('"', "\\\""));
    let ticked: Vec<String> = ticked.iter().map(|k| quote(k)).collect();
    let chosen: Vec<String> = chosen.iter().map(|c| quote(&c.to_string_lossy())).collect();
    std::fs::write(file, format!("{{\"ticked\":[{}],\"chosen\":[{}]}}", ticked.join(","), chosen.join(",")))
}

/// What the place line says: the one place ticked, how many, or what to do.
pub fn line(places: &[Place]) -> String {
    let ticked: Vec<&Place> = places.iter().filter(|p| p.ticked).collect();
    match ticked.as_slice() {
        [] if places.is_empty() => text::PLACE_CHOOSE_FOLDER.to_string(),
        [] if places.iter().any(|p| p.kind == Kind::Game) => text::PLACE_CHOOSE_GAME.to_string(),
        [] => text::PLACE_CHOOSE.to_string(),
        [one] => text::place_to(&one.short),
        many => text::place_to(&text::places_count(many.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of the test's own under the system's temp folder, emptied.
    fn root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mere-places-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A game folder, with a Tectonic config or without one.
    fn game(dir: &Path, with_config: bool) -> PathBuf {
        std::fs::create_dir_all(dir.join("mods")).unwrap();
        if with_config {
            std::fs::create_dir_all(dir.join("config")).unwrap();
            std::fs::write(config_of(dir), "{}").unwrap();
        }
        dir.to_path_buf()
    }

    fn profiles(minecraft: &Path, body: &str) {
        std::fs::create_dir_all(minecraft).unwrap();
        std::fs::write(minecraft.join("launcher_profiles.json"), body).unwrap();
    }

    fn json_path(p: &Path) -> String {
        p.to_string_lossy().replace('\\', "\\\\")
    }

    #[test]
    fn the_official_launcher_s_profiles_name_their_folders() {
        let r = root("official");
        let mc = r.join(".minecraft");
        let modded = r.join("modded");
        profiles(&mc, &format!(
            "\u{feff}{{\"profiles\":{{\"a\":{{\"name\":\"Exploration\",\"gameDir\":\"{m}\",\"type\":\"custom\"}},\"b\":{{\"name\":\"\",\"type\":\"latest-release\"}},\"c\":{{\"name\":\"Again\",\"gameDir\":\"{m}\\\\\"}}}},\"version\":3}}",
            m = json_path(&modded)
        ));
        let games = official(&mc);
        // By name: one place per folder, the first profile's name; a profile with none goes by its folder's.
        assert_eq!(games, vec![Game { name: ".minecraft".into(), dir: mc.clone() }, Game { name: "Exploration".into(), dir: modded.clone() }]);
        assert_eq!(official(&r.join("nowhere")), vec![], "no launcher, no game");
        profiles(&mc, "not json");
        assert_eq!(official(&mc), vec![]);
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn only_a_folder_with_a_tectonic_config_is_a_place_and_the_repository_s_come_last() {
        let r = root("find");
        let prism_a = game(&r.join("prism/instances/Exploration/minecraft"), true);
        let prism_b = game(&r.join("prism/instances/Vanilla/minecraft"), false);
        let mc = r.join(".minecraft");
        let vanilla = game(&r.join(".minecraft-exploration"), true);
        profiles(&mc, &format!("{{\"profiles\":{{\"x\":{{\"name\":\"Exploration\",\"gameDir\":\"{}\"}}}}}}", json_path(&vanilla)));
        let server = r.join("server");
        std::fs::create_dir_all(server.join("config")).unwrap();
        let repo_dir = r.join("repo");
        let pack = game(&repo_dir.join("pack"), true);
        let sources = Sources {
            prism: vec![Game { name: "Exploration".into(), dir: prism_a.clone() }, Game { name: "Vanilla".into(), dir: prism_b }],
            minecraft: Some(mc),
            dev_server: Some(server.clone()),
        };
        let places = find(&sources, &Repo { pack: Some(pack.clone()), snapshot: None }, &[]);
        let rows: Vec<(Kind, &str, &str, &str)> = places.iter().map(|p| (p.kind, p.name.as_str(), p.short.as_str(), p.how.as_str())).collect();
        assert_eq!(rows, vec![
            (Kind::Game, "Exploration, your game", "Exploration (Prism Launcher)", "Prism Launcher"),
            (Kind::Game, "Exploration, your game", "Exploration (Minecraft Launcher)", "Minecraft Launcher"),
            (Kind::Server, text::SERVER_PLACE, text::SERVER_SHORT, text::SERVER_HOW),
            (Kind::Source, text::SOURCE_PLACE, text::SOURCE_SHORT, text::SOURCE_HOW),
        ]);
        assert_eq!(places[0].dir, prism_a);
        assert!(places[0].path.ends_with(r"instances\Exploration"), "a Prism instance shows its own folder: {}", places[0].path);
        assert_eq!(places[3].path, repo_dir.to_string_lossy(), "the source shows its repository");
        assert!(places.iter().all(|p| !p.ticked), "nothing is ticked by finding");
        assert_eq!(places[0].held, vec![None], "a file that lacks the keys holds nothing known");
        // Without the server's config folder, no server.
        std::fs::remove_dir_all(server.join("config")).unwrap();
        assert!(find(&sources, &Repo::default(), &[]).iter().all(|p| p.kind == Kind::Game));
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn the_ticked_set_is_kept_and_a_place_gone_is_dropped() {
        let r = root("memory");
        let a = game(&r.join("a"), true);
        let b = game(&r.join("b"), true);
        let sources = Sources { prism: vec![Game { name: "A".into(), dir: a.clone() }, Game { name: "B".into(), dir: b.clone() }], ..Sources::default() };
        let mut places = find(&sources, &Repo::default(), &[]);
        let file = r.join("places.json");
        assert_eq!(load(&file), None, "nothing kept yet");
        places[1].ticked = true;
        save(&file, &ticked_set(&places), &[]).unwrap();
        let kept = load(&file).unwrap();
        assert_eq!(kept, BTreeSet::from([key_of(&b)]));
        // The next session: B's folder is gone, A stays unticked.
        std::fs::remove_dir_all(&b).unwrap();
        let mut again = find(&sources, &Repo::default(), &[]);
        tick(&mut again, Some(&kept));
        assert_eq!(again.len(), 1);
        assert!(!again[0].ticked, "only what was ticked is ticked");
        assert_eq!(ticked_set(&again), BTreeSet::new(), "the place that is gone left the set");
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn a_single_game_is_ticked_only_when_nothing_was_kept() {
        let r = root("single");
        let a = game(&r.join("a"), true);
        let sources = Sources { prism: vec![Game { name: "A".into(), dir: a }], ..Sources::default() };
        let mut places = find(&sources, &Repo::default(), &[]);
        tick(&mut places, None);
        assert!(places[0].ticked, "the only place, a game: ticked");
        tick(&mut places, Some(&BTreeSet::new()));
        assert!(!places[0].ticked, "unticked by the person: it stays so");
        // The pack's source alone is never ticked by itself.
        let pack = game(&r.join("repo/pack"), true);
        let mut only_source = find(&Sources::default(), &Repo { pack: Some(pack), snapshot: None }, &[]);
        tick(&mut only_source, None);
        assert!(!only_source[0].ticked);
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn a_chosen_folder_is_a_game_when_it_holds_a_mods_folder() {
        let r = root("chosen");
        let plain = r.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(game_dir(&plain), None, "no mods folder: not a game");
        let own = game(&r.join("Survival"), false);
        assert_eq!(game_dir(&own), Some(own.clone()));
        // A Prism instance chosen by its own folder: the game folder inside it.
        let inner = game(&r.join(r"instances\Atelier\minecraft"), true);
        let instance = r.join(r"instances\Atelier");
        assert_eq!(game_dir(&instance), Some(inner.clone()));
        let found = find(&Sources::default(), &Repo::default(), &[own.clone(), plain, instance.clone()]);
        let rows: Vec<(Kind, &str, &str, &str, &str)> = found.iter().map(|p| (p.kind, p.name.as_str(), p.short.as_str(), p.how.as_str(), p.path.as_str())).collect();
        let (own_s, inst_s) = (own.to_string_lossy().into_owned(), instance.to_string_lossy().into_owned());
        assert_eq!(rows, vec![
            (Kind::Game, "Survival, your game", "Survival", text::CHOSEN_HOW, own_s.as_str()),
            (Kind::Game, "Atelier, your game", "Atelier", text::CHOSEN_HOW, inst_s.as_str()),
        ], "a folder that is no game is left out; each is shown as it was chosen");
        assert_eq!(found[1].dir, inner, "the instance's game folder is the one written");
        assert_eq!(found[0].held, vec![None], "a game without a Tectonic config holds nothing yet");
        // Kept with the ticks.
        let file = r.join("places.json");
        save(&file, &BTreeSet::from([found[0].key.clone()]), &[own.clone(), instance.clone()]).unwrap();
        assert_eq!(load_chosen(&file), vec![own, instance]);
        assert_eq!(load(&file), Some(BTreeSet::from([found[0].key.clone()])));
        assert_eq!(load_chosen(&r.join("nowhere.json")), Vec::<PathBuf>::new());
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn the_place_line_says_what_is_ticked() {
        let r = root("line");
        let a = game(&r.join("a"), true);
        let b = game(&r.join("b"), true);
        let sources = Sources { prism: vec![Game { name: "Exploration".into(), dir: a }, Game { name: "Atelier".into(), dir: b }], ..Sources::default() };
        let mut places = find(&sources, &Repo::default(), &[]);
        assert_eq!(line(&[]), text::PLACE_CHOOSE_FOLDER);
        assert_eq!(line(&places), text::PLACE_CHOOSE_GAME);
        places[0].ticked = true;
        assert_eq!(line(&places), "To: Exploration");
        places[1].ticked = true;
        assert_eq!(line(&places), "To: 2 places");
        let _ = std::fs::remove_dir_all(r);
    }
}
