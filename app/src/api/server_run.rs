// Starting and stopping the servers of the servers folder (board C1's Start, Home's power buttons). The dev server
// has its own path (dev_server.rs, in a console of its own); these run as Java processes with no window, started
// from servers.rs and held here so that a stop reaches the same process: its console input takes "stop", the server
// saves, and a process that has not ended after the timeout is killed. What starts a process, and which Java, is
// behind `Runner`, so the tests start nothing.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, SystemTime},
};

use serde_json::json;

use super::java::{self, Locations};
use super::servers::{self, JavaSpawner, Server, ServerProcess, Spawner, StartError, StopOutcome};
use super::{Api, Failure, Live, Reply, screens};
use crate::system;

/// How long a server has to save and end after "stop" before it is killed.
const STOP_TIMEOUT: Duration = Duration::from_secs(60);
const STOP_POLL: Duration = Duration::from_millis(250);

/// What starts a server and where its Java is: the real ones, or what a test puts in place of them.
pub struct Runner {
    pub spawner: Arc<dyn Spawner>,
    /// The Java a server runs with, looked for under Outpost Tools' data folder; None when the PC has none that fits.
    pub java: Arc<dyn Fn(&Path) -> Option<PathBuf> + Send + Sync>,
    pub stop_timeout: Duration,
    pub poll: Duration,
}

impl Runner {
    /// Java through `JavaSpawner`, the Java the first start's step finds (java_setup.rs looks in the same places).
    pub fn real() -> Runner {
        Runner {
            spawner: Arc::new(JavaSpawner),
            java: Arc::new(|root| java::find_java(&Locations::from_env(root), java::MIN_MAJOR).map(|j| j.exe)),
            stop_timeout: STOP_TIMEOUT,
            poll: STOP_POLL,
        }
    }
}

/// A server this program started and still holds: stopping it goes through the same process. The process sits behind
/// its own lock so that a stop, which waits, does not hold up the requests that only read where the servers stand.
pub(super) struct Run {
    dir: PathBuf,
    since: SystemTime,
    process: Arc<Mutex<Box<dyn ServerProcess>>>,
}

/// The servers this program holds and that have not ended, with the time each was started. One that has ended
/// (stopped from its console, crashed) is let go. A process being stopped is locked by the stop that waits on it, and
/// stays listed until that stop is over.
pub(super) fn known_runs(live: &mut Live) -> Vec<(PathBuf, SystemTime)> {
    live.runs.retain(|run| match run.process.try_lock() {
        Ok(mut process) => process.has_exited().map(|ended| !ended).unwrap_or(true),
        Err(_) => true,
    });
    live.runs.iter().map(|run| (run.dir.clone(), run.since)).collect()
}

fn refused(error: &StartError) -> Reply {
    let sentence = match error {
        StartError::EulaNotAccepted => "The Minecraft EULA is not accepted yet: accept it on the server's page.".to_string(),
        StartError::JavaMissing(_) => "Java is missing, so the server cannot start.".to_string(),
        other => other.to_string(),
    };
    let status = match error {
        StartError::EulaNotAccepted | StartError::JavaMissing(_) => 412,
        StartError::LauncherMissing => 422,
        StartError::PortTaken(_) => 409,
        StartError::Spawn(_) => 500,
    };
    Reply::error(status, sentence)
}

impl Api {
    /// Starts a server of the servers folder: its Java, its folder as the working folder and the memory set for it
    /// (`launch.json`, as for the dev server, never more than the PC has). Refused while its EULA is not accepted.
    pub(super) fn folder_start(&self, server: &Server) -> Result<Reply, Failure> {
        let mut live = self.live();
        let known = known_runs(&mut live);
        let processes = system::processes(&mut live.system);
        if screens::state_of(&known, &processes, &server.dir) != "stopped" {
            return Ok(Reply::error(409, "The server is already starting or running."));
        }
        let Some(java) = (self.runner.java)(&self.repo.join("run").join("app")) else {
            return Ok(refused(&StartError::JavaMissing(PathBuf::new())));
        };
        let (memory, java_args) = screens::read_launch(&server.dir);
        let memory = u32::try_from(memory.clamp(1024, self.machine_mb.max(1024) as i64)).unwrap_or(1024);
        let args: Vec<String> = java_args.split_whitespace().map(String::from).collect();
        let since = SystemTime::now();
        match servers::start_server(&*self.runner.spawner, &server.dir, &java, memory, &args) {
            Ok(process) => {
                live.runs.push(Run { dir: server.dir.clone(), since, process: Arc::new(Mutex::new(process)) });
                Ok(Reply::empty(200))
            }
            Err(error) => {
                self.write_log(&format!("{} did not start: {error}", server.dir.display()));
                Ok(refused(&error))
            }
        }
    }

    /// Stops a server: "stop" on its console, then the wait for it to save and end, then the kill when the timeout
    /// passes. The answer comes when it is over.
    pub(super) fn folder_stop(&self, server: &Server) -> Reply {
        let process = {
            let mut live = self.live();
            known_runs(&mut live);
            match live.runs.iter().find(|run| run.dir == server.dir) {
                Some(run) => run.process.clone(),
                None => {
                    let processes = system::processes(&mut live.system);
                    return match screens::state_in(&processes, &server.dir) {
                        "stopped" => Reply::error(409, "The server is not running."),
                        // Its console input is held by whatever started it: only the process could be killed, and the
                        // worlds would not be saved.
                        _ => Reply::error(409, "This server was started before Outpost Tools opened: stop it where it was started."),
                    };
                }
            }
        };
        let outcome = {
            let mut process = process.lock().unwrap_or_else(PoisonError::into_inner);
            servers::stop_server(&mut **process, self.runner.stop_timeout, self.runner.poll)
        };
        self.live().runs.retain(|run| run.dir != server.dir);
        match outcome {
            Ok(StopOutcome::Killed) => {
                self.write_log(&format!("{} did not stop in time and was killed.", server.dir.display()));
                Reply::json(200, json!({ "outcome": "killed" }))
            }
            Ok(StopOutcome::Saved) => Reply::json(200, json!({ "outcome": "saved" })),
            Ok(StopOutcome::AlreadyExited) => Reply::json(200, json!({ "outcome": "exited" })),
            Err(e) => {
                self.write_log(&format!("{} could not be stopped: {e}", server.dir.display()));
                Reply::error(502, format!("The server did not take the stop command: {e}"))
            }
        }
    }
}
