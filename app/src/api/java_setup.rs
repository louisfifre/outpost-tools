// The routes the first start's Java step calls (native/ board E1 and E2): what Java the PC has, what the download
// would weigh, the download itself, and its stop. The work is java.rs's; this only keeps one download at a time
// and says how far it is. Nothing here runs unless a POST from the page's own origin asks for it.
//
//   GET /api/java        -> {state: found | missing | downloading | failed, ...}   may ask Adoptium for the size
//   GET /api/java/found  -> {found, version?}   the PC's files only, never the network: what the app reads at its start
//                           to open the set-up before Home when no Java 25 is there

use std::{
    fs,
    panic::{self, AssertUnwindSafe},
    path::Path,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

use serde_json::json;

use super::java::{self, JavaPackage, Locations, Progress, Stage};
use super::{Api, Reply};

// Where the archive waits while it downloads, below the Java folder: removed whatever happened.
const ARCHIVE_DIR: &str = ".download";

enum Job {
    Idle,
    Running { stage: Stage, done: u64, total: u64, stop: Arc<AtomicBool> },
    // Why the last download failed, said once.
    Failed(String),
}

// One download for the whole program: the page asks it again by its state, so nothing else must know of it.
static JOB: Mutex<Job> = Mutex::new(Job::Idle);
// What Adoptium offered, kept once it answered so the size is asked once.
static PLAN: Mutex<Option<JavaPackage>> = Mutex::new(None);

fn lock<T>(m: &'static Mutex<T>) -> MutexGuard<'static, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// What a stop unwinds with, so that java.rs's download loop ends without a way to be told to.
struct Stopped;

fn stage_name(stage: Stage) -> &'static str {
    match stage {
        Stage::Downloading => "downloading",
        Stage::Verifying => "verifying",
        Stage::Extracting => "extracting",
    }
}

impl Api {
    // The root Outpost Tools keeps its own downloads under (`<root>/java/`).
    fn java_root(&self) -> std::path::PathBuf {
        self.repo.join("run").join("app")
    }

    // Whether a Java 25 is on the PC, from the files alone (no network, no process): the app opens its set-up first
    // when there is none.
    pub(super) fn java_found(&self) -> Reply {
        match java_version_in(&Locations::from_env(&self.java_root())) {
            Some(version) => Reply::json(200, json!({ "found": true, "version": version })),
            None => Reply::json(200, json!({ "found": false })),
        }
    }

    // `found`, `downloading`, `failed` or `missing`, with what the page says of each. A GET: the size of the
    // download is read from Adoptium's metadata (a few kilobytes, never the archive).
    pub(super) fn java_state(&self) -> Reply {
        match &*lock(&JOB) {
            Job::Running { stage, done, total, .. } => {
                return Reply::json(200, json!({ "state": "downloading", "stage": stage_name(*stage), "done": done, "total": total }));
            }
            Job::Failed(why) => return Reply::json(200, json!({ "state": "failed", "error": why })),
            Job::Idle => {}
        }
        let root = self.java_root();
        if let Some(java) = java::find_java(&Locations::from_env(&root), java::MIN_MAJOR) {
            return Reply::json(200, json!({ "state": "found", "version": java.version }));
        }
        match self.java_plan() {
            Ok(package) => Reply::json(200, json!({ "state": "missing", "size": package.size })),
            Err(why) => Reply::json(200, json!({ "state": "missing", "error": why })),
        }
    }

    fn java_plan(&self) -> Result<JavaPackage, String> {
        if let Some(package) = lock(&PLAN).clone() {
            return Ok(package);
        }
        let package = java::plan_download(&java::Http::new(), java::MIN_MAJOR)?;
        *lock(&PLAN) = Some(package.clone());
        Ok(package)
    }

    pub(super) fn java_download(&self) -> Reply {
        let package = match self.java_plan() {
            Ok(package) => package,
            Err(why) => return Reply::error(502, why),
        };
        let stop = Arc::new(AtomicBool::new(false));
        {
            let mut job = lock(&JOB);
            if matches!(*job, Job::Running { .. }) {
                return Reply::empty(409);
            }
            *job = Job::Running { stage: Stage::Downloading, done: 0, total: package.size, stop: stop.clone() };
        }
        let root = self.java_root();
        std::thread::spawn(move || {
            let result = panic::catch_unwind(AssertUnwindSafe(|| fetch_and_install(&root, &package, &stop)));
            let _ = fs::remove_dir_all(java::java_dir(&root).join(ARCHIVE_DIR));
            *lock(&JOB) = match result {
                Ok(Ok(())) => Job::Idle,
                Ok(Err(why)) => Job::Failed(why),
                Err(payload) if payload.is::<Stopped>() => Job::Idle,
                Err(_) => Job::Failed("The download stopped unexpectedly.".to_string()),
            };
        });
        Reply::empty(200)
    }

    // Stops the download: whatever it had is removed by the thread as it ends, nothing is kept.
    pub(super) fn java_stop(&self) -> Reply {
        if let Job::Running { stop, .. } = &*lock(&JOB) {
            stop.store(true, Ordering::Relaxed);
        }
        Reply::empty(200)
    }
}

// The version of the first Java 25 the places hold.
fn java_version_in(locations: &Locations) -> Option<String> {
    java::find_java(locations, java::MIN_MAJOR).map(|java| java.version)
}

fn fetch_and_install(root: &Path, package: &JavaPackage, stop: &AtomicBool) -> Result<(), String> {
    let archive = java::java_dir(root).join(ARCHIVE_DIR).join(&package.file_name);
    let mut on = |p: Progress| {
        if stop.load(Ordering::Relaxed) {
            panic::resume_unwind(Box::new(Stopped));
        }
        if let Job::Running { stage, done, total, .. } = &mut *lock(&JOB) {
            (*stage, *done, *total) = (p.stage, p.done, p.total);
        }
    };
    java::download(package, &archive, &mut on)?;
    java::install_zip(root, &archive, &package.sha256, &mut on).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::java::JavaSource;

    fn home(dir: &Path, version: &str) {
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::write(dir.join("bin").join("java.exe"), b"not a program").unwrap();
        fs::write(dir.join("release"), format!("JAVA_VERSION=\"{version}\"
IMAGE_TYPE=\"JRE\"
")).unwrap();
    }

    #[test]
    fn a_java_25_is_found_from_its_files_and_an_older_one_is_not() {
        let root = std::env::temp_dir().join(format!("outpost-java-found-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let at = |sub: &str| Locations { scanned: vec![(JavaSource::Prism, root.join(sub))], ..Locations::default() };
        assert_eq!(java_version_in(&at("none")), None, "nothing is there");
        home(&root.join("old").join("jdk-21"), "21.0.4");
        assert_eq!(java_version_in(&at("old")), None, "Java 21 is not enough");
        home(&root.join("new").join("jdk-25"), "25.0.5");
        assert_eq!(java_version_in(&at("new")).as_deref(), Some("25.0.5"));
        let _ = fs::remove_dir_all(&root);
    }
}
