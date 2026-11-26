//! The one place the real api is built: a ping and a miss, and the routes of the screens that answer from files, through
//! the same `Backend` Home uses. Its repository is a folder under target, so nothing of the pack is read and nothing is
//! started: the servers are folders written here, the EULA is accepted in one of them.

use launcher::api::Api;
use launcher::api::servers::{META_FILE, eula_accepted};
use outpost_tools::ORIGIN;
use outpost_tools::bridge::Backend;

#[test]
fn the_real_api_answers_a_ping_and_a_miss_in_process() {
    let repo = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("api_repo");
    std::fs::create_dir_all(&repo).unwrap();
    let api = Api::new(repo, ORIGIN.to_string());
    assert_eq!(api.ask("GET", "/api/ping", b"").status, 200);
    assert_eq!(api.ask("GET", "/api/nothing", b"").status, 404);
}

#[test]
fn the_real_api_describes_a_server_of_the_folder_and_accepts_its_eula_on_a_temp_folder() {
    let repo = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("api_repo_servers");
    let _ = std::fs::remove_dir_all(&repo);
    let server = repo.join("run").join("app").join("servers").join("Test server");
    std::fs::create_dir_all(&server).unwrap();
    std::fs::write(server.join(META_FILE), r#"{"name":"Test server","profile":"exploration","worldProject":"Tectonic islands","seed":48213,"leftOut":3}"#).unwrap();
    let api = Api::new(repo.clone(), ORIGIN.to_string());
    let described: serde_json::Value = serde_json::from_slice(&api.ask("GET", "/api/server?id=Test%20server", b"").body).unwrap();
    assert_eq!(described["madeFrom"], serde_json::json!({"profile": "exploration", "world": "Tectonic islands", "seed": 48213, "leftOut": 3}));
    assert_eq!(described["eula"], false);
    assert!(!server.join("eula.txt").exists(), "describing a server writes nothing");
    assert_eq!(api.ask("POST", "/api/server/eula", br#"{"id":"Test server"}"#).status, 200);
    assert!(eula_accepted(&server));
    let _ = std::fs::remove_dir_all(&repo);
}
