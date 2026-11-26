//! The api, asked in process: the page asks it by HTTP-like requests, the native launcher calls it directly with
//! the same method, path and body, and the same origin, so the api's own checks (a POST only from the page's origin)
//! pass exactly as for the page. Home asks through a `Bridge`, which keeps the window from waiting on a slow answer
//! (reading the processes takes a moment) by asking on a thread, as the wry launcher does for each request.

use std::collections::VecDeque;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use serde_json::Value;

pub struct Answer {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Whatever answers the page's requests: the real api, or a fake in a test.
pub trait Backend: Send + Sync {
    fn ask(&self, method: &str, path: &str, body: &[u8]) -> Answer;
}

impl Backend for launcher::api::Api {
    fn ask(&self, method: &str, path: &str, body: &[u8]) -> Answer {
        let reply = self.handle(method, path, Some(crate::ORIGIN), body);
        Answer { status: reply.status, body: reply.body }
    }
}

/// What a request was for: the answer comes back with it.
#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    Profiles,
    Server,
    /// Every server Home lists: the servers folder's and the dev server.
    Servers,
    Launch,
    Play(String),
    ServerStart,
    ServerStop,
    SaveProfile,
    SaveServer,
    Folder,
    /// A profile's game version, Fabric and mods (its page).
    Instance(String),
    /// One of the profile page's actions on a mod, Fabric or all of them: what it was, to say it.
    ModAction(String),
    /// A link opened in the browser.
    OpenUrl,
    ServerFolder,
    /// A request of the page that adds a mod.
    Mods(crate::pages::add_mod::Ask),
    /// Accept the Minecraft EULA for the server (the api writes the line; nothing here does).
    Eula,
    /// Open a link in the person's browser.
    OpenLink,
    Accounts,
    /// The skin of an account, by its name.
    Skin(String),
    AccountUse,
    AccountAdd,
    AccountSignOut,
    /// The first start's Java step: its state, the start of its download, its stop.
    Java,
    /// Whether a Java 25 is on the PC, asked once at the start (files only): none opens the set-up before Home.
    JavaFound,
    JavaStart,
    JavaStop,
}

#[derive(Clone, Debug)]
pub struct Done {
    pub tag: Tag,
    pub status: u16,
    /// Null when the body is not JSON.
    pub body: Value,
}

pub enum Bridge {
    /// One thread per request; the answers come back through a channel.
    Threads { backend: Arc<dyn Backend>, tx: Sender<Done>, rx: Receiver<Done>, pending: usize },
    /// The answer is made on the spot, for tests and pictures.
    Inline { backend: Arc<dyn Backend>, done: VecDeque<Done> },
}

fn run(backend: &dyn Backend, tag: Tag, method: &str, path: &str, body: Option<Value>) -> Done {
    let bytes = body.map(|b| b.to_string().into_bytes()).unwrap_or_default();
    let a = backend.ask(method, path, &bytes);
    Done { tag, status: a.status, body: serde_json::from_slice(&a.body).unwrap_or(Value::Null) }
}

impl Bridge {
    pub fn threads(backend: Arc<dyn Backend>) -> Bridge {
        let (tx, rx) = channel();
        Bridge::Threads { backend, tx, rx, pending: 0 }
    }

    pub fn inline(backend: Arc<dyn Backend>) -> Bridge {
        Bridge::Inline { backend, done: VecDeque::new() }
    }

    pub fn send(&mut self, tag: Tag, method: &str, path: &str, body: Option<Value>) {
        match self {
            Bridge::Threads { backend, tx, pending, .. } => {
                *pending += 1;
                let (backend, tx) = (backend.clone(), tx.clone());
                let (method, path) = (method.to_string(), path.to_string());
                std::thread::spawn(move || {
                    // A request that panics still answers, as a 500: `pending` then comes back down, and Home does
                    // not stay busy for ever nor wait on an answer that will never come.
                    let asked = tag.clone();
                    let done = std::panic::catch_unwind(AssertUnwindSafe(|| run(&*backend, asked, &method, &path, body)))
                        .unwrap_or_else(|_| Done { tag, status: 500, body: Value::Null });
                    let _ = tx.send(done);
                });
            }
            Bridge::Inline { backend, done } => done.push_back(run(&**backend, tag, method, path, body)),
        }
    }

    /// The answers that have come back since the last call, in the order they arrived.
    pub fn take(&mut self) -> Vec<Done> {
        match self {
            Bridge::Threads { rx, pending, .. } => {
                let got: Vec<Done> = rx.try_iter().collect();
                *pending = pending.saturating_sub(got.len());
                got
            }
            Bridge::Inline { done, .. } => done.drain(..).collect(),
        }
    }

    /// True while a request has not answered (or an answer waits to be taken).
    pub fn busy(&self) -> bool {
        match self {
            Bridge::Threads { pending, .. } => *pending > 0,
            Bridge::Inline { done, .. } => !done.is_empty(),
        }
    }
}
