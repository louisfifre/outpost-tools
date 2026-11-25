// The search of the page that adds a mod. It is asked at each pause in the typing, so it is light: one search call
// to Modrinth for the query, then one call for the versions the rows need (the newest version each hit names, by ids),
// and what Modrinth said is kept a few minutes by (query, game version, loader), so the same words typed again, or a
// refresh after an add, ask nothing. The profile's own jars are looked up once per set of files, not per query.
// A search that a newer one has overtaken stops before its second call and answers `stale`, so an old answer never
// replaces a newer one (the page drops it).

use std::{
    collections::HashMap,
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use super::*;

// How long what Modrinth said is trusted.
pub(super) const KEPT: Duration = Duration::from_secs(5 * 60);
// How many answers are kept: the least recently made goes first.
const ENTRIES: usize = 32;

// What Modrinth said of a query: its hits, and the version number each could bring (by project id).
#[derive(Clone)]
struct Said {
    hits: Vec<modrinth::Mod>,
    versions: HashMap<String, String>,
}

// The cache and the order of the searches. One per `Api`.
#[derive(Default)]
pub struct Searches {
    // (query, game version, loader) -> the answer and when it was made.
    said: Mutex<Vec<((String, String, String), Instant, Said)>>,
    // The jars' state, by their hashes and the game version.
    installed: Mutex<Vec<((Vec<String>, String), Instant, Vec<Known>)>>,
    // The number of the latest search asked.
    latest: AtomicU64,
}

fn kept<K: PartialEq, V: Clone>(table: &Mutex<Vec<(K, Instant, V)>>, key: &K, now: Instant) -> Option<V> {
    let mut all = table.lock().unwrap_or_else(PoisonError::into_inner);
    all.retain(|(_, at, _)| now.saturating_duration_since(*at) < KEPT);
    all.iter().find(|(k, _, _)| k == key).map(|(_, _, v)| v.clone())
}

fn keep<K: PartialEq, V>(table: &Mutex<Vec<(K, Instant, V)>>, key: K, value: V, now: Instant) {
    let mut all = table.lock().unwrap_or_else(PoisonError::into_inner);
    all.retain(|(k, _, _)| *k != key);
    all.push((key, now, value));
    if all.len() > ENTRIES {
        all.remove(0);
    }
}

impl Searches {
    pub(super) fn ticket(&self) -> u64 {
        self.latest.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn overtaken(&self, ticket: u64) -> bool {
        self.latest.load(Ordering::SeqCst) != ticket
    }
}

// The jars' state, asked of Modrinth only when the set of files (or the day's answer) is new.
fn installed(searches: &Searches, outside: &Arc<dyn Outside>, jars: &[Jar], game: &str, now: Instant) -> Result<Vec<Known>, modrinth::Error> {
    let key = (jars.iter().map(|j| j.sha1.clone()).collect::<Vec<_>>(), game.to_string());
    if let Some(known) = kept(&searches.installed, &key, now) {
        return Ok(known);
    }
    let known = inspect(outside, jars, game)?;
    keep(&searches.installed, key, known.clone(), now);
    Ok(known)
}

pub(super) fn search(searches: &Searches, profile: &Profile, outside: &Arc<dyn Outside>, query: &str) -> Done<Reply> {
    search_at(searches, profile, outside, query, Instant::now())
}

pub(super) fn search_at(searches: &Searches, profile: &Profile, outside: &Arc<dyn Outside>, query: &str, now: Instant) -> Done<Reply> {
    let ticket = searches.ticket();
    let (_, game, _) = profile.pack()?;
    let modrinth = Modrinth::new(outside.clone());
    let key = (query.trim().to_lowercase(), game.clone(), modrinth::LOADER.to_string());
    let said = match kept(&searches.said, &key, now) {
        Some(said) => said,
        None => {
            let page = modrinth.discover(&game, query, 0, FOUND).map_err(from_modrinth)?;
            // A newer search is under way: this answer would be dropped, so it does not ask for more.
            if searches.overtaken(ticket) {
                return Ok(stale());
            }
            // The newest version each hit names, in one request. A hit whose newest version does not run on this
            // game with Fabric is shown with no version.
            let ids: Vec<String> = page.hits.iter().filter_map(|h| h.latest_version.clone()).collect();
            let found = if ids.is_empty() { Vec::new() } else { modrinth.versions(&ids).map_err(from_modrinth)? };
            let versions = found.iter().filter(|v| v.fits(&game)).map(|v| (v.project_id.clone(), v.version_number.clone())).collect();
            let said = Said { hits: page.hits, versions };
            keep(&searches.said, key, said.clone(), now);
            said
        }
    };
    let known = installed(searches, outside, &jars(&profile.mods), &game, now).map_err(from_modrinth)?;
    if searches.overtaken(ticket) {
        return Ok(stale());
    }
    let have = |project: &str| known.iter().find(|k| k.current.as_ref().is_some_and(|c| c.project_id == project));
    let mods: Vec<Value> = said
        .hits
        .iter()
        .map(|hit| {
            let (state, version, newer) = match have(&hit.id) {
                Some(Known { current, state: UpdateState::Newer { latest, .. }, .. }) => {
                    ("update", current.as_ref().map(|c| c.version_number.clone()).unwrap_or_default(), latest.version_number.clone())
                }
                Some(k) => ("added", k.current.as_ref().map(|c| c.version_number.clone()).unwrap_or_default(), String::new()),
                None => ("add", said.versions.get(&hit.id).cloned().unwrap_or_default(), String::new()),
            };
            json!({
                "id": hit.id, "title": hit.title, "description": hit.description, "version": version,
                "url": hit.url(), "state": state, "newVersion": newer,
            })
        })
        .collect();
    Ok(Reply::json(200, json!({ "mods": mods })))
}

fn stale() -> Reply {
    Reply::json(200, json!({ "stale": true }))
}
