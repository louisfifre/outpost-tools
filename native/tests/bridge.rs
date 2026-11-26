//! The threaded bridge, as the window uses it: a request that panics still answers, and Home does not ask a poll
//! again while the last one has not answered. The backends here are stand-ins; nothing is started.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use outpost_tools::LauncherHome;
use outpost_tools::bridge::{Answer, Backend, Bridge, Done, Tag};
use outpost_tools::home::Calendar;
use preview::Home;

struct Panics;

impl Backend for Panics {
    fn ask(&self, _: &str, _: &str, _: &[u8]) -> Answer {
        panic!("a request that panics");
    }
}

/// Answers once the test opens the gate, and counts what it was asked.
#[derive(Default)]
struct Slow {
    open: Mutex<bool>,
    opened: Condvar,
    server: AtomicUsize,
}

impl Slow {
    fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.opened.notify_all();
    }
}

impl Backend for Slow {
    fn ask(&self, _: &str, path: &str, _: &[u8]) -> Answer {
        if path == "/api/server" {
            self.server.fetch_add(1, Ordering::SeqCst);
        }
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.opened.wait(open).unwrap();
        }
        Answer { status: 404, body: b"{}".to_vec() }
    }
}

/// The answers of `bridge` until there are `count`, or none after 5 s.
fn answers(bridge: &mut Bridge, count: usize) -> Vec<Done> {
    let until = Instant::now() + Duration::from_secs(5);
    let mut got = Vec::new();
    while got.len() < count && Instant::now() < until {
        got.extend(bridge.take());
        std::thread::sleep(Duration::from_millis(5));
    }
    got
}

/// Waits up to 5 s for `count` to reach `n`.
fn wait_for(count: &AtomicUsize, n: usize) {
    let until = Instant::now() + Duration::from_secs(5);
    while count.load(Ordering::SeqCst) < n && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_request_that_panics_answers_500_and_the_bridge_is_free_again() {
    let mut bridge = Bridge::threads(Arc::new(Panics));
    bridge.send(Tag::Server, "GET", "/api/server", None);
    assert!(bridge.busy());
    let got = answers(&mut bridge, 1);
    assert_eq!(got.len(), 1, "the panic gave no answer");
    assert_eq!((got[0].tag.clone(), got[0].status), (Tag::Server, 500));
    assert!(!bridge.busy());
}

#[test]
fn a_poll_still_asked_is_not_asked_again() {
    let slow = Arc::new(Slow::default());
    let mut home = LauncherHome::new(Bridge::threads(slow.clone()), Calendar::fixed());
    home.poll(0.0, true);
    wait_for(&slow.server, 1);
    // The server is polled every 3 s: three more times due while the first answer has not come.
    for now in [3.5, 7.0, 10.5] {
        home.poll(now, true);
    }
    // A poll sent anyway would be counted as soon as its thread runs.
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(slow.server.load(Ordering::SeqCst), 1);
    slow.open();
    // Once answered, the next due poll asks again.
    let until = Instant::now() + Duration::from_secs(5);
    while slow.server.load(Ordering::SeqCst) < 2 && Instant::now() < until {
        home.poll(14.0, true);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(slow.server.load(Ordering::SeqCst), 2);
}
