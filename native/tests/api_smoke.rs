//! The one place the real api is built: a ping and a miss, through the same `Backend` Home uses. Its repository
//! is a folder under target, so nothing of the pack is read and nothing is started.

use launcher::api::Api;
use launcher_native::ORIGIN;
use launcher_native::bridge::Backend;

#[test]
fn the_real_api_answers_a_ping_and_a_miss_in_process() {
    let repo = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("api_repo");
    std::fs::create_dir_all(&repo).unwrap();
    let api = Api::new(repo, ORIGIN.to_string());
    assert_eq!(api.ask("GET", "/api/ping", b"").status, 200);
    assert_eq!(api.ask("GET", "/api/nothing", b"").status, 404);
}
